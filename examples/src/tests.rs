use std::{path::Path, sync::Arc};

use anyhow::Context;
use glam::UVec2;
use rend3_framework::{App, DefaultRoutines, Mutex, RedrawContext, SetupContext};
use rend3_test::{compare_image_to_path, download_image};

pub struct TestConfiguration<A> {
    pub app: A,
    pub reference_path: &'static str,
    pub size: UVec2,
    pub threshold_set: rend3_test::ThresholdSet,
}

#[allow(clippy::await_holding_lock)] // false positive
pub async fn test_app<A: App<T>, T: 'static>(mut config: TestConfiguration<A>) -> anyhow::Result<()> {
    config.app.register_logger();
    config.app.register_panic_hook();

    let iad =
        rend3_test::no_gpu_return!(config.app.create_iad().await).context("InstanceAdapterDevice creation failed")?;

    let renderer =
        rend3::Renderer::new(iad.clone(), A::HANDEDNESS, Some(config.size.x as f32 / config.size.y as f32)).unwrap();

    let mut spp = rend3::ShaderPreProcessor::new();
    rend3_routine::builtin_shaders(&mut spp);

    let base_rendergraph = config.app.create_base_rendergraph(&renderer, &spp);
    let mut data_core = renderer.data_core.lock();
    let routines = Arc::new(DefaultRoutines {
        pbr: Mutex::new(rend3_routine::pbr::PbrRoutine::new(
            &renderer,
            &mut data_core,
            &spp,
            &base_rendergraph.interfaces,
        )),
        skybox: Mutex::new(rend3_routine::skybox::SkyboxRoutine::new(&renderer, &spp, &base_rendergraph.interfaces)),
        tonemapping: Mutex::new(rend3_routine::tonemapping::TonemappingRoutine::new(
            &renderer,
            &spp,
            &base_rendergraph.interfaces,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )),
    });
    drop(data_core);

    let surface_format = wgpu::TextureFormat::Rgba8UnormSrgb;
    config.app.setup(SetupContext {
        windowing: None,
        renderer: &renderer,
        routines: &routines,
        surface_format,
        resolution: config.size,
        scale_factor: 1.0,
    });

    let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Render Texture"),
        size: wgpu::Extent3d { width: config.size.x, height: config.size.y, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: surface_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    config.app.handle_redraw(RedrawContext {
        window: None,
        renderer: &renderer,
        routines: &routines,
        base_rendergraph: &base_rendergraph,
        surface_texture: &texture,
        resolution: config.size,
        control_flow: &mut |_| unreachable!(),
        event_loop_window_target: None,
        delta_t_seconds: 0.0,
    });

    let image = download_image(&renderer, texture, config.size).await.unwrap();

    compare_image_to_path(&image, Path::new(config.reference_path), config.threshold_set)
        .context(config.reference_path) // Location of reference image
        .unwrap();

    Ok(())
}
