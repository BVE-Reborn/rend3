use std::{
    ops::Range,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::{
    managers::{MeshManager, ObjectManager},
    util::registry::ResourceRegistry,
};

use glam::{Mat4, UVec2};
use rend3_types::{MeshHandle, RawSkeletonHandle, Skeleton, SkeletonHandle};
use wgpu::{CommandEncoder, Device};

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
    pub mesh_range: glam::UVec2,
    /// The range of the vertex buffer that holds the duplicate mesh data, owned
    /// by the Skeleton
    pub skeleton_range: glam::UVec2,
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

    #[allow(clippy::too_many_arguments)]
    pub fn fill(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        mesh_manager: &mut MeshManager,
        object_manager: &mut ObjectManager,
        handle: &SkeletonHandle,
        skeleton: Skeleton,
    ) {
        let internal_mesh = mesh_manager.internal_data(skeleton.mesh.get_raw());
        let num_joints = internal_mesh.num_joints as usize;

        assert!(
            internal_mesh.num_joints as usize <= skeleton.joint_matrices.len(),
            "Not enough joints to create this skeleton. The mesh has {} joints, \
             but only {} joint matrices were provided.",
            num_joints,
            skeleton.joint_matrices.len(),
        );

        self.global_joint_count += num_joints;

        let skeleton_range = mesh_manager.allocate_skeleton_mesh(device, encoder, object_manager, self, &skeleton.mesh);

        // It is important that we fetch the internal mesh again after calling
        // `allocate_skeleton_mesh`, because that may trigger a reallocation and
        // data like the vertex ranges gets invalidated.
        //
        // Similarly, we don't want to register the back reference to the
        // skeleton before allocating, otherwise the mesh manager will try to
        // reallocate the data for the skeleton we're trying to allocate.
        let internal_mesh = mesh_manager.internal_data_mut(skeleton.mesh.get_raw());
        internal_mesh.skeletons.push(handle.get_raw());
        let mesh_range = internal_mesh.vertex_range.clone();
        let input = GpuVertexRanges {
            skeleton_range: UVec2::new(skeleton_range.start as u32, skeleton_range.end as u32),
            mesh_range: UVec2::new(mesh_range.start as u32, mesh_range.end as u32),
        };

        // Ensure there will be exactly `num_joints` matrices.
        let mut joint_matrices = skeleton.joint_matrices;
        joint_matrices.truncate(num_joints);

        let internal = InternalSkeleton {
            joint_matrices,
            mesh_handle: skeleton.mesh,
            skeleton_vertex_range: skeleton_range,
            ranges: input,
        };
        self.registry.insert(handle, internal);
    }

    pub fn ready(&mut self, mesh_manager: &mut MeshManager) {
        profiling::scope!("Skeleton Manager Ready");
        self.registry.remove_all_dead(|_, handle_idx, skeleton| {
            self.global_joint_count -= skeleton.joint_matrices.len();

            // Clean back references in the mesh data
            let mesh = mesh_manager.internal_data_mut(skeleton.mesh_handle.get_raw());
            let index = mesh.skeletons.iter().position(|sk| sk.idx == handle_idx).unwrap();
            mesh.skeletons.swap_remove(index);

            // Free the owned region of the vertex buffer
            mesh_manager.free_skeleton_mesh(skeleton.skeleton_vertex_range);
        });
    }

    pub fn set_joint_matrices(&mut self, handle: RawSkeletonHandle, mut joint_matrices: Vec<Mat4>) {
        let skeleton = self.registry.get_mut(handle);
        assert!(
            skeleton.joint_matrices.len() <= joint_matrices.len(),
            "Not enough joints to update this skeleton. The mesh has {} joints, \
            but only {} joint matrices were provided.",
            skeleton.joint_matrices.len(),
            joint_matrices.len(),
        );
        // Truncate to avoid storing any extra joint matrices
        joint_matrices.truncate(skeleton.joint_matrices.len());
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

    pub fn set_skeleton_range(
        &mut self,
        handle: RawSkeletonHandle,
        new_skeleton_vert_range: &Range<usize>,
        new_mesh_vert_range: &Range<usize>,
    ) {
        let skeleton = self.registry.get_mut(handle);
        skeleton.skeleton_vertex_range = new_skeleton_vert_range.clone();
        skeleton.ranges.mesh_range = UVec2::new(new_mesh_vert_range.start as u32, new_mesh_vert_range.end as u32);
        skeleton.ranges.skeleton_range =
            UVec2::new(new_skeleton_vert_range.start as u32, new_skeleton_vert_range.end as u32);
    }
}

impl Default for SkeletonManager {
    fn default() -> Self {
        Self::new()
    }
}
