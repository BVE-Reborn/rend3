use crate::{
    graph::ReadyData,
    instruction::{Instruction, InstructionKind},
    Renderer,
};
use wgpu::{CommandBuffer, CommandEncoderDescriptor};

pub fn ready(renderer: &Renderer) -> (Vec<CommandBuffer>, ReadyData) {
    profiling::scope!("Renderer::ready");

    renderer.instructions.swap();

    let mut instructions = renderer.instructions.consumer.lock();

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
                InstructionKind::AddMesh { handle, mesh } => {
                    profiling::scope!("Add Mesh");
                    data_core
                        .profiler
                        .begin_scope("Add Mesh", &mut encoder, &renderer.device);
                    data_core
                        .mesh_manager
                        .add(&renderer.device, &renderer.queue, &mut encoder, &handle, mesh);
                    data_core.profiler.end_scope(&mut encoder);
                }
                InstructionKind::AddSkeleton { handle, skeleton } => {
                    profiling::scope!("Add Skeleton");
                    data_core
                        .profiler
                        .begin_scope("Add Skeleton", &mut encoder, &renderer.device);
                    data_core.skeleton_manager.add(
                        &renderer.device,
                        &mut encoder,
                        &mut data_core.mesh_manager,
                        &handle,
                        skeleton,
                    );
                    data_core.profiler.end_scope(&mut encoder);
                }
                InstructionKind::AddTexture2D {
                    handle,
                    desc,
                    texture,
                    view,
                    buffer,
                } => {
                    cmd_bufs.extend(buffer);
                    data_core.d2_texture_manager.add(*handle, desc, texture, view);
                }
                InstructionKind::AddTextureCube {
                    handle,
                    desc,
                    texture,
                    view,
                    buffer,
                } => {
                    cmd_bufs.extend(buffer);
                    data_core.d2c_texture_manager.add(*handle, desc, texture, view);
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
                InstructionKind::ChangeMaterial { handle, change_invoke } => {
                    profiling::scope!("Change Material");

                    change_invoke(
                        &mut data_core.material_manager,
                        &renderer.device,
                        &mut data_core.d2_texture_manager,
                        &handle,
                    )
                }
                InstructionKind::AddObject { handle, object } => {
                    data_core.object_manager.add(
                        &renderer.device,
                        &handle,
                        object,
                        &mut data_core.mesh_manager,
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
                        &mut data_core.mesh_manager,
                        &data_core.skeleton_manager,
                        &mut data_core.material_manager,
                    );
                }
                InstructionKind::DeleteMesh { handle } => {
                    renderer.resource_handle_allocators.mesh.deallocate(handle);
                    data_core.mesh_manager.remove(handle)
                }
                InstructionKind::DeleteSkeleton { handle } => {
                    renderer.resource_handle_allocators.skeleton.deallocate(handle);
                    data_core.skeleton_manager.remove(&mut data_core.mesh_manager, handle)
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
            }
        }
    }

    // Do these in dependency order
    // Level 3
    data_core
        .object_manager
        .ready(&renderer.device, &mut encoder, &renderer.scatter);

    // Level 2
    let d2_texture = data_core.d2_texture_manager.ready(&renderer.device);

    // Level 1
    // The material manager needs to be able to pull correct internal indices from
    // the d2 texture manager, so it has to go first.
    data_core.material_manager.ready(
        &renderer.device,
        &mut encoder,
        &renderer.scatter,
        renderer.profile,
        &data_core.d2_texture_manager,
    );

    // Level 0
    let d2c_texture = data_core.d2c_texture_manager.ready(&renderer.device);
    let (shadow_target_size, shadows) = data_core
        .directional_light_manager
        .ready(renderer, &data_core.camera_manager);

    cmd_bufs.push(encoder.finish());

    (
        cmd_bufs,
        ReadyData {
            d2_texture,
            d2c_texture,
            shadow_target_size,
            shadows,
        },
    )
}
