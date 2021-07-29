use crate::{
    cache::BindGroupCache,
    datatypes::{Material, MaterialChange, MaterialFlags, MaterialHandle, TextureHandle},
    mode::ModeData,
    resources::TextureManager,
    util::{bind_merge::BindGroupBuilder, buffer::WrappedPotBuffer, registry::ResourceRegistry},
    RendererMode,
};
use glam::{Vec3, Vec4};
use std::{mem::size_of, num::{NonZeroU32, NonZeroU64}, sync::Arc};
use wgpu::{BindGroup, BindGroupLayout, BindingResource, BindingType, Buffer, BufferBindingType, BufferUsage, Device, Queue, ShaderStage, TextureSampleType, TextureViewDimension, util::{BufferInitDescriptor, DeviceExt}};

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
pub struct CPUShaderMaterial {
    uv_transform_row0: Vec4,
    uv_transform_row1: Vec4,
    uv_transform_row2: Vec4,
    albedo: Vec4,
    emissive: Vec3,
    roughness: f32,
    metallic: f32,
    reflectance: f32,
    clear_coat: f32,
    clear_coat_roughness: f32,
    anisotropy: f32,
    ambient_occlusion: f32,
    alpha_cutout: f32,

    texture_enable: u32,
    material_flags: MaterialFlags,
}

unsafe impl bytemuck::Zeroable for CPUShaderMaterial {}
unsafe impl bytemuck::Pod for CPUShaderMaterial {}

impl CPUShaderMaterial {
    pub fn from_material(material: &Material) -> Self {
        Self {
            uv_transform_row0: material.transform.x_axis.extend(0.0),
            uv_transform_row1: material.transform.y_axis.extend(0.0),
            uv_transform_row2: material.transform.z_axis.extend(0.0),
            albedo: material.albedo.to_value(),
            roughness: material.roughness_factor.unwrap_or(0.0),
            metallic: material.metallic_factor.unwrap_or(0.0),
            reflectance: material.reflectance.to_value(0.5),
            clear_coat: material.clearcoat_factor.unwrap_or(0.0),
            clear_coat_roughness: material.clearcoat_roughness_factor.unwrap_or(0.0),
            emissive: material.emissive.to_value(Vec3::ZERO),
            anisotropy: material.anisotropy.to_value(0.0),
            ambient_occlusion: material.ao_factor.unwrap_or(1.0),
            alpha_cutout: material.alpha_cutout.unwrap_or(0.0),
            texture_enable: material.albedo.is_texture() as u32
                | (material.normal.to_texture(|_| ()).is_some() as u32) << 1
                | (material.aomr_textures.to_roughness_texture(|_| ()).is_some() as u32) << 2
                | (material.aomr_textures.to_metallic_texture(|_| ()).is_some() as u32) << 3
                | (material.reflectance.is_texture() as u32) << 4
                | (material.clearcoat_textures.to_clearcoat_texture(|_| ()).is_some() as u32) << 5
                | (material
                    .clearcoat_textures
                    .to_clearcoat_roughness_texture(|_| ())
                    .is_some() as u32)
                    << 6
                | (material.emissive.is_texture() as u32) << 7
                | (material.anisotropy.is_texture() as u32) << 8
                | (material.aomr_textures.to_ao_texture(|_| ()).is_some() as u32) << 9,
            material_flags: {
                let mut flags = material.albedo.to_flags();
                flags |= material.normal.to_flags();
                flags |= material.aomr_textures.to_flags();
                flags |= material.clearcoat_textures.to_flags();
                flags.set(MaterialFlags::ALPHA_CUTOUT, material.alpha_cutout.is_some());
                flags.set(MaterialFlags::UNLIT, material.unlit);
                flags.set(MaterialFlags::NEAREST, material.nearest);
                flags
            },
        }
    }
}

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
pub struct GPUShaderMaterial {
    albedo: Vec4,
    emissive: Vec3,
    roughness: f32,
    metallic: f32,
    reflectance: f32,
    clear_coat: f32,
    clear_coat_roughness: f32,
    anisotropy: f32,
    ambient_occlusion: f32,
    alpha_cutout: f32,

    uv_transform_row0: Vec4,
    uv_transform_row1: Vec4,
    uv_transform_row2: Vec4,

    albedo_tex: Option<NonZeroU32>,
    normal_tex: Option<NonZeroU32>,
    roughness_tex: Option<NonZeroU32>,
    metallic_tex: Option<NonZeroU32>,
    reflectance_tex: Option<NonZeroU32>,
    clear_coat_tex: Option<NonZeroU32>,
    clear_coat_roughness_tex: Option<NonZeroU32>,
    emissive_tex: Option<NonZeroU32>,
    anisotropy_tex: Option<NonZeroU32>,
    ambient_occlusion_tex: Option<NonZeroU32>,
    material_flags: MaterialFlags,
}

unsafe impl bytemuck::Zeroable for GPUShaderMaterial {}
unsafe impl bytemuck::Pod for GPUShaderMaterial {}

impl GPUShaderMaterial {
    pub fn from_material(material: &Material, translate_texture: &impl Fn(TextureHandle) -> NonZeroU32) -> Self {
        Self {
            albedo: material.albedo.to_value(),
            emissive: material.emissive.to_value(Vec3::ZERO),
            roughness: material.roughness_factor.unwrap_or(0.0),
            metallic: material.metallic_factor.unwrap_or(0.0),
            reflectance: material.reflectance.to_value(0.5),
            clear_coat: material.clearcoat_factor.unwrap_or(0.0),
            clear_coat_roughness: material.clearcoat_roughness_factor.unwrap_or(0.0),
            anisotropy: material.anisotropy.to_value(0.0),
            ambient_occlusion: material.ao_factor.unwrap_or(1.0),
            alpha_cutout: material.alpha_cutout.unwrap_or(0.0),

            uv_transform_row0: material.transform.x_axis.extend(0.0),
            uv_transform_row1: material.transform.y_axis.extend(0.0),
            uv_transform_row2: material.transform.z_axis.extend(0.0),

            albedo_tex: material.albedo.to_texture(translate_texture),
            normal_tex: material.normal.to_texture(translate_texture),
            roughness_tex: material.aomr_textures.to_roughness_texture(translate_texture),
            metallic_tex: material.aomr_textures.to_metallic_texture(translate_texture),
            reflectance_tex: material.reflectance.to_texture(translate_texture),
            clear_coat_tex: material.clearcoat_textures.to_clearcoat_texture(translate_texture),
            clear_coat_roughness_tex: material
                .clearcoat_textures
                .to_clearcoat_roughness_texture(translate_texture),
            emissive_tex: material.emissive.to_texture(translate_texture),
            anisotropy_tex: material.anisotropy.to_texture(translate_texture),
            ambient_occlusion_tex: material.aomr_textures.to_ao_texture(translate_texture),
            material_flags: {
                let mut flags = material.albedo.to_flags();
                flags |= material.normal.to_flags();
                flags |= material.aomr_textures.to_flags();
                flags |= material.clearcoat_textures.to_flags();
                flags.set(MaterialFlags::ALPHA_CUTOUT, material.alpha_cutout.is_some());
                flags.set(MaterialFlags::UNLIT, material.unlit);
                flags.set(MaterialFlags::NEAREST, material.nearest);
                flags
            },
        }
    }
}

struct InternalMaterial {
    mat: Material,
    bind_group: ModeData<Arc<BindGroup>, ()>,
    material_buffer: ModeData<Buffer, ()>,
}

pub struct MaterialManager {
    buffer: ModeData<(), WrappedPotBuffer>,

    registry: ResourceRegistry<InternalMaterial>,
}

impl MaterialManager {
    pub fn new(device: &Device, mode: RendererMode) -> Self {
        span_transfer!(_ -> new_span, INFO, "Creating Material Manager");

        let buffer = mode.into_data(
            || (),
            || WrappedPotBuffer::new(device, 0, BufferUsage::STORAGE, Some("material buffer")),
        );
        let registry = ResourceRegistry::new();

        Self { buffer, registry }
    }

    pub fn allocate(&self) -> MaterialHandle {
        MaterialHandle(self.registry.allocate())
    }

    pub fn fill(
        &mut self,
        device: &Device,
        mode: RendererMode,
        texture_manager_2d: &mut TextureManager,
        bgc: &mut BindGroupCache,
        handle: MaterialHandle,
        material: Material,
    ) {
        span_transfer!(_ -> fill_span, INFO, "Material Manager Fill");

        texture_manager_2d.ensure_null_view();
        let null_tex = texture_manager_2d.get_null_view();

        let material_buffer = mode.into_data(
            || {
                let data = CPUShaderMaterial::from_material(&material);

                device.create_buffer_init(&BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::bytes_of(&data),
                    usage: BufferUsage::COPY_DST | BufferUsage::UNIFORM,
                })
            },
            || (),
        );

        let lookup_fn = |handle: TextureHandle| texture_manager_2d.get_view(handle);

        self.registry.insert(
            handle.0,
            InternalMaterial {
                bind_group: mode.into_data(
                    || {
                        let visibility = ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE;
                        let ty = BindingType::Texture {
                            view_dimension: TextureViewDimension::D2,
                            sample_type: TextureSampleType::Float { filterable: true },
                            multisampled: false,
                        };

                        let mut bgb = BindGroupBuilder::new_no_label();
                        bgb.append(
                            visibility,
                            ty,
                            None,
                            BindingResource::TextureView(material.albedo.to_texture(lookup_fn).unwrap_or(null_tex)),
                        );
                        bgb.append(
                            visibility,
                            ty,
                            None,
                            BindingResource::TextureView(material.normal.to_texture(lookup_fn).unwrap_or(null_tex)),
                        );
                        bgb.append(
                            visibility,
                            ty,
                            None,
                            BindingResource::TextureView(
                                material
                                    .aomr_textures
                                    .to_roughness_texture(lookup_fn)
                                    .unwrap_or(null_tex),
                            ),
                        );
                        bgb.append(
                            visibility,
                            ty,
                            None,
                            BindingResource::TextureView(
                                material
                                    .aomr_textures
                                    .to_metallic_texture(lookup_fn)
                                    .unwrap_or(null_tex),
                            ),
                        );
                        bgb.append(
                            visibility,
                            ty,
                            None,
                            BindingResource::TextureView(
                                material.reflectance.to_texture(lookup_fn).unwrap_or(null_tex),
                            ),
                        );
                        bgb.append(
                            visibility,
                            ty,
                            None,
                            BindingResource::TextureView(
                                material
                                    .clearcoat_textures
                                    .to_clearcoat_texture(lookup_fn)
                                    .unwrap_or(null_tex),
                            ),
                        );
                        bgb.append(
                            visibility,
                            ty,
                            None,
                            BindingResource::TextureView(
                                material
                                    .clearcoat_textures
                                    .to_clearcoat_roughness_texture(lookup_fn)
                                    .unwrap_or(null_tex),
                            ),
                        );
                        bgb.append(
                            visibility,
                            ty,
                            None,
                            BindingResource::TextureView(material.emissive.to_texture(lookup_fn).unwrap_or(null_tex)),
                        );
                        bgb.append(
                            visibility,
                            ty,
                            None,
                            BindingResource::TextureView(material.anisotropy.to_texture(lookup_fn).unwrap_or(null_tex)),
                        );
                        bgb.append(
                            visibility,
                            ty,
                            None,
                            BindingResource::TextureView(
                                material.aomr_textures.to_ao_texture(lookup_fn).unwrap_or(null_tex),
                            ),
                        );
                        bgb.append(
                            visibility,
                            BindingType::Buffer {
                                ty: BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            None,
                            material_buffer.as_cpu().as_entire_binding(),
                        );
                        bgb.build(device, bgc).1
                    },
                    || (),
                ),
                mat: material,
                material_buffer,
            },
        );
    }

    pub fn remove(&mut self, handle: MaterialHandle) {
        self.registry.remove(handle.0);
    }

    pub fn update_from_changes(&mut self, queue: &Queue, handle: MaterialHandle, change: MaterialChange) {
        let material = self.registry.get_mut(handle.0);
        material.mat.update_from_changes(change);

        if let ModeData::CPU(ref mut mat_buffer) = material.material_buffer {
            let cpu = CPUShaderMaterial::from_material(&material.mat);
            queue.write_buffer(mat_buffer, 0, bytemuck::bytes_of(&cpu));
        }
    }

    pub fn cpu_get_bind_group(&self, handle: MaterialHandle) -> &BindGroup {
        self.registry.get(handle.0).bind_group.as_cpu()
    }

    pub fn internal_index(&self, handle: MaterialHandle) -> usize {
        self.registry.get_index_of(handle.0)
    }

    pub fn ready(&mut self, device: &Device, queue: &Queue, texture_manager: &TextureManager) {
        span_transfer!(_ -> ready_span, INFO, "Material Manager Ready");

        if let ModeData::GPU(ref mut buffer) = self.buffer {
            let translate_texture = texture_manager.translation_fn();
            let data: Vec<_> = self
                .registry
                .values()
                .map(|internal| GPUShaderMaterial::from_material(&internal.mat, &translate_texture))
                .collect();

            buffer.write_to_buffer(device, queue, bytemuck::cast_slice(&data));
        }
    }

    pub fn gpu_make_bg<'a>(&'a self, device: &Device, cache: &mut BindGroupCache, visibility: ShaderStage) -> (Arc<BindGroupLayout>, Arc<BindGroup>) {
        let mut bgb = BindGroupBuilder::new("material data");
        bgb.append(
            visibility,
            BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(size_of::<GPUShaderMaterial>() as _),
            },
            None,
            self.buffer.as_gpu().as_entire_binding(),
        );
        bgb.build(device, cache)
    }
}
