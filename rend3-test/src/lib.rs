mod helpers;
mod runner;

pub use runner::TestRunner;

#[macro_export]
macro_rules! no_gpu_return {
    ($value:expr) => {
        match $value {
            Ok(value) => Ok(value),
            Err(rend3::RendererInitializationError::MissingAdapter) => return Ok(()),
            Err(err) => Err(err),
        }
    };
}

// These always need to go last in the file, or RA gets mightily confused.
#[cfg(not(target_arch = "wasm32"))]
pub use tokio::test as test_attr;
#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen_test::wasm_bindgen_test as test_attr;
