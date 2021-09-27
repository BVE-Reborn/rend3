//! gltf scene and model loader for rend3.
//!
//! This crate attempts to map the concepts into gltf as best it can into rend3, but there is quite a variety of things that would be insane to properly represent.
//!
//! To "just load a gltf/glb", look at the documentation for [`load_gltf`] and use the default [`filesystem_io_func`].
//!
//! Individual components of a gltf can be loaded with the other functions in this crate.
//!
//! # Supported Extensions
//! - `KHR_punctual_lights`
//! - `KHR_texture_transform`
//! - `KHR_material_unlit`
//!
//! # Known Limitations
//! - Only the albedo texture's transform from `KHR_texture_transform` will be used.
//! - Double sided materials are currently unsupported.

use glam::{Mat3, Mat4, UVec2, Vec2, Vec3, Vec4, Vec4Swizzles};
use gltf::buffer::Source;
use rend3::{
    types::{self, ObjectHandle},
    util::typedefs::{FastHashMap, SsoString},
    Renderer,
};
use rend3_pbr::material;
use std::{borrow::Cow, collections::hash_map::Entry, future::Future, path::Path};
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
        Self {
            inner,
            label: label.map(SsoString::from),
        }
    }
}

/// A single sub-mesh of a gltf.
#[derive(Debug)]
pub struct MeshPrimitive {
    pub handle: types::MeshHandle,
    /// Index into the material vector given by [`load_materials_and_textures`] or [`LoadedGltfScene::materials`].
    pub material: Option<usize>,
}

/// Set of [`MeshPrimitive`]s that make up a logical mesh.
#[derive(Debug)]
pub struct Mesh {
    pub primitives: Vec<MeshPrimitive>,
}

/// Set of [`ObjectHandle`]s that correspond to a logical object in the node tree.
///
/// This is to a [`ObjectHandle`], as a [`Mesh`] is to a [`MeshPrimitive`].
#[derive(Debug)]
pub struct Object {
    pub primitives: Vec<ObjectHandle>,
}

/// Node in the gltf scene tree
#[derive(Debug)]
pub struct Node {
    pub children: Vec<Labeled<Node>>,
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

/// Hashmap which stores a mapping from [`ImageKey`] to a labeled handle.
pub type ImageMap = FastHashMap<ImageKey, Labeled<types::TextureHandle>>;

/// A fully loaded Gltf scene.
#[derive(Debug)]
pub struct LoadedGltfScene {
    pub meshes: Vec<Labeled<Mesh>>,
    pub materials: Vec<Labeled<types::MaterialHandle>>,
    pub default_material: types::MaterialHandle,
    pub images: ImageMap,
    pub nodes: Vec<Labeled<Node>>,
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
    TextureLoad(SsoString, #[source] image::ImageError),
    #[error("Gltf file must have at least one scene")]
    MissingScene,
    #[error("Mesh {0} does not have positions")]
    MissingPositions(usize),
    #[error("Gltf file references mesh {0} but mesh does not exist")]
    MissingMesh(usize),
    #[error("Gltf file references material {0} but material does not exist")]
    MissingMaterial(usize),
    #[error("Mesh {0} primitive {1} uses unsupported mode {2:?}. Only triangles are supported.")]
    UnsupportedPrimitiveMode(usize, usize, gltf::mesh::Mode),
}

/// Default implementation of [`load_gltf`]'s `io_func` that loads from the filesystem relative to the gltf.
///
/// The first argumnet is the directory all relative paths should be considered against. This is more than likely
/// the directory the gltf/glb is in.
pub async fn filesystem_io_func(parent_director: impl AsRef<Path>, uri: SsoString) -> Result<Vec<u8>, std::io::Error> {
    let octet_stream_header = "data:";
    if let Some(base64_data) = uri.strip_prefix(octet_stream_header) {
        let (_mime, rest) = base64_data.split_once(";").unwrap();
        let (encoding, data) = rest.split_once(",").unwrap();
        assert_eq!(encoding, "base64");
        log::info!("loading {} bytes of base64 data", data.len());
        // TODO: errors
        Ok(base64::decode(data).unwrap())
    } else {
        let path_resolved = parent_director.as_ref().join(&*uri);
        log::info!("loading file '{}' from disk", path_resolved.display());
        std::fs::read(path_resolved)
    }
}

/// Load a given gltf into the renderer's world.
///
/// Allows the user to specify how URIs are resolved into their underlying data. Supports most gltfs and glbs.
///
/// **Must** keep the [`LoadedGltfScene`] alive for the scene to remain.
///
/// ```no_run
/// # use std::path::Path;
/// # let renderer = unimplemented!();
/// let path = Path::new("some/path/scene.gltf"); // or glb
/// let gltf_data = std::fs::read(&path).unwrap();
/// let parent_directory = path.parent().unwrap();
/// let _loaded = pollster::block_on(rend3_gltf::load_gltf(&renderer, &gltf_data, |p| rend3_gltf::filesystem_io_func(&parent_directory, p)));
/// ```
pub async fn load_gltf<F, Fut, E>(
    renderer: &Renderer,
    data: &[u8],
    mut io_func: F,
) -> Result<LoadedGltfScene, GltfLoadError<E>>
where
    F: FnMut(SsoString) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, E>>,
    E: std::error::Error + 'static,
{
    let mut file = gltf::Gltf::from_slice_without_validation(data)?;
    let blob = file.blob.take();

    let buffers = load_buffers(file.buffers(), blob, &mut io_func).await?;

    let default_material = load_default_material(renderer);
    let meshes = load_meshes(renderer, file.meshes(), &buffers)?;
    let (materials, images) = load_materials_and_textures(renderer, file.materials(), &buffers, &mut io_func).await?;

    let scene = file
        .default_scene()
        .or_else(|| file.scenes().next())
        .ok_or(GltfLoadError::MissingScene)?;

    let mut loaded = LoadedGltfScene {
        meshes,
        materials,
        default_material,
        images,
        nodes: Vec::with_capacity(scene.nodes().len()),
    };

    loaded.nodes = load_gltf_nodes(
        renderer,
        &mut loaded,
        scene.nodes(),
        Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0)),
    )?;

    Ok(loaded)
}

fn load_gltf_nodes<'a, E: std::error::Error + 'static>(
    renderer: &Renderer,
    loaded: &mut LoadedGltfScene,
    nodes: impl Iterator<Item = gltf::Node<'a>>,
    parent_transform: Mat4,
) -> Result<Vec<Labeled<Node>>, GltfLoadError<E>> {
    let mut final_nodes = Vec::new();
    for node in nodes {
        let local_transform = Mat4::from_cols_array_2d(&node.transform().matrix());
        let transform = parent_transform * local_transform;

        let object = if let Some(mesh) = node.mesh() {
            let mesh_handle = loaded
                .meshes
                .get(mesh.index())
                .ok_or_else(|| GltfLoadError::MissingMesh(mesh.index()))?;
            let primitives: Result<Vec<_>, GltfLoadError<_>> = mesh_handle
                .inner
                .primitives
                .iter()
                .map(|prim| {
                    let mat_idx = prim.material;
                    let mat = mat_idx
                        .map_or_else(
                            || Some(&loaded.default_material),
                            |mat_idx| loaded.materials.get(mat_idx).map(|m| &m.inner),
                        )
                        .ok_or_else(|| {
                            GltfLoadError::MissingMaterial(mat_idx.expect("Could not find default material"))
                        })?;
                    Ok(renderer.add_object(types::Object {
                        mesh: prim.handle.clone(),
                        material: mat.clone(),
                        transform,
                    }))
                })
                .collect();
            Some(Labeled::new(
                Object {
                    primitives: primitives?,
                },
                mesh.name(),
            ))
        } else {
            None
        };

        let light = if let Some(light) = node.light() {
            match light.kind() {
                gltf::khr_lights_punctual::Kind::Directional => {
                    let direction = (transform * (-Vec3::Z).extend(1.0)).xyz();
                    Some(renderer.add_directional_light(types::DirectionalLight {
                        color: Vec3::from(light.color()),
                        intensity: light.intensity(),
                        direction,
                        distance: 400.0,
                    }))
                }
                _ => None,
            }
        } else {
            None
        };

        let children = load_gltf_nodes(renderer, loaded, node.children(), transform)?;

        final_nodes.push(Labeled::new(
            Node {
                children,
                local_transform,
                object,
                directional_light: light,
            },
            node.name(),
        ));
    }
    Ok(final_nodes)
}

/// Loads buffers from a [`gltf::Buffer`] iterator, calling io_func to resolve them from URI.
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
    let mut buffers = Vec::with_capacity(file.len());
    let mut blob_index = None;
    for b in file {
        let data = match b.source() {
            Source::Bin => {
                blob_index = Some(b.index());
                Vec::new()
            }
            Source::Uri(uri) => io_func(SsoString::from(uri))
                .await
                .map_err(|e| GltfLoadError::BufferIo(SsoString::from(uri), e))?,
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
/// All binary data buffers must be provided. Call this with [`gltf::Document::meshes`] as the mesh argument.
pub fn load_meshes<'a, E: std::error::Error + 'static>(
    renderer: &Renderer,
    meshes: impl Iterator<Item = gltf::Mesh<'a>>,
    buffers: &[Vec<u8>],
) -> Result<Vec<Labeled<Mesh>>, GltfLoadError<E>> {
    meshes
        .into_iter()
        .map(|mesh| {
            let mut res_prims = Vec::new();
            for prim in mesh.primitives() {
                if prim.mode() != gltf::mesh::Mode::Triangles {
                    return Err(GltfLoadError::UnsupportedPrimitiveMode(
                        mesh.index(),
                        prim.index(),
                        prim.mode(),
                    ));
                }

                let reader = prim.reader(|b| Some(&buffers[b.index()][..b.length()]));

                let vertex_positions: Vec<_> = reader
                    .read_positions()
                    .ok_or_else(|| GltfLoadError::MissingPositions(mesh.index()))?
                    .map(Vec3::from)
                    .collect();

                // glTF models are right handed, so we must flip their winding order
                let mut builder = types::MeshBuilder::new(vertex_positions).with_right_handed();

                if let Some(normals) = reader.read_normals() {
                    builder = builder.with_vertex_normals(normals.map(Vec3::from).collect())
                }

                if let Some(tangents) = reader.read_tangents() {
                    // todo: handedness
                    builder = builder.with_vertex_tangents(tangents.map(|[x, y, z, _]| Vec3::new(x, y, z)).collect())
                }

                if let Some(uvs) = reader.read_tex_coords(0) {
                    builder = builder.with_vertex_uv0(uvs.into_f32().map(Vec2::from).collect())
                }

                if let Some(uvs) = reader.read_tex_coords(1) {
                    builder = builder.with_vertex_uv1(uvs.into_f32().map(Vec2::from).collect())
                }

                if let Some(colors) = reader.read_colors(0) {
                    builder = builder.with_vertex_colors(colors.into_rgba_u8().collect())
                }

                if let Some(indices) = reader.read_indices() {
                    builder = builder.with_indices(indices.into_u32().collect())
                }

                let mesh = builder.build();

                let handle = renderer.add_mesh(mesh);

                res_prims.push(MeshPrimitive {
                    handle,
                    material: prim.material().index(),
                })
            }
            Ok(Labeled::new(Mesh { primitives: res_prims }, mesh.name()))
        })
        .collect()
}

/// Creates a gltf default material.
pub fn load_default_material(renderer: &Renderer) -> types::MaterialHandle {
    renderer.add_material(material::PbrMaterial {
        albedo: material::AlbedoComponent::Value(Vec4::splat(1.0)),
        transparency: material::Transparency::Opaque,
        normal: material::NormalTexture::None,
        aomr_textures: material::AoMRTextures::None,
        ao_factor: Some(1.0),
        metallic_factor: Some(1.0),
        roughness_factor: Some(1.0),
        clearcoat_textures: material::ClearcoatTextures::None,
        clearcoat_factor: Some(1.0),
        clearcoat_roughness_factor: Some(1.0),
        emissive: material::MaterialComponent::None,
        reflectance: material::MaterialComponent::None,
        anisotropy: material::MaterialComponent::None,
        uv_transform0: Mat3::IDENTITY,
        uv_transform1: Mat3::IDENTITY,
        unlit: false,
        sample_type: material::SampleType::Linear,
    })
}

/// Loads materials and textures from a [`gltf::Material`] iterator.
///
/// All binary data buffers must be provided. Call this with [`gltf::Document::materials`] as the materials argument.
///
/// io_func determines how URIs are resolved into their underlying data.
pub async fn load_materials_and_textures<F, Fut, E>(
    renderer: &Renderer,
    materials: impl ExactSizeIterator<Item = gltf::Material<'_>>,
    buffers: &[Vec<u8>],
    io_func: &mut F,
) -> Result<(Vec<Labeled<types::MaterialHandle>>, ImageMap), GltfLoadError<E>>
where
    F: FnMut(SsoString) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, E>>,
    E: std::error::Error + 'static,
{
    let mut images = ImageMap::default();
    let mut result = Vec::with_capacity(materials.len());
    for material in materials {
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
                Some(gltf::texture::MagFilter::Nearest) => material::SampleType::Nearest,
                Some(gltf::texture::MagFilter::Linear) => material::SampleType::Linear,
                None => material::SampleType::Linear,
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

        let handle = renderer.add_material(material::PbrMaterial {
            albedo: match albedo_tex {
                Some(tex) => material::AlbedoComponent::TextureVertexValue {
                    texture: tex,
                    value: Vec4::from(albedo_factor),
                    srgb: false,
                },
                None => material::AlbedoComponent::Value(Vec4::from(albedo_factor)),
            },
            transparency: match material.alpha_mode() {
                gltf::material::AlphaMode::Opaque => material::Transparency::Opaque,
                gltf::material::AlphaMode::Mask => material::Transparency::Cutout {
                    cutout: material.alpha_cutoff().unwrap_or(0.5),
                },
                gltf::material::AlphaMode::Blend => material::Transparency::Blend,
            },
            normal: match normals_tex {
                Some(tex) => material::NormalTexture::Tricomponent(tex),
                None => material::NormalTexture::None,
            },
            aomr_textures: match (metallic_roughness_tex, occlusion_tex) {
                (Some(mr), Some(ao)) if mr == ao => material::AoMRTextures::GltfCombined { texture: Some(mr) },
                (mr, ao) => material::AoMRTextures::GltfSplit {
                    mr_texture: mr,
                    ao_texture: ao,
                },
            },
            metallic_factor: Some(metallic_factor),
            roughness_factor: Some(roughness_factor),
            emissive: match emissive_tex {
                Some(tex) => material::MaterialComponent::TextureValue {
                    texture: tex,
                    value: Vec3::from(emissive_factor),
                },
                None => material::MaterialComponent::Value(Vec3::from(emissive_factor)),
            },
            uv_transform0: uv_transform,
            uv_transform1: uv_transform,
            unlit: material.unlit(),
            sample_type: nearest,
            ..material::PbrMaterial::default()
        });

        result.push(Labeled::new(handle, material.name()));
    }

    Ok((result, images))
}

/// Loads a single image from a [`gltf::Image`], with caching.
///
/// Uses the given ImageMap as a cache.
///
/// All binary data buffers must be provided. You can get the image from a texture by calling [`gltf::Texture::source`].
///
/// io_func determines how URIs are resolved into their underlying data.
pub async fn load_image_cached<F, Fut, E>(
    renderer: &Renderer,
    images: &mut ImageMap,
    image: gltf::Image<'_>,
    srgb: bool,
    buffers: &[Vec<u8>],
    io_func: &mut F,
) -> Result<Labeled<types::TextureHandle>, GltfLoadError<E>>
where
    F: FnMut(SsoString) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, E>>,
    E: std::error::Error + 'static,
{
    let key = ImageKey {
        index: image.index(),
        srgb,
    };

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
/// All binary data buffers must be provided. Call this with [`gltf::Document::materials`] as the materials argument.
///
/// io_func determines how URIs are resolved into their underlying data.
pub async fn load_image<F, Fut, E>(
    renderer: &Renderer,
    image: gltf::Image<'_>,
    srgb: bool,
    buffers: &[Vec<u8>],
    io_func: &mut F,
) -> Result<Labeled<types::TextureHandle>, GltfLoadError<E>>
where
    F: FnMut(SsoString) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, E>>,
    E: std::error::Error + 'static,
{
    // TODO: Address format detection for compressed texs
    let (data, uri) = match image.source() {
        gltf::image::Source::Uri { uri, .. } => {
            let data = io_func(SsoString::from(uri))
                .await
                .map_err(|e| GltfLoadError::TextureIo(SsoString::from(uri), e))?;
            (Cow::Owned(data), SsoString::from(uri))
        }
        gltf::image::Source::View { view, .. } => {
            let start = view.offset();
            let end = start + view.length();
            (
                Cow::Borrowed(&buffers[view.buffer().index()][start..end]),
                SsoString::from("<embedded>"),
            )
        }
    };

    let parsed = image::load_from_memory(&data).map_err(|e| GltfLoadError::TextureLoad(uri, e))?;
    let rgba = parsed.to_rgba8();
    let handle = renderer.add_texture_2d(types::Texture {
        label: image.name().map(str::to_owned),
        format: match srgb {
            true => types::TextureFormat::Rgba8UnormSrgb,
            false => types::TextureFormat::Rgba8Unorm,
        },
        size: UVec2::new(rgba.width(), rgba.height()),
        data: rgba.into_raw(),
        mip_count: types::MipmapCount::Maximum,
        mip_source: types::MipmapSource::Generated,
    });

    Ok(Labeled::new(handle, image.name()))
}

/// Implementation utilities.
pub mod util {
    use std::future::Future;

    use crate::Labeled;

    /// Turns a `Option<Future<Output = Result<Labeled<T>, E>>>>` into a `Future<Output = Result<Option<T>, E>>`
    ///
    /// This is a very specific transformation that shows up a lot when using [`load_image_cached`](super::load_image_cached).
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
}
