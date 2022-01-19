use crate::{
    managers::CameraManager,
    types::{Camera, CameraProjection, DirectionalLight, DirectionalLightHandle},
    util::{
        bind_merge::{BindGroupBuilder, BindGroupLayoutBuilder},
        buffer::WrappedPotBuffer,
        registry::ResourceRegistry,
        typedefs::FastHashMap,
    },
    INTERNAL_SHADOW_DEPTH_FORMAT, SHADOW_DIMENSIONS,
};
use arrayvec::ArrayVec;
use glam::{Mat4, UVec2, Vec2, Vec3, Vec3A};
use rend3_types::{DirectionalLightChange, Handedness, RawDirectionalLightHandle};
use std::{
    mem::{self, size_of},
    num::{NonZeroU32, NonZeroU64},
    sync::atomic::{AtomicUsize, Ordering},
};
use wgpu::{
    BindGroup, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, Buffer,
    BufferBindingType, BufferUsages, Device, Extent3d, Queue, ShaderStages, TextureAspect, TextureDescriptor,
    TextureDimension, TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor, TextureViewDimension,
};

/// Internal representation of a directional light.
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
    pub offset: Vec2,
    pub size: f32,
}

unsafe impl bytemuck::Zeroable for ShaderDirectionalLight {}
unsafe impl bytemuck::Pod for ShaderDirectionalLight {}

/// Manages directional lights and their associated shadow maps.
pub struct DirectionalLightManager {
    buffer: WrappedPotBuffer,

    view: TextureView,
    layer_views: Vec<TextureView>,
    coords: Vec<ShadowCoordinates>,
    extent: Extent3d,

    bgl: BindGroupLayout,
    bg: BindGroup,

    registry: ResourceRegistry<InternalDirectionalLight, DirectionalLight>,
}
impl DirectionalLightManager {
    pub fn new(device: &Device) -> Self {
        profiling::scope!("DirectionalLightManager::new");

        let registry = ResourceRegistry::new();

        let buffer = WrappedPotBuffer::new(
            device,
            0,
            mem::size_of::<ShaderDirectionalLight>() as _,
            BufferUsages::STORAGE,
            Some("directional lights"),
        );

        let (view, layer_views) = create_shadow_texture(device, Extent3d::default());

        let bgl = create_shadow_bgl(device);
        let bg = create_shadow_bg(device, &bgl, &buffer, &view);

        Self {
            buffer,
            view,
            layer_views,
            coords: Vec::default(),
            extent: Extent3d::default(),

            bgl,
            bg,

            registry,
        }
    }

    pub fn allocate(counter: &AtomicUsize) -> DirectionalLightHandle {
        let idx = counter.fetch_add(1, Ordering::Relaxed);

        DirectionalLightHandle::new(idx)
    }

    pub fn fill(&mut self, handle: &DirectionalLightHandle, light: DirectionalLight) {
        self.registry.insert(handle, InternalDirectionalLight { inner: light });
    }

    pub fn get_mut(&mut self, handle: RawDirectionalLightHandle) -> &mut InternalDirectionalLight {
        self.registry.get_mut(handle)
    }

    pub fn get_layer_views(&self) -> &[TextureView] {
        &self.layer_views
    }

    pub fn update_directional_light(&mut self, handle: RawDirectionalLightHandle, change: DirectionalLightChange) {
        let internal = self.registry.get_mut(handle);
        internal.inner.update_from_changes(change);
    }

    pub fn add_to_bgl(bglb: &mut BindGroupLayoutBuilder) {
        bglb.append(
            ShaderStages::FRAGMENT,
            BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(
                    (mem::size_of::<ShaderDirectionalLightBufferHeader>() + mem::size_of::<ShaderDirectionalLight>())
                        as _,
                ),
            },
            None,
        )
        .append(
            ShaderStages::FRAGMENT,
            BindingType::Texture {
                sample_type: TextureSampleType::Depth,
                view_dimension: TextureViewDimension::D2Array,
                multisampled: false,
            },
            None,
        );
    }

    pub fn add_to_bg<'a>(&'a self, bgb: &mut BindGroupBuilder<'a>) {
        bgb.append_buffer(&self.buffer).append_texture_view(&self.view);
    }

    pub fn get_coords(&self) -> &[ShadowCoordinates] {
        &self.coords
    }

    pub fn ready(&mut self, device: &Device, queue: &Queue, user_camera: &CameraManager) -> Vec<CameraManager> {
        profiling::scope!("Directional Light Ready");

        self.registry.remove_all_dead(|_, _, _| ());

        let registered_count: usize = self.registry.values().len();
        let recreate_view = registered_count != self.coords.len() && registered_count != 0;
        if recreate_view {
            let (extent, coords) = allocate_shadows(self.registry.values().map(|_i| SHADOW_DIMENSIONS as usize));
            let (view, layer_views) = create_shadow_texture(device, extent);
            self.view = view;
            self.layer_views = layer_views;
            self.coords = coords;
            self.extent = extent;
        }

        let registry = &self.registry;

        let size =
            registered_count * size_of::<ShaderDirectionalLight>() + size_of::<ShaderDirectionalLightBufferHeader>();

        let mut cameras = Vec::with_capacity(registered_count);

        let mut buffer = Vec::with_capacity(size);
        buffer.extend_from_slice(bytemuck::bytes_of(&ShaderDirectionalLightBufferHeader {
            total_lights: registry.count() as u32,
        }));
        for (coords, light) in self.coords.iter().zip(registry.values()) {
            let cs = shadow(light, user_camera);
            for camera in &cs {
                buffer.extend_from_slice(bytemuck::bytes_of(&ShaderDirectionalLight {
                    view_proj: camera.view_proj(),
                    color: (light.inner.color * light.inner.intensity).into(),
                    direction: light.inner.direction.into(),
                    offset: coords.offset.as_vec2() / Vec2::splat(self.extent.width as f32),
                    size: coords.size as f32 / self.extent.width as f32,
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

/// The location of a shadow map in the shadow atlas.
#[derive(Debug, Default, Clone, Copy)]
pub struct ShadowCoordinates {
    pub layer: usize,
    pub offset: UVec2,
    pub size: usize,
}

// TODO: re-enable cascades
fn shadow(l: &InternalDirectionalLight, user_camera: &CameraManager) -> ArrayVec<CameraManager, 4> {
    let mut cascades = ArrayVec::new();

    let camera_location = user_camera.location();

    let shadow_texel_size = l.inner.distance / SHADOW_DIMENSIONS as f32;

    let look_at = match user_camera.handedness() {
        Handedness::Left => Mat4::look_at_lh,
        Handedness::Right => Mat4::look_at_rh,
    };

    let origin_view = look_at(Vec3::ZERO, l.inner.direction, Vec3::Y);
    let camera_origin_view = origin_view.transform_point3(camera_location);

    let offset = camera_origin_view.truncate() % shadow_texel_size;
    let shadow_location = camera_origin_view - Vec3::from((offset, 0.0));

    let inv_origin_view = origin_view.inverse();
    let new_shadow_location = inv_origin_view.transform_point3(shadow_location);

    cascades.push(CameraManager::new(
        Camera {
            projection: CameraProjection::Orthographic {
                size: Vec3A::splat(l.inner.distance),
            },
            view: look_at(new_shadow_location, new_shadow_location + l.inner.direction, Vec3::Y),
        },
        user_camera.handedness(),
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

fn allocate_shadows(shadow_sizes: impl Iterator<Item = usize>) -> (Extent3d, Vec<ShadowCoordinates>) {
    let mut sorted = shadow_sizes
        .enumerate()
        .map(|(id, size)| (id, size))
        .collect::<Vec<_>>();
    sorted.sort_unstable_by_key(|(_, size)| usize::MAX - size);

    let mut shadow_coordinates = FastHashMap::with_capacity_and_hasher(sorted.len(), Default::default());
    let mut sorted_iter = sorted.into_iter();
    let (id, max_size) = sorted_iter.next().unwrap();
    shadow_coordinates.insert(
        id,
        ShadowCoordinates {
            layer: 0,
            offset: UVec2::splat(0),
            size: max_size,
        },
    );

    let mut current_layer = 0usize;
    let mut current_size = 0usize;
    let mut current_count = 0usize;

    for (id, size) in sorted_iter {
        if size != current_size {
            current_layer += 1;
            current_size = size;
            current_count = 0;
        }

        let maps_per_dim = max_size / current_size;
        let total_maps_per_layer = maps_per_dim * maps_per_dim;

        if current_count >= total_maps_per_layer {
            current_layer += 1;
            current_count = 0;
        }

        let offset = UVec2::new(
            (current_count % maps_per_dim) as u32,
            (current_count / maps_per_dim) as u32,
        ) * current_size as u32;

        shadow_coordinates.insert(
            id,
            ShadowCoordinates {
                layer: current_layer,
                offset,
                size,
            },
        );
    }

    let mut shadow_coord_vec = vec![ShadowCoordinates::default(); shadow_coordinates.len()];

    for (idx, value) in shadow_coordinates {
        shadow_coord_vec[idx] = value;
    }

    (
        Extent3d {
            width: max_size as u32,
            height: max_size as u32,
            depth_or_array_layers: (current_layer + 1) as u32,
        },
        shadow_coord_vec,
    )
}

fn create_shadow_texture(device: &Device, size: Extent3d) -> (TextureView, Vec<TextureView>) {
    profiling::scope!("shadow texture creation");

    let texture = device.create_texture(&TextureDescriptor {
        label: Some("shadow texture"),
        size,
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

    let layer_views: Vec<_> = (0..size.depth_or_array_layers)
        .map(|idx| {
            texture.create_view(&TextureViewDescriptor {
                label: Some(&format!("shadow texture layer {}", idx)),
                format: None,
                dimension: Some(TextureViewDimension::D2),
                aspect: TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: idx,
                array_layer_count: NonZeroU32::new(1),
            })
        })
        .collect();

    (primary_view, layer_views)
}

fn create_shadow_bgl(device: &Device) -> BindGroupLayout {
    profiling::scope!("shadow bgl creation");
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
    profiling::scope!("shadow bg creation");
    BindGroupBuilder::new()
        .append_buffer(buffer)
        .append_texture_view(view)
        .build(device, Some("shadow bg"), bgl)
}
