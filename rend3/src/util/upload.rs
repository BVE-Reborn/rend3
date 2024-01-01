use wgpu::{Buffer, CommandEncoder, Device};

use crate::util::error_scope::AllocationErrorScope;

struct Upload<'a> {
    staging_offset: u64,
    offset: u64,
    data: &'a [u8],
}

pub struct UploadChainer<'a> {
    staging_buffer: Option<wgpu::Buffer>,
    uploads: Vec<Upload<'a>>,
    total_size: u64,
}

impl<'a> UploadChainer<'a> {
    pub fn new() -> Self {
        Self {
            staging_buffer: None,
            uploads: Vec::new(),
            total_size: 0,
        }
    }

    pub fn add(&mut self, offset: u64, data: &'a [u8]) {
        self.uploads.push(Upload {
            staging_offset: self.total_size,
            offset,
            data,
        });
        self.total_size += data.len() as u64;
    }

    pub fn create_staging_buffer(&mut self, device: &Device) -> Result<(), wgpu::Error> {
        let scope = AllocationErrorScope::new(device);
        self.staging_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh staging buffer"),
            size: self.total_size,
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::MAP_WRITE,
            mapped_at_creation: true,
        }));
        scope.end()?;

        Ok(())
    }

    pub fn encode_upload(&self, encoder: &mut CommandEncoder, buffer: &Buffer) {
        let staging_buffer = self.staging_buffer.as_ref().unwrap();

        for upload in &self.uploads {
            encoder.copy_buffer_to_buffer(
                staging_buffer,
                upload.staging_offset,
                buffer,
                upload.offset,
                upload.data.len() as u64,
            );
        }
    }

    pub fn stage(&mut self) {
        let staging_buffer = self.staging_buffer.as_ref().unwrap();

        let mut mapping = staging_buffer.slice(..).get_mapped_range_mut();
        for upload in &self.uploads {
            mapping[upload.staging_offset as usize..][..upload.data.len()].copy_from_slice(upload.data);
        }
        drop(mapping);

        staging_buffer.unmap();
    }
}

impl<'a> Default for UploadChainer<'a> {
    fn default() -> Self {
        Self::new()
    }
}
