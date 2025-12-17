//! # Comp-Gate Library
//!
//! `comp-gate` is a library designed for managing and monitoring USB devices on Windows systems.
//! It provides functionality for:
//!
//! - Detecting USB device insertion and removal events.
//! - Managing device drivers (enabling/disabling devices).
//! - Whitelisting specific USB devices based on their hardware IDs.
//! - Interacting with the Windows SetupAPI for device information.
//!
//! This library is structured into modules handling errors, helper functions for IO and device management,
//! and core logic for device monitoring.

pub mod error;
pub mod helper;
