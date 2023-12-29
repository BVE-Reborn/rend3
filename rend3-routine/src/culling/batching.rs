use std::{cmp::Ordering, collections::HashMap};

use encase::ShaderType;
use ordered_float::OrderedFloat;
use rend3::{
    graph::NodeExecutionContext,
    managers::{CameraState, TextureBindGroupIndex},
    types::{GraphDataHandle, Material, RawObjectHandle, SortingOrder, SortingReason},
    util::{math::round_up, typedefs::FastHashMap},
};

use crate::common::CameraSpecifier;

use super::{BATCH_SIZE, WORKGROUP_SIZE};

#[derive(Debug)]
pub struct ShaderBatchDatas {
    pub(super) regions: Vec<JobSubRegion>,
    pub(super) jobs: Vec<ShaderBatchData>,
}

#[derive(Debug)]
pub(super) struct JobSubRegion {
    pub job_index: u32,
    pub key: ShaderJobKey,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ShaderJobKey {
    pub material_key: u64,
    pub bind_group_index: TextureBindGroupIndex,
}

#[derive(Debug, Clone, Copy, Eq)]
pub(super) struct ShaderJobSortingKey {
    pub job_key: ShaderJobKey,
    pub distance: OrderedFloat<f32>,
    pub sorting_reason: SortingReason,
}

impl PartialEq for ShaderJobSortingKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl PartialOrd for ShaderJobSortingKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ShaderJobSortingKey {
    fn cmp(&self, other: &Self) -> Ordering {
        // material key always first
        match self.job_key.material_key.cmp(&other.job_key.material_key) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.sorting_reason.cmp(&other.sorting_reason) {
            Ordering::Equal => {}
            ord => return ord,
        }
        // The above comparison means that both sides are equal
        if self.sorting_reason == SortingReason::Requirement {
            match self.distance.cmp(&other.distance) {
                Ordering::Equal => {}
                ord => return ord,
            }
            self.job_key.bind_group_index.cmp(&other.job_key.bind_group_index)
        } else {
            match self.job_key.bind_group_index.cmp(&other.job_key.bind_group_index) {
                Ordering::Equal => {}
                ord => return ord,
            }
            self.distance.cmp(&other.distance)
        }
    }
}

#[derive(Debug, ShaderType)]
pub struct ShaderBatchData {
    #[align(256)]
    pub(super) total_objects: u32,
    pub(super) total_invocations: u32,
    pub(super) batch_base_invocation: u32,
    pub(super) object_culling_information: [ShaderObjectCullingInformation; BATCH_SIZE],
}

#[derive(Debug, Copy, Clone, Default, ShaderType)]
pub(super) struct ShaderObjectCullingInformation {
    pub invocation_start: u32,
    pub invocation_end: u32,
    pub object_id: u32,
    pub region_id: u32,
    pub base_region_invocation: u32,
    pub local_region_id: u32,
    pub previous_global_invocation: u32,
    pub atomic_capable: u32,
}

/// Map containing the previous invocation of each object.
pub struct PerCameraPreviousInvocationsMap {
    inner: FastHashMap<CameraSpecifier, FastHashMap<RawObjectHandle, u32>>,
}
impl PerCameraPreviousInvocationsMap {
    pub fn new() -> Self {
        Self {
            inner: HashMap::default(),
        }
    }

    pub fn get_and_reset_camera(&mut self, camera: CameraSpecifier) -> FastHashMap<RawObjectHandle, u32> {
        self.inner.remove(&camera).unwrap_or_default()
    }

    pub fn set_camera(&mut self, camera: CameraSpecifier, previous_invocations: FastHashMap<RawObjectHandle, u32>) {
        self.inner.insert(camera, previous_invocations);
    }
}

pub(super) fn batch_objects<M: Material>(
    ctx: &mut NodeExecutionContext,
    previous_invocation_map_handle: &GraphDataHandle<PerCameraPreviousInvocationsMap>,
    camera: &CameraState,
    camera_specifier: CameraSpecifier,
) -> ShaderBatchDatas {
    profiling::scope!("Batch Objects");

    let mut per_camera_previous_invocation_map = ctx.data_core.graph_storage.get_mut(previous_invocation_map_handle);
    let previous_invocation_map = per_camera_previous_invocation_map.get_and_reset_camera(camera_specifier);
    let mut current_invocation_map = FastHashMap::default();

    let mut jobs = ShaderBatchDatas {
        jobs: Vec::new(),
        regions: Vec::new(),
    };

    let objects = match ctx.data_core.object_manager.enumerated_objects::<M>() {
        Some(o) => o,
        None => return jobs,
    };

    let material_archetype = ctx.data_core.material_manager.archetype_view::<M>();

    let mut sorted_objects = Vec::with_capacity(objects.len());
    {
        profiling::scope!("Sort Key Creation");
        for (handle, object) in objects {
            // Frustum culling
            if !camera.world_frustum().contains_sphere(object.inner.bounding_sphere) {
                continue;
            }

            let material = material_archetype.material(*object.material_handle);
            let bind_group_index = material
                .bind_group_index
                .map_gpu(|_| TextureBindGroupIndex::DUMMY)
                .into_common();

            let material_key = material.inner.key();
            let sorting = material.inner.sorting();

            let mut distance_sq = ctx
                .data_core
                .viewport_camera_state
                .location()
                .distance_squared(object.location.into());
            if sorting.order == SortingOrder::BackToFront {
                distance_sq = -distance_sq;
            }
            sorted_objects.push((
                ShaderJobSortingKey {
                    job_key: ShaderJobKey {
                        material_key,
                        bind_group_index,
                    },
                    distance: OrderedFloat(distance_sq),
                    sorting_reason: sorting.reason,
                },
                handle,
                object,
            ))
        }
    }

    {
        profiling::scope!("Sorting");
        sorted_objects.sort_unstable_by_key(|(k, _, _)| *k);
    }

    if !sorted_objects.is_empty() {
        profiling::scope!("Batch Data Creation");
        let mut current_region_idx = 0_u32;
        let mut current_region_object_index = 0_u32;
        let mut current_base_invocation = 0_u32;
        let mut current_region_invocation = 0_u32;
        let mut current_invocation = 0_u32;
        let mut current_object_index = 0_u32;
        let mut current_ranges = [ShaderObjectCullingInformation::default(); BATCH_SIZE];
        let mut current_key = sorted_objects.first().unwrap().0.job_key;

        let max_dispatch_count = ctx.renderer.limits.max_compute_workgroups_per_dimension;

        for (
            ShaderJobSortingKey {
                job_key: key,
                sorting_reason,
                ..
            },
            handle,
            object,
        ) in sorted_objects
        {
            let invocation_count = object.inner.index_count / 3;

            let key_difference = key != current_key;
            let object_limit = current_object_index == BATCH_SIZE as u32;
            let dispatch_limit = (current_invocation + invocation_count) >= max_dispatch_count * WORKGROUP_SIZE;

            if key_difference || object_limit || dispatch_limit {
                jobs.regions.push(JobSubRegion {
                    job_index: jobs.jobs.len() as u32,
                    key: current_key,
                });
                current_region_idx += 1;
                current_key = key;
                current_region_object_index = 0;
                current_region_invocation = current_invocation;
            }
            if object_limit || dispatch_limit {
                jobs.jobs.push(ShaderBatchData {
                    object_culling_information: current_ranges,
                    total_objects: current_object_index,
                    total_invocations: current_invocation,
                    batch_base_invocation: current_base_invocation,
                });

                current_base_invocation += current_invocation;
                current_invocation = 0;
                current_region_invocation = 0;
                current_object_index = 0;
            }

            let range = ShaderObjectCullingInformation {
                invocation_start: current_invocation,
                invocation_end: current_invocation + invocation_count,
                region_id: current_region_idx,
                object_id: handle.idx as u32,
                base_region_invocation: current_region_invocation,
                local_region_id: current_region_object_index,
                previous_global_invocation: previous_invocation_map.get(&handle).copied().unwrap_or(u32::MAX),
                atomic_capable: matches!(sorting_reason, SortingReason::Optimization) as u32,
            };

            current_invocation_map.insert(handle, current_invocation + current_base_invocation);

            current_ranges[current_object_index as usize] = range;
            current_object_index += 1;
            current_region_object_index += 1;
            current_invocation += round_up(invocation_count, WORKGROUP_SIZE);
        }

        jobs.regions.push(JobSubRegion {
            job_index: jobs.jobs.len() as u32,
            key: current_key,
        });
        jobs.jobs.push(ShaderBatchData {
            object_culling_information: current_ranges,
            total_objects: current_object_index,
            total_invocations: current_invocation,
            batch_base_invocation: current_base_invocation,
        });
    }

    per_camera_previous_invocation_map.set_camera(camera_specifier, current_invocation_map);

    jobs
}
