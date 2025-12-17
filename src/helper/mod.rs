//! # Helper Module
//!
//! This module aggregates various utility sub-modules that provide core functionality for the `comp-gate` application.
//! It includes:
//!
//! - `device_managment`: Tools for interacting with the Windows SetupAPI to manage device drivers and properties.
//! - `usb_connection_callback`: Event handling logic for USB device insertion and removal.
//! - `whitelist`: Functionality to manage and check against a list of authorized USB devices.
//! - `ioapi`: Input/Output utilities for handling configuration files and data persistence.

pub mod device_managment;
pub mod ioapi;
pub mod usb_connection_callback;
pub mod whitelist;
