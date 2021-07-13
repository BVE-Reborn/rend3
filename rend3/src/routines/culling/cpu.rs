use glam::{Mat3, Vec4Swizzles};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BufferUsage, Device, RenderPass,
};

use crate::{
    resources::{CameraManager, InternalObject, MaterialManager},
    routines::culling::{CPUDrawCall, CulledObjectSet, CullingOutput},
    util::{frustum::ShaderFrustum, math::IndexedDistance},
    ModeData,
};

pub fn cull(device: &Device, objects: &[InternalObject], camera: &CameraManager) -> CulledObjectSet {
    let frustum = ShaderFrustum::from_matrix(camera.proj());
    let view = camera.view();
    let view_proj = camera.view_proj();

    let mut outputs = Vec::with_capacity(objects.len());
    let mut calls = Vec::with_capacity(objects.len());
    let mut _distances = Vec::with_capacity(objects.len());

    for (index, object) in objects.into_iter().enumerate() {
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

        outputs.push(CullingOutput {
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

    let output_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("culling output"),
        contents: bytemuck::cast_slice(&outputs),
        usage: BufferUsage::STORAGE,
    });

    CulledObjectSet {
        calls: ModeData::CPU(calls),
        output_buffer,
    }
}

pub fn run<'rpass>(
    rpass: &mut RenderPass<'rpass>,
    draws: &'rpass Vec<CPUDrawCall>,
    materials: &'rpass MaterialManager,
    material_binding_index: u32,
) {
    for draws in draws {
        rpass.set_bind_group(material_binding_index, materials.cpu_get_bind_group(draws.handle), &[]);
        rpass.draw_indexed(0..draws.count, draws.vertex_offset, 0..1);
    }
}
