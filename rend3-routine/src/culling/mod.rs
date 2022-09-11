use encase::ShaderType;
use rend3::{
    managers::{MaterialManager, ObjectManager, TextureBindGroupIndex},
    types::Material,
    util::math::round_up_pot,
};

const BATCH_SIZE: usize = 256;
const WORKGROUP_SIZE: u32 = 256;

struct ShaderCullingJobs {
    keys: Vec<ShaderJobKey>,
    jobs: Vec<ShaderCullingJob>,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ShaderJobKey {
    material_key: u64,
    bind_group_index: TextureBindGroupIndex,
}

#[derive(ShaderType)]
struct ShaderCullingJob {
    ranges: [ShaderObjectRange; BATCH_SIZE],
    total_objects: u32,
    total_invocations: u32,
}

#[derive(Copy, Clone, Default, ShaderType)]
struct ShaderObjectRange {
    invocation_start: u32,
    invocation_end: u32,
    object_id: u32,
}

fn batch_objects<M: Material>(material_manager: &MaterialManager, object_manager: &ObjectManager) -> ShaderCullingJobs {
    let objects = object_manager.enumerated_objects::<M>();
    let predicted_count = objects.size_hint().1.unwrap_or(0);

    let material_archetype = material_manager.archetype_view::<M>();

    let mut sorted_objects = Vec::with_capacity(predicted_count);
    for (handle, object) in objects {
        let material = material_archetype.material(*object.material_handle);
        let bind_group_index = material.bind_group_index.into_cpu();

        let material_key = material.inner.key();

        sorted_objects.push((
            ShaderJobKey {
                material_key,
                bind_group_index,
            },
            handle,
            object,
        ))
    }

    sorted_objects.sort_unstable_by_key(|(k, _, _)| k);

    let jobs = ShaderCullingJobs {
        jobs: Vec::new(),
        keys: Vec::new(),
    };

    if !sorted_objects.is_empty() {
        let mut current_invocation = 0_u32;
        let mut current_object_index = 0_u32;
        let mut current_ranges = [ShaderObjectRange::default(); BATCH_SIZE];
        let mut current_key = sorted_objects.first().unwrap().0;

        for (key, handle, object) in sorted_objects {
            if key != current_key {
                jobs.jobs.push(ShaderCullingJob {
                    ranges: current_ranges,
                    total_objects: current_object_index,
                    total_invocations: current_invocation,
                });
                jobs.keys.push(key);

                current_key = key;
                current_invocation = 0;
                current_object_index = 0;
            }

            let invocation_count = round_up_pot(object.inner.index_count, WORKGROUP_SIZE);
            let range = ShaderObjectRange {
                invocation_start: current_invocation,
                invocation_end: current_invocation + invocation_count,
                object_id: handle.idx as u32,
            };

            current_ranges[current_object_index as usize] = range;
            current_object_index += 1;
        }
    }

    jobs
}
