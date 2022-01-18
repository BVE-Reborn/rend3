use rend3::{format_sso, managers::SkeletonManager, DataHandle, RenderGraph};
use wgpu::{Buffer, BufferDescriptor, BufferUsages, Device};

/// The per-skeleton data, as uploaded to the GPU compute shader.
#[repr(C, align(16))]
#[derive(Copy, Clone)]
pub struct GpuSkinningInput {
    /// See [GpuVertexRanges::mesh_range].
    pub mesh_range: [u32; 2],
    /// See [GpuVertexRanges::skeleton_range].
    pub skeleton_range: [u32; 2],
    /// The index of this skeleton's first joint in the global joint matrix
    /// buffer.
    pub joint_idx: u32,
}

pub fn add_pre_skin_to_graph<'node>(
    graph: &mut RenderGraph<'node>,
    name: &str,
    pre_skin_data: DataHandle<PreSkinningBuffers>,
) {
    let mut builder = graph.add_node(format_sso!("pre-skin {:?}", name));
    let pre_skin_handle = builder.add_data_output(pre_skin_data);

    builder.build(move |_pt, renderer, _encoder_or_pass, _temps, _ready, graph_data| {
        let buffers = build_gpu_skinning_input_buffers(&renderer.device, &graph_data.skeleton_manager);
        graph_data.set_data::<PreSkinningBuffers>(pre_skin_handle, Some(buffers));
    });
}

/// The two buffers uploaded to the GPU during pre-skinning.
pub struct PreSkinningBuffers {
    gpu_skinning_inputs: Buffer,
    joint_matrices: Buffer,
}

fn build_gpu_skinning_input_buffers<'node>(device: &Device, skeleton_manager: &SkeletonManager) -> PreSkinningBuffers {
    profiling::scope!("Building GPU Skinning Input Data");

    let skinning_inputs_length = skeleton_manager.skeletons().len() * std::mem::size_of::<GpuSkinningInput>();
    let gpu_skinning_inputs = device.create_buffer(&BufferDescriptor {
        label: Some("skinning inputs"),
        size: skinning_inputs_length as u64,
        usage: BufferUsages::STORAGE,
        mapped_at_creation: true,
    });

    let joint_matrices_length: usize = skeleton_manager
        .skeletons()
        .map(|sk| sk.joint_matrices.len())
        .sum::<usize>()
        * std::mem::size_of::<[[f32; 4]; 4]>();
    let joint_matrices = device.create_buffer(&BufferDescriptor {
        label: Some("joint matrices"),
        size: joint_matrices_length as u64,
        usage: BufferUsages::STORAGE,
        mapped_at_creation: true,
    });

    let mut skinning_input_data = gpu_skinning_inputs.slice(..).get_mapped_range_mut();
    let mut joint_matrices_data = joint_matrices.slice(..).get_mapped_range_mut();

    // Skeletons have a variable number of joints, so we need to keep track of
    // the global index here.
    let mut joint_matrix_idx = 0;

    // Iterate over the skeletons, fill the buffers
    for (idx, skeleton) in skeleton_manager.skeletons().enumerate() {
        // SAFETY: We are always accessing elements in bounds and all accesses are
        // aligned
        unsafe {
            let input = GpuSkinningInput {
                skeleton_range: skeleton.ranges.skeleton_range,
                mesh_range: skeleton.ranges.mesh_range,
                joint_idx: joint_matrix_idx,
            };

            // The skinning inputs buffer has as many elements as skeletons, so
            // using the same index as the current skeleton will never access OOB
            let skin_input_ptr = skinning_input_data.as_mut_ptr() as *mut GpuSkinningInput;
            skin_input_ptr.add(idx).write_unaligned(input);

            let joint_matrices_ptr = joint_matrices_data.as_mut_ptr() as *mut [[f32; 4]; 4];
            for joint_matrix in &skeleton.joint_matrices {
                // Here, the access can't be OOB either: The joint_matrix_idx
                // will get incremented once for every joint matrix, and the
                // length of the buffer is exactly the sum of all joint matrix
                // vector lengths.
                joint_matrices_ptr
                    .add(joint_matrix_idx as usize)
                    .write_unaligned(joint_matrix.to_cols_array_2d());
                joint_matrix_idx += 1;
            }
        }
    }

    drop(skinning_input_data);
    drop(joint_matrices_data);
    gpu_skinning_inputs.unmap();
    joint_matrices.unmap();

    PreSkinningBuffers {
        gpu_skinning_inputs,
        joint_matrices,
    }
}
