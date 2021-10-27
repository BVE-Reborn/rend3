use glam::{Mat4, Vec3A};
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
    culling::{CPUDrawCall, CulledObjectSet},
    material::{PbrMaterial, SampleType, TransparencyType},
};

pub struct CpuCullerCullArgs<'a> {
    pub device: &'a Device,
    pub camera: &'a CameraManager,

    pub interfaces: &'a ShaderInterfaces,

    pub objects: &'a ObjectManager,

    pub transparency: TransparencyType,
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

        let objects = args.objects.get_objects::<PbrMaterial>(args.transparency as u64);

        let objects = crate::common::sorting::sort_objects(objects, args.camera, args.transparency.to_sorting());

        let (mut outputs, calls) = cull_internal(&objects, frustum, view, view_proj);

        assert_eq!(calls.len(), outputs.len());

        if outputs.is_empty() {
            // Dummy data
            outputs.push(PerObjectData {
                model_view: Mat4::ZERO,
                model_view_proj: Mat4::ZERO,
                inv_squared_scale: Vec3A::ZERO,
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
        let model = object.input.transform;
        let model_view = view * model;

        let transformed = object.input.bounding_sphere.apply_transform(model_view);
        if !frustum.contains_sphere(transformed) {
            continue;
        }

        let model_view_proj = view_proj * model;

        calls.push(CPUDrawCall {
            start_idx: object.input.start_idx,
            end_idx: object.input.start_idx + object.input.count,
            vertex_offset: object.input.vertex_offset,
            material_index: object.input.material_index,
        });

        let squared_scale = Vec3A::new(
            model_view.x_axis.length_squared() * model_view.determinant().signum(),
            model_view.y_axis.length_squared(),
            model_view.z_axis.length_squared(),
        );

        let inv_squared_scale = squared_scale.recip();

        outputs.push(PerObjectData {
            model_view,
            model_view_proj,
            inv_squared_scale,
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
        if previous_mat_handle != Some(draw.material_index) {
            previous_mat_handle = Some(draw.material_index);
            // TODO(material): only resolve the archetype lookup once
            let (material, internal) =
                materials.get_internal_material_full_by_index::<PbrMaterial>(draw.material_index as usize);
            let sample_type = material.sample_type;

            // As a workaround for OpenGL's combined samplers, we need to manually swap the linear and nearest samplers so that shader code can think it's always using linear.
            if state_sample_type != sample_type {
                let bg = match sample_type {
                    SampleType::Nearest => samplers.nearest_linear_bg.as_ref().as_cpu(),
                    SampleType::Linear => &samplers.linear_nearest_bg,
                };
                state_sample_type = sample_type;
                rpass.set_bind_group(samplers_binding_index, bg, &[]);
            }

            rpass.set_bind_group(material_binding_index, internal.bind_group.as_ref().as_cpu(), &[]);
        }
        let idx = idx as u32;
        rpass.draw_indexed(draw.start_idx..draw.end_idx, draw.vertex_offset, idx..idx + 1);
    }
}
