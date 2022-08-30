use std::array;

use glam::{Mat4, Vec3, Vec3A};
use rend3_types::{Camera, CameraProjection, Handedness, RawDirectionalLightHandle};

use crate::managers::{CameraManager, InternalDirectionalLight, ShadowCoordinates};

fn shadow_camera(l: &InternalDirectionalLight, user_camera: &CameraManager) -> CameraManager {
    let camera_location = user_camera.location();

    let shadow_texel_size = l.inner.distance / l.inner.resolution as f32;

    let look_at = match user_camera.handedness() {
        Handedness::Left => Mat4::look_at_lh,
        Handedness::Right => Mat4::look_at_rh,
    };

    let origin_view = look_at(Vec3::ZERO, l.inner.direction, Vec3::Y);
    let camera_origin_view = origin_view.transform_point3(camera_location);

    let offset = camera_origin_view.truncate() % shadow_texel_size;
    let shadow_location = camera_origin_view - Vec3::from((offset, 0.0));

    let inv_origin_view = origin_view.inverse();
    let new_shadow_location = inv_origin_view.transform_point3(shadow_location);

    CameraManager::new(
        Camera {
            projection: CameraProjection::Orthographic {
                size: Vec3A::splat(l.inner.distance),
            },
            view: look_at(new_shadow_location, new_shadow_location + l.inner.direction, Vec3::Y),
        },
        user_camera.handedness(),
        None,
    )
}

enum ShadowNode {
    Vacant,
    Leaf(RawDirectionalLightHandle),
    Children([usize; 4]),
}

impl ShadowNode {
    fn try_alloc(
        nodes: &mut Vec<ShadowNode>,
        node_idx: usize,
        relative_order: u32,
        handle: RawDirectionalLightHandle,
    ) -> bool {
        let this = &mut nodes[node_idx];
        match *this {
            ShadowNode::Vacant => {
                if relative_order == 0 {
                    *this = ShadowNode::Leaf(handle);

                    true
                } else {
                    let base_idx = nodes.len();
                    nodes[node_idx] = ShadowNode::Children(array::from_fn(|idx| base_idx + idx));
                    nodes.resize_with(base_idx + 4, || ShadowNode::Vacant);

                    ShadowNode::try_alloc(nodes, node_idx, relative_order, handle)
                }
            }
            ShadowNode::Leaf(_) => false,
            ShadowNode::Children(children) => {
                if relative_order == 0 {
                    return false;
                }

                children
                    .into_iter()
                    .any(|child| ShadowNode::try_alloc(nodes, child, relative_order - 1, handle))
            }
        }
    }
}

fn allocate_shadows(mut lights: Vec<(RawDirectionalLightHandle, u16)>) -> Vec<ShadowCoordinates> {
    lights.sort_by_key(|(_idx, res)| *res);

    if lights.is_empty() {
        return Vec::new();
    }

    let maximum_size = lights.first().unwrap().1;
    let min_leading_zeros = maximum_size.leading_zeros();

    let mut nodes = Vec::new();
    let mut roots = Vec::new();

    for (handle, resolution) in lights {
        let order = resolution.leading_zeros() - min_leading_zeros;

        loop {
            let idx = nodes.len();
            nodes.push(ShadowNode::Vacant);
            roots.push(idx);

            if ShadowNode::try_alloc(&mut nodes, *roots.last().unwrap(), order, handle) {
                break;
            }
        }
    }

    todo!()
}
