use crate::{
    managers::{ObjectManager, TextureManager},
    profile::ProfileData,
    types::MaterialHandle,
    util::{
        bind_merge::{BindGroupBuilder, BindGroupLayoutBuilder},
        buffer::WrappedPotBuffer,
        freelist::FreelistDerivedBuffer,
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
struct InternalMaterial<M> {
    bind_group_index: ProfileData<TextureBindGroupIndex, ()>,
    inner: M,
}

struct PerTypeInfo {
    buffer: FreelistDerivedBuffer,
    data_vec: VecAny,
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
    handle_to_typeid: FastHashMap<RawMaterialHandle, TypeId>,
    storage: FastHashMap<TypeId, PerTypeInfo>,

    texture_deduplicator: texture_dedupe::TextureDeduplicator,
}

impl MaterialManager {
    pub fn new(device: &Device, profile: RendererProfile) -> Self {
        profiling::scope!("MaterialManager::new");

        let texture_deduplicator = texture_dedupe::TextureDeduplicator::new(device);

        Self {
            handle_to_typeid: FastHashMap::default(),
            storage: FastHashMap::default(),
            texture_deduplicator,
        }
    }

    pub fn ensure_archetype<M: Material>(&mut self, device: &Device, profile: RendererProfile) {
        self.ensure_archetype_inner::<M>(device, profile);
    }

    fn ensure_archetype_inner<M: Material>(&mut self, device: &Device, profile: RendererProfile) -> &mut PerTypeInfo {
        profiling::scope!("MaterialManager::ensure_archetype");

        let ty = TypeId::of::<M>();

        self.storage.entry(ty).or_insert_with(|| PerTypeInfo {
            buffer: match profile {
                RendererProfile::CpuDriven => FreelistDerivedBuffer::new::<CpuPoweredShaderWrapper<M>>(device),
                RendererProfile::GpuDriven => FreelistDerivedBuffer::new::<GpuPoweredShaderWrapper<M>>(device),
            },
            data_vec: VecAny::new::<Option<M>>(),
        })
    }

    pub fn add<M: Material>(
        &mut self,
        device: &Device,
        profile: RendererProfile,
        texture_manager_2d: &mut TextureManager,
        handle: &MaterialHandle,
        material: M,
    ) {
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

        let type_info = self.ensure_archetype_inner::<M>(device, profile);
        type_info.buffer.use_index(handle.idx);

        let mut data_vec = type_info.data_vec.downcast_mut::<Option<M>>().unwrap();
        if handle.idx > data_vec.len() {
            data_vec.resize_with(handle.idx.saturating_sub(1).next_power_of_two(), || None);
            data_vec[handle.idx] = Some(InternalMaterial {
                bind_group_index,
                inner: material,
            });
        }

        self.handle_to_typeid.insert(*handle, TypeId::of::<M>());
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
        let type_id = self.handle_to_typeid[&**handle];

        assert_eq!(type_id, TypeId::of::<M>());

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
