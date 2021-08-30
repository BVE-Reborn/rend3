use fnv::FnvHashMap;
use glam::{Mat3, Mat4, Vec2, Vec3, Vec4, Vec4Swizzles};
use rend3::{
    types::{self, MeshBuilder, Transparency},
    Renderer,
};
use std::future::Future;
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

pub async fn load_gltf<F, Fut, E>(
    renderer: &Renderer,
    data: &[u8],
    binary: &[u8],
    mut texture_func: F,
) -> Result<LoadedGltfScene, GltfLoadError<E>>
where
    F: FnMut(&str) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, E>>,
    E: std::error::Error + 'static,
{
    let file = gltf::Gltf::from_slice_without_validation(data)?;

    let mut loaded = LoadedGltfScene::default();
    load_meshes(renderer, &mut loaded, file.meshes(), binary)?;
    load_default_material(renderer, &mut loaded);
    load_materials_and_textures(renderer, &mut loaded, file.materials(), &mut texture_func).await?;

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
    binary: &[u8],
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

            let reader = prim.reader(|b| {
                if b.index() != 0 {
                    return None;
                }
                Some(&binary[..b.length()])
            });

            let vertex_positions: Vec<_> = reader
                .read_positions()
                .ok_or_else(|| GltfLoadError::MissingPositions(mesh.index()))?
                .map(Vec3::from)
                .collect();

            // glTF models are right handed, so we must flip their winding order
            let mut builder = MeshBuilder::new(vertex_positions).with_right_handed();

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
            transparency: Transparency::Opaque,
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
    texture_func: &mut F,
) -> Result<(), GltfLoadError<E>>
where
    F: FnMut(&str) -> Fut,
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

        let albedo_tex =
            option_resolve(albedo.map(|i| load_image(renderer, loaded, i.texture().source(), true, texture_func)))
                .await
                .transpose()?;
        let occlusion_tex =
            option_resolve(occlusion.map(|i| load_image(renderer, loaded, i.texture().source(), false, texture_func)))
                .await
                .transpose()?;
        let emissive_tex =
            option_resolve(emissive.map(|i| load_image(renderer, loaded, i.texture().source(), true, texture_func)))
                .await
                .transpose()?;
        let normals_tex =
            option_resolve(normals.map(|i| load_image(renderer, loaded, i.texture().source(), false, texture_func)))
                .await
                .transpose()?;
        let metallic_roughness_tex = option_resolve(
            metallic_roughness.map(|i| load_image(renderer, loaded, i.texture().source(), false, texture_func)),
        )
        .await
        .transpose()?;

        let handle = renderer.add_material(types::Material {
            albedo: match albedo_tex {
                Some(tex) => types::AlbedoComponent::TextureValue {
                    texture: tex,
                    value: Vec4::from(albedo_factor),
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
            roughness_factor: Some(roughness_factor),
            metallic_factor: Some(metallic_factor),
            emissive: match emissive_tex {
                Some(tex) => types::MaterialComponent::TextureValue {
                    texture: tex,
                    value: Vec3::from(emissive_factor),
                },
                None => types::MaterialComponent::Value(Vec3::from(emissive_factor)),
            },
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
    texture_func: &mut F,
) -> Result<types::TextureHandle, GltfLoadError<E>>
where
    F: FnMut(&str) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, E>>,
    E: std::error::Error + 'static,
{
    // TODO: Address format detection for compressed texs
    // TODO: Allow embedded images
    if let gltf::image::Source::Uri { uri, .. } = image.source() {
        let key = ImageKey {
            index: image.index(),
            srgb,
        };

        let data = texture_func(uri)
            .await
            .map_err(|e| GltfLoadError::TextureIo(uri.to_string(), e))?;
        let parsed = image::load_from_memory(&data).map_err(|e| GltfLoadError::TextureLoad(uri.to_string(), e))?;
        let rgba = parsed.to_rgba8();
        let handle = renderer.add_texture_2d(types::Texture {
            label: image.name().map(str::to_owned),
            format: match srgb {
                true => types::TextureFormat::Rgba8UnormSrgb,
                false => types::TextureFormat::Rgba8Unorm,
            },
            width: rgba.width(),
            height: rgba.height(),
            data: rgba.into_raw(),
            /// TODO: automatic mipmapping (#53)
            mip_levels: 1,
        });

        loaded.images.insert(key, handle.clone());

        Ok(handle)
    } else {
        unimplemented!()
    }
}

pub async fn option_resolve<F: Future>(fut: Option<F>) -> Option<F::Output> {
    if let Some(f) = fut {
        Some(f.await)
    } else {
        None
    }
}
