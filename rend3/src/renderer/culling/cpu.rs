use crate::{
    datatypes::MaterialHandle,
    renderer::{
        camera::Camera,
        culling::CullingPassData,
        frustum::ShaderFrustum,
        object::{InternalObject, ObjectManager},
        OrdEqFloat,
    },
    JobPriorities,
};
use futures::{stream::FuturesUnordered, StreamExt};
use glam::{Mat4, Vec4, Vec4Swizzles};
use itertools::Itertools;
use smallvec::SmallVec;
use switchyard::Switchyard;
use wgpu::Queue;

#[derive(Debug, Copy, Clone)]
pub struct CPUDrawCall {
    pub start_idx: u32,
    pub count: u32,
    pub vertex_offset: i32,
    pub handle: MaterialHandle,
}

#[derive(Debug, Clone)]
pub struct CullingOutputData {
    call: CPUDrawCall,
    output: ShaderOutputObject,
    distance: f32,
}

#[derive(Debug, Copy, Clone)]
#[repr(C, align(16))]
struct ShaderOutputObject {
    model_view: Mat4,
    model_view_proj: Mat4,
    // Actually a mat3, but funky shader time
    inv_trans_model_view_0: Vec4,
    inv_trans_model_view_1: Vec4,
    inv_trans_model_view_2: Vec4,
    // Unused in shader
    _material_idx: u32,
    // Unused in shader
    _active: u32,
}

unsafe impl bytemuck::Zeroable for ShaderOutputObject {}
unsafe impl bytemuck::Pod for ShaderOutputObject {}

pub(crate) async fn run<TD>(
    yard: &Switchyard<TD>,
    yard_priorities: JobPriorities,
    queue: &Queue,
    object_manager: &ObjectManager,
    data: &mut CullingPassData,
    camera: Camera,
) where
    TD: 'static,
{
    let object_count = data.object_count;

    let proj = camera.proj();
    let frustum = ShaderFrustum::from_matrix(proj);
    let view = camera.view();
    let view_proj = camera.view_proj();

    // TODO: real thread count
    let threads = 8;
    // Want chunks of no smaller than 1 to not trigger assert in chunks.
    let chunks = ((object_count + threads - 1) / threads).max(1);

    let chunks = object_manager
        .values()
        .cloned()
        .chunks((object_count as usize / 8).max(1));

    let mut res_futures = FuturesUnordered::new();
    for object_chunk in (&chunks).into_iter().map(|v| v.collect_vec()) {
        let object_chunk: Vec<InternalObject> = object_chunk;

        res_futures.push(yard.spawn(
            yard_priorities.compute_pool,
            yard_priorities.culling_priority,
            async move {
                let mut chunk_results = Vec::with_capacity(object_chunk.len());

                for object in object_chunk {
                    let model = object.transform.transform;
                    let model_view = view * model;

                    let transformed = object.sphere.apply_transform(model_view);
                    if !frustum.contains_sphere(transformed) {
                        continue;
                    }

                    let view_position = (model_view * object.sphere.center.extend(1.0)).xyz();
                    let distance = view_position.length_squared();

                    let model_view_proj = view_proj * model;

                    let inv_trans_model_view = model_view.inverse().transpose();

                    let output = ShaderOutputObject {
                        model_view,
                        model_view_proj,
                        inv_trans_model_view_0: inv_trans_model_view.x_axis,
                        inv_trans_model_view_1: inv_trans_model_view.y_axis,
                        inv_trans_model_view_2: inv_trans_model_view.z_axis,
                        _material_idx: 0,
                        _active: 0,
                    };

                    let call = CPUDrawCall {
                        start_idx: object.start_idx,
                        count: object.count,
                        vertex_offset: object.vertex_offset,
                        handle: object.material,
                    };

                    chunk_results.push(CullingOutputData { call, output, distance })
                }

                chunk_results
            },
        ))
    }

    let mut total_post_cull_objects = 0_usize;
    let mut res_vectors: SmallVec<[_; 32]> = SmallVec::new();
    while let Some(vec) = res_futures.next().await {
        total_post_cull_objects += vec.len();
        res_vectors.push(vec)
    }

    let mut res = Vec::with_capacity(total_post_cull_objects);
    for vec in res_vectors {
        res.extend_from_slice(&vec);
    }

    res.sort_unstable_by_key(|v| (v.call.handle.0, OrdEqFloat(v.distance)));

    let mut output_data = Vec::with_capacity(res.len());
    let mut calls = Vec::with_capacity(res.len());

    for data in res {
        output_data.push(data.output);
        calls.push(data.call);
    }

    queue.write_buffer(&data.output_buffer, 0, bytemuck::cast_slice(&output_data));

    *data.inner.as_cpu_mut() = calls;
}
