mod helper;
mod error;

use helper::device_managment::DeviceTracker;
use anyhow::Result;

// TODO list of tasks to implement:
// - [#] Implement device tracking functionality
// - [_] Implement device blocking functionality
// - [_] Combine last two points into a Whitelist/Blacklist system
// - [_] Implement GUI using egui around the core functionality

fn main() -> Result<()> {
    let device_tracker = DeviceTracker::load()?;
    for device in device_tracker.devices.iter() {
        println!("Device Instance ID: {}", device);
    }
    Ok(())
}
