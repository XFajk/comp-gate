//! This module defines the IOAPI protocol for communicating between comp-gate applications

use std::{net::SocketAddr, ops::Deref, rc::Rc};

#[cfg(target_family = "windows")]
pub const CONNECTION_FILE_PATH: &'static str = r"C:\Users\Rudolf Vrbensky\comp-gate.txt";

#[cfg(target_family = "unix")]
pub const CONNECTION_FILE_PATH: &'static str = "/tmp/comp-gate.txt";

#[derive(Debug, Clone)]
#[repr(u8)]
pub enum IoApiCommand {
    GetDeviceList = 2,
    DisableDevice(Rc<str>) = 3,
    EnableDevice(Rc<str>) = 4,
    GetDeviceConnectionLogs = 5,
}

impl IoApiCommand {
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

    fn try_from(cmd_tokens: &[&str]) -> Result<Self, Self::Error> {
        match cmd_tokens[0] {
            "get_device_list" => Ok(IoApiCommand::GetDeviceList),
            "disable_device" => Ok(IoApiCommand::DisableDevice(Rc::from(cmd_tokens[1]))),
            "enable_device" => Ok(IoApiCommand::EnableDevice(Rc::from(cmd_tokens[1]))),
            "get_device_connection_logs" => Ok(IoApiCommand::GetDeviceConnectionLogs),
            _ => Err(()),
        }
    }
}

impl TryFrom<(u8, Vec<Rc<str>>)> for IoApiCommand {
    type Error = ();

    fn try_from((code, args): (u8, Vec<Rc<str>>)) -> Result<Self, Self::Error> {
        match code {
            2 => Ok(IoApiCommand::GetDeviceList),
            3 => Ok(IoApiCommand::DisableDevice(args[0].clone())),
            4 => Ok(IoApiCommand::EnableDevice(args[0].clone())),
            5 => Ok(IoApiCommand::GetDeviceConnectionLogs),
            _ => Err(()),
        }
    }
}

pub struct IoApiRequest(Rc<[u8]>);

impl From<IoApiCommand> for IoApiRequest {
    fn from(value: IoApiCommand) -> Self {
        let cmd_code = value.cmd_code();

        let result_bytes = match value {
            IoApiCommand::GetDeviceList | IoApiCommand::GetDeviceConnectionLogs => vec![cmd_code],
            IoApiCommand::DisableDevice(id) | IoApiCommand::EnableDevice(id) => vec![cmd_code]
                .into_iter()
                .chain(id.as_bytes().to_vec())
                .collect(),
        };

        Self(result_bytes.into())
    }
}

impl Deref for IoApiRequest {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub fn get_core_connection_addr() -> anyhow::Result<SocketAddr> {
    let addr_line: Box<str> = std::fs::read_to_string(CONNECTION_FILE_PATH)?
        .trim()
        .split("\n")
        .collect::<Vec<&str>>()[0]
        .into();

    let addr_parts = addr_line.split(":").collect::<Vec<&str>>();
    let ip = addr_parts[0];
    let port = addr_parts[1].parse::<u16>()?;

    Ok(SocketAddr::new(ip.parse()?, port))
}
