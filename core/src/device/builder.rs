/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
use super::Device;
use crate::connection::Connection;
use crate::core::devinfo::DeviceInfo;
use crate::core::log_buffer::DeviceLog;
use crate::error::{Error, Result};

/// A builder for creating a new [`Device`].
///
/// This struct allows for configuring various parameters before constructing the device instance.
/// You can optionally (but suggested) provide DA data to enable DA protocol support.
/// When no DA data is provided, only preloader commands will be available, limiting functionality.
/// A MTKPort must be provided to build the device.
///
/// # Example
/// ```rust
/// use penumbra::{Device, DeviceBuilder, find_mtk_port};
///
/// let mtk_port = find_mtk_port().await.ok_or("No MTK port found")?;
/// let da_data = std::fs::read("path/to/da/file").expect("Failed to read DA file");
/// let device =
///     DeviceBuilder::default().with_mtk_port(your_mtk_port).with_da_data(your_da_data).build()?;
/// ```
#[derive(Default)]
pub struct DeviceBuilder {
    /// MTK port to use during connection. It can be either a serial port or a USB port.
    /// This field is required to build a Device.
    mtk_port: Option<Box<dyn crate::connection::port::MTKPort>>,
    /// DA data to use for the device. This field is optional, but recommended.
    /// If not provided, the device will not be able to use DA protocol, and instead
    /// Only preloader commands will be available.
    da_data: Option<Vec<u8>>,
    /// Preloader data to use for the device. This field is optional.
    /// If provided, it can be used to extract EMI settings or other information.
    /// Only needed if told to do so, like when the device is in BROM mode.
    preloader_data: Option<Vec<u8>>,
    /// Whether to enable verbose logging.
    verbose: bool,
    /// Whether to use USB as the DA log channel instead of UART.
    /// When enabled, DA log messages are captured into a [`DeviceLog`] buffer
    /// instead of being sent over UART.
    usb_log_channel: bool,
    /// A buffer to store DA log messages when `usb_log_channel` is enabled.
    /// This allows for capturing logs from devices without needing UART.
    device_log: Option<DeviceLog>,
}

impl DeviceBuilder {
    /// Assigns the MTK port to be used for the device connection.
    pub fn with_mtk_port(mut self, port: Box<dyn crate::connection::port::MTKPort>) -> Self {
        self.mtk_port = Some(port);
        self
    }

    /// Assigns the DA data to be used for the device.
    pub fn with_da_data(mut self, data: Vec<u8>) -> Self {
        self.da_data = Some(data);
        self
    }

    /// Assigns the preloader data to be used for the device.
    pub fn with_preloader(mut self, data: Vec<u8>) -> Self {
        self.preloader_data = Some(data);
        self
    }

    /// Enables verbose logging mode.
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Enable USB logging
    pub fn with_usb_log_channel(mut self, enabled: bool) -> Self {
        self.usb_log_channel = enabled;
        self
    }

    /// Assigns a [`DeviceLog`] buffer to capture DA log messages
    /// when `usb_log_channel` is enabled.
    /// This allows to attach an optional Callback to the log buffer
    /// (i.e. to save to a file).
    pub fn with_device_log(mut self, log: DeviceLog) -> Self {
        self.device_log = Some(log);
        self
    }

    /// Builds and returns a new `Device` instance.
    pub fn build(self) -> Result<Device> {
        let connection = self.mtk_port.map(Connection::new);

        if connection.is_none() {
            return Err(Error::penumbra("MTK port must be provided to build a Device."));
        }

        let device_log = self.device_log.unwrap_or_default();

        Ok(Device {
            dev_info: DeviceInfo::default(),
            connection,
            protocol: None,
            connected: false,
            da_data: self.da_data,
            preloader_data: self.preloader_data,
            verbose: self.verbose,
            usb_log_channel: self.usb_log_channel,
            device_log,
        })
    }
}
