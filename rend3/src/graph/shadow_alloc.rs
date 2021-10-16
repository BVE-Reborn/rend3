use glam::UVec2;
use wgpu::Extent3d;

use crate::{util::typedefs::FastHashMap, ShadowCoordinates};

pub fn allocate_shadows(
    shadows: &FastHashMap<usize, usize>,
) -> Option<(Extent3d, FastHashMap<usize, ShadowCoordinates>)> {
    let mut sorted = shadows.iter().map(|(&id, &size)| (id, size)).collect::<Vec<_>>();
    sorted.sort_by_key(|(_, size)| usize::MAX - size);

    if sorted.is_empty() {
        return None;
    }

    let mut shadow_coordinate = FastHashMap::with_capacity_and_hasher(sorted.len(), Default::default());
    let mut sorted_iter = sorted.into_iter();
    let (id, max_size) = sorted_iter.next().unwrap();
    shadow_coordinate.insert(
        id,
        ShadowCoordinates {
            layer: 0,
            offset: UVec2::splat(0),
            size: max_size,
        },
    );

    let mut current_layer = 1usize;
    let mut current_size = 0usize;
    let mut current_count = 0usize;

    for (id, size) in sorted_iter {
        if size != current_size {
            current_layer += 1;
            current_size = size;
            current_count = 0;
        }

        let maps_per_dim = max_size / current_size;
        let total_maps_per_layer = maps_per_dim * maps_per_dim;

        if current_count >= total_maps_per_layer {
            current_layer += 1;
            current_count = 0;
        }

        let offset = UVec2::new(
            (current_count % maps_per_dim) as u32,
            (current_count / maps_per_dim) as u32,
        ) * current_size as u32;

        shadow_coordinate.insert(
            id,
            ShadowCoordinates {
                layer: current_layer,
                offset,
                size,
            },
        );
    }

    Some((
        Extent3d {
            width: max_size as u32,
            height: max_size as u32,
            depth_or_array_layers: current_layer as u32,
        },
        shadow_coordinate,
    ))
}
