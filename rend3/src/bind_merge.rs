use std::sync::Arc;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    Device,
};

pub struct BindGroupManager {
    layout: Option<Arc<BindGroupLayout>>,
    layout_entries: Vec<BindGroupLayoutEntry>,
    layout_dirty: bool,
    label: Option<String>,
}
impl BindGroupManager {
    pub fn new(device: &Device, entries: Option<Vec<BindGroupLayoutEntry>>, label: Option<String>) -> Self {
        Self {
            layout: entries.as_ref().map(|entries| {
                let label = match label {
                    Some(ref s) => Some(format!("{} bgl", s)),
                    None => None,
                };
                Arc::new(device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                    label: label.as_deref(),
                    entries,
                }))
            }),
            layout_entries: entries.unwrap_or_else(|| Vec::with_capacity(16)),
            layout_dirty: false,
            label,
        }
    }

    pub fn layout(&self) -> &BindGroupLayout {
        self.layout.as_ref().unwrap()
    }

    pub fn builder(&mut self) -> BindGroupBuilder<'_> {
        BindGroupBuilder::new(self)
    }
}

pub struct BindGroupBuilder<'a> {
    manager: &'a mut BindGroupManager,
    bindings: Vec<BindGroupEntry<'a>>,
}
impl<'a> BindGroupBuilder<'a> {
    fn new(manager: &'a mut BindGroupManager) -> Self {
        Self {
            manager,
            bindings: Vec::with_capacity(16),
        }
    }

    pub fn append(&mut self, layout: Option<BindGroupLayoutEntry>, binding: BindGroupEntry<'a>) {
        let index = self.bindings.len();
        self.bindings.push(BindGroupEntry {
            binding: index as u32,
            ..binding
        });

        if let Some(layout) = layout {
            let eq = if let Some(entry) = self.manager.layout_entries.get(index) {
                let count_eq = layout.count == entry.count;
                let ty_eq = layout.ty == entry.ty;
                let vis_eq = layout.visibility == entry.visibility;
                count_eq && ty_eq && vis_eq
            } else {
                false
            };

            if !eq {
                let layout = BindGroupLayoutEntry {
                    binding: index as u32,
                    ..layout
                };
                match self.manager.layout_entries.get_mut(index) {
                    Some(layout_ref) => *layout_ref = layout,
                    None => {
                        self.manager.layout_entries.push(layout);
                        debug_assert_eq!(self.manager.layout_entries.len(), self.bindings.len())
                    }
                }
                self.manager.layout_dirty = true;
            }
        }
    }

    pub fn build(self, device: &Device) -> (Arc<BindGroupLayout>, BindGroup, bool) {
        if self.manager.layout_entries.len() > self.bindings.len() {
            self.manager.layout_entries.drain(self.bindings.len()..);
            self.manager.layout_dirty = true;
        }

        let bgl_updated = if self.manager.layout_dirty {
            let label = match self.manager.label {
                Some(ref s) => Some(format!("{} bgl", s)),
                None => None,
            };
            self.manager.layout = Some(Arc::new(device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: label.as_deref(),
                entries: &self.manager.layout_entries,
            })));

            true
        } else {
            false
        };

        let label = match self.manager.label {
            Some(ref s) => Some(format!("{} bg", s)),
            None => None,
        };
        let layout = self.manager.layout.as_ref().unwrap();
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: label.as_deref(),
            layout,
            entries: &self.bindings,
        });

        (Arc::clone(layout), bind_group, bgl_updated)
    }
}
