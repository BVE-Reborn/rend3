use rend3_types::{Backend, DeviceType};
use wgpu::AdapterInfo;

/// Set of common GPU vendors.
#[derive(Clone, Debug, PartialEq)]
pub enum Vendor {
    Nv,
    Amd,
    Intel,
    Microsoft,
    Arm,
    Broadcom,
    Qualcomm,
    /// Don't recognize this vendor. This is the given PCI id.
    Unknown(usize),
}

/// Information about an adapter. Includes named PCI IDs for vendors.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtendedAdapterInfo {
    /// Adapter name
    pub name: String,
    /// Vendor/brand of adapter.
    pub vendor: Vendor,
    /// PCI id of the adapter.
    pub device: usize,
    /// Type of device.
    pub device_type: DeviceType,
    /// Backend used for device
    pub backend: Backend,
}
impl From<AdapterInfo> for ExtendedAdapterInfo {
    fn from(info: AdapterInfo) -> Self {
        Self {
            name: info.name,
            vendor: match info.vendor {
                0x1002 => Vendor::Amd,
                0x10DE => Vendor::Nv,
                0x13B5 => Vendor::Arm,
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
