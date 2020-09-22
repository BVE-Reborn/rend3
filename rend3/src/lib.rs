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

pub mod datatypes;
mod instruction;
mod options;
mod registry;
mod renderer;
mod statistics;
mod tls;

pub use options::*;
pub use renderer::{error::*, Renderer};
pub use statistics::*;
pub use tls::TLS;
