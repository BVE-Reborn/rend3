use std::num::NonZeroU64;

use glam::UVec2;
use rend3::{
    graph::{DataHandle, RenderGraph},
    managers::{
        MeshBuffers, SkeletonManager, VERTEX_JOINT_INDEX_SIZE, VERTEX_JOINT_WEIGHT_SIZE, VERTEX_NORMAL_SIZE,
        VERTEX_POSITION_SIZE, VERTEX_TANGENT_SIZE,
    },
    util::{
        bind_merge::{BindGroupBuilder, BindGroupLayoutBuilder},
        math::round_up_div,
    },
};
use wgpu::{BindGroupLayout, Buffer, BufferDescriptor, BufferUsages, ComputePipeline, Device};

/// The per-skeleton data, as uploaded to the GPU compute shader.
#[repr(C, align(16))]
#[derive(Copy, Clone)]
pub struct GpuSkinningInput {
    /// See [rend3::managers::GpuVertexRanges].
    pub mesh_range: UVec2,
    /// See [rend3::managers::GpuVertexRanges].
    pub skeleton_range: UVec2,
    /// The index of this skeleton's first joint in the global joint matrix
    /// buffer.
    pub joint_idx: u32,
}

pub fn add_pre_skin_to_graph(graph: &mut RenderGraph, pre_skin_data: DataHandle<PreSkinningBuffers>) {
    let mut builder = graph.add_node("pre-skinning");
    let pre_skin_handle = builder.add_data_output(pre_skin_data);

    builder.build(move |_pt, renderer, _encoder_or_pass, _temps, _ready, graph_data| {
        let buffers = build_gpu_skinning_input_buffers(&renderer.device, graph_data.skeleton_manager);
        graph_data.set_data::<PreSkinningBuffers>(pre_skin_handle, Some(buffers));
    });
}

/// The two buffers uploaded to the GPU during pre-skinning.
pub struct PreSkinningBuffers {
    gpu_skinning_inputs: Buffer,
    joint_matrices: Buffer,
}

fn build_gpu_skinning_input_buffers(device: &Device, skeleton_manager: &SkeletonManager) -> PreSkinningBuffers {
    profiling::scope!("Building GPU Skinning Input Data");

    let skinning_inputs_length = skeleton_manager.skeletons().len() * std::mem::size_of::<GpuSkinningInput>();
    let gpu_skinning_inputs = device.create_buffer(&BufferDescriptor {
        label: Some("skinning inputs"),
        size: skinning_inputs_length as u64,
        usage: BufferUsages::UNIFORM,
        mapped_at_creation: true,
    });

    let joint_matrices = device.create_buffer(&BufferDescriptor {
        label: Some("joint matrices"),
        size: (skeleton_manager.global_joint_count() * std::mem::size_of::<[[f32; 4]; 4]>()) as u64,
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

pub struct GpuSkinner {
    pub pipeline: ComputePipeline,
    pub vertex_buffers_bgl: BindGroupLayout,
    pub joint_matrices_bgl: BindGroupLayout,
    pub skinning_inputs_bgl: BindGroupLayout,
}

impl GpuSkinner {
    const WORKGROUP_SIZE: u32 = 64;

    pub fn new(device: &wgpu::Device) -> GpuSkinner {
        let storage_buffer_ty = |read_only, size| wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: size,
        };

        let pos_size = NonZeroU64::new(VERTEX_POSITION_SIZE as u64);
        let nrm_size = NonZeroU64::new(VERTEX_NORMAL_SIZE as u64);
        let tan_size = NonZeroU64::new(VERTEX_TANGENT_SIZE as u64);
        let j_idx_size = NonZeroU64::new(VERTEX_JOINT_INDEX_SIZE as u64);
        let j_wt_size = NonZeroU64::new(VERTEX_JOINT_WEIGHT_SIZE as u64);
        let mat_size = NonZeroU64::new(std::mem::size_of::<[[f32; 4]; 4]>() as u64);

        // Bind group 0 contains some vertex buffers bound as storage buffers
        let vertex_buffers_bgl = BindGroupLayoutBuilder::new()
            .append(wgpu::ShaderStages::COMPUTE, storage_buffer_ty(false, pos_size), None) // Positions
            .append(wgpu::ShaderStages::COMPUTE, storage_buffer_ty(false, nrm_size), None) // Normals
            .append(wgpu::ShaderStages::COMPUTE, storage_buffer_ty(false, tan_size), None) // Tangents
            .append(wgpu::ShaderStages::COMPUTE, storage_buffer_ty(false, j_idx_size), None) // Joint indices
            .append(wgpu::ShaderStages::COMPUTE, storage_buffer_ty(false, j_wt_size), None) // Joint weights
            .build(device, Some("Gpu skinning vertex buffers"));

        // Bind group 1 contains the joint matrices global buffer. Each
        // skinning input struct contains an offset into this array.
        let joint_matrices_bgl = BindGroupLayoutBuilder::new()
            .append(wgpu::ShaderStages::COMPUTE, storage_buffer_ty(true, mat_size), None)
            .build(device, Some("Gpu skinning joint matrices"));

        // Bind group 2 contains the pre skinning inputs. This uses dynamic
        // offsets because there is one dispatch per input, and the offset is
        // used to indicate which is the current input to the shader.
        //
        // NOTE: This would be an ideal use case for push constants, but they are
        // not available on all platforms so we need to use this workaround.
        let skinning_inputs_bgl = BindGroupLayoutBuilder::new()
            .append(
                wgpu::ShaderStages::COMPUTE,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(std::mem::size_of::<GpuSkinningInput>() as u64),
                },
                None,
            )
            .build(device, Some("Gpu skinning inputs"));

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&vertex_buffers_bgl, &joint_matrices_bgl, &skinning_inputs_bgl],
            push_constant_ranges: &[],
        });

        let module = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
            label: Some("Gpu skinning compute shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/src/skinning.wgsl").into()),
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Gpu skinning pipeline"),
            layout: Some(&layout),
            module: &module,
            entry_point: "main",
        });

        GpuSkinner {
            vertex_buffers_bgl,
            joint_matrices_bgl,
            skinning_inputs_bgl,
            pipeline,
        }
    }

    pub fn execute_pass(
        &self,
        device: &Device,
        encoder: &mut wgpu::CommandEncoder,
        buffers: &PreSkinningBuffers,
        mesh_buffers: &MeshBuffers,
        // The number of inputs in the skinning_inputs buffer
        skeleton_manager: &SkeletonManager,
    ) {
        let vertex_buffers_bg = BindGroupBuilder::new()
            .append_buffer(&mesh_buffers.vertex_position)
            .append_buffer(&mesh_buffers.vertex_normal)
            .append_buffer(&mesh_buffers.vertex_tangent)
            .append_buffer(&mesh_buffers.vertex_joint_index)
            .append_buffer(&mesh_buffers.vertex_joint_weight)
            .build(device, Some("GPU skinning vertex buffers"), &self.vertex_buffers_bgl);

        let joint_matrices_bg = BindGroupBuilder::new().append_buffer(&buffers.joint_matrices).build(
            device,
            Some("GPU skinning joint matrices"),
            &self.joint_matrices_bgl,
        );

        let skinning_inputs_bgb = BindGroupBuilder::new()
            .append_buffer(&buffers.gpu_skinning_inputs)
            .build(device, Some("GPU skinning inputs"), &self.skinning_inputs_bgl);

        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("GPU Skinning"),
        });
        cpass.set_bind_group(0, &vertex_buffers_bg, &[]);
        cpass.set_bind_group(1, &joint_matrices_bg, &[]);

        for (i, skel) in skeleton_manager.skeletons().enumerate() {
            cpass.set_pipeline(&self.pipeline);
            let offset = (i * std::mem::size_of::<GpuSkinningInput>()) as u32;
            cpass.set_bind_group(2, &skinning_inputs_bgb, &[offset]);

            let num_verts = (skel.ranges.mesh_range[1] - skel.ranges.mesh_range[0]) as u32;
            let num_workgroups = round_up_div(num_verts, Self::WORKGROUP_SIZE);
            cpass.dispatch(num_workgroups, 1, 1);
        }
    }
}

pub struct SkinningOutput;

pub fn add_skinning_to_graph<'node>(
    graph: &mut RenderGraph<'node>,
    gpu_skinner: &'node GpuSkinner,
    pre_skin_data: DataHandle<PreSkinningBuffers>,
    skinned_data: DataHandle<SkinningOutput>,
) {
    let mut builder = graph.add_node("skinning");
    let pre_skin_handle = builder.add_data_input(pre_skin_data);
    let skinned_data_handle = builder.add_data_output(skinned_data);

    let skinner_pt = builder.passthrough_ref(gpu_skinner);

    builder.build(move |pt, renderer, encoder_or_pass, temps, _ready, graph_data| {
        let skinner = pt.get(skinner_pt);
        let encoder = encoder_or_pass.get_encoder();
        let skin_input = graph_data
            .get_data(temps, pre_skin_handle)
            .expect("Skinning requires pre-skinning to run first");

        // Avoid running the compute pass if there are no skeletons. This
        // prevents binding an empty buffer
        if graph_data.skeleton_manager.skeletons().len() > 0 {
            skinner.execute_pass(
                &renderer.device,
                encoder,
                skin_input,
                graph_data.mesh_manager.buffers(),
                graph_data.skeleton_manager,
            );
        }

        graph_data.set_data(skinned_data_handle, Some(SkinningOutput));
    });
}
