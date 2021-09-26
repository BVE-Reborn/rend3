use glam::{Mat3, Mat3A, Mat4};
use ordered_float::OrderedFloat;
use rend3::{
    resources::{CameraManager, InternalObject, MaterialManager, ObjectManager},
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
    material::{PbrMaterial, SampleType, TransparencyType},
};

pub struct CpuCullerCullArgs<'a> {
    pub device: &'a Device,
    pub camera: &'a CameraManager,

    pub interfaces: &'a ShaderInterfaces,

    pub objects: &'a mut ObjectManager,

    pub transparency: TransparencyType,

    pub sort: Option<Sorting>,
}

pub struct CpuCuller {}

impl CpuCuller {
    pub fn new() -> Self {
        Self {}
    }

    pub fn cull(&self, args: CpuCullerCullArgs<'_>) -> CulledObjectSet {
        profiling::scope!("CPU Culling");
        let frustum = ShaderFrustum::from_matrix(args.camera.proj());
        let view = args.camera.view();
        let view_proj = args.camera.view_proj();

        let objects = args.objects.get_objects_mut::<PbrMaterial>(args.transparency as u64);

        if let Some(sorting) = args.sort {
            profiling::scope!("Sorting");

            let camera_location = args.camera.get_data().location;

            match sorting {
                Sorting::FrontToBack => {
                    objects.sort_unstable_by_key(|o| OrderedFloat(o.location.distance_squared(camera_location)));
                }
                Sorting::BackToFront => {
                    objects.sort_unstable_by_key(|o| OrderedFloat(-o.location.distance_squared(camera_location)));
                }
            }
        }

        let (mut outputs, calls) = cull_internal(objects, frustum, view, view_proj);

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

pub fn cull_internal(
    objects: &[InternalObject],
    frustum: ShaderFrustum,
    view: Mat4,
    view_proj: Mat4,
) -> (Vec<PerObjectData>, Vec<CPUDrawCall>) {
    let mut outputs = Vec::with_capacity(objects.len());
    let mut calls = Vec::with_capacity(objects.len());

    for object in objects {
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

    let mut previous_mat_handle = None;
    for (idx, draw) in draws.iter().enumerate() {
        if previous_mat_handle != Some(draw.material_handle) {
            previous_mat_handle = Some(draw.material_handle);
            // TODO(material): only resolve the archetype lookup once
            let material = materials.get_internal_material::<PbrMaterial>(draw.material_handle);
            let sample_type = material.mat.sample_type;

            // As a workaround for OpenGL's combined samplers, we need to manually swap the linear and nearest samplers so that shader code can think it's always using linear.
            if state_sample_type != sample_type {
                let bg = match sample_type {
                    SampleType::Nearest => samplers.nearest_linear_bg.as_ref().as_cpu(),
                    SampleType::Linear => &samplers.linear_nearest_bg,
                };
                state_sample_type = sample_type;
                rpass.set_bind_group(samplers_binding_index, bg, &[]);
            }

            rpass.set_bind_group(material_binding_index, material.bind_group.as_ref().as_cpu(), &[]);
        }
        let idx = idx as u32;
        rpass.draw_indexed(draw.start_idx..draw.end_idx, draw.vertex_offset, idx..idx + 1);
    }
}
