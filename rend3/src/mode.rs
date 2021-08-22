#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RendererMode {
    CPUPowered,
    GPUPowered,
}

impl RendererMode {
    pub fn into_data<C, G>(self, cpu: impl FnOnce() -> C, gpu: impl FnOnce() -> G) -> ModeData<C, G> {
        match self {
            Self::CPUPowered => ModeData::CPU(cpu()),
            Self::GPUPowered => ModeData::GPU(gpu()),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModeData<C, G> {
    CPU(C),
    GPU(G),
}
#[allow(dead_code)] // Even if these are unused, don't warn
impl<C, G> ModeData<C, G> {
    pub fn mode(&self) -> RendererMode {
        match self {
            Self::CPU(_) => RendererMode::CPUPowered,
            Self::GPU(_) => RendererMode::GPUPowered,
        }
    }

    pub fn into_cpu(self) -> C {
        match self {
            Self::CPU(c) => c,
            Self::GPU(_) => panic!("tried to extract cpu data in gpu mode"),
        }
    }

    pub fn as_cpu(&self) -> &C {
        match self {
            Self::CPU(c) => c,
            Self::GPU(_) => panic!("tried to extract cpu data in gpu mode"),
        }
    }

    pub fn as_cpu_mut(&mut self) -> &mut C {
        match self {
            Self::CPU(c) => c,
            Self::GPU(_) => panic!("tried to extract cpu data in gpu mode"),
        }
    }

    pub fn into_gpu(self) -> G {
        match self {
            Self::GPU(g) => g,
            Self::CPU(_) => panic!("tried to extract gpu data in cpu mode"),
        }
    }

    pub fn as_gpu(&self) -> &G {
        match self {
            Self::GPU(g) => g,
            Self::CPU(_) => panic!("tried to extract gpu data in cpu mode"),
        }
    }

    pub fn as_gpu_mut(&mut self) -> &mut G {
        match self {
            Self::GPU(g) => g,
            Self::CPU(_) => panic!("tried to extract gpu data in cpu mode"),
        }
    }

    pub fn as_ref(&self) -> ModeData<&C, &G> {
        match self {
            Self::CPU(c) => ModeData::CPU(c),
            Self::GPU(c) => ModeData::GPU(c),
        }
    }

    pub fn as_ref_mut(&mut self) -> ModeData<&mut C, &mut G> {
        match self {
            Self::CPU(c) => ModeData::CPU(c),
            Self::GPU(c) => ModeData::GPU(c),
        }
    }

    pub fn map_cpu<C2>(self, func: impl FnOnce(C) -> C2) -> ModeData<C2, G> {
        match self {
            Self::CPU(c) => ModeData::CPU(func(c)),
            Self::GPU(g) => ModeData::GPU(g),
        }
    }

    pub fn map_gpu<G2>(self, func: impl FnOnce(G) -> G2) -> ModeData<C, G2> {
        match self {
            Self::CPU(c) => ModeData::CPU(c),
            Self::GPU(g) => ModeData::GPU(func(g)),
        }
    }

    pub fn map<C2, G2>(self, cpu_func: impl FnOnce(C) -> C2, gpu_func: impl FnOnce(G) -> G2) -> ModeData<C2, G2> {
        match self {
            Self::CPU(c) => ModeData::CPU(cpu_func(c)),
            Self::GPU(g) => ModeData::GPU(gpu_func(g)),
        }
    }
}

impl<T> ModeData<T, T> {
    pub fn into_common(self) -> T {
        match self {
            Self::CPU(c) => c,
            Self::GPU(g) => g,
        }
    }
}
