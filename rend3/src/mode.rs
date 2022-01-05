/// Determines if a more-compatible CPU driven rendering, or faster GPU driven
/// rendering.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RendererMode {
    CpuPowered,
    GpuPowered,
}

impl RendererMode {
    /// Turns a RendererMode into a [`ModeData`] calling the appropriate
    /// initalization function.
    pub fn into_data<C, G>(self, cpu: impl FnOnce() -> C, gpu: impl FnOnce() -> G) -> ModeData<C, G> {
        match self {
            Self::CpuPowered => ModeData::Cpu(cpu()),
            Self::GpuPowered => ModeData::Gpu(gpu()),
        }
    }
}

/// Stores two different types of data depending on the renderer mode.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModeData<C, G> {
    Cpu(C),
    Gpu(G),
}
#[allow(dead_code)] // Even if these are unused, don't warn
impl<C, G> ModeData<C, G> {
    pub fn mode(&self) -> RendererMode {
        match self {
            Self::Cpu(_) => RendererMode::CpuPowered,
            Self::Gpu(_) => RendererMode::GpuPowered,
        }
    }

    pub fn into_cpu(self) -> C {
        match self {
            Self::Cpu(c) => c,
            Self::Gpu(_) => panic!("tried to extract cpu data in gpu mode"),
        }
    }

    pub fn as_cpu(&self) -> &C {
        match self {
            Self::Cpu(c) => c,
            Self::Gpu(_) => panic!("tried to extract cpu data in gpu mode"),
        }
    }

    pub fn as_cpu_mut(&mut self) -> &mut C {
        match self {
            Self::Cpu(c) => c,
            Self::Gpu(_) => panic!("tried to extract cpu data in gpu mode"),
        }
    }

    pub fn as_cpu_only_ref(&self) -> ModeData<&C, ()> {
        match self {
            Self::Cpu(c) => ModeData::Cpu(c),
            Self::Gpu(_) => ModeData::Gpu(()),
        }
    }

    pub fn as_cpu_only_mut(&mut self) -> ModeData<&mut C, ()> {
        match self {
            Self::Cpu(c) => ModeData::Cpu(c),
            Self::Gpu(_) => ModeData::Gpu(()),
        }
    }

    pub fn into_gpu(self) -> G {
        match self {
            Self::Gpu(g) => g,
            Self::Cpu(_) => panic!("tried to extract gpu data in cpu mode"),
        }
    }

    pub fn as_gpu(&self) -> &G {
        match self {
            Self::Gpu(g) => g,
            Self::Cpu(_) => panic!("tried to extract gpu data in cpu mode"),
        }
    }

    pub fn as_gpu_mut(&mut self) -> &mut G {
        match self {
            Self::Gpu(g) => g,
            Self::Cpu(_) => panic!("tried to extract gpu data in cpu mode"),
        }
    }

    pub fn as_gpu_only_ref(&self) -> ModeData<(), &G> {
        match self {
            Self::Gpu(g) => ModeData::Gpu(g),
            Self::Cpu(_) => ModeData::Cpu(()),
        }
    }

    pub fn as_gpu_only_mut(&mut self) -> ModeData<(), &mut G> {
        match self {
            Self::Gpu(g) => ModeData::Gpu(g),
            Self::Cpu(_) => ModeData::Cpu(()),
        }
    }

    pub fn as_ref(&self) -> ModeData<&C, &G> {
        match self {
            Self::Cpu(c) => ModeData::Cpu(c),
            Self::Gpu(c) => ModeData::Gpu(c),
        }
    }

    pub fn as_ref_mut(&mut self) -> ModeData<&mut C, &mut G> {
        match self {
            Self::Cpu(c) => ModeData::Cpu(c),
            Self::Gpu(c) => ModeData::Gpu(c),
        }
    }

    pub fn map_cpu<C2>(self, func: impl FnOnce(C) -> C2) -> ModeData<C2, G> {
        match self {
            Self::Cpu(c) => ModeData::Cpu(func(c)),
            Self::Gpu(g) => ModeData::Gpu(g),
        }
    }

    pub fn map_gpu<G2>(self, func: impl FnOnce(G) -> G2) -> ModeData<C, G2> {
        match self {
            Self::Cpu(c) => ModeData::Cpu(c),
            Self::Gpu(g) => ModeData::Gpu(func(g)),
        }
    }

    pub fn map<C2, G2>(self, cpu_func: impl FnOnce(C) -> C2, gpu_func: impl FnOnce(G) -> G2) -> ModeData<C2, G2> {
        match self {
            Self::Cpu(c) => ModeData::Cpu(cpu_func(c)),
            Self::Gpu(g) => ModeData::Gpu(gpu_func(g)),
        }
    }
}

impl<T> ModeData<T, T> {
    pub fn into_common(self) -> T {
        match self {
            Self::Cpu(c) => c,
            Self::Gpu(g) => g,
        }
    }
}
