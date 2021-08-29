use glam::{Mat3, Mat3A, Mat4, Vec4Swizzles};
use rend3::{
    resources::{CameraManager, InternalObject, MaterialManager},
    types::SampleType,
    util::{frustum::ShaderFrustum, math::IndexedDistance},
    ModeData,
};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BindGroupDescriptor, BindGroupEntry, BufferUsages, Device, RenderPass,
};

use crate::{
    common::{
        interfaces::{PerObjectData, ShaderInterfaces},
        samplers::Samplers,
    },
    culling::{CPUDrawCall, CulledObjectSet},
};

pub struct CpuCullerCullArgs<'a> {
    pub device: &'a Device,
    pub camera: &'a CameraManager,

    pub interfaces: &'a ShaderInterfaces,

    pub objects: &'a [InternalObject],
}

pub struct CpuCuller {}

impl CpuCuller {
    pub fn new() -> Self {
        Self {}
    }

    pub fn cull(&self, args: CpuCullerCullArgs<'_>) -> CulledObjectSet {
        let frustum = ShaderFrustum::from_matrix(args.camera.proj());
        let view = args.camera.view();
        let view_proj = args.camera.view_proj();

        let mut outputs = Vec::with_capacity(args.objects.len());
        let mut calls = Vec::with_capacity(args.objects.len());
        let mut _distances = Vec::with_capacity(args.objects.len());

        for (index, object) in args.objects.iter().enumerate() {
            let model = object.transform;
            let model_view = view * model;

            let transformed = object.sphere.apply_transform(model_view);
            if !frustum.contains_sphere(transformed) {
                continue;
            }

            let view_position = (model_view * object.sphere.center.extend(1.0)).xyz();
            let distance = view_position.length_squared();

            let model_view_proj = view_proj * model;

            let inv_trans_model_view = Mat3::from_mat4(model_view.inverse().transpose());

            calls.push(CPUDrawCall {
                start_idx: object.start_idx,
                end_idx: object.start_idx + object.count,
                vertex_offset: object.vertex_offset,
                // TODO: Elide these clones?
                material_handle: object.material.get_raw(),
            });

            outputs.push(PerObjectData {
                model_view,
                model_view_proj,
                inv_trans_model_view: inv_trans_model_view.into(),
                material_idx: 0,
            });

            _distances.push(IndexedDistance { distance, index });
        }

        // TODO: Sorting

        assert_eq!(calls.len(), outputs.len());
        assert_eq!(calls.len(), _distances.len());

        if outputs.is_empty() {
            // Dummy data
            outputs.push(PerObjectData {
                model_view: Mat4::ZERO,
                model_view_proj: Mat4::ZERO,
                inv_trans_model_view: Mat3A::ZERO,
                material_idx: 0,
            });
        }

        let output_buffer = args.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("culling output"),
            contents: bytemuck::cast_slice(&outputs),
            usage: BufferUsages::STORAGE,
        });

        let output_bg = args.device.create_bind_group(&BindGroupDescriptor {
            label: Some("culling input bg"),
            layout: &args.interfaces.culled_object_bgl,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: output_buffer.as_entire_binding(),
            }],
        });

        CulledObjectSet {
            calls: ModeData::CPU(calls),
            output_bg,
        }
    }
}

impl Default for CpuCuller {
    fn default() -> Self {
        Self::new()
    }
}

pub fn run<'rpass>(
    rpass: &mut RenderPass<'rpass>,
    draws: &'rpass [CPUDrawCall],
    samplers: &'rpass Samplers,
    samplers_binding_index: u32,
    materials: &'rpass MaterialManager,
    material_binding_index: u32,
) {
    let mut state_sample_type = SampleType::Linear;

    for (idx, draw) in draws.iter().enumerate() {
        let (material_bind_group, sample_type) = materials.cpu_get_bind_group(draw.material_handle);

        // As a workaround for OpenGL's combined samplers, we need to manually swap the linear and nearest samplers so that shader code can think it's always using linear.
        if state_sample_type != sample_type {
            let bg = match sample_type {
                SampleType::Nearest => samplers.nearest_linear_bg.as_ref().as_cpu(),
                SampleType::Linear => &samplers.linear_nearest_bg,
            };
            state_sample_type = sample_type;
            rpass.set_bind_group(samplers_binding_index, bg, &[]);
        }

        rpass.set_bind_group(material_binding_index, material_bind_group, &[]);
        let idx = idx as u32;
        rpass.draw_indexed(draw.start_idx..draw.end_idx, draw.vertex_offset, idx..idx + 1);
    }
}
