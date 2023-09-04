use std::ops::Deref;

use wgpu::CommandEncoder;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum InputOutputPartition {
    Input,
    Output,
}

#[derive(Debug)]
pub struct InputOutputBuffer {
    /// Current active buffer
    buffer: wgpu::Buffer,
    /// Amount of space reserved in the buffer for data, not including the header.
    data_capacity: u64,
    /// Size of output partition
    output_partition_size: u64,
    /// Size of input partition
    input_partition_size: u64,
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
    type Target = wgpu::Buffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl InputOutputBuffer {
    const HEADER_SIZE: u64 = 8;

    fn capacity(input_partition_size: u64, output_partition_size: u64) -> u64 {
        let max = input_partition_size.max(output_partition_size);
        let buffer = max.next_power_of_two() * 2;
        buffer
    }

    fn buffer_size(capacity: u64) -> u64 {
        let with_header = capacity + Self::HEADER_SIZE;
        with_header
    }

    pub fn new(device: &wgpu::Device, partition_elements: u64, element_size: u64, clear_on_swap: bool) -> Self {
        let partition_size = partition_elements * element_size as u64;
        let data_capacity = Self::capacity(partition_size, partition_size);
        let buffer_length = Self::buffer_size(data_capacity);

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SuballocatedPingPongBuffer"),
            size: buffer_length,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::INDEX
                | wgpu::BufferUsages::INDIRECT,
            mapped_at_creation: false,
        });

        Self {
            buffer,
            data_capacity,
            output_partition_size: partition_elements,
            input_partition_size: partition_elements,
            flipped: false,
            clear_on_swap,
            element_size,
        }
    }

    /// Returns the offset in bytes for a given element in the given partition
    pub fn element_offset(&self, partition: InputOutputPartition, element: u64) -> u64 {
        let partition_offset = match partition {
            InputOutputPartition::Input => self.input_partition_offset(),
            InputOutputPartition::Output => self.output_partition_offset(),
        };
        Self::HEADER_SIZE + partition_offset + element * self.element_size
    }

    pub fn partition_slice(&self, partition: InputOutputPartition) -> wgpu::BufferSlice {
        let partition_offset = match partition {
            InputOutputPartition::Input => self.input_partition_offset(),
            InputOutputPartition::Output => self.output_partition_offset(),
        };
        let partition_size = match partition {
            InputOutputPartition::Input => self.input_partition_size,
            InputOutputPartition::Output => self.output_partition_size,
        };
        let slice_start = Self::HEADER_SIZE + partition_offset;
        let slice_end: u64 = slice_start + partition_size;
        self.buffer.slice(slice_start..slice_end)
    }

    /// Returns the offset in bytes to get to the start of the output partition, not including the header.
    fn output_partition_offset(&self) -> u64 {
        if self.flipped {
            self.data_capacity / 2
        } else {
            0
        }
    }

    /// Returns the offset in bytes to get to the start of the input partition, not including the header.
    fn input_partition_offset(&self) -> u64 {
        if self.flipped {
            0
        } else {
            self.data_capacity / 2
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
        self.input_partition_size = self.output_partition_size;
        // The new output is of the given size.
        self.output_partition_size = new_partition_elements * self.element_size;
        // We're now flipped.
        self.flipped = !self.flipped;

        // Gather a new data capcity
        let new_data_capacity = Self::capacity(self.input_partition_size, self.output_partition_size);

        if new_data_capacity != self.data_capacity {
            // Set the capacity reserved
            self.data_capacity = new_data_capacity;
            let new_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("SuballocatedPingPongBuffer"),
                size: Self::buffer_size(new_data_capacity),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            if !self.clear_on_swap {
                // We copy the old output partition to the input partition of the new buffer.
                //
                // Note that we call output_partition_offset before we change any internal parameters,
                // as we need the old buffer offsets.
                encoder.copy_buffer_to_buffer(
                    &self.buffer,
                    old_output_partition_offset,
                    &new_buffer,
                    self.input_partition_offset(),
                    self.input_partition_size,
                );
            }
            // We now set the new buffer.
            self.buffer = new_buffer;
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
