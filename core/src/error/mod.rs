/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
mod xflash;
mod xml;

use std::sync::PoisonError;

use thiserror::Error;

pub use self::xflash::{XFlashError, XFlashErrorKind};
pub use self::xml::{XmlError, XmlErrorKind};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    /// An error related to XFlash protocol (and its error codes)
    #[error("XFlash error: {0}")]
    XFlash(#[from] XFlashError),
    #[error("XML error: {0}")]
    Xml(#[from] XmlError),
    /// Generic Protocol error
    #[error("Protocol Error {0}")]
    Protocol(String),
    /// Connection specific error
    #[error("Connection Error: {0}")]
    Connection(String),
    /// Error related to I/O operations
    /// In particular with the connection backends
    #[error("I/O Error: {0}")]
    Io(String),
    /// Generic error that happens in Penumbra, can
    /// be used for anything
    #[error("Penumbra Error: {0}")]
    Penumbra(String),
    /// Error that takes a status code and formats it as hex.
    /// When dealing with statuses in general, use
    /// this, unless a more specific implementation
    /// is there (e.g. XFlash)
    #[error("{ctx}: Status is 0x{status:X}")]
    Status { ctx: String, status: u32 },
}

impl Error {
    pub fn io<S: Into<String>>(msg: S) -> Self {
        Error::Io(msg.into())
    }

    pub fn conn<S: Into<String>>(msg: S) -> Self {
        Error::Connection(msg.into())
    }

    pub fn proto<S: Into<String>>(msg: S) -> Self {
        Error::Protocol(msg.into())
    }

    pub fn penumbra<S: Into<String>>(msg: S) -> Self {
        Error::Penumbra(msg.into())
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::penumbra(value.to_string())
    }
}

impl From<nusb::Error> for Error {
    fn from(err: nusb::Error) -> Self {
        Error::io(err.to_string())
    }
}

impl<T> From<PoisonError<T>> for Error {
    fn from(e: PoisonError<T>) -> Self {
        Error::penumbra(format!("Lock poisoned: {}", e))
    }
}

#[cfg(feature = "libusb")]
impl From<rusb::Error> for Error {
    fn from(err: rusb::Error) -> Self {
        Error::io(err.to_string())
    }
}
