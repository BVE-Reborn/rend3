use std::sync::Arc;

use glam::{UVec2, Vec4};
use rend3::{ModeData, RenderRoutine, Renderer, types::TextureHandle, util::output::OutputFrame};
use wgpu::{
    Color, CommandBuffer, Device, Extent3d, LoadOp, Operations, RenderPassColorAttachment,
    RenderPassDepthStencilAttachment, RenderPassDescriptor, TextureDescriptor, TextureDimension, TextureFormat,
    TextureUsages, TextureView, TextureViewDescriptor,
};

pub mod common;
pub mod culling;
pub mod directional;
pub mod opaque;
pub mod shaders;
pub mod tonemapping;
pub mod uniforms;
pub mod vertex;

pub struct PbrRenderRoutine {
    pub interfaces: common::interfaces::ShaderInterfaces,
    pub samplers: common::samplers::Samplers,
    pub cpu_culler: culling::cpu::CpuCuller,
    pub gpu_culler: ModeData<(), culling::gpu::GpuCuller>,
    pub shadow_passes: directional::DirectionalShadowPass,
    pub opaque_pass: opaque::OpaquePass,
    pub tonemapping_pass: tonemapping::TonemappingPass,

    pub background_texture: Option<TextureHandle>,

    pub internal_buffer: TextureView,
    pub internal_depth_buffer: TextureView,
}

impl PbrRenderRoutine {
    pub fn new(renderer: &Renderer, resolution: UVec2) -> Self {
        let device = renderer.device();
        let mode = renderer.mode();
        let interfaces = common::interfaces::ShaderInterfaces::new(device);

        let samplers = common::samplers::Samplers::new(device, mode, &interfaces.samplers_bgl);

        let cpu_culler = culling::cpu::CpuCuller::new();
        let gpu_culler = mode.into_data(|| (), || culling::gpu::GpuCuller::new(device));

        let gpu_d2_texture_manager_guard = mode.into_data(|| (), || renderer.d2_texture_manager.read());
        let gpu_d2_texture_bgl = gpu_d2_texture_manager_guard
            .as_ref()
            .map(|_| (), |guard| guard.gpu_bgl());

        let directional_light_manager = &renderer.directional_light_manager.read();

        let colorless_depth_pipeline = Arc::new(common::depth_pass::build_depth_pass_shader(
            common::depth_pass::BuildDepthPassShaderArgs {
                mode,
                device,
                interfaces: &interfaces,
                texture_bgl: gpu_d2_texture_bgl,
                materials: &renderer.material_manager.read(),
                include_color: false,
            },
        ));
        let colored_depth_pipeline = Arc::new(common::depth_pass::build_depth_pass_shader(
            common::depth_pass::BuildDepthPassShaderArgs {
                mode,
                device,
                interfaces: &interfaces,
                texture_bgl: gpu_d2_texture_bgl,
                materials: &renderer.material_manager.read(),
                include_color: true,
            },
        ));
        let opaque_pipeline = Arc::new(common::opaque_pass::build_opaque_pass_shader(
            common::opaque_pass::BuildOpaquePassShaderArgs {
                mode,
                device,
                interfaces: &interfaces,
                directional_light_bgl: directional_light_manager.get_bgl(),
                texture_bgl: gpu_d2_texture_bgl,
                materials: &renderer.material_manager.read(),
            },
        ));
        let shadow_passes = directional::DirectionalShadowPass::new(Arc::clone(&colorless_depth_pipeline));
        let opaque_pass = opaque::OpaquePass::new(Arc::clone(&colored_depth_pipeline), Arc::clone(&opaque_pipeline));
        let tonemapping_pass = tonemapping::TonemappingPass::new(tonemapping::TonemappingPassNewArgs {
            device: &device,
            interfaces: &interfaces,
        });

        let internal_buffer = create_internal_buffer(device, resolution);
        let internal_depth_buffer = create_internal_depth_buffer(device, resolution);

        Self {
            interfaces,
            samplers,
            cpu_culler,
            gpu_culler,
            shadow_passes,
            opaque_pass,
            tonemapping_pass,

            background_texture: None,

            internal_buffer,
            internal_depth_buffer,
        }
    }

    pub fn set_background_texture(&mut self, handle: Option<TextureHandle>) {
        self.background_texture = handle;
    }

    pub fn resize(&mut self, device: &Device, resolution: UVec2) {
        self.internal_buffer = create_internal_buffer(device, resolution);
        self.internal_depth_buffer = create_internal_depth_buffer(device, resolution);
    }
}

impl RenderRoutine for PbrRenderRoutine {
    fn render(&self, renderer: Arc<Renderer>, encoders: &mut Vec<CommandBuffer>, frame: &OutputFrame) {
        let mut encoder = renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("primary encoder"),
        });

        let mesh_manager = renderer.mesh_manager.read();
        let mut directional_light = renderer.directional_light_manager.write();
        let mut materials = renderer.material_manager.write();
        let mut d2_textures = renderer.d2_texture_manager.write();

        directional_light.ready(&renderer.device, &renderer.queue);
        materials.ready(&renderer.device, &renderer.queue, &d2_textures);
        let d2_texture_output = d2_textures.ready(&renderer.device);
        let objects = renderer.object_manager.read().ready();

        let culler = self.gpu_culler.as_ref().map_cpu(|_| &self.cpu_culler);

        let culled_lights = self
            .shadow_passes
            .cull_shadows(directional::DirectionalShadowPassCullShadowsArgs {
                device: &renderer.device,
                encoder: &mut encoder,
                culler,
                materials: &materials,
                interfaces: &self.interfaces,
                lights: &directional_light,
                objects: &objects,
            });

        let global_resources = renderer.global_resources.read();

        let culled_objects = self.opaque_pass.cull_opaque(opaque::OpaquePassCullArgs {
            device: &renderer.device,
            encoder: &mut encoder,
            culler,
            materials: &materials,
            interfaces: &self.interfaces,
            camera: &global_resources.camera,
            objects: &objects,
        });

        let d2_texture_output_bg_ref = d2_texture_output.bg.as_ref().map(|_| (), |a| &**a);

        self.shadow_passes
            .draw_culled_shadows(directional::DirectionalShadowPassDrawCulledShadowsArgs {
                encoder: &mut encoder,
                materials: &materials,
                meshes: mesh_manager.buffers(),
                samplers: &self.samplers,
                texture_bg: d2_texture_output_bg_ref,
                culled_lights: &culled_lights,
            });

        let primary_camera_uniform_bg = uniforms::create_shader_uniform(uniforms::CreateShaderUniformArgs {
            device: &renderer.device,
            camera: &global_resources.camera,
            interfaces: &self.interfaces,
            ambient: Vec4::new(0.0, 0.0, 0.0, 1.0),
        });

        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("primary renderpass"),
            color_attachments: &[RenderPassColorAttachment {
                view: &self.internal_buffer,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: true,
                },
            }],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &self.internal_depth_buffer,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(0.0),
                    store: true,
                }),
                stencil_ops: None,
            }),
        });

        self.opaque_pass.prepass(opaque::OpaquePassPrepassArgs {
            rpass: &mut rpass,
            materials: &materials,
            meshes: mesh_manager.buffers(),
            samplers: &self.samplers,
            texture_bg: d2_texture_output_bg_ref,
            culled_objects: &culled_objects,
        });

        self.opaque_pass.draw(opaque::OpaquePassDrawArgs {
            rpass: &mut rpass,
            materials: &materials,
            meshes: mesh_manager.buffers(),
            samplers: &self.samplers,
            directional_light_bg: directional_light.get_bg(),
            texture_bg: d2_texture_output_bg_ref,
            shader_uniform_bg: &primary_camera_uniform_bg,
            culled_objects: &culled_objects,
        });

        drop(rpass);

        self.tonemapping_pass.blit(tonemapping::TonemappingPassBlitArgs {
            device: &renderer.device,
            encoder: &mut encoder,
            interfaces: &self.interfaces,
            samplers: &self.samplers,
            source: &self.internal_buffer,
            target: frame.as_view(),
        });

        encoders.push(encoder.finish())
    }
}

fn create_internal_buffer(device: &Device, resolution: UVec2) -> TextureView {
    device
        .create_texture(&TextureDescriptor {
            label: Some("internal renderbuffer"),
            size: Extent3d {
                width: resolution.x,
                height: resolution.y,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba16Float,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC,
        })
        .create_view(&TextureViewDescriptor::default())
}

fn create_internal_depth_buffer(device: &Device, resolution: UVec2) -> TextureView {
    device
        .create_texture(&TextureDescriptor {
            label: Some("internal depth renderbuffer"),
            size: Extent3d {
                width: resolution.x,
                height: resolution.y,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Depth32Float,
            usage: TextureUsages::RENDER_ATTACHMENT,
        })
        .create_view(&TextureViewDescriptor::default())
}
