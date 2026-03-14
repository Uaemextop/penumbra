/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/

use std::fmt::Debug;

use log::{debug, error, info};

use crate::connection::backend::*;
use crate::error::Result;

/// List of all ports available for connecting and what mode they refer to.
/// Add more entries here for vendor specific ports.
///
/// These match the hardware IDs from the open-source MTK USB CDC ACM driver
/// (see drivers/windows/opensource/mtk_usb2ser.inf) and the device IDs
/// used by mtkclient.
#[rustfmt::skip]
pub const KNOWN_PORTS: &[(u16, u16, ConnectionType)] = &[
    // MediaTek core boot modes
    (0x0E8D, 0x0003, ConnectionType::Brom),      // MediaTek USB Port (BROM)
    (0x0E8D, 0x6000, ConnectionType::Preloader), // MediaTek USB Port (Preloader)
    (0x0E8D, 0x2000, ConnectionType::Preloader), // MediaTek USB Port (Preloader)
    (0x0E8D, 0x2001, ConnectionType::Da),        // MediaTek USB Port (DA)
    (0x0E8D, 0x20FF, ConnectionType::Preloader), // MediaTek USB Port (Preloader)
    (0x0E8D, 0x3000, ConnectionType::Preloader), // MediaTek USB Port (Preloader)
    // MediaTek Meta Mode / VCOM (additional IDs from MTK driver INF)
    (0x0E8D, 0x2006, ConnectionType::Da),        // MediaTek VCOM (Meta Mode)
    (0x0E8D, 0x2007, ConnectionType::Da),        // MediaTek VCOM (Meta Mode)
    // LG
    (0x1004, 0x6000, ConnectionType::Preloader), // LG USB Port (Preloader)
    // OPPO
    (0x22D9, 0x0006, ConnectionType::Preloader), // OPPO USB Port (Preloader)
    // Sony
    (0x0FCE, 0xF200, ConnectionType::Brom),      // Sony USB Port (BROM)
    (0x0FCE, 0xD1E9, ConnectionType::Brom),      // Sony USB Port (BROM XA1)
    (0x0FCE, 0xD1E2, ConnectionType::Brom),      // Sony USB Port (BROM)
    (0x0FCE, 0xD1EC, ConnectionType::Brom),      // Sony USB Port (BROM L1)
    (0x0FCE, 0xD1DD, ConnectionType::Brom),      // Sony USB Port (BROM F3111)
];

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum ConnectionType {
    Brom,
    Preloader,
    Da,
}

#[async_trait::async_trait]
pub trait MTKPort: Send + Debug {
    async fn open(&mut self) -> Result<()>;
    async fn close(&mut self) -> Result<()>;
    async fn read_exact(&mut self, buf: &mut [u8]) -> Result<usize>;
    async fn write_all(&mut self, buf: &[u8]) -> Result<()>;
    async fn flush(&mut self) -> Result<()>;

    async fn handshake(&mut self) -> Result<()>;
    fn get_connection_type(&self) -> ConnectionType;
    fn get_baudrate(&self) -> u32;
    fn get_port_name(&self) -> String;

    async fn find_device() -> Result<Option<Self>>
    where
        Self: Sized;

    // Only for USB ports
    async fn ctrl_out(
        &mut self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
    ) -> Result<()>;
    async fn ctrl_in(
        &mut self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        len: usize,
    ) -> Result<Vec<u8>>;
}

pub async fn find_mtk_port() -> Option<Box<dyn MTKPort>> {
    debug!("Searching for MTK device...");

    // Default NUSB backend
    #[cfg(not(any(feature = "libusb", feature = "serial")))]
    let port = UsbMTKPort::find_device().await;

    // LibUSB backend
    #[cfg(feature = "libusb")]
    let port = UsbMTKPort::find_device().await;

    // Serial backend, not ideal since some features (i.e. linecoding) aren't available.
    #[cfg(feature = "serial")]
    let port = SerialMTKPort::find_device().await;

    match port {
        Ok(Some(mut port)) => {
            info!("Found MTK device: {}", port.get_port_name());
            match port.open().await {
                Ok(()) => Some(Box::new(port)),
                Err(e) => {
                    error!("Failed to open MTK device: {}", e);
                    #[cfg(windows)]
                    error!(
                        "On Windows, ensure the correct USB drivers are installed. \
                         See docs/WINDOWS.md or use the driver installer in drivers/windows/."
                    );
                    None
                }
            }
        }
        Ok(None) => {
            debug!("No MTK device found");
            None
        }
        Err(e) => {
            error!("Error searching for MTK device: {}", e);
            None
        }
    }
}
