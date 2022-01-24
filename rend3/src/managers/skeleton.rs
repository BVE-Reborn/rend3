use std::{
    ops::Range,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::{managers::MeshManager, util::registry::ResourceRegistry};

use glam::Mat4;
use rend3_types::{MeshHandle, RawSkeletonHandle, Skeleton, SkeletonHandle};
use wgpu::{CommandEncoder, Device, Queue};

/// Internal representation of a Skeleton
#[derive(Debug)]
pub struct InternalSkeleton {
    /// A handle to the mesh this skeleton deforms.
    pub mesh_handle: MeshHandle,
    /// The list of per-joint transformation matrices that will be applied to
    /// vertices.
    pub joint_matrices: Vec<Mat4>,
    /// The portion of the vertex buffer data owned by this skeleton
    pub skeleton_vertex_range: Range<usize>,
    /// The vertex ranges that is sent to the GPU Skinning compute shader,
    /// cached here for improved performance.
    pub ranges: GpuVertexRanges,
}

/// The skeleton and mes vertex ranges, in a format that's suitable to be sent
/// to the GPU.
///
/// Note that there's no need for this struct to be `#[repr(C)]`
/// because this is not the actual data that gets uploaded for GPU skinning.
#[derive(Debug, Copy, Clone)]
pub struct GpuVertexRanges {
    /// The range of the vertex buffer that holds the original mesh.
    pub mesh_range: [u32; 2],
    /// The range of the vertex buffer that holds the duplicate mesh data, owned
    /// by the Skeleton
    pub skeleton_range: [u32; 2],
}

/// Manages skeletons.
///
/// Skeletons only contain the relevant data for vertex skinning. No bone
/// hierarchy is stored.
pub struct SkeletonManager {
    registry: ResourceRegistry<InternalSkeleton, Skeleton>,
    /// The number of joints of all the skeletons in this manager
    global_joint_count: usize,
}
impl SkeletonManager {
    pub fn new() -> Self {
        profiling::scope!("SkeletonManager::new");

        let registry = ResourceRegistry::new();

        Self {
            registry,
            global_joint_count: 0,
        }
    }

    pub fn allocate(counter: &AtomicUsize) -> SkeletonHandle {
        let idx = counter.fetch_add(1, Ordering::Relaxed);

        SkeletonHandle::new(idx)
    }

    pub fn fill(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        mesh_manager: &mut MeshManager,
        handle: &SkeletonHandle,
        skeleton: Skeleton,
    ) {
        self.global_joint_count += skeleton.joint_matrices.len();
        let mesh_range = mesh_manager.internal_data(skeleton.mesh.get_raw()).vertex_range.clone();
        let skeleton_range = mesh_manager.allocate_skeleton_mesh(device, encoder, &skeleton.mesh);

        let input = GpuVertexRanges {
            skeleton_range: [skeleton_range.start as u32, skeleton_range.end as u32],
            mesh_range: [mesh_range.start as u32, mesh_range.end as u32],
        };

        let internal = InternalSkeleton {
            joint_matrices: skeleton.joint_matrices,
            mesh_handle: skeleton.mesh,
            skeleton_vertex_range: skeleton_range,
            ranges: input,
        };
        self.registry.insert(handle, internal);
    }

    pub fn ready(&mut self) {
        profiling::scope!("Skeleton Manager Ready");
        self.registry.remove_all_dead(|_, _, skeleton| {
            self.global_joint_count -= skeleton.joint_matrices.len();
        });
    }

    pub fn set_joint_matrices(&mut self, handle: RawSkeletonHandle, joint_matrices: Vec<Mat4>) {
        let skeleton = self.registry.get_mut(handle);
        assert_eq!(joint_matrices.len(), skeleton.joint_matrices.len());
        skeleton.joint_matrices = joint_matrices;
    }

    pub fn internal_data(&self, handle: RawSkeletonHandle) -> &InternalSkeleton {
        self.registry.get(handle)
    }

    pub fn skeletons(&self) -> impl ExactSizeIterator<Item = &InternalSkeleton> {
        self.registry.values()
    }

    /// Get the skeleton manager's global joint count.
    pub fn global_joint_count(&self) -> usize {
        self.global_joint_count
    }
}

impl Default for SkeletonManager {
    fn default() -> Self {
        Self::new()
    }
}
