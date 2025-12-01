use keyring::Entry;

use crate::helper::{device_managment::DeviceTracker, whitelist};

pub struct Whitelist {
    entry: Entry,
    devices: DeviceTracker,
}

impl Whitelist {
    pub fn new(device_tracker: DeviceTracker) -> anyhow::Result<Self> {
        let entry = Entry::new("comp-gate.xfajk", "device_whitelist")?;
        Ok(Whitelist {
            entry,
            devices: device_tracker,
        })
    }
}
