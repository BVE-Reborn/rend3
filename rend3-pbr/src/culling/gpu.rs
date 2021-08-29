use std::{mem, num::NonZeroU64};

use glam::Mat4;
use rend3::{
    resources::{CameraManager, InternalObject, MaterialManager},
    util::{bind_merge::BindGroupBuilder, frustum::ShaderFrustum},
    ModeData,
};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType,
    BufferBindingType, BufferDescriptor, BufferUsages, CommandEncoder, ComputePassDescriptor, ComputePipeline,
    ComputePipelineDescriptor, Device, PipelineLayoutDescriptor, RenderPass, ShaderModuleDescriptorSpirV, ShaderStages,
};

use crate::{
    common::interfaces::{PerObjectData, ShaderInterfaces},
    culling::{CulledObjectSet, GPUCullingInput, GPUIndirectData},
    shaders::SPIRV_SHADERS,
};

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
struct GPUCullingUniforms {
    view: Mat4,
    view_proj: Mat4,
    frustum: ShaderFrustum,
    object_count: u32,
}

unsafe impl bytemuck::Pod for GPUCullingUniforms {}
unsafe impl bytemuck::Zeroable for GPUCullingUniforms {}

pub struct GpuCullerCullArgs<'a> {
    pub device: &'a Device,
    pub encoder: &'a mut CommandEncoder,

    pub interfaces: &'a ShaderInterfaces,

    pub materials: &'a MaterialManager,
    pub camera: &'a CameraManager,

    pub objects: &'a [InternalObject],
}

pub struct GpuCuller {
    bgl: BindGroupLayout,
    pipeline: ComputePipeline,
}
impl GpuCuller {
    pub fn new(device: &Device) -> Self {
        let bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("gpu culling pll"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(mem::size_of::<GPUCullingUniforms>() as _),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(mem::size_of::<PerObjectData>() as _),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(16 + 20),
                    },
                    count: None,
                },
            ],
        });

        let pll = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("gpu culling pll"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let sm = unsafe {
            device.create_shader_module_spirv(&ShaderModuleDescriptorSpirV {
                label: Some("cull"),
                source: wgpu::util::make_spirv_raw(SPIRV_SHADERS.get_file("cull.comp.spv").unwrap().contents()),
            })
        };

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("gpu culling pl"),
            layout: Some(&pll),
            module: &sm,
            entry_point: "main",
        });

        Self { bgl, pipeline }
    }

    pub fn cull(&self, args: GpuCullerCullArgs<'_>) -> CulledObjectSet {
        let mut data = Vec::<u8>::with_capacity(
            mem::size_of::<GPUCullingUniforms>() + args.objects.len() * mem::size_of::<GPUCullingInput>(),
        );
        data.extend(bytemuck::bytes_of(&GPUCullingUniforms {
            view: args.camera.view(),
            view_proj: args.camera.view_proj(),
            frustum: ShaderFrustum::from_matrix(args.camera.proj()),
            object_count: args.objects.len() as u32,
        }));
        for object in args.objects {
            data.extend(bytemuck::bytes_of(&GPUCullingInput {
                start_idx: object.start_idx,
                count: object.count,
                vertex_offset: object.vertex_offset,
                material_idx: args.materials.internal_index(object.material.get_raw()) as u32,
                transform: object.transform,
                bounding_sphere: object.sphere,
            }));
        }

        let output_buffer = args.device.create_buffer(&BufferDescriptor {
            label: Some("culling output"),
            size: (args.objects.len().max(1) * mem::size_of::<PerObjectData>()) as _,
            usage: BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let indirect_buffer = args.device.create_buffer(&BufferDescriptor {
            label: Some("indirect buffer"),
            // 16 bytes for count, the rest for the indirect count
            size: (args.objects.len() * 20 + 16) as _,
            usage: BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::VERTEX,
            mapped_at_creation: false,
        });

        if !args.objects.is_empty() {
            let input_buffer = args.device.create_buffer_init(&BufferInitDescriptor {
                label: Some("culling inputs"),
                contents: &data,
                usage: BufferUsages::STORAGE,
            });

            let bg = BindGroupBuilder::new(Some("gpu culling bg"))
                .with_buffer(&input_buffer)
                .with_buffer(&output_buffer)
                .with_buffer(&indirect_buffer)
                .build(args.device, &self.bgl);

            let mut cpass = args.encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("compute cull"),
            });

            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bg, &[]);
            cpass.dispatch(((args.objects.len() + 255) / 256) as _, 1, 1);

            drop(cpass);
        }

        let output_bg = args.device.create_bind_group(&BindGroupDescriptor {
            label: Some("culling input bg"),
            layout: &args.interfaces.culled_object_bgl,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: output_buffer.as_entire_binding(),
            }],
        });

        CulledObjectSet {
            calls: ModeData::GPU(GPUIndirectData {
                indirect_buffer,
                count: args.objects.len(),
            }),
            output_bg,
        }
    }
}

pub fn run<'rpass>(rpass: &mut RenderPass<'rpass>, indirect_data: &'rpass GPUIndirectData) {
    if indirect_data.count != 0 {
        rpass.set_vertex_buffer(6, indirect_data.indirect_buffer.slice(16..));
        rpass.multi_draw_indexed_indirect_count(
            &indirect_data.indirect_buffer,
            16,
            &indirect_data.indirect_buffer,
            0,
            indirect_data.count as _,
        );
    }
}
