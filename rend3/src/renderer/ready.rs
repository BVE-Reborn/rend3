use crate::{instruction::Instruction, ReadyData, Renderer};
use wgpu::{CommandBuffer, CommandEncoderDescriptor};

pub fn ready(renderer: &Renderer) -> (Vec<CommandBuffer>, ReadyData) {
    profiling::scope!("render_loop");

    renderer.instructions.swap();

    let mut instructions = renderer.instructions.consumer.lock();

    // 16 encoders is a reasonable default
    let mut cmd_bufs = Vec::with_capacity(16);

    let mut encoder = renderer.device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("primary encoder"),
    });

    let mut mesh_manager = renderer.mesh_manager.write();
    let mut texture_manager_2d = renderer.d2_texture_manager.write();
    let mut texture_manager_cube = renderer.d2c_texture_manager.write();
    let mut material_manager = renderer.material_manager.write();
    let mut object_manager = renderer.object_manager.write();
    let mut directional_light_manager = renderer.directional_light_manager.write();
    let mut camera_manager = renderer.camera_manager.write();
    let mut profiler = renderer.profiler.lock();

    {
        profiling::scope!("Instruction Processing");
        for cmd in instructions.drain(..) {
            match cmd {
                Instruction::AddMesh { handle, mesh } => {
                    profiling::scope!("Add Mesh");
                    profiler.begin_scope("Add Mesh", &mut encoder, &renderer.device);
                    mesh_manager.fill(&renderer.device, &renderer.queue, &mut encoder, &handle, mesh);
                    profiler.end_scope(&mut encoder);
                }
                Instruction::AddTexture {
                    handle,
                    desc,
                    texture,
                    view,
                    buffer,
                    cube,
                } => {
                    cmd_bufs.extend(buffer);
                    if cube {
                        texture_manager_cube.fill(&handle, desc, texture, view);
                    } else {
                        texture_manager_2d.fill(&handle, desc, texture, view);
                    }
                }
                Instruction::AddMaterial { handle, fill_invoke } => {
                    profiling::scope!("Add Material");
                    fill_invoke(
                        &mut material_manager,
                        &renderer.device,
                        renderer.mode,
                        &mut texture_manager_2d,
                        &handle,
                    );
                }
                Instruction::ChangeMaterial { handle, change_invoke } => {
                    profiling::scope!("Change Material");

                    change_invoke(
                        &mut material_manager,
                        &renderer.device,
                        renderer.mode,
                        &mut texture_manager_2d,
                        &mut object_manager,
                        &handle,
                    )
                }
                Instruction::AddObject { handle, object } => {
                    object_manager.fill(&handle, object, &mesh_manager, &mut material_manager);
                }
                Instruction::SetObjectTransform { handle, transform } => {
                    object_manager.set_object_transform(handle, transform);
                }
                Instruction::AddDirectionalLight { handle, light } => {
                    directional_light_manager.fill(&handle, light);
                }
                Instruction::ChangeDirectionalLight { handle, change } => {
                    directional_light_manager.update_directional_light(handle, change);
                }
                Instruction::SetAspectRatio { ratio } => camera_manager.set_aspect_ratio(Some(ratio)),
                Instruction::SetCameraData { data } => {
                    camera_manager.set_data(data);
                }
            }
        }
    }

    // Do these in dependency order
    // Level 2
    object_manager.ready(&mut material_manager);

    // Level 1
    material_manager.ready(&renderer.device, &renderer.queue, &texture_manager_2d);

    // Level 0
    let d2_texture = texture_manager_2d.ready(&renderer.device);
    let d2c_texture = texture_manager_cube.ready(&renderer.device);
    let directional_light_cameras = directional_light_manager.ready(&renderer.device, &renderer.queue, &camera_manager);
    mesh_manager.ready();

    drop((
        camera_manager,
        mesh_manager,
        texture_manager_2d,
        texture_manager_cube,
        material_manager,
        object_manager,
        directional_light_manager,
        profiler,
    ));

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
