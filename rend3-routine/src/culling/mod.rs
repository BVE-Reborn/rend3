const BATCH_SIZE: usize = 256;
const WORKGROUP_SIZE: u32 = 256;

mod batching;
mod culler;

pub use batching::{ShaderBatchDatas, ShaderBatchData};
pub use culler::{DrawCall, DrawCallSet, GpuCuller};
