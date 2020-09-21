use crate::{renderer::SWAPCHAIN_FORMAT, VSyncMode};
use wgpu::{Device, PresentMode, Surface, SwapChain, SwapChainDescriptor, TextureUsage};
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
