use crate::{renderer::SWAPCHAIN_FORMAT, VSyncMode};
use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, Device, PresentMode, ShaderStage,
    Surface, SwapChain, SwapChainDescriptor, TextureUsage,
};
use winit::dpi::PhysicalSize;

pub fn create_swapchain(device: &Device, surface: &Surface, size: PhysicalSize<u32>, vsync: VSyncMode) -> SwapChain {
    device.create_swap_chain(
        &surface,
        &SwapChainDescriptor {
            width: size.width,
            height: size.height,
            usage: TextureUsage::OUTPUT_ATTACHMENT,
            format: SWAPCHAIN_FORMAT,
            present_mode: match vsync {
                VSyncMode::On => PresentMode::Fifo,
                VSyncMode::Off => PresentMode::Mailbox,
            },
        },
    )
}

pub fn create_object_input_bgl(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("object input bgl"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStage::COMPUTE,
            ty: BindingType::StorageBuffer {
                dynamic: false,
                min_binding_size: None,
                readonly: true,
            },
            count: None,
        }],
    })
}

pub fn create_object_output_bgl(device: &Device) -> BindGroupLayout {
    let entry = BindGroupLayoutEntry {
        binding: 0,
        visibility: ShaderStage::COMPUTE,
        ty: BindingType::StorageBuffer {
            dynamic: false,
            min_binding_size: None,
            readonly: false,
        },
        count: None,
    };

    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("object output bgl"),
        entries: &[
            entry.clone(),
            BindGroupLayoutEntry {
                binding: 1,
                ..entry.clone()
            },
            BindGroupLayoutEntry { binding: 2, ..entry },
        ],
    })
}

pub fn create_uniform_bgl(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("uniform bgl"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStage::COMPUTE,
            ty: BindingType::UniformBuffer {
                dynamic: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}
