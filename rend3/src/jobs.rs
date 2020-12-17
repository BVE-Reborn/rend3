#[derive(Debug, Copy, Clone)]
pub struct JobPriorities {
    pub compute_pool: u8,

    pub buffer_recall_priority: u32,
    pub main_task_priority: u32,
    pub culling_priority: u32,
    pub render_record_priority: u32,
    pub pipeline_build_priority: u32,
}
impl Default for JobPriorities {
    fn default() -> Self {
        Self {
            compute_pool: 0,

            buffer_recall_priority: 0,
            main_task_priority: 1,
            culling_priority: 2,
            render_record_priority: 3,
            pipeline_build_priority: 4,
        }
    }
}
