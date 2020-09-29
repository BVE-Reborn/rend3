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

pub mod datatypes;
mod instruction;
mod options;
mod registry;
mod renderer;
mod statistics;

pub use options::*;
pub use renderer::{error::*, Renderer};
pub use statistics::*;
