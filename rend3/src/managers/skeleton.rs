use std::ops::Range;

use arrayvec::ArrayVec;
use glam::Mat4;
use rend3_types::{
    MeshHandle, RawSkeletonHandle, Skeleton, VertexAttributeId, VERTEX_ATTRIBUTE_JOINT_INDICES,
    VERTEX_ATTRIBUTE_JOINT_WEIGHTS, VERTEX_ATTRIBUTE_NORMAL, VERTEX_ATTRIBUTE_POSITION, VERTEX_ATTRIBUTE_TANGENT,
};
use thiserror::Error;
use wgpu::Device;

use crate::{
    managers::{MeshCreationError, MeshManager},
    util::iter::ExactSizerIterator,
};

/// Internal representation of a Skeleton
#[derive(Debug)]
pub struct InternalSkeleton {
    /// A handle to the mesh this skeleton deforms.
    pub mesh_handle: MeshHandle,
    /// The list of per-joint transformation matrices that will be applied to
    /// vertices.
    pub joint_matrices: Vec<Mat4>,
    /// There are 5 different ranges we need to store here:
    /// Position, Normals, Tangent, Joint Index, Joint Weight
    pub source_attribute_ranges: ArrayVec<(VertexAttributeId, Range<u64>), 5>,
    /// There are three attributes that we can possibly override here:
    /// Position, Normals, and Tangent
    pub overridden_attribute_ranges: ArrayVec<(VertexAttributeId, Range<u64>), 3>,
    /// Amount of vertices in the pointed to mesh
    pub vertex_count: u32,
}

#[derive(Debug, Error)]
pub enum SkeletonCreationError {
    #[error("Failed to create needed resources in the mesh manager")]
    MeshFailure(#[from] MeshCreationError),
    #[error("Mesh must have joint indices to be used in a skeleton")]
    MissingAttributesJointIndices,
    #[error("Mesh must have joint weights to be used in a skeleton")]
    MissingAttributesJointWeights,
    #[error(
        "Not enough joints to create this skeleton. The mesh has {mesh_joint_count} joints, \
         but only {joint_matrix_count} joint matrices were provided."
    )]
    NotEnoughJoints { mesh_joint_count: u16, joint_matrix_count: usize },
}

/// Manages skeletons.
///
/// Skeletons only contain the relevant data for vertex skinning. No bone
/// hierarchy is stored.
pub struct SkeletonManager {
    data: Vec<Option<InternalSkeleton>>,
    skeleton_count: usize,
    /// The number of joints of all the skeletons in this manager
    global_joint_count: usize,
}
impl SkeletonManager {
    pub fn new() -> Self {
        profiling::scope!("SkeletonManager::new");

        Self { data: Vec::new(), skeleton_count: 0, global_joint_count: 0 }
    }

    pub fn validate_skeleton(
        device: &Device,
        mesh_manager: &MeshManager,
        skeleton: Skeleton,
    ) -> Result<InternalSkeleton, SkeletonCreationError> {
        let internal_mesh = &mesh_manager.lock_internal_data()[*skeleton.mesh.get_raw()];
        let required_joint_count =
            internal_mesh.required_joint_count.ok_or(SkeletonCreationError::MissingAttributesJointIndices)?;

        let joint_weight_range = internal_mesh
            .get_attribute(&VERTEX_ATTRIBUTE_JOINT_WEIGHTS)
            .ok_or(SkeletonCreationError::MissingAttributesJointWeights)?;
        let joint_indices_range = internal_mesh
            .get_attribute(&VERTEX_ATTRIBUTE_JOINT_INDICES)
            .ok_or(SkeletonCreationError::MissingAttributesJointIndices)?;

        // Converts the following assert to an error
        if required_joint_count as usize > skeleton.joint_matrices.len() {
            return Err(SkeletonCreationError::NotEnoughJoints {
                mesh_joint_count: required_joint_count,
                joint_matrix_count: skeleton.joint_matrices.len(),
            });
        }

        let overridden_attributes = [&VERTEX_ATTRIBUTE_POSITION, &VERTEX_ATTRIBUTE_NORMAL, &VERTEX_ATTRIBUTE_TANGENT];

        let mut source_attribute_ranges: ArrayVec<_, 5> = ArrayVec::new();
        source_attribute_ranges.push((*VERTEX_ATTRIBUTE_JOINT_WEIGHTS.id(), joint_weight_range));
        source_attribute_ranges.push((*VERTEX_ATTRIBUTE_JOINT_INDICES.id(), joint_indices_range));

        let mut overridden_attribute_ranges: ArrayVec<_, 3> = ArrayVec::new();
        for attribute in overridden_attributes {
            let original_range = match internal_mesh.get_attribute(attribute) {
                Some(a) => a,
                None => continue,
            };
            source_attribute_ranges.push((*attribute.id(), original_range));
        }

        // We split this for loop into two parts so that because we need &mut on the mesh manager
        // the original loop needs & on the mesh manager to call get_attribute.
        //
        // We skip the first two as those are always the joint* attributes.
        for (attribute_id, original_range) in &source_attribute_ranges[2..] {
            let skeleton_range = mesh_manager.allocate_range(device, original_range.end - original_range.start)?;
            overridden_attribute_ranges.push((*attribute_id, skeleton_range));
        }

        // Ensure there will be exactly `num_joints` matrices.
        let mut joint_matrices = skeleton.joint_matrices;
        joint_matrices.truncate(required_joint_count as _);

        Ok(InternalSkeleton {
            joint_matrices,
            mesh_handle: skeleton.mesh,
            source_attribute_ranges,
            overridden_attribute_ranges,
            vertex_count: internal_mesh.vertex_count,
        })
    }

    pub fn add(&mut self, handle: RawSkeletonHandle, internal: InternalSkeleton) {
        self.global_joint_count += internal.joint_matrices.len();

        if handle.idx >= self.data.len() {
            self.data.resize_with(handle.idx + 1, || None);
        }
        self.data[handle.idx] = Some(internal);

        self.skeleton_count += 1;
    }

    pub fn remove(&mut self, mesh_manager: &MeshManager, handle: &RawSkeletonHandle) {
        let skeleton = self.data[handle.idx].take().unwrap();
        self.global_joint_count -= skeleton.joint_matrices.len();

        // Free the owned regions of the mesh data buffer
        for (_, range) in skeleton.overridden_attribute_ranges {
            mesh_manager.free_range(range);
        }

        self.skeleton_count -= 1;
    }

    pub fn set_joint_matrices(&mut self, handle: &RawSkeletonHandle, mut joint_matrices: Vec<Mat4>) {
        let skeleton = self.data[handle.idx].as_mut().unwrap();
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
        self.data[handle.idx].as_ref().unwrap()
    }

    pub fn skeletons(&self) -> impl ExactSizeIterator<Item = &InternalSkeleton> {
        ExactSizerIterator::new(self.data.iter().filter_map(Option::as_ref), self.skeleton_count)
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
