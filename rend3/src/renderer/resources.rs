use crate::{
    resources::CameraManager,
    types::{Camera, TextureHandle},
    util::output::SURFACE_FORMAT,
    InternalSurfaceOptions, VSyncMode,
};
use glam::UVec2;
use wgpu::{Device, PresentMode, Surface, SurfaceConfiguration, TextureUsages};

pub struct RendererGlobalResources {
    pub camera: CameraManager,
    pub background_texture: Option<TextureHandle>,
}
impl RendererGlobalResources {
    pub fn new(device: &Device, surface: Option<&Surface>, options: &InternalSurfaceOptions) -> Self {
        if let Some(surface) = surface {
            configure_surface(device, surface, options.size, options.vsync)
        }

        let camera = CameraManager::new(Camera::default(), Some(options.aspect_ratio()));

        Self {
            camera,
            background_texture: None,
        }
    }

    pub fn update(
        &mut self,
        device: &Device,
        surface: Option<&Surface>,
        old_options: &mut InternalSurfaceOptions,
        new_options: InternalSurfaceOptions,
    ) {
        let dirty = determine_dirty(old_options, &new_options);

        if dirty.contains(DirtyResources::SWAPCHAIN) {
            if let Some(surface) = surface {
                configure_surface(device, surface, new_options.size, new_options.vsync)
            }
        }
        if dirty.contains(DirtyResources::CAMERA) {
            self.camera.set_aspect_ratio(Some(new_options.aspect_ratio()));
        }

        *old_options = new_options
    }
}

bitflags::bitflags! {
    struct DirtyResources: u8 {
        const SWAPCHAIN = 0x01;
        const CAMERA = 0x02;
    }
}

fn determine_dirty(current: &InternalSurfaceOptions, new: &InternalSurfaceOptions) -> DirtyResources {
    let mut dirty = DirtyResources::empty();

    if current.size != new.size {
        dirty |= DirtyResources::SWAPCHAIN;
        dirty |= DirtyResources::CAMERA;
    }

    if current.vsync != new.vsync {
        dirty |= DirtyResources::SWAPCHAIN;
    }

    dirty
}

fn configure_surface(device: &Device, surface: &Surface, size: UVec2, vsync: VSyncMode) {
    surface.configure(
        device,
        &SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: SURFACE_FORMAT,
            width: size.x,
            height: size.y,
            present_mode: match vsync {
                VSyncMode::On => PresentMode::Mailbox,
                VSyncMode::Off => PresentMode::Immediate,
            },
        },
    )
}
