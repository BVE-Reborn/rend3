use std::{borrow::Cow, mem};

use encase::{ShaderSize, ShaderType};
use glam::Mat4;
use rend3::{
    graph::{NodeExecutionContext, RenderGraph},
    types::{
        VERTEX_ATTRIBUTE_JOINT_INDICES, VERTEX_ATTRIBUTE_JOINT_WEIGHTS, VERTEX_ATTRIBUTE_NORMAL,
        VERTEX_ATTRIBUTE_POSITION, VERTEX_ATTRIBUTE_TANGENT,
    },
    util::{
        bind_merge::{BindGroupBuilder, BindGroupLayoutBuilder},
        math::div_round_up,
    },
    ShaderPreProcessor,
};
use wgpu::{
    BindGroupLayout, Buffer, BufferBindingType, BufferDescriptor, BufferUsages, CommandEncoder, ComputePassDescriptor,
    ComputePipeline, ComputePipelineDescriptor, PipelineCompilationOptions, PipelineLayoutDescriptor,
    ShaderModuleDescriptor, ShaderStages,
};

/// The per-skeleton data, as uploaded to the GPU compute shader.
#[derive(Copy, Clone, ShaderType)]
pub struct GpuSkinningInput {
    /// Byte offset into vertex buffer of position attribute of unskinned mesh.
    base_position_offset: u32,
    /// Byte offset into vertex buffer of normal attribute of unskinned mesh.
    base_normal_offset: u32,
    /// Byte offset into vertex buffer of tangent attribute of unskinned mesh.
    base_tangent_offset: u32,
    /// Byte offset into vertex buffer of joint indices of mesh.
    joint_indices_offset: u32,
    /// Byte offset into vertex buffer of joint weights of mesh.
    joint_weight_offset: u32,
    /// Byte offset into vertex buffer of position attribute of skinned mesh.
    updated_position_offset: u32,
    /// Byte offset into vertex buffer of normal attribute of skinned mesh.
    updated_normal_offset: u32,
    /// Byte offset into vertex buffer of tangent attribute of skinned mesh.
    updated_tangent_offset: u32,

    /// Index into the matrix buffer that joint_indices is relative to.
    joint_matrix_base_offset: u32,
    /// Count of vertices in this mesh.
    vertex_count: u32,
}

/// The two buffers uploaded to the GPU during pre-skinning.
pub struct PreSkinningBuffers {
    gpu_skinning_inputs: Buffer,
    joint_matrices: Buffer,
}

fn build_gpu_skinning_input_buffers(ctx: &NodeExecutionContext) -> PreSkinningBuffers {
    profiling::scope!("Building GPU Skinning Input Data");

    let skinning_inputs_size =
        ctx.data_core.skeleton_manager.skeletons().len() as u64 * GpuSkinningInput::SHADER_SIZE.get();
    let gpu_skinning_inputs = ctx.renderer.device.create_buffer(&BufferDescriptor {
        label: Some("skinning inputs"),
        size: skinning_inputs_size,
        usage: BufferUsages::STORAGE,
        mapped_at_creation: true,
    });

    let joint_matrices = ctx.renderer.device.create_buffer(&BufferDescriptor {
        label: Some("joint matrices"),
        size: (ctx.data_core.skeleton_manager.global_joint_count() * mem::size_of::<Mat4>()) as u64,
        usage: BufferUsages::STORAGE,
        mapped_at_creation: true,
    });

    let mut skinning_input_range = gpu_skinning_inputs.slice(..).get_mapped_range_mut();
    let mut skinning_input_data = encase::DynamicStorageBuffer::new(&mut *skinning_input_range);
    let mut joint_matrices_data = joint_matrices.slice(..).get_mapped_range_mut();

    // Skeletons have a variable number of joints, so we need to keep track of
    // the global index here.
    let mut joint_matrix_idx = 0;

    // Iterate over the skeletons, fill the buffers
    for skeleton in ctx.data_core.skeleton_manager.skeletons() {
        // SAFETY: We are always accessing elements in bounds and all accesses are
        // aligned
        unsafe {
            let mut input = GpuSkinningInput {
                base_position_offset: u32::MAX,
                base_normal_offset: u32::MAX,
                base_tangent_offset: u32::MAX,
                joint_indices_offset: u32::MAX,
                joint_weight_offset: u32::MAX,
                updated_position_offset: u32::MAX,
                updated_normal_offset: u32::MAX,
                updated_tangent_offset: u32::MAX,
                joint_matrix_base_offset: joint_matrix_idx,
                vertex_count: skeleton.vertex_count,
            };

            for (attribute, range) in &skeleton.source_attribute_ranges {
                match attribute {
                    a if *a == *VERTEX_ATTRIBUTE_POSITION => input.base_position_offset = range.start as u32,
                    a if *a == *VERTEX_ATTRIBUTE_NORMAL => input.base_normal_offset = range.start as u32,
                    a if *a == *VERTEX_ATTRIBUTE_TANGENT => input.base_tangent_offset = range.start as u32,
                    a if *a == *VERTEX_ATTRIBUTE_JOINT_INDICES => input.joint_indices_offset = range.start as u32,
                    a if *a == *VERTEX_ATTRIBUTE_JOINT_WEIGHTS => input.joint_weight_offset = range.start as u32,
                    a => unreachable!("Unknown skinning input attribute {a:?}"),
                }
            }

            for (attribute, range) in &skeleton.overridden_attribute_ranges {
                match attribute {
                    a if *a == *VERTEX_ATTRIBUTE_POSITION => input.updated_position_offset = range.start as u32,
                    a if *a == *VERTEX_ATTRIBUTE_NORMAL => input.updated_normal_offset = range.start as u32,
                    a if *a == *VERTEX_ATTRIBUTE_TANGENT => input.updated_tangent_offset = range.start as u32,
                    a => unreachable!("Unknown skinning output attribute {a:?}"),
                }
            }

            skinning_input_data.write(&input).unwrap();

            let joint_matrices_ptr = joint_matrices_data.as_mut_ptr() as *mut [[f32; 4]; 4];
            for joint_matrix in &skeleton.joint_matrices {
                // Here, the access can't be OOB either: The joint_matrix_idx
                // will get incremented once for every joint matrix, and the
                // length of the buffer is exactly the sum of all joint matrix
                // vector lengths.
                joint_matrices_ptr.add(joint_matrix_idx as usize).write_unaligned(joint_matrix.to_cols_array_2d());
                joint_matrix_idx += 1;
            }
        }
    }

    drop(skinning_input_range);
    drop(joint_matrices_data);
    gpu_skinning_inputs.unmap();
    joint_matrices.unmap();

    PreSkinningBuffers { gpu_skinning_inputs, joint_matrices }
}

/// Holds the necessary wgpu data structures for the GPU skinning compute pass
pub struct GpuSkinner {
    pub pipeline: ComputePipeline,
    pub bgl: BindGroupLayout,
}

impl GpuSkinner {
    const WORKGROUP_SIZE: u32 = 256;

    pub fn new(device: &wgpu::Device, spp: &ShaderPreProcessor) -> GpuSkinner {
        // Bind group 0 contains some vertex buffers bound as storage buffers
        let bgl = BindGroupLayoutBuilder::new()
            .append_buffer(ShaderStages::COMPUTE, BufferBindingType::Storage { read_only: false }, false, 4) // Vertices
            .append_buffer(ShaderStages::COMPUTE, BufferBindingType::Storage { read_only: true }, true, GpuSkinningInput::SHADER_SIZE.get()) // Inputs
            .append_buffer(ShaderStages::COMPUTE, BufferBindingType::Storage { read_only: true }, false, Mat4::SHADER_SIZE.get()) // Matrices
            .build(device, Some("Gpu skinning mesh data"));

        let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Gpu skinning compute shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(
                spp.render_shader("rend3-routine/skinning.wgsl", &(), None).unwrap(),
            )),
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("Gpu skinning pipeline"),
            layout: Some(&layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: PipelineCompilationOptions::default(), // use default WGPU options. New in WGPU 0.20 (JN)
            cache: None,    // (JN) no cache used
        });

        Self { bgl, pipeline }
    }

    pub fn execute_pass(&self, ctx: &NodeExecutionContext, encoder: &mut CommandEncoder, buffers: &PreSkinningBuffers) {
        let bg = BindGroupBuilder::new()
            .append_buffer(&ctx.eval_output.mesh_buffer)
            .append_buffer_with_size(&buffers.gpu_skinning_inputs, GpuSkinningInput::SHADER_SIZE.get())
            .append_buffer(&buffers.joint_matrices)
            .build(&ctx.renderer.device, Some("GPU skinning inputs"), &self.bgl);

        let mut cpass =
            encoder.begin_compute_pass(&ComputePassDescriptor { label: Some("GPU Skinning"), timestamp_writes: None });
        cpass.set_pipeline(&self.pipeline);
        for (i, skel) in ctx.data_core.skeleton_manager.skeletons().enumerate() {
            let offset = (i as u64 * GpuSkinningInput::SHADER_SIZE.get()) as u32;
            cpass.set_bind_group(0, &bg, &[offset]);

            let num_workgroups = div_round_up(skel.vertex_count, Self::WORKGROUP_SIZE);
            cpass.dispatch_workgroups(num_workgroups, 1, 1);
        }
    }
}

/// The GPU skinning node works by producing a side effect: Mutating the
/// skeleton copies of the vertex buffer in-place. All this happens on GPU
/// memory, so there is no data to be returned on the CPU side. This type
/// represents the (virtual) output of GPU skinning.
///
/// This is used to ensure skinning will be called at the right time in the
/// render graph (before any culling happens).
pub struct SkinningOutput;

/// Performs skinning on the GPU.
pub fn add_skinning_to_graph<'node>(graph: &mut RenderGraph<'node>, gpu_skinner: &'node GpuSkinner) {
    let mut builder = graph.add_node("skinning");
    builder.add_side_effect();

    builder.build(move |mut ctx| {
        let encoder = ctx.encoder_or_pass.take_encoder();
        // Avoid running the compute pass if there are no skeletons. This
        // prevents binding an empty buffer
        if ctx.data_core.skeleton_manager.skeletons().len() > 0 {
            let skinning_input = build_gpu_skinning_input_buffers(&ctx); // only do this if we have skeletons. WGPU does not like zero length buffers.
            gpu_skinner.execute_pass(&ctx, &mut encoder.borrow_mut(), &skinning_input);
        }
    });
}
