const BATCH_SIZE: usize = 256;
const WORKGROUP_SIZE: u32 = 64;

mod batching;
mod culler;
mod suballoc;

pub use batching::{ShaderBatchData, ShaderBatchDatas};
pub use culler::{CullingBufferMap, DrawCall, DrawCallSet, GpuCuller};
pub use suballoc::{InputOutputBuffer, InputOutputPartition};
