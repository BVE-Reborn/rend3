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
use image::GenericImageView;
use rend3::{
    types::{self, ObjectHandle},
    util::typedefs::{FastHashMap, SsoString},
    Renderer,
};
use rend3_pbr::material;
use std::{borrow::Cow, collections::hash_map::Entry, future::Future, num::NonZeroU32, path::Path};
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

/// Format
#[derive(Debug, Clone)]
pub struct Texture {
    pub handle: types::TextureHandle,
    pub format: types::TextureFormat,
}

/// Hashmap which stores a mapping from [`ImageKey`] to a labeled handle.
pub type ImageMap = FastHashMap<ImageKey, Labeled<Texture>>;

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
    TextureDecode(SsoString, #[source] image::ImageError),
    #[error("Texture {0} failed to be loaded as a ktx2 format due to incompatible format {1:?}")]
    TextureBadKxt2Format(SsoString, ktx2::Format),
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
        profiling::scope!("decoding base64 uri");
        log::info!("loading {} bytes of base64 data", data.len());
        // TODO: errors
        Ok(base64::decode(data).unwrap())
    } else {
        let path_resolved = parent_director.as_ref().join(&*uri);
        let display = path_resolved.as_os_str().to_string_lossy();
        profiling::scope!("loading file", &display);
        log::info!("loading file '{}' from disk", &display);
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
    profiling::scope!("loading gltf");
    let mut file = {
        profiling::scope!("parsing gltf");
        gltf::Gltf::from_slice_without_validation(data)?
    };
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
    profiling::scope!("loading buffers");
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
    profiling::scope!("creating default material");
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
    profiling::scope!("loading materials and textures");

    let mut images = ImageMap::default();
    let mut result = Vec::with_capacity(materials.len());
    for material in materials {
        profiling::scope!("load material", material.name().unwrap_or_default());

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
                    texture: util::extract_handle(tex),
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
                Some(tex) => material::NormalTexture::Bicomponent(util::extract_handle(tex)),
                None => material::NormalTexture::None,
            },
            aomr_textures: match (metallic_roughness_tex, occlusion_tex) {
                (Some(mr), Some(ao)) if mr == ao => material::AoMRTextures::Combined {
                    texture: Some(util::extract_handle(mr)),
                },
                (mr, ao) if ao.map(|ao| ao.format.describe().components < 3).unwrap_or(false) => {
                    material::AoMRTextures::Split {
                        mr_texture: util::extract_handle(mr),
                        ao_texture: util::extract_handle(ao),
                    }
                }
                (mr, ao) => material::AoMRTextures::SwizzledSplit {
                    mr_texture: util::extract_handle(mr),
                    ao_texture: util::extract_handle(ao),
                },
            },
            metallic_factor: Some(metallic_factor),
            roughness_factor: Some(roughness_factor),
            emissive: match emissive_tex {
                Some(tex) => material::MaterialComponent::TextureValue {
                    texture: util::extract_handle(tex),
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
) -> Result<Labeled<Texture>, GltfLoadError<E>>
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
) -> Result<Labeled<Texture>, GltfLoadError<E>>
where
    F: FnMut(SsoString) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, E>>,
    E: std::error::Error + 'static,
{
    profiling::scope!("load image", image.name().unwrap_or_default());
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

    let texture = if let Ok(reader) = ktx2::Reader::new(&data) {
        profiling::scope!("parsing ktx2");

        let header = reader.header();

        let src_format = header.format.unwrap();
        let format = util::map_ktx2_format(src_format, srgb).ok_or_else(|| {
            GltfLoadError::TextureBadKxt2Format(image.name().map(SsoString::from).unwrap_or_default(), src_format)
        })?;

        let guaranteed_format = format.describe().guaranteed_format_features;
        let generate = header.level_count == 1
            && guaranteed_format.filterable
            && guaranteed_format.allowed_usages.contains(
                rend3::types::TextureUsages::TEXTURE_BINDING | rend3::types::TextureUsages::RENDER_ATTACHMENT,
            );

        types::Texture {
            label: image.name().map(str::to_owned),
            format,
            size: UVec2::new(header.pixel_width, header.pixel_height),
            data: reader.data().to_vec(),
            mip_count: if generate {
                types::MipmapCount::Maximum
            } else {
                types::MipmapCount::Specific(NonZeroU32::new(header.level_count).unwrap())
            },
            mip_source: if generate {
                types::MipmapSource::Generated
            } else {
                types::MipmapSource::Uploaded
            },
        }
    } else {
        profiling::scope!("decoding image");
        let parsed = image::load_from_memory(&data).map_err(|e| GltfLoadError::TextureDecode(uri, e))?;
        let size = UVec2::new(parsed.width(), parsed.height());
        let (data, format) = util::convert_dynamic_image(parsed, srgb);

        types::Texture {
            label: image.name().map(str::to_owned),
            format,
            size,
            data,
            mip_count: types::MipmapCount::Maximum,
            mip_source: types::MipmapSource::Generated,
        }
    };
    let format = texture.format;
    let handle = renderer.add_texture_2d(texture);

    Ok(Labeled::new(Texture { handle, format }, image.name()))
}

/// Implementation utilities.
pub mod util {
    use std::future::Future;

    use image::{buffer::ConvertBuffer, Bgra, ImageBuffer, Luma, Rgba};
    use rend3::types;

    use crate::{Labeled, Texture};

    /// Turns an `Option<Texture>` into `Option<types::TextureHandle>`
    pub fn extract_handle(texture: Option<Texture>) -> Option<types::TextureHandle> {
        textures.map(|t| t.handle)
    }

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

    pub fn convert_dynamic_image(image: image::DynamicImage, srgb: bool) -> (Vec<u8>, rend3::types::TextureFormat) {
        use rend3::types::TextureFormat as r3F;

        profiling::scope!("convert dynamic image");
        match image {
            image::DynamicImage::ImageLuma8(i) => (i.into_raw(), r3F::R8Unorm),
            image::DynamicImage::ImageLumaA8(i) => (
                ConvertBuffer::<ImageBuffer<Luma<u8>, Vec<u8>>>::convert(&i).into_raw(),
                r3F::R8Unorm,
            ),
            image::DynamicImage::ImageRgb8(i) => (
                ConvertBuffer::<ImageBuffer<Rgba<u8>, Vec<u8>>>::convert(&i).into_raw(),
                if srgb { r3F::Rgba8UnormSrgb } else { r3F::Rgba8Unorm },
            ),
            image::DynamicImage::ImageRgba8(i) => (i.into_raw(), r3F::Rgba8Unorm),
            image::DynamicImage::ImageBgr8(i) => (
                ConvertBuffer::<ImageBuffer<Bgra<u8>, Vec<u8>>>::convert(&i).into_raw(),
                if srgb { r3F::Bgra8UnormSrgb } else { r3F::Bgra8Unorm },
            ),
            image::DynamicImage::ImageBgra8(i) => {
                (i.into_raw(), if srgb { r3F::Bgra8UnormSrgb } else { r3F::Bgra8Unorm })
            }
            i => (
                i.into_rgba8().into_raw(),
                if srgb { r3F::Rgba8UnormSrgb } else { r3F::Rgba8Unorm },
            ),
        }
    }

    /// Maps a ktx2 format into the rend3's TextureFormat
    pub fn map_ktx2_format(format: ktx2::Format, srgb: bool) -> Option<rend3::types::TextureFormat> {
        use ktx2::Format as k2F;
        use rend3::types::TextureFormat as r3F;
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
            k2F::A2R10G10B10_UNORM_PACK32
            | k2F::A2R10G10B10_SNORM_PACK32
            | k2F::A2R10G10B10_UINT_PACK32
            | k2F::A2R10G10B10_SINT_PACK32
            | k2F::A2B10G10R10_UNORM_PACK32
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
            k2F::R16G16B16A16_UNORM | k2F::R16G16B16A16_SNORM => return None,
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
            k2F::B10G11R11_UFLOAT_PACK32 => r3F::Rg11b10Float,
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
            k2F::BC6H_SFLOAT_BLOCK => r3F::Bc6hRgbSfloat,
            k2F::BC7_UNORM_BLOCK | k2F::BC7_SRGB_BLOCK => {
                if srgb {
                    r3F::Bc7RgbaUnormSrgb
                } else {
                    r3F::Bc7RgbaUnorm
                }
            }
            k2F::ETC2_R8G8B8_UNORM_BLOCK | k2F::ETC2_R8G8B8_SRGB_BLOCK => {
                if srgb {
                    r3F::Etc2RgbUnormSrgb
                } else {
                    r3F::Etc2RgbUnorm
                }
            }
            k2F::ETC2_R8G8B8A1_UNORM_BLOCK | k2F::ETC2_R8G8B8A1_SRGB_BLOCK => {
                if srgb {
                    r3F::Etc2RgbA1UnormSrgb
                } else {
                    r3F::Etc2RgbA1Unorm
                }
            }
            k2F::ETC2_R8G8B8A8_UNORM_BLOCK | k2F::ETC2_R8G8B8A8_SRGB_BLOCK => return None,
            k2F::EAC_R11_UNORM_BLOCK => r3F::EacRUnorm,
            k2F::EAC_R11_SNORM_BLOCK => r3F::EacRSnorm,
            k2F::EAC_R11G11_UNORM_BLOCK => r3F::EacRgUnorm,
            k2F::EAC_R11G11_SNORM_BLOCK => r3F::EacRgSnorm,
            k2F::ASTC_4x4_UNORM_BLOCK | k2F::ASTC_4x4_SRGB_BLOCK => {
                if srgb {
                    r3F::Astc4x4RgbaUnormSrgb
                } else {
                    r3F::Astc4x4RgbaUnorm
                }
            }
            k2F::ASTC_5x4_UNORM_BLOCK | k2F::ASTC_5x4_SRGB_BLOCK => {
                if srgb {
                    r3F::Astc5x4RgbaUnormSrgb
                } else {
                    r3F::Astc5x4RgbaUnorm
                }
            }
            k2F::ASTC_5x5_UNORM_BLOCK | k2F::ASTC_5x5_SRGB_BLOCK => {
                if srgb {
                    r3F::Astc5x5RgbaUnormSrgb
                } else {
                    r3F::Astc5x5RgbaUnorm
                }
            }
            k2F::ASTC_6x5_UNORM_BLOCK | k2F::ASTC_6x5_SRGB_BLOCK => {
                if srgb {
                    r3F::Astc6x5RgbaUnormSrgb
                } else {
                    r3F::Astc6x5RgbaUnorm
                }
            }
            k2F::ASTC_6x6_UNORM_BLOCK | k2F::ASTC_6x6_SRGB_BLOCK => {
                if srgb {
                    r3F::Astc6x6RgbaUnormSrgb
                } else {
                    r3F::Astc6x6RgbaUnorm
                }
            }
            k2F::ASTC_8x5_UNORM_BLOCK | k2F::ASTC_8x5_SRGB_BLOCK => {
                if srgb {
                    r3F::Astc8x5RgbaUnormSrgb
                } else {
                    r3F::Astc8x5RgbaUnorm
                }
            }
            k2F::ASTC_8x6_UNORM_BLOCK | k2F::ASTC_8x6_SRGB_BLOCK => {
                if srgb {
                    r3F::Astc8x6RgbaUnormSrgb
                } else {
                    r3F::Astc8x6RgbaUnorm
                }
            }
            k2F::ASTC_8x8_UNORM_BLOCK | k2F::ASTC_8x8_SRGB_BLOCK => {
                if srgb {
                    r3F::Astc8x8RgbaUnormSrgb
                } else {
                    r3F::Astc8x8RgbaUnorm
                }
            }
            k2F::ASTC_10x5_UNORM_BLOCK | k2F::ASTC_10x5_SRGB_BLOCK => {
                if srgb {
                    r3F::Astc10x5RgbaUnormSrgb
                } else {
                    r3F::Astc10x5RgbaUnorm
                }
            }
            k2F::ASTC_10x6_UNORM_BLOCK | k2F::ASTC_10x6_SRGB_BLOCK => {
                if srgb {
                    r3F::Astc10x6RgbaUnormSrgb
                } else {
                    r3F::Astc10x6RgbaUnorm
                }
            }
            k2F::ASTC_10x8_UNORM_BLOCK | k2F::ASTC_10x8_SRGB_BLOCK => {
                if srgb {
                    r3F::Astc10x8RgbaUnormSrgb
                } else {
                    r3F::Astc10x8RgbaUnorm
                }
            }
            k2F::ASTC_10x10_UNORM_BLOCK | k2F::ASTC_10x10_SRGB_BLOCK => {
                if srgb {
                    r3F::Astc10x10RgbaUnormSrgb
                } else {
                    r3F::Astc10x10RgbaUnorm
                }
            }
            k2F::ASTC_12x10_UNORM_BLOCK | k2F::ASTC_12x10_SRGB_BLOCK => {
                if srgb {
                    r3F::Astc12x10RgbaUnormSrgb
                } else {
                    r3F::Astc12x10RgbaUnorm
                }
            }
            k2F::ASTC_12x12_UNORM_BLOCK | k2F::ASTC_12x12_SRGB_BLOCK => {
                if srgb {
                    r3F::Astc12x12RgbaUnormSrgb
                } else {
                    r3F::Astc12x12RgbaUnorm
                }
            }
            _ => return None,
        })
    }
}
