use std::{mem, num::NonZeroU64};

use glam::Mat4;
use ordered_float::OrderedFloat;
use rend3::{
    resources::{CameraManager, GPUCullingInput, InternalObject, ObjectManager},
    util::{bind_merge::BindGroupBuilder, frustum::ShaderFrustum},
    ModeData,
};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType,
    Buffer, BufferBindingType, BufferDescriptor, BufferUsages, CommandEncoder, ComputePassDescriptor, ComputePipeline,
    ComputePipelineDescriptor, Device, PipelineLayoutDescriptor, PushConstantRange, RenderPass,
    ShaderModuleDescriptorSpirV, ShaderStages,
};

use crate::{
    common::interfaces::{PerObjectData, ShaderInterfaces},
    culling::{CulledObjectSet, GPUIndirectData, Sorting},
    material::{PbrMaterial, TransparencyType},
    shaders::SPIRV_SHADERS,
};

pub struct PreCulledBuffer {
    inner: Buffer,
    count: usize,
}

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

pub struct GpuCullerPreCullArgs<'a> {
    pub device: &'a Device,

    pub camera: &'a CameraManager,

    pub objects: &'a mut ObjectManager,

    pub transparency: TransparencyType,
    pub sort: Option<Sorting>,
}

pub struct GpuCullerCullArgs<'a> {
    pub device: &'a Device,
    pub encoder: &'a mut CommandEncoder,

    pub interfaces: &'a ShaderInterfaces,

    pub camera: &'a CameraManager,

    pub input_buffer: &'a PreCulledBuffer,

    pub sort: Option<Sorting>,
}

pub struct GpuCuller {
    atomic_bgl: BindGroupLayout,
    atomic_pipeline: ComputePipeline,

    prefix_bgl: BindGroupLayout,
    prefix_cull_pipeline: ComputePipeline,
    prefix_sum_pipeline: ComputePipeline,
    prefix_output_pipeline: ComputePipeline,
}
impl GpuCuller {
    pub fn new(device: &Device) -> Self {
        let atomic_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("atomic culling pll"),
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
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(mem::size_of::<GPUCullingUniforms>() as _),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(mem::size_of::<PerObjectData>() as _),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
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

        let prefix_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("prefix culling pll"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(mem::size_of::<GPUCullingInput>() as _),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(mem::size_of::<GPUCullingUniforms>() as _),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(mem::size_of::<u32>() as _),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(mem::size_of::<u32>() as _),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(mem::size_of::<PerObjectData>() as _),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 5,
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

        let atomic_pll = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("atomic culling pll"),
            bind_group_layouts: &[&atomic_bgl],
            push_constant_ranges: &[],
        });

        let prefix_pll = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("prefix culling pll"),
            bind_group_layouts: &[&prefix_bgl],
            push_constant_ranges: &[],
        });

        let prefix_sum_pll = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("prefix sum pll"),
            bind_group_layouts: &[&prefix_bgl],
            push_constant_ranges: &[PushConstantRange {
                stages: ShaderStages::COMPUTE,
                range: 0..4,
            }],
        });

        let atomic_sm = unsafe {
            device.create_shader_module_spirv(&ShaderModuleDescriptorSpirV {
                label: Some("cull-atomic-cull"),
                source: wgpu::util::make_spirv_raw(
                    SPIRV_SHADERS.get_file("cull-atomic-cull.comp.spv").unwrap().contents(),
                ),
            })
        };

        let prefix_cull_sm = unsafe {
            device.create_shader_module_spirv(&ShaderModuleDescriptorSpirV {
                label: Some("cull-prefix-cull"),
                source: wgpu::util::make_spirv_raw(
                    SPIRV_SHADERS.get_file("cull-prefix-cull.comp.spv").unwrap().contents(),
                ),
            })
        };

        let prefix_sum_sm = unsafe {
            device.create_shader_module_spirv(&ShaderModuleDescriptorSpirV {
                label: Some("cull-prefix-sum"),
                source: wgpu::util::make_spirv_raw(
                    SPIRV_SHADERS.get_file("cull-prefix-sum.comp.spv").unwrap().contents(),
                ),
            })
        };

        let prefix_output_sm = unsafe {
            device.create_shader_module_spirv(&ShaderModuleDescriptorSpirV {
                label: Some("cull-prefix-output"),
                source: wgpu::util::make_spirv_raw(
                    SPIRV_SHADERS
                        .get_file("cull-prefix-output.comp.spv")
                        .unwrap()
                        .contents(),
                ),
            })
        };

        let atomic_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("atomic culling pl"),
            layout: Some(&atomic_pll),
            module: &atomic_sm,
            entry_point: "main",
        });

        let prefix_cull_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("prefix cull pl"),
            layout: Some(&prefix_pll),
            module: &prefix_cull_sm,
            entry_point: "main",
        });

        let prefix_sum_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("prefix sum pl"),
            layout: Some(&prefix_sum_pll),
            module: &prefix_sum_sm,
            entry_point: "main",
        });

        let prefix_output_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("prefix output pl"),
            layout: Some(&prefix_pll),
            module: &prefix_output_sm,
            entry_point: "main",
        });

        Self {
            atomic_bgl,
            atomic_pipeline,
            prefix_bgl,
            prefix_cull_pipeline,
            prefix_sum_pipeline,
            prefix_output_pipeline,
        }
    }

    pub fn pre_cull(&self, args: GpuCullerPreCullArgs<'_>) -> PreCulledBuffer {
        let objects = args.objects.get_objects_mut::<PbrMaterial>(args.transparency as u64);
        let count = objects.len();

        if let Some(sorting) = args.sort {
            profiling::scope!("Sorting");

            let camera_location = args.camera.get_data().location;

            match sorting {
                Sorting::FrontToBack => {
                    objects.sort_unstable_by_key(|o| OrderedFloat(o.location.distance_squared(camera_location)));
                }
                Sorting::BackToFront => {
                    objects.sort_unstable_by_key(|o| OrderedFloat(-o.location.distance_squared(camera_location)));
                }
            }
        }
        let buffer = build_cull_data(args.device, objects);

        PreCulledBuffer { inner: buffer, count }
    }

    pub fn cull(&self, args: GpuCullerCullArgs<'_>) -> CulledObjectSet {
        profiling::scope!("Record GPU Culling");

        let count = args.input_buffer.count;

        let uniform = GPUCullingUniforms {
            view: args.camera.view(),
            view_proj: args.camera.view_proj(),
            frustum: ShaderFrustum::from_matrix(args.camera.proj()),
            object_count: count as u32,
        };

        let uniform_buffer = args.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("gpu culling uniform buffer"),
            contents: bytemuck::bytes_of(&uniform),
            usage: BufferUsages::UNIFORM,
        });

        let output_buffer = args.device.create_buffer(&BufferDescriptor {
            label: Some("culling output"),
            size: (count.max(1) * mem::size_of::<PerObjectData>()) as _,
            usage: BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let indirect_buffer = args.device.create_buffer(&BufferDescriptor {
            label: Some("indirect buffer"),
            // 16 bytes for count, the rest for the indirect count
            size: (count * 20 + 16) as _,
            usage: BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::VERTEX,
            mapped_at_creation: false,
        });

        if count != 0 {
            let dispatch_count = ((count + 255) / 256) as u32;

            if args.sort.is_some() {
                let buffer_a = args.device.create_buffer(&BufferDescriptor {
                    label: Some("cull result index buffer A"),
                    size: (count * 4) as _,
                    usage: BufferUsages::STORAGE,
                    mapped_at_creation: false,
                });

                let buffer_b = args.device.create_buffer(&BufferDescriptor {
                    label: Some("cull result index buffer B"),
                    size: (count * 4) as _,
                    usage: BufferUsages::STORAGE,
                    mapped_at_creation: false,
                });

                let bg_a = BindGroupBuilder::new(Some("prefix cull A bg"))
                    .with_buffer(&args.input_buffer.inner)
                    .with_buffer(&uniform_buffer)
                    .with_buffer(&buffer_a)
                    .with_buffer(&buffer_b)
                    .with_buffer(&output_buffer)
                    .with_buffer(&indirect_buffer)
                    .build(args.device, &self.prefix_bgl);

                let bg_b = BindGroupBuilder::new(Some("prefix cull B bg"))
                    .with_buffer(&args.input_buffer.inner)
                    .with_buffer(&uniform_buffer)
                    .with_buffer(&buffer_b)
                    .with_buffer(&buffer_a)
                    .with_buffer(&output_buffer)
                    .with_buffer(&indirect_buffer)
                    .build(args.device, &self.prefix_bgl);

                let mut cpass = args.encoder.begin_compute_pass(&ComputePassDescriptor {
                    label: Some("prefix cull"),
                });

                cpass.set_pipeline(&self.prefix_cull_pipeline);
                cpass.set_bind_group(0, &bg_a, &[]);
                cpass.dispatch(dispatch_count, 1, 1);

                cpass.set_pipeline(&self.prefix_sum_pipeline);
                let mut stride = 1_u32;
                let mut iteration = 0;
                while stride < count as u32 {
                    let bind_group = if iteration % 2 == 0 { &bg_a } else { &bg_b };

                    cpass.set_push_constants(0, bytemuck::cast_slice(&[stride]));
                    cpass.set_bind_group(0, bind_group, &[]);
                    cpass.dispatch(dispatch_count, 1, 1);
                    stride <<= 1;
                    iteration += 1;
                }

                let bind_group = if iteration % 2 == 0 { &bg_a } else { &bg_b };
                cpass.set_pipeline(&self.prefix_output_pipeline);
                cpass.set_bind_group(0, bind_group, &[]);
                cpass.dispatch(dispatch_count, 1, 1);
            } else {
                let bg = BindGroupBuilder::new(Some("atomic culling bg"))
                    .with_buffer(&args.input_buffer.inner)
                    .with_buffer(&uniform_buffer)
                    .with_buffer(&output_buffer)
                    .with_buffer(&indirect_buffer)
                    .build(args.device, &self.atomic_bgl);

                let mut cpass = args.encoder.begin_compute_pass(&ComputePassDescriptor {
                    label: Some("atomic cull"),
                });

                cpass.set_pipeline(&self.atomic_pipeline);
                cpass.set_bind_group(0, &bg, &[]);
                cpass.dispatch(dispatch_count, 1, 1);

                drop(cpass);
            }
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
            calls: ModeData::GPU(GPUIndirectData { indirect_buffer, count }),
            output_bg,
        }
    }
}

fn build_cull_data(device: &Device, objects: &[InternalObject]) -> Buffer {
    profiling::scope!("Building Input Data");

    let total_length = objects.len() * mem::size_of::<GPUCullingInput>();

    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("culling inputs"),
        size: total_length as u64,
        usage: BufferUsages::STORAGE,
        mapped_at_creation: true,
    });

    let mut data = buffer.slice(..).get_mapped_range_mut();

    // This unsafe block measured a bit faster in my tests, and as this is basically _the_ hot path, so this is worthwhile.
    unsafe {
        let ptr = data.as_mut_ptr();

        // Assert everything is aligned
        assert!((ptr as usize).trailing_zeros() >= mem::align_of::<GPUCullingInput>().trailing_zeros());

        // Skip over the uniform data
        let data_ptr = data.as_mut_ptr() as *mut GPUCullingInput;

        // Iterate over the objects
        for idx in 0..objects.len() {
            // We're iterating over 0..len so this is never going to be out of bounds
            let object = objects.get_unchecked(idx);

            // This is aligned, and we know the vector has enough bytes to hold this, so this is safe
            data_ptr.add(idx).write(object.input);
        }
    }

    drop(data);
    buffer.unmap();

    buffer
}

pub fn run<'rpass>(rpass: &mut RenderPass<'rpass>, indirect_data: &'rpass GPUIndirectData) {
    if indirect_data.count != 0 {
        rpass.set_vertex_buffer(7, indirect_data.indirect_buffer.slice(16..));
        rpass.multi_draw_indexed_indirect_count(
            &indirect_data.indirect_buffer,
            16,
            &indirect_data.indirect_buffer,
            0,
            indirect_data.count as _,
        );
    }
}
