//! gltf scene and model loader for rend3.
//!
//! This crate attempts to map the concepts into gltf as best it can into rend3,
//! but there is quite a variety of things that would be insane to properly
//! represent.
//!
//! To "just load a gltf/glb", look at the documentation for [`load_gltf`] and
//! use the default [`filesystem_io_func`].
//!
//! Individual components of a gltf can be loaded with the other functions in
//! this crate.
//!
//! # Supported Extensions
//! - `KHR_punctual_lights`
//! - `KHR_texture_transform`
//! - `KHR_material_unlit`
//!
//! # Known Limitations
//! - Only the albedo texture's transform from `KHR_texture_transform` will be
//!   used.
//! - Double sided materials are currently unsupported.

use std::{
    borrow::Cow,
    collections::{hash_map::Entry, BTreeMap, HashMap, VecDeque},
    future::Future,
    path::Path,
    sync::Arc,
};

use base64::Engine;
use glam::{Mat3, Mat4, Quat, UVec2, Vec2, Vec3, Vec4};
use gltf::buffer::Source;
use rend3::{
    types::{self, Handedness, MeshValidationError, ObjectHandle, ObjectMeshKind, Skeleton, SkeletonHandle},
    util::typedefs::{FastHashMap, SsoString},
    Renderer,
};
use rend3_routine::pbr;
use thiserror::Error;

/// Wrapper around a T that stores an optional label.
#[derive(Debug, Clone)]
pub struct Labeled<T> {
    /// Inner value
    pub inner: T,
    /// Label associated with the T
    pub label: Option<SsoString>,
}
impl<T> Labeled<T> {
    /// Create a new
    pub fn new(inner: T, label: Option<&str>) -> Self {
        Self { inner, label: label.map(SsoString::from) }
    }
}

/// A single sub-mesh of a gltf.
#[derive(Debug)]
pub struct MeshPrimitive {
    pub handle: types::MeshHandle,
    /// Index into the material vector given by [`load_materials_and_textures`]
    /// or [`LoadedGltfScene::materials`].
    pub material: Option<usize>,
}

/// Set of [`MeshPrimitive`]s that make up a logical mesh.
#[derive(Debug)]
pub struct Mesh {
    pub primitives: Vec<MeshPrimitive>,
}

/// A set of [`SkeletonHandle`]s, one per mesh in the wrapping object, plus the
/// index of the skin data in the `skins` array of [`LoadedGltfScene`].
///
/// All the skeletons are guaranteed to have the same number of joints and
/// structure, but deform different meshes of the object.
#[derive(Debug, Clone)]
pub struct Armature {
    /// The list of skeletons that are deforming this node's mesh primitives.
    pub skeletons: Vec<SkeletonHandle>,
    /// Index to the skin that contains the inverse bind matrices for the
    /// skeletons in this armature.
    pub skin_index: usize,
}

/// Set of [`ObjectHandle`]s that correspond to a logical object in the node
/// tree. When the node corresponds to an animated mesh, the `armature` will
/// contain the necessary data to deform the primitives.
///
/// This is to a [`ObjectHandle`], as a [`Mesh`] is to a [`MeshPrimitive`].
#[derive(Debug, Clone)]
pub struct Object {
    pub primitives: Vec<ObjectHandle>,
    pub armature: Option<Armature>,
}

/// Node in the gltf scene tree
#[derive(Debug, Default, Clone)]
pub struct Node {
    /// The index of the parent node in the nodes array, if any.
    pub parent: Option<usize>,
    /// The index of the children nodes in the nodes array
    pub children: Vec<usize>,
    /// Transform of this node relative to its parents.
    pub local_transform: Mat4,
    /// Object for this node.
    pub object: Option<Labeled<Object>>,
    /// Directional light for this node.
    pub directional_light: Option<types::DirectionalLightHandle>,
}

/// Hashmap key for caching images.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct ImageKey {
    /// Index into the image array.
    pub index: usize,
    /// If the image should be viewed as srgb or not.
    pub srgb: bool,
}

/// A uploaded texture and its format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texture {
    pub handle: types::Texture2DHandle,
    pub format: types::TextureFormat,
}

#[derive(Debug)]
pub struct Joint {
    pub node_idx: usize,
}

#[derive(Debug)]
pub struct Skin {
    pub inverse_bind_matrices: Vec<Mat4>,
    pub joints: Vec<Labeled<Joint>>,
}

#[derive(Debug)]
pub struct AnimationChannel<T> {
    pub values: Vec<T>,
    pub times: Vec<f32>,
}

/// Animation data for a single joint, with translation, rotation and scale
/// channels.
#[derive(Debug)]
pub struct PosRotScale {
    pub node_idx: u32,
    pub translation: Option<AnimationChannel<Vec3>>,
    pub rotation: Option<AnimationChannel<Quat>>,
    pub scale: Option<AnimationChannel<Vec3>>,
}

impl PosRotScale {
    pub fn new(node_idx: u32) -> Self {
        Self { node_idx, translation: None, rotation: None, scale: None }
    }
}

#[derive(Debug)]
pub struct Animation {
    /// Maps the node index of a joint to its animation keyframe data.
    pub channels: HashMap<usize, PosRotScale>,
    /// The total duration of the animation. Computed as the maximum time of any
    /// keyframe on any channel.
    pub duration: f32,
}

/// Hashmap which stores a mapping from [`ImageKey`] to a labeled handle.
pub type ImageMap = FastHashMap<ImageKey, Labeled<Texture>>;

/// Loaded data on a gltf scene that can be reused across multiple instances of
/// the same set of objects.
#[derive(Debug)]
pub struct LoadedGltfScene {
    pub meshes: Vec<Labeled<Mesh>>,
    pub materials: Vec<Labeled<types::MaterialHandle>>,
    pub default_material: types::MaterialHandle,
    pub images: ImageMap,
    pub skins: Vec<Labeled<Skin>>,
    pub animations: Vec<Labeled<Animation>>,
}

/// Data specific to each instance of a gltf scene.
pub struct GltfSceneInstance {
    /// The flat list of nodes in the scene. Each node points to a list of
    /// children and optionally a parent using indices.
    pub nodes: Vec<Labeled<Node>>,
    /// Iterating the `nodes` following the order in this list guarantees that
    /// parents will always be visited before children. This allows avoiding
    /// recursion in several algorithms.
    pub topological_order: Vec<usize>,
}

/// Describes how loading gltf failed.
#[derive(Debug, Error)]
pub enum GltfLoadError<E: std::error::Error + 'static> {
    #[error("Gltf parsing or validation error")]
    Gltf(#[from] gltf::Error),
    #[error("Buffer {0} failed to be loaded from the fs")]
    BufferIo(SsoString, #[source] E),
    #[error("Texture {0} failed to be loaded from the fs")]
    TextureIo(SsoString, #[source] E),
    #[error("Texture {0} failed to be loaded as an image")]
    TextureDecode(SsoString, #[source] image::ImageError),
    #[cfg(feature = "ddsfile")]
    #[error("Texture {0} failed to be loaded as a ddsfile due to incompatible dxgi format {1:?}")]
    TextureBadDxgiFormat(SsoString, ddsfile::DxgiFormat),
    #[cfg(feature = "ddsfile")]
    #[error("Texture {0} failed to be loaded as a ddsfile due to incompatible d3d format {1:?}")]
    TextureBadD3DFormat(SsoString, ddsfile::D3DFormat),
    #[cfg(feature = "ktx2")]
    #[error("Texture {0} failed to be loaded as a ktx2 file due to incompatible format {1:?}")]
    TextureBadKxt2Format(SsoString, ktx2::Format),
    #[error("Texture {0} failed to be loaded as it has 0 levels")]
    TextureZeroLevels(SsoString),
    #[error("Texture {0} failed to be loaded as it has 0 layers")]
    TextureTooManyLayers(SsoString),
    #[error("Rend3-gltf expects gltf files to have a single scene.")]
    GltfSingleSceneOnly,
    #[error("Mesh {0} does not have positions")]
    MissingPositions(usize),
    #[error("Gltf file references mesh {0} but mesh does not exist")]
    MissingMesh(usize),
    #[error("Gltf file references skin {0} but skin does not exist")]
    MissingSkin(usize),
    #[error("Gltf file references material {0} but material does not exist")]
    MissingMaterial(usize),
    #[error("Mesh {0} primitive {1} uses unsupported mode {2:?}. Only triangles are supported")]
    UnsupportedPrimitiveMode(usize, usize, gltf::mesh::Mode),
    #[error("Mesh {0} failed validation")]
    MeshValidationError(usize, #[source] MeshValidationError),
    #[error("Animation {0} channel {1} does not have keyframe times.")]
    MissingKeyframeTimes(usize, usize),
    #[error("Animation {0} channel {1} does not have keyframe values.")]
    MissingKeyframeValues(usize, usize),
    #[error("Failed to create a mesh")]
    MeshCreation(#[from] rend3::managers::MeshCreationError),
    #[error("Failed to create a skeleton")]
    SkeletonCreation(#[from] rend3::managers::SkeletonCreationError),
    #[error("Failed to create a texture")]
    TextureCreation(#[from] rend3::managers::TextureCreationError),
}

unsafe impl<E: std::error::Error + 'static> Send for GltfLoadError<E> {}
unsafe impl<E: std::error::Error + 'static> Sync for GltfLoadError<E> {}

/// Tries to decode data as base64, failing if the prefix doesn't match.
pub fn try_load_base64(uri: &str) -> Option<Vec<u8>> {
    let octet_stream_header = "data:";

    let base64_data = uri.strip_prefix(octet_stream_header)?;

    let (_mime, rest) = base64_data.split_once(';').unwrap();
    let (encoding, data) = rest.split_once(',').unwrap();
    if encoding != "base64" {
        return None;
    }

    log::info!("loading {} bytes of base64 data", data.len());
    profiling::scope!("decoding base64 uri");
    // TODO: errors
    Some(base64::prelude::BASE64_STANDARD.decode(data).unwrap())
}

/// Default implementation of [`load_gltf`]'s `io_func` that loads from the
/// filesystem relative to the gltf.
///
/// The first argumnet is the directory all relative paths should be considered
/// against. This is more than likely the directory the gltf/glb is in.
pub async fn filesystem_io_func(parent_directory: impl AsRef<Path>, uri: &str) -> Result<Vec<u8>, std::io::Error> {
    if let Some(data) = try_load_base64(uri) {
        return Ok(data);
    }

    let path_resolved = parent_directory.as_ref().join(uri);
    let display = path_resolved.as_os_str().to_string_lossy();
    // profiling::scope!("loading file", &display);
    log::info!("loading file '{}' from disk", &display);
    std::fs::read(path_resolved)
}

/// Determines parameters that are given to various parts of the gltf world that
/// cannot be specified by gltf alone.
#[derive(Copy, Clone)]
pub struct GltfLoadSettings {
    /// Global scale applied to all objects (default: 1)
    pub scale: f32,
    /// Size of the shadow map in world space (default: 100)
    pub directional_light_shadow_distance: f32,
    /// Resolution of the shadow map (default: 2048)
    pub directional_light_resolution: u16,
    /// Coordinate space normal maps should use (default Up)
    pub normal_direction: pbr::NormalTextureYDirection,
    /// Enable built-in directional lights (default true)
    pub enable_directional: bool,
}

impl Default for GltfLoadSettings {
    fn default() -> Self {
        Self {
            scale: 1.0,
            directional_light_shadow_distance: 100.0,
            directional_light_resolution: 2048,
            normal_direction: pbr::NormalTextureYDirection::Up,
            enable_directional: true,
        }
    }
}

/// Load a given gltf into the renderer's world.
///
/// Allows the user to specify how URIs are resolved into their underlying data.
/// Supports most gltfs and glbs.
///
/// **Must** keep the [`LoadedGltfScene`] alive for the scene to remain.
///
/// See [`load_gltf_data`] and [`instance_loaded_scene`] if you need more
/// fine-grained control about how and when the scene data is instanced.
///
/// ```no_run
/// # use std::path::Path;
/// # let renderer = unimplemented!();
/// let path = Path::new("some/path/scene.gltf"); // or glb
/// let gltf_data = std::fs::read(&path).unwrap();
/// let parent_directory = path.parent().unwrap();
/// let _loaded = pollster::block_on(rend3_gltf::load_gltf(
///     &renderer,
///     &gltf_data,
///     &rend3_gltf::GltfLoadSettings::default(),
///     |p| async move { rend3_gltf::filesystem_io_func(&parent_directory, &p).await }
/// ));
/// ```
pub async fn load_gltf<F, Fut, E>(
    renderer: &Arc<Renderer>,
    data: &[u8],
    settings: &GltfLoadSettings,
    io_func: F,
) -> Result<(LoadedGltfScene, GltfSceneInstance), GltfLoadError<E>>
where
    F: FnMut(SsoString) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, E>>,
    E: std::error::Error + 'static,
{
    // profiling::scope!("loading gltf");

    let mut file = {
        // profiling::scope!("parsing gltf");
        gltf::Gltf::from_slice_without_validation(data)?
    };

    let loaded = load_gltf_data(renderer, &mut file, settings, io_func).await?;

    if file.scenes().len() != 1 {
        return Err(GltfLoadError::GltfSingleSceneOnly);
    }

    let instance = instance_loaded_scene(
        renderer,
        &loaded,
        file.nodes().collect(),
        settings,
        Mat4::from_scale(Vec3::new(
            settings.scale,
            settings.scale,
            if renderer.handedness == Handedness::Left { -settings.scale } else { settings.scale },
        )),
    )?;

    Ok((loaded, instance))
}

/// Load a given gltf's data, like meshes and materials, without yet adding
/// any of the nodes to the scene.
///
/// Allows the user to specify how URIs are resolved into their underlying data.
/// Supports most gltfs and glbs.
///
/// **Must** keep the [`LoadedGltfScene`] alive for the meshes and materials
pub async fn load_gltf_data<F, Fut, E>(
    renderer: &Arc<Renderer>,
    file: &mut gltf::Gltf,
    settings: &GltfLoadSettings,
    mut io_func: F,
) -> Result<LoadedGltfScene, GltfLoadError<E>>
where
    F: FnMut(SsoString) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, E>>,
    E: std::error::Error + 'static,
{
    // profiling::scope!("loading gltf data");
    let blob = file.blob.take();

    let buffers = load_buffers(file.buffers(), blob, &mut io_func).await?;

    let default_material = load_default_material(renderer);
    let meshes = load_meshes(renderer, file.meshes(), &buffers)?;
    let (materials, images) =
        load_materials_and_textures(renderer, file.materials(), &buffers, settings, &mut io_func).await?;
    let skins = load_skins(file.skins(), &buffers)?;
    let animations = load_animations(file.animations(), &buffers)?;

    let loaded = LoadedGltfScene { meshes, materials, default_material, images, skins, animations };

    Ok(loaded)
}

/// Adds a single mesh from the [`LoadedGltfScene`] found by its index,
/// as an object to the scene.
pub fn add_mesh_by_index<E: std::error::Error + 'static>(
    renderer: &Arc<Renderer>,
    loaded: &LoadedGltfScene,
    mesh_index: usize,
    name: Option<&str>,
    skin_index: Option<usize>,
    transform: Mat4,
) -> Result<Labeled<Object>, GltfLoadError<E>> {
    let mesh_handle = loaded.meshes.get(mesh_index).ok_or(GltfLoadError::MissingMesh(mesh_index))?;

    let mut primitives = Vec::new();
    let mut skeletons = Vec::new();

    let skin = if let Some(skin_index) = skin_index {
        let skin = loaded.skins.get(skin_index).ok_or(GltfLoadError::MissingSkin(skin_index))?;
        Some(skin)
    } else {
        None
    };

    for prim in &mesh_handle.inner.primitives {
        let mat_idx = prim.material;
        let mat = mat_idx
            .map_or_else(|| Some(&loaded.default_material), |mat_idx| loaded.materials.get(mat_idx).map(|m| &m.inner))
            .ok_or_else(|| GltfLoadError::MissingMaterial(mat_idx.expect("Could not find default material")))?;

        let mesh_kind = if let Some(skin) = skin {
            let skeleton = renderer.add_skeleton(Skeleton {
                // We don't need to use the inverse bind matrices. At rest pose, every
                // joint matrix is inv_bind_pose * bind_pose, thus the identity matrix.
                joint_matrices: vec![Mat4::IDENTITY; skin.inner.inverse_bind_matrices.len()],
                mesh: prim.handle.clone(),
            })?;
            skeletons.push(skeleton.clone());
            ObjectMeshKind::Animated(skeleton)
        } else {
            ObjectMeshKind::Static(prim.handle.clone())
        };

        primitives.push(renderer.add_object(types::Object { mesh_kind, material: mat.clone(), transform }));
    }

    Ok(Labeled::new(
        Object { primitives, armature: skin_index.map(|skin_index| Armature { skeletons, skin_index }) },
        name,
    ))
}

/// Computes topological ordering and children->parent map.
fn node_indices_topological_sort(nodes: &[gltf::Node]) -> (Vec<usize>, BTreeMap<usize, usize>) {
    // NOTE: The algorithm uses BTreeMaps to guarantee consistent ordering.

    // Maps parent to list of children
    let mut children = BTreeMap::<usize, Vec<usize>>::new();
    for node in nodes {
        children.insert(node.index(), node.children().map(|n| n.index()).collect());
    }

    // Maps child to parent
    let parents: BTreeMap<usize, usize> =
        children.iter().flat_map(|(parent, children)| children.iter().map(|ch| (*ch, *parent))).collect();

    // Initialize the BFS queue with nodes that don't have any parent (i.e. roots)
    let mut queue: VecDeque<usize> = children.keys().filter(|n| parents.get(n).is_none()).cloned().collect();

    let mut topological_sort = Vec::<usize>::new();

    while let Some(n) = queue.pop_front() {
        topological_sort.push(n);
        for ch in &children[&n] {
            queue.push_back(*ch);
        }
    }

    (topological_sort, parents)
}

/// Instances a Gltf scene that has been loaded using [`load_gltf_data`]. Will
/// create as many [`Object`]s as required.
///
/// You need to hold onto the returned value from this function to make sure the
/// objects don't get deleted.
pub fn instance_loaded_scene<E: std::error::Error + 'static>(
    renderer: &Arc<Renderer>,
    loaded: &LoadedGltfScene,
    nodes: Vec<gltf::Node<'_>>,
    settings: &GltfLoadSettings,
    parent_transform: Mat4,
) -> Result<GltfSceneInstance, GltfLoadError<E>> {
    let (topological_order, parents) = node_indices_topological_sort(&nodes);

    let num_nodes = nodes.len();

    debug_assert_eq!(topological_order.len(), num_nodes);

    let mut node_transforms = vec![Mat4::IDENTITY; num_nodes];

    let mut final_nodes = vec![Labeled::new(Node::default(), None); nodes.len()];
    for node_idx in topological_order.iter() {
        let node = &nodes[*node_idx];

        let local_transform = Mat4::from_cols_array_2d(&node.transform().matrix());
        let parent_transform = parents.get(&node.index()).map(|p| node_transforms[*p]).unwrap_or(parent_transform);
        let transform = parent_transform * local_transform;
        node_transforms[*node_idx] = transform;

        let object = if let Some(mesh) = node.mesh() {
            Some(add_mesh_by_index(
                renderer,
                loaded,
                mesh.index(),
                mesh.name(),
                node.skin().map(|s| s.index()),
                transform,
            )?)
        } else {
            None
        };

        let light = if let Some(light) = node.light() {
            match light.kind() {
                gltf::khr_lights_punctual::Kind::Directional if settings.enable_directional => {
                    let direction = transform.transform_vector3(-Vec3::Z);
                    Some(renderer.add_directional_light(types::DirectionalLight {
                        color: Vec3::from(light.color()),
                        intensity: light.intensity(),
                        direction,
                        distance: settings.directional_light_shadow_distance,
                        resolution: settings.directional_light_resolution,
                    }))
                }
                _ => None,
            }
        } else {
            None
        };

        let children = node.children().map(|node| node.index()).collect();

        final_nodes[*node_idx] = Labeled::new(
            Node {
                parent: parents.get(&node.index()).cloned(),
                children,
                local_transform,
                object,
                directional_light: light,
            },
            node.name(),
        )
    }
    Ok(GltfSceneInstance { nodes: final_nodes, topological_order })
}

/// Loads buffers from a [`gltf::Buffer`] iterator, calling io_func to resolve
/// them from URI.
///
/// If the gltf came from a .glb, the glb's blob should be provided.
///
/// # Panics
///
/// Panics if buffers requires a blob but no blob was given.
pub async fn load_buffers<F, Fut, E>(
    file: impl ExactSizeIterator<Item = gltf::Buffer<'_>>,
    blob: Option<Vec<u8>>,
    mut io_func: F,
) -> Result<Vec<Vec<u8>>, GltfLoadError<E>>
where
    F: FnMut(SsoString) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, E>>,
    E: std::error::Error + 'static,
{
    // profiling::scope!("loading buffers");
    let mut buffers = Vec::with_capacity(file.len());
    let mut blob_index = None;
    for b in file {
        let data = match b.source() {
            Source::Bin => {
                blob_index = Some(b.index());
                Vec::new()
            }
            Source::Uri(uri) => {
                io_func(SsoString::from(uri)).await.map_err(|e| GltfLoadError::BufferIo(SsoString::from(uri), e))?
            }
        };
        buffers.push(data);
    }
    if let Some(blob_index) = blob_index {
        buffers[blob_index] = blob.expect("glb blob not found, but gltf expected it");
    }
    Ok(buffers)
}

/// Loads meshes from a [`gltf::Mesh`] iterator.
///
/// All binary data buffers must be provided. Call this with
/// [`gltf::Document::meshes`] as the mesh argument.
pub fn load_meshes<'a, E: std::error::Error + 'static>(
    renderer: &Arc<Renderer>,
    meshes: impl Iterator<Item = gltf::Mesh<'a>>,
    buffers: &[Vec<u8>],
) -> Result<Vec<Labeled<Mesh>>, GltfLoadError<E>> {
    profiling::scope!("loading meshes");
    meshes
        .into_iter()
        .map(|mesh| {
            let mut res_prims = Vec::new();
            for prim in mesh.primitives() {
                if prim.mode() != gltf::mesh::Mode::Triangles {
                    return Err(GltfLoadError::UnsupportedPrimitiveMode(mesh.index(), prim.index(), prim.mode()));
                }

                let reader = prim.reader(|b| Some(&buffers[b.index()][..b.length()]));

                let vertex_positions: Vec<_> = reader
                    .read_positions()
                    .ok_or_else(|| GltfLoadError::MissingPositions(mesh.index()))?
                    .map(Vec3::from)
                    .collect();

                // glTF models are right handed, so we must flip their winding order
                let mut builder = types::MeshBuilder::new(vertex_positions, renderer.handedness);
                if renderer.handedness == Handedness::Left {
                    builder = builder.with_flip_winding_order();
                }

                if let Some(normals) = reader.read_normals() {
                    builder = builder.with_vertex_normals(normals.map(Vec3::from).collect())
                }

                if let Some(tangents) = reader.read_tangents() {
                    // todo: handedness
                    builder = builder.with_vertex_tangents(tangents.map(|[x, y, z, _]| Vec3::new(x, y, z)).collect())
                }

                if let Some(uvs) = reader.read_tex_coords(0) {
                    builder = builder.with_vertex_texture_coordinates_0(uvs.into_f32().map(Vec2::from).collect())
                }

                if let Some(uvs) = reader.read_tex_coords(1) {
                    builder = builder.with_vertex_texture_coordinates_1(uvs.into_f32().map(Vec2::from).collect())
                }

                if let Some(colors) = reader.read_colors(0) {
                    builder = builder.with_vertex_color_0(colors.into_rgba_u8().collect())
                }

                if let Some(indices) = reader.read_indices() {
                    builder = builder.with_indices(indices.into_u32().collect())
                }

                if let Some(joint_indices) = reader.read_joints(0) {
                    builder = builder.with_vertex_joint_indices(joint_indices.into_u16().collect())
                }

                if let Some(joint_weights) = reader.read_weights(0) {
                    builder = builder.with_vertex_joint_weights(joint_weights.into_f32().map(Vec4::from).collect())
                }

                let mesh = builder.build().map_err(|valid| GltfLoadError::MeshValidationError(mesh.index(), valid))?;

                let handle = renderer.add_mesh(mesh)?;

                res_prims.push(MeshPrimitive { handle, material: prim.material().index() })
            }
            Ok(Labeled::new(Mesh { primitives: res_prims }, mesh.name()))
        })
        .collect()
}

fn load_skins<E: std::error::Error + 'static>(
    skins: gltf::iter::Skins,
    buffers: &[Vec<u8>],
) -> Result<Vec<Labeled<Skin>>, GltfLoadError<E>> {
    let mut res_skins = vec![];

    for skin in skins {
        let num_joints = skin.joints().count();
        let reader = skin.reader(|b| Some(&buffers[b.index()][..b.length()]));

        let inv_b_mats = if let Some(inv_b_mats) = reader.read_inverse_bind_matrices() {
            inv_b_mats.map(|mat| Mat4::from_cols_array_2d(&mat)).collect()
        } else {
            // The inverse bind matrices are sometimes not provided. This has to
            // be interpreted as all of them being the identity transform.
            vec![Mat4::IDENTITY; num_joints]
        };

        let joints = skin.joints().map(|node| Labeled::new(Joint { node_idx: node.index() }, node.name())).collect();

        res_skins.push(Labeled::new(Skin { inverse_bind_matrices: inv_b_mats, joints }, skin.name()))
    }

    Ok(res_skins)
}

fn compute_animation_duration(channels: &HashMap<usize, PosRotScale>) -> f32 {
    fn channel_duration<T>(channel: &AnimationChannel<T>) -> f32 {
        channel.times.iter().copied().map(float_ord::FloatOrd).max().map(|f_ord| f_ord.0).unwrap_or(0.0)
    }
    channels
        .values()
        .map(|ch| {
            let m1 = ch.translation.as_ref().map(channel_duration).unwrap_or(0.0);
            let m2 = ch.rotation.as_ref().map(channel_duration).unwrap_or(0.0);
            let m3 = ch.scale.as_ref().map(channel_duration).unwrap_or(0.0);
            m1.max(m2).max(m3)
        })
        .map(float_ord::FloatOrd)
        .max()
        .map(|x| x.0)
        .unwrap_or(0.0)
}

fn load_animations<E: std::error::Error + 'static>(
    animations: gltf::iter::Animations,
    buffers: &[Vec<u8>],
) -> Result<Vec<Labeled<Animation>>, GltfLoadError<E>> {
    let mut result = Vec::new();
    for anim in animations {
        let mut result_channels = HashMap::<usize, PosRotScale>::new();

        for (ch_idx, ch) in anim.channels().enumerate() {
            let target = ch.target();
            let node_idx = target.node().index();

            // Get the PosRotScale for the current target node or create a new
            // one if it doesn't exist.
            let chs = result_channels.entry(node_idx).or_insert_with(|| PosRotScale::new(node_idx as u32));

            let reader = ch.reader(|b| Some(&buffers[b.index()][..b.length()]));

            // In gltf, 'inputs' refers to the keyframe times
            let times = reader
                .read_inputs()
                .ok_or_else(|| GltfLoadError::MissingKeyframeTimes(anim.index(), ch_idx))?
                .collect();

            // And 'outputs' means the keyframe values, which varies depending on the type
            // of keyframe
            match reader.read_outputs().ok_or_else(|| GltfLoadError::MissingKeyframeValues(anim.index(), ch_idx))? {
                gltf::animation::util::ReadOutputs::Translations(trs) => {
                    chs.translation = Some(AnimationChannel { values: trs.map(Vec3::from).collect(), times })
                }
                gltf::animation::util::ReadOutputs::Rotations(rots) => {
                    chs.rotation =
                        Some(AnimationChannel { values: rots.into_f32().map(Quat::from_array).collect(), times });
                }
                gltf::animation::util::ReadOutputs::Scales(scls) => {
                    chs.scale = Some(AnimationChannel { values: scls.map(Vec3::from).collect(), times });
                }
                gltf::animation::util::ReadOutputs::MorphTargetWeights(_) => {
                    // TODO
                }
            }
        }

        result.push(Labeled::new(
            Animation { duration: compute_animation_duration(&result_channels), channels: result_channels },
            anim.name(),
        ))
    }

    Ok(result)
}

/// Creates a gltf default material.
pub fn load_default_material(renderer: &Arc<Renderer>) -> types::MaterialHandle {
    profiling::scope!("creating default material");
    renderer.add_material(pbr::PbrMaterial {
        albedo: pbr::AlbedoComponent::Value(Vec4::splat(1.0)),
        transparency: pbr::Transparency::Opaque,
        normal: pbr::NormalTexture::None,
        aomr_textures: pbr::AoMRTextures::None,
        ao_factor: Some(1.0),
        metallic_factor: Some(1.0),
        roughness_factor: Some(1.0),
        clearcoat_textures: pbr::ClearcoatTextures::None,
        clearcoat_factor: Some(1.0),
        clearcoat_roughness_factor: Some(1.0),
        emissive: pbr::MaterialComponent::None,
        reflectance: pbr::MaterialComponent::None,
        anisotropy: pbr::MaterialComponent::None,
        uv_transform0: Mat3::IDENTITY,
        uv_transform1: Mat3::IDENTITY,
        unlit: false,
        sample_type: pbr::SampleType::Linear,
    })
}

/// Loads materials and textures from a [`gltf::Material`] iterator.
///
/// All binary data buffers must be provided. Call this with
/// [`gltf::Document::materials`] as the materials argument.
///
/// io_func determines how URIs are resolved into their underlying data.
pub async fn load_materials_and_textures<F, Fut, E>(
    renderer: &Arc<Renderer>,
    materials: impl ExactSizeIterator<Item = gltf::Material<'_>>,
    buffers: &[Vec<u8>],
    settings: &GltfLoadSettings,
    io_func: &mut F,
) -> Result<(Vec<Labeled<types::MaterialHandle>>, ImageMap), GltfLoadError<E>>
where
    F: FnMut(SsoString) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, E>>,
    E: std::error::Error + 'static,
{
    // profiling::scope!("loading materials and textures");

    let mut images = ImageMap::default();
    let mut result = Vec::with_capacity(materials.len());
    for material in materials {
        // profiling::scope!("load material", material.name().unwrap_or_default());

        let pbr = material.pbr_metallic_roughness();
        let albedo = pbr.base_color_texture();
        let albedo_factor = pbr.base_color_factor();
        let occlusion = material.occlusion_texture();
        let emissive = material.emissive_texture();
        let emissive_factor = material.emissive_factor();
        let normals = material.normal_texture();
        let roughness_factor = pbr.roughness_factor();
        let metallic_factor = pbr.metallic_factor();
        let metallic_roughness = pbr.metallic_roughness_texture();

        let nearest = albedo
            .as_ref()
            .map(|i| match i.texture().sampler().mag_filter() {
                Some(gltf::texture::MagFilter::Nearest) => pbr::SampleType::Nearest,
                Some(gltf::texture::MagFilter::Linear) => pbr::SampleType::Linear,
                None => pbr::SampleType::Linear,
            })
            .unwrap_or_default();

        let uv_transform = albedo
            .as_ref()
            .and_then(|i| {
                let transform = i.texture_transform()?;
                Some(Mat3::from_scale_angle_translation(
                    transform.scale().into(),
                    transform.rotation(),
                    transform.offset().into(),
                ))
            })
            .unwrap_or(Mat3::IDENTITY);

        let albedo_tex = util::texture_option_resolve(
            albedo.map(|i| load_image_cached(renderer, &mut images, i.texture().source(), true, buffers, io_func)),
        )
        .await?;
        let occlusion_tex = util::texture_option_resolve(
            occlusion.map(|i| load_image_cached(renderer, &mut images, i.texture().source(), false, buffers, io_func)),
        )
        .await?;
        let emissive_tex = util::texture_option_resolve(
            emissive.map(|i| load_image_cached(renderer, &mut images, i.texture().source(), true, buffers, io_func)),
        )
        .await?;
        let normals_tex = util::texture_option_resolve(
            normals.map(|i| load_image_cached(renderer, &mut images, i.texture().source(), false, buffers, io_func)),
        )
        .await?;
        let metallic_roughness_tex = util::texture_option_resolve(
            metallic_roughness
                .map(|i| load_image_cached(renderer, &mut images, i.texture().source(), false, buffers, io_func)),
        )
        .await?;

        let handle = renderer.add_material(pbr::PbrMaterial {
            albedo: match albedo_tex {
                Some(tex) => pbr::AlbedoComponent::TextureVertexValue {
                    texture: tex.handle,
                    value: Vec4::from(albedo_factor),
                    srgb: false,
                },
                None => pbr::AlbedoComponent::ValueVertex { value: Vec4::from(albedo_factor), srgb: false },
            },
            transparency: match material.alpha_mode() {
                gltf::material::AlphaMode::Opaque => pbr::Transparency::Opaque,
                gltf::material::AlphaMode::Mask => {
                    pbr::Transparency::Cutout { cutout: material.alpha_cutoff().unwrap_or(0.5) }
                }
                gltf::material::AlphaMode::Blend => pbr::Transparency::Blend,
            },
            normal: match normals_tex {
                Some(tex) if util::format_components(tex.format) == Some(2) => {
                    pbr::NormalTexture::Bicomponent(tex.handle, settings.normal_direction)
                }
                Some(tex) if util::format_components(tex.format) >= Some(3) => {
                    pbr::NormalTexture::Tricomponent(tex.handle, settings.normal_direction)
                }
                _ => pbr::NormalTexture::None,
            },
            aomr_textures: match (metallic_roughness_tex, occlusion_tex) {
                (Some(mr), Some(ao)) if mr == ao => pbr::AoMRTextures::Combined { texture: Some(mr.handle) },
                (mr, ao)
                    if ao
                        .as_ref()
                        .map(|ao| util::format_components(ao.format).map_or(false, |v| v < 3))
                        .unwrap_or(false) =>
                {
                    pbr::AoMRTextures::Split {
                        mr_texture: util::extract_handle(mr),
                        ao_texture: util::extract_handle(ao),
                    }
                }
                (mr, ao) => pbr::AoMRTextures::SwizzledSplit {
                    mr_texture: util::extract_handle(mr),
                    ao_texture: util::extract_handle(ao),
                },
            },
            metallic_factor: Some(metallic_factor),
            roughness_factor: Some(roughness_factor),
            emissive: match emissive_tex {
                Some(tex) => {
                    pbr::MaterialComponent::TextureValue { texture: tex.handle, value: Vec3::from(emissive_factor) }
                }
                None => pbr::MaterialComponent::Value(Vec3::from(emissive_factor)),
            },
            uv_transform0: uv_transform,
            uv_transform1: uv_transform,
            unlit: material.unlit(),
            sample_type: nearest,
            ..pbr::PbrMaterial::default()
        });

        result.push(Labeled::new(handle, material.name()));
    }

    Ok((result, images))
}

/// Loads a single image from a [`gltf::Image`], with caching.
///
/// Uses the given ImageMap as a cache.
///
/// All binary data buffers must be provided. You can get the image from a
/// texture by calling [`gltf::Texture::source`].
///
/// io_func determines how URIs are resolved into their underlying data.
pub async fn load_image_cached<F, Fut, E>(
    renderer: &Arc<Renderer>,
    images: &mut ImageMap,
    image: gltf::Image<'_>,
    srgb: bool,
    buffers: &[Vec<u8>],
    io_func: &mut F,
) -> Result<Labeled<Texture>, GltfLoadError<E>>
where
    F: FnMut(SsoString) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, E>>,
    E: std::error::Error + 'static,
{
    let key = ImageKey { index: image.index(), srgb };

    let entry = match images.entry(key) {
        Entry::Occupied(handle) => return Ok(handle.get().clone()),
        Entry::Vacant(v) => v,
    };

    let handle = load_image(renderer, image, srgb, buffers, io_func).await?;

    entry.insert(handle.clone());

    Ok(handle)
}

/// Loads a single image from a [`gltf::Image`].
///
/// All binary data buffers must be provided. Call this with
/// [`gltf::Document::materials`] as the materials argument.
///
/// io_func determines how URIs are resolved into their underlying data.
pub async fn load_image<F, Fut, E>(
    renderer: &Arc<Renderer>,
    image: gltf::Image<'_>,
    srgb: bool,
    buffers: &[Vec<u8>],
    io_func: &mut F,
) -> Result<Labeled<Texture>, GltfLoadError<E>>
where
    F: FnMut(SsoString) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, E>>,
    E: std::error::Error + 'static,
{
    // profiling::scope!("load image", image.name().unwrap_or_default());
    let (data, uri) = match image.source() {
        gltf::image::Source::Uri { uri, .. } => {
            let data =
                io_func(SsoString::from(uri)).await.map_err(|e| GltfLoadError::TextureIo(SsoString::from(uri), e))?;
            (Cow::Owned(data), SsoString::from(uri))
        }
        gltf::image::Source::View { view, .. } => {
            let start = view.offset();
            let end = start + view.length();
            (Cow::Borrowed(&buffers[view.buffer().index()][start..end]), SsoString::from("<embedded>"))
        }
    };

    let mut uri = Some(uri);
    let mut texture = None;

    #[cfg(feature = "ktx2")]
    if let Ok(reader) = ktx2::Reader::new(&data) {
        profiling::scope!("parsing ktx2");

        let header = reader.header();

        let src_format = header.format.unwrap();
        let format = util::map_ktx2_format(src_format, srgb)
            .ok_or_else(|| GltfLoadError::TextureBadKxt2Format(uri.take().unwrap(), src_format))?;

        if header.level_count == 0 {
            return Err(GltfLoadError::TextureZeroLevels(uri.take().unwrap()));
        }
        if header.layer_count >= 2 {
            return Err(GltfLoadError::TextureTooManyLayers(uri.take().unwrap()));
        }

        let guaranteed_format = format.guaranteed_format_features(Default::default());
        let generate = header.level_count == 1
            && guaranteed_format.flags.contains(rend3::types::TextureFormatFeatureFlags::FILTERABLE)
            && guaranteed_format.allowed_usages.contains(
                rend3::types::TextureUsages::TEXTURE_BINDING | rend3::types::TextureUsages::RENDER_ATTACHMENT,
            );

        let size: usize = reader.levels().map(|s| s.len()).sum();

        let mut data = Vec::with_capacity(size);
        for level in reader.levels() {
            data.extend_from_slice(level);
        }

        texture = Some(types::Texture {
            label: image.name().map(str::to_owned),
            format,
            size: UVec2::new(header.pixel_width, header.pixel_height),
            data,
            mip_count: if generate {
                types::MipmapCount::Maximum
            } else {
                types::MipmapCount::Specific(std::num::NonZeroU32::new(header.level_count).unwrap())
            },
            mip_source: if generate { types::MipmapSource::Generated } else { types::MipmapSource::Uploaded },
        })
    }

    #[cfg(feature = "ddsfile")]
    if texture.is_none() {
        if let Ok(dds) = ddsfile::Dds::read(&mut std::io::Cursor::new(&data)) {
            profiling::scope!("parsing dds");
            let format = dds
                .get_dxgi_format()
                .map(|f| {
                    util::map_dxgi_format(f, srgb)
                        .ok_or_else(|| GltfLoadError::TextureBadDxgiFormat(uri.take().unwrap(), f))
                })
                .or_else(|| {
                    dds.get_d3d_format().map(|f| {
                        util::map_d3d_format(f, srgb)
                            .ok_or_else(|| GltfLoadError::TextureBadD3DFormat(uri.take().unwrap(), f))
                    })
                })
                .unwrap()?;

            let levels = dds.get_num_mipmap_levels();

            if levels == 0 {
                return Err(GltfLoadError::TextureZeroLevels(uri.take().unwrap()));
            }

            let guaranteed_format = format.guaranteed_format_features(Default::default());
            let generate = dds.get_num_mipmap_levels() == 1
                && guaranteed_format.flags.contains(rend3::types::TextureFormatFeatureFlags::FILTERABLE)
                && guaranteed_format.allowed_usages.contains(
                    rend3::types::TextureUsages::TEXTURE_BINDING | rend3::types::TextureUsages::RENDER_ATTACHMENT,
                );

            let data = dds.get_data(0).map_err(|_| GltfLoadError::TextureTooManyLayers(uri.take().unwrap()))?;

            texture = Some(types::Texture {
                label: image.name().map(str::to_owned),
                format,
                size: UVec2::new(dds.get_width(), dds.get_height()),
                data: data.to_vec(),
                mip_count: if generate {
                    types::MipmapCount::Maximum
                } else {
                    types::MipmapCount::Specific(std::num::NonZeroU32::new(dds.get_num_mipmap_levels()).unwrap())
                },
                mip_source: if generate { types::MipmapSource::Generated } else { types::MipmapSource::Uploaded },
            })
        }
    }

    if texture.is_none() {
        profiling::scope!("decoding image");
        let parsed =
            image::load_from_memory(&data).map_err(|e| GltfLoadError::TextureDecode(uri.take().unwrap(), e))?;
        let size = UVec2::new(parsed.width(), parsed.height());
        let (data, format) = util::convert_dynamic_image(parsed, srgb);

        texture = Some(types::Texture {
            label: image.name().map(str::to_owned),
            format,
            size,
            data,
            mip_count: types::MipmapCount::Maximum,
            mip_source: types::MipmapSource::Generated,
        })
    };

    let texture = texture.unwrap();
    let format = texture.format;
    let handle = renderer.add_texture_2d(texture)?;

    Ok(Labeled::new(Texture { handle, format }, image.name()))
}

/// Implementation utilities.
pub mod util {
    use std::future::Future;

    use image::{buffer::ConvertBuffer, ImageBuffer, Luma, Rgba};
    use rend3::types;

    use crate::{Labeled, Texture};

    /// Turns an `Option<Texture>` into `Option<types::TextureHandle>`
    pub fn extract_handle(texture: Option<Texture>) -> Option<types::Texture2DHandle> {
        texture.map(|t| t.handle)
    }

    /// Turns a `Option<Future<Output = Result<Labeled<T>, E>>>>` into a
    /// `Future<Output = Result<Option<T>, E>>`
    ///
    /// This is a very specific transformation that shows up a lot when using
    /// [`load_image_cached`](super::load_image_cached).
    pub async fn texture_option_resolve<F: Future, T, E>(fut: Option<F>) -> Result<Option<T>, E>
    where
        F: Future<Output = Result<Labeled<T>, E>>,
    {
        if let Some(f) = fut {
            match f.await {
                Ok(l) => Ok(Some(l.inner)),
                Err(e) => Err(e),
            }
        } else {
            Ok(None)
        }
    }

    pub fn convert_dynamic_image(image: image::DynamicImage, srgb: bool) -> (Vec<u8>, rend3::types::TextureFormat) {
        use rend3::types::TextureFormat as r3F;

        // profiling::scope!("convert dynamic image");
        match image {
            image::DynamicImage::ImageLuma8(i) => (i.into_raw(), r3F::R8Unorm),
            image::DynamicImage::ImageLumaA8(i) => {
                (ConvertBuffer::<ImageBuffer<Luma<u8>, Vec<u8>>>::convert(&i).into_raw(), r3F::R8Unorm)
            }
            image::DynamicImage::ImageRgb8(i) => (
                ConvertBuffer::<ImageBuffer<Rgba<u8>, Vec<u8>>>::convert(&i).into_raw(),
                if srgb { r3F::Rgba8UnormSrgb } else { r3F::Rgba8Unorm },
            ),
            image::DynamicImage::ImageRgba8(i) => {
                (i.into_raw(), if srgb { r3F::Rgba8UnormSrgb } else { r3F::Rgba8Unorm })
            }
            i => (i.into_rgba8().into_raw(), if srgb { r3F::Rgba8UnormSrgb } else { r3F::Rgba8Unorm }),
        }
    }

    /// Maps a ktx2 format into the rend3's TextureFormat
    #[cfg(feature = "ktx2")]
    pub fn map_ktx2_format(format: ktx2::Format, srgb: bool) -> Option<rend3::types::TextureFormat> {
        use ktx2::Format as k2F;
        use rend3::types::{AstcBlock, AstcChannel, TextureFormat as r3F};
        Some(match format {
            k2F::R4G4_UNORM_PACK8
            | k2F::R4G4B4A4_UNORM_PACK16
            | k2F::B4G4R4A4_UNORM_PACK16
            | k2F::R5G6B5_UNORM_PACK16
            | k2F::B5G6R5_UNORM_PACK16
            | k2F::R5G5B5A1_UNORM_PACK16
            | k2F::B5G5R5A1_UNORM_PACK16
            | k2F::A1R5G5B5_UNORM_PACK16 => return None,
            k2F::R8_UNORM | k2F::R8_SRGB => {
                if srgb {
                    return None;
                } else {
                    r3F::R8Unorm
                }
            }
            k2F::R8_SNORM => r3F::R8Snorm,
            k2F::R8_UINT => r3F::R8Uint,
            k2F::R8_SINT => r3F::R8Sint,
            k2F::R8G8_UNORM | k2F::R8G8_SRGB => {
                if srgb {
                    return None;
                } else {
                    r3F::Rg8Unorm
                }
            }
            k2F::R8G8_SNORM => r3F::Rg8Snorm,
            k2F::R8G8_UINT => r3F::Rg8Uint,
            k2F::R8G8_SINT => r3F::Rg8Sint,
            k2F::R8G8B8_UNORM
            | k2F::R8G8B8_SNORM
            | k2F::R8G8B8_UINT
            | k2F::R8G8B8_SINT
            | k2F::R8G8B8_SRGB
            | k2F::B8G8R8_UNORM
            | k2F::B8G8R8_SNORM
            | k2F::B8G8R8_UINT
            | k2F::B8G8R8_SINT
            | k2F::B8G8R8_SRGB => return None,
            k2F::R8G8B8A8_UNORM | k2F::R8G8B8A8_SRGB => {
                if srgb {
                    r3F::Rgba8UnormSrgb
                } else {
                    r3F::Rgba8Unorm
                }
            }
            k2F::R8G8B8A8_SNORM => r3F::Rgba8Snorm,
            k2F::R8G8B8A8_UINT => r3F::Rgba8Uint,
            k2F::R8G8B8A8_SINT => r3F::Rgba8Sint,
            k2F::B8G8R8A8_UNORM | k2F::B8G8R8A8_SRGB => {
                if srgb {
                    r3F::Bgra8UnormSrgb
                } else {
                    r3F::Bgra8Unorm
                }
            }
            k2F::B8G8R8A8_SNORM | k2F::B8G8R8A8_UINT | k2F::B8G8R8A8_SINT => return None,
            k2F::A2B10G10R10_UNORM_PACK32 => r3F::Rgb10a2Unorm,
            k2F::A2R10G10B10_UNORM_PACK32
            | k2F::A2R10G10B10_SNORM_PACK32
            | k2F::A2R10G10B10_UINT_PACK32
            | k2F::A2R10G10B10_SINT_PACK32
            | k2F::A2B10G10R10_SNORM_PACK32
            | k2F::A2B10G10R10_UINT_PACK32
            | k2F::A2B10G10R10_SINT_PACK32 => return None,
            k2F::R16_UNORM | k2F::R16_SNORM => return None,
            k2F::R16_UINT => r3F::R16Uint,
            k2F::R16_SINT => r3F::R16Sint,
            k2F::R16_SFLOAT => r3F::R16Float,
            k2F::R16G16_UNORM | k2F::R16G16_SNORM => return None,
            k2F::R16G16_UINT => r3F::Rg16Uint,
            k2F::R16G16_SINT => r3F::Rg16Sint,
            k2F::R16G16_SFLOAT => r3F::Rg16Float,
            k2F::R16G16B16_UNORM
            | k2F::R16G16B16_SNORM
            | k2F::R16G16B16_UINT
            | k2F::R16G16B16_SINT
            | k2F::R16G16B16_SFLOAT => return None,
            k2F::R16G16B16A16_UNORM => r3F::Rgba16Unorm,
            k2F::R16G16B16A16_SNORM => r3F::Rgba16Snorm,
            k2F::R16G16B16A16_UINT => r3F::Rgba16Uint,
            k2F::R16G16B16A16_SINT => r3F::Rgba16Sint,
            k2F::R16G16B16A16_SFLOAT => r3F::Rgba16Float,
            k2F::R32_UINT => r3F::R32Uint,
            k2F::R32_SINT => r3F::R32Sint,
            k2F::R32_SFLOAT => r3F::R32Float,
            k2F::R32G32_UINT => r3F::Rg32Uint,
            k2F::R32G32_SINT => r3F::Rg32Sint,
            k2F::R32G32_SFLOAT => r3F::Rg32Float,
            k2F::R32G32B32_UINT | k2F::R32G32B32_SINT | k2F::R32G32B32_SFLOAT => return None,
            k2F::R32G32B32A32_UINT => r3F::Rgba32Uint,
            k2F::R32G32B32A32_SINT => r3F::Rgba32Sint,
            k2F::R32G32B32A32_SFLOAT => r3F::Rgba32Float,
            k2F::R64_UINT
            | k2F::R64_SINT
            | k2F::R64_SFLOAT
            | k2F::R64G64_UINT
            | k2F::R64G64_SINT
            | k2F::R64G64_SFLOAT
            | k2F::R64G64B64_UINT
            | k2F::R64G64B64_SINT
            | k2F::R64G64B64_SFLOAT
            | k2F::R64G64B64A64_UINT
            | k2F::R64G64B64A64_SINT
            | k2F::R64G64B64A64_SFLOAT => return None,
            k2F::B10G11R11_UFLOAT_PACK32 => r3F::Rg11b10Ufloat,
            k2F::E5B9G9R9_UFLOAT_PACK32 => r3F::Rgb9e5Ufloat,
            k2F::D16_UNORM => return None,
            k2F::X8_D24_UNORM_PACK32 => r3F::Depth24Plus,
            k2F::D32_SFLOAT => r3F::Depth32Float,
            k2F::S8_UINT | k2F::D16_UNORM_S8_UINT => return None,
            k2F::D24_UNORM_S8_UINT => r3F::Depth24PlusStencil8,
            k2F::D32_SFLOAT_S8_UINT => return None,
            k2F::BC1_RGB_UNORM_BLOCK
            | k2F::BC1_RGB_SRGB_BLOCK
            | k2F::BC1_RGBA_UNORM_BLOCK
            | k2F::BC1_RGBA_SRGB_BLOCK => {
                if srgb {
                    r3F::Bc1RgbaUnormSrgb
                } else {
                    r3F::Bc1RgbaUnorm
                }
            }
            k2F::BC2_UNORM_BLOCK | k2F::BC2_SRGB_BLOCK => {
                if srgb {
                    r3F::Bc2RgbaUnormSrgb
                } else {
                    r3F::Bc2RgbaUnorm
                }
            }
            k2F::BC3_UNORM_BLOCK | k2F::BC3_SRGB_BLOCK => {
                if srgb {
                    r3F::Bc3RgbaUnormSrgb
                } else {
                    r3F::Bc3RgbaUnorm
                }
            }
            k2F::BC4_UNORM_BLOCK => r3F::Bc4RUnorm,
            k2F::BC4_SNORM_BLOCK => r3F::Bc4RSnorm,
            k2F::BC5_UNORM_BLOCK => r3F::Bc5RgUnorm,
            k2F::BC5_SNORM_BLOCK => r3F::Bc5RgSnorm,
            k2F::BC6H_UFLOAT_BLOCK => r3F::Bc6hRgbUfloat,
            k2F::BC6H_SFLOAT_BLOCK => r3F::Bc6hRgbFloat,
            k2F::BC7_UNORM_BLOCK | k2F::BC7_SRGB_BLOCK => {
                if srgb {
                    r3F::Bc7RgbaUnormSrgb
                } else {
                    r3F::Bc7RgbaUnorm
                }
            }
            k2F::ETC2_R8G8B8_UNORM_BLOCK | k2F::ETC2_R8G8B8_SRGB_BLOCK => {
                if srgb {
                    r3F::Etc2Rgb8UnormSrgb
                } else {
                    r3F::Etc2Rgb8Unorm
                }
            }
            k2F::ETC2_R8G8B8A1_UNORM_BLOCK | k2F::ETC2_R8G8B8A1_SRGB_BLOCK => {
                if srgb {
                    r3F::Etc2Rgb8A1UnormSrgb
                } else {
                    r3F::Etc2Rgb8A1Unorm
                }
            }
            k2F::ETC2_R8G8B8A8_UNORM_BLOCK | k2F::ETC2_R8G8B8A8_SRGB_BLOCK => return None,
            k2F::EAC_R11_UNORM_BLOCK => r3F::EacR11Unorm,
            k2F::EAC_R11_SNORM_BLOCK => r3F::EacR11Snorm,
            k2F::EAC_R11G11_UNORM_BLOCK => r3F::EacRg11Unorm,
            k2F::EAC_R11G11_SNORM_BLOCK => r3F::EacRg11Snorm,
            k2F::ASTC_4x4_UNORM_BLOCK | k2F::ASTC_4x4_SRGB_BLOCK => r3F::Astc {
                block: AstcBlock::B4x4,
                channel: if srgb { AstcChannel::UnormSrgb } else { AstcChannel::Unorm },
            },
            k2F::ASTC_5x4_UNORM_BLOCK | k2F::ASTC_5x4_SRGB_BLOCK => r3F::Astc {
                block: AstcBlock::B5x4,
                channel: if srgb { AstcChannel::UnormSrgb } else { AstcChannel::Unorm },
            },
            k2F::ASTC_5x5_UNORM_BLOCK | k2F::ASTC_5x5_SRGB_BLOCK => r3F::Astc {
                block: AstcBlock::B5x5,
                channel: if srgb { AstcChannel::UnormSrgb } else { AstcChannel::Unorm },
            },
            k2F::ASTC_6x5_UNORM_BLOCK | k2F::ASTC_6x5_SRGB_BLOCK => r3F::Astc {
                block: AstcBlock::B6x5,
                channel: if srgb { AstcChannel::UnormSrgb } else { AstcChannel::Unorm },
            },
            k2F::ASTC_6x6_UNORM_BLOCK | k2F::ASTC_6x6_SRGB_BLOCK => r3F::Astc {
                block: AstcBlock::B6x6,
                channel: if srgb { AstcChannel::UnormSrgb } else { AstcChannel::Unorm },
            },
            k2F::ASTC_8x5_UNORM_BLOCK | k2F::ASTC_8x5_SRGB_BLOCK => r3F::Astc {
                block: AstcBlock::B8x5,
                channel: if srgb { AstcChannel::UnormSrgb } else { AstcChannel::Unorm },
            },
            k2F::ASTC_8x6_UNORM_BLOCK | k2F::ASTC_8x6_SRGB_BLOCK => r3F::Astc {
                block: AstcBlock::B8x6,
                channel: if srgb { AstcChannel::UnormSrgb } else { AstcChannel::Unorm },
            },
            k2F::ASTC_8x8_UNORM_BLOCK | k2F::ASTC_8x8_SRGB_BLOCK => r3F::Astc {
                block: AstcBlock::B8x8,
                channel: if srgb { AstcChannel::UnormSrgb } else { AstcChannel::Unorm },
            },
            k2F::ASTC_10x5_UNORM_BLOCK | k2F::ASTC_10x5_SRGB_BLOCK => r3F::Astc {
                block: AstcBlock::B10x5,
                channel: if srgb { AstcChannel::UnormSrgb } else { AstcChannel::Unorm },
            },
            k2F::ASTC_10x6_UNORM_BLOCK | k2F::ASTC_10x6_SRGB_BLOCK => r3F::Astc {
                block: AstcBlock::B10x6,
                channel: if srgb { AstcChannel::UnormSrgb } else { AstcChannel::Unorm },
            },
            k2F::ASTC_10x8_UNORM_BLOCK | k2F::ASTC_10x8_SRGB_BLOCK => r3F::Astc {
                block: AstcBlock::B10x8,
                channel: if srgb { AstcChannel::UnormSrgb } else { AstcChannel::Unorm },
            },
            k2F::ASTC_10x10_UNORM_BLOCK | k2F::ASTC_10x10_SRGB_BLOCK => r3F::Astc {
                block: AstcBlock::B10x10,
                channel: if srgb { AstcChannel::UnormSrgb } else { AstcChannel::Unorm },
            },
            k2F::ASTC_12x10_UNORM_BLOCK | k2F::ASTC_12x10_SRGB_BLOCK => r3F::Astc {
                block: AstcBlock::B12x10,
                channel: if srgb { AstcChannel::UnormSrgb } else { AstcChannel::Unorm },
            },
            k2F::ASTC_12x12_UNORM_BLOCK | k2F::ASTC_12x12_SRGB_BLOCK => r3F::Astc {
                block: AstcBlock::B12x12,
                channel: if srgb { AstcChannel::UnormSrgb } else { AstcChannel::Unorm },
            },
            _ => return None,
        })
    }

    /// Maps a dds file d3dformat into the rend3's TextureFormat
    #[cfg(feature = "ddsfile")]
    pub fn map_d3d_format(format: ddsfile::D3DFormat, srgb: bool) -> Option<rend3::types::TextureFormat> {
        use ddsfile::D3DFormat as d3F;
        use rend3::types::TextureFormat as r3F;

        Some(match format {
            d3F::A8B8G8R8 => {
                if srgb {
                    r3F::Rgba8UnormSrgb
                } else {
                    r3F::Rgba8Unorm
                }
            }
            d3F::G16R16 => r3F::Rg16Uint,
            d3F::A2B10G10R10 => return None,
            d3F::A1R5G5B5 => return None,
            d3F::R5G6B5 => return None,
            d3F::A8 => r3F::R8Unorm,
            d3F::A8R8G8B8 => {
                if srgb {
                    r3F::Bgra8UnormSrgb
                } else {
                    r3F::Bgra8Unorm
                }
            }
            d3F::X8R8G8B8
            | d3F::X8B8G8R8
            | d3F::A2R10G10B10
            | d3F::R8G8B8
            | d3F::X1R5G5B5
            | d3F::A4R4G4B4
            | d3F::X4R4G4B4
            | d3F::A8R3G3B2 => return None,
            d3F::A8L8 => r3F::Rg8Uint,
            d3F::L16 => r3F::R16Uint,
            d3F::L8 => r3F::R8Uint,
            d3F::A4L4 => return None,
            d3F::DXT1 => {
                if srgb {
                    r3F::Bc1RgbaUnormSrgb
                } else {
                    r3F::Bc1RgbaUnorm
                }
            }
            d3F::DXT2 | d3F::DXT3 => {
                if srgb {
                    r3F::Bc2RgbaUnormSrgb
                } else {
                    r3F::Bc2RgbaUnorm
                }
            }
            d3F::DXT4 | d3F::DXT5 => {
                if srgb {
                    r3F::Bc3RgbaUnormSrgb
                } else {
                    r3F::Bc3RgbaUnorm
                }
            }
            d3F::R8G8_B8G8 => return None,
            d3F::G8R8_G8B8 => return None,
            d3F::A16B16G16R16 => r3F::Rgba16Uint,
            d3F::Q16W16V16U16 => r3F::Rgba16Sint,
            d3F::R16F => r3F::R16Float,
            d3F::G16R16F => r3F::Rg16Float,
            d3F::A16B16G16R16F => r3F::Rgba16Float,
            d3F::R32F => r3F::R32Float,
            d3F::G32R32F => r3F::Rg32Float,
            d3F::A32B32G32R32F => r3F::Rgba32Float,
            d3F::UYVY => return None,
            d3F::YUY2 => return None,
            d3F::CXV8U8 => return None,
        })
    }

    #[cfg(feature = "ddsfile")]
    pub fn map_dxgi_format(format: ddsfile::DxgiFormat, srgb: bool) -> Option<rend3::types::TextureFormat> {
        use ddsfile::DxgiFormat as d3F;
        use rend3::types::TextureFormat as r3F;

        Some(match format {
            d3F::Unknown => return None,
            d3F::R32G32B32A32_Typeless | d3F::R32G32B32A32_Float => r3F::Rgba32Float,
            d3F::R32G32B32A32_UInt => r3F::Rgba32Uint,
            d3F::R32G32B32A32_SInt => r3F::Rgba32Sint,
            d3F::R32G32B32_Typeless | d3F::R32G32B32_Float | d3F::R32G32B32_UInt | d3F::R32G32B32_SInt => return None,
            d3F::R16G16B16A16_Typeless | d3F::R16G16B16A16_Float => r3F::Rgba16Float,
            d3F::R16G16B16A16_UInt => r3F::Rgba16Uint,
            d3F::R16G16B16A16_UNorm | d3F::R16G16B16A16_SNorm => return None,
            d3F::R16G16B16A16_SInt => r3F::Rgba16Sint,
            d3F::R32G32_Typeless | d3F::R32G32_Float => r3F::Rg32Float,
            d3F::R32G32_UInt => r3F::Rg32Uint,
            d3F::R32G32_SInt => r3F::Rg32Sint,
            d3F::R32G8X24_Typeless
            | d3F::D32_Float_S8X24_UInt
            | d3F::R32_Float_X8X24_Typeless
            | d3F::X32_Typeless_G8X24_UInt
            | d3F::R10G10B10A2_Typeless
            | d3F::R10G10B10A2_UNorm
            | d3F::R10G10B10A2_UInt => return None,
            d3F::R11G11B10_Float => r3F::Rg11b10Ufloat,
            d3F::R8G8B8A8_Typeless | d3F::R8G8B8A8_UNorm | d3F::R8G8B8A8_UNorm_sRGB => {
                if srgb {
                    r3F::Rgba8UnormSrgb
                } else {
                    r3F::Rgba8Unorm
                }
            }
            d3F::R8G8B8A8_UInt => r3F::Rgba8Uint,
            d3F::R8G8B8A8_SNorm => r3F::Rgba8Snorm,
            d3F::R8G8B8A8_SInt => r3F::Rgba8Sint,
            d3F::R16G16_Typeless | d3F::R16G16_Float => r3F::Rg16Float,
            d3F::R16G16_UInt => r3F::Rg16Uint,
            d3F::R16G16_SInt => r3F::Rg16Sint,
            d3F::R16G16_UNorm | d3F::R16G16_SNorm => return None,
            d3F::R32_Typeless | d3F::R32_Float => r3F::R32Float,
            d3F::D32_Float => r3F::Depth32Float,
            d3F::R32_UInt => r3F::R32Uint,
            d3F::R32_SInt => r3F::R32Sint,
            d3F::R24G8_Typeless | d3F::D24_UNorm_S8_UInt => r3F::Depth24PlusStencil8,
            d3F::R24_UNorm_X8_Typeless => r3F::Depth24Plus,
            d3F::X24_Typeless_G8_UInt => return None,
            d3F::R8G8_Typeless | d3F::R8G8_UNorm => r3F::Rg8Unorm,
            d3F::R8G8_UInt => r3F::Rg8Uint,
            d3F::R8G8_SNorm => r3F::Rg8Snorm,
            d3F::R8G8_SInt => r3F::Rg8Sint,
            d3F::R16_Typeless | d3F::R16_Float => r3F::R16Float,
            d3F::D16_UNorm | d3F::R16_SNorm | d3F::R16_UNorm => return None,
            d3F::R16_UInt => r3F::R16Uint,
            d3F::R16_SInt => r3F::R16Sint,
            d3F::R8_Typeless | d3F::R8_UNorm => r3F::R8Unorm,
            d3F::R8_UInt => r3F::R8Uint,
            d3F::R8_SNorm => r3F::R8Snorm,
            d3F::R8_SInt => r3F::R8Sint,
            d3F::A8_UNorm => return None,
            d3F::R1_UNorm => return None,
            d3F::R9G9B9E5_SharedExp => r3F::Rgb9e5Ufloat,
            d3F::R8G8_B8G8_UNorm => return None,
            d3F::G8R8_G8B8_UNorm => return None,
            d3F::BC1_Typeless | d3F::BC1_UNorm | d3F::BC1_UNorm_sRGB => {
                if srgb {
                    r3F::Bc1RgbaUnormSrgb
                } else {
                    r3F::Bc1RgbaUnorm
                }
            }

            d3F::BC2_Typeless | d3F::BC2_UNorm | d3F::BC2_UNorm_sRGB => {
                if srgb {
                    r3F::Bc2RgbaUnormSrgb
                } else {
                    r3F::Bc2RgbaUnorm
                }
            }

            d3F::BC3_Typeless | d3F::BC3_UNorm | d3F::BC3_UNorm_sRGB => {
                if srgb {
                    r3F::Bc3RgbaUnormSrgb
                } else {
                    r3F::Bc3RgbaUnorm
                }
            }

            d3F::BC4_Typeless | d3F::BC4_UNorm => r3F::Bc4RUnorm,
            d3F::BC4_SNorm => r3F::Bc4RSnorm,
            d3F::BC5_Typeless | d3F::BC5_UNorm => r3F::Bc5RgUnorm,
            d3F::BC5_SNorm => r3F::Bc5RgSnorm,
            d3F::B5G6R5_UNorm | d3F::B5G5R5A1_UNorm => return None,
            d3F::B8G8R8A8_UNorm | d3F::B8G8R8A8_Typeless | d3F::B8G8R8A8_UNorm_sRGB => {
                if srgb {
                    r3F::Bgra8UnormSrgb
                } else {
                    r3F::Bgra8Unorm
                }
            }
            d3F::B8G8R8X8_UNorm
            | d3F::R10G10B10_XR_Bias_A2_UNorm
            | d3F::B8G8R8X8_Typeless
            | d3F::B8G8R8X8_UNorm_sRGB => return None,
            d3F::BC6H_Typeless | d3F::BC6H_UF16 => r3F::Bc6hRgbUfloat,
            d3F::BC6H_SF16 => r3F::Bc6hRgbFloat,
            d3F::BC7_Typeless | d3F::BC7_UNorm | d3F::BC7_UNorm_sRGB => {
                if srgb {
                    r3F::Bc7RgbaUnormSrgb
                } else {
                    r3F::Bc7RgbaUnorm
                }
            }
            d3F::AYUV
            | d3F::Y410
            | d3F::Y416
            | d3F::NV12
            | d3F::P010
            | d3F::P016
            | d3F::Format_420_Opaque
            | d3F::YUY2
            | d3F::Y210
            | d3F::Y216
            | d3F::NV11
            | d3F::AI44
            | d3F::IA44
            | d3F::P8
            | d3F::A8P8
            | d3F::B4G4R4A4_UNorm
            | d3F::P208
            | d3F::V208
            | d3F::V408
            | d3F::Force_UInt => return None,
        })
    }

    pub fn format_components(tex: types::TextureFormat) -> Option<u8> {
        Some(tex.components())
    }
}
