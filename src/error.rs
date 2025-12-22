//! # Error Handling Module
//!
//! This module defines custom error types used throughout the `comp-gate` application.
//! It primarily wraps Windows system error codes into Rust-friendly enums and provides
//! specific error types for device polling, insertion, and property retrieval operations.

use thiserror::Error;
use windows_sys::Win32::{Devices::DeviceAndDriverInstallation::CR_SUCCESS, Foundation::*};

/// Represents various Windows System Error codes encountered during API calls.
///
/// This enum maps raw `u32` error codes returned by Windows APIs (like `GetLastError`)
/// to meaningful Rust variants. It covers common errors related to file I/O,
/// device management, and system resources.
#[derive(Error, Debug)]
pub enum Win32Error {
    /// The operation completed successfully (ERROR_SUCCESS).
    #[error("The operation completed successfully")]
    Success,

    /// The system cannot find the file specified (ERROR_FILE_NOT_FOUND).
    #[error("File not found")]
    FileNotFound,

    /// The system cannot find the path specified (ERROR_PATH_NOT_FOUND).
    #[error("Path not found")]
    PathNotFound,

    /// Access is denied (ERROR_ACCESS_DENIED).
    #[error("Access denied")]
    AccessDenied,

    /// The handle is invalid (ERROR_INVALID_HANDLE).
    #[error("Invalid handle")]
    InvalidHandle,

    /// Not enough storage is available to process this command (ERROR_NOT_ENOUGH_MEMORY).
    #[error("Not enough memory")]
    NotEnoughMemory,

    /// The parameter is incorrect (ERROR_INVALID_PARAMETER).
    #[error("Invalid parameter")]
    InvalidParameter,

    /// The data area passed to a system call is too small (ERROR_INSUFFICIENT_BUFFER).
    #[error("Insufficient buffer")]
    InsufficientBuffer,

    /// More data is available (ERROR_MORE_DATA).
    #[error("More data available")]
    MoreData,

    /// Overlapped I/O operation is in progress (ERROR_IO_PENDING).
    #[error("I/O pending")]
    IoPending,

    /// The operation was aborted (ERROR_OPERATION_ABORTED).
    #[error("Operation aborted")]
    OperationAborted,

    /// The process cannot access the file because it is being used by another process (ERROR_SHARING_VIOLATION).
    #[error("Sharing violation")]
    SharingViolation,

    /// The disk is full (ERROR_DISK_FULL).
    #[error("Disk full")]
    DiskFull,

    /// The semaphore timeout period has expired (ERROR_SEM_TIMEOUT) or Wait timeout (WAIT_TIMEOUT).
    #[error("Timeout")]
    Timeout,

    /// No more data is available (ERROR_NO_MORE_ITEMS).
    #[error("No more items")]
    NoMoreItems,

    /// The data is invalid (ERROR_INVALID_DATA).
    #[error("Invalid data")]
    InvalidData,

    /// Element not found (ERROR_NOT_FOUND).
    #[error("Not found")]
    NotFound,

    /// Cannot create a file when that file already exists (ERROR_ALREADY_EXISTS).
    #[error("Already exists")]
    AlreadyExists,

    /// The specified device does not exist (ERROR_DEV_NOT_EXIST).
    #[error("Device does not exist")]
    DeviceNotExist,

    /// Config manager error (ERROR_CONFIG_MANAGER_ERROR).
    #[error("Config manager error")]
    ConfigManagerError(#[from] ConfigManagerError),

    /// An unknown error code not explicitly mapped in this enum.
    #[error("Unknown error with code: {0}")]
    UnknownError(u32),
}

impl From<u32> for Win32Error {
    /// Converts a raw Windows error code (`u32`) into a `Win32Error` variant.
    ///
    /// # Arguments
    ///
    /// * `code` - The raw error code returned by `GetLastError()` or similar functions.
    ///
    /// # Example
    ///
    /// ```rust
    /// use comp_gate::error::Win32Error;
    /// use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;
    ///
    /// let error: Win32Error = ERROR_ACCESS_DENIED.into();
    /// assert!(matches!(error, Win32Error::AccessDenied));
    /// ```
    fn from(code: u32) -> Self {
        match code {
            ERROR_SUCCESS => Win32Error::Success, // ERROR_SUCCESS
            ERROR_FILE_NOT_FOUND => Win32Error::FileNotFound, // ERROR_FILE_NOT_FOUND
            ERROR_PATH_NOT_FOUND => Win32Error::PathNotFound, // ERROR_PATH_NOT_FOUND
            ERROR_ACCESS_DENIED => Win32Error::AccessDenied, // ERROR_ACCESS_DENIED
            ERROR_INVALID_HANDLE => Win32Error::InvalidHandle, // ERROR_INVALID_HANDLE
            ERROR_NOT_ENOUGH_MEMORY => Win32Error::NotEnoughMemory, // ERROR_NOT_ENOUGH_MEMORY
            ERROR_SHARING_VIOLATION => Win32Error::SharingViolation, // ERROR_SHARING_VIOLATION
            ERROR_INVALID_PARAMETER => Win32Error::InvalidParameter, // ERROR_INVALID_PARAMETER
            ERROR_DISK_FULL => Win32Error::DiskFull, // ERROR_DISK_FULL
            ERROR_SEM_TIMEOUT => Win32Error::Timeout, // ERROR_SEM_TIMEOUT (treated as timeout)
            ERROR_INSUFFICIENT_BUFFER => Win32Error::InsufficientBuffer, // ERROR_INSUFFICIENT_BUFFER
            ERROR_MORE_DATA => Win32Error::MoreData,                     // ERROR_MORE_DATA
            ERROR_OPERATION_ABORTED => Win32Error::OperationAborted,     // ERROR_OPERATION_ABORTED
            ERROR_IO_PENDING => Win32Error::IoPending,                   // ERROR_IO_PENDING
            ERROR_TIMEOUT => Win32Error::Timeout, // WAIT_TIMEOUT / ERROR_TIMEOUT
            ERROR_NO_MORE_ITEMS => Win32Error::NoMoreItems, // ERROR_NO_MORE_ITEMS (used by SetupDiEnumDeviceInfo)
            ERROR_INVALID_DATA => Win32Error::InvalidData,  // ERROR_INVALID_DATA
            ERROR_NOT_FOUND => Win32Error::NotFound,        // ERROR_NOT_FOUND
            ERROR_ALREADY_EXISTS => Win32Error::AlreadyExists, // ERROR_ALREADY_EXISTS
            ERROR_DEV_NOT_EXIST => Win32Error::DeviceNotExist, // ERROR_DEV_NOT_EXIST
            _ => Win32Error::UnknownError(code),
        }
    }
}

#[derive(Error, Debug)]
pub enum ConfigManagerError {
    #[error("Config manager success")]
    Success,

    #[error("Config manager instance device instance")]
    InvalidDeviceInstance,

    /// Config manager error (ERROR_CONFIG_MANAGER_ERROR).
    #[error("Config manager error {0}")]
    UnknownError(u32),
}

impl From<u32> for ConfigManagerError {
    fn from(code: u32) -> Self {
        match code {
            CR_SUCCESS => ConfigManagerError::Success,
            _ => ConfigManagerError::UnknownError(code),
        }
    }
}

/// Errors that can occur during the event polling loop.
#[derive(Error, Debug)]
pub enum PollEventError {
    /// A Windows API call failed.
    #[error("Win32 error occurred: {0}")]
    Win32Error(#[from] Win32Error),

    /// Failed to receive a message from a channel (e.g., when communicating with the UI thread).
    #[error("Thread receive error: {0}")]
    ThreadRecvError(#[from] std::sync::mpsc::TryRecvError),

    /// The thread has finished execution or was signaled to stop.
    #[error("Thread finished")]
    ThreadFinished,
}

/// Errors related to detecting and processing a new device insertion.
#[derive(Error, Debug)]
pub enum DeviceInsertionError {
    /// A Windows API call failed during device inspection.
    #[error("Win32 error occurred: {0}")]
    Win32Error(#[from] Win32Error),

    /// The detected device was filtered out because it is not a USB device.
    #[error("Device filtered out not a USB device")]
    DeviceFilteredNotUsb,
}

/// Errors encountered when retrieving string properties from a device.
#[derive(Error, Debug)]
pub enum DeviceStringPropertyError {
    /// A Windows API call failed while querying the property.
    #[error("Win32 error occurred: {0}")]
    Win32Error(#[from] Win32Error),

    /// The requested property exists but is not of a string type (REG_SZ).
    #[error("Property is not a string property")]
    PropertyNotString,
}
