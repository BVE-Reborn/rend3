use std::any::TypeId;

use encase::{private::WriteInto, ShaderSize};
use wgpu::{Buffer, BufferDescriptor, BufferUsages, CommandEncoder, Device};

use crate::util::scatter_copy::{ScatterCopy, ScatterData};

const STARTING_SIZE: usize = 16;
const NEEDED_USAGES: BufferUsages = BufferUsages::STORAGE
    .union(BufferUsages::COPY_DST)
    .union(BufferUsages::COPY_SRC);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FreelistBufferIndex(pub usize);

pub struct FreelistBuffer {
    inner: Buffer,

    current_count: usize,
    reserved_count: usize,
    rounded_size: u64,
    stored_type: TypeId,
    freelist: Vec<usize>,

    stale: Vec<usize>,
}
impl FreelistBuffer {
    pub fn new<T>(device: &Device) -> Self
    where
        T: ShaderSize + WriteInto + 'static,
    {
        let rounded_size = T::METADATA.alignment().round_up(T::SHADER_SIZE.get());

        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("freelist buffer"),
            size: rounded_size * STARTING_SIZE as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            inner: buffer,

            current_count: STARTING_SIZE,
            reserved_count: STARTING_SIZE,
            rounded_size,
            stored_type: TypeId::of::<T>(),
            freelist: Vec::new(),

            stale: Vec::new(),
        }
    }

    pub fn add(&mut self) -> FreelistBufferIndex {
        if let Some(idx) = self.freelist.pop() {
            self.stale.push(idx);
            return FreelistBufferIndex(idx);
        }

        let old_count = self.reserved_count;
        self.reserved_count = old_count.next_power_of_two();

        self.freelist.extend(old_count..self.reserved_count);

        let idx = self.freelist.pop().unwrap();
        self.stale.push(idx);

        FreelistBufferIndex(idx)
    }

    pub fn update(&mut self, index: FreelistBufferIndex) {
        self.stale.push(index.0);
    }

    pub fn remove(&mut self, index: FreelistBufferIndex) {
        self.freelist.push(index.0);
    }

    pub fn apply<T>(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        scatter: &ScatterCopy,
        mut get_value: impl FnMut(FreelistBufferIndex) -> T,
    ) where
        T: ShaderSize + WriteInto + 'static,
    {
        assert_eq!(self.stored_type, TypeId::of::<T>());

        if self.current_count != self.reserved_count {
            let new_buffer = device.create_buffer(&BufferDescriptor {
                label: Some("freelist buffer"),
                size: self.rounded_size * self.reserved_count as u64,
                usage: BufferUsages::STORAGE,
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

        let data = self.stale.drain(..).map(|idx| {
            let data = get_value(FreelistBufferIndex(idx));
            ScatterData {
                word_offset: (idx as u64 * self.rounded_size).try_into().unwrap(),
                data,
            }
        });

        scatter.execute_copy(device, encoder, &self.inner, data);
    }
}
