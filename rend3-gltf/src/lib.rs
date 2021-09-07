use fnv::FnvHashMap;
use glam::{Mat3, Mat4, UVec2, Vec2, Vec3, Vec4, Vec4Swizzles};
use gltf::buffer::Source;
use rend3::{types, util::typedefs::SsoString, Renderer};
use std::{borrow::Cow, collections::hash_map::Entry, future::Future, path::Path};
use thiserror::Error;

#[derive(Debug)]
pub struct MeshPrimitive {
    pub handle: types::MeshHandle,
    pub material: Option<usize>,
}

#[derive(Debug)]
pub struct Mesh {
    pub primitives: Vec<MeshPrimitive>,
}

#[derive(Debug)]
pub struct Node {
    pub children: Vec<Node>,
    pub local_transform: Mat4,
    pub objects: Vec<types::ObjectHandle>,
    pub light: Option<types::DirectionalLightHandle>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct ImageKey {
    pub index: usize,
    pub srgb: bool,
}

#[derive(Debug, Default)]
pub struct LoadedGltfScene {
    pub meshes: FnvHashMap<usize, Mesh>,
    pub materials: FnvHashMap<Option<usize>, types::MaterialHandle>,
    pub images: FnvHashMap<ImageKey, types::TextureHandle>,
    pub nodes: Vec<Node>,
}

#[derive(Debug, Error)]
pub enum GltfLoadError<E: std::error::Error + 'static> {
    #[error("Gltf parsing or validation error")]
    Gltf(#[from] gltf::Error),
    #[error("Buffer {0} failed to be loaded from the fs")]
    BufferIo(String, #[source] E),
    #[error("Texture {0} failed to be loaded from the fs")]
    TextureIo(String, #[source] E),
    #[error("Texture {0} failed to be loaded as an image")]
    TextureLoad(String, #[source] image::ImageError),
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

/// Default implementation of [`load_gltf`]'s `io_func`.
///
/// The first argumnet is the directory all relative paths should be considered against. This is more than likely
/// the directory the gltf/glb is in.
pub async fn filesystem_io_func(parent_director: impl AsRef<Path>, uri: SsoString) -> Result<Vec<u8>, std::io::Error> {
    let octet_stream_header = "data:";
    if let Some(base64_data) = uri.strip_prefix(octet_stream_header) {
        let (_mime, rest) = base64_data.split_once(";").unwrap();
        let (encoding, data) = rest.split_once(",").unwrap();
        assert_eq!(encoding, "base64");
        // TODO: errors
        Ok(base64::decode(data).unwrap())
    } else {
        let tex_resolved = parent_director.as_ref().join(&*uri);
        std::fs::read(tex_resolved)
    }
}

/// Load a given gltf's data into the renderer's world. Allows the user to specify how URIs are resolved into their underlying data. Supports most gltfs and glbs.
///
/// ```no_run
/// # use std::path::Path;
/// # let renderer = unimplemented!();
/// let path = Path::new("some/path/scene.gltf"); // or glb
/// let gltf_data = std::fs::read(&path).unwrap();
/// let parent_directory = path.parent().unwrap();
/// pollster::block_on(rend3_gltf::load_gltf(&renderer, &gltf_data, |p| rend3_gltf::filesystem_io_func(&parent_directory, p)));
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

    let mut buffers = Vec::with_capacity(file.buffers().len());
    let mut blob_index = None;
    for b in file.buffers() {
        let data = match b.source() {
            Source::Bin => {
                blob_index = Some(b.index());
                Vec::new()
            }
            Source::Uri(uri) => io_func(SsoString::from(uri))
                .await
                .map_err(|e| GltfLoadError::BufferIo(uri.to_string(), e))?,
        };
        buffers.push(data);
    }
    if let Some(blob_index) = blob_index {
        buffers[blob_index] = file.blob.take().expect("glb blob not found, but gltf expected it");
    }

    let mut loaded = LoadedGltfScene::default();
    load_meshes(renderer, &mut loaded, file.meshes(), &buffers)?;
    load_default_material(renderer, &mut loaded);
    load_materials_and_textures(renderer, &mut loaded, file.materials(), &buffers, &mut io_func).await?;

    let scene = file
        .default_scene()
        .or_else(|| file.scenes().next())
        .ok_or(GltfLoadError::MissingScene)?;

    loaded.nodes = load_gltf_impl(
        renderer,
        &mut loaded,
        scene.nodes(),
        Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0)),
    )?;

    Ok(loaded)
}

fn load_gltf_impl<'a, E: std::error::Error + 'static>(
    renderer: &Renderer,
    loaded: &mut LoadedGltfScene,
    nodes: impl Iterator<Item = gltf::Node<'a>>,
    parent_transform: Mat4,
) -> Result<Vec<Node>, GltfLoadError<E>> {
    let mut final_nodes = Vec::new();
    for node in nodes {
        let local_transform = Mat4::from_cols_array_2d(&node.transform().matrix());
        let transform = parent_transform * local_transform;

        let mut objects = Vec::new();
        if let Some(mesh) = node.mesh() {
            let mesh_handle = loaded
                .meshes
                .get(&mesh.index())
                .ok_or_else(|| GltfLoadError::MissingMesh(mesh.index()))?;
            for prim in &mesh_handle.primitives {
                let mat_idx = prim.material;
                let mat = loaded
                    .materials
                    .get(&mat_idx)
                    .ok_or_else(|| GltfLoadError::MissingMaterial(mat_idx.expect("Could not find default material")))?;
                let object_handle = renderer.add_object(types::Object {
                    mesh: prim.handle.clone(),
                    material: mat.clone(),
                    transform,
                });
                objects.push(object_handle);
            }
        }

        let light = if let Some(light) = node.light() {
            match light.kind() {
                gltf::khr_lights_punctual::Kind::Directional => {
                    let direction = (transform * (-Vec3::Z).extend(1.0)).xyz();
                    Some(renderer.add_directional_light(types::DirectionalLight {
                        color: Vec3::from(light.color()),
                        intensity: light.intensity(),
                        direction,
                    }))
                }
                _ => None,
            }
        } else {
            None
        };

        let children = load_gltf_impl(renderer, loaded, node.children(), transform)?;

        final_nodes.push(Node {
            children,
            local_transform,
            objects,
            light,
        })
    }
    Ok(final_nodes)
}

fn load_meshes<'a, E: std::error::Error + 'static>(
    renderer: &Renderer,
    loaded: &mut LoadedGltfScene,
    meshes: impl Iterator<Item = gltf::Mesh<'a>>,
    buffers: &[Vec<u8>],
) -> Result<(), GltfLoadError<E>> {
    for mesh in meshes {
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
                builder = builder.with_vertex_uvs(uvs.into_f32().map(Vec2::from).collect())
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
        loaded.meshes.insert(mesh.index(), Mesh { primitives: res_prims });
    }

    Ok(())
}

fn load_default_material(renderer: &Renderer, loaded: &mut LoadedGltfScene) {
    loaded.materials.insert(
        None,
        renderer.add_material(types::Material {
            albedo: types::AlbedoComponent::Value(Vec4::splat(1.0)),
            transparency: types::Transparency::Opaque,
            normal: types::NormalTexture::None,
            aomr_textures: types::AoMRTextures::None,
            ao_factor: Some(1.0),
            metallic_factor: Some(1.0),
            roughness_factor: Some(1.0),
            clearcoat_textures: types::ClearcoatTextures::None,
            clearcoat_factor: Some(1.0),
            clearcoat_roughness_factor: Some(1.0),
            emissive: types::MaterialComponent::None,
            reflectance: types::MaterialComponent::None,
            anisotropy: types::MaterialComponent::None,
            transform: Mat3::IDENTITY,
            unlit: false,
            sample_type: types::SampleType::Linear,
        }),
    );
}

async fn load_materials_and_textures<'a, F, Fut, E>(
    renderer: &Renderer,
    loaded: &mut LoadedGltfScene,
    materials: impl Iterator<Item = gltf::Material<'a>>,
    buffers: &[Vec<u8>],
    io_func: &mut F,
) -> Result<(), GltfLoadError<E>>
where
    F: FnMut(SsoString) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, E>>,
    E: std::error::Error + 'static,
{
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
                Some(gltf::texture::MagFilter::Nearest) => types::SampleType::Nearest,
                Some(gltf::texture::MagFilter::Linear) => types::SampleType::Linear,
                None => types::SampleType::Linear,
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

        let albedo_tex =
            option_resolve(albedo.map(|i| load_image(renderer, loaded, i.texture().source(), true, buffers, io_func)))
                .await
                .transpose()?;
        let occlusion_tex = option_resolve(
            occlusion.map(|i| load_image(renderer, loaded, i.texture().source(), false, buffers, io_func)),
        )
        .await
        .transpose()?;
        let emissive_tex = option_resolve(
            emissive.map(|i| load_image(renderer, loaded, i.texture().source(), true, buffers, io_func)),
        )
        .await
        .transpose()?;
        let normals_tex = option_resolve(
            normals.map(|i| load_image(renderer, loaded, i.texture().source(), false, buffers, io_func)),
        )
        .await
        .transpose()?;
        let metallic_roughness_tex = option_resolve(
            metallic_roughness.map(|i| load_image(renderer, loaded, i.texture().source(), false, buffers, io_func)),
        )
        .await
        .transpose()?;

        let handle = renderer.add_material(types::Material {
            albedo: match albedo_tex {
                Some(tex) => types::AlbedoComponent::TextureVertexValue {
                    texture: tex,
                    value: Vec4::from(albedo_factor),
                    srgb: false,
                },
                None => types::AlbedoComponent::Value(Vec4::from(albedo_factor)),
            },
            transparency: match material.alpha_mode() {
                gltf::material::AlphaMode::Opaque => types::Transparency::Opaque,
                gltf::material::AlphaMode::Mask => types::Transparency::Cutout {
                    cutout: material.alpha_cutoff().unwrap_or(0.5),
                },
                gltf::material::AlphaMode::Blend => types::Transparency::Blend,
            },
            normal: match normals_tex {
                Some(tex) => types::NormalTexture::Tricomponent(tex),
                None => types::NormalTexture::None,
            },
            aomr_textures: match (metallic_roughness_tex, occlusion_tex) {
                (Some(mr), Some(ao)) if mr == ao => types::AoMRTextures::GltfCombined { texture: Some(mr) },
                (mr, ao) => types::AoMRTextures::GltfSplit {
                    mr_texture: mr,
                    ao_texture: ao,
                },
            },
            metallic_factor: Some(metallic_factor),
            roughness_factor: Some(roughness_factor),
            emissive: match emissive_tex {
                Some(tex) => types::MaterialComponent::TextureValue {
                    texture: tex,
                    value: Vec3::from(emissive_factor),
                },
                None => types::MaterialComponent::Value(Vec3::from(emissive_factor)),
            },
            transform: uv_transform,
            unlit: material.unlit(),
            sample_type: nearest,
            ..types::Material::default()
        });

        loaded
            .materials
            .insert(Some(material.index().expect("unexpected default material")), handle);
    }

    Ok(())
}

async fn load_image<F, Fut, E>(
    renderer: &Renderer,
    loaded: &mut LoadedGltfScene,
    image: gltf::Image<'_>,
    srgb: bool,
    buffers: &[Vec<u8>],
    io_func: &mut F,
) -> Result<types::TextureHandle, GltfLoadError<E>>
where
    F: FnMut(SsoString) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, E>>,
    E: std::error::Error + 'static,
{
    let key = ImageKey {
        index: image.index(),
        srgb,
    };

    let entry = match loaded.images.entry(key) {
        Entry::Occupied(handle) => return Ok(handle.get().clone()),
        Entry::Vacant(v) => v,
    };

    // TODO: Address format detection for compressed texs
    // TODO: Allow embedded images
    let (data, uri) = match image.source() {
        gltf::image::Source::Uri { uri, .. } => {
            let data = io_func(SsoString::from(uri))
                .await
                .map_err(|e| GltfLoadError::TextureIo(uri.to_string(), e))?;
            (Cow::Owned(data), uri.to_string())
        }
        gltf::image::Source::View { view, .. } => {
            let start = view.offset();
            let end = start + view.length();
            (
                Cow::Borrowed(&buffers[view.buffer().index()][start..end]),
                String::from("<embedded>"),
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

    entry.insert(handle.clone());

    Ok(handle)
}

pub async fn option_resolve<F: Future>(fut: Option<F>) -> Option<F::Output> {
    if let Some(f) = fut {
        Some(f.await)
    } else {
        None
    }
}
