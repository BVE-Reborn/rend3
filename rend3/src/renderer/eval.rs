use wgpu::CommandEncoderDescriptor;

use crate::{
    graph::InstructionEvaluationOutput,
    instruction::{Instruction, InstructionKind},
    Renderer,
};

use std::collections::{HashSet};

/// Set to true to check instructions for deletion before use.
/// This is a debug trap.
const CHECK_INSTRUCTIONS: bool = true; // didn't fail in 20 hour test with this enabled.

/// Instruction checker. Checks for instructions which delete an object preceding instructions which use that object.
/// This is a debug trap for a race condition.
fn check_instructions(renderer: &Renderer, instructions: &Vec<Instruction>) {
    use rend3_types::{ObjectMeshKind, Object};
    use crate::managers::MeshManager;
    ///  Check that when we add an object, its mesh was not previously deleted.
    fn check_mesh_in_add_object(
        deleted_mesh_handles: &HashSet<usize>,
        object: &Object,
        _mesh_manager: &MeshManager,
    ) {
        //////let mesh_manager_guard = mesh_manager.lock_internal_data();    // performance problem
        match &object.mesh_kind {
            ObjectMeshKind::Animated(_skeleton) => {} // no check, because bug is in static meshes
            ObjectMeshKind::Static(mesh_handle) => {
                let raw_mesh_handle = mesh_handle.get_raw();
                if deleted_mesh_handles.contains(&raw_mesh_handle.idx) {
                    panic!("Add of deleted mesh handle #{} at add object", raw_mesh_handle.idx);
                }
            }
        }
    }

    profiling::scope!("Instruction checking");
    let mut deleted_object_handles = HashSet::new();
    let mut deleted_mesh_handles = HashSet::new();
    //  Preallocate. 
    deleted_object_handles.reserve(instructions.len());
    deleted_mesh_handles.reserve(instructions.len());
    //  Prescan instructions for problems.
    for Instruction { kind, location : _} in instructions. iter() {
        match kind {
            InstructionKind::AddObject { handle, object } => {
                //  Must not add a deleted object in the same pass.
                if deleted_object_handles.contains(&handle.idx) {
                    panic!("Add of deleted object of object handle #{}", handle.idx);
                }
                check_mesh_in_add_object(&deleted_mesh_handles, &object, &renderer.mesh_manager);                    
            }
            
            InstructionKind::DeleteObject { handle } => {
                //  Track deleted object
                if !deleted_object_handles.insert(handle.idx) {
                    panic!("Two deletes of object handle #{}", handle.idx);
                }               
            }
            
            InstructionKind::DeleteMesh { handle } => {
                if !deleted_mesh_handles.insert(handle.idx) {
                    panic!("Two deletes of mesh handle #{}", handle.idx);
                }       
            }   
            
            _ => {}
        }     
    }
}

pub fn evaluate_instructions(renderer: &Renderer) -> InstructionEvaluationOutput {
    profiling::scope!("Renderer::evaluate_instructions");

    let mut instructions = renderer.instructions.consumer.lock();
    if CHECK_INSTRUCTIONS {
        check_instructions(&renderer, &instructions);   // debug trap
    }

    // 16 encoders is a reasonable default
    let mut cmd_bufs = Vec::with_capacity(16);

    let mut encoder =
        renderer.device.create_command_encoder(&CommandEncoderDescriptor { label: Some("primary encoder") });

    let mut data_core = renderer.data_core.lock();
    let data_core = &mut *data_core;

    {
        profiling::scope!("Instruction Processing");
        for Instruction { kind, location: _ } in instructions.drain(..) {
            match kind {
                InstructionKind::AddSkeleton { handle, skeleton } => {
                    profiling::scope!("Add Skeleton");
                    let profiler_query = data_core.profiler.try_lock().unwrap().begin_query(
                        "Add Skeleton",
                        &mut encoder,
                        &renderer.device,
                    );
                    data_core.skeleton_manager.add(handle, *skeleton);
                    data_core.profiler.try_lock().unwrap().end_query(&mut encoder, profiler_query);
                }
                InstructionKind::AddTexture2D { handle, internal_texture, cmd_buf } => {
                    cmd_bufs.extend(cmd_buf);
                    data_core.d2_texture_manager.fill(handle, internal_texture);
                }
                InstructionKind::AddTexture2DFromTexture { handle, texture } => {
                    data_core.d2_texture_manager.fill_from_texture(&renderer.device, &mut encoder, handle, texture)
                }
                InstructionKind::AddTextureCube { handle, internal_texture, cmd_buf } => {
                    cmd_bufs.extend(cmd_buf);
                    data_core.d2c_texture_manager.fill(handle, internal_texture);
                }
                InstructionKind::AddMaterial { handle, fill_invoke } => {
                    profiling::scope!("Add Material");
                    fill_invoke(
                        &mut data_core.material_manager,
                        &renderer.device,
                        renderer.profile,
                        &mut data_core.d2_texture_manager,
                        handle,
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
                        handle,
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
                    data_core.object_manager.set_object_transform(&handle, transform);
                }
                InstructionKind::SetSkeletonJointDeltas { handle, joint_matrices } => {
                    data_core.skeleton_manager.set_joint_matrices(&handle, joint_matrices);
                }
                InstructionKind::AddDirectionalLight { handle, light } => {
                    data_core.directional_light_manager.add(&handle, light);
                }
                InstructionKind::ChangeDirectionalLight { handle, change } => {
                    data_core.directional_light_manager.update(&handle, change);
                }
                InstructionKind::AddPointLight { handle, light } => {
                    data_core.point_light_manager.add(&handle, light);
                }
                InstructionKind::ChangePointLight { handle, change } => {
                    data_core.point_light_manager.update(handle, change);
                }
                InstructionKind::SetAspectRatio { ratio } => {
                    data_core.viewport_camera_state.set_aspect_ratio(Some(ratio))
                }
                InstructionKind::SetCameraData { data } => {
                    data_core.viewport_camera_state.set_data(data);
                }
                InstructionKind::DuplicateObject { src_handle, dst_handle, change } => {
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
                    renderer.mesh_manager.remove(&handle);
                    renderer.resource_handle_allocators.mesh.deallocate(handle);       // this finally consumes the handle.
                    
                }
                InstructionKind::DeleteSkeleton { handle } => {           
                    data_core.skeleton_manager.remove(&renderer.mesh_manager, &handle);
                    renderer.resource_handle_allocators.skeleton.deallocate(handle);
                }
                InstructionKind::DeleteTexture2D { handle } => {        
                    data_core.d2_texture_manager.remove(&handle);
                    renderer.resource_handle_allocators.d2_texture.deallocate(handle);
                }
                InstructionKind::DeleteTextureCube { handle } => {                   
                    data_core.d2c_texture_manager.remove(&handle);
                    renderer.resource_handle_allocators.d2c_texture.deallocate(handle);
                }
                InstructionKind::DeleteMaterial { handle } => {                    
                    data_core.material_manager.remove(&handle);
                    renderer.resource_handle_allocators.material.deallocate(handle);
                }
                InstructionKind::DeleteObject { handle } => {                   
                    data_core.object_manager.remove(&handle);
                    renderer.resource_handle_allocators.object.deallocate(handle);
                }
                InstructionKind::DeleteDirectionalLight { handle } => {
                    data_core.directional_light_manager.remove(&handle);
                    renderer.resource_handle_allocators.directional_light.deallocate(handle);
                    
                }
                InstructionKind::DeletePointLight { handle } => {
                    data_core.point_light_manager.remove(&handle);
                    renderer.resource_handle_allocators.point_light.deallocate(handle);
                    
                }
                InstructionKind::DeleteGraphData { handle } => {
                    data_core.graph_storage.remove(&handle);
                    renderer.resource_handle_allocators.graph_storage.deallocate(handle);
                    
                }
            }
        }
    }

    // Do these in dependency order
    // Level 3
    data_core.object_manager.evaluate(&renderer.device, &mut encoder, &renderer.scatter);

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
    let (shadow_target_size, shadows) =
        data_core.directional_light_manager.evaluate(renderer, &data_core.viewport_camera_state);
    data_core.point_light_manager.evaluate(renderer);
    let (mesh_buffer, mesh_cmd_buf) = renderer.mesh_manager.evaluate(&renderer.device);

    cmd_bufs.push(mesh_cmd_buf);
    cmd_bufs.push(encoder.finish());

    InstructionEvaluationOutput { cmd_bufs, d2_texture, d2c_texture, shadow_target_size, shadows, mesh_buffer }
}
