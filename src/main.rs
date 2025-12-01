mod error;
mod helper;

use anyhow::{Ok, Result};
use helper::device_managment::DeviceTracker;
use helper::whitelist::Whitelist;

use crate::{error::PollEventError, helper::usb_connection_callback::{UsbConnectionCallbacksHandle, UsbConnectionEvent}};

// TODO list of tasks to implement:
// - [#] Implement device tracking functionality
// - [#] Implement device blocking functionality
// - [_] Combine last two points into a Whitelist/Blacklist system
// - [_] Implement GUI using egui around the core functionality

fn main() -> Result<()> {
    let device_tracker = DeviceTracker::load()?;
    for (_, device) in device_tracker.devices.iter() {
        println!("{}", device);
    }

    let callback_handle = UsbConnectionCallbacksHandle::setup_connection_callbacks()?;

    loop {
        match callback_handle.poll_events() {
            Ok(event) => match event {
                UsbConnectionEvent::Connected(device_name) => {
                    println!("USB Device connected: {:?}", device_name);
                }
                UsbConnectionEvent::Disconnected(device_name) => {
                    println!("USB Device disconnected: {:?}", device_name);
                }
            },
            Err(e) => {
                match e {
                    PollEventError::ThreadFinished => {
                        println!("USB connection callback thread has finished");
                        break;
                    }
                    _ => {
                        return Err(e.into())
                    }
                }
            }
        }
    }

    Ok(())
}
