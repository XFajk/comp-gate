//! # IOAPI Module
//!
//! This module defines the IOAPI protocol used for inter-process communication (IPC)
//! between different components of the `comp-gate` system (e.g., the core service and the shell/GUI).
//!
//! It handles:
//! - Defining the command structure (`IoApiCommand`).
//! - Serializing commands into byte requests (`IoApiRequest`).
//! - Locating the connection address for the core service.

use std::{net::SocketAddr, ops::Deref, path::PathBuf, rc::Rc};

use crate::helper::device_managment::DeviceId;

/// Returns a per-user OS temporary directory path for the connection file.
///
/// This uses `std::env::temp_dir()` which maps to:
/// - On Unix: typically `/tmp`
/// - On Windows: `%TEMP%` / `%TMP%`
/// - On macOS: typically `/var/folders/.../T` or similar
///
/// Using the OS temporary directory avoids hardcoding usernames or absolute paths
/// that won't exist on other users' machines.
pub fn connection_file_path() -> PathBuf {
    std::env::temp_dir().join("comp-gate.txt")
}

/// Represents the available commands in the IOAPI protocol.
///
/// Each variant corresponds to a specific action that can be requested from the core service.
#[derive(Debug, Clone)]
#[repr(u8)]
pub enum IoApiCommand {
    /// Request a list of all connected devices.
    GetDeviceList = 2,
    /// Request to disable a specific device by its ID.
    DisableDevice(DeviceId) = 3,
    /// Request to enable a specific device by its ID.
    EnableDevice(DeviceId) = 4,
    /// Request the logs of device connection events.
    GetDeviceConnectionLogs = 5,
}

impl IoApiCommand {
    /// Returns the numeric operation code associated with the command.
    fn cmd_code(&self) -> u8 {
        match self {
            Self::GetDeviceList => 2,
            Self::GetDeviceConnectionLogs => 5,
            Self::DisableDevice(_) => 3,
            Self::EnableDevice(_) => 4,
        }
    }
}

impl TryFrom<&[&str]> for IoApiCommand {
    type Error = ();

    /// Tries to parse a command from a slice of string tokens.
    ///
    /// This is useful for parsing command-line arguments or text-based input.
    ///
    /// # Example
    ///
    /// ```rust
    /// use comp_gate::helper::ioapi::IoApiCommand;
    /// use std::rc::Rc;
    ///
    /// let tokens = ["disable_device", "USB\\VID_1234&PID_5678"];
    /// let cmd = IoApiCommand::try_from(&tokens[..]).unwrap();
    ///
    /// if let IoApiCommand::DisableDevice(id) = cmd {
    ///     assert_eq!(id.as_ref(), "USB\\VID_1234&PID_5678");
    /// }
    /// ```
    fn try_from(cmd_tokens: &[&str]) -> Result<Self, Self::Error> {
        match cmd_tokens[0] {
            "list" => Ok(IoApiCommand::GetDeviceList),
            "disable" => Ok(IoApiCommand::DisableDevice(DeviceId::from(
                Rc::<str>::from(cmd_tokens[1]),
            ))),
            "enable" => Ok(IoApiCommand::EnableDevice(DeviceId::from(Rc::<str>::from(
                cmd_tokens[1],
            )))),
            "logs" => Ok(IoApiCommand::GetDeviceConnectionLogs),
            _ => Err(()),
        }
    }
}

impl TryFrom<(u8, Vec<Rc<str>>)> for IoApiCommand {
    type Error = ();

    /// Tries to reconstruct a command from a raw opcode and a list of arguments.
    fn try_from((code, args): (u8, Vec<Rc<str>>)) -> Result<Self, Self::Error> {
        match code {
            2 => Ok(IoApiCommand::GetDeviceList),
            3 => Ok(IoApiCommand::DisableDevice(args[0].clone().into())),
            4 => Ok(IoApiCommand::EnableDevice(args[0].clone().into())),
            5 => Ok(IoApiCommand::GetDeviceConnectionLogs),
            _ => Err(()),
        }
    }
}

/// A serialized request ready to be sent over the network.
///
/// This struct wraps the raw byte representation of an `IoApiCommand`.
pub struct IoApiRequest(Rc<[u8]>);

impl From<IoApiCommand> for IoApiRequest {
    /// Converts an `IoApiCommand` into a serialized `IoApiRequest`.
    ///
    /// The serialization format is generally `[opcode, payload...]`.
    fn from(value: IoApiCommand) -> Self {
        let cmd_code = value.cmd_code();

        let result_bytes = match value {
            IoApiCommand::GetDeviceList | IoApiCommand::GetDeviceConnectionLogs => vec![cmd_code],
            IoApiCommand::DisableDevice(id) | IoApiCommand::EnableDevice(id) => vec![cmd_code]
                .into_iter()
                .chain(id.as_bytes().to_vec())
                .collect(),
        };

        let prefix_length: u32 = result_bytes.len() as u32;
        let result_bytes: Vec<u8> = prefix_length
            .to_be_bytes()
            .into_iter()
            .chain(result_bytes.into_iter())
            .collect();

        Self(result_bytes.into())
    }
}

impl Deref for IoApiRequest {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Retrieves the socket address of the running core service.
///
/// This function reads the connection file (located in the OS temporary directory)
/// to find the IP and port where the core service is listening.
///
/// # Returns
///
/// * `Ok(SocketAddr)` - The address of the core service.
/// * `Err(anyhow::Error)` - If the file cannot be read or parsed.
pub fn get_core_connection_addr() -> anyhow::Result<SocketAddr> {
    let path = connection_file_path();
    let content = std::fs::read_to_string(&path)?;
    let first_line = content
        .lines()
        .find(|l| !l.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("Connection file is empty"))?
        .trim()
        .to_string();

    let mut parts = first_line.split(':');
    let ip_str = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Malformed address"))?;
    let port_str = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Malformed address"))?;
    let port = port_str.parse::<u16>()?;

    Ok(SocketAddr::new(ip_str.parse()?, port))
}
