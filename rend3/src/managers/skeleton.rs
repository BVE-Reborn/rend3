use std::sync::atomic::{AtomicUsize, Ordering};

use crate::util::registry::ResourceRegistry;

use glam::Mat4;
use rend3_types::{RawSkeletonHandle, Skeleton, SkeletonHandle};

/// Internal representation of a Skeleton
#[repr(C, align(16))]
#[derive(Debug, Clone)]
pub struct InternalSkeleton {
    /// Stores one transformation matrix for each joint. These are the
    /// transformations that will be applied to the vertices affected by the
    /// corresponding joint. Not to be confused with the transform matrix of the
    /// joint itself.
    /// TODO: Add note about utility function that takes inverseBindMatrices
    pub joint_deltas: Vec<Mat4>,
}

/// Manages skeletons. Skeletons only contain the relevant data for vertex
/// skinning. No bone hierarchy is stored.
pub struct SkeletonManager {
    registry: ResourceRegistry<InternalSkeleton, Skeleton>,
}
impl SkeletonManager {
    pub fn new() -> Self {
        profiling::scope!("SkeletonManager::new");

        let registry = ResourceRegistry::new();

        Self { registry }
    }

    pub fn allocate(counter: &AtomicUsize) -> SkeletonHandle {
        let idx = counter.fetch_add(1, Ordering::Relaxed);

        SkeletonHandle::new(idx)
    }

    pub fn fill(&mut self, handle: &SkeletonHandle, skeleton: Skeleton) {
        let internal = InternalSkeleton {
            joint_deltas: skeleton.joint_deltas,
        };
        self.registry.insert(handle, internal);
    }

    pub fn ready(&mut self) {
        profiling::scope!("Skeleton Manager Ready");
        self.registry.remove_all_dead(|_, _, _| {});
    }

    pub fn set_joint_deltas(&mut self, handle: RawSkeletonHandle, joint_deltas: Vec<Mat4>) {
        let skeleton = self.registry.get_mut(handle);
        skeleton.joint_deltas = joint_deltas;
    }
}

impl Default for SkeletonManager {
    fn default() -> Self {
        Self::new()
    }
}
