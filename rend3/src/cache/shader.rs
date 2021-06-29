use std::{hash::Hash, sync::Arc};

use wgpu::{Device, ShaderFlags, ShaderModule, ShaderModuleDescriptor, ShaderSource};

use crate::{
    cache::Cached,
    util::typedefs::{FastHashMap, SsoString},
};

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum OwnedShaderSource {
    SpirV(Vec<u32>),
    Wgsl(String),
}

struct AddressedShaderModuleDescriptor {
    label: Option<SsoString>,
    source: OwnedShaderSource,
    flags: ShaderFlags,
}

impl AddressedShaderModuleDescriptor {
    fn from_wgpu(desc: &ShaderModuleDescriptor) -> Self {
        Self {
            label: desc.label.map(SsoString::from),
            source: match desc.source {
                ShaderSource::SpirV(ref spv) => OwnedShaderSource::SpirV(spv.to_vec()),
                ShaderSource::Wgsl(ref wgs) => OwnedShaderSource::Wgsl(wgs.to_string()),
            },
            flags: desc.flags,
        }
    }
}

impl PartialEq for AddressedShaderModuleDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label && self.source == other.source && self.flags == other.flags
    }
}

impl Eq for AddressedShaderModuleDescriptor {}

impl Hash for AddressedShaderModuleDescriptor {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.label.hash(state);
        self.source.hash(state);
        self.flags.bits().hash(state);
    }
}

pub struct ShaderModuleCache {
    cache: FastHashMap<AddressedShaderModuleDescriptor, Cached<ShaderModule>>,
    current_epoch: usize,
}
impl ShaderModuleCache {
    pub fn new() -> Self {
        Self {
            cache: FastHashMap::default(),
            current_epoch: 0,
        }
    }

    pub fn mark_new_epoch(&mut self) {
        self.current_epoch += 1;
    }

    pub fn clear_old_epochs(&mut self) {
        let current_epoch = self.current_epoch;
        self.cache.retain(|_, v| v.epoch == current_epoch);
    }

    pub fn shader_module(&mut self, device: &Device, module: &ShaderModuleDescriptor<'_>) -> Arc<ShaderModule> {
        let sm_key = AddressedShaderModuleDescriptor::from_wgpu(module);

        let current_epoch = self.current_epoch;
        let module = self.cache.entry(sm_key).or_insert_with(|| {
            let module = device.create_shader_module(module);

            Cached {
                inner: Arc::new(module),
                epoch: current_epoch,
            }
        });
        module.epoch = current_epoch;

        Arc::clone(&module.inner)
    }
}
