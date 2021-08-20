use crate::{
    mode::ModeData,
    resources::TextureManager,
    types::{Material, MaterialChange, MaterialFlags, MaterialHandle, SampleType, TextureHandle},
    util::{bind_merge::BindGroupBuilder, buffer::WrappedPotBuffer, registry::ResourceRegistry},
    RendererMode,
};
use glam::{Vec3, Vec4};
use std::{
    mem,
    num::{NonZeroU32, NonZeroU64},
};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BindGroup, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, Buffer,
    BufferBindingType, BufferUsages, Device, Queue, ShaderStages, TextureSampleType, TextureViewDimension,
};

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
                flags.set(
                    MaterialFlags::NEAREST,
                    match material.sample_type {
                        SampleType::Nearest => true,
                        SampleType::Linear => false,
                    },
                );
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
                flags.set(
                    MaterialFlags::NEAREST,
                    match material.sample_type {
                        SampleType::Nearest => true,
                        SampleType::Linear => false,
                    },
                );
                flags
            },
        }
    }
}

struct InternalMaterial {
    mat: Material,
    bind_group: ModeData<BindGroup, ()>,
    material_buffer: ModeData<Buffer, ()>,
}

pub struct MaterialManager {
    bgl: ModeData<BindGroupLayout, BindGroupLayout>,
    bg: ModeData<(), BindGroup>,
    buffer: ModeData<(), WrappedPotBuffer>,

    registry: ResourceRegistry<InternalMaterial>,
}

impl MaterialManager {
    pub fn new(device: &Device, mode: RendererMode) -> Self {
        let bgl = mode.into_data(
            || {
                let texture_binding = |idx| BindGroupLayoutEntry {
                    binding: idx,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                };

                device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                    label: Some("cpu material bgl"),
                    entries: &[
                        texture_binding(0),
                        texture_binding(1),
                        texture_binding(2),
                        texture_binding(3),
                        texture_binding(4),
                        texture_binding(5),
                        texture_binding(6),
                        texture_binding(7),
                        texture_binding(8),
                        texture_binding(9),
                        BindGroupLayoutEntry {
                            binding: 10,
                            visibility: ShaderStages::FRAGMENT,
                            ty: BindingType::Buffer {
                                ty: BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: NonZeroU64::new(mem::size_of::<CPUShaderMaterial>() as _),
                            },
                            count: None,
                        },
                    ],
                })
            },
            || {
                device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                    label: Some("gpu material bgl"),
                    entries: &[BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(mem::size_of::<GPUShaderMaterial>() as _),
                        },
                        count: None,
                    }],
                })
            },
        );

        let buffer = mode.into_data(
            || (),
            || {
                WrappedPotBuffer::new(
                    device,
                    0,
                    mem::size_of::<GPUShaderMaterial>() as _,
                    BufferUsages::STORAGE,
                    Some("material buffer"),
                )
            },
        );

        let bg = mode.into_data(|| (), || create_gpu_buffer_bg(device, bgl.as_gpu(), buffer.as_gpu()));

        let registry = ResourceRegistry::new();

        Self {
            bgl,
            bg,

            buffer,
            registry,
        }
    }

    pub fn allocate(&self) -> MaterialHandle {
        MaterialHandle(self.registry.allocate())
    }

    pub fn fill(
        &mut self,
        device: &Device,
        mode: RendererMode,
        texture_manager_2d: &mut TextureManager,
        handle: MaterialHandle,
        material: Material,
    ) {
        texture_manager_2d.ensure_null_view();
        let null_tex = texture_manager_2d.get_null_view();

        let material_buffer = mode.into_data(
            || {
                let data = CPUShaderMaterial::from_material(&material);

                device.create_buffer_init(&BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::bytes_of(&data),
                    usage: BufferUsages::COPY_DST | BufferUsages::UNIFORM,
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
                        BindGroupBuilder::new(None)
                            .with_texture_view(material.albedo.to_texture(lookup_fn).unwrap_or(null_tex))
                            .with_texture_view(material.normal.to_texture(lookup_fn).unwrap_or(null_tex))
                            .with_texture_view(
                                material
                                    .aomr_textures
                                    .to_roughness_texture(lookup_fn)
                                    .unwrap_or(null_tex),
                            )
                            .with_texture_view(
                                material
                                    .aomr_textures
                                    .to_metallic_texture(lookup_fn)
                                    .unwrap_or(null_tex),
                            )
                            .with_texture_view(material.reflectance.to_texture(lookup_fn).unwrap_or(null_tex))
                            .with_texture_view(
                                material
                                    .clearcoat_textures
                                    .to_clearcoat_texture(lookup_fn)
                                    .unwrap_or(null_tex),
                            )
                            .with_texture_view(
                                material
                                    .clearcoat_textures
                                    .to_clearcoat_roughness_texture(lookup_fn)
                                    .unwrap_or(null_tex),
                            )
                            .with_texture_view(material.emissive.to_texture(lookup_fn).unwrap_or(null_tex))
                            .with_texture_view(material.anisotropy.to_texture(lookup_fn).unwrap_or(null_tex))
                            .with_texture_view(material.aomr_textures.to_ao_texture(lookup_fn).unwrap_or(null_tex))
                            .with_buffer(material_buffer.as_cpu())
                            .build(device, self.bgl.as_cpu())
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

    pub fn get_bind_group_layout(&self) -> &BindGroupLayout {
        self.bgl.as_ref().into_common()
    }

    pub fn cpu_get_bind_group(&self, handle: MaterialHandle) -> (&BindGroup, SampleType) {
        let material = self.registry.get(handle.0);
        (material.bind_group.as_cpu(), material.mat.sample_type)
    }

    pub fn gpu_get_bind_group(&self) -> &BindGroup {
        self.bg.as_gpu()
    }

    pub fn internal_index(&self, handle: MaterialHandle) -> usize {
        self.registry.get_index_of(handle.0)
    }

    pub fn ready(&mut self, device: &Device, queue: &Queue, texture_manager: &TextureManager) {
        if let ModeData::GPU(ref mut buffer) = self.buffer {
            let translate_texture = texture_manager.translation_fn();
            let data: Vec<_> = self
                .registry
                .values()
                .map(|internal| GPUShaderMaterial::from_material(&internal.mat, &translate_texture))
                .collect();

            let resized = buffer.write_to_buffer(device, queue, bytemuck::cast_slice(&data));

            if resized {
                *self.bg.as_gpu_mut() = create_gpu_buffer_bg(device, self.bgl.as_gpu_mut(), self.buffer.as_gpu_mut());
            }
        }
    }
}

fn create_gpu_buffer_bg(device: &Device, bgl: &BindGroupLayout, buffer: &Buffer) -> BindGroup {
    BindGroupBuilder::new(Some("gpu material bg"))
        .with_buffer(buffer)
        .build(device, bgl)
}
