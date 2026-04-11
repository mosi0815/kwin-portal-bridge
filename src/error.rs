use std::sync::PoisonError;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum XCapError {
    #[error("{0}")]
    Error(String),
    #[error("StdSyncPoisonError {0}")]
    StdSyncPoisonError(String),
    #[cfg(target_os = "linux")]
    #[error(transparent)]
    XcbError(#[from] xcb::Error),
    #[cfg(target_os = "linux")]
    #[error(transparent)]
    XcbConnError(#[from] xcb::ConnError),
    #[cfg(target_os = "linux")]
    #[error(transparent)]
    ImageImageError(#[from] image::ImageError),
    #[cfg(target_os = "linux")]
    #[error(transparent)]
    StdStringFromUtf8Error(#[from] std::string::FromUtf8Error),
    #[cfg(target_os = "linux")]
    #[error(transparent)]
    StdIOError(#[from] std::io::Error),
    #[cfg(target_os = "linux")]
    #[error(transparent)]
    StdMPSCRecvError(#[from] std::sync::mpsc::RecvError),
    #[cfg(target_os = "linux")]
    #[error(transparent)]
    StdTimeSystemTimeError(#[from] std::time::SystemTimeError),

}

impl XCapError {
    pub fn new<S: ToString>(err: S) -> Self {
        XCapError::Error(err.to_string())
    }
}

pub type XCapResult<T> = Result<T, XCapError>;

impl<T> From<PoisonError<T>> for XCapError {
    fn from(value: PoisonError<T>) -> Self {
        XCapError::StdSyncPoisonError(value.to_string())
    }
}