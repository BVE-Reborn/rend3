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
                    data_core.mesh_manager.fill(
                        &renderer.device,
                        &renderer.queue,
                        &mut encoder,
                        &mut data_core.object_manager,
                        &mut data_core.skeleton_manager,
                        &handle,
                        mesh,
                    );
                    data_core.profiler.end_scope(&mut encoder);
                }
                InstructionKind::AddSkeleton { handle, skeleton } => {
                    profiling::scope!("Add Skeleton");
                    data_core
                        .profiler
                        .begin_scope("Add Skeleton", &mut encoder, &renderer.device);
                    data_core.skeleton_manager.fill(
                        &renderer.device,
                        &mut encoder,
                        &mut data_core.mesh_manager,
                        &mut data_core.object_manager,
                        &handle,
                        skeleton,
                    );
                    data_core.profiler.end_scope(&mut encoder);
                }
                InstructionKind::AddTexture {
                    handle,
                    desc,
                    texture,
                    view,
                    buffer,
                    cube,
                } => {
                    cmd_bufs.extend(buffer);
                    if cube {
                        data_core.d2c_texture_manager.fill(&handle, desc, texture, view);
                    } else {
                        data_core.d2_texture_manager.fill(&handle, desc, texture, view);
                    }
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
                        renderer.profile,
                        &mut data_core.d2_texture_manager,
                        &mut data_core.object_manager,
                        &handle,
                    )
                }
                InstructionKind::AddObject { handle, object } => {
                    data_core.object_manager.fill(
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
                    data_core.directional_light_manager.fill(&handle, light);
                }
                InstructionKind::ChangeDirectionalLight { handle, change } => {
                    data_core
                        .directional_light_manager
                        .update_directional_light(handle, change);
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
                        src_handle,
                        dst_handle,
                        change,
                        &mut data_core.mesh_manager,
                        &data_core.skeleton_manager,
                        &mut data_core.material_manager,
                    );
                }
            }
        }
    }

    // Do these in dependency order
    // Level 3
    data_core.object_manager.ready(&mut data_core.material_manager);

    // Level 2
    let d2_texture = data_core.d2_texture_manager.ready(&renderer.device);

    // Level 1
    // The material manager needs to be able to pull correct internal indices from
    // the d2 texture manager, so it has to go first.
    data_core.material_manager.ready(
        &renderer.device,
        &renderer.queue,
        &mut data_core.object_manager,
        &data_core.d2_texture_manager,
    );

    // Level 0
    let d2c_texture = data_core.d2c_texture_manager.ready(&renderer.device);
    let directional_light_cameras =
        data_core
            .directional_light_manager
            .ready(&renderer.device, &renderer.queue, &data_core.camera_manager);
    data_core.mesh_manager.ready();
    data_core.skeleton_manager.ready(&mut data_core.mesh_manager);

    cmd_bufs.push(encoder.finish());

    (
        cmd_bufs,
        ReadyData {
            d2_texture,
            d2c_texture,
            directional_light_cameras,
        },
    )
}
