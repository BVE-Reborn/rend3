const BATCH_SIZE: usize = 256;
const WORKGROUP_SIZE: u32 = 64;

mod batching;
mod culler;
mod suballoc;

pub use batching::{ShaderBatchData, ShaderBatchDatas};
pub use culler::{DrawCall, DrawCallSet, GpuCuller};
