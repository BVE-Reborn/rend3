//! Easy to use, customizable, efficient 3D renderer library built on wgpu.
//!
//! Library is under active development. While internals will likely change
//! quite a bit, the external api will only experience minor changes as features
//! are added.
//!
//! To use rend3 add the following to your Cargo.toml:
//!
//! ```text
//! rend3 = "0.2.0"
//! ```
//!
//! # Screenshots
//!
//! ![scifi-base](https://raw.githubusercontent.com/BVE-Reborn/rend3/trunk/examples/scene-viewer/scifi-base.jpg)
//! ![example](https://raw.githubusercontent.com/BVE-Reborn/rend3/trunk/examples/scene-viewer/screenshot.jpg)
//! ![bistro](https://raw.githubusercontent.com/BVE-Reborn/rend3/trunk/examples/scene-viewer/bistro.jpg)
//! ![emerald-square](https://raw.githubusercontent.com/BVE-Reborn/rend3/trunk/examples/scene-viewer/emerald-square.jpg)
//!
//! # Examples
//!
//! Take a look at the [examples] getting started with the api.
//!
//! [examples]: https://github.com/BVE-Reborn/rend3/tree/trunk/examples
//!
//! # Purpose
//!
//! `rend3` tries to fulfill the following usecases:
//!  1. Games and visualizations that need a customizable, and efficient
//! renderer.  2. Small projects that just want to put objects on screen, but
//! want lighting and effects.  3. A small cog in a big machine: a renderer
//! doesn't interfere with the rest of the program.
//!
//! `rend3` is not:
//!  1. A framework or engine. It does not include all the parts needed to make
//! an advanced game or simulation nor care how you structure     your program.
//! I do have plans for a `rend3-util` (or similar) crate that is a very basic
//! framework for the second use case listed above.
//!
//! # Helper Crates
//!
//! This is the primary crate which holds the main [`Renderer`] struct. We have
//! some other crates:
//! - `rend3-gltf`: contains code to load from a .gltf or .glb file.
//! - `rend3-routine`: contains render routines for drawing PBR-style objects.
//!
//! # GPU Culling
//!
//! On Vulkan and DX12 "gpu mode" is enabled by default, which uses modern
//! bindless resources and gpu-based culling. This reduces CPU load and allows
//! significantly more powerful culling.
//!
//! # Future Plans
//!
//! I have grand plans for this library. An overview can be found in the issue
//! tracker under the [enhancement] label.
//!
//! [enhancement]: https://github.com/BVE-Reborn/rend3/labels/enhancement
//!
//! # Matrix Chatroom
//!
//! We have a matrix chatroom that you can come and join if you want to chat
//! about using rend3 or developing it:
//!
//! [![Matrix](https://img.shields.io/static/v1?label=rend3%20dev&message=%23rend3&color=blueviolet&logo=matrix)](https://matrix.to/#/#rend3:matrix.org)
//! [![Matrix](https://img.shields.io/static/v1?label=rend3%20users&message=%23rend3-users&color=blueviolet&logo=matrix)](https://matrix.to/#/#rend3-users:matrix.org)
//!
//! If discord is more your style, our meta project has a channel which mirrors
//! the matrix rooms:
//!
//! [![Discord](https://img.shields.io/discord/451037457475960852?color=7289DA&label=discord)](https://discord.gg/mjxXTVzaDg)
//!
//! # Helping Out
//!
//! We welcome all contributions and ideas. If you want to participate or have
//! ideas for this library, we'd love to hear them!

mod renderer;
/// Managers for various type of resources.
pub mod managers {
    mod camera;
    mod directional;
    mod material;
    mod mesh;
    mod object;
    mod skeleton;
    mod texture;

    pub use camera::*;
    pub use directional::*;
    pub use material::*;
    pub use mesh::*;
    pub use object::*;
    pub use skeleton::*;
    pub use texture::*;
}

/// Reexport of [`rend3_types`] with some added wgpu re-exports.
pub mod types {
    pub use rend3_types::*;
    #[doc(inline)]
    pub use wgpu::{Surface, SurfaceError};
}
/// Utilities and isolated bits of functionality that need a home.
pub mod util {
    pub mod bind_merge;
    pub mod buffer;
    pub mod frustum;
    pub mod graph_texture_store;
    pub mod math;
    pub mod mipmap;
    pub mod output;
    pub mod registry {
        mod archetypical;
        mod basic;
        mod erased;

        pub use archetypical::*;
        pub use basic::*;
        pub use erased::*;
    }
    pub mod typedefs;
}

pub mod graph;
mod instruction;
mod mode;
mod setup;
mod surface;

pub use mode::*;
pub use renderer::{error::*, Renderer, RendererDataCore};
pub use setup::*;
pub use surface::*;

/// Format of all shadow maps.
pub const INTERNAL_SHADOW_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
// TODO: This needs to be dynamic
/// Resolution of all shadow maps.
pub const SHADOW_DIMENSIONS: u32 = 2048;
