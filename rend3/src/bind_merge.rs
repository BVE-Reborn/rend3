use wgpu::{BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, Device};

pub struct BindGroupBuilder<'a> {
    bindings: Vec<BindGroupEntry<'a>>,
    label: Option<String>,
}
impl<'a> BindGroupBuilder<'a> {
    pub fn new(label: Option<String>) -> Self {
        Self {
            label,
            bindings: Vec::with_capacity(16),
        }
    }

    pub fn append(&mut self, binding: BindGroupEntry<'a>) {
        let index = self.bindings.len();
        self.bindings.push(BindGroupEntry {
            binding: index as u32,
            ..binding
        });
    }

    pub fn build(self, device: &Device, layout: &BindGroupLayout) -> BindGroup {
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: self.label.as_deref(),
            layout,
            entries: &self.bindings,
        });

        bind_group
    }
}
