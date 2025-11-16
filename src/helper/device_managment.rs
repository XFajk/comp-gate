/// This file holds the functions related to device management
/// such as listing connected devices, ejecting devices, etc.
use std::{
    ptr::{null, null_mut},
    rc::Rc,
};
use windows_sys::{Win32::Devices::DeviceAndDriverInstallation::*, Win32::Foundation::*};
use crate::error::Win32Error;

pub struct Device {
    pub device_instance_id: Rc<str>,
}

impl Device {
    fn from_bare_devinfo(devinfo: SP_DEVINFO_DATA, devinfoset: HDEVINFO) -> Result<Self, Win32Error> {
        let mut buffer: [u16; 512] = [0; 512];
        let mut required_size: u32 = 0;
        let get_id_result = unsafe {
            SetupDiGetDeviceInstanceIdW(
                devinfoset,
                &devinfo as *const SP_DEVINFO_DATA,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut required_size as *mut u32,
            )
        } == TRUE;

        if !get_id_result {
            Err(unsafe { GetLastError().into() })
        } else {
            let len = if required_size == 0 {
                buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len())
            } else {
                (required_size as usize).saturating_sub(1)
            };
            let device_instance_id: Rc<str> =
                String::from_utf16_lossy(&buffer[..len]).into();
            Ok(Device { device_instance_id })
        }
    }
}

pub struct DeviceTracker {
    device_information_set: HDEVINFO,
    pub devices: Vec<Device>,
}

impl Drop for DeviceTracker {
    fn drop(&mut self) {
        if self.device_information_set == INVALID_HANDLE_VALUE as HDEVINFO {
            return;
        }
        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(self.device_information_set);
        }
    }
}

impl DeviceTracker {
    pub fn load() -> Result<Self, Win32Error> {
        let device_information_set: HDEVINFO = unsafe {
            SetupDiGetClassDevsA(
                null(),
                c"USB".as_ptr() as *const u8,
                null_mut(),
                DIGCF_ALLCLASSES | DIGCF_PRESENT,
            )
        };

        if device_information_set == INVALID_HANDLE_VALUE as HDEVINFO {
            return Err(unsafe { GetLastError().into() });
        }

        Ok(DeviceTracker {
            device_information_set,
            devices: Self::get_listed_devices(device_information_set)?,
        })
    }

    fn get_listed_devices(devinfoset: HDEVINFO) -> Result<Vec<Device>, Win32Error> {
        let mut devices: Vec<Device> = Vec::new();
        let mut index: u32 = 0;

        loop {
            unsafe {
                let mut device_data: SP_DEVINFO_DATA = std::mem::zeroed();
                device_data.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
                let operation_result = SetupDiEnumDeviceInfo(
                    devinfoset,
                    index,
                    &mut device_data as *mut SP_DEVINFO_DATA,
                ) == TRUE;

                if operation_result {
                    println!("- Device found at index: {}", index);
                    devices.push(Device::from_bare_devinfo(device_data, devinfoset)?);
                    index += 1;
                } else {
                    let error = GetLastError();
                    if error == ERROR_NO_MORE_ITEMS {
                        break;
                    } else {
                        return Err(error.into());
                    }
                }
            }
        }
        println!("Total devices found: {}", devices.len());
        Ok(devices)
    }
}
