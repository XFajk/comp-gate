use crate::error::Win32Error;
/// This file holds the functions related to device management
/// such as listing connected devices, ejecting devices, etc.
use std::{
    ops::Deref, panic, ptr::{null, null_mut}, rc::Rc
};
use windows_sys::Win32::{
    Devices::{
        DeviceAndDriverInstallation::*,
        Properties::{
            DEVPKEY_Device_Class, DEVPKEY_Device_DevType, DEVPKEY_Device_DeviceDesc, DEVPKEY_Device_FriendlyName, DEVPKEY_Device_Service, DEVPROP_MASK_TYPE, DEVPROP_TYPE_EMPTY, DEVPROP_TYPE_STRING, DEVPROPTYPE
        },
    },
    Foundation::*,
};

pub enum DeviceProperty {
    EmptyProperty, // May not be used but is here so that the enum has more that one variant
    StringProperty { data: String },
}

impl TryFrom<(&[u8], DEVPROPTYPE)> for DeviceProperty {
    // I decided on a () because there is only one place where this conversion can fail.
    // That means creating a whole seperate Error enum for this is redundent
    // and I will rather let the caller device how to handle failiure of this conversion
    type Error = ();

    fn try_from(value: (&[u8], DEVPROPTYPE)) -> Result<Self, Self::Error> {
        match value.1 & DEVPROP_MASK_TYPE {
            DEVPROP_TYPE_STRING => {
                let u16_slice: &[u16] = unsafe {
                    std::slice::from_raw_parts(
                        value.0.as_ptr() as *const u16,
                        value.0.len() / std::mem::size_of::<u16>(),
                    )
                };

                let len = u16_slice
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(u16_slice.len());

                Ok(DeviceProperty::StringProperty {
                    data: String::from_utf16_lossy(&u16_slice[..len]),
                })
            }
            DEVPROP_TYPE_EMPTY => Ok(DeviceProperty::EmptyProperty),
            _ => Err(()),
        }
    }
}

pub struct Device {
    devinfo: SP_DEVINFO_DATA,
    pub device_id: Rc<str>,
    pub sub_interface_devices: Vec<Box<Device>>,

    pub device_service: Option<Rc<str>>,
    pub device_class: Option<Rc<str>>,
    pub device_friendly_name: Option<Rc<str>>,
    pub device_type: Option<Rc<str>>,
    pub device_description: Option<Rc<str>>,
}

impl std::fmt::Display for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Device ID: {}", self.device_id)?;
        writeln!(
            f,
            " - Device Service: {}",
            self.device_service.as_deref().unwrap_or("None")
        )?;
        writeln!(
            f,
            " - Device Class: {}",
            self.device_class.as_deref().unwrap_or("None")
        )?;  
        writeln!(
            f,
            " - Device Friendly Name: {}",
            self.device_friendly_name.as_deref().unwrap_or("None")
        )?;
        writeln!(
            f,
            " - Device Type: {}",
            self.device_type.as_deref().unwrap_or("None")
        )?;
        writeln!(
            f,
            " - Device Description: {}",
            self.device_description.as_deref().unwrap_or("None")
        ) 
    }
}

impl Device {
    fn from_bare_devinfo(
        devinfo: SP_DEVINFO_DATA,
        devinfoset: HDEVINFO,
    ) -> Result<Self, Win32Error> {
        let device_id = Self::retrive_device_id(devinfo, devinfoset)?;

        let device_service = match unsafe {
            Self::retrive_string_property(devinfo, devinfoset, &DEVPKEY_Device_Service)
        } {
            Ok(prop) => Some(prop.to_lowercase().into()),
            Err(e) => {
                println!(
                    "Warning: Could not retrieve Device Service for Device ID {} because of an error: {:?}",
                    device_id, e
                );
                None
            }
        };

        let device_class = match unsafe {
            Self::retrive_string_property(devinfo, devinfoset, &DEVPKEY_Device_Class)
        } {
            Ok(prop) => Some(prop),
            Err(e) => {
                println!(
                    "Warning: Could not retrieve Device Class for Device ID {} because of an error: {:?}",
                    device_id, e
                );
                None
            }
        };

        let device_type = match unsafe {
            Self::retrive_string_property(devinfo, devinfoset, &DEVPKEY_Device_DevType)
        } {
            Ok(prop) => Some(prop),
            Err(e) => {
                println!(
                    "Warning: Could not retrieve Device Type for Device ID {} because of an error: {:?}",
                    device_id, e
                );
                None
            }
        };

        let device_description = unsafe {
            match Self::retrive_string_property(devinfo, devinfoset, &DEVPKEY_Device_DeviceDesc) {
                Ok(prop) => Some(prop),
                Err(e) => {
                    println!(
                        "Warning: Could not retrieve Device Description for Device ID {} because of an error: {:?}",
                        device_id, e
                    );
                    None
                }
            }
        };

        let device_friendly_name = unsafe {
            match Self::retrive_string_property(devinfo, devinfoset, &DEVPKEY_Device_FriendlyName) {
                Ok(prop) => Some(prop),
                Err(e) => {
                    println!(
                        "Warning: Could not retrieve Device Friendly Name for Device ID {} because of an error: {:?}",
                        device_id, e
                    );
                    None
                }
            }
        };

        Ok(Device {
            devinfo,
            device_id,
            sub_interface_devices: vec![],
            device_service,
            device_class,
            device_friendly_name,
            device_type,
            device_description,
        })
    }

    fn retrive_device_id(
        devinfo: SP_DEVINFO_DATA,
        devinfoset: HDEVINFO,
    ) -> Result<Rc<str>, Win32Error> {
        let mut buffer: Vec<u16> = vec![];
        let mut required_size: u32 = 0;

        // First call to get the required size
        unsafe {
            SetupDiGetDeviceInstanceIdW(
                devinfoset,
                &devinfo as *const SP_DEVINFO_DATA,
                null_mut(),
                0u32,
                &mut required_size as *mut u32,
            );
        }

        buffer.resize(required_size as usize, 0u16);
        let get_id_operation_result = unsafe {
            SetupDiGetDeviceInstanceIdW(
                devinfoset,
                &devinfo as *const SP_DEVINFO_DATA,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut required_size as *mut u32,
            )
        } == TRUE;

        if !get_id_operation_result {
            Err(unsafe { GetLastError().into() })
        } else {
            let len = if required_size == 0 {
                buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len())
            } else {
                (required_size as usize).saturating_sub(1)
            };
            let device_instance_id: Rc<str> = String::from_utf16_lossy(&buffer[..len]).into();
            Ok(device_instance_id)
        }
    }

    fn retrive_device_property(
        devinfo: SP_DEVINFO_DATA,
        devinfoset: HDEVINFO,
        property: &DEVPROPKEY,
    ) -> Result<(Vec<u8>, DEVPROPTYPE), Win32Error> {
        let mut buffer: Vec<u8> = vec![];
        let mut required_size: u32 = 0;
        let mut property_type: DEVPROPTYPE = 0;

        // First call to get the required size
        unsafe {
            SetupDiGetDevicePropertyW(
                devinfoset,
                &devinfo as *const SP_DEVINFO_DATA,
                property as *const _,
                &mut property_type as *mut DEVPROPTYPE,
                null_mut(),
                0,
                &mut required_size as *mut u32,
                0,
            );
        }

        buffer.resize(required_size as usize, 0u8);

        let get_type_operation_result = unsafe {
            SetupDiGetDevicePropertyW(
                devinfoset,
                &devinfo as *const SP_DEVINFO_DATA,
                property as *const _,
                &mut property_type as *mut DEVPROPTYPE,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut required_size as *mut u32,
                0,
            )
        } == TRUE;

        if !get_type_operation_result {
            Err(unsafe { GetLastError().into() })
        } else {
            Ok((buffer.into(), property_type))
        }
    }

    unsafe fn retrive_string_property(
        devinfo: SP_DEVINFO_DATA,
        devinfoset: HDEVINFO,
        property: &DEVPROPKEY,
    ) -> Result<Rc<str>, Win32Error> {
        let device_property = Device::retrive_device_property(devinfo, devinfoset, property)?;
        let device_property =
            DeviceProperty::try_from((device_property.0.as_slice(), device_property.1))
                .expect("Failed to convert the Device Type Property!");
        let device_property = match device_property {
            DeviceProperty::StringProperty { data } => Rc::from(data),
            _ => panic!("Unexpected property type for Device Type!"),
        };
        Ok(device_property)
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
                println!("Attempting to enumerate device at index: {}", index);
                let mut device_data: SP_DEVINFO_DATA = std::mem::zeroed();
                device_data.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
                let operation_result = SetupDiEnumDeviceInfo(
                    devinfoset,
                    index,
                    &mut device_data as *mut SP_DEVINFO_DATA,
                ) == TRUE;

                if operation_result {
                    let next_device = Device::from_bare_devinfo(device_data, devinfoset)?; 

                    devices.push(next_device);
                    println!("\t- Device found at index: {}", index);
                    index += 1;
                } else {
                    println!("\t- No device found at index: {}", index);
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


fn device_filter_function(device: &Device) -> bool {
    if let Some(service) = &device.device_service {
        service.as_ref() == "usbhub3" || service.as_ref() == "usbhub"
    } else {
        false
    }
}
