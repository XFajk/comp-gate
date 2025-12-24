//! # Core Service Binary
//!
//! This binary acts as the central daemon for the `comp-gate` system.
//! It is responsible for:
//!
//! - **Device Monitoring**: Continuously listening for USB device insertion and removal events.
//! - **Device Management**: Maintaining an in-memory tree of connected devices (`DeviceTracker`).
//! - **Access Control**: Enforcing a whitelist policy to automatically disable unauthorized devices (WIP).
//! - **Inter-Process Communication (IPC)**: Hosting a TCP server (IOAPI) to allow external tools (like the CLI or GUI shell) to query device status and issue commands.
//!
//! ## Architecture
//!
//! The core service runs a single-threaded event loop that polls for two types of events:
//! 1. **Network Events**: New TCP connections or incoming data on existing connections.
//! 2. **System Events**: USB hardware changes detected by the `UsbConnectionCallbacksHandle`.
//!
//! ## Usage
//!
//! This binary is intended to be run as a background service (daemon) with administrative privileges,
//! as it requires access to the Windows SetupAPI to enable/disable drivers.

use std::{
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    rc::Rc,
    sync::mpsc::TryRecvError,
};

use anyhow::Result;

use comp_gate::{helper::ioapi::connection_file_path, *};
use error::PollEventError;
use helper::{
    device_managment::{DeviceTracker, device_path_to_device_id},
    ioapi::IoApiCommand,
    usb_connection_callback::{UsbConnectionCallbacksHandle, UsbConnectionEvent},
    whitelist::Whitelist,
};

// TODO list of tasks to implement:
// - [#] Implement device tracking functionality
// - [#] Implement device blocking functionality
// - [#] Implement whitelist functionality
// - [_] Combine last three points into a Whitelist/Blacklist system
// - [_] Implement GUI using egui around the core functionality

/// The main entry point for the Core service.
///
/// It performs the following initialization steps:
/// 1. Binds a TCP listener to a random local port for the IOAPI.
/// 2. Writes the connection address to a known file path so clients can find it.
/// 3. Loads the initial state of connected USB/HID devices.
/// 4. Initializes the whitelist system.
/// 5. Starts the background thread for USB event monitoring.
///
/// Then it enters the main event loop.
fn main() -> Result<()> {
    // IO API stuff
    let ioapi_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    ioapi_listener.set_nonblocking(true)?;
    println!(
        "Application IO API on address: {}",
        ioapi_listener.local_addr()?
    );
    std::fs::write(
        connection_file_path(),
        ioapi_listener.local_addr()?.to_string(),
    )?;

    let mut ioapi_connections: Vec<TcpStream> = vec![];

    // Device Tracker stuff
    let device_tracker = DeviceTracker::load()?;
    println!("{}", device_tracker);

    let mut whitelist = Whitelist::new(device_tracker)?;

    let callback_handle = UsbConnectionCallbacksHandle::setup_connection_callbacks()?;

    let mut device_connection_logs: Vec<Box<str>> = vec![];

    loop {
        // IO API logic
        handle_new_ioapi_connection(&ioapi_listener, &mut ioapi_connections);

        let mut closed_connections = Vec::new();
        for (index, connection) in ioapi_connections.iter_mut().enumerate() {
            // Read message length (first 4 bytes)
            let mut length_buf = [0u8; 4];
            match connection.read_exact(&mut length_buf) {
                Ok(_) => {
                    let message_length = u32::from_be_bytes(length_buf) as usize;
                    println!("recving a packet of size {}", message_length);

                    handle_ioapi_message(message_length);

                    let cmd = parse_cmd_message(connection, message_length);
                    let cmd = if cmd.is_some() {
                        println!("Command parsed successfully: {:?}", cmd);
                        cmd.unwrap()
                    } else {
                        println!("Error parsing command message");
                        continue;
                    };

                    match cmd {
                        IoApiCommand::GetDeviceList => {
                            let payload = convert_bytes_to_payload(
                                whitelist.device_tracker.to_string().as_bytes(),
                            );

                            connection.write_all(&payload).unwrap_or_else(|err| {
                                println!("Error writing to IO API connection: {}", err);
                            });
                        }
                        IoApiCommand::GetDeviceConnectionLogs => {
                            let mut core_payload = vec![0u8; 1024];
                            for log in device_connection_logs.iter() {
                                core_payload.extend_from_slice(&log.as_bytes());
                                core_payload.push(b'\n');
                            }

                            connection
                                .write_all(&convert_bytes_to_payload(&core_payload))
                                .unwrap_or_else(|err| {
                                    println!("Error writing to IO API connection: {}", err);
                                });
                        }
                        IoApiCommand::EnableDevice(device_id) => {
                            println!("Enabling device: {}", device_id);
                            let payload = if let Err(e) = whitelist.device_tracker.set_device_state(
                                &device_id,
                                helper::device_managment::DeviceState::Enable,
                            ) {
                                convert_bytes_to_payload(
                                    format!("Enabling device failed: {}", e).as_bytes(),
                                )
                            } else {
                                convert_bytes_to_payload(b"Device enabled.")
                            };

                            connection
                                .write_all(&convert_bytes_to_payload(&payload))
                                .unwrap_or_else(|err| {
                                    println!("Error writing to IO API connection: {}", err);
                                });
                        }
                        IoApiCommand::DisableDevice(device_id) => {
                            println!("Disabling device: {}", device_id);
                            let payload = if let Err(e) = whitelist.device_tracker.set_device_state(
                                &device_id,
                                helper::device_managment::DeviceState::Disable,
                            ) {
                                convert_bytes_to_payload(
                                    format!("Disabling device failed: {}", e).as_bytes(),
                                )
                            } else {
                                convert_bytes_to_payload(b"Device disabled.")
                            };

                            connection
                                .write_all(&convert_bytes_to_payload(&payload))
                                .unwrap_or_else(|err| {
                                    println!("Error writing to IO API connection: {}", err);
                                });
                        }
                    }
                }
                Err(e) if e.kind() != std::io::ErrorKind::WouldBlock => {
                    closed_connections.push(index);
                    println!("Error reading from IO API connection: {}, {}", e, e.kind());
                }
                _ => {}
            }
        }

        for index in closed_connections {
            ioapi_connections.remove(index);
        }

        // Device Tracking logic
        match callback_handle.poll_events() {
            Ok(event) => match event {
                UsbConnectionEvent::Connected(device_path) => {
                    let device_id = device_path_to_device_id(&device_path);

                    let log = format!("USB Device connected: {}", device_id);
                    println!("{}", log);
                    device_connection_logs.push(log.into_boxed_str());

                    match whitelist.device_tracker.insert_device_by_id(&device_id) {
                        Ok(_) => {
                            println!("- Device inserted into tracker");
                            println!(
                                "- Current device tracker state:\n{}",
                                whitelist.device_tracker
                            );
                        }
                        Err(e) => println!("- Error inserting device into tracker: {}", e),
                    }
                }
                UsbConnectionEvent::Disconnected(device_path) => {
                    let device_id = device_path_to_device_id(&device_path);

                    let log = format!("USB Device disconnected: {}", device_id);
                    println!("{}", log);
                    device_connection_logs.push(log.into_boxed_str());

                    match whitelist.device_tracker.remove_device_by_id(&device_id) {
                        None => {
                            println!("- Device removed from tracker");
                            println!(
                                "- Current device tracker state:\n{}",
                                whitelist.device_tracker
                            );
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

/// Accepts new incoming TCP connections on the IOAPI listener.
///
/// This function is non-blocking. It accepts all currently pending connections
/// and adds them to the `connections` vector.
///
/// # Arguments
///
/// * `listener` - The bound TCP listener.
/// * `connections` - The vector to store active connections.
fn handle_new_ioapi_connection(listener: &TcpListener, connections: &mut Vec<TcpStream>) {
    loop {
        match listener.accept() {
            Ok((tcp_connection, _addr)) => {
                connections.push(tcp_connection);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No more pending connections right now
                break;
            }
            Err(e) => {
                println!("Error accepting IO API connection: {}", e);
                break;
            }
        }
    }
}

/// Parses a raw byte message from a TCP stream into an `IoApiCommand`.
///
/// # Arguments
///
/// * `connection` - The TCP stream to read from.
/// * `message_length` - The expected length of the message payload.
///
/// # Returns
///
/// * `Some(IoApiCommand)` - If parsing is successful.
/// * `None` - If reading fails or the command is invalid.
fn parse_cmd_message(connection: &mut TcpStream, message_length: usize) -> Option<IoApiCommand> {
    let mut message_buf = vec![0u8; message_length];
    // TODO WARING: logical BUG if the read_exact return would block this code bugs out everything
    if let Ok(_) = connection.read_exact(&mut message_buf) {
        if message_buf.len() >= 1 {
            let command_code = message_buf[0];

            let args_data = &message_buf[1..];
            let args_str = String::from_utf8_lossy(args_data);
            let arguments: Vec<Rc<str>> = args_str.split(" ").map(Rc::from).collect();
            return IoApiCommand::try_from((command_code, arguments)).ok();
        }
    }
    None
}

/// Helper function to wrap a byte slice into a length-prefixed payload.
///
/// The format is `[4 bytes length (Big Endian)][payload]`.
fn convert_bytes_to_payload(bytes: &[u8]) -> Box<[u8]> {
    let length_prefix = (bytes.len() as u32).to_be_bytes();
    [&length_prefix, bytes].concat().into_boxed_slice()
}

fn handle_ioapi_message(connection: &mut TcpStream, message_length: usize) {
    let cmd = parse_cmd_message(connection, message_length);
    let cmd = if cmd.is_some() {
        println!("Command parsed successfully: {:?}", cmd);
        cmd.unwrap()
    } else {
        println!("Error parsing command message");
        continue;
    };

    match cmd {
        IoApiCommand::GetDeviceList => {
            let payload = convert_bytes_to_payload(whitelist.device_tracker.to_string().as_bytes());

            connection.write_all(&payload).unwrap_or_else(|err| {
                println!("Error writing to IO API connection: {}", err);
            });
        }
        IoApiCommand::GetDeviceConnectionLogs => {
            let mut core_payload = vec![0u8; 1024];
            for log in device_connection_logs.iter() {
                core_payload.extend_from_slice(&log.as_bytes());
                core_payload.push(b'\n');
            }

            connection
                .write_all(&convert_bytes_to_payload(&core_payload))
                .unwrap_or_else(|err| {
                    println!("Error writing to IO API connection: {}", err);
                });
        }
        IoApiCommand::EnableDevice(device_id) => {
            println!("Enabling device: {}", device_id);
            let payload = if let Err(e) = whitelist
                .device_tracker
                .set_device_state(&device_id, helper::device_managment::DeviceState::Enable)
            {
                convert_bytes_to_payload(format!("Enabling device failed: {}", e).as_bytes())
            } else {
                convert_bytes_to_payload(b"Device enabled.")
            };

            connection
                .write_all(&convert_bytes_to_payload(&payload))
                .unwrap_or_else(|err| {
                    println!("Error writing to IO API connection: {}", err);
                });
        }
        IoApiCommand::DisableDevice(device_id) => {
            println!("Disabling device: {}", device_id);
            let payload = if let Err(e) = whitelist
                .device_tracker
                .set_device_state(&device_id, helper::device_managment::DeviceState::Disable)
            {
                convert_bytes_to_payload(format!("Disabling device failed: {}", e).as_bytes())
            } else {
                convert_bytes_to_payload(b"Device disabled.")
            };

            connection
                .write_all(&convert_bytes_to_payload(&payload))
                .unwrap_or_else(|err| {
                    println!("Error writing to IO API connection: {}", err);
                });
        }
    }
}
