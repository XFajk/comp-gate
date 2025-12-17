use std::{
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    rc::Rc,
    sync::mpsc::TryRecvError,
};

use anyhow::Result;

use comp_gate::{helper::ioapi::CONNECTION_FILE_PATH, *};
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

fn main() -> Result<()> {
    // IO API stuff
    let ioapi_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    ioapi_listener.set_nonblocking(true)?;
    println!(
        "Application IO API on address: {}",
        ioapi_listener.local_addr()?
    );
    std::fs::write(
        CONNECTION_FILE_PATH,
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
                        _ => {
                            let payload = convert_bytes_to_payload(b"Unknown command");

                            connection.write_all(&payload).unwrap_or_else(|err| {
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
                UsbConnectionEvent::Connected(device_name) => {
                    let device_id = device_path_to_device_id(&device_name);

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
                UsbConnectionEvent::Disconnected(device_name) => {
                    let device_id = device_path_to_device_id(&device_name);

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

fn parse_cmd_message(connection: &mut TcpStream, message_length: usize) -> Option<IoApiCommand> {
    let mut message_buf = vec![0u8; message_length];
    // TODO WARING: logical BUG if the read_exact return would block this code bugs out everything
    if let Ok(_) = connection.read_exact(&mut message_buf) {
        if message_buf.len() >= 1 {
            let command_code = message_buf[0];

            let args_data = &message_buf[1..];
            let args_str = String::from_utf8_lossy(args_data);
            let arguments: Vec<Rc<str>> = args_str.split(" ").map(Rc::from).collect();
            return Some(IoApiCommand::from((command_code, arguments)));
        }
    }
    None
}

fn convert_bytes_to_payload(bytes: &[u8]) -> Box<[u8]> {
    let length_prefix = (bytes.len() as u32).to_be_bytes();
    [&length_prefix, bytes].concat().into_boxed_slice()
}
