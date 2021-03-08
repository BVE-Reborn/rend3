//! Easy to use, customizable, efficient 3D renderer library built on wgpu.
//!
//! Library is currently under heavy development and the api will rapidly change
//! as things are factored. While it's still in development, rend3 is able to be
//! used to build programs.
//!
//! Rend3 is not currently release on crates.io, to use it add the following
//! to your Cargo.toml:
//!
//! ```text
//! rend3 = "0.0.4"
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

#[macro_export]
macro_rules! span {
    ($guard_name:tt, $level:ident, $name:expr, $($fields:tt)*) => {
        let span = tracing::span!(tracing::Level::$level, $name, $($fields)*);
        let $guard_name = span.enter();
    };
    ($guard_name:tt, $level:ident, $name:expr) => {
        let span = tracing::span!(tracing::Level::$level, $name);
        let $guard_name = span.enter();
    };
}

#[macro_export]
macro_rules! span_transfer {
    (_ -> $guard_name:tt, $level:ident, $name:expr, $($fields:tt)*) => {
        let span = tracing::span!(tracing::Level::$level, $name, $($fields)*);
        #[allow(unused_variables)]
        let $guard_name = span.enter();
    };
    (_ -> $guard_name:tt, $level:ident, $name:expr) => {
        let span = tracing::span!(tracing::Level::$level, $name);
        #[allow(unused_variables)]
        let $guard_name = span.enter();
    };
    ($old_guard:tt -> _) => {
        drop($old_guard);
    };
    ($old_guard:tt -> $guard_name:tt, $level:ident, $name:expr, $($fields:tt)*) => {
        drop($old_guard);
        let span = tracing::span!(tracing::Level::$level, $name, $($fields)*)
        #[allow(unused_variables)]
        let $guard_name = span.enter();
    };
    ($old_guard:tt -> $guard_name:tt, $level:ident, $name:expr) => {
        drop($old_guard);
        let span = tracing::span!(tracing::Level::$level, $name);
        #[allow(unused_variables)]
        let $guard_name = span.enter();
    };
}

mod bind_merge;
mod builder;
pub mod datatypes;
mod instruction;
mod jobs;
pub mod list;
mod mode;
mod options;
mod output;
mod registry;
mod renderer;
mod statistics;

pub use builder::*;
pub use jobs::*;
pub use mode::*;
pub use options::*;
pub use output::*;
pub use renderer::{error::*, Renderer};
pub use statistics::*;
