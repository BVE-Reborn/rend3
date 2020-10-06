use crate::datatypes::{DirectionalLight, DirectionalLightHandle};
use crate::registry::ResourceRegistry;

pub struct DirectionalLightManager {
    registry: ResourceRegistry<DirectionalLight>,
}
impl DirectionalLightManager {
    pub fn new() -> Self {
        let registry = ResourceRegistry::new();

        Self { registry }
    }

    pub fn allocate(&self) -> DirectionalLightHandle {
        DirectionalLightHandle(self.registry.allocate())
    }

    pub fn fill(&mut self, handle: DirectionalLightHandle, light: DirectionalLight) {
        self.registry.insert(handle.0, light);
    }

    pub fn ready(&mut self) {
        todo!()
    }
}
