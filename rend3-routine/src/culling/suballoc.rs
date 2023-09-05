use std::{
    ops::{Deref, Range},
    sync::Arc,
};

use rend3::util::typedefs::SsoString;
use wgpu::CommandEncoder;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum InputOutputPartition {
    Input,
    Output,
}

#[derive(Debug)]
pub struct InputOutputBuffer {
    /// Label for the buffer
    label: SsoString,
    /// Current active buffer
    buffer: Arc<wgpu::Buffer>,
    /// Amount of elements reserved in the buffer for data, not including the header.
    capacity_elements: u64,
    /// Size of output partition
    output_partition_elements: u64,
    /// Size of input partition
    input_partition_elements: u64,
    /// When false, output partition is comes first.
    ///
    /// When true, input partition comes first.
    flipped: bool,
    /// Clear on swap
    ///
    /// When true, the data in both partitions will be cleared when the buffer
    /// is swapped.
    clear_on_swap: bool,
    /// The size of each element in the buffer. This allows the user to provide sizes in element counts only.
    element_size: u64,
}

impl Deref for InputOutputBuffer {
    type Target = Arc<wgpu::Buffer>;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl InputOutputBuffer {
    const HEADER_SIZE: u64 = 8;
    const USAGES: wgpu::BufferUsages = wgpu::BufferUsages::STORAGE
        .union(wgpu::BufferUsages::COPY_DST)
        .union(wgpu::BufferUsages::COPY_SRC)
        .union(wgpu::BufferUsages::INDEX)
        .union(wgpu::BufferUsages::INDIRECT);

    fn capacity_elements(input_partition_elements: u64, output_partition_elements: u64) -> u64 {
        let max = input_partition_elements.max(output_partition_elements);
        let buffer = max.next_power_of_two() * 2;
        buffer
    }

    fn buffer_size(capacity_elements: u64, element_size: u64) -> u64 {
        let with_header = capacity_elements * element_size + Self::HEADER_SIZE;
        with_header
    }

    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        partition_elements: u64,
        label: &str,
        element_size: u64,
        clear_on_swap: bool,
    ) -> Self {
        let capacity_elements = Self::capacity_elements(partition_elements, partition_elements);
        let buffer_length = Self::buffer_size(capacity_elements, element_size as u64);

        let buffer = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: buffer_length,
            usage: Self::USAGES,
            mapped_at_creation: false,
        }));

        let this = Self {
            label: SsoString::from(label),
            buffer,
            capacity_elements,
            output_partition_elements: partition_elements,
            input_partition_elements: partition_elements,
            flipped: false,
            clear_on_swap,
            element_size,
        };

        this.write_headers(&queue);

        this
    }

    /// Returns the offset in bytes for a given element in the given partition
    pub fn element_offset(&self, partition: InputOutputPartition, element: u64) -> u64 {
        let partition_offset = match partition {
            InputOutputPartition::Input => self.input_partition_offset(),
            InputOutputPartition::Output => self.output_partition_offset(),
        };
        Self::HEADER_SIZE + partition_offset + element * self.element_size
    }

    pub fn partition_slice(&self, partition: InputOutputPartition) -> Range<u64> {
        let partition_offset = match partition {
            InputOutputPartition::Input => self.input_partition_offset(),
            InputOutputPartition::Output => self.output_partition_offset(),
        };
        let partition_elements = match partition {
            InputOutputPartition::Input => self.input_partition_elements,
            InputOutputPartition::Output => self.output_partition_elements,
        };
        let partition_size = partition_elements * self.element_size;
        let slice_start = Self::HEADER_SIZE + partition_offset;
        let slice_end: u64 = slice_start + partition_size;
        slice_start..slice_end
    }

    /// Returns the offset in bytes to get to the start of the output partition, not including the header.
    fn output_partition_offset(&self) -> u64 {
        if self.flipped {
            (self.capacity_elements * self.element_size) / 2
        } else {
            0
        }
    }

    /// Returns the offset in bytes to get to the start of the input partition, not including the header.
    fn input_partition_offset(&self) -> u64 {
        if self.flipped {
            0
        } else {
            (self.capacity_elements * self.element_size) / 2
        }
    }

    pub fn swap(
        &mut self,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
        encoder: &mut CommandEncoder,
        new_partition_elements: u64,
    ) {
        // Offset of the output partition in the old buffer.
        let old_output_partition_offset = self.output_partition_offset();

        // The output of last frame is now the input of this frame.
        self.input_partition_elements = self.output_partition_elements;
        // The new output is of the given size.
        self.output_partition_elements = new_partition_elements;
        // We're now flipped.
        self.flipped = !self.flipped;

        // Gather a new data capcity
        let new_capacity_elements =
            Self::capacity_elements(self.input_partition_elements, self.output_partition_elements);

        if new_capacity_elements != self.capacity_elements {
            // Set the capacity reserved
            self.capacity_elements = new_capacity_elements;
            let new_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&self.label),
                size: Self::buffer_size(new_capacity_elements, self.element_size),
                usage: Self::USAGES,
                mapped_at_creation: false,
            });
            if !self.clear_on_swap {
                // We copy the old output partition to the input partition of the new buffer.
                //
                // Note that we call output_partition_offset before we change any internal parameters,
                // as we need the old buffer offsets.
                encoder.copy_buffer_to_buffer(
                    &self.buffer,
                    old_output_partition_offset + Self::HEADER_SIZE,
                    &new_buffer,
                    self.input_partition_offset() + Self::HEADER_SIZE,
                    self.input_partition_elements * self.element_size,
                );
            }
            // We now set the new buffer.
            self.buffer = Arc::new(new_buffer);
        } else if self.clear_on_swap {
            encoder.clear_buffer(&self.buffer, Self::HEADER_SIZE, None);
        }

        self.write_headers(queue)
    }

    fn write_headers(&self, queue: &wgpu::Queue) {
        let offsets = [
            (self.output_partition_offset() / self.element_size) as u32,
            (self.input_partition_offset() / self.element_size) as u32,
        ];
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&offsets));
    }
}
