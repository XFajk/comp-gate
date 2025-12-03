use crate::error::{DeviceInsertionError, DeviceStringPropertyError, Win32Error};
/// This file holds the functions related to device management
/// such as listing connected devices, ejecting devices, etc.
use std::{
    collections::HashMap,
    path::Display,
    ptr::{null, null_mut},
    rc::Rc,
};
use windows_sys::Win32::{
    Devices::{
        DeviceAndDriverInstallation::*,
        Properties::{
            DEVPKEY_Device_Class, DEVPKEY_Device_DevType, DEVPKEY_Device_DeviceDesc,
            DEVPKEY_Device_FriendlyName, DEVPKEY_Device_Parent, DEVPKEY_Device_Service,
            DEVPROP_MASK_TYPE, DEVPROP_TYPE_EMPTY, DEVPROP_TYPE_STRING, DEVPROPTYPE,
        },
    },
    Foundation::*,
};

#[repr(u32)]
pub enum DeviceState {
    Enable = DICS_ENABLE,
    Disable = DICS_DISABLE,
}

pub enum DeviceProperty {
    EmptyProperty, // May not be used but is here so that the enum has more that one variant
    StringProperty {
        data: String,
    },
    UnsupportedProperty {
        raw_data: Rc<[u8]>,
        property_type: DEVPROPTYPE,
    },
}

impl From<(&[u8], DEVPROPTYPE)> for DeviceProperty {
    fn from(value: (&[u8], DEVPROPTYPE)) -> Self {
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

                DeviceProperty::StringProperty {
                    data: String::from_utf16_lossy(&u16_slice[..len]),
                }
            }
            DEVPROP_TYPE_EMPTY => DeviceProperty::EmptyProperty,
            _ => DeviceProperty::UnsupportedProperty {
                raw_data: value.0.into(), // CLONING THE SLICE DATA
                property_type: value.1,
            },
        }
    }
}

pub struct Device {
    devinfo: SP_DEVINFO_DATA,
    pub device_id: Rc<str>,
    pub parent_id: Option<Rc<str>>,
    pub tree_level: u32,
    pub devices: HashMap<Rc<str>, Device>,

    pub device_service: Option<Rc<str>>,
    pub device_class: Option<Rc<str>>,
    pub device_friendly_name: Option<Rc<str>>,
    pub device_type: Option<Rc<str>>,
    pub device_description: Option<Rc<str>>,
}

impl std::fmt::Display for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{}Device ID: {}",
            "\t".repeat(self.tree_level as usize),
            self.device_id
        )?;
        writeln!(
            f,
            "{} - Device Service: {}",
            "\t".repeat(self.tree_level as usize),
            self.device_service.as_deref().unwrap_or("None")
        )?;
        writeln!(
            f,
            "{} - Device Class: {}",
            "\t".repeat(self.tree_level as usize),
            self.device_class.as_deref().unwrap_or("None")
        )?;
        writeln!(
            f,
            "{} - Device Friendly Name: {}",
            "\t".repeat(self.tree_level as usize),
            self.device_friendly_name.as_deref().unwrap_or("None")
        )?;
        writeln!(
            f,
            "{} - Device Type: {}",
            "\t".repeat(self.tree_level as usize),
            self.device_type.as_deref().unwrap_or("None")
        )?;
        writeln!(
            f,
            "{} - Device Description: {}",
            "\t".repeat(self.tree_level as usize),
            self.device_description.as_deref().unwrap_or("None")
        )?;
        for (_, sub_device) in self.devices.iter() {
            writeln!(
                f,
                "{}Sub-device:",
                "\t".repeat(self.tree_level as usize + 1)
            )?;
            writeln!(f, "{}", sub_device)?;
        }
        Ok(())
    }
}

impl Device {
    fn from_bare_devinfo(
        devinfo: SP_DEVINFO_DATA,
        devinfoset: HDEVINFO,
    ) -> Result<Self, Win32Error> {
        let device_id = Self::retrieve_device_id(devinfo, devinfoset)?;

        let parent_id = match unsafe {
            Self::retrieve_string_property(devinfo, devinfoset, &DEVPKEY_Device_Parent)
        } {
            Ok(prop) => Some(prop.to_uppercase().into()),
            Err(_) => None,
        };

        let device_service = match unsafe {
            Self::retrieve_string_property(devinfo, devinfoset, &DEVPKEY_Device_Service)
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
            Self::retrieve_string_property(devinfo, devinfoset, &DEVPKEY_Device_Class)
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
            Self::retrieve_string_property(devinfo, devinfoset, &DEVPKEY_Device_DevType)
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
            match Self::retrieve_string_property(devinfo, devinfoset, &DEVPKEY_Device_DeviceDesc) {
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
            match Self::retrieve_string_property(devinfo, devinfoset, &DEVPKEY_Device_FriendlyName)
            {
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
            parent_id,
            tree_level: 0,
            devices: HashMap::new(),
            device_service,
            device_class,
            device_friendly_name,
            device_type,
            device_description,
        })
    }

    fn retrieve_device_id(
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
            let device_instance_id: Rc<str> = String::from_utf16_lossy(&buffer[..len])
                .to_uppercase()
                .into();
            Ok(device_instance_id)
        }
    }

    fn retrieve_device_property(
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

    unsafe fn retrieve_string_property(
        devinfo: SP_DEVINFO_DATA,
        devinfoset: HDEVINFO,
        property: &DEVPROPKEY,
    ) -> Result<Rc<str>, DeviceStringPropertyError> {
        let device_property = Device::retrieve_device_property(devinfo, devinfoset, property)?;
        let device_property =
            DeviceProperty::from((device_property.0.as_slice(), device_property.1));
        let device_property = match device_property {
            DeviceProperty::StringProperty { data } => Rc::from(data),
            _ => return Err(DeviceStringPropertyError::PropertyNotString),
        };
        Ok(device_property)
    }

    fn change_state(
        &self,
        new_state: DeviceState,
        information_set: HDEVINFO,
    ) -> Result<(), Win32Error> {
        let property_change: SP_PROPCHANGE_PARAMS = SP_PROPCHANGE_PARAMS {
            ClassInstallHeader: SP_CLASSINSTALL_HEADER {
                cbSize: std::mem::size_of::<SP_CLASSINSTALL_HEADER>() as u32,
                InstallFunction: DIF_PROPERTYCHANGE,
            },
            StateChange: new_state as u32,
            Scope: DICS_FLAG_GLOBAL,
            HwProfile: 0,
        };

        let set_params_result = unsafe {
            SetupDiSetClassInstallParamsW(
                information_set,
                &self.devinfo as *const SP_DEVINFO_DATA,
                &property_change as *const SP_PROPCHANGE_PARAMS as *const SP_CLASSINSTALL_HEADER,
                std::mem::size_of::<SP_PROPCHANGE_PARAMS>() as u32,
            )
        } == TRUE;

        if !set_params_result {
            return Err(unsafe { GetLastError().into() });
        }

        let call_result = unsafe {
            SetupDiCallClassInstaller(
                DIF_PROPERTYCHANGE,
                information_set,
                &self.devinfo as *const SP_DEVINFO_DATA,
            )
        } == TRUE;

        if !call_result {
            return Err(unsafe { GetLastError().into() });
        }

        Ok(())
    }
}

pub struct DeviceTracker {
    device_information_set: HDEVINFO,
    pub devices: HashMap<Rc<str>, Device>,
}

impl std::fmt::Display for DeviceTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (_, device) in self.devices.iter() {
            writeln!(f, "{}", device)?;
        }
        Ok(())
    }
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

    pub fn set_device_state(&self, device_id: &str, state: DeviceState) -> Result<(), Win32Error> {
        fn find_device_in_tree<'a>(
            devices: &'a HashMap<Rc<str>, Device>,
            target_id: &str,
        ) -> Option<&'a Device> {
            if let Some(device) = devices.get(target_id) {
                return Some(device);
            }

            for device in devices.values() {
                if let Some(found) = find_device_in_tree(&device.devices, target_id) {
                    return Some(found);
                }
            }

            None
        }

        if let Some(device) = find_device_in_tree(&self.devices, device_id) {
            device.change_state(state, self.device_information_set)
        } else {
            Err(Win32Error::from(ERROR_DEV_NOT_EXIST))
        }
    }

    fn get_listed_devices(devinfoset: HDEVINFO) -> Result<HashMap<Rc<str>, Device>, Win32Error> {
        let mut devices: HashMap<Rc<str>, Device> = HashMap::new();
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

                    if !device_filter_function(&next_device) {
                        devices.insert(next_device.device_id.clone(), next_device);
                    }
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
        Ok(convert_devices_into_tree(devices))
    }

    pub fn insert_device_by_id(&mut self, device_id: &str) -> Result<(), DeviceInsertionError> {
        let device_id_wide = device_id
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>();

        let mut device_info: SP_DEVINFO_DATA = unsafe { std::mem::zeroed() };
        device_info.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;

        let insertion_result = unsafe {
            SetupDiOpenDeviceInfoW(
                self.device_information_set,
                device_id_wide.as_ptr(),
                null_mut(),
                0,
                &mut device_info as *mut SP_DEVINFO_DATA,
            )
        } == TRUE;

        if !insertion_result {
            return Err(unsafe { DeviceInsertionError::from(Win32Error::from(GetLastError())) });
        }

        let new_device = Device::from_bare_devinfo(device_info, self.device_information_set)?;

        if device_filter_function(&new_device) {
            let deletion_result = unsafe {
                SetupDiDeleteDeviceInfo(
                    self.device_information_set,
                    &device_info as *const SP_DEVINFO_DATA,
                )
            } == TRUE;
            if !deletion_result {
                println!(
                    "Warning: Could not delete filtered device {} after insertion failure. because of {}",
                    device_id,
                    Win32Error::from(unsafe { GetLastError() })
                );
            }

            return Err(DeviceInsertionError::DeviceFilteredNotUsb);
        }

        self.insert_deivice_into_tree(new_device);

        Ok(())
    }

    fn insert_deivice_into_tree(&mut self, new_device: Device) {
        let new_device_id = new_device.device_id.clone();

        if let Some(parent_id) = &new_device.parent_id {
            /// Helper to recursively find a parent in the existing tree
            fn find_parent_mut<'a>(
                devices: &'a mut HashMap<Rc<str>, Device>,
                target_parent_id: &str,
            ) -> Option<&'a mut Device> {
                if devices.contains_key(target_parent_id) {
                    return devices.get_mut(target_parent_id);
                }
                for dev in devices.values_mut() {
                    if let Some(found) = find_parent_mut(&mut dev.devices, target_parent_id) {
                        return Some(found);
                    }
                }
                None
            }

            if let Some(parent) = find_parent_mut(&mut self.devices, parent_id) {
                // Update tree level based on parent
                let mut child = new_device;
                child.tree_level = parent.tree_level + 1;

                parent.devices.insert(child.device_id.clone(), child);
                return;
            }
        }

        self.devices.insert(new_device_id.clone(), new_device);

        let orphan_ids: Vec<Rc<str>> = self
            .devices
            .iter()
            .filter(|(_, dev)| {
                dev.parent_id.as_ref().map(|p| p.as_ref()) == Some(new_device_id.as_ref())
            })
            .map(|(id, _)| id.clone())
            .collect();

        for orphan_id in orphan_ids {
            if let Some(mut orphan) = self.devices.remove(&orphan_id) {
                if let Some(new_parent) = self.devices.get_mut(&new_device_id) {
                    println!(
                        "- Re-parenting orphan device {} under {}",
                        orphan_id, new_device_id
                    );
                    orphan.tree_level = new_parent.tree_level + 1;
                    new_parent.devices.insert(orphan_id, orphan);
                }
            }
        }
    }

    pub fn remove_device_by_id(&mut self, device_id: &str) -> Option<Win32Error> {
        fn find_and_remove_device(
            devices: &mut HashMap<Rc<str>, Device>,
            device_id: &str,
        ) -> Option<Device> {
            if devices.contains_key(device_id) {
                devices.remove(device_id)
            } else {
                for d in devices.values_mut() {
                    if let Some(rd) = find_and_remove_device(&mut d.devices, device_id) {
                        return Some(rd);
                    }
                }
                None
            }
        }

        if let Some(d) = find_and_remove_device(&mut self.devices, device_id) {
            let deletion_result = unsafe {
                SetupDiDeleteDeviceInfo(
                    self.device_information_set,
                    &d.devinfo as *const SP_DEVINFO_DATA,
                )
            } == TRUE;

            if !deletion_result {
                Some(unsafe { GetLastError().into() })
            } else {
                None
            }
        } else {
            None
        }
    }
}

pub struct DeviceIterator<'a> {
    stack: Vec<&'a Device>,
}

impl<'a> DeviceIterator<'a> {
    pub fn new(devices: &'a HashMap<Rc<str>, Device>) -> Self {
        let stack = devices.values().collect();

        DeviceIterator { stack }
    }
}

impl<'a> Iterator for DeviceIterator<'a> {
    type Item = &'a Device;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(device) = self.stack.pop() {
            self.stack.extend(device.devices.values());
            Some(device)
        } else {
            None
        }
    }
}

impl DeviceTracker {
    pub fn iter<'a>(&'a self) -> DeviceIterator<'a> {
        DeviceIterator::new(&self.devices)
    }
}

fn device_filter_function(device: &Device) -> bool {
    if let Some(service) = &device.device_service {
        service.as_ref() == "usbhub3" || service.as_ref() == "usbhub"
    } else {
        false
    }
}

fn convert_devices_into_tree(mut devices: HashMap<Rc<str>, Device>) -> HashMap<Rc<str>, Device> {
    let device_ids: Vec<Rc<str>> = devices.keys().cloned().collect();
    let parent_ids: Vec<(Rc<str>, Rc<str>)> = devices
        .values()
        .filter_map(|d| {
            if let Some(pid) = &d.parent_id {
                Some((pid.clone(), d.device_id.clone()))
            } else {
                None
            }
        })
        .collect();

    for (pid, cid) in parent_ids.iter() {
        place_child_in_parent(pid, cid, &mut devices, &device_ids, &parent_ids, 0);
    }

    devices
}

fn place_child_in_parent(
    parent_id: &Rc<str>,
    child_id: &Rc<str>,
    devices: &mut HashMap<Rc<str>, Device>,
    device_ids: &Vec<Rc<str>>,
    parent_ids: &Vec<(Rc<str>, Rc<str>)>,
    level: u32,
) -> () {
    if device_ids.contains(parent_id) {
        // This code here tracks a bug if we have a more nested device tree
        // what can happen is that a child_device can also be a perent of another device
        // and since we are moving the child_device from the devices HashMap to the sub_interface_devices
        // we need to track where when we find the device that has the child_device as parent
        // we can get this child_device from the parent_device's sub_interface_devices
        // instead of trying to get it from the devices HashMap which no longer contains it
        while let Some((pid, cid)) = parent_ids.iter().find(|(p, _)| p == child_id) {
            place_child_in_parent(pid, cid, devices, device_ids, parent_ids, level + 1);
        }

        let mut child_device = devices.remove(child_id).unwrap();
        let parent_device = devices.get_mut(parent_id).unwrap();

        child_device.tree_level = level + 1;

        parent_device
            .devices
            .insert(child_device.device_id.clone(), child_device);
    }
}

/// Extract device instance ID from device interface path
/// Input:  \\?\USB#VID_046D&PID_C52B#5&2752457f&0&2#{a5dcbf10-6530-11d2-901f-00c04fb951ed}
/// Output: USB\VID_046D&PID_C52B\5&2752457f&0&2
pub fn device_path_to_device_id(device_path: &str) -> Rc<str> {
    // Remove \\?\ prefix
    let path = device_path.strip_prefix(r"\\?\").unwrap_or(device_path);

    // Remove GUID suffix (everything after the last #)
    let path = if let Some(pos) = path.rfind('#') {
        &path[..pos]
    } else {
        path
    };

    // Replace # with \
    let instance_id = path.replace('#', r"\");

    instance_id.into()
}
