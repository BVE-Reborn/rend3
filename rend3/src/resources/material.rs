use crate::{RendererMode, mode::ModeData, resources::TextureManager, types::{Material, MaterialChange, MaterialFlags, MaterialHandle, SampleType, TextureHandle}, util::{bind_merge::BindGroupBuilder, buffer::WrappedPotBuffer, math::round_up_pot, registry::{ArchitypeResourceStorage, ArchitypicalRegistry, ResourceRegistry}, typedefs::FastHashMap}};
use glam::{Vec3, Vec4};
use list_any::VecAny;
use rend3_types::{MaterialTrait, RawMaterialHandle};
use std::{
    any::TypeId,
    mem,
    num::{NonZeroU32, NonZeroU64},
};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BindGroup, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType, Buffer,
    BufferBindingType, BufferUsages, Device, Queue, ShaderStages, TextureSampleType, TextureViewDimension,
};

struct InternalMaterial<M: MaterialTrait> {
    mat: M,
    bind_group: ModeData<BindGroup, ()>,
    material_buffer: ModeData<Buffer, ()>,
}

#[derive(Copy, Clone)]
struct PerTypeInfo {
    data_count: u32,
    texture_count: u32,
}

/// Manages materials and their associated BindGroups in CPU modes.
pub struct MaterialManager {
    bgl: FastHashMap<TypeId, ModeData<BindGroupLayout, BindGroupLayout>>,
    bg: FastHashMap<TypeId, ModeData<(), BindGroup>>,

    type_info: FastHashMap<TypeId, PerTypeInfo>,

    buffer: ModeData<(), WrappedPotBuffer>,

    registry: ArchitypicalRegistry<Material>,
}

impl MaterialManager {
    pub fn new(device: &Device, mode: RendererMode) -> Self {
        let buffer = mode.into_data(
            || (),
            || WrappedPotBuffer::new(device, 0, 16 as _, BufferUsages::STORAGE, Some("material buffer")),
        );

        let registry = ArchitypicalRegistry::new();

        Self {
            bgl: FastHashMap::default(),
            bg: FastHashMap::default(),
            type_info: FastHashMap::default(),
            buffer,
            registry,
        }
    }

    pub fn allocate(&self) -> MaterialHandle {
        self.registry.allocate()
    }

    pub fn fill<M: MaterialTrait>(
        &mut self,
        device: &Device,
        mode: RendererMode,
        texture_manager_2d: &mut TextureManager,
        handle: &MaterialHandle,
        material: M,
    ) {
        let null_tex = texture_manager_2d.get_null_view();

        let texture_count = material.texture_count();
        let data_count = material.data_count();

        let material_buffer = mode.into_data(
            || {
                // TODO: stack allocation
                let mut data = vec![0u8; data_count as usize];
                material.to_data(&mut data);

                device.create_buffer_init(&BufferInitDescriptor {
                    label: None,
                    contents: &data,
                    usage: BufferUsages::COPY_DST | BufferUsages::UNIFORM,
                })
            },
            || (),
        );

        let ty = TypeId::of::<M>();

        let bgl = self.bgl.entry(ty).or_insert_with(|| {
            mode.into_data(
                || {
                    let texture_binding = |idx: u32| BindGroupLayoutEntry {
                        binding: idx as u32,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Texture {
                            sample_type: TextureSampleType::Float { filterable: true },
                            view_dimension: TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    };

                    let mut entries: Vec<_> = (0..texture_count).map(texture_binding).collect();
                    entries.push(BindGroupLayoutEntry {
                        binding: texture_count + 1,
                        visibility: ShaderStages::VERTEX_FRAGMENT,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(data_count as _),
                        },
                        count: None,
                    });
                    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                        label: Some("cpu material bgl"),
                        entries: &entries,
                    })
                },
                || {
                    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                        label: Some("gpu material bgl"),
                        entries: &[BindGroupLayoutEntry {
                            binding: 0,
                            visibility: ShaderStages::VERTEX_FRAGMENT,
                            ty: BindingType::Buffer {
                                ty: BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: NonZeroU64::new(
                                    (material.texture_count() + material.data_count()) as _,
                                ),
                            },
                            count: None,
                        }],
                    })
                },
            )
        });

        self.type_info.insert(
            ty,
            PerTypeInfo {
                data_count,
                texture_count,
            },
        );

        let translation_fn = texture_manager_2d.translation_fn();

        self.registry.insert(
            handle,
            InternalMaterial {
                bind_group: mode.into_data(
                    || {
                        let mut textures = vec![NonZeroU32::new(u32::MAX); texture_count as usize];
                        material.to_texture(&mut textures, &mut translation_fn);

                        let mut builder = BindGroupBuilder::new(None);
                        for texture in textures {
                            builder.append(BindingResource::TextureView(
                                texture
                                    .map(|tex| texture_manager_2d.get_view_from_index(tex))
                                    .unwrap_or(null_tex),
                            ));
                        }
                        builder
                            .with_buffer(material_buffer.as_cpu())
                            .build(device, bgl.as_ref().as_cpu())
                    },
                    || (),
                ),
                mat: material,
                material_buffer,
            },
        );
    }

    // pub fn update_from_changes<M: MaterialTrait>(&mut self, queue: &Queue, handle: RawMaterialHandle, change: M) {
    //     let material = self.registry.get_mut(handle);
    //     material.mat.update_from_changes(change);

    //     if let ModeData::CPU(ref mut mat_buffer) = material.material_buffer {
    //         let cpu = CPUShaderMaterial::from_material(&material.mat);
    //         queue.write_buffer(mat_buffer, 0, bytemuck::bytes_of(&cpu));
    //     }
    // }

    // pub fn get_material(&self, handle: RawMaterialHandle) -> &Material {
    //     &self.registry.get(handle).mat
    // }

    // pub fn get_bind_group_layout(&self) -> &BindGroupLayout {
    //     self.bgl.as_ref().into_common()
    // }

    // pub fn cpu_get_bind_group(&self, handle: RawMaterialHandle) -> (&BindGroup, SampleType) {
    //     let material = self.registry.get(handle);
    //     (material.bind_group.as_cpu(), material.mat.sample_type)
    // }

    // pub fn gpu_get_bind_group(&self) -> &BindGroup {
    //     self.bg.as_gpu()
    // }

    // pub fn internal_index(&self, handle: RawMaterialHandle) -> usize {
    //     self.registry.get_index_of(handle)
    // }

    pub fn ready(&mut self, device: &Device, queue: &Queue, texture_manager: &TextureManager) {
        profiling::scope!("Material Ready");
        self.registry.remove_all_dead();

        if let ModeData::GPU(ref mut buffer) = self.buffer {
            profiling::scope!("Update GPU Material Buffer");
            let translate_texture = texture_manager.translation_fn();

            let count: usize = self
                .registry
                .architype_lengths()
                .map(|(ty, len)| {
                    let type_info = self.type_info[&ty];

                    len * (round_up_pot(type_info.texture_count, 16) + round_up_pot(type_info.data_count, 16))
                })
                .sum();
            
            let mut data = vec![0u8; count];
            

            

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

fn write_gpu_materials<M: MaterialTrait>(mut dest: &mut [u8], vec_any: &VecAny, translation_fn: &mut (dyn FnMut(&TextureHandle) -> NonZeroU32 + '_)) {
    let materials = vec_any.downcast_slice::<ArchitypeResourceStorage<M>>().unwrap();

    for mat in materials {
        let mat_size = round_up_pot((mat.data.texture_count() * 4) as usize, 16);
        mat.data.to_texture(bytemuck::cast_slice_mut(&mut dest[0..mat_size]), translation_fn);

        dest = &mut dest[mat_size..];

        let data_size = round_up_pot(mat.data.data_count() as usize, 16);
        mat.data.to_data(&mut dest[0..data_size]);

        dest = &mut dest[data_size..];
    }
}
