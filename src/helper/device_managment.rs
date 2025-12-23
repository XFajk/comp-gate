//! # Device Management Module
//!
//! This module provides the core functionality for interacting with the Windows SetupAPI
//! to manage USB and HID devices. It allows for:
//!
//! - Enumerating connected devices.
//! - Retrieving device properties (ID, Class, Description, etc.).
//! - Organizing devices into a hierarchical tree structure based on parent-child relationships.
//! - Enabling and disabling devices.
//! - Tracking device insertion and removal at runtime.

use crate::error::{
    ConfigManagerError, DeviceInsertionError, DeviceStringPropertyError, Win32Error,
};

use std::{
    collections::HashMap,
    ops::Deref,
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

pub struct DeviceInstance(u32);

impl Deref for DeviceInstance {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TryFrom<u32> for DeviceInstance {
    type Error = ConfigManagerError;

    fn try_from(raw_devinst: u32) -> Result<Self, Self::Error> {
        let devinst = DeviceInstance(raw_devinst);
        if !devinst.is_device_instance_valid() {
            return Err(ConfigManagerError::InvalidDeviceInstance);
        }

        Ok(devinst)
    }
}

impl TryFrom<&str> for DeviceInstance {
    type Error = ConfigManagerError;

    fn try_from(id: &str) -> Result<Self, Self::Error> {
        let device_id_wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();

        let mut devinst: u32 = 0;
        let result = unsafe {
            CM_Locate_DevNodeW(
                &mut devinst,
                device_id_wide.as_ptr(),
                CM_LOCATE_DEVNODE_NORMAL,
            )
        };

        if result != CR_SUCCESS {
            return Err(ConfigManagerError::from(result));
        }

        Ok(DeviceInstance(devinst))
    }
}

impl DeviceInstance {
    /// Retrieves the Device Instance ID string.
    fn retrieve_device_id(&self) -> Result<Rc<str>, Win32Error> {
        if !self.is_device_instance_valid() {
            return Err(Win32Error::InvalidParameter);
        }

        let mut buffer: Vec<u16> = vec![0; 512];
        let mut buffer_size = buffer.len() as u32;

        // SAFETY: This call is safe because we are passing a valid pointer to a mutable buffer
        //  and valid device instance.
        let call_result = unsafe { CM_Get_Device_ID_Size(&mut buffer_size as *mut _, **self, 0) };
        if call_result != CR_SUCCESS {
            return Err(ConfigManagerError::from(call_result).into());
        }

        buffer_size += 1;
        buffer.resize(buffer_size as usize, 0);

        // First call to get the required size

        let call_result = unsafe { CM_Get_Device_IDW(**self, buffer.as_mut_ptr(), buffer_size, 0) };
        if call_result != CR_SUCCESS {
            return Err(ConfigManagerError::from(call_result).into());
        }

        let len = if buffer_size == 0 {
            buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len())
        } else {
            (buffer_size as usize).saturating_sub(1)
        };
        let device_instance_id: Rc<str> = String::from_utf16_lossy(&buffer[..len])
            .to_uppercase()
            .into();
        Ok(device_instance_id)
    }

    /// Retrieves a raw property from the device.
    fn retrieve_device_property(
        &self,
        property: &DEVPROPKEY,
    ) -> Result<(Vec<u8>, DEVPROPTYPE), Win32Error> {
        if !self.is_device_instance_valid() {
            return Err(Win32Error::InvalidParameter);
        }

        let mut buffer: Vec<u8> = vec![];
        let mut required_size: u32 = 0;
        let mut property_type: DEVPROPTYPE = 0;

        // First call to get the required size
        // SAFETY: We are passing a valid device instance and property key.
        let call_result = unsafe {
            CM_Get_DevNode_PropertyW(
                **self,
                property as *const _,
                &mut property_type as *mut DEVPROPTYPE,
                null_mut(),
                &mut required_size as *mut _,
                0,
            )
        };

        // CR_BUFFER_SMALL (26) is expected here - it means the property exists but we need a buffer
        const CR_BUFFER_SMALL: u32 = 26;
        if call_result != CR_SUCCESS && call_result != CR_BUFFER_SMALL {
            return Err(ConfigManagerError::from(call_result).into());
        }

        buffer.resize(required_size as usize, 0u8);

        // SAFETY: We are passing a valid device instance, property key, and buffer.
        let call_result = unsafe {
            CM_Get_DevNode_PropertyW(
                **self,
                property as *const _,
                &mut property_type as *mut DEVPROPTYPE,
                buffer.as_mut_ptr(),
                &mut required_size as *mut u32,
                0,
            )
        };
        if call_result != CR_SUCCESS {
            return Err(ConfigManagerError::from(call_result).into());
        }

        Ok((buffer.into(), property_type))
    }

    /// Retrieves a specific string property from the device.
    fn retrieve_string_property(
        &self,
        property: &DEVPROPKEY,
    ) -> Result<Rc<str>, DeviceStringPropertyError> {
        let device_property = self.retrieve_device_property(property)?;
        let device_property =
            DeviceProperty::from((device_property.0.as_slice(), device_property.1));
        let device_property = match device_property {
            DeviceProperty::StringProperty { data } => Rc::from(data),
            _ => return Err(DeviceStringPropertyError::PropertyNotString),
        };
        Ok(device_property)
    }

    fn is_device_instance_valid(&self) -> bool {
        let mut status = 0u32;
        let mut problem_number = 0u32;

        let call_result = unsafe {
            CM_Get_DevNode_Status(
                &mut status as *mut _,
                &mut problem_number as *mut _,
                **self,
                0,
            )
        };
        call_result == CR_SUCCESS
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceId(Rc<str>);

impl std::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Deref for DeviceId {
    type Target = Rc<str>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Rc<str>> for DeviceId {
    fn from(id: Rc<str>) -> Self {
        DeviceId(id)
    }
}

/// Represents the desired state of a device driver.
#[repr(u32)]
pub enum DeviceState {
    /// Enable the device driver.
    Enable = DICS_ENABLE,
    /// Disable the device driver.
    Disable = DICS_DISABLE,
}

/// Represents a property retrieved from a device.
///
/// This enum handles different types of properties that can be queried from the SetupAPI.
/// Currently, it focuses on string properties but handles unsupported types gracefully.
pub enum DeviceProperty {
    EmptyProperty,
    /// Represents a string property (REG_SZ).
    StringProperty {
        /// The string value of the property.
        data: String,
    },
    /// Represents a property type that is not explicitly handled by this wrapper.
    UnsupportedProperty {
        /// The raw byte data of the property.
        raw_data: Rc<[u8]>,
        /// The Windows property type identifier.
        property_type: DEVPROPTYPE,
    },
}

impl From<(&[u8], DEVPROPTYPE)> for DeviceProperty {
    /// Converts a raw byte slice and property type into a `DeviceProperty`.
    ///
    /// This function handles the parsing of raw bytes into Rust types based on the `DEVPROPTYPE`.
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

/// Represents a physical or logical device on the system.
///
/// This struct holds metadata about the device and maintains a list of its child devices,
/// forming a tree structure.
pub struct Device {
    /// Internal Windows handle data for the device.
    devinst: DeviceInstance,
    /// The unique Instance ID of the device (e.g., `USB\VID_XXXX&PID_XXXX\SN`).
    pub device_id: DeviceId,
    /// The Instance ID of the parent device, if any.
    pub parent_id: Option<DeviceId>,
    /// The depth of this device in the device tree (0 for root).
    pub tree_level: u32,
    /// A collection of child devices attached to this device.
    pub devices: HashMap<DeviceId, Device>,

    /// The name of the service driving the device.
    pub device_service: Option<Rc<str>>,
    /// The device setup class (e.g., "USB", "HIDClass").
    pub device_class: Option<Rc<str>>,
    /// The friendly name of the device as seen in Device Manager.
    pub device_friendly_name: Option<Rc<str>>,
    /// The device type identifier.
    pub device_type: Option<Rc<str>>,
    /// The description of the device.
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

impl TryFrom<DeviceInstance> for Device {
    type Error = Win32Error;

    fn try_from(devinst: DeviceInstance) -> Result<Self, Self::Error> {
        let device_id = devinst.retrieve_device_id()?.into();

        let parent_id = match devinst.retrieve_string_property(&DEVPKEY_Device_Parent) {
            Ok(prop) => Some(DeviceId::from(Rc::from(prop.to_uppercase()))),
            Err(_) => None,
        };

        let device_service = match devinst.retrieve_string_property(&DEVPKEY_Device_Service) {
            Ok(prop) => Some(prop.to_lowercase().into()),
            Err(e) => {
                println!(
                    "Warning: Could not retrieve Device Service for Device ID {} because of an error: {:?}",
                    device_id, e
                );
                None
            }
        };

        let device_class = match devinst.retrieve_string_property(&DEVPKEY_Device_Class) {
            Ok(prop) => Some(prop),
            Err(e) => {
                println!(
                    "Warning: Could not retrieve Device Class for Device ID {} because of an error: {:?}",
                    device_id, e
                );
                None
            }
        };

        let device_type = match devinst.retrieve_string_property(&DEVPKEY_Device_DevType) {
            Ok(prop) => Some(prop),
            Err(e) => {
                println!(
                    "Warning: Could not retrieve Device Type for Device ID {} because of an error: {:?}",
                    device_id, e
                );
                None
            }
        };

        let device_description = match devinst.retrieve_string_property(&DEVPKEY_Device_DeviceDesc)
        {
            Ok(prop) => Some(prop),
            Err(e) => {
                println!(
                    "Warning: Could not retrieve Device Description for Device ID {} because of an error: {:?}",
                    device_id, e
                );
                None
            }
        };

        let device_friendly_name = match devinst
            .retrieve_string_property(&DEVPKEY_Device_FriendlyName)
        {
            Ok(prop) => Some(prop),
            Err(e) => {
                println!(
                    "Warning: Could not retrieve Device Friendly Name for Device ID {} because of an error: {:?}",
                    device_id, e
                );
                None
            }
        };

        Ok(Device {
            devinst,
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
}

impl Device {
    /// Changes the state of the device (Enable/Disable).
    ///
    /// This function uses `SetupDiSetClassInstallParams` and `SetupDiCallClassInstaller` to modify the device state.
    ///
    /// # Arguments
    ///
    /// * `new_state` - The target state (`Enable` or `Disable`).
    /// * `information_set` - The handle to the device information set.
    fn change_state(&self, new_state: DeviceState) -> Result<(), Win32Error> {
        let result = unsafe {
            match new_state {
                DeviceState::Enable => CM_Enable_DevNode(*self.devinst, 0),
                DeviceState::Disable => CM_Disable_DevNode(*self.devinst, 0),
            }
        };

        if result != CR_SUCCESS {
            return Err(ConfigManagerError::from(result).into());
        }

        Ok(())
    }
}

/// Manages a collection of devices using the Windows Configuration Manager API.
///
/// This struct is the main entry point for querying and manipulating devices. It uses
/// DEVINST handles from the CM API to interact with devices without requiring HDEVINFO sets.
pub struct DeviceTracker {
    /// A map of root-level devices managed by this tracker.
    pub devices: HashMap<DeviceId, Device>,
}

impl std::fmt::Display for DeviceTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (_, device) in self.devices.iter() {
            writeln!(f, "{}", device)?;
        }
        Ok(())
    }
}

impl DeviceTracker {
    /// Sets the state (Enable/Disable) of a specific device by its ID.
    ///
    /// This function searches the entire device tree for the specified ID.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The Instance ID of the device to modify.
    /// * `state` - The desired state.
    pub fn set_device_state(
        &self,
        device_id: &DeviceId,
        state: DeviceState,
    ) -> Result<(), Win32Error> {
        fn find_device_in_tree<'a>(
            devices: &'a HashMap<DeviceId, Device>,
            target_id: &DeviceId,
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
            device.change_state(state)
        } else {
            Err(Win32Error::from(ERROR_DEV_NOT_EXIST))
        }
    }

    /// Inserts a new device into the tracker by its ID.
    ///
    /// This is typically called when a new device is detected via a system event.
    /// It adds the device to the internal `HDEVINFO` set and places it in the device tree.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The Instance ID of the new device.
    pub fn insert_device_by_id(&mut self, device_id: &str) -> Result<(), DeviceInsertionError> {
        let device_instance = DeviceInstance::try_from(device_id)
            .map_err(|err| DeviceInsertionError::from(Win32Error::from(err)))?;
        let new_device = Device::try_from(device_instance)?;

        if device_filter_function(&new_device) {
            return Err(DeviceInsertionError::DeviceFilteredNotUsb);
        }

        self.insert_deivice_into_tree(new_device);

        Ok(())
    }

    /// Internal helper to place a new device into the correct position in the tree.
    ///
    /// It handles finding the parent device and also checks if the new device
    /// should become the parent of any existing "orphan" devices.
    fn insert_deivice_into_tree(&mut self, new_device: Device) {
        let new_device_id = new_device.device_id.clone();

        if let Some(parent_id) = &new_device.parent_id {
            /// Helper to recursively find a parent in the existing tree
            fn find_parent_mut<'a>(
                devices: &'a mut HashMap<DeviceId, Device>,
                target_parent_id: &DeviceId,
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

        let orphan_ids: Vec<DeviceId> = self
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

    /// Removes a device from the tracker by its ID.
    ///
    /// This removes the device from the tree and deletes it from the `HDEVINFO` set.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The Instance ID of the device to remove.
    pub fn remove_device_by_id(&mut self, device_id: &DeviceId) -> Option<Device> {
        fn find_and_remove_device(
            devices: &mut HashMap<DeviceId, Device>,
            device_id: &DeviceId,
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

        find_and_remove_device(&mut self.devices, device_id)
    }
}

impl DeviceTracker {
    /// Helper to get a handle to devices of a specific class.
    fn get_class_devs(class_name: *const u8) -> Result<HDEVINFO, Win32Error> {
        let devinfo_set: HDEVINFO = unsafe {
            SetupDiGetClassDevsA(
                null(),
                class_name,
                null_mut(),
                DIGCF_ALLCLASSES | DIGCF_PRESENT,
            )
        };

        if devinfo_set == INVALID_HANDLE_VALUE as HDEVINFO {
            return Err(unsafe { GetLastError().into() });
        }

        Ok(devinfo_set)
    }

    /// Loads all currently connected USB and HID devices into a new `DeviceTracker`.
    ///
    /// This is the primary factory method for creating a `DeviceTracker`.
    pub fn load() -> Result<Self, Win32Error> {
        let usb_device_information_set = Self::get_class_devs(c"USB".as_ptr() as *const u8)?;
        let hid_device_information_set = Self::get_class_devs(c"HID".as_ptr() as *const u8)?;

        Self::merge_device_information_sets(&[
            usb_device_information_set,
            hid_device_information_set,
        ])
    }

    /// Enumerates all devices in a given `HDEVINFO` set.
    fn get_listed_devices(devinfoset: HDEVINFO) -> Result<HashMap<DeviceId, Device>, Win32Error> {
        let mut devices: HashMap<DeviceId, Device> = HashMap::new();
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
                    let device_instance = DeviceInstance::try_from(device_data.DevInst)
                        .map_err(|err| Win32Error::from(err))?;
                    let next_device = Device::try_from(device_instance)?;

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

    /// Merges multiple `HDEVINFO` sets into a single `DeviceTracker`.
    fn merge_device_information_sets(sets: &[HDEVINFO]) -> Result<Self, Win32Error> {
        let mut merged_devices = HashMap::new();

        for set in sets.iter() {
            let devices = DeviceTracker::get_listed_devices(*set)?;
            Self::merge_device_trees(&mut merged_devices, devices);

            // free the device information set
            if *set == INVALID_HANDLE_VALUE as isize {
                continue;
            }
            unsafe {
                let _ = SetupDiDestroyDeviceInfoList(*set);
            }
        }

        Ok(Self {
            devices: merged_devices,
        })
    }

    /// Merges two device trees into one by finding the correct parent-child relationships.
    ///
    /// This function takes a base tree and a tree to merge, then iterates through all devices
    /// in the tree_to_merge and inserts them into the correct position in the base tree based
    /// on their parent_id relationships.
    ///
    /// # Arguments
    /// * `base_tree` - The target tree that will receive all devices (will be modified in place)
    /// * `tree_to_merge` - The source tree whose devices will be merged into base_tree
    pub fn merge_device_trees(
        base_tree: &mut HashMap<DeviceId, Device>,
        tree_to_merge: HashMap<DeviceId, Device>,
    ) {
        // Collect all devices from tree_to_merge into a flat Vec
        // We need to do this because we can't iterate over tree_to_merge while moving devices out
        let mut devices_to_insert = Vec::new();

        fn collect_devices(devices: HashMap<DeviceId, Device>, collector: &mut Vec<Device>) {
            for (_, mut device) in devices {
                let children = std::mem::take(&mut device.devices);
                collector.push(device);
                collect_devices(children, collector);
            }
        }

        collect_devices(tree_to_merge, &mut devices_to_insert);

        // Helper function to recursively find a device by ID in the tree
        fn find_device_mut<'a>(
            devices: &'a mut HashMap<DeviceId, Device>,
            target_id: &DeviceId,
        ) -> Option<&'a mut Device> {
            if devices.contains_key(target_id) {
                return devices.get_mut(target_id);
            }
            for dev in devices.values_mut() {
                if let Some(found) = find_device_mut(&mut dev.devices, target_id) {
                    return Some(found);
                }
            }
            None
        }

        // Insert each device into the correct location in base_tree
        for mut device in devices_to_insert {
            let device_id = device.device_id.clone();

            // Try to find the parent in the base tree
            if let Some(parent_id) = &device.parent_id {
                if let Some(parent) = find_device_mut(base_tree, parent_id) {
                    // Found the parent, insert as a child
                    device.tree_level = parent.tree_level + 1;
                    parent.devices.insert(device_id, device);
                    continue;
                }
            }

            // No parent found (or no parent_id), insert at root level
            device.tree_level = 0;
            let inserted_device_id = device_id.clone();
            base_tree.insert(device_id, device);

            // Check if any existing root-level devices should be re-parented under this new device
            let orphan_ids: Vec<DeviceId> = base_tree
                .iter()
                .filter(|(id, dev)| {
                    **id != inserted_device_id
                        && dev.parent_id.as_ref().map(|p| p.as_ref())
                            == Some(inserted_device_id.as_ref())
                })
                .map(|(id, _)| id.clone())
                .collect();

            for orphan_id in orphan_ids {
                if let Some(mut orphan) = base_tree.remove(&orphan_id) {
                    if let Some(new_parent) = base_tree.get_mut(&inserted_device_id) {
                        orphan.tree_level = new_parent.tree_level + 1;
                        new_parent.devices.insert(orphan_id, orphan);
                    }
                }
            }
        }
    }
}

/// An iterator over all devices in a `DeviceTracker`.
///
/// This iterator performs a depth-first traversal of the device tree.
pub struct DeviceIterator<'a> {
    stack: Vec<&'a Device>,
}

impl<'a> DeviceIterator<'a> {
    /// Creates a new iterator from a map of devices.
    pub fn new(devices: &'a HashMap<DeviceId, Device>) -> Self {
        let stack = devices.values().collect();

        DeviceIterator { stack }
    }
}

impl<'a> From<&'a HashMap<DeviceId, Device>> for DeviceIterator<'a> {
    fn from(devices: &'a HashMap<DeviceId, Device>) -> Self {
        Self::new(devices)
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
    /// Returns an iterator over all devices tracked by this instance.
    pub fn iter<'a>(&'a self) -> DeviceIterator<'a> {
        DeviceIterator::new(&self.devices)
    }
}

/// Filters out devices that should not be tracked (e.g., USB hubs).
fn device_filter_function(device: &Device) -> bool {
    if let Some(service) = &device.device_service {
        service.as_ref() == "usbhub3" || service.as_ref() == "usbhub"
    } else {
        false
    }
}

/// Converts a flat map of devices into a hierarchical tree.
fn convert_devices_into_tree(mut devices: HashMap<DeviceId, Device>) -> HashMap<DeviceId, Device> {
    let device_ids: Vec<DeviceId> = devices.keys().cloned().collect();
    let parent_ids: Vec<(DeviceId, DeviceId)> = devices
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

/// Recursive helper to move a child device into its parent's `devices` map.
fn place_child_in_parent(
    parent_id: &DeviceId,
    child_id: &DeviceId,
    devices: &mut HashMap<DeviceId, Device>,
    device_ids: &Vec<DeviceId>,
    parent_ids: &Vec<(DeviceId, DeviceId)>,
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

/// Extract device instance ID from device interface path.
///
/// # Example
///
/// Input:  `\\?\USB#VID_046D&PID_C52B#5&2752457f&0&2#{a5dcbf10-6530-11d2-901f-00c04fb951ed}`
/// Output: `USB\VID_046D&PID_C52B\5&2752457f&0&2`
pub fn device_path_to_device_id(device_path: &str) -> DeviceId {
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

    Rc::<str>::from(instance_id.to_uppercase()).into()
}
