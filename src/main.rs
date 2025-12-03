mod error;
mod helper;

use std::sync::mpsc::TryRecvError;

use anyhow::Result;
use helper::device_managment::DeviceTracker;

use crate::{
    error::PollEventError,
    helper::{
        device_managment::device_path_to_device_id,
        usb_connection_callback::{UsbConnectionCallbacksHandle, UsbConnectionEvent},
        whitelist::Whitelist,
    },
};

// TODO list of tasks to implement:
// - [#] Implement device tracking functionality
// - [#] Implement device blocking functionality
// - [#] Implement whitelist functionality
// - [_] Combine last three points into a Whitelist/Blacklist system
// - [_] Implement GUI using egui around the core functionality

fn main() -> Result<()> {
    let device_tracker = DeviceTracker::load()?;
    println!("{}", device_tracker);

    let mut whitelist = Whitelist::new(device_tracker)?;

    let callback_handle = UsbConnectionCallbacksHandle::setup_connection_callbacks()?;

    loop {
        match callback_handle.poll_events() {
            Ok(event) => match event {
                UsbConnectionEvent::Connected(device_name) => {
                    println!("USB Device connected: {:?}", device_name);
                    match whitelist
                        .device_tracker
                        .insert_device_by_id(&device_path_to_device_id(&device_name))
                    {
                        Ok(_) => {
                            println!("- Device inserted into tracker");
                            println!("- Current device tracker state:\n{}", whitelist.device_tracker);
                        }
                        Err(e) => println!("- Error inserting device into tracker: {}", e),
                    }
                }
                UsbConnectionEvent::Disconnected(device_name) => {
                    println!("USB Device disconnected: {:?}", device_name);
                    match whitelist
                        .device_tracker
                        .remove_device_by_id(&device_path_to_device_id(&device_name))
                    {
                        None => {
                            println!("- Device removed from tracker");
                            println!("- Current device tracker state:\n{}", whitelist.device_tracker);
                        }
                        Some(e) => println!("- Error removing device from tracker: {}", e),
                    }
                }
            },
            Err(e) => match e {
                PollEventError::ThreadFinished => {
                    println!("USB connection callback thread has finished");
                    break;
                }
                PollEventError::ThreadRecvError(TryRecvError::Empty) => {}
                _ => {
                    return Err(e.into());
                }
            },
        }
    }

    Ok(())
}
