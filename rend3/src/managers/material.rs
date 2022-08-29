use crate::{
    managers::{object_callback, ObjectCallbackArgs, TextureManager},
    profile::ProfileData,
    types::MaterialHandle,
    util::{
        bind_merge::{BindGroupBuilder, BindGroupLayoutBuilder},
        freelist::FreelistDerivedBuffer,
        math::round_up_pot,
        scatter_copy::ScatterCopy,
        typedefs::FastHashMap,
    },
    RendererProfile,
};
use encase::{ShaderSize, ShaderType};
use list_any::VecAny;
use rend3_types::{Material, MaterialArray, RawMaterialHandle, RawObjectHandle, RawTextureHandle, VertexAttributeId};
use std::{
    any::TypeId,
    mem,
    num::{NonZeroU32, NonZeroU64},
};
use wgpu::{BindingType, BufferBindingType, CommandEncoder, Device, ShaderStages};

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
    // Inner type is CpuPoweredShaderWrapper<M> or GpuPoweredShaderWrapper<M>
    buffer: FreelistDerivedBuffer,
    // Inner type is Option<M>
    data_vec: VecAny,
    remove_data: fn(&mut VecAny, RawMaterialHandle) -> ProfileData<TextureBindGroupIndex, ()>,
    apply_data_cpu: fn(&mut FreelistDerivedBuffer, &Device, &mut CommandEncoder, &ScatterCopy, &mut VecAny),
    apply_data_gpu:
        fn(&mut FreelistDerivedBuffer, &Device, &mut CommandEncoder, &ScatterCopy, &mut VecAny, &TextureManager),
    get_attributes: fn(&mut dyn FnMut(&[&'static VertexAttributeId], &[&'static VertexAttributeId])),
    get_material_key: fn(&VecAny, usize) -> MaterialKeyPair,
    object_callback: fn(&VecAny, usize, ObjectCallbackArgs<'_>),
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
            remove_data: remove_data::<M>,
            apply_data_cpu: apply_buffer_cpu::<M>,
            apply_data_gpu: apply_buffer_gpu::<M>,
            get_attributes: get_attributes::<M>,
            get_material_key: get_material_key::<M>,
            object_callback: object_callback_wrapper::<M>,
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

        let mut data_vec = type_info
            .data_vec
            .downcast_mut::<Option<InternalMaterial<M>>>()
            .unwrap();
        if handle.idx > data_vec.len() {
            data_vec.resize_with(handle.idx.saturating_sub(1).next_power_of_two(), || None);
        }
        data_vec[handle.idx] = Some(InternalMaterial {
            bind_group_index,
            inner: material,
        });

        self.handle_to_typeid.insert(**handle, TypeId::of::<M>());
    }

    pub fn update<M: Material>(
        &mut self,
        device: &Device,
        texture_manager_2d: &TextureManager,
        handle: &MaterialHandle,
        material: M,
    ) {
        let type_id = self.handle_to_typeid[&**handle];

        assert_eq!(type_id, TypeId::of::<M>());

        let type_info = &mut self.storage[&type_id];

        let data_vec = type_info
            .data_vec
            .downcast_slice_mut::<Option<InternalMaterial<M>>>()
            .unwrap();
        let internal = data_vec[handle.idx].as_mut().unwrap();

        if let ProfileData::Cpu(ref mut index) = internal.bind_group_index {
            // Create the new bind group first. If the bind group didn't change, this will prevent
            // the bind group from dying on the call to remove.
            let bind_group_index =
                self.texture_deduplicator
                    .get_or_insert(device, texture_manager_2d, material.to_textures().as_ref());
            self.texture_deduplicator.remove(*index);
            *index = bind_group_index;
        }
        type_info.buffer.use_index(handle.idx);
        internal.inner = material;
    }

    pub fn remove(&mut self, handle: RawMaterialHandle) {
        let type_id = self.handle_to_typeid.remove(&handle).unwrap();

        let type_info = &mut self.storage[&type_id];
        let bind_group_index = (type_info.remove_data)(&mut type_info.data_vec, handle);

        if let ProfileData::Cpu(index) = bind_group_index {
            self.texture_deduplicator.remove(index);
        }
    }

    pub fn ready(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        scatter: &ScatterCopy,
        profile: RendererProfile,
        texture_manager: &TextureManager,
    ) {
        profiling::scope!("Material Ready");

        for (&type_id, type_info) in &mut self.storage {
            match profile {
                RendererProfile::CpuDriven => {
                    (type_info.apply_data_cpu)(&mut type_info.buffer, device, encoder, scatter, &mut type_info.data_vec)
                }
                RendererProfile::GpuDriven => (type_info.apply_data_gpu)(
                    &mut type_info.buffer,
                    device,
                    encoder,
                    scatter,
                    &mut type_info.data_vec,
                    texture_manager,
                ),
            }
        }
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

    pub(super) fn call_object_callback(&self, handle: RawMaterialHandle, args: ObjectCallbackArgs) {
        let type_id = self.handle_to_typeid[&handle];

        let type_info = &self.storage[&type_id];

        (type_info.object_callback)(&type_info.data_vec, handle.idx, args);
    }

    pub fn get_material_key(&mut self, handle: RawMaterialHandle) -> MaterialKeyPair {
        let type_id = self.handle_to_typeid[&handle];

        let type_info = &self.storage[&type_id];

        (type_info.get_material_key)(&type_info.data_vec, handle.idx)
    }

    pub fn get_objects(&mut self, handle: RawMaterialHandle) -> &mut Vec<RawObjectHandle> {
        todo!()
    }

    pub fn get_attributes(
        &self,
        handle: RawMaterialHandle,
        mut callback: impl FnMut(&[&'static VertexAttributeId], &[&'static VertexAttributeId]),
    ) {
        let type_id = self.handle_to_typeid[&handle];

        let type_info = &self.storage[&type_id];

        (type_info.get_attributes)(&mut callback)
    }
}

fn remove_data<M: Material>(
    data_vec: &mut VecAny,
    handle: RawMaterialHandle,
) -> ProfileData<TextureBindGroupIndex, ()> {
    let data_vec = data_vec.downcast_slice_mut::<Option<InternalMaterial<M>>>().unwrap();
    // This should be .take() instead of mem::take, but RA gets confused due to https://github.com/rust-lang/rust-analyzer/issues/6418
    let internal: InternalMaterial<M> = mem::take(&mut data_vec[handle.idx]).unwrap();

    internal.bind_group_index
}

fn apply_buffer_cpu<M: Material>(
    buffer: &mut FreelistDerivedBuffer,
    device: &Device,
    encoder: &mut CommandEncoder,
    scatter: &ScatterCopy,
    data_vec: &mut VecAny,
) {
    let data_vec = data_vec.downcast_slice::<Option<InternalMaterial<M>>>().unwrap();

    buffer.apply(device, encoder, scatter, |idx| {
        let material = data_vec[idx].as_ref().unwrap().inner;
        CpuPoweredShaderWrapper::<M> {
            data: material.to_data(),
            texture_enable: {
                let mut bits = 0x0;
                for t in material.to_textures().as_ref().into_iter().rev() {
                    bits |= t.is_some() as u32;
                    bits <<= 1;
                }
                bits
            },
        }
    });
}

fn apply_buffer_gpu<M: Material>(
    buffer: &mut FreelistDerivedBuffer,
    device: &Device,
    encoder: &mut CommandEncoder,
    scatter: &ScatterCopy,
    data_vec: &mut VecAny,
    texture_manager: &TextureManager,
) {
    let data_vec = data_vec.downcast_slice::<Option<InternalMaterial<M>>>().unwrap();

    let translation_fn = texture_manager.translation_fn();

    buffer.apply(device, encoder, scatter, |idx| {
        let material = data_vec[idx].as_ref().unwrap().inner;
        GpuPoweredShaderWrapper::<M> {
            textures: material
                .to_textures()
                .map_to_u32(|handle_opt| handle_opt.map(translation_fn).map_or(0, NonZeroU32::get)),
            data: material.to_data(),
        }
    });
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

fn object_callback_wrapper<M: Material>(vec_any: &VecAny, idx: usize, args: ObjectCallbackArgs) {
    let data_vec = vec_any.downcast_slice::<Option<InternalMaterial<M>>>().unwrap();

    let material = &data_vec[idx].as_ref().unwrap().inner;

    object_callback(material, args)
}
