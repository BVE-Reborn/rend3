use include_dir::{include_dir, Dir};

pub const SPIRV_SHADERS: Dir = include_dir!("$CARGO_MANIFEST_DIR/shaders/spirv");
pub const WGSL_SHADERS: Dir = include_dir!("$CARGO_MANIFEST_DIR/shaders/wgsl");
