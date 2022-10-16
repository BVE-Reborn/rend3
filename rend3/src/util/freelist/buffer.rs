use std::{any::TypeId, ops::Deref};

use encase::{private::WriteInto, ShaderSize};
use wgpu::{Buffer, BufferDescriptor, BufferUsages, CommandEncoder, Device};

use crate::util::scatter_copy::{ScatterCopy, ScatterData};

const STARTING_SIZE: usize = 16;
const NEEDED_USAGES: BufferUsages = BufferUsages::STORAGE
    .union(BufferUsages::COPY_DST)
    .union(BufferUsages::COPY_SRC);

pub struct FreelistDerivedBuffer {
    inner: Buffer,

    current_count: usize,
    reserved_count: usize,
    rounded_size: u64,
    stored_type: TypeId,

    stale: Vec<usize>,
}
impl FreelistDerivedBuffer {
    pub fn new<T>(device: &Device) -> Self
    where
        T: ShaderSize + WriteInto + 'static,
    {
        let rounded_size = T::METADATA.alignment().round_up(T::SHADER_SIZE.get());

        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("freelist buffer"),
            size: rounded_size * STARTING_SIZE as u64,
            usage: NEEDED_USAGES,
            mapped_at_creation: false,
        });

        Self {
            inner: buffer,

            current_count: STARTING_SIZE,
            reserved_count: STARTING_SIZE,
            rounded_size,
            stored_type: TypeId::of::<T>(),

            stale: Vec::new(),
        }
    }

    pub fn use_index(&mut self, index: usize) {
        if index > self.reserved_count {
            self.reserved_count = index.saturating_sub(1).next_power_of_two();
        }

        self.stale.push(index);
    }

    pub fn apply<T, F>(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        scatter: &ScatterCopy,
        mut get_value: F,
    ) where
        T: ShaderSize + WriteInto + 'static,
        F: FnMut(usize) -> T,
    {
        assert_eq!(self.stored_type, TypeId::of::<T>());

        if self.current_count != self.reserved_count {
            let new_buffer = device.create_buffer(&BufferDescriptor {
                label: Some("freelist buffer"),
                size: self.rounded_size * self.reserved_count as u64,
                usage: NEEDED_USAGES,
                mapped_at_creation: false,
            });

            encoder.copy_buffer_to_buffer(
                &self.inner,
                0,
                &new_buffer,
                0,
                self.current_count as u64 * self.rounded_size,
            );

            self.inner = new_buffer;
            self.current_count = self.reserved_count;
        }
        
        if self.stale.is_empty() {
            return;
        }

        let data = self.stale.drain(..).map(|idx| {
            let data = get_value(idx);
            ScatterData {
                word_offset: (idx as u64 * self.rounded_size).try_into().unwrap(),
                data,
            }
        });

        scatter.execute_copy(device, encoder, &self.inner, data);
    }
}

impl Deref for FreelistDerivedBuffer {
    type Target = wgpu::Buffer;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
