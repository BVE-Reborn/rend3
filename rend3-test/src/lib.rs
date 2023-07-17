mod helpers;
mod runner;

#[cfg(not(target_arch = "wasm32"))]
pub use tokio::test as test_attr;
#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen_test::wasm_bindgen_test as test_attr;

pub use runner::TestRunner;
