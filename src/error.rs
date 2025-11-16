use thiserror::Error;
use windows_sys::Win32::Foundation::*;

#[derive(Error, Debug)]
pub enum Win32Error {
    #[error("The operation completed successfully")]
    Success,

    #[error("File not found")]
    FileNotFound,

    #[error("Path not found")]
    PathNotFound,

    #[error("Access denied")]
    AccessDenied,

    #[error("Invalid handle")]
    InvalidHandle,

    #[error("Not enough memory")]
    NotEnoughMemory,

    #[error("Invalid parameter")]
    InvalidParameter,

    #[error("Insufficient buffer")]
    InsufficientBuffer,

    #[error("More data available")]
    MoreData,

    #[error("I/O pending")]
    IoPending,

    #[error("Operation aborted")]
    OperationAborted,

    #[error("Sharing violation")]
    SharingViolation,

    #[error("Disk full")]
    DiskFull,

    #[error("Timeout")]
    Timeout,

    #[error("No more items")]
    NoMoreItems,

    #[error("Invalid data")]
    InvalidData,

    #[error("Not found")]
    NotFound,

    #[error("Already exists")]
    AlreadyExists,

    #[error("Unknown error with code: {0}")]
    UnknownError(u32),
}

impl From<u32> for Win32Error {
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
            ERROR_MORE_DATA => Win32Error::MoreData, // ERROR_MORE_DATA
            ERROR_OPERATION_ABORTED => Win32Error::OperationAborted, // ERROR_OPERATION_ABORTED
            ERROR_IO_PENDING => Win32Error::IoPending, // ERROR_IO_PENDING
            ERROR_TIMEOUT => Win32Error::Timeout, // WAIT_TIMEOUT / ERROR_TIMEOUT
            ERROR_NO_MORE_ITEMS => Win32Error::NoMoreItems, // ERROR_NO_MORE_ITEMS (used by SetupDiEnumDeviceInfo)
            ERROR_INVALID_DATA => Win32Error::InvalidData, // ERROR_INVALID_DATA
            ERROR_NOT_FOUND => Win32Error::NotFound, // ERROR_NOT_FOUND
            ERROR_ALREADY_EXISTS => Win32Error::AlreadyExists, // ERROR_ALREADY_EXISTS
            _ => Win32Error::UnknownError(code),
        }
    }
}