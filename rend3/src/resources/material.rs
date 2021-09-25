use crate::{
    mode::ModeData,
    resources::TextureManager,
    types::{Material, MaterialHandle, TextureHandle},
    util::{
        bind_merge::BindGroupBuilder,
        buffer::WrappedPotBuffer,
        math::round_up_pot,
        registry::{ArchitypeResourceStorage, ArchitypicalRegistry},
        typedefs::FastHashMap,
    },
    RendererMode,
};
use list_any::VecAny;
use rend3_types::{MaterialTrait, RawMaterialHandle};
use std::{
    any::TypeId,
    num::{NonZeroU32, NonZeroU64},
};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BindGroup, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType, Buffer,
    BufferBinding, BufferBindingType, BufferUsages, Device, Queue, ShaderStages, TextureSampleType,
    TextureViewDimension,
};

pub struct InternalMaterial<M: MaterialTrait> {
    pub mat: M,
    pub bind_group: ModeData<BindGroup, ()>,
    pub material_buffer: ModeData<Buffer, ()>,
}

struct PerTypeInfo {
    bgl: ModeData<BindGroupLayout, BindGroupLayout>,
    data_count: u32,
    texture_count: u32,
    write_gpu_materials_fn: fn(
        dest: &mut [u8],
        vec_any: &VecAny,
        translation_fn: &mut (dyn FnMut(&TextureHandle) -> NonZeroU32 + '_),
    ) -> usize,
}

/// Manages materials and their associated BindGroups in CPU modes.
pub struct MaterialManager {
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

        let create_bgl = || {
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
        };

        let ty = TypeId::of::<M>();

        let type_info = self.type_info.entry(ty).or_insert_with(|| PerTypeInfo {
            bgl: create_bgl(),
            data_count,
            texture_count,
            write_gpu_materials_fn: write_gpu_materials::<M>,
        });

        let mut translation_fn = texture_manager_2d.translation_fn();

        let bind_group = mode.into_data(
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
                    .build(device, type_info.bgl.as_ref().as_cpu())
            },
            || (),
        );

        self.registry.insert(
            handle,
            InternalMaterial {
                bind_group,
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

    pub fn get_material<M: MaterialTrait>(&self, handle: RawMaterialHandle) -> &M {
        &self.registry.get_ref::<InternalMaterial<M>>(handle).mat
    }

    pub fn get_bind_group_layout<M: MaterialTrait>(&self) -> &BindGroupLayout {
        self.type_info[&TypeId::of::<M>()].bgl.as_ref().into_common()
    }

    pub fn get_internal_material<M: MaterialTrait>(&self, handle: RawMaterialHandle) -> &InternalMaterial<M> {
        self.registry.get_ref::<InternalMaterial<M>>(handle)
    }

    pub fn get_bind_group_gpu<M: MaterialTrait>(&self) -> &BindGroup {
        self.bg[&TypeId::of::<M>()].as_gpu()
    }

    pub fn get_internal_index(&self, handle: RawMaterialHandle) -> usize {
        self.registry.get_index(handle)
    }

    pub fn ready(&mut self, device: &Device, queue: &Queue, texture_manager: &TextureManager) {
        profiling::scope!("Material Ready");
        self.registry.remove_all_dead();

        if let ModeData::GPU(ref mut buffer) = self.buffer {
            profiling::scope!("Update GPU Material Buffer");
            let mut translate_texture = texture_manager.translation_fn();

            let self_type_info = &self.type_info;

            let count: usize = self
                .registry
                .architype_lengths()
                .map(|(ty, len)| {
                    let type_info = &self_type_info[&ty];

                    len * (round_up_pot(type_info.texture_count, 16) + round_up_pot(type_info.data_count, 16)) as usize
                })
                .sum();

            buffer.ensure_size(device, count as u64);

            let mut data = vec![0u8; count];

            let mut offset = 0_usize;
            for (ty, architype) in self.registry.architypes_mut() {
                let type_info = &self.type_info[&ty];

                let size = (type_info.write_gpu_materials_fn)(&mut data[offset..], architype, &mut translate_texture);
                let size = size.max(16);

                self.bg.insert(
                    ty,
                    ModeData::GPU(create_gpu_buffer_bg(
                        device,
                        type_info.bgl.as_gpu(),
                        buffer,
                        offset,
                        size,
                    )),
                );

                offset += size;
            }

            // TODO: I know the size before hand, we could elide this cpu side copy
            buffer.write_to_buffer(device, queue, bytemuck::cast_slice(&data));
        }
    }
}

fn create_gpu_buffer_bg(
    device: &Device,
    bgl: &BindGroupLayout,
    buffer: &Buffer,
    offset: usize,
    size: usize,
) -> BindGroup {
    BindGroupBuilder::new(Some("gpu material bg"))
        .with(BindingResource::Buffer(BufferBinding {
            buffer,
            offset: offset as u64,
            size: Some(NonZeroU64::new(size as u64).unwrap()),
        }))
        .build(device, bgl)
}

fn write_gpu_materials<'a, M: MaterialTrait>(
    dest: &mut [u8],
    vec_any: &VecAny,
    translation_fn: &mut (dyn FnMut(&TextureHandle) -> NonZeroU32 + '_),
) -> usize {
    let materials = vec_any.downcast_slice::<ArchitypeResourceStorage<M>>().unwrap();

    let mut offset = 0_usize;

    for mat in materials {
        let mat_size = round_up_pot(mat.data.texture_count() * 4, 16) as usize;
        mat.data.to_texture(
            bytemuck::cast_slice_mut(&mut dest[offset..offset + mat_size]),
            translation_fn,
        );

        offset += mat_size;

        let data_size = round_up_pot(mat.data.data_count(), 16) as usize;
        mat.data.to_data(&mut dest[offset..offset + data_size]);

        offset += mat_size;
    }

    offset
}
