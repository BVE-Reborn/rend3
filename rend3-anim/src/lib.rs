//! Utility library to play gltf animations.
//!
//! This library is meant to be used together with `rend3` and `rend3-gltf` and
//! allows posing meshes according to the animation data stored in a gltf file.
//!
//! In order to play animations, you need to:
//! - Create an [`AnimationData`] once when spawning your scene and store it.
//! - Each simulation frame, use [`pose_animation_frame`] to set the mesh's
//!   joints to a specific animation at a specific time.
//!
//! For now, this library aims to be a simple utility abstraction. Updating the
//! current state of the animation by changing the currently played animation or
//! increasing the playback time should be handled in user code.

use std::collections::HashMap;

use itertools::Itertools;
use rend3::{
    types::{
        glam::{Mat4, Quat, Vec3},
        SkeletonHandle,
    },
    util::typedefs::{FastHashMap, FastHashSet},
    Renderer,
};
use rend3_gltf::{AnimationChannel, GltfSceneInstance, LoadedGltfScene};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct AnimationIndex(pub usize);
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SkinIndex(pub usize);
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NodeIndex(pub usize);
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct JointIndex(pub usize);

/// Cached data structures per each of the Skins in a gltf model. This struct is
/// part of [`AnimationData`]
pub struct PerSkinData {
    /// Translates node indices to joint indices for this particular skin. This
    /// translation is necessary because an animation may be animating a scene
    /// node which is present in multiple skins.
    pub node_to_joint_idx: FastHashMap<NodeIndex, JointIndex>,
    /// Stores the node indices for this skin's joints in topological order.
    /// This is used to avoid iterating all the scene hierarchy when
    /// computing global positions for a node.
    pub joint_nodes_topological_order: Vec<NodeIndex>,
    /// The list of skeletons deformed by this animation. There's one skeleton
    /// for each of the mesh primitives. Every skeleton in this list is
    /// shares the same bone structure.
    pub skeletons: Vec<SkeletonHandle>,
}

/// Caches animation data necessary to run [`pose_animation_frame`].
pub struct AnimationData {
    /// For each skin, stores several cached data structures that speed up the
    /// animation loop at runtime.
    pub skin_data: FastHashMap<SkinIndex, PerSkinData>,
    /// For each animation, stores the list of skins it affects. An animation
    /// affects a skin if it deforms any of its joints. This is used to avoid
    /// iterating unaffected skins when playing an animation.
    pub animation_skin_usage: FastHashMap<AnimationIndex, Vec<SkinIndex>>,
}

impl AnimationData {
    /// Creates an [`AnimationData`] from a loaded gltf scene and instance.
    ///
    /// Note that the instance is necessary, as one `AnimationData` must exist
    /// per each instance of the same scene.
    ///
    /// ## Parameters
    /// - scene: The loaded scene, as returned by
    ///   [`load_gltf`](rend3_gltf::load_gltf) or
    ///   [`load_gltf_data`](rend3_gltf::load_gltf_data)
    /// - instance: An instance of `scene`, as returned by
    ///   [`load_gltf`](rend3_gltf::load_gltf) or
    ///   [`instance_loaded_scene`](rend3_gltf::instance_loaded_scene)
    pub fn from_gltf_scene(scene: &LoadedGltfScene, instance: &GltfSceneInstance) -> Self {
        // The set of joints that each animation affects, stored as node indices
        // NOTE: Uses a std HashMap because `GroupingMap::collect()` is
        // hardcoded to return that.
        let animation_to_joint_nodes: HashMap<AnimationIndex, FastHashSet<NodeIndex>> = scene
            .animations
            .iter()
            .enumerate()
            .flat_map(|(anim_idx, anim)| {
                anim.inner
                    .channels
                    .keys()
                    .map(move |node_idx| (AnimationIndex(anim_idx), NodeIndex(*node_idx)))
            })
            .into_grouping_map()
            .collect::<FastHashSet<_>>();

        let mut animation_skin_usage = FastHashMap::<AnimationIndex, Vec<SkinIndex>>::default();
        for animation_idx in 0..scene.animations.len() {
            let animation_idx = AnimationIndex(animation_idx);
            for (skin_index, skin) in scene.skins.iter().enumerate() {
                let skin_index = SkinIndex(skin_index);

                let anim_affected_nodes = &animation_to_joint_nodes[&animation_idx];
                if skin
                    .inner
                    .joints
                    .iter()
                    .any(|j| anim_affected_nodes.contains(&NodeIndex(j.inner.node_idx)))
                {
                    let entry = animation_skin_usage
                        .entry(animation_idx)
                        .or_insert_with(Default::default);
                    entry.push(skin_index);
                }
            }
        }

        let mut skin_data = FastHashMap::default();
        for (skin_index, skin) in scene.skins.iter().enumerate() {
            let skin_index = SkinIndex(skin_index);

            let node_to_joint_idx = skin
                .inner
                .joints
                .iter()
                .enumerate()
                .map(|(idx, joint)| (NodeIndex(joint.inner.node_idx), JointIndex(idx)))
                .collect();

            // Nodes affected by this skin (i.e. joints)
            let skin_nodes: Vec<NodeIndex> = skin.inner.joints.iter().map(|j| NodeIndex(j.inner.node_idx)).collect();

            let joint_nodes_topological_order: Vec<NodeIndex> = instance
                .topological_order
                .iter()
                .map(|node_idx| NodeIndex(*node_idx))
                .filter(|node_idx| skin_nodes.contains(node_idx))
                .collect();

            let skeletons: Vec<SkeletonHandle> = instance
                .nodes
                .iter()
                .flat_map(|node| &node.inner.object)
                .flat_map(|object| &object.inner.armature)
                .filter(|armature| armature.skin_index == skin_index.0)
                .flat_map(|armature| &armature.skeletons)
                .cloned()
                .collect();

            skin_data.insert(
                skin_index,
                PerSkinData {
                    node_to_joint_idx,
                    joint_nodes_topological_order,
                    skeletons,
                },
            );
        }

        AnimationData {
            skin_data,
            animation_skin_usage,
        }
    }
}

/// Helper trait that exposes a generic `lerp` function for various `glam` types
pub trait Lerp {
    fn lerp(self, other: Self, t: f32) -> Self;
}
impl Lerp for Vec3 {
    fn lerp(self, other: Self, t: f32) -> Self {
        self.lerp(other, t)
    }
}
impl Lerp for Quat {
    fn lerp(self, other: Self, t: f32) -> Self {
        // Uses Normalized Linear Interpolation (a.k.a. nlerp) as slerp replacement
        // See: *"Understanding Slerp, Then Not Using It"*
        // http://number-none.com/product/Understanding%20Slerp,%20Then%20Not%20Using%20It/
        self.lerp(other, t).normalize()
    }
}

/// Samples the data value for an animation channel at a given time. Will
/// interpolate between the two closest keyframes.
fn sample_at_time<T: Lerp + Copy>(channel: &AnimationChannel<T>, current_time: f32) -> T {
    let next_idx = channel
        .times
        .iter()
        .position(|time| *time > current_time)
        .unwrap_or(channel.times.len() - 1);
    let prev_idx = next_idx.saturating_sub(1);

    let interp_factor = f32::clamp(
        (current_time - channel.times[prev_idx]) / (channel.times[next_idx] - channel.times[prev_idx]),
        0.0,
        1.0,
    );

    channel.values[prev_idx].lerp(channel.values[next_idx], interp_factor)
}

/// Sets the pose of the meshes at the given scene by using the animation at
/// index `animation_index` at a given `time`. The provided time gets clamped to
/// the valid range of times for the selected animation.
pub fn pose_animation_frame(
    renderer: &Renderer,
    scene: &LoadedGltfScene,
    instance: &GltfSceneInstance,
    animation_data: &AnimationData,
    animation_index: usize,
    time: f32,
) {
    let animation = &scene.animations[animation_index];
    let time = time.clamp(0.0, animation.inner.duration);

    for (skin_index, per_skin_data) in &animation_data.skin_data {
        let skin = &scene.skins[skin_index.0];
        let inv_bind_mats = &skin.inner.inverse_bind_matrices;

        // The local position of each joint, relative to its parent
        let mut joint_local_matrices = vec![Mat4::IDENTITY; inv_bind_mats.len()];

        let node_to_joint_idx = &per_skin_data.node_to_joint_idx;

        // Compute each bone's local transformation
        for (&node_idx, channels) in &animation.inner.channels {
            // NOTE: If a channel's property is not present, we need to set the
            // joint at its bind pose for that individual property
            let local_transform = instance.nodes[node_idx].inner.local_transform;
            let (bind_scale, bind_rotation, bind_translation) = local_transform.to_scale_rotation_translation();

            let translation = channels
                .translation
                .as_ref()
                .map(|tra| sample_at_time(tra, time))
                .unwrap_or(bind_translation);
            let rotation = channels
                .rotation
                .as_ref()
                .map(|rot| sample_at_time(rot, time))
                .unwrap_or(bind_rotation);
            let scale = channels
                .scale
                .as_ref()
                .map(|sca| sample_at_time(sca, time))
                .unwrap_or(bind_scale);

            let matrix = Mat4::from_scale_rotation_translation(scale, rotation, translation);
            let joint_idx = node_to_joint_idx[&NodeIndex(node_idx)];
            joint_local_matrices[joint_idx.0] = matrix;
        }

        let mut global_joint_transforms = vec![Mat4::IDENTITY; inv_bind_mats.len()];

        // Compute bone global transformations
        for node_idx in &per_skin_data.joint_nodes_topological_order {
            let node = &instance.nodes[node_idx.0].inner;
            let joint_idx = node_to_joint_idx[node_idx];
            if let Some(parent_joint_idx) = node.parent.map(|pi| node_to_joint_idx.get(&NodeIndex(pi))) {
                // This is guaranteed to be computed because we're iterating
                // the hierarchy nodes in topological order
                let parent_transform = parent_joint_idx
                    .map(|p| global_joint_transforms[p.0])
                    .unwrap_or(Mat4::IDENTITY);
                let current_transform = joint_local_matrices[joint_idx.0];

                global_joint_transforms[joint_idx.0] = parent_transform * current_transform;
            } else {
                global_joint_transforms[joint_idx.0] = joint_local_matrices[joint_idx.0];
            }
        }

        // Set the joint positions in rend3
        for skeleton in &per_skin_data.skeletons {
            renderer.set_skeleton_joint_transforms(skeleton, &global_joint_transforms, inv_bind_mats);
        }
    }
}
