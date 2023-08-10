use core::fmt::Debug;

use alloc::{collections::VecDeque, sync::Arc, vec::Vec};
use log::{debug, error};

use super::{device_info::PciDeviceId, PciCommonDevice};

pub trait PciDevice: Sync + Send + Debug {
    fn device_id(&self) -> PciDeviceId;
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PciDriverProbeError {
    DeviceNotMatch,
    ConfigurationSpaceError,
}

/// PCI device driver, PCI bus will pass the device through the `probe` function when a new device is registered.
pub trait PciDriver: Sync + Send + Debug {
    /// Probe an unclaimed PCI device.
    ///
    /// If the driver matches and succeeds in initializing the unclaimed device,
    /// then the driver will return an claimed instance of the device,
    /// signaling that the PCI device is now ready to work.
    ///
    /// Once a device is matched and claimed by a driver,
    /// it won't be fed to another driver for probing.
    fn probe(
        &self,
        device: PciCommonDevice,
    ) -> Result<Arc<dyn PciDevice>, (PciDriverProbeError, PciCommonDevice)>;
}

/// The PCI bus used to register PCI devices. If a component wishes to drive a PCI device, it needs to provide the following:
///
/// 1. The structure that implements the PciDevice trait.
/// 2. PCI driver.
pub struct PciBus {
    common_devices: VecDeque<PciCommonDevice>,
    devices: Vec<Arc<dyn PciDevice>>,
    drivers: Vec<Arc<dyn PciDriver>>,
}

impl PciBus {
    pub fn register_driver(&mut self, driver: Arc<dyn PciDriver>) {
        debug!("Register driver:{:#x?}", driver);
        let length = self.common_devices.len();
        for i in (0..length).rev() {
            let common_device = self.common_devices.pop_front().unwrap();
            let device_id = *common_device.device_id();
            let device = match driver.probe(common_device) {
                Ok(device) => {
                    debug_assert!(device_id == device.device_id());
                    self.devices.push(device);
                    continue;
                }
                Err((err, common_device)) => {
                    if err != PciDriverProbeError::DeviceNotMatch {
                        error!("PCI device construction failed, reason: {:?}", err);
                    }
                    debug_assert!(device_id == *common_device.device_id());
                    common_device
                }
            };
            self.common_devices.push_back(device);
        }
        self.drivers.push(driver);
    }

    pub(super) fn register_common_device(&mut self, mut common_device: PciCommonDevice) {
        debug!("Find pci common devices:{:x?}", common_device);
        let device_id = common_device.device_id().clone();
        for driver in self.drivers.iter() {
            common_device = match driver.probe(common_device) {
                Ok(device) => {
                    debug_assert!(device_id == device.device_id());
                    self.devices.push(device);
                    return;
                }
                Err((err, common_device)) => {
                    if err != PciDriverProbeError::DeviceNotMatch {
                        error!("PCI device construction failed, reason: {:?}", err);
                    }
                    debug_assert!(device_id == *common_device.device_id());
                    common_device
                }
            };
        }
        self.common_devices.push_back(common_device);
    }

    pub(super) const fn new() -> Self {
        Self {
            common_devices: VecDeque::new(),
            devices: Vec::new(),
            drivers: Vec::new(),
        }
    }
}
