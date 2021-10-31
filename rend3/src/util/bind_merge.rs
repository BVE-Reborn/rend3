use std::num::NonZeroU32;

use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingResource, BindingType, Buffer, Device, Sampler, ShaderStages, TextureView,
};

pub struct BindGroupLayoutBuilder {
    bgl_entries: Vec<BindGroupLayoutEntry>,
}
impl BindGroupLayoutBuilder {
    pub fn new() -> Self {
        Self {
            bgl_entries: Vec::with_capacity(16),
        }
    }

    pub fn append(&mut self, visibility: ShaderStages, ty: BindingType, count: Option<NonZeroU32>) -> &mut Self {
        let binding = self.bgl_entries.len() as u32;
        self.bgl_entries.push(BindGroupLayoutEntry {
            binding,
            visibility,
            ty,
            count,
        });
        self
    }

    pub fn build(&self, device: &Device, label: Option<&str>) -> BindGroupLayout {
        device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label,
            entries: &self.bgl_entries,
        })
    }
}

pub struct BindGroupBuilder<'a> {
    bg_entries: Vec<BindGroupEntry<'a>>,
}
impl<'a> BindGroupBuilder<'a> {
    pub fn new() -> Self {
        Self {
            bg_entries: Vec::with_capacity(16),
        }
    }

    pub fn append(&mut self, resource: BindingResource<'a>) -> &mut Self {
        let index = self.bg_entries.len();
        self.bg_entries.push(BindGroupEntry {
            binding: index as u32,
            resource,
        });
        self
    }

    pub fn append_buffer(&mut self, buffer: &'a Buffer) -> &mut Self {
        self.append(buffer.as_entire_binding());
        self
    }

    pub fn append_sampler(&mut self, sampler: &'a Sampler) -> &mut Self {
        self.append(BindingResource::Sampler(sampler));
        self
    }

    pub fn append_texture_view(&mut self, texture_view: &'a TextureView) -> &mut Self {
        self.append(BindingResource::TextureView(texture_view));
        self
    }

    pub fn append_texture_view_array(&mut self, texture_view_array: &'a [&'a TextureView]) -> &mut Self {
        self.append(BindingResource::TextureViewArray(texture_view_array));
        self
    }

    pub fn build(&self, device: &Device, label: Option<&str>, bgl: &BindGroupLayout) -> BindGroup {
        device.create_bind_group(&BindGroupDescriptor {
            label,
            layout: bgl,
            entries: &self.bg_entries,
        })
    }
}
