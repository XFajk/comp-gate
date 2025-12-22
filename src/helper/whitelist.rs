//! # Whitelist Module
//!
//! This module manages the list of authorized USB devices.
//! It uses the system keyring to securely store the whitelist of device IDs.
//!
//! The `Whitelist` struct provides methods to:
//! - Initialize the whitelist from currently connected devices.
//! - Apply the whitelist (disable unauthorized devices).
//! - Add or remove devices from the whitelist.
//! - Persist the whitelist state.

use keyring::Entry;
use std::{collections::HashSet, rc::Rc, str};

use crate::helper::device_managment::{DeviceId, DeviceTracker};
use anyhow::{Result, anyhow};

/// Manages the authorized device list and enforces it on the system.
pub struct Whitelist {
    /// The keyring entry used for secure storage.
    entry: Entry,

    /// The tracker used to interact with system devices.
    pub device_tracker: DeviceTracker,
}

impl Whitelist {
    /// Creates a new `Whitelist` instance.
    ///
    /// This initializes the keyring entry and, if creating for the first time,
    /// might populate it with the currently connected devices (based on the implementation logic).
    ///
    /// # Arguments
    ///
    /// * `device_tracker` - An initialized `DeviceTracker` containing current system devices.
    ///
    /// # Returns
    ///
    /// * `Ok(Whitelist)` - The initialized whitelist manager.
    /// * `Err(anyhow::Error)` - If keyring access fails.
    pub fn new(device_tracker: DeviceTracker) -> anyhow::Result<Self> {
        let entry = Entry::new("comp-gate.xfajk", "device_whitelist")?;

        // collect ids
        let whitelist_entries: HashSet<DeviceId> = device_tracker
            .devices
            .iter()
            .map(|(id, _)| id.clone())
            .collect();

        let whitelist = Whitelist {
            entry,
            device_tracker,
        };

        whitelist.store_whitelist(&whitelist_entries)?;

        Ok(whitelist)
    }

    /// Enforces the whitelist on the system.
    ///
    /// Iterates through all connected devices. If a device ID is not found in the
    /// stored whitelist, it is disabled. If it is found, it is enabled.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If all operations succeed.
    /// * `Err(anyhow::Error)` - If loading the whitelist or changing device state fails.
    pub fn apply_whitelist(&mut self) -> anyhow::Result<()> {
        let whitelist_entries = self.load_whitelist()?;

        for d in self.device_tracker.iter() {
            if !whitelist_entries.contains(&d.device_id) {
                self.device_tracker.set_device_state(
                    &d.device_id,
                    super::device_managment::DeviceState::Disable,
                )?;
            } else {
                self.device_tracker
                    .set_device_state(&d.device_id, super::device_managment::DeviceState::Enable)?;
            }
        }

        Ok(())
    }

    /// Adds a device ID to the authorized list.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The Instance ID of the device to authorize.
    pub fn whitelist_device(&mut self, device_id: &str) -> anyhow::Result<()> {
        let mut whitelist_entries = self.load_whitelist()?;

        let rc_id: Rc<str> = Rc::from(device_id);
        let id = DeviceId::from(rc_id);

        whitelist_entries.insert(id);

        self.store_whitelist(&whitelist_entries)?;

        Ok(())
    }

    /// Removes a device ID from the authorized list.
    ///
    /// Note: This does not immediately disable the device; `apply_whitelist` must be called.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The Instance ID of the device to de-authorize.
    pub fn blacklist_device(&mut self, device_id: &str) -> anyhow::Result<()> {
        let mut whitelist_entries = self.load_whitelist()?;
        let rc_id: Rc<str> = Rc::from(device_id);
        let id = DeviceId::from(rc_id);

        whitelist_entries.remove(&id);

        self.store_whitelist(&whitelist_entries)?;

        Ok(())
    }

    /// Loads the whitelist from the system keyring.
    ///
    /// # Returns
    ///
    /// * `Ok(HashSet<Rc<str>>)` - The set of authorized device IDs.
    /// * `Err(anyhow::Error)` - If the keyring cannot be accessed or data is corrupt.
    pub fn load_whitelist(&self) -> Result<HashSet<DeviceId>> {
        let hex = match self.entry.get_password() {
            Ok(s) => s,
            Err(e) => return Err(anyhow!("failed to read whitelist from keyring: {}", e)),
        };

        let bytes = decode_hex(&hex)?;
        let set = deserialize_set_bytes(&bytes)?;
        Ok(set)
    }

    /// Saves the whitelist to the system keyring.
    ///
    /// # Arguments
    ///
    /// * `set` - The set of device IDs to store.
    pub fn store_whitelist(&self, set: &HashSet<DeviceId>) -> Result<()> {
        let bytes = serialize_set_bytes(set);
        let hex = encode_hex(&bytes);
        self.entry
            .set_password(&hex)
            .map_err(|e| anyhow!("failed to write whitelist to keyring: {}", e))?;
        Ok(())
    }
}

// helper: serialize as [u64 len LE][bytes][u64 len][bytes]...
fn serialize_set_bytes(set: &HashSet<DeviceId>) -> Vec<u8> {
    let mut out = Vec::new();
    for s in set {
        let b = s.as_bytes();
        let len = b.len() as u64;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(b);
    }
    out
}

fn deserialize_set_bytes(bytes: &[u8]) -> Result<HashSet<DeviceId>> {
    let mut out = HashSet::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if i + 8 > bytes.len() {
            return Err(anyhow!(
                "corrupt whitelist data: unexpected EOF reading length"
            ));
        }
        let mut len_bytes = [0u8; 8];
        len_bytes.copy_from_slice(&bytes[i..i + 8]);
        let len = u64::from_le_bytes(len_bytes) as usize;
        i += 8;
        if i + len > bytes.len() {
            return Err(anyhow!(
                "corrupt whitelist data: unexpected EOF reading string"
            ));
        }
        let slice = &bytes[i..i + len];
        let s = str::from_utf8(slice)
            .map_err(|e| anyhow!("corrupt whitelist data: invalid UTF-8: {}", e))?;
        let rc = Rc::<str>::from(s.to_owned().into_boxed_str());
        let id = DeviceId::from(rc);
        out.insert(id);
        i += len;
    }
    Ok(out)
}

// small hex encoder/decoder to avoid extra deps
fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        let hi = HEX[(b >> 4) as usize];
        let lo = HEX[(b & 0x0f) as usize];
        s.push(hi as char);
        s.push(lo as char);
    }
    s
}

fn decode_hex(s: &str) -> Result<Vec<u8>> {
    let bytes = s.as_bytes();
    if bytes.len() % 2 != 0 {
        return Err(anyhow!("invalid hex string length"));
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_val(bytes[i])?;
        let lo = hex_val(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn hex_val(c: u8) -> Result<u8> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(10 + (c - b'a')),
        b'A'..=b'F' => Ok(10 + (c - b'A')),
        _ => Err(anyhow!("invalid hex char: {}", c as char)),
    }
}
