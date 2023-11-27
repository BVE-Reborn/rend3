//! Automatic management of Power-of-Two sized buffers.

use std::{marker::PhantomData, ops::Deref, sync::Arc};

use encase::{private::WriteInto, ShaderType};
use wgpu::{Buffer, BufferAddress, BufferDescriptor, BufferUsages, Device, Queue};

use crate::util::typedefs::SsoString;

/// Creates, fills, and automatically resizes a power-of-two sized buffer.
#[derive(Debug)]
pub struct WrappedPotBuffer<T> {
    inner: Arc<Buffer>,
    size: BufferAddress,
    // This field is assumed to be a power of 2.
    minimum: BufferAddress,
    usage: BufferUsages,
    label: SsoString,
    _phantom: PhantomData<T>,
}

impl<T> WrappedPotBuffer<T>
where
    T: ShaderType + WriteInto,
{
    pub fn new(device: &Device, usage: BufferUsages, label: &str) -> Self {
        profiling::scope!("WrappedPotBuffer::new");

        let minimum = T::min_size().get().next_power_of_two().max(4);

        let usage = usage | BufferUsages::COPY_DST;

        Self {
            inner: Arc::new(device.create_buffer(&BufferDescriptor {
                label: Some(label),
                size: minimum,
                usage,
                mapped_at_creation: false,
            })),
            size: minimum,
            minimum,
            usage,
            label: SsoString::from(label),
            _phantom: PhantomData,
        }
    }

    fn ensure_size(&mut self, device: &Device, desired: BufferAddress) {
        let resize = resize_po2(self.size, desired, self.minimum);
        if let Some(size) = resize {
            self.size = size;
            self.inner = Arc::new(device.create_buffer(&BufferDescriptor {
                label: Some(&self.label),
                size,
                usage: self.usage,
                mapped_at_creation: false,
            }));
        }
    }

    pub fn write_to_buffer(&mut self, device: &Device, queue: &Queue, data: &T) {
        let size = data.size();
        self.ensure_size(device, size.get());

        let mut mapped = queue.write_buffer_with(&self.inner, 0, size).unwrap();
        encase::StorageBuffer::new(&mut *mapped).write(data).unwrap();
        drop(mapped);
    }
}

impl<T> Deref for WrappedPotBuffer<T> {
    type Target = Buffer;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

fn resize_po2(current: BufferAddress, desired: BufferAddress, minimum: BufferAddress) -> Option<BufferAddress> {
    assert!(current.is_power_of_two());
    if current == minimum && desired <= minimum {
        return None;
    }
    let lower_bound = current / 4;
    if desired <= lower_bound || current < desired {
        Some((desired + 1).next_power_of_two())
    } else {
        None
    }
}

#[cfg(test)]
mod test {
    use super::resize_po2;

    #[test]
    fn automated_buffer_resize() {
        assert_eq!(resize_po2(64, 128, 0), Some(256));
        assert_eq!(resize_po2(128, 128, 0), None);
        assert_eq!(resize_po2(256, 128, 0), None);

        assert_eq!(resize_po2(64, 64, 0), None);
        assert_eq!(resize_po2(128, 64, 0), None);
        assert_eq!(resize_po2(256, 65, 0), None);
        assert_eq!(resize_po2(256, 64, 0), Some(128));
        assert_eq!(resize_po2(256, 63, 0), Some(64));

        assert_eq!(resize_po2(16, 16, 0), None);
        assert_eq!(resize_po2(16, 8, 0), None);
        assert_eq!(resize_po2(16, 4, 0), Some(8));
    }
}
