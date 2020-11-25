use crate::{
    bind_merge::BindGroupBuilder,
    datatypes::{Material, MaterialChange, MaterialFlags, MaterialHandle, RendererTextureFormat, TextureHandle},
    registry::ResourceRegistry,
    renderer::{limits::MAX_UNIFORM_BUFFER_BINDING_SIZE, texture::TextureManager, ModeData, RendererMode},
};
use glam::f32::Vec4;
use std::{mem::size_of, num::NonZeroU32, sync::Arc};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BindGroup, BindGroupLayout, BindingResource, Buffer, BufferAddress, BufferUsage, CommandEncoder, Device, Queue,
};
use wgpu_conveyor::{AutomatedBuffer, AutomatedBufferManager, IdBuffer};

pub const MAX_MATERIALS: usize = MAX_UNIFORM_BUFFER_BINDING_SIZE as usize / size_of::<GPUShaderMaterial>();
pub const MATERIALS_SIZE: BufferAddress = (MAX_MATERIALS * size_of::<GPUShaderMaterial>()) as BufferAddress;

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
struct CPUShaderMaterial {
    albedo: Vec4,
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
    pub fn from_material(material: &Material, texture_manager_2d: &TextureManager) -> Self {
        Self {
            albedo: material.albedo.to_value(),
            roughness: material.roughness.to_value(0.0),
            metallic: material.metallic.to_value(0.0),
            reflectance: material.reflectance.to_value(0.5),
            clear_coat: material.clear_coat.to_value(0.0),
            clear_coat_roughness: material.clear_coat_roughness.to_value(0.0),
            anisotropy: material.anisotropy.to_value(0.0),
            ambient_occlusion: material.ambient_occlusion.to_value(1.0),
            alpha_cutout: material.alpha_cutout.unwrap_or(0.0),
            texture_enable: !0,
            material_flags: {
                let mut flags = material.albedo.to_flags();
                flags.set(MaterialFlags::ALPHA_CUTOUT, material.alpha_cutout.is_some());
                flags.set(
                    MaterialFlags::BICOMPONENT_NORMAL,
                    material
                        .normal
                        .and_then(|handle| texture_manager_2d.get(handle).format)
                        .map(|format| format == RendererTextureFormat::Bc5Normal)
                        .unwrap_or(false),
                );
                flags
            },
        }
    }
}

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
struct GPUShaderMaterial {
    albedo: Vec4,
    roughness: f32,
    metallic: f32,
    reflectance: f32,
    clear_coat: f32,
    clear_coat_roughness: f32,
    anisotropy: f32,
    ambient_occlusion: f32,
    alpha_cutout: f32,

    albedo_tex: Option<NonZeroU32>,
    normal_tex: Option<NonZeroU32>,
    roughness_tex: Option<NonZeroU32>,
    metallic_tex: Option<NonZeroU32>,
    reflectance_tex: Option<NonZeroU32>,
    clear_coat_tex: Option<NonZeroU32>,
    clear_coat_roughness_tex: Option<NonZeroU32>,
    anisotropy_tex: Option<NonZeroU32>,
    ambient_occlusion_tex: Option<NonZeroU32>,
    material_flags: MaterialFlags,
}

unsafe impl bytemuck::Zeroable for GPUShaderMaterial {}
unsafe impl bytemuck::Pod for GPUShaderMaterial {}

struct InternalMaterial {
    mat: Material,
    bind_group: ModeData<Arc<BindGroup>, ()>,
    material_buffer: ModeData<Buffer, ()>,
}

pub struct MaterialManager {
    buffer: ModeData<(), AutomatedBuffer>,
    buffer_storage: ModeData<(), Option<Arc<IdBuffer>>>,

    registry: ResourceRegistry<InternalMaterial>,
}

impl MaterialManager {
    pub fn new(device: &Device, mode: RendererMode, manager: &mut AutomatedBufferManager) -> Self {
        span_transfer!(_ -> new_span, INFO, "Creating Material Manager");

        let buffer = mode.into_data(
            || (),
            || {
                manager.create_new_buffer(
                    device,
                    MAX_UNIFORM_BUFFER_BINDING_SIZE,
                    BufferUsage::UNIFORM,
                    Some("material buffer"),
                )
            },
        );
        let registry = ResourceRegistry::new();

        Self {
            buffer,
            buffer_storage: mode.into_data(|| (), || None),
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
        material_bgl: &BindGroupLayout,
        handle: MaterialHandle,
        material: Material,
    ) {
        span_transfer!(_ -> fill_span, INFO, "Material Manager Fill");

        texture_manager_2d.ensure_null_view();
        let null_tex = texture_manager_2d.get_null_view();

        let material_buffer = mode.into_data(
            || {
                let data = CPUShaderMaterial::from_material(&material, texture_manager_2d);

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
                        let mut bgb = BindGroupBuilder::new(None);
                        bgb.append(BindingResource::TextureView(
                            material.albedo.to_texture(lookup_fn).unwrap_or(null_tex),
                        ));
                        bgb.append(BindingResource::TextureView(
                            material.normal.map(lookup_fn).unwrap_or(null_tex),
                        ));
                        bgb.append(BindingResource::TextureView(
                            material.roughness.to_texture(lookup_fn).unwrap_or(null_tex),
                        ));
                        bgb.append(BindingResource::TextureView(
                            material.metallic.to_texture(lookup_fn).unwrap_or(null_tex),
                        ));
                        bgb.append(BindingResource::TextureView(
                            material.reflectance.to_texture(lookup_fn).unwrap_or(null_tex),
                        ));
                        bgb.append(BindingResource::TextureView(
                            material.clear_coat.to_texture(lookup_fn).unwrap_or(null_tex),
                        ));
                        bgb.append(BindingResource::TextureView(
                            material.clear_coat_roughness.to_texture(lookup_fn).unwrap_or(null_tex),
                        ));
                        bgb.append(BindingResource::TextureView(
                            material.anisotropy.to_texture(lookup_fn).unwrap_or(null_tex),
                        ));
                        bgb.append(BindingResource::TextureView(
                            material.ambient_occlusion.to_texture(lookup_fn).unwrap_or(null_tex),
                        ));
                        bgb.append(material_buffer.as_cpu().as_entire_binding());
                        bgb.build(device, material_bgl)
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

    pub fn update_from_changes(
        &mut self,
        queue: &Queue,
        texture_manager_2d: &TextureManager,
        handle: MaterialHandle,
        change: MaterialChange,
    ) {
        let material = self.registry.get_mut(handle.0);
        material.mat.update_from_changes(change);

        if let ModeData::CPU(ref mut mat_buffer) = material.material_buffer {
            let cpu = CPUShaderMaterial::from_material(&material.mat, texture_manager_2d);
            queue.write_buffer(mat_buffer, 0, bytemuck::bytes_of(&cpu));
        }
    }

    pub fn internal_index(&self, handle: MaterialHandle) -> usize {
        self.registry.get_index_of(handle.0)
    }

    pub fn ready(&mut self, device: &Device, encoder: &mut CommandEncoder, texture_manager: &TextureManager) {
        span_transfer!(_ -> ready_span, INFO, "Material Manager Ready");

        let registry = &self.registry;
        if let ModeData::GPU(ref mut buffer) = self.buffer {
            buffer.write_to_buffer(device, encoder, MATERIALS_SIZE, move |_, slice| {
                let typed_slice: &mut [GPUShaderMaterial] = bytemuck::cast_slice_mut(slice);

                let translate_texture = texture_manager.translation_fn();

                for (index, internal) in registry.values().enumerate() {
                    let material = &internal.mat;
                    typed_slice[index] = GPUShaderMaterial {
                        albedo: material.albedo.to_value(),
                        roughness: material.roughness.to_value(0.0),
                        metallic: material.metallic.to_value(0.0),
                        reflectance: material.reflectance.to_value(0.5),
                        clear_coat: material.clear_coat.to_value(0.0),
                        clear_coat_roughness: material.clear_coat_roughness.to_value(0.0),
                        anisotropy: material.anisotropy.to_value(0.0),
                        ambient_occlusion: material.ambient_occlusion.to_value(1.0),
                        alpha_cutout: material.alpha_cutout.unwrap_or(0.0),
                        albedo_tex: material.albedo.to_texture(translate_texture),
                        normal_tex: material.normal.map(translate_texture),
                        roughness_tex: material.roughness.to_texture(translate_texture),
                        metallic_tex: material.metallic.to_texture(translate_texture),
                        reflectance_tex: material.reflectance.to_texture(translate_texture),
                        clear_coat_tex: material.clear_coat.to_texture(translate_texture),
                        clear_coat_roughness_tex: material.clear_coat_roughness.to_texture(translate_texture),
                        anisotropy_tex: material.anisotropy.to_texture(translate_texture),
                        ambient_occlusion_tex: material.ambient_occlusion.to_texture(translate_texture),
                        material_flags: {
                            let mut flags = material.albedo.to_flags();
                            flags.set(MaterialFlags::ALPHA_CUTOUT, material.alpha_cutout.is_some());
                            flags.set(
                                MaterialFlags::BICOMPONENT_NORMAL,
                                material
                                    .normal
                                    .and_then(|handle| texture_manager.get(handle).format)
                                    .map(|format| format == RendererTextureFormat::Bc5Normal)
                                    .unwrap_or(false),
                            );
                            flags
                        },
                    }
                }
            });
            *self.buffer_storage.as_gpu_mut() = Some(self.buffer.as_gpu().get_current_inner());
        }
    }

    pub fn append_to_bgb<'a>(&'a self, general_bgb: &mut BindGroupBuilder<'a>) {
        general_bgb.append(self.buffer_storage.as_gpu().as_ref().unwrap().inner.as_entire_binding());
    }
}
