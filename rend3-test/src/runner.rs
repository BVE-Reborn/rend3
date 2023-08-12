#![cfg_attr(target_arch = "wasm32", allow(unused))] // While there's no wasm comparisons

use std::{fs::create_dir_all, ops::Deref, path::Path, sync::Arc};

use anyhow::{bail, ensure, Context, Result};
use glam::UVec2;
use image::buffer::ConvertBuffer;
use rend3::{
    types::{Handedness, SampleCount},
    Renderer,
};
use rend3_routine::{base::BaseRenderGraph, pbr::PbrRoutine, tonemapping::TonemappingRoutine};
use wgpu::{
    Extent3d, ImageCopyBuffer, ImageDataLayout, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};

pub struct FrameRenderSettings {
    size: u32,
    samples: SampleCount,
}

impl FrameRenderSettings {
    pub fn new() -> Self {
        Self {
            size: 64,
            samples: SampleCount::One,
        }
    }

    pub fn size(mut self, size: u32) -> Result<Self> {
        ensure!(size % 64 == 0, "Size must be a multiple of 64, is {}", size);
        self.size = size;
        Ok(self)
    }

    pub fn samples(mut self, samples: SampleCount) -> Self {
        self.samples = samples;
        self
    }
}

impl Default for FrameRenderSettings {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
pub struct TestRunnerBuilder {
    handness: Option<Handedness>,
    iad: Option<rend3::InstanceAdapterDevice>,
}

impl TestRunnerBuilder {
    pub fn new() -> Self {
        TestRunnerBuilder::default()
    }

    pub fn handedness(mut self, handness: Handedness) -> Self {
        self.handness = Some(handness);
        self
    }

    pub fn iad(mut self, iad: rend3::InstanceAdapterDevice) -> Self {
        self.iad = Some(iad);
        self
    }

    pub async fn build(self) -> Result<TestRunner> {
        let iad = match self.iad {
            Some(iad) => iad,
            None => rend3::create_iad(None, None, None, None)
                .await
                .context("InstanceAdapterDevice creation failed")?,
        };

        let renderer = rend3::Renderer::new(iad, self.handness.unwrap_or(Handedness::Left), None)
            .context("Renderer initialization failed")?;
        let mut spp = rend3::ShaderPreProcessor::new();
        rend3_routine::builtin_shaders(&mut spp);

        let base_rendergraph = BaseRenderGraph::new(&renderer, &spp);

        let pbr = PbrRoutine::new(
            &renderer,
            &mut renderer.data_core.lock(),
            &spp,
            &base_rendergraph.interfaces,
        );
        let tonemapping = TonemappingRoutine::new(
            &renderer,
            &spp,
            &base_rendergraph.interfaces,
            TextureFormat::Rgba8UnormSrgb,
        );

        Ok(TestRunner {
            renderer,
            pbr,
            tonemapping,
            base_rendergraph,
        })
    }
}

pub struct TestRunner {
    pub renderer: Arc<Renderer>,
    pub pbr: PbrRoutine,
    pub tonemapping: TonemappingRoutine,
    pub base_rendergraph: BaseRenderGraph,
}

impl Deref for TestRunner {
    type Target = Arc<Renderer>;

    fn deref(&self) -> &Self::Target {
        &self.renderer
    }
}

impl TestRunner {
    pub fn builder() -> TestRunnerBuilder {
        TestRunnerBuilder::new()
    }

    pub async fn render_frame(&self, settings: FrameRenderSettings) -> Result<image::RgbaImage> {
        let buffer = self.renderer.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Test output buffer"),
            size: (settings.size * settings.size * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let texture = self.renderer.device.create_texture(&TextureDescriptor {
            label: Some("Test output image"),
            size: Extent3d {
                width: settings.size,
                height: settings.size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::COPY_SRC | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        self.renderer.swap_instruction_buffers();

        let mut eval_output = self.renderer.evaluate_instructions();

        let mut graph = rend3::graph::RenderGraph::new();
        let frame_handle = graph.add_imported_render_target(
            &texture,
            0..1,
            rend3::graph::ViewportRect::from_size(UVec2::splat(settings.size)),
        );

        self.base_rendergraph.add_to_graph(
            &mut graph,
            &eval_output,
            &self.pbr,
            None,
            &self.tonemapping,
            frame_handle,
            UVec2::splat(settings.size),
            settings.samples,
            glam::Vec4::ZERO,
            glam::Vec4::ZERO,
        );

        graph.execute(&self.renderer, &mut eval_output);

        let mut encoder = self
            .renderer
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Test output encoder"),
            });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            ImageCopyBuffer {
                buffer: &buffer,
                layout: ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(settings.size * 4),
                    rows_per_image: None,
                },
            },
            Extent3d {
                width: settings.size,
                height: settings.size,
                depth_or_array_layers: 1,
            },
        );

        let submit_index = self.renderer.queue.submit(Some(encoder.finish()));

        let (sender, receiver) = flume::bounded(1);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |_| sender.send(()).unwrap());
        self.renderer
            .device
            .poll(wgpu::Maintain::WaitForSubmissionIndex(submit_index));

        receiver
            .recv_async()
            .await
            .context("Failed to recieve message from map_async")?;

        let mapping = buffer.slice(..).get_mapped_range();

        image::RgbaImage::from_raw(settings.size, settings.size, mapping.to_vec())
            .context("Failed to create image from mapping")
    }

    pub fn compare_image_to_path(&self, test_rgba: &image::RgbaImage, path: &Path, threshold: f32) -> Result<()> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let parent_path = path.parent().context("Path given had no parent")?;
            let Ok(expected) = image::open(path) else {
                create_dir_all(parent_path).context("Could not create parent directory")?;
                test_rgba.save(path).context("Could not save image")?;
                return Ok(())
            };

            let expected_rgb = expected.into_rgb8();
            let test_rgb: image::RgbImage = test_rgba.convert();

            let expected_flip =
                nv_flip::FlipImageRgb8::with_data(expected_rgb.width(), expected_rgb.height(), &expected_rgb);
            let test_flip = nv_flip::FlipImageRgb8::with_data(test_rgb.width(), test_rgb.height(), &test_rgb);

            let result_float = nv_flip::flip(expected_flip, test_flip, nv_flip::DEFAULT_PIXELS_PER_DEGREE);

            let magma = result_float.apply_color_lut(&nv_flip::magma_lut());

            let magma_image = image::RgbImage::from_raw(magma.width(), magma.height(), magma.to_vec())
                .context("Failed to create image from magma image")?;

            let mut pool = nv_flip::FlipPool::from_image(&result_float);

            let mean: f32 = pool.mean();

            let pass = mean <= threshold;

            println!("Image Comparison Results: {}", if pass { "passed" } else { "failed" });
            println!("    Mean: {}", pool.mean());
            println!("     Min: {}", pool.min_value());
            println!("     25%: {}", pool.get_percentile(0.25, true));
            println!("     50%: {}", pool.get_percentile(0.50, true));
            println!("     75%: {}", pool.get_percentile(0.75, true));
            println!("     95%: {}", pool.get_percentile(0.95, true));
            println!("     99%: {}", pool.get_percentile(0.99, true));
            println!("     Max: {}", pool.max_value());

            let filename = path.file_stem().unwrap();

            let diff_path = parent_path.join(format!("{}-diff.png", filename.to_string_lossy()));
            let success_path = parent_path.join(format!("{}-success.png", filename.to_string_lossy()));
            let failure_path = parent_path.join(format!("{}-failure.png", filename.to_string_lossy()));

            magma_image.save(&diff_path).context("Could not save diff image")?;

            if pass {
                test_rgba.save(&success_path).context("Could not save success image")?;
            } else {
                test_rgba.save(&failure_path).context("Could not save failure image")?;
                bail!("Image comparison failed");
            }
        }

        Ok(())
    }

    pub async fn render_and_compare(
        &self,
        settings: FrameRenderSettings,
        path: impl AsRef<Path>,
        threshold: f32,
    ) -> Result<()> {
        let test_rgba = self.render_frame(settings).await?;

        self.compare_image_to_path(&test_rgba, path.as_ref(), threshold)
    }
}
