use std::rc::Rc;

#[derive(Debug, Clone)]
pub enum IoApiCommand {
    GetDeviceList,
    DisableDevice(Rc<str>),
    EnableDevice(Rc<str>),
    GetDeviceConnectionLogs,
}

impl From<(u8, Vec<Rc<str>>)> for IoApiCommand {
    fn from((code, args): (u8, Vec<Rc<str>>)) -> Self {
        match code {
            2 => IoApiCommand::GetDeviceList,
            3 => IoApiCommand::DisableDevice(args[0].clone()),
            4 => IoApiCommand::EnableDevice(args[0].clone()),
            5 => IoApiCommand::GetDeviceConnectionLogs,
            _ => panic!("Invalid command code"), // TODO: Handle invalid command code gracefully
        }
    }
}
