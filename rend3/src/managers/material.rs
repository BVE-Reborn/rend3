use std::{
    any::TypeId,
    mem,
    num::{NonZeroU32, NonZeroU64},
};

use encase::{ShaderSize, ShaderType};
use rend3_types::{Material, MaterialArray, RawMaterialHandle, RawTexture2DHandle, VertexAttributeId, WasmVecAny};
use wgpu::{BindGroup, BindGroupLayout, BindingType, Buffer, BufferBindingType, CommandEncoder, Device, ShaderStages};

use crate::{
    managers::{object_add_callback, ObjectAddCallbackArgs, TextureManager},
    profile::ProfileData,
    util::{
        bind_merge::BindGroupLayoutBuilder, freelist::FreelistDerivedBuffer, math::round_up, scatter_copy::ScatterCopy,
        typedefs::FastHashMap,
    },
    RendererProfile,
};

mod texture_dedupe;

pub use texture_dedupe::TextureBindGroupIndex;

#[derive(ShaderType)]
struct GpuPoweredShaderWrapper<M: Material> {
    textures: <M::TextureArrayType as MaterialArray<Option<RawTexture2DHandle>>>::U32Array,
    data: M::DataType,
}

#[derive(ShaderType)]
struct CpuPoweredShaderWrapper<M: Material> {
    data: M::DataType,
    texture_enable: u32,
}

/// Internal representation of a material.
pub struct InternalMaterial<M> {
    pub bind_group_index: ProfileData<TextureBindGroupIndex, ()>,
    pub inner: M,
}

struct MaterialArchetype {
    // Inner type is CpuPoweredShaderWrapper<M> or GpuPoweredShaderWrapper<M>
    buffer: FreelistDerivedBuffer,
    // Inner type is Option<InnerMaterial<M>>
    data_vec: WasmVecAny,
    remove_data: fn(&mut WasmVecAny, RawMaterialHandle) -> ProfileData<TextureBindGroupIndex, ()>,
    apply_data_cpu: fn(&mut FreelistDerivedBuffer, &Device, &mut CommandEncoder, &ScatterCopy, &mut WasmVecAny),
    apply_data_gpu: fn(
        &mut FreelistDerivedBuffer,
        &Device,
        &mut CommandEncoder,
        &ScatterCopy,
        &mut WasmVecAny,
        &TextureManager<crate::types::Texture2DTag>,
    ),
    #[allow(clippy::type_complexity)]
    get_attributes: fn(&mut dyn FnMut(&[&'static VertexAttributeId], &[&'static VertexAttributeId])),
    object_add_callback_wrapper: fn(&WasmVecAny, usize, ObjectAddCallbackArgs<'_>),
}

pub struct MaterialArchetypeView<'a, M: Material> {
    buffer: &'a Buffer,
    data_vec: &'a [Option<InternalMaterial<M>>],
}

impl<'a, M: Material> Copy for MaterialArchetypeView<'a, M> {}

impl<'a, M: Material> Clone for MaterialArchetypeView<'a, M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, M: Material> MaterialArchetypeView<'a, M> {
    pub fn buffer(&self) -> &'a Buffer {
        self.buffer
    }

    pub fn material(&self, handle: RawMaterialHandle) -> &'a InternalMaterial<M> {
        self.data_vec[handle.idx].as_ref().unwrap()
    }
}

/// Key which determine's an object's archetype.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct MaterialKeyPair {
    /// TypeId of the material the object uses.
    pub ty: TypeId,
    /// Per-material data.
    pub key: u64,
}

/// Manages materials and their associated BindGroups in CPU modes.
pub struct MaterialManager {
    handle_to_typeid: FastHashMap<RawMaterialHandle, TypeId>,
    archetypes: FastHashMap<TypeId, MaterialArchetype>,

    texture_deduplicator: texture_dedupe::TextureDeduplicator,
}

impl MaterialManager {
    pub fn new(device: &Device) -> Self {
        profiling::scope!("MaterialManager::new");

        let texture_deduplicator = texture_dedupe::TextureDeduplicator::new(device);

        Self { handle_to_typeid: FastHashMap::default(), archetypes: FastHashMap::default(), texture_deduplicator }
    }

    pub fn ensure_archetype<M: Material>(&mut self, device: &Device, profile: RendererProfile) {
        self.ensure_archetype_inner::<M>(device, profile);
    }

    fn ensure_archetype_inner<M: Material>(
        &mut self,
        device: &Device,
        profile: RendererProfile,
    ) -> &mut MaterialArchetype {
        profiling::scope!("MaterialManager::ensure_archetype");

        let ty = TypeId::of::<M>();

        self.archetypes.entry(ty).or_insert_with(|| MaterialArchetype {
            buffer: match profile {
                RendererProfile::CpuDriven => FreelistDerivedBuffer::new::<CpuPoweredShaderWrapper<M>>(device),
                RendererProfile::GpuDriven => FreelistDerivedBuffer::new::<GpuPoweredShaderWrapper<M>>(device),
            },
            data_vec: WasmVecAny::new::<Option<InternalMaterial<M>>>(),
            remove_data: remove_data::<M>,
            apply_data_cpu: apply_buffer_cpu::<M>,
            apply_data_gpu: apply_buffer_gpu::<M>,
            get_attributes: get_attributes::<M>,
            object_add_callback_wrapper: object_add_callback_wrapper::<M>,
        })
    }

    pub fn add<M: Material>(
        &mut self,
        device: &Device,
        profile: RendererProfile,
        texture_manager_2d: &mut TextureManager<crate::types::Texture2DTag>,
        handle: &RawMaterialHandle,
        material: M,
    ) {
        let bind_group_index = if profile == RendererProfile::CpuDriven {
            let textures = material.to_textures();

            let texture_bg_index =
                self.texture_deduplicator.get_or_insert(device, texture_manager_2d, textures.as_ref());

            ProfileData::Cpu(texture_bg_index)
        } else {
            ProfileData::Gpu(())
        };

        let archetype = self.ensure_archetype_inner::<M>(device, profile);
        archetype.buffer.use_index(handle.idx);

        let mut data_vec = archetype.data_vec.downcast_mut::<Option<InternalMaterial<M>>>().unwrap();
        if handle.idx >= data_vec.len() {
            data_vec.resize_with((handle.idx + 1).next_power_of_two(), || None);
        }
        data_vec[handle.idx] = Some(InternalMaterial { bind_group_index, inner: material });
        drop(data_vec);

        self.handle_to_typeid.insert(*handle, TypeId::of::<M>());
    }

    pub fn update<M: Material>(
        &mut self,
        device: &Device,
        texture_manager_2d: &TextureManager<crate::types::Texture2DTag>,
        handle: &RawMaterialHandle,
        material: M,
    ) {
        let type_id = self.handle_to_typeid[&handle];

        assert_eq!(type_id, TypeId::of::<M>());

        let archetype = self.archetypes.get_mut(&type_id).unwrap();

        let data_vec = archetype.data_vec.downcast_slice_mut::<Option<InternalMaterial<M>>>().unwrap();
        let internal = data_vec[handle.idx].as_mut().unwrap();

        if let ProfileData::Cpu(ref mut index) = internal.bind_group_index {
            // Create the new bind group first. If the bind group didn't change, this will prevent
            // the bind group from dying on the call to remove.
            let bind_group_index =
                self.texture_deduplicator.get_or_insert(device, texture_manager_2d, material.to_textures().as_ref());
            self.texture_deduplicator.remove(*index);
            *index = bind_group_index;
        }
        archetype.buffer.use_index(handle.idx);
        internal.inner = material;
    }

    pub fn remove(&mut self, handle: &RawMaterialHandle) {
        let type_id = self.handle_to_typeid.remove(handle).unwrap();

        let archetype = self.archetypes.get_mut(&type_id).unwrap();
        let bind_group_index = (archetype.remove_data)(&mut archetype.data_vec, *handle);

        if let ProfileData::Cpu(index) = bind_group_index {
            self.texture_deduplicator.remove(index);
        }
    }

    pub fn evaluate(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        scatter: &ScatterCopy,
        profile: RendererProfile,
        texture_manager: &TextureManager<crate::types::Texture2DTag>,
    ) {
        profiling::scope!("MaterialManager::evaluate");

        for archetype in self.archetypes.values_mut() {
            match profile {
                RendererProfile::CpuDriven => {
                    (archetype.apply_data_cpu)(&mut archetype.buffer, device, encoder, scatter, &mut archetype.data_vec)
                }
                RendererProfile::GpuDriven => (archetype.apply_data_gpu)(
                    &mut archetype.buffer,
                    device,
                    encoder,
                    scatter,
                    &mut archetype.data_vec,
                    texture_manager,
                ),
            }
        }
    }

    pub fn archetype_view<M: Material>(&self) -> MaterialArchetypeView<'_, M> {
        let archetype = &self.archetypes[&TypeId::of::<M>()];

        MaterialArchetypeView { buffer: &archetype.buffer, data_vec: archetype.data_vec.downcast_slice().unwrap() }
    }

    pub fn texture_bind_group(&self, index: TextureBindGroupIndex) -> &BindGroup {
        &self.texture_deduplicator[index]
    }

    pub fn add_to_bgl_gpu<M: Material>(bglb: &mut BindGroupLayoutBuilder) {
        bglb.append(
            ShaderStages::VERTEX_FRAGMENT,
            BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(
                    (round_up(M::TextureArrayType::COUNT * 4, 16) + round_up(M::DataType::SHADER_SIZE.get() as u32, 16))
                        as _,
                ),
            },
            None,
        );
    }

    pub(super) fn call_object_add_callback(&self, handle: &RawMaterialHandle, args: ObjectAddCallbackArgs) {
        let type_id = self.handle_to_typeid[handle];

        let archetype = &self.archetypes[&type_id];

        (archetype.object_add_callback_wrapper)(&archetype.data_vec, handle.idx, args);
    }

    pub fn get_bind_group_layout_cpu<M: Material>(&self) -> &BindGroupLayout {
        self.texture_deduplicator
            .get_bgl(<M::TextureArrayType as MaterialArray<Option<RawTexture2DHandle>>>::COUNT as usize)
    }

    pub fn get_attributes(
        &self,
        handle: RawMaterialHandle,
        mut callback: impl FnMut(&[&'static VertexAttributeId], &[&'static VertexAttributeId]),
    ) {
        let type_id = self.handle_to_typeid[&handle];

        let archetype = &self.archetypes[&type_id];

        (archetype.get_attributes)(&mut callback)
    }
}

fn remove_data<M: Material>(
    data_vec: &mut WasmVecAny,
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
    data_vec: &mut WasmVecAny,
) {
    let data_vec = data_vec.downcast_slice::<Option<InternalMaterial<M>>>().unwrap();

    buffer.apply(device, encoder, scatter, |idx| {
        let material = &data_vec[idx].as_ref().unwrap().inner;
        CpuPoweredShaderWrapper::<M> {
            data: material.to_data(),
            texture_enable: {
                let mut bits = 0x0;
                for t in material.to_textures().as_ref().iter().rev() {
                    // Shift must happen first, if it happens second, the last bit will also be shifted
                    bits <<= 1;
                    bits |= t.is_some() as u32;
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
    data_vec: &mut WasmVecAny,
    texture_manager: &TextureManager<crate::types::Texture2DTag>,
) {
    let data_vec = data_vec.downcast_slice::<Option<InternalMaterial<M>>>().unwrap();

    let translation_fn = texture_manager.translation_fn();

    buffer.apply(device, encoder, scatter, |idx| {
        let material = &data_vec[idx].as_ref().unwrap().inner;
        GpuPoweredShaderWrapper::<M> {
            textures: material
                .to_textures()
                .map_to_u32(|handle_opt| handle_opt.map(translation_fn).map_or(0, NonZeroU32::get)),
            data: material.to_data(),
        }
    });
}

fn get_attributes<M: Material>(callback: &mut dyn FnMut(&[&'static VertexAttributeId], &[&'static VertexAttributeId])) {
    callback(M::required_attributes().as_ref(), M::supported_attributes().as_ref())
}

fn object_add_callback_wrapper<M: Material>(vec_any: &WasmVecAny, idx: usize, args: ObjectAddCallbackArgs) {
    let data_vec = vec_any.downcast_slice::<Option<InternalMaterial<M>>>().unwrap();

    let material = &data_vec[idx].as_ref().unwrap().inner;

    object_add_callback(material, args)
}
