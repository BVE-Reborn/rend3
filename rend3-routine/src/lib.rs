#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]

//! Render Routines for the rend3 3D renderer library.
//!
//! The routines in this crate provide powerful default routines as well as
//! building blocks for writing your own custom render routines.
//!
//! # Getting Started
//!
//! The starting point when using this crate is
//! [`BaseRenderGraph`](base::BaseRenderGraph), which provides a
//! fully-put-together rendergraph including the PBR impl, skybox renderer, and
//! tonemapper.
//!
//! As you reach for more customization, you can copy
//! [`BaseRenderGraph::add_to_graph`](base::BaseRenderGraph::add_to_graph) into
//! your own code and adding/modifying the routine to your hearts content. The
//! abstraction is designed to be easily replaced and extended without needing
//! too much user side boilerplate.

pub mod base;
pub mod common;
pub mod culling;
pub mod forward;
pub mod hi_z;
pub mod pbr;
mod shaders;
pub mod skinning;
pub mod skybox;
pub mod tonemapping;
pub mod uniforms;

pub use shaders::builtin_shaders;
