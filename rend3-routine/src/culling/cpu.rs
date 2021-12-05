use glam::{Mat4, Vec3};
use rend3::{
    managers::{CameraManager, InternalObject, MaterialManager, ObjectManager},
    util::frustum::ShaderFrustum,
    ModeData,
};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BufferUsages, Device, RenderPass,
};

use crate::{
    common::interfaces::{PerObjectData, ShaderInterfaces},
    culling::{CPUDrawCall, CulledObjectSet},
    material::{PbrMaterial, TransparencyType},
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
                pad0: [0; 12],
                material_idx: 0,
                inv_squared_scale: Vec3::ZERO,
            });
        }

        let output_buffer = args.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("culling output"),
            contents: bytemuck::cast_slice(&outputs),
            usage: BufferUsages::STORAGE,
        });

        CulledObjectSet {
            calls: ModeData::CPU(calls),
            output_buffer,
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

        let squared_scale = Vec3::new(
            model_view.x_axis.truncate().length_squared(),
            model_view.y_axis.truncate().length_squared(),
            model_view.z_axis.truncate().length_squared(),
        );

        let inv_squared_scale = squared_scale.recip();

        outputs.push(PerObjectData {
            model_view,
            model_view_proj,
            material_idx: 0,
            pad0: [0; 12],
            inv_squared_scale,
        });
    }

    (outputs, calls)
}

pub fn run<'rpass>(
    rpass: &mut RenderPass<'rpass>,
    draws: &'rpass [CPUDrawCall],
    materials: &'rpass MaterialManager,
    material_binding_index: u32,
) {
    let mut previous_mat_handle = None;
    for (idx, draw) in draws.iter().enumerate() {
        if previous_mat_handle != Some(draw.material_index) {
            previous_mat_handle = Some(draw.material_index);
            // TODO(material): only resolve the archetype lookup once
            let (_, internal) =
                materials.get_internal_material_full_by_index::<PbrMaterial>(draw.material_index as usize);

            // TODO: GL always gets linear sampling.

            rpass.set_bind_group(material_binding_index, internal.bind_group.as_ref().as_cpu(), &[]);
        }
        let idx = idx as u32;
        rpass.draw_indexed(draw.start_idx..draw.end_idx, draw.vertex_offset, idx..idx + 1);
    }
}
