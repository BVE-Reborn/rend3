use glam::{Mat3, Vec4Swizzles};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BindGroupDescriptor, BindGroupEntry, BufferUsages, Device, RenderPass,
};

use crate::{
    resources::{CameraManager, InternalObject, MaterialManager},
    routines::{
        common::interfaces::{PerObjectData, ShaderInterfaces},
        culling::{CPUDrawCall, CulledObjectSet},
    },
    util::{frustum::ShaderFrustum, math::IndexedDistance},
    ModeData,
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
                count: object.count,
                vertex_offset: object.vertex_offset,
                handle: object.material,
            });

            outputs.push(PerObjectData {
                model_view: model_view.into(),
                model_view_proj: model_view_proj.into(),
                inv_trans_model_view: inv_trans_model_view.into(),
                material_idx: 0,
            });

            _distances.push(IndexedDistance { distance, index });
        }

        // TODO: Sorting

        assert_eq!(calls.len(), outputs.len());
        assert_eq!(calls.len(), _distances.len());

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

pub fn run<'rpass>(
    rpass: &mut RenderPass<'rpass>,
    draws: &'rpass [CPUDrawCall],
    materials: &'rpass MaterialManager,
    material_binding_index: u32,
) {
    for (idx, draws) in draws.iter().enumerate() {
        rpass.set_bind_group(material_binding_index, materials.cpu_get_bind_group(draws.handle), &[]);
        let idx = idx as u32;
        rpass.draw_indexed(0..draws.count, draws.vertex_offset, idx..idx + 1);
    }
}
