use crate::{
    managers::{ObjectManager, TextureManager},
    profile::ProfileData,
    types::MaterialHandle,
    util::{
        bind_merge::{BindGroupBuilder, BindGroupLayoutBuilder},
        buffer::WrappedPotBuffer,
        freelist::{FreelistBuffer, FreelistBufferIndex},
        math::round_up_pot,
        registry::ArchitypicalErasedRegistry,
        typedefs::FastHashMap,
    },
    RendererProfile,
};
use encase::{ShaderSize, ShaderType, StorageBuffer};
use list_any::VecAny;
use rend3_types::{
    Material, MaterialArray, MaterialTag, RawMaterialHandle, RawObjectHandle, RawTextureHandle, VertexAttributeId,
};
use std::{
    any::TypeId,
    num::{NonZeroU32, NonZeroU64},
    sync::atomic::{AtomicUsize, Ordering},
};
use wgpu::{
    BindGroup, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType, Buffer,
    BufferBinding, BufferBindingType, BufferDescriptor, BufferUsages, Device, Queue, ShaderStages, TextureSampleType,
    TextureViewDimension,
};

mod texture_dedupe;

pub use texture_dedupe::TextureBindGroupIndex;

#[derive(ShaderType)]
struct GpuPoweredShaderWrapper<M: Material> {
    textures: <M::TextureArrayType as MaterialArray<Option<RawTextureHandle>>>::U32Array,
    data: M::DataType,
}

#[derive(ShaderType)]
struct CpuPoweredShaderWrapper<M: Material> {
    data: M::DataType,
    texture_enable: u32,
}

/// Internal representation of a material.
pub struct InternalMaterial {
    pub bind_group_index: ProfileData<TextureBindGroupIndex, ()>,
    pub buffer_index: FreelistBufferIndex,
    pub key: u64,
    /// Handles of all objects
    pub objects: Vec<RawObjectHandle>,
}

#[allow(clippy::type_complexity)]
struct PerTypeInfo {
    buffer: FreelistBuffer,
    data_size: u32,
    texture_count: u32,
    write_gpu_materials_fn: fn(&mut [u8], &VecAny, &mut (dyn FnMut(RawTextureHandle) -> NonZeroU32 + '_)) -> usize,
    get_material_key: fn(&VecAny, usize) -> MaterialKeyPair,
    get_attributes: fn(&mut dyn FnMut(&[&'static VertexAttributeId], &[&'static VertexAttributeId])),
}

/// Key which determine's an object's archetype.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct MaterialKeyPair {
    /// TypeId of the material the object uses.
    pub ty: TypeId,
    /// Per-material data.
    pub key: u64,
}

struct BufferRange {
    offset: u64,
    size: NonZeroU64,
}

/// Manages materials and their associated BindGroups in CPU modes.
pub struct MaterialManager {
    bg: FastHashMap<TypeId, ProfileData<(), BufferRange>>,
    type_info: FastHashMap<TypeId, PerTypeInfo>,

    texture_deduplicator: texture_dedupe::TextureDeduplicator,

    registry: ArchitypicalErasedRegistry<MaterialTag, InternalMaterial>,
}

impl MaterialManager {
    pub fn new(device: &Device, profile: RendererProfile) -> Self {
        profiling::scope!("MaterialManager::new");

        let registry = ArchitypicalErasedRegistry::new();

        let texture_deduplicator = texture_dedupe::TextureDeduplicator::new(device);

        Self {
            bg: FastHashMap::default(),
            type_info: FastHashMap::default(),
            texture_deduplicator,
            registry,
        }
    }

    pub fn allocate(counter: &AtomicUsize) -> MaterialHandle {
        let idx = counter.fetch_add(1, Ordering::Relaxed);

        MaterialHandle::new(idx)
    }

    pub fn ensure_archetype<M: Material>(&mut self, device: &Device, profile: RendererProfile) {
        self.ensure_archetype_inner::<M>(device, profile);
    }

    fn ensure_archetype_inner<M: Material>(&mut self, device: &Device, profile: RendererProfile) -> &mut PerTypeInfo {
        profiling::scope!("MaterialManager::ensure_archetype");

        self.registry.ensure_archetype::<M>();

        let ty = TypeId::of::<M>();

        self.type_info.entry(ty).or_insert_with(|| PerTypeInfo {
            buffer: match profile {
                RendererProfile::CpuDriven => FreelistBuffer::new::<CpuPoweredShaderWrapper<M>>(device),
                RendererProfile::GpuDriven => FreelistBuffer::new::<GpuPoweredShaderWrapper<M>>(device),
            },
            data_size: M::DataType::SHADER_SIZE.get() as _,
            texture_count: M::TextureArrayType::COUNT,
            write_gpu_materials_fn: write_gpu_materials::<M>,
            get_material_key: get_material_key::<M>,
            get_attributes: get_attributes::<M>,
        })
    }

    fn fill_inner<M: Material>(
        &mut self,
        device: &Device,
        profile: RendererProfile,
        texture_manager_2d: &mut TextureManager,
        material: &M,
    ) -> InternalMaterial {
        let null_tex = texture_manager_2d.get_null_view();

        let translation_fn = texture_manager_2d.translation_fn();

        let type_info = self.ensure_archetype_inner::<M>(device, profile);

        let bind_group_index;
        if profile == RendererProfile::CpuDriven {
            let textures = material.to_textures();

            let texture_bg_index =
                self.texture_deduplicator
                    .get_or_insert(device, texture_manager_2d, textures.as_ref());

            bind_group_index = ProfileData::Cpu(texture_bg_index);
        } else {
            bind_group_index = ProfileData::Gpu(());
        };
        let buffer_index = type_info.buffer.add();

        InternalMaterial {
            buffer_index,
            bind_group_index,
            key: material.object_key(),
            objects: Vec::new(),
        }
    }

    pub fn fill<M: Material>(
        &mut self,
        device: &Device,
        profile: RendererProfile,
        texture_manager_2d: &mut TextureManager,
        handle: &MaterialHandle,
        material: M,
    ) {
        let internal = self.fill_inner(device, profile, texture_manager_2d, &material);

        self.registry.insert(handle, material, internal);
    }

    pub fn update<M: Material>(
        &mut self,
        device: &Device,
        profile: RendererProfile,
        texture_manager_2d: &mut TextureManager,
        object_manager: &mut ObjectManager,
        handle: &MaterialHandle,
        material: M,
    ) {
        let new_key = material.object_key();

        self.registry.update(handle, material);

        let internal = self.registry.get_metadata_mut::<M>(handle.get_raw());
        if let ProfileData::Cpu(ref mut index) = internal.bind_group_index {
            // Create the new bind group first. If the bind group didn't change, this will prevent
            // the bind group from dying on the call to remove.
            let bind_group_index =
                self.texture_deduplicator
                    .get_or_insert(device, texture_manager_2d, material.to_textures().as_ref());
            self.texture_deduplicator.remove(*index);
            *index = bind_group_index;
        }
        if internal.key != new_key {
            for object in &internal.objects {
                object_manager.set_key(
                    *object,
                    MaterialKeyPair {
                        ty: TypeId::of::<M>(),
                        key: internal.key,
                    },
                )
            }
            internal.key = new_key;
        }
    }

    pub fn get_material<M: Material>(&self, handle: RawMaterialHandle) -> &M {
        self.registry.get_ref::<M>(handle)
    }

    pub fn get_bind_group_layout_cpu<M: Material>(&self) -> &BindGroupLayout {
        todo!()
    }

    pub fn add_to_bgl_gpu<M: Material>(bglb: &mut BindGroupLayoutBuilder) {
        bglb.append(
            ShaderStages::VERTEX_FRAGMENT,
            BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(
                    (round_up_pot(M::TextureArrayType::COUNT * 4, 16)
                        + round_up_pot(M::DataType::SHADER_SIZE.get() as u32, 16)) as _,
                ),
            },
            None,
        );
    }

    pub fn add_to_bg_gpu<'a, M: Material>(&'a self, bgb: &mut BindGroupBuilder<'a>) {
        todo!()
    }

    pub fn get_internal_material_full<M: Material>(&self, handle: RawMaterialHandle) -> (&M, &InternalMaterial) {
        self.registry.get_ref_full::<M>(handle)
    }

    pub fn get_internal_material_full_by_index<M: Material>(&self, index: usize) -> (&M, &InternalMaterial) {
        self.registry.get_ref_full_by_index::<M>(index)
    }

    pub fn get_material_key_and_objects(
        &mut self,
        handle: RawMaterialHandle,
    ) -> (MaterialKeyPair, &mut Vec<RawObjectHandle>) {
        let index = self.registry.get_index(handle);
        let ty = self.registry.get_type_id(handle);
        let type_info = &self.type_info[&ty];
        let arch = self.registry.get_archetype_mut(ty);

        let key_pair = (type_info.get_material_key)(&arch.vec, index);

        (key_pair, &mut arch.non_erased[index].inner.objects)
    }

    pub fn get_objects(&mut self, handle: RawMaterialHandle) -> &mut Vec<RawObjectHandle> {
        let index = self.registry.get_index(handle);
        let ty = self.registry.get_type_id(handle);
        let arch = self.registry.get_archetype_mut(ty);

        &mut arch.non_erased[index].inner.objects
    }

    pub fn get_attributes(
        &self,
        handle: RawMaterialHandle,
        mut callback: impl FnMut(&[&'static VertexAttributeId], &[&'static VertexAttributeId]),
    ) {
        let ty = self.registry.get_type_id(handle);
        let type_info = &self.type_info[&ty];

        (type_info.get_attributes)(&mut callback)
    }

    pub fn get_internal_index(&self, handle: RawMaterialHandle) -> usize {
        self.registry.get_index(handle)
    }

    pub fn ready(
        &mut self,
        device: &Device,
        queue: &Queue,
        object_manager: &mut ObjectManager,
        texture_manager: &TextureManager,
    ) {
        profiling::scope!("Material Ready");
        self.registry.remove_all_dead(|type_id, internal, idx| {
            for object in &internal.objects {
                object_manager.set_material_index(*object, idx);
            }
            if let ProfileData::Cpu(index) = internal.bind_group_index {
                self.texture_deduplicator.remove(index);
            }
            self.type_info[&type_id].buffer.remove(internal.buffer_index);
        });
    }
}

fn write_gpu_materials<M: Material>(
    type_info: &PerTypeInfo,
    vec_any: &VecAny,
    translation_fn: &mut (dyn FnMut(RawTextureHandle) -> NonZeroU32 + '_),
) -> usize {
    let materials = vec_any.downcast_slice::<M>().unwrap();

    // TODO: figure out how to deal with freelist buffer and material registry wanting
    // to give materials their own ids.
}

fn get_material_key<M: Material>(vec_any: &VecAny, index: usize) -> MaterialKeyPair {
    let materials = vec_any.downcast_slice::<M>().unwrap();

    let key = materials[index].object_key();

    MaterialKeyPair {
        ty: TypeId::of::<M>(),
        key,
    }
}

fn get_attributes<M: Material>(callback: &mut dyn FnMut(&[&'static VertexAttributeId], &[&'static VertexAttributeId])) {
    callback(M::required_attributes().as_ref(), M::supported_attributes().as_ref())
}
