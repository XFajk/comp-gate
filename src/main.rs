mod helper;
mod error;

use helper::device_managment::DeviceTracker;
use anyhow::Result;

fn main() -> Result<()> {
    let device_tracker = DeviceTracker::load()?;
    for device in device_tracker.devices.iter() {
        println!("Device Instance ID: {}", device);
    }
    Ok(())
}
