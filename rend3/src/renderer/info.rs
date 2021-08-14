use wgpu::{AdapterInfo, Backend, DeviceType};

#[derive(Clone, Debug, PartialEq)]
pub enum Vendor {
    NV,
    AMD,
    Intel,
    Microsoft,
    Broadcom,
    Qualcomm,
    Unknown(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExtendedAdapterInfo {
    /// Adapter name
    pub name: String,
    /// Vendor PCI id of the adapter
    pub vendor: Vendor,
    /// PCI id of the adapter
    pub device: usize,
    /// Type of device
    pub device_type: DeviceType,
    /// Backend used for device
    pub backend: Backend,
}
impl From<AdapterInfo> for ExtendedAdapterInfo {
    fn from(info: AdapterInfo) -> Self {
        Self {
            name: info.name,
            vendor: match info.vendor {
                0x1002 => Vendor::AMD,
                0x10DE => Vendor::NV,
                0x1414 => Vendor::Microsoft,
                0x14E4 => Vendor::Broadcom,
                0x5143 => Vendor::Qualcomm,
                0x8086 => Vendor::Intel,
                v => Vendor::Unknown(v),
            },
            device: info.device,
            device_type: info.device_type,
            backend: info.backend,
        }
    }
}
impl ExtendedAdapterInfo {
    pub fn subgroup_size(&self) -> u32 {
        match self.vendor {
            Vendor::Microsoft => 4,
            // TODO: buy a rpi4 and verify
            Vendor::Broadcom => 16,
            Vendor::Intel | Vendor::NV => 32,
            Vendor::AMD | Vendor::Qualcomm | Vendor::Unknown(_) => 64,
        }
    }
}
