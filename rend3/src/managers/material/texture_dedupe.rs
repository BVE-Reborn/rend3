use std::ops::Index;

use arrayvec::ArrayVec;
use bimap::BiMap;
use rend3_types::RawTexture2DHandle;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingResource, Device, ShaderStages, TextureViewDimension,
};

use crate::{
    managers::TextureManager,
    util::freelist::{FreelistIndex, FreelistVec},
};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextureBindGroupIndex(FreelistIndex);

impl std::fmt::Debug for TextureBindGroupIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut tuple = f.debug_tuple("TextureBindGroupIndex");
        if *self == Self::DUMMY {
            tuple.field(&"DUMMY");
        } else {
            tuple.field(&self.0 .0);
        }
        tuple.finish()
    }
}

impl TextureBindGroupIndex {
    pub const DUMMY: Self = Self(FreelistIndex(usize::MAX));
}

struct StoredBindGroup {
    refcount: usize,
    inner: BindGroup,
}

pub struct TextureDeduplicator {
    bgls: Vec<BindGroupLayout>,
    deduplication_map: BiMap<Vec<Option<RawTexture2DHandle>>, TextureBindGroupIndex>,
    storage: FreelistVec<StoredBindGroup>,
}
impl TextureDeduplicator {
    pub fn new(device: &Device) -> Self {
        let entries: Vec<_> = (0..16)
            .map(|i| BindGroupLayoutEntry {
                binding: i,
                visibility: ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            })
            .collect();

        let bgls = (0..16_usize)
            .map(|max| {
                let max_name = max + 1;
                device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                    label: Some(&format!("rend3 texture BGL for {max_name} textures")),
                    entries: &entries[0..max],
                })
            })
            .collect();

        Self { bgls, deduplication_map: BiMap::default(), storage: FreelistVec::new() }
    }

    pub fn get_or_insert(
        &mut self,
        device: &Device,
        texture_manager_2d: &TextureManager<crate::types::Texture2DTag>,
        array: &[Option<RawTexture2DHandle>],
    ) -> TextureBindGroupIndex {
        if let Some(&index) = self.deduplication_map.get_by_left(array) {
            self.storage[index.0].refcount += 1;

            return index;
        }

        let entries: ArrayVec<_, 32> = array
            .iter()
            .enumerate()
            .map(|(idx, handle)| {
                let view = if let Some(handle) = *handle {
                    texture_manager_2d.get_view(&handle)
                } else {
                    texture_manager_2d.get_null_view()
                };

                BindGroupEntry { binding: idx as u32, resource: BindingResource::TextureView(view) }
            })
            .collect();

        let bg = device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.bgls[array.len()],
            entries: &entries,
        });

        let index = self.storage.push(StoredBindGroup { refcount: 1, inner: bg });
        let index = TextureBindGroupIndex(index);

        self.deduplication_map.insert(array.to_vec(), index);

        index
    }

    pub fn remove(&mut self, index: TextureBindGroupIndex) {
        let refcount = &mut self.storage[index.0].refcount;
        *refcount = refcount.checked_sub(1).unwrap();

        if *refcount == 0 {
            self.storage.remove(index.0);
            self.deduplication_map.remove_by_right(&index);
        }
    }

    pub fn get_bgl(&self, count: usize) -> &BindGroupLayout {
        &self.bgls[count]
    }
}

impl Index<TextureBindGroupIndex> for TextureDeduplicator {
    type Output = BindGroup;

    fn index(&self, index: TextureBindGroupIndex) -> &Self::Output {
        &self.storage[index.0].inner
    }
}
