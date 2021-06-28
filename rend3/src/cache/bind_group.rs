use crate::{
    cache::{Cached, ParentedCached},
    util::typedefs::{FastHashMap, SsoString},
};
use std::{num::NonZeroU32, sync::Arc};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingResource, BindingType, Buffer, BufferAddress, BufferSize, Device, Sampler, ShaderStage, TextureView,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AddressedBindGroupDescriptor {
    label: Option<SsoString>,
    entries: Vec<AddressedBindGroupEntry>,
}

impl AddressedBindGroupDescriptor {
    fn from_wgpu<L>(label: Option<L>, bgl_entries: &[BindGroupLayoutEntry], bg_entries: &[BindGroupEntry<'_>]) -> Self
    where
        SsoString: From<L>,
    {
        Self {
            label: label.map(SsoString::from),
            entries: bgl_entries
                .iter()
                .zip(bg_entries.iter())
                .map(AddressedBindGroupEntry::from_wgpu)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AddressedBindGroupEntry {
    pub binding: u32,
    pub layout: AddressedBindGroupLayoutEntry,
    pub resource: AddressedBindingResource,
}

impl AddressedBindGroupEntry {
    fn from_wgpu((bgl_entry, bg_entry): (&BindGroupLayoutEntry, &BindGroupEntry)) -> Self {
        assert_eq!(
            bgl_entry.binding, bg_entry.binding,
            "The bind group cache requires the binding indexes be in the same order."
        );
        AddressedBindGroupEntry {
            binding: bg_entry.binding,
            layout: AddressedBindGroupLayoutEntry::from_wgpu(bgl_entry),
            resource: AddressedBindingResource::from_wgpu(&bg_entry.resource),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AddressedBindGroupLayoutEntry {
    pub visibility: ShaderStage,
    pub ty: BindingType,
    pub count: Option<NonZeroU32>,
}

impl AddressedBindGroupLayoutEntry {
    fn from_wgpu(bgl_entry: &BindGroupLayoutEntry) -> Self {
        AddressedBindGroupLayoutEntry {
            visibility: bgl_entry.visibility,
            ty: bgl_entry.ty,
            count: bgl_entry.count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum AddressedBindingResource {
    Buffer {
        buffer: usize,
        offset: BufferAddress,
        size: Option<BufferSize>,
    },
    Sampler(usize),
    TextureView(usize),
    TextureViewArray(Vec<usize>),
}

impl AddressedBindingResource {
    fn from_wgpu(resource: &BindingResource) -> Self {
        match *resource {
            BindingResource::Buffer { buffer, offset, size } => Self::Buffer {
                buffer: buffer as *const Buffer as usize,
                offset,
                size,
            },
            BindingResource::Sampler(s) => Self::Sampler(s as *const Sampler as usize),
            BindingResource::TextureView(v) => Self::TextureView(v as *const TextureView as usize),
            BindingResource::TextureViewArray(views) => {
                Self::TextureViewArray(views.iter().map(|&v| v as *const TextureView as usize).collect())
            }
            _ => unreachable!(),
        }
    }
}

pub struct BindGroupCache {
    bgl_cache: FastHashMap<Vec<BindGroupLayoutEntry>, Cached<BindGroupLayout>>,
    bg_cache: FastHashMap<AddressedBindGroupDescriptor, ParentedCached<BindGroup, BindGroupLayout>>,
    current_epoch: usize,
}

impl BindGroupCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            bgl_cache: FastHashMap::default(),
            bg_cache: FastHashMap::default(),
            current_epoch: 0,
        }
    }

    pub fn mark_new_epoch(&mut self) {
        self.current_epoch += 1;
    }

    pub fn clear_old_epochs(&mut self) {
        let current_epoch = self.current_epoch;
        self.bgl_cache.retain(|_, v| v.epoch == current_epoch);
        self.bg_cache.retain(|_, v| v.epoch == current_epoch);
    }

    pub fn bind_group(
        &mut self,
        device: &Device,
        label: Option<&str>,
        bgl_entries: &[BindGroupLayoutEntry],
        bg_entries: &[BindGroupEntry<'_>],
    ) -> (Arc<BindGroupLayout>, Arc<BindGroup>) {
        let label_deref = label.map(SsoString::from);
        let bg_key = AddressedBindGroupDescriptor::from_wgpu(label, bgl_entries, bg_entries);

        let current_epoch = self.current_epoch;
        let bg_cache = &mut self.bg_cache;
        let bgl_cache = &mut self.bgl_cache;
        let bind_group = bg_cache.entry(bg_key).or_insert_with(|| {
            let bgl_key = bgl_entries.to_vec();
            let bind_group_layout = bgl_cache.entry(bgl_key).or_insert_with(|| Cached {
                inner: Arc::new(device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                    label: label_deref.as_deref(),
                    entries: bgl_entries,
                })),
                epoch: current_epoch,
            });
            bind_group_layout.epoch = current_epoch;

            ParentedCached {
                inner: Arc::new(device.create_bind_group(&BindGroupDescriptor {
                    label: label_deref.as_deref(),
                    layout: &bind_group_layout.inner,
                    entries: bg_entries,
                })),
                epoch: current_epoch,
                parent: Arc::clone(&bind_group_layout.inner),
            }
        });

        bind_group.epoch = self.current_epoch;
        (Arc::clone(&bind_group.parent), Arc::clone(&bind_group.inner))
    }
}

impl Default for BindGroupCache {
    fn default() -> Self {
        Self::new()
    }
}
