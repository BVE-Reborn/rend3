//! Easy to use, customizable, efficient 3D renderer library built on wgpu.
//!
//! Library is currently under heavy development. While the render routine api
//! will likely have signifgant changes, the `Renderer` api has stayed
//! very similar throughout development.
//!
//! To use rend3 add the following to your Cargo.toml:
//!
//! ```text
//! rend3 = "0.0.6"
//! ```
//!
//! # Examples
//!
//! Take a look at the [examples] for examples on how to use the api.
//!
//! [examples]: https://github.com/BVE-Reborn/rend3/tree/trunk/examples
//!
//! # Purpose
//!
//! `rend3` tries to fulfill the following usecases:
//!  1. Games and visualizations that need a customizable and efficient renderer.
//!  2. Small projects that just want to put objects on screen, but want lighting and effects.
//!  3. A small cog in a big machine: a renderer doesn't interfere with the rest of the program.
//!
//! `rend3` is not:
//!  1. A renderer for AAA games. AAA games have requirements far beyond any possible indie game and would be unreasonable to target.
//!  2. A framework or engine. It does not include all the parts needed to make an advanced game or simulation nor care how you structure
//!     your program. I do have plans for a `rend3-util` (or similar) crate that is a very basic framework for the second use case listed above.
//!
//! # Future Plans
//!
//! I have grand plans for this library. An overview can be found in the issue tracker
//! under the [enhancement] label.
//!
//! [enhancement]: https://github.com/BVE-Reborn/rend3/labels/enhancement

mod renderer;
pub mod resources {
    mod camera;
    mod directional;
    mod material;
    mod mesh;
    mod object;
    mod texture;

    pub use camera::*;
    pub use directional::*;
    pub use material::*;
    pub use mesh::*;
    pub use object::*;
    pub use texture::*;
}
pub mod util {
    pub mod bind_merge;
    pub mod buffer;
    pub mod frustum;
    pub mod math;
    pub mod output;
    pub mod registry;
    pub mod typedefs;
}

mod builder;
mod instruction;
mod mode;
mod options;
mod routine;
mod statistics;

pub use builder::*;
pub use mode::*;
pub use options::*;
pub use rend3_types as types;
pub use renderer::{error::*, Renderer};
pub use routine::*;
pub use statistics::*;

pub const INTERNAL_SHADOW_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
// This needs to be dynamic
pub const SHADOW_DIMENSIONS: u32 = 2048;
