/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
use thiserror::Error;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum XmlErrorKind {
    Unknown,
    UnsupportedCmd,
    Cancel,
}

#[derive(Debug, Error)]
#[error("XML Error: {message}")]
pub struct XmlError {
    pub message: String,
    pub kind: XmlErrorKind,
}

impl XmlError {
    pub fn new<S: Into<String>>(msg: S, kind: XmlErrorKind) -> Self {
        XmlError { message: msg.into(), kind }
    }

    pub fn from_message(resp: &[u8]) -> Self {
        let msg = std::str::from_utf8(resp).unwrap_or("Invalid UTF-8");

        let msg = msg.trim_end_matches('\0');

        match msg {
            "ERR!UNSUPPORTED" => XmlError::new("Unsupported command", XmlErrorKind::UnsupportedCmd),
            "ERR!CANCEL" => XmlError::new("Cancelled", XmlErrorKind::Cancel),
            _ => XmlError::new(msg, XmlErrorKind::Unknown),
        }
    }
}
