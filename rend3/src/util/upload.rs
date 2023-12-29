use wgpu::{Buffer, CommandEncoder, Device};

use crate::util::error_scope::AllocationErrorScope;

pub fn upload_buffer_via_encoder(
    device: &Device,
    encoder: &mut CommandEncoder,
    buffer: &Buffer,
    offset: u64,
    data: &[u8],
) -> Result<(), wgpu::Error> {
    let scope = AllocationErrorScope::new(device);
    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mesh staging buffer"),
        size: data.len() as u64,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::MAP_WRITE,
        mapped_at_creation: true,
    });
    scope.end()?;

    let mut mapping = staging_buffer.slice(..).get_mapped_range_mut();
    mapping.copy_from_slice(data);
    drop(mapping);

    staging_buffer.unmap();

    encoder.copy_buffer_to_buffer(&staging_buffer, 0, buffer, offset, data.len() as u64);

    Ok(())
}
