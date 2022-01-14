use std::{
    ops::Range,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::util::registry::ResourceRegistry;

use glam::Mat4;
use rend3_types::{MeshHandle, RawSkeletonHandle, Skeleton, SkeletonHandle};
use wgpu::{CommandEncoder, Device, Queue};

use super::MeshManager;

/// Internal representation of a Skeleton
#[repr(C, align(16))]
#[derive(Debug, Clone)]
pub struct InternalSkeleton {
    pub mesh_handle: MeshHandle,
    pub joint_deltas: Vec<Mat4>,
    pub vertex_range: Range<usize>,
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

    pub fn fill(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        mesh_manager: &mut MeshManager,
        handle: &SkeletonHandle,
        skeleton: Skeleton,
    ) {
        let vertex_range = mesh_manager.allocate_skeleton_mesh(device, queue, encoder, &skeleton.mesh);

        let internal = InternalSkeleton {
            joint_deltas: skeleton.joint_deltas,
            mesh_handle: skeleton.mesh,
            vertex_range,
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

    pub fn internal_data(&self, handle: RawSkeletonHandle) -> &InternalSkeleton {
        self.registry.get(handle)
    }
}

impl Default for SkeletonManager {
    fn default() -> Self {
        Self::new()
    }
}
