use wgpu::CommandEncoderDescriptor;

use crate::{
    graph::InstructionEvaluationOutput,
    instruction::{Instruction, InstructionKind},
    Renderer,
};

pub fn evaluate_instructions(renderer: &Renderer) -> InstructionEvaluationOutput {
    profiling::scope!("Renderer::evaluate_instructions");

    let mut instructions = renderer.instructions.consumer.lock();

    let delayed_object_handles = renderer.resource_handle_allocators.object.reclaim_delayed_handles();

    // 16 encoders is a reasonable default
    let mut cmd_bufs = Vec::with_capacity(16);

    let mut encoder = renderer.device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("primary encoder"),
    });

    let mut data_core = renderer.data_core.lock();
    let data_core = &mut *data_core;

    {
        profiling::scope!("Instruction Processing");
        for Instruction { kind, location: _ } in instructions.drain(..) {
            match kind {
                InstructionKind::AddMesh {
                    handle,
                    internal_mesh,
                    cmd_buf: buffer,
                } => {
                    profiling::scope!("Add Mesh");
                    renderer.mesh_manager.fill(&handle, internal_mesh);
                    cmd_bufs.push(buffer);
                }
                InstructionKind::AddSkeleton { handle, skeleton } => {
                    profiling::scope!("Add Skeleton");
                    data_core
                        .profiler
                        .try_lock()
                        .unwrap()
                        .begin_scope("Add Skeleton", &mut encoder, &renderer.device);
                    data_core.skeleton_manager.add(
                        &renderer.device,
                        &mut encoder,
                        &renderer.mesh_manager,
                        &handle,
                        skeleton,
                    );
                    let _ = data_core.profiler.try_lock().unwrap().end_scope(&mut encoder);
                }
                InstructionKind::AddTexture2D {
                    handle,
                    internal_texture,
                    cmd_buf,
                } => {
                    cmd_bufs.extend(cmd_buf);
                    data_core.d2_texture_manager.fill(*handle, internal_texture);
                }
                InstructionKind::AddTexture2DFromTexture { handle, texture } => data_core
                    .d2_texture_manager
                    .fill_from_texture(&renderer.device, &mut encoder, *handle, texture),
                InstructionKind::AddTextureCube {
                    handle,
                    internal_texture,
                    cmd_buf,
                } => {
                    cmd_bufs.extend(cmd_buf);
                    data_core.d2c_texture_manager.fill(*handle, internal_texture);
                }
                InstructionKind::AddMaterial { handle, fill_invoke } => {
                    profiling::scope!("Add Material");
                    fill_invoke(
                        &mut data_core.material_manager,
                        &renderer.device,
                        renderer.profile,
                        &mut data_core.d2_texture_manager,
                        &handle,
                    );
                }
                InstructionKind::AddGraphData { add_invoke } => {
                    add_invoke(&mut data_core.graph_storage);
                }
                InstructionKind::ChangeMaterial { handle, change_invoke } => {
                    profiling::scope!("Change Material");

                    change_invoke(
                        &mut data_core.material_manager,
                        &renderer.device,
                        &mut data_core.d2_texture_manager,
                        &handle,
                    );
                }
                InstructionKind::AddObject { handle, object } => {
                    data_core.object_manager.add(
                        &renderer.device,
                        &handle,
                        object,
                        &renderer.mesh_manager,
                        &data_core.skeleton_manager,
                        &mut data_core.material_manager,
                    );
                }
                InstructionKind::SetObjectTransform { handle, transform } => {
                    data_core.object_manager.set_object_transform(handle, transform);
                }
                InstructionKind::SetSkeletonJointDeltas { handle, joint_matrices } => {
                    data_core.skeleton_manager.set_joint_matrices(handle, joint_matrices);
                }
                InstructionKind::AddDirectionalLight { handle, light } => {
                    data_core.directional_light_manager.add(&handle, light);
                }
                InstructionKind::ChangeDirectionalLight { handle, change } => {
                    data_core.directional_light_manager.update(handle, change);
                }
                InstructionKind::AddPointLight { handle, light } => {
                    data_core.point_light_manager.add(&handle, light);
                }
                InstructionKind::ChangePointLight { handle, change } => {
                    data_core.point_light_manager.update(handle, change);
                }
                InstructionKind::SetAspectRatio { ratio } => data_core.camera_manager.set_aspect_ratio(Some(ratio)),
                InstructionKind::SetCameraData { data } => {
                    data_core.camera_manager.set_data(data);
                }
                InstructionKind::DuplicateObject {
                    src_handle,
                    dst_handle,
                    change,
                } => {
                    data_core.object_manager.duplicate_object(
                        &renderer.device,
                        src_handle,
                        dst_handle,
                        change,
                        &renderer.mesh_manager,
                        &data_core.skeleton_manager,
                        &mut data_core.material_manager,
                    );
                }
                InstructionKind::DeleteMesh { handle } => {
                    renderer.resource_handle_allocators.mesh.deallocate(handle);
                    renderer.mesh_manager.remove(handle)
                }
                InstructionKind::DeleteSkeleton { handle } => {
                    renderer.resource_handle_allocators.skeleton.deallocate(handle);
                    data_core.skeleton_manager.remove(&renderer.mesh_manager, handle)
                }
                InstructionKind::DeleteTexture2D { handle } => {
                    renderer.resource_handle_allocators.d2_texture.deallocate(handle);
                    data_core.d2_texture_manager.remove(handle)
                }
                InstructionKind::DeleteTextureCube { handle } => {
                    renderer.resource_handle_allocators.d2c_texture.deallocate(handle);
                    data_core.d2c_texture_manager.remove(handle)
                }
                InstructionKind::DeleteMaterial { handle } => {
                    renderer.resource_handle_allocators.material.deallocate(handle);
                    data_core.material_manager.remove(handle)
                }
                InstructionKind::DeleteObject { handle } => {
                    renderer.resource_handle_allocators.object.deallocate(handle);
                    data_core.object_manager.remove(handle)
                }
                InstructionKind::DeleteDirectionalLight { handle } => {
                    renderer.resource_handle_allocators.directional_light.deallocate(handle);
                    data_core.directional_light_manager.remove(handle)
                }
                InstructionKind::DeletePointLight { handle } => {
                    renderer.resource_handle_allocators.point_light.deallocate(handle);
                    data_core.point_light_manager.remove(handle);
                }
                InstructionKind::DeleteGraphData { handle } => {
                    renderer.resource_handle_allocators.graph_storage.deallocate(handle);
                    data_core.graph_storage.remove(&handle);
                }
            }
        }
    }

    // Do these in dependency order
    // Level 3
    data_core.object_manager.evaluate(
        &renderer.device,
        &mut encoder,
        &renderer.scatter,
        &delayed_object_handles,
    );

    // Level 2
    let d2_texture = data_core.d2_texture_manager.evaluate(&renderer.device);

    // Level 1
    // The material manager needs to be able to pull correct internal indices from
    // the d2 texture manager, so it has to go first.
    data_core.material_manager.evaluate(
        &renderer.device,
        &mut encoder,
        &renderer.scatter,
        renderer.profile,
        &data_core.d2_texture_manager,
    );

    // Level 0
    let d2c_texture = data_core.d2c_texture_manager.evaluate(&renderer.device);
    let (shadow_target_size, shadows) = data_core
        .directional_light_manager
        .evaluate(renderer, &data_core.camera_manager);
    data_core.point_light_manager.evaluate(renderer);
    let mesh_buffer = renderer.mesh_manager.evaluate();

    cmd_bufs.push(encoder.finish());

    InstructionEvaluationOutput {
        cmd_bufs,
        d2_texture,
        d2c_texture,
        shadow_target_size,
        shadows,
        mesh_buffer,
    }
}
