use std::{mem, num::NonZeroU64};

use glam::Mat4;
use wgpu::{BindingResource, BindingType, BufferBindingType, BufferDescriptor, BufferUsage, CommandEncoder, ComputePassDescriptor, ComputePipelineDescriptor, Device, PipelineLayoutDescriptor, RenderPass, ShaderFlags, ShaderModuleDescriptor, ShaderStage, util::{BufferInitDescriptor, DeviceExt}};

use super::{CulledObjectSet, GPUCullingInput, GPUIndirectData};
use crate::{
    resources::{CameraManager, InternalObject, MaterialManager},
    routines::{culling::CullingOutput, CacheContext},
    shaders::SPIRV_SHADERS,
    util::bind_merge::BindGroupBuilder,
    ModeData,
};

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
struct GPUCullingUniforms {
    view: Mat4,
    view_proj: Mat4,
    object_count: u32,
}

unsafe impl bytemuck::Pod for GPUCullingUniforms {}
unsafe impl bytemuck::Zeroable for GPUCullingUniforms {}

pub fn cull(
    device: &Device,
    ctx: &mut CacheContext<'_>,
    encoder: &mut CommandEncoder,
    material: &MaterialManager,
    camera: &CameraManager,
    objects: &[InternalObject],
) -> CulledObjectSet {
    let sm = ctx.sm_cache.shader_module(
        device,
        &ShaderModuleDescriptor {
            label: Some("cull"),
            source: wgpu::util::make_spirv(SPIRV_SHADERS.get_file("cull.comp.spv").unwrap().contents()),
            flags: ShaderFlags::empty(),
        },
    );

    let mut data = Vec::<u8>::with_capacity(
        mem::size_of::<GPUCullingUniforms>() + objects.len() * mem::size_of::<GPUCullingInput>(),
    );
    data.extend(bytemuck::bytes_of(&GPUCullingUniforms {
        view: camera.view().into(),
        view_proj: camera.view_proj().into(),
        object_count: objects.len() as u32,
    }));
    for object in objects {
        data.extend(bytemuck::bytes_of(&GPUCullingInput {
            start_idx: object.start_idx,
            count: object.count,
            vertex_offset: object.vertex_offset,
            material_idx: material.internal_index(object.material) as u32,
            transform: object.transform.into(),
            bounding_sphere: object.sphere.into(),
        }));
    }

    let input_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("culling inputs"),
        contents: &data,
        usage: BufferUsage::STORAGE,
    });

    let output_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("culling output"),
        size: (objects.len() * mem::size_of::<CullingOutput>()) as _,
        usage: BufferUsage::STORAGE,
        mapped_at_creation: false,
    });

    let indirect_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("indirect buffer"),
        // 16 bytes for count, the rest for the indirect count
        size: (objects.len() * 20 + 16) as _,
        usage: BufferUsage::STORAGE | BufferUsage::INDIRECT,
        mapped_at_creation: false,
    });

    let mut bgb = BindGroupBuilder::new("culling");
    bgb.append(
        ShaderStage::COMPUTE,
        BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(mem::size_of::<GPUCullingUniforms>() as _),
        },
        None,
        BindingResource::Buffer {
            buffer: &input_buffer,
            offset: 0,
            size: None,
        },
    );
    bgb.append(
        ShaderStage::COMPUTE,
        BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(mem::size_of::<CullingOutput>() as _),
        },
        None,
        BindingResource::Buffer {
            buffer: &output_buffer,
            offset: 0,
            size: None,
        },
    );
    bgb.append(
        ShaderStage::COMPUTE,
        BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(20),
        },
        None,
        BindingResource::Buffer {
            buffer: &indirect_buffer,
            offset: 0,
            size: None,
        },
    );
    let (bgl, bg) = bgb.build_transient(&device, ctx.bind_group_cache);

    let pipeline = ctx.pipeline_cache.compute_pipeline(
        device,
        &PipelineLayoutDescriptor {
            label: Some("cull"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        },
        &ComputePipelineDescriptor {
            label: Some("cull"),
            layout: None,
            entry_point: "main",
            module: &sm,
        },
    );

    let mut cpass = encoder.begin_compute_pass(&ComputePassDescriptor {
        label: Some("compute cull"),
    });

    cpass.set_pipeline(&pipeline);
    cpass.set_bind_group(0, &bg, &[]);
    cpass.dispatch((objects.len() / 256) as _, 1, 1);

    drop(cpass);

    CulledObjectSet {
        calls: ModeData::GPU(GPUIndirectData {
            indirect_buffer,
            count: objects.len(),
        }),
        output_buffer,
    }
}

pub fn run<'rpass>(rpass: &mut RenderPass<'rpass>, indirect_data: &'rpass GPUIndirectData) {
    rpass.multi_draw_indexed_indirect_count(
        &indirect_data.indirect_buffer,
        16,
        &indirect_data.indirect_buffer,
        0,
        indirect_data.count as _,
    );
}
