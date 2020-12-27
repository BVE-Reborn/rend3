use fnv::FnvHashMap;
use futures_util::future::OptionFuture;
use glam::{Mat4, Vec2, Vec3, Vec4, Vec4Swizzles};
use rend3::{datatypes as dt, datatypes::AffineTransform, Renderer};
use std::future::Future;
use thiserror::Error;

#[derive(Debug)]
pub struct MeshPrimitive {
    handle: dt::MeshHandle,
    material: usize,
}

#[derive(Debug)]
pub struct Mesh {
    primitives: Vec<MeshPrimitive>,
}

#[derive(Debug)]
pub struct Node {
    children: Vec<Node>,
    local_transform: Mat4,
    objects: Vec<dt::ObjectHandle>,
    light: Option<dt::DirectionalLightHandle>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct ImageKey {
    index: usize,
    srgb: bool,
}

#[derive(Debug, Default)]
pub struct LoadedGltfScene {
    meshes: FnvHashMap<usize, Mesh>,
    materials: FnvHashMap<usize, dt::MaterialHandle>,
    images: FnvHashMap<ImageKey, dt::TextureHandle>,
    nodes: Vec<Node>,
}

#[derive(Debug, Error)]
pub enum GltfLoadError {
    #[error("Gltf parsing or validation error")]
    Gltf(#[from] gltf::Error),
    #[error("Texture {0} failed to be loaded from the fs")]
    TextureIo(String, #[source] async_std::io::Error),
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

pub async fn load_gltf<TLD, F, Fut>(
    renderer: &Renderer<TLD>,
    data: &[u8],
    binary: &[u8],
    mut texture_func: F,
) -> Result<LoadedGltfScene, GltfLoadError>
where
    TLD: 'static,
    F: FnMut(&str) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, async_std::io::Error>>,
{
    let file = gltf::Gltf::from_slice_without_validation(data)?;

    let mut loaded = LoadedGltfScene::default();
    load_meshes(renderer, &mut loaded, file.meshes(), binary)?;
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

fn load_gltf_impl<'a, TLD>(
    renderer: &Renderer<TLD>,
    loaded: &mut LoadedGltfScene,
    nodes: impl Iterator<Item = gltf::Node<'a>>,
    parent_transform: Mat4,
) -> Result<Vec<Node>, GltfLoadError>
where
    TLD: 'static,
{
    let mut final_nodes = Vec::new();
    for node in nodes {
        let local_transform = Mat4::from_cols_array_2d(&node.transform().matrix());
        let transform = parent_transform * local_transform;

        let mut objects = Vec::new();
        if let Some(mesh) = node.mesh() {
            let mesh_handle = loaded
                .meshes
                .get(&mesh.index())
                .ok_or(GltfLoadError::MissingMesh(mesh.index()))?;
            for prim in &mesh_handle.primitives {
                let mat_idx = prim.material;
                let mat = loaded
                    .materials
                    .get(&mat_idx)
                    .ok_or(GltfLoadError::MissingMaterial(mat_idx))?;
                let object_handle = renderer.add_object(dt::Object {
                    mesh: prim.handle,
                    material: *mat,
                    transform: AffineTransform { transform },
                });
                objects.push(object_handle);
            }
        }

        let light = if let Some(light) = node.light() {
            match light.kind() {
                gltf::khr_lights_punctual::Kind::Directional => {
                    let direction = (transform * (-Vec3::unit_z()).extend(1.0)).xyz();
                    Some(renderer.add_directional_light(dt::DirectionalLight {
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

fn load_meshes<'a, TLD>(
    renderer: &Renderer<TLD>,
    loaded: &mut LoadedGltfScene,
    meshes: impl Iterator<Item = gltf::Mesh<'a>>,
    binary: &[u8],
) -> Result<(), GltfLoadError>
where
    TLD: 'static,
{
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

            let mut vertices: Vec<_> = reader
                .read_positions()
                .ok_or(GltfLoadError::MissingPositions(mesh.index()))?
                .map(|pos| rend3::datatypes::ModelVertex {
                    position: Vec3::from(pos),
                    normal: Default::default(),
                    uv: Default::default(),
                    color: [0; 4],
                })
                .collect();

            let has_normals = if let Some(normals) = reader.read_normals() {
                vertices.iter_mut().zip(normals).for_each(|(vert, normal)| {
                    vert.normal = Vec3::from(normal);
                });
                true
            } else {
                false
            };
            if let Some(coords) = reader.read_tex_coords(0) {
                vertices.iter_mut().zip(coords.into_f32()).for_each(|(vert, coord)| {
                    vert.uv = Vec2::from(coord);
                });
            }
            if let Some(colors) = reader.read_colors(0) {
                vertices
                    .iter_mut()
                    .zip(colors.into_rgba_u8())
                    .for_each(|(vert, color)| {
                        vert.color = color;
                    });
            }

            let indices: Vec<_> = if let Some(indices) = reader.read_indices() {
                indices.into_u32().collect()
            } else {
                (0..vertices.len() as u32).collect()
            };

            let mut mesh = dt::Mesh { vertices, indices };
            if !has_normals {
                mesh.calculate_normals();
            }

            let handle = renderer.add_mesh(mesh);

            res_prims.push(MeshPrimitive {
                handle,
                // TODO: handle default material
                material: prim.material().index().unwrap(),
            })
        }
        loaded.meshes.insert(mesh.index(), Mesh { primitives: res_prims });
    }

    Ok(())
}

pub async fn load_materials_and_textures<'a, TLD, F, Fut>(
    renderer: &Renderer<TLD>,
    loaded: &mut LoadedGltfScene,
    materials: impl Iterator<Item = gltf::Material<'a>>,
    texture_func: &mut F,
) -> Result<(), GltfLoadError>
where
    TLD: 'static,
    F: FnMut(&str) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, async_std::io::Error>>,
{
    for material in materials {
        let pbr = material.pbr_metallic_roughness();
        let albedo = pbr.base_color_texture();
        let albedo_factor = pbr.base_color_factor();
        let occlusion = material.occlusion_texture();
        // TODO: implement emissive
        let _emissive = material.emissive_texture();
        let _emissive_factor = material.emissive_factor();
        // TODO: implement scaling
        let normals = material.normal_texture();
        let roughness_factor = pbr.roughness_factor();
        let metallic_factor = pbr.metallic_factor();
        let metallic_roughness = pbr.metallic_roughness_texture();

        let albedo_tex =
            OptionFuture::from(albedo.map(|i| load_image(renderer, loaded, i.texture().source(), true, texture_func)))
                .await
                .transpose()?;
        let occlusion_tex = OptionFuture::from(
            occlusion.map(|i| load_image(renderer, loaded, i.texture().source(), false, texture_func)),
        )
        .await
        .transpose()?;
        let _emissive_tex = OptionFuture::from(
            _emissive.map(|i| load_image(renderer, loaded, i.texture().source(), false, texture_func)),
        )
        .await
        .transpose()?;
        let normals_tex = OptionFuture::from(
            normals.map(|i| load_image(renderer, loaded, i.texture().source(), false, texture_func)),
        )
        .await
        .transpose()?;
        let metallic_roughness_tex = OptionFuture::from(
            metallic_roughness.map(|i| load_image(renderer, loaded, i.texture().source(), false, texture_func)),
        )
        .await
        .transpose()?;

        let handle = renderer.add_material(dt::Material {
            albedo: match albedo_tex {
                // TODO: Allow texture * value
                Some(tex) => dt::AlbedoComponent::Texture(tex),
                None => dt::AlbedoComponent::Value(Vec4::from(albedo_factor)),
            },
            normal: match normals_tex {
                Some(tex) => dt::NormalTexture::Tricomponent(tex),
                None => dt::NormalTexture::None,
            },
            aomr_textures: match (metallic_roughness_tex, occlusion_tex) {
                (Some(mr), Some(ao)) if mr == ao => dt::AoMRTextures::GltfCombined { texture: Some(mr) },
                (mr, ao) => dt::AoMRTextures::GltfSplit {
                    mr_texture: mr,
                    ao_texture: ao,
                },
            },
            roughness_factor: Some(roughness_factor),
            metallic_factor: Some(metallic_factor),
            ..dt::Material::default()
        });

        // TODO: why is this unwrap needed
        loaded.materials.insert(material.index().unwrap(), handle);
    }

    Ok(())
}

pub async fn load_image<'a, TLD, F, Fut>(
    renderer: &Renderer<TLD>,
    loaded: &mut LoadedGltfScene,
    image: gltf::Image<'a>,
    srgb: bool,
    texture_func: &mut F,
) -> Result<dt::TextureHandle, GltfLoadError>
where
    TLD: 'static,
    F: FnMut(&str) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, async_std::io::Error>>,
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
        let handle = renderer.add_texture_2d(dt::Texture {
            label: image.name().map(str::to_owned),
            format: match srgb {
                true => dt::RendererTextureFormat::Rgba8Srgb,
                false => dt::RendererTextureFormat::Rgba8Linear,
            },
            width: rgba.width(),
            height: rgba.height(),
            data: rgba.into_raw(),
        });

        loaded.images.insert(key, handle);

        Ok(handle)
    } else {
        unimplemented!()
    }
}
