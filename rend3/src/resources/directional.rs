use crate::{
    resources::CameraManager,
    types::{Camera, CameraProjection, DirectionalLight, DirectionalLightHandle},
    util::{bind_merge::BindGroupBuilder, buffer::WrappedPotBuffer, registry::ResourceRegistry},
    INTERNAL_SHADOW_DEPTH_FORMAT, SHADOW_DIMENSIONS,
};
use arrayvec::ArrayVec;
use glam::{Mat4, Vec3A};
use rend3_types::{DirectionalLightChange, RawDirectionalLightHandle};
use std::{
    mem::{self, size_of},
    num::{NonZeroU32, NonZeroU64},
    sync::Arc,
};
use wgpu::{
    BindGroup, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, Buffer,
    BufferBindingType, BufferUsages, Device, Extent3d, Queue, ShaderStages, TextureAspect, TextureDescriptor,
    TextureDimension, TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor, TextureViewDimension,
};

pub struct InternalDirectionalLight {
    pub inner: DirectionalLight,
}

#[derive(Debug, Copy, Clone)]
#[repr(C, align(16))]
struct ShaderDirectionalLightBufferHeader {
    total_lights: u32,
}

unsafe impl bytemuck::Zeroable for ShaderDirectionalLightBufferHeader {}
unsafe impl bytemuck::Pod for ShaderDirectionalLightBufferHeader {}

#[derive(Debug, Copy, Clone)]
#[repr(C, align(16))]
struct ShaderDirectionalLight {
    pub view_proj: Mat4,
    pub color: Vec3A,
    pub direction: Vec3A,
}

unsafe impl bytemuck::Zeroable for ShaderDirectionalLight {}
unsafe impl bytemuck::Pod for ShaderDirectionalLight {}

pub struct DirectionalLightManager {
    buffer: WrappedPotBuffer,

    view: TextureView,
    layer_views: Vec<Arc<TextureView>>,

    bgl: BindGroupLayout,
    bg: BindGroup,

    registry: ResourceRegistry<InternalDirectionalLight, DirectionalLight>,
}
impl DirectionalLightManager {
    pub fn new(device: &Device) -> Self {
        let registry = ResourceRegistry::new();

        let buffer = WrappedPotBuffer::new(
            device,
            0,
            mem::size_of::<ShaderDirectionalLight>() as _,
            BufferUsages::STORAGE,
            Some("directional lights"),
        );

        let (view, layer_views) = create_shadow_texture(device, 1);

        let bgl = create_shadow_bgl(device);
        let bg = create_shadow_bg(device, &bgl, &buffer, &view);

        Self {
            buffer,
            view,
            layer_views,

            bgl,
            bg,

            registry,
        }
    }

    pub fn allocate(&self) -> DirectionalLightHandle {
        self.registry.allocate()
    }

    pub fn fill(&mut self, handle: &DirectionalLightHandle, light: DirectionalLight) {
        self.registry.insert(handle, InternalDirectionalLight { inner: light });
    }

    pub fn get_mut(&mut self, handle: RawDirectionalLightHandle) -> &mut InternalDirectionalLight {
        self.registry.get_mut(handle)
    }

    pub fn get_layer_view_arc(&self, layer: u32) -> Arc<TextureView> {
        Arc::clone(&self.layer_views[layer as usize])
    }

    pub fn update_directional_light(&mut self, handle: RawDirectionalLightHandle, change: DirectionalLightChange) {
        let internal = self.registry.get_mut(handle);
        internal.inner.update_from_changes(change);
    }

    pub fn get_bgl(&self) -> &BindGroupLayout {
        &self.bgl
    }

    pub fn get_bg(&self) -> &BindGroup {
        &self.bg
    }

    pub fn ready(&mut self, device: &Device, queue: &Queue, user_camera: &CameraManager) -> Vec<CameraManager> {
        profiling::scope!("Directional Light Ready");

        self.registry.remove_all_dead(|_, _, _| ());

        let registered_count: usize = self.registry.values().len();
        let recreate_view = registered_count != self.layer_views.len() && registered_count != 0;
        if recreate_view {
            let (view, layer_views) = create_shadow_texture(device, registered_count as u32);
            self.view = view;
            self.layer_views = layer_views;
        }

        let registry = &self.registry;

        let size =
            registered_count * size_of::<ShaderDirectionalLight>() + size_of::<ShaderDirectionalLightBufferHeader>();

        let mut cameras = Vec::with_capacity(registered_count);

        let mut buffer = Vec::with_capacity(size);
        buffer.extend_from_slice(bytemuck::bytes_of(&ShaderDirectionalLightBufferHeader {
            total_lights: registry.count() as u32,
        }));
        for light in registry.values() {
            let cs = shadow(light, user_camera);
            for camera in &cs {
                buffer.extend_from_slice(bytemuck::bytes_of(&ShaderDirectionalLight {
                    view_proj: camera.view_proj(),
                    color: (light.inner.color * light.inner.intensity).into(),
                    direction: light.inner.direction.into(),
                }));
            }
            cameras.extend_from_slice(&cs);
        }

        let reallocated_buffer = self.buffer.write_to_buffer(device, queue, &buffer);

        if reallocated_buffer || recreate_view {
            self.bg = create_shadow_bg(device, &self.bgl, &self.buffer, &self.view);
        }

        cameras
    }

    pub fn values(&self) -> impl Iterator<Item = &InternalDirectionalLight> {
        self.registry.values()
    }
}

// TODO: re-enable cascades
fn shadow(l: &InternalDirectionalLight, user_camera: &CameraManager) -> ArrayVec<CameraManager, 4> {
    let mut cascades = ArrayVec::new();

    cascades.push(CameraManager::new(
        Camera {
            projection: CameraProjection::Orthographic {
                size: Vec3A::splat(l.inner.distance),
                direction: l.inner.direction.into(),
            },
            location: user_camera.get_data().location,
        },
        None,
    ));

    cascades
}

/*
fn shadow_cascades(l: &InternalDirectionalLight, user_camera: &CameraManager) -> ArrayVec<CameraManager, 4> {
    let mut cascades = ArrayVec::new();

    let view = Mat4::look_at_lh(Vec3::ZERO, l.inner.direction, Vec3::Y);
    let user_camera_proj = user_camera.proj();
    let user_camera_inv_view_proj = user_camera.view_proj().inverse();
    for window in l.inner.distances.windows(2) {
        let start = window[0];
        let end = window[1];

        let start_projected = user_camera_proj.project_point3(Vec3::new(0.0, 0.0, start.max(0.1))).z;
        let end_projected = user_camera_proj.project_point3(Vec3::new(0.0, 0.0, end)).z;

        let frustum_points = [
            Vec3::new(-1.0, -1.0, start_projected),
            Vec3::new(1.0, -1.0, start_projected),
            Vec3::new(-1.0, 1.0, start_projected),
            Vec3::new(1.0, 1.0, start_projected),
            Vec3::new(-1.0, -1.0, end_projected),
            Vec3::new(1.0, -1.0, end_projected),
            Vec3::new(-1.0, 1.0, end_projected),
            Vec3::new(1.0, 1.0, end_projected),
        ];

        let mat = view * user_camera_inv_view_proj;

        let (sview_min, sview_max) =
            vec_min_max(IntoIterator::into_iter(frustum_points).map(|p| Vec3A::from(mat.project_point3(p))));
        let (world_min, world_max) = vec_min_max(
            IntoIterator::into_iter(frustum_points).map(|p| Vec3A::from(user_camera_inv_view_proj.project_point3(p))),
        );

        // TODO: intermediate bounding sphere
        cascades.push(CameraManager::new(
            Camera {
                projection: CameraProjection::Orthographic {
                    size: sview_max - sview_min,
                    direction: l.inner.direction.into(),
                },
                location: ((world_min + world_max) * 0.5).into(),
            },
            None,
        ));
    }

    cascades
}
*/

#[allow(unused)]
fn vec_min_max(iter: impl IntoIterator<Item = Vec3A>) -> (Vec3A, Vec3A) {
    let mut iter = iter.into_iter();
    let mut min = iter.next().unwrap();
    let mut max = min;
    for point in iter {
        min = min.min(point);
        max = max.max(point);
    }

    (min, max)
}

fn create_shadow_texture(device: &Device, count: u32) -> (TextureView, Vec<Arc<TextureView>>) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("shadow texture"),
        size: Extent3d {
            width: SHADOW_DIMENSIONS,
            height: SHADOW_DIMENSIONS,
            depth_or_array_layers: count,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: INTERNAL_SHADOW_DEPTH_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
    });

    let primary_view = texture.create_view(&TextureViewDescriptor {
        label: Some("shadow texture view"),
        format: None,
        dimension: Some(TextureViewDimension::D2Array),
        aspect: TextureAspect::All,
        base_mip_level: 0,
        mip_level_count: None,
        base_array_layer: 0,
        array_layer_count: None,
    });

    let layer_views: Vec<_> = (0..count)
        .map(|idx| {
            Arc::new(texture.create_view(&TextureViewDescriptor {
                label: Some(&format!("shadow texture layer {}", count)),
                format: None,
                dimension: Some(TextureViewDimension::D2),
                aspect: TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: idx,
                array_layer_count: NonZeroU32::new(1),
            }))
        })
        .collect();

    (primary_view, layer_views)
}

fn create_shadow_bgl(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("shadow bgl"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(
                        (mem::size_of::<ShaderDirectionalLightBufferHeader>()
                            + mem::size_of::<ShaderDirectionalLight>()) as _,
                    ),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Depth,
                    view_dimension: TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}

fn create_shadow_bg(device: &Device, bgl: &BindGroupLayout, buffer: &Buffer, view: &TextureView) -> BindGroup {
    BindGroupBuilder::new(Some("shadow bg"))
        .with_buffer(buffer)
        .with_texture_view(view)
        .build(device, bgl)
}
