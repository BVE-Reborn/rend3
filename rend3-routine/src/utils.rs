use std::convert::TryFrom;

use glam::UVec2;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RenderTextureOptions {
    pub resolution: UVec2,
    pub samples: SampleCount,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum SampleCount {
    One = 1,
    Four = 4,
}

impl Default for SampleCount {
    fn default() -> Self {
        Self::One
    }
}

impl TryFrom<u8> for SampleCount {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            1 => Self::One,
            4 => Self::Four,
            v => return Err(v),
        })
    }
}
