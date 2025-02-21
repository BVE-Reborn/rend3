use std::{array, cmp::Reverse, collections::VecDeque};

use glam::UVec2;
use rend3_types::RawDirectionalLightHandle;

#[cfg_attr(test, derive(Debug, PartialEq))]
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
        handle: &RawDirectionalLightHandle,
    ) -> bool {
        let this = &mut nodes[node_idx];
        match *this {
            ShadowNode::Vacant => {
                if relative_order == 0 {
                    *this = ShadowNode::Leaf(*handle);

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

                children.into_iter().any(|child| ShadowNode::try_alloc(nodes, child, relative_order - 1, handle))
            }
        }
    }
}

pub(super) struct ShadowAtlas {
    pub texture_dimensions: UVec2,
    pub maps: Vec<ShadowMap>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ShadowMap {
    pub offset: UVec2,
    pub size: u32,
    pub handle: RawDirectionalLightHandle,
}

pub(super) fn allocate_shadow_atlas(
    mut maps: Vec<(RawDirectionalLightHandle, u16)>,
    max_dimension: u32,
) -> Option<ShadowAtlas> {
    if maps.is_empty() {
        return None;
    }
    if max_dimension == 0 {
        return None;
    }

    maps.sort_by_key(|(_idx, res)| Reverse(*res));

    let root_size = maps.first().unwrap().1 as u32;
    let min_leading_zeros = (root_size as u16).leading_zeros();

    let mut nodes = Vec::with_capacity(maps.len().next_power_of_two());
    let mut roots = Vec::new();

    nodes.push(ShadowNode::Vacant);
    roots.push(0);

    for (handle, resolution) in maps {
        debug_assert!(resolution.is_power_of_two());
        debug_assert_ne!(resolution, 0);
        let order = resolution.leading_zeros() - min_leading_zeros;

        loop {
            if ShadowNode::try_alloc(&mut nodes, *roots.last().unwrap(), order, &handle) {
                break;
            }

            let idx = nodes.len();
            nodes.push(ShadowNode::Vacant);
            roots.push(idx);
        }
    }

    let available_columns = max_dimension / root_size;
    let root_count = roots.len() as f32;
    let rows_needed = f32::ceil(root_count / available_columns as f32);
    let columns_needed = f32::ceil(root_count / rows_needed) as u32;

    let texture_dimensions = UVec2::new(columns_needed, rows_needed as u32) * root_size;

    let mut nodes_to_visit: VecDeque<_> = roots
        .into_iter()
        .enumerate()
        .map(|(root_idx, node_idx)| {
            let offset = UVec2::new(root_idx as u32 % columns_needed, root_idx as u32 / columns_needed) * root_size;

            (1_u32, offset, node_idx)
        })
        .collect();

    let mut output_maps = Vec::with_capacity(nodes.len());
    while let Some((root_divisor, offset, node_idx)) = nodes_to_visit.pop_front() {
        let size = root_size / root_divisor;
        let half_size = size / 2;

        match nodes[node_idx] {
            ShadowNode::Vacant => {}
            ShadowNode::Leaf(handle) => output_maps.push(ShadowMap { offset, size, handle }),
            ShadowNode::Children(children) => {
                let child_divisor = root_divisor * 2;
                nodes_to_visit.extend(children.into_iter().enumerate().map(|(child_idx, node_idx)| {
                    // child_idx turned from [0, 3] to a 2x2 square.
                    let child_2d_idx = UVec2::new(child_idx as u32 % 2, child_idx as u32 / 2);
                    let child_offset = offset + half_size * child_2d_idx;

                    (child_divisor, child_offset, node_idx)
                }))
            }
        }
    }

    Some(ShadowAtlas { texture_dimensions, maps: output_maps })
}

#[cfg(test)]
mod tests {
    use glam::UVec2;
    use rend3_types::RawDirectionalLightHandle as RDLH;

    use super::ShadowNode;
    use crate::managers::directional::shadow_alloc::{allocate_shadow_atlas, ShadowMap};

    #[test]
    fn chunk_subdivision_single() {
        let mut nodes = vec![ShadowNode::Vacant];

        assert!(ShadowNode::try_alloc(&mut nodes, 0, 0, &RDLH::new(0)));
        assert_eq!(&nodes, &[ShadowNode::Leaf(RDLH::new(0))]);
    }

    #[test]
    fn chunk_subdivision_single_failure() {
        let mut nodes = vec![ShadowNode::Vacant];

        assert!(ShadowNode::try_alloc(&mut nodes, 0, 0, &RDLH::new(0)));
        assert!(!ShadowNode::try_alloc(&mut nodes, 0, 0, &RDLH::new(1)));
        assert_eq!(&nodes, &[ShadowNode::Leaf(RDLH::new(0))]);
    }

    #[test]
    fn chunk_subdivision_multiple() {
        let mut nodes = vec![ShadowNode::Vacant];

        assert!(ShadowNode::try_alloc(&mut nodes, 0, 1, &RDLH::new(0)));
        assert!(ShadowNode::try_alloc(&mut nodes, 0, 1, &RDLH::new(1)));
        assert_eq!(
            &nodes,
            &[
                ShadowNode::Children([1, 2, 3, 4]),
                ShadowNode::Leaf(RDLH::new(0)),
                ShadowNode::Leaf(RDLH::new(1)),
                ShadowNode::Vacant,
                ShadowNode::Vacant
            ]
        );
    }

    #[test]
    fn chunk_subdivision_multiple_failure() {
        let mut nodes = vec![ShadowNode::Vacant];

        for i in 0..4 {
            assert!(ShadowNode::try_alloc(&mut nodes, 0, 1, &RDLH::new(i)));
        }
        assert!(!ShadowNode::try_alloc(&mut nodes, 0, 1, &RDLH::new(5)));
        assert_eq!(
            &nodes,
            &[
                ShadowNode::Children([1, 2, 3, 4]),
                ShadowNode::Leaf(RDLH::new(0)),
                ShadowNode::Leaf(RDLH::new(1)),
                ShadowNode::Leaf(RDLH::new(2)),
                ShadowNode::Leaf(RDLH::new(3)),
            ]
        );
    }

    #[test]
    fn chunk_subdivision_multiple_nested() {
        let mut nodes = vec![ShadowNode::Vacant];

        assert!(ShadowNode::try_alloc(&mut nodes, 0, 1, &RDLH::new(0)));
        assert!(ShadowNode::try_alloc(&mut nodes, 0, 1, &RDLH::new(1)));
        assert!(ShadowNode::try_alloc(&mut nodes, 0, 2, &RDLH::new(2)));
        assert!(ShadowNode::try_alloc(&mut nodes, 0, 1, &RDLH::new(3)));
        assert!(ShadowNode::try_alloc(&mut nodes, 0, 2, &RDLH::new(4)));
        assert_eq!(
            &nodes,
            &[
                ShadowNode::Children([1, 2, 3, 4]),
                ShadowNode::Leaf(RDLH::new(0)),
                ShadowNode::Leaf(RDLH::new(1)),
                ShadowNode::Children([5, 6, 7, 8]),
                ShadowNode::Leaf(RDLH::new(3)),
                ShadowNode::Leaf(RDLH::new(2)),
                ShadowNode::Leaf(RDLH::new(4)),
                ShadowNode::Vacant,
                ShadowNode::Vacant,
            ]
        );
    }

    #[test]
    fn allocate_single() {
        let maps = vec![(RDLH::new(0), 16)];

        let res = allocate_shadow_atlas(maps, 16).unwrap();
        assert_eq!(res.texture_dimensions, UVec2::splat(16));
        assert_eq!(res.maps, &[ShadowMap { offset: UVec2::splat(0), size: 16, handle: RDLH::new(0) }]);
    }

    #[test]
    fn allocate_single_level_single_row() {
        let maps = vec![(RDLH::new(0), 16), (RDLH::new(1), 16), (RDLH::new(2), 16)];

        let res = allocate_shadow_atlas(maps, 48).unwrap();
        assert_eq!(res.texture_dimensions, UVec2::new(48, 16));
        assert_eq!(
            res.maps,
            &[
                ShadowMap { offset: UVec2::splat(0), size: 16, handle: RDLH::new(0) },
                ShadowMap { offset: UVec2::new(16, 0), size: 16, handle: RDLH::new(1) },
                ShadowMap { offset: UVec2::new(32, 0), size: 16, handle: RDLH::new(2) }
            ]
        );
    }

    #[test]
    fn allocate_single_level_double_row() {
        let maps = vec![(RDLH::new(0), 16), (RDLH::new(1), 16), (RDLH::new(2), 16)];

        let res = allocate_shadow_atlas(maps, 32).unwrap();
        assert_eq!(res.texture_dimensions, UVec2::new(32, 32));
        assert_eq!(
            res.maps,
            &[
                ShadowMap { offset: UVec2::splat(0), size: 16, handle: RDLH::new(0) },
                ShadowMap { offset: UVec2::new(16, 0), size: 16, handle: RDLH::new(1) },
                ShadowMap { offset: UVec2::new(0, 16), size: 16, handle: RDLH::new(2) }
            ]
        );
    }

    #[test]
    fn allocate_single_level_double_row_extra_space() {
        let maps =
            vec![(RDLH::new(0), 16), (RDLH::new(1), 16), (RDLH::new(2), 16), (RDLH::new(3), 16), (RDLH::new(4), 16)];

        let res = allocate_shadow_atlas(maps, 64).unwrap();
        assert_eq!(res.texture_dimensions, UVec2::new(48, 32));
        assert_eq!(
            res.maps,
            &[
                ShadowMap { offset: UVec2::splat(0), size: 16, handle: RDLH::new(0) },
                ShadowMap { offset: UVec2::new(16, 0), size: 16, handle: RDLH::new(1) },
                ShadowMap { offset: UVec2::new(32, 0), size: 16, handle: RDLH::new(2) },
                ShadowMap { offset: UVec2::new(0, 16), size: 16, handle: RDLH::new(3) },
                ShadowMap { offset: UVec2::new(16, 16), size: 16, handle: RDLH::new(4) }
            ]
        );
    }

    /// ┌───────────────┬───────┬───────┐
    /// │               │       │       │
    /// │               │   1   │   2   │
    /// │               │       │       │
    /// │       0       ├───┬───┼───────┘
    /// │               │ 3 │ 4 │
    /// │               ├───┼───┘
    /// │               │ 5 │
    /// └───────────────┴───┘
    #[test]
    fn allocate_multiple_level() {
        let maps = vec![
            (RDLH::new(0), 16),
            (RDLH::new(1), 8),
            (RDLH::new(2), 8),
            (RDLH::new(3), 4),
            (RDLH::new(4), 4),
            (RDLH::new(5), 4),
        ];

        let res = allocate_shadow_atlas(maps, 32).unwrap();
        assert_eq!(res.texture_dimensions, UVec2::new(32, 16));
        assert_eq!(
            res.maps,
            &[
                ShadowMap { offset: UVec2::splat(0), size: 16, handle: RDLH::new(0) },
                ShadowMap { offset: UVec2::new(16, 0), size: 8, handle: RDLH::new(1) },
                ShadowMap { offset: UVec2::new(24, 0), size: 8, handle: RDLH::new(2) },
                ShadowMap { offset: UVec2::new(16, 8), size: 4, handle: RDLH::new(3) },
                ShadowMap { offset: UVec2::new(20, 8), size: 4, handle: RDLH::new(4) },
                ShadowMap { offset: UVec2::new(16, 12), size: 4, handle: RDLH::new(5) },
            ]
        );
    }
}
