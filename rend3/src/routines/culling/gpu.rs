use std::{io::Cursor, num::NonZeroU64};

use crevice::std430::AsStd430;
use wgpu::{BindGroupLayoutEntry, BindingResource, BindingType, BufferBindingType, BufferDescriptor, BufferUsage, CommandEncoder, ComputePassDescriptor, ComputePipelineDescriptor, Device, PipelineLayoutDescriptor, ShaderFlags, ShaderModuleDescriptor, ShaderStage, util::{BufferInitDescriptor, DeviceExt}};

use super::GPUCullingInput;
use crate::{
    cache::{BindGroupCache, PipelineCache, ShaderModuleCache},
    resources::{CameraManager, InternalObject, MaterialManager},
    routines::culling::{CullingOutput, GpuCulledObjectSet},
    shaders::SPIRV_SHADERS,
    util::bind_merge::BindGroupBuilder,
};

#[derive(Debug, Copy, Clone, AsStd430)]
struct GPUCullingUniforms {
    view: mint::ColumnMatrix4<f32>,
    view_proj: mint::ColumnMatrix4<f32>,
    object_count: u32,
}

pub(super) fn run(
    device: &Device,
    encoder: &mut CommandEncoder,
    sm_cache: &mut ShaderModuleCache,
    pipeline_cache: &mut PipelineCache,
    bind_group_cache: &mut BindGroupCache,
    objects: &[InternalObject],
    material: &MaterialManager,
    camera: &CameraManager,
) -> GpuCulledObjectSet {
    let sm = sm_cache.shader_module(
        device,
        &ShaderModuleDescriptor {
            label: Some("cull"),
            source: wgpu::util::make_spirv(SPIRV_SHADERS.get_file("cull.comp.spv").unwrap().contents()),
            flags: ShaderFlags::empty(),
        },
    );

    let mut data = Vec::<u8>::new();
    let mut writer = crevice::std430::Writer::new(Cursor::new(&mut data));
    writer
        .write(&GPUCullingUniforms {
            view: camera.view().into(),
            view_proj: camera.view_proj().into(),
            object_count: objects.len() as u32,
        })
        .unwrap();
    for object in objects {
        writer
            .write(&GPUCullingInput {
                start_idx: object.start_idx,
                count: object.count,
                vertex_offset: object.vertex_offset,
                material_idx: material.internal_index(object.material) as u32,
                transform: object.transform.into(),
                bounding_sphere: object.sphere.into(),
            })
            .unwrap();
    }

    let input_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("culling inputs"),
        contents: &data,
        usage: BufferUsage::STORAGE,
    });

    let output_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("culling output"),
        size: (objects.len() * CullingOutput::std430_size_static()) as _,
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
            min_binding_size: NonZeroU64::new(GPUCullingUniforms::std430_size_static() as _),
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
            min_binding_size: NonZeroU64::new(CullingOutput::std430_size_static() as _),
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
    let (bgl, bg) = bgb.build_transient(&device, bind_group_cache);

    let pipeline = pipeline_cache.compute_pipeline(
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

    let mut cpass = encoder.begin_compute_pass(&ComputePassDescriptor { label: Some("compute") });

    cpass.set_pipeline(&pipeline);
    cpass.set_bind_group(0, &bg, &[]);
    cpass.dispatch((objects.len() / 256) as _, y, z);

    drop(cpass);


    let mut bgb = BindGroupBuilder::new("shader input");
    bgb.append(
        ShaderStage::COMPUTE,
        BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(CullingOutput::std430_size_static() as _),
        },
        None,
        BindingResource::Buffer {
            buffer: &output_buffer,
            offset: 0,
            size: None,
        },
    );
    let (bgl, bg) = bgb.build_transient(&device, bind_group_cache);


    GpuCulledObjectSet {
        indirect_buffer,
        output_buffer,
    }
}
