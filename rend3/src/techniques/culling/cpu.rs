use glam::{Mat3, Vec4Swizzles};

use crate::{
    modules::{CameraManager, InternalObject},
    techniques::culling::{CPUDrawCall, CpuCulledObjectSet, ShaderOutputObject},
    util::{frustum::ShaderFrustum, math::IndexedDistance},
};

pub(super) fn run(objects: &[InternalObject], camera: &CameraManager) -> CpuCulledObjectSet {
    let frustum = ShaderFrustum::from_matrix(camera.proj());
    let view = camera.view();
    let view_proj = camera.view_proj();

    let mut object_set = CpuCulledObjectSet {
        call: Vec::with_capacity(objects.len()),
        output: Vec::with_capacity(objects.len()),
        distance: Vec::with_capacity(objects.len()),
    };

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

        let inv_trans_model_view = Mat3::from(model_view.inverse().transpose());

        object_set.call.push(CPUDrawCall {
            start_idx: object.start_idx,
            count: object.count,
            vertex_offset: object.vertex_offset,
            handle: object.material,
        });

        object_set.output.push(ShaderOutputObject {
            model_view: model_view.into(),
            model_view_proj: model_view_proj.into(),
            inv_trans_model_view: inv_trans_model_view.into(),
            _material_idx: 0,
            _active: 0,
        });

        object_set.distance.push(IndexedDistance { distance, index });
    }

    assert_eq!(object_set.call.len(), object_set.output.len());
    assert_eq!(object_set.call.len(), object_set.distance.len());

    object_set
}
