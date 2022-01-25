//! Automatic management of Power-of-Two sized buffers.

use crate::util::typedefs::SsoString;
use std::{ops::Deref, sync::Arc};
use wgpu::{Buffer, BufferAddress, BufferDescriptor, BufferUsages, Device, Queue};

/// Creates, fills, and automatically resizes a power-of-two sized buffer.
pub struct WrappedPotBuffer {
    inner: Arc<Buffer>,
    size: BufferAddress,
    // This field is assumed to be a power of 2.
    minimum: BufferAddress,
    usage: BufferUsages,
    label: Option<SsoString>,
}

impl WrappedPotBuffer {
    pub fn new<T>(
        device: &Device,
        size: BufferAddress,
        minimum: BufferAddress,
        usage: BufferUsages,
        label: Option<T>,
    ) -> Self
    where
        SsoString: From<T>,
        T: Deref<Target = str>,
    {
        profiling::scope!("WrappedPotBuffer::new");

        let minimum_pot = (minimum - 1).next_power_of_two().max(16);
        let starting_size = if size <= minimum_pot {
            minimum_pot
        } else {
            (size - 1).next_power_of_two()
        };

        let usage = usage | BufferUsages::COPY_DST;

        Self {
            inner: Arc::new(device.create_buffer(&BufferDescriptor {
                label: label.as_deref(),
                size: starting_size,
                usage,
                mapped_at_creation: false,
            })),
            size: starting_size,
            minimum: minimum_pot,
            usage,
            label: label.map(SsoString::from),
        }
    }

    /// Determines if the buffer will resize given the desired size.
    pub fn will_resize(&self, desired: BufferAddress) -> Option<BufferAddress> {
        will_resize_inner(self.size, desired, self.minimum)
    }

    pub fn ensure_size(&mut self, device: &Device, desired: BufferAddress) -> Option<BufferAddress> {
        let resize = self.will_resize(desired);
        if let Some(size) = resize {
            self.size = size;
            self.inner = Arc::new(device.create_buffer(&BufferDescriptor {
                label: self.label.as_deref(),
                size,
                usage: self.usage,
                mapped_at_creation: false,
            }));
        }
        resize
    }

    pub fn write_to_buffer(&mut self, device: &Device, queue: &Queue, data: &[u8]) -> bool {
        let resize = self.ensure_size(device, data.len() as u64);

        queue.write_buffer(&self.inner, 0, data);

        resize.is_some()
    }
}

impl Deref for WrappedPotBuffer {
    type Target = Arc<Buffer>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

fn will_resize_inner(current: BufferAddress, desired: BufferAddress, minimum: BufferAddress) -> Option<BufferAddress> {
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
    use super::will_resize_inner;

    #[test]
    fn automated_buffer_resize() {
        assert_eq!(will_resize_inner(64, 128, 0), Some(256));
        assert_eq!(will_resize_inner(128, 128, 0), None);
        assert_eq!(will_resize_inner(256, 128, 0), None);

        assert_eq!(will_resize_inner(64, 64, 0), None);
        assert_eq!(will_resize_inner(128, 64, 0), None);
        assert_eq!(will_resize_inner(256, 65, 0), None);
        assert_eq!(will_resize_inner(256, 64, 0), Some(128));
        assert_eq!(will_resize_inner(256, 63, 0), Some(64));

        assert_eq!(will_resize_inner(16, 16, 0), None);
        assert_eq!(will_resize_inner(16, 8, 0), None);
        assert_eq!(will_resize_inner(16, 4, 0), Some(8));
    }
}
