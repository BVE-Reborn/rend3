/// Determines if the renderer is using cpu-driven rendering, or faster gpu-driven
/// rendering.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RendererProfile {
    CpuDriven,
    GpuDriven,
}

impl RendererProfile {
    /// Turns a RendererMode into a [`ProfileData`] calling the appropriate
    /// initalization function.
    pub fn into_data<C, G>(self, cpu: impl FnOnce() -> C, gpu: impl FnOnce() -> G) -> ProfileData<C, G> {
        match self {
            Self::CpuDriven => ProfileData::Cpu(cpu()),
            Self::GpuDriven => ProfileData::Gpu(gpu()),
        }
    }
}

/// Stores two different types of data depending on the renderer mode.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProfileData<C, G> {
    Cpu(C),
    Gpu(G),
}
#[allow(dead_code)] // Even if these are unused, don't warn
impl<C, G> ProfileData<C, G> {
    pub fn profile(&self) -> RendererProfile {
        match self {
            Self::Cpu(_) => RendererProfile::CpuDriven,
            Self::Gpu(_) => RendererProfile::GpuDriven,
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

    pub fn as_cpu_only_ref(&self) -> ProfileData<&C, ()> {
        match self {
            Self::Cpu(c) => ProfileData::Cpu(c),
            Self::Gpu(_) => ProfileData::Gpu(()),
        }
    }

    pub fn as_cpu_only_mut(&mut self) -> ProfileData<&mut C, ()> {
        match self {
            Self::Cpu(c) => ProfileData::Cpu(c),
            Self::Gpu(_) => ProfileData::Gpu(()),
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

    pub fn as_gpu_only_ref(&self) -> ProfileData<(), &G> {
        match self {
            Self::Gpu(g) => ProfileData::Gpu(g),
            Self::Cpu(_) => ProfileData::Cpu(()),
        }
    }

    pub fn as_gpu_only_mut(&mut self) -> ProfileData<(), &mut G> {
        match self {
            Self::Gpu(g) => ProfileData::Gpu(g),
            Self::Cpu(_) => ProfileData::Cpu(()),
        }
    }

    pub fn as_ref(&self) -> ProfileData<&C, &G> {
        match self {
            Self::Cpu(c) => ProfileData::Cpu(c),
            Self::Gpu(c) => ProfileData::Gpu(c),
        }
    }

    pub fn as_ref_mut(&mut self) -> ProfileData<&mut C, &mut G> {
        match self {
            Self::Cpu(c) => ProfileData::Cpu(c),
            Self::Gpu(c) => ProfileData::Gpu(c),
        }
    }

    pub fn map_cpu<C2>(self, func: impl FnOnce(C) -> C2) -> ProfileData<C2, G> {
        match self {
            Self::Cpu(c) => ProfileData::Cpu(func(c)),
            Self::Gpu(g) => ProfileData::Gpu(g),
        }
    }

    pub fn map_gpu<G2>(self, func: impl FnOnce(G) -> G2) -> ProfileData<C, G2> {
        match self {
            Self::Cpu(c) => ProfileData::Cpu(c),
            Self::Gpu(g) => ProfileData::Gpu(func(g)),
        }
    }

    pub fn map<C2, G2>(self, cpu_func: impl FnOnce(C) -> C2, gpu_func: impl FnOnce(G) -> G2) -> ProfileData<C2, G2> {
        match self {
            Self::Cpu(c) => ProfileData::Cpu(cpu_func(c)),
            Self::Gpu(g) => ProfileData::Gpu(gpu_func(g)),
        }
    }
}

impl<T> ProfileData<T, T> {
    pub fn into_common(self) -> T {
        match self {
            Self::Cpu(c) => c,
            Self::Gpu(g) => g,
        }
    }
}
