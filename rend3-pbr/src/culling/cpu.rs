use glam::{Mat3, Mat3A, Mat4};
use ordered_float::OrderedFloat;
use rend3::{
    resources::{CameraManager, InternalObject, MaterialManager},
    types::{Material, SampleType},
    util::frustum::ShaderFrustum,
    ModeData,
};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BindGroupDescriptor, BindGroupEntry, BufferUsages, Device, RenderPass,
};

use crate::{
    common::{
        interfaces::{PerObjectData, ShaderInterfaces},
        samplers::Samplers,
    },
    culling::{CPUDrawCall, CulledObjectSet, Sorting},
};

pub struct CpuCullerCullArgs<'a, FilterFn>
where
    FilterFn: FnMut(&InternalObject, &Material) -> bool,
{
    pub device: &'a Device,
    pub camera: &'a CameraManager,

    pub interfaces: &'a ShaderInterfaces,

    pub materials: &'a MaterialManager,

    pub objects: &'a [InternalObject],
    pub filter: FilterFn,

    pub sort: Option<Sorting>,
}

pub struct CpuCuller {}

impl CpuCuller {
    pub fn new() -> Self {
        Self {}
    }

    pub fn cull<FilterFn>(&self, mut args: CpuCullerCullArgs<'_, FilterFn>) -> CulledObjectSet
    where
        FilterFn: FnMut(&InternalObject, &Material) -> bool,
    {
        profiling::scope!("CPU Culling");
        let frustum = ShaderFrustum::from_matrix(args.camera.proj());
        let view = args.camera.view();
        let view_proj = args.camera.view_proj();

        let (mut outputs, calls) = if let Some(sorting) = args.sort {
            profiling::scope!("Sorting");
            let mut objects: Vec<_> = args
                .objects
                .iter()
                .map(|o| {
                    let distance = args.camera.get_data().location.distance_squared(o.location);
                    (o, OrderedFloat(distance))
                })
                .collect();

            match sorting {
                Sorting::FrontToBack => {
                    objects.sort_unstable_by(|(_, lh_distance), (_, rh_distance)| lh_distance.cmp(rh_distance))
                }
                Sorting::BackToFront => {
                    objects.sort_unstable_by(|(_, lh_distance), (_, rh_distance)| rh_distance.cmp(lh_distance))
                }
            }

            cull_internal(
                args.materials,
                &mut args.filter,
                objects.into_iter().map(|(o, _)| o),
                frustum,
                view,
                view_proj,
            )
        } else {
            cull_internal(
                args.materials,
                &mut args.filter,
                args.objects.iter(),
                frustum,
                view,
                view_proj,
            )
        };

        assert_eq!(calls.len(), outputs.len());

        if outputs.is_empty() {
            // Dummy data
            outputs.push(PerObjectData {
                model_view: Mat4::ZERO,
                model_view_proj: Mat4::ZERO,
                inv_trans_model_view: Mat3A::ZERO,
                material_idx: 0,
            });
        }

        let output_buffer = args.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("culling output"),
            contents: bytemuck::cast_slice(&outputs),
            usage: BufferUsages::STORAGE,
        });

        let output_bg = args.device.create_bind_group(&BindGroupDescriptor {
            label: Some("culling input bg"),
            layout: &args.interfaces.culled_object_bgl,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: output_buffer.as_entire_binding(),
            }],
        });

        CulledObjectSet {
            calls: ModeData::CPU(calls),
            output_bg,
        }
    }
}

impl Default for CpuCuller {
    fn default() -> Self {
        Self::new()
    }
}

pub fn cull_internal<'a, FilterFn>(
    materials: &MaterialManager,
    filter: &mut FilterFn,
    objects: impl ExactSizeIterator<Item = &'a InternalObject>,
    frustum: ShaderFrustum,
    view: Mat4,
    view_proj: Mat4,
) -> (Vec<PerObjectData>, Vec<CPUDrawCall>)
where
    FilterFn: FnMut(&InternalObject, &Material) -> bool,
{
    let mut outputs = Vec::with_capacity(objects.len());
    let mut calls = Vec::with_capacity(objects.len());

    for object in objects.into_iter() {
        if !filter(object, materials.get_material(object.material.get_raw())) {
            continue;
        }

        let model = object.transform;
        let model_view = view * model;

        let transformed = object.sphere.apply_transform(model_view);
        if !frustum.contains_sphere(transformed) {
            continue;
        }

        let model_view_proj = view_proj * model;

        let inv_trans_model_view = Mat3::from_mat4(model_view.inverse().transpose());

        calls.push(CPUDrawCall {
            start_idx: object.start_idx,
            end_idx: object.start_idx + object.count,
            vertex_offset: object.vertex_offset,
            // TODO: Elide these clones?
            material_handle: object.material.get_raw(),
        });

        outputs.push(PerObjectData {
            model_view,
            model_view_proj,
            inv_trans_model_view: inv_trans_model_view.into(),
            material_idx: 0,
        });
    }

    (outputs, calls)
}

pub fn run<'rpass>(
    rpass: &mut RenderPass<'rpass>,
    draws: &'rpass [CPUDrawCall],
    samplers: &'rpass Samplers,
    samplers_binding_index: u32,
    materials: &'rpass MaterialManager,
    material_binding_index: u32,
) {
    let mut state_sample_type = SampleType::Linear;

    for (idx, draw) in draws.iter().enumerate() {
        let (material_bind_group, sample_type) = materials.cpu_get_bind_group(draw.material_handle);

        // As a workaround for OpenGL's combined samplers, we need to manually swap the linear and nearest samplers so that shader code can think it's always using linear.
        if state_sample_type != sample_type {
            let bg = match sample_type {
                SampleType::Nearest => samplers.nearest_linear_bg.as_ref().as_cpu(),
                SampleType::Linear => &samplers.linear_nearest_bg,
            };
            state_sample_type = sample_type;
            rpass.set_bind_group(samplers_binding_index, bg, &[]);
        }

        rpass.set_bind_group(material_binding_index, material_bind_group, &[]);
        let idx = idx as u32;
        rpass.draw_indexed(draw.start_idx..draw.end_idx, draw.vertex_offset, idx..idx + 1);
    }
}
