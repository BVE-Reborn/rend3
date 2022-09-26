use std::{any::type_name, marker::PhantomData, num::NonZeroU64};

use encase::{ShaderSize, ShaderType};
use rend3::{
    format_sso,
    managers::{MaterialManager, ObjectManager, ShaderObject, TextureBindGroupIndex},
    types::{Material, MaterialArray},
    util::math::round_up_pot,
    Renderer, ShaderPreProcessor,
};
use serde::Serialize;
use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BufferBindingType, ComputePipeline,
    ComputePipelineDescriptor, PipelineLayoutDescriptor, RenderPipeline, ShaderModuleDescriptor, ShaderStages,
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

            let invocation_count = object.inner.index_count / 3;
            let range = ShaderObjectRange {
                invocation_start: current_invocation,
                invocation_end: current_invocation + invocation_count,
                object_id: handle.idx as u32,
            };

            current_ranges[current_object_index as usize] = range;
            current_object_index += 1;
            current_invocation += round_up_pot(invocation_count, WORKGROUP_SIZE);
        }
    }

    jobs
}

struct DrawCall {}

#[derive(Serialize)]
struct CullingPreprocessingArguments {
    vertex_array_counts: u32,
}

struct GpuCuller<M> {
    bgl: BindGroupLayout,
    pipeline: ComputePipeline,
    _phantom: PhantomData<M>,
}

impl<M> GpuCuller<M>
where
    M: Material,
{
    pub fn new(renderer: &Renderer, spp: &ShaderPreProcessor) {
        let type_name = type_name::<M>();

        let source = spp
            .render_shader(
                "base",
                &CullingPreprocessingArguments {
                    vertex_array_counts: <M::SupportedAttributeArrayType as MaterialArray>::COUNT,
                },
            )
            .unwrap();

        let sm = renderer.device.create_shader_module(ShaderModuleDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} SM")),
            source: todo!(),
        });

        let bgl = renderer.device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} BGL")),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(4),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(ShaderObject::<M>::SHADER_SIZE),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(ShaderCullingJob::SHADER_SIZE),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZeroU64::new(4)),
                    },
                    count: None,
                },
            ],
        });

        let pll = renderer.device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} PLL")),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = renderer.device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} PLL")),
            layout: Some(&pll),
            module: todo!(),
            entry_point: todo!(),
        });

        Self {
            bgl,
            pipeline: todo!(),
            _phantom: PhantomData,
        }
    }

    pub fn cull(&self, jobs: ShaderCullingJobs) -> DrawCall {}
}
