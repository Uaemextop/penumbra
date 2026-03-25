/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
mod builder;
#[cfg(not(feature = "no_exploits"))]
mod exploit_ops;
mod flash_ops;

use std::time::Duration;

use log::{error, info, warn};
use tokio::time::timeout;

use crate::connection::Connection;
use crate::connection::port::ConnectionType;
use crate::core::chip::{ChipInfo, chip_from_hw_code};
use crate::core::crypto::config::CryptoIO;
use crate::core::devinfo::{DevInfoData, DeviceInfo};
use crate::core::log_buffer::DeviceLog;
use crate::core::storage::Partition;
use crate::da::{DAFile, DAProtocol, DAType, XFlash, Xml};
use crate::error::{Error, Result};

pub use builder::DeviceBuilder;

/// Represents a connected MTK device.
///
/// This struct is the **main interface** for interacting with the device.
/// It handles initialization, entering DA mode, reading/writing partitions,
/// and accessing connection or protocol information.
///
/// # Lifecycle
/// 1. Construct via [`DeviceBuilder`].
/// 2. Call [`Device::init`] to handshake with the device.
/// 3. Optionally call [`Device::enter_da_mode`] to switch to DA protocol.
/// 4. Perform operations like `read_partition`, `write_partition`, etc.
pub struct Device {
    /// Device information and metadata, shared accross the whole crate.
    pub dev_info: DeviceInfo,
    /// Connection to the device via MTK port, null if DA protocol is used.
    connection: Option<Connection>,
    /// DA protocol handler, null if only preloader commands are used.
    protocol: Option<Box<dyn DAProtocol + Send>>,
    /// Whether the device is connected and initialized.
    connected: bool,
    /// Raw DA file data, if provided.
    da_data: Option<Vec<u8>>,
    /// Preloader data, if provided.
    preloader_data: Option<Vec<u8>>,
    /// Whether verbose logging is enabled.
    verbose: bool,
    /// Whether to log DA messages over USB.
    usb_log_channel: bool,
    /// Buffer to store DA log messages.
    device_log: DeviceLog,
}

impl Device {
    /// Initializes the device by performing handshake and retrieving device information.
    /// This must be called before any other operations.
    ///
    /// # Examples
    /// ```rust
    /// use penumbra::{DeviceBuilder, find_mtk_port};
    ///
    /// let mtk_port = find_mtk_port().await.ok_or("No MTK port found")?;
    /// let mut device = DeviceBuilder::default().with_mtk_port(mtk_port).build()?;
    ///
    /// device.init().await?;
    /// assert_eq!(device.connected, true);
    /// ```
    pub async fn init(&mut self) -> Result<()> {
        let mut conn = self
            .connection
            .take()
            .ok_or_else(|| Error::penumbra("Connection is not initialized."))?;

        conn.handshake().await?;

        let soc_id = conn.get_soc_id().await?;
        let meid = conn.get_meid().await?;
        let hw_code = conn.get_hw_code().await?;
        let target_config = conn.get_target_config().await?;

        let device_info =
            DevInfoData { soc_id, meid, hw_code, storage: None, partitions: vec![], target_config };

        self.dev_info.set_data(device_info).await;
        let chip = chip_from_hw_code(hw_code);
        if chip.hw_code() == 0x0000 {
            warn!("Unknown hardware code 0x{:04X}. Device might not work as expected.", hw_code);
            warn!("If you think this is incorrect, please report this hw code to the developers.");
        }

        self.dev_info.set_chip(chip);

        if self.da_data.is_some() {
            self.protocol = Some(self.init_da_protocol(conn).await?);
        } else {
            self.connection = Some(conn);
        }

        self.connected = true;

        Ok(())
    }

    /// Reinits the device connection based on the current connection type and optional DA info.
    /// This is useful for CLIs or scenarios where the Device instance needs to be reset.
    pub async fn reinit(&mut self, dev_info: DevInfoData) -> Result<()> {
        let mut conn = self
            .connection
            .take()
            .ok_or_else(|| Error::penumbra("Connection is not initialized."))?;

        self.dev_info.set_data(dev_info).await;
        self.dev_info.set_chip(chip_from_hw_code(self.dev_info.hw_code().await));

        match conn.connection_type {
            ConnectionType::Preloader | ConnectionType::Brom => {
                // If we already are in preloader/brom mode, we either handshake again or timeout
                let handshake_result = timeout(Duration::from_secs(3), conn.handshake()).await;
                match handshake_result {
                    Ok(result) => result?,
                    Err(_) => {
                        return Err(Error::conn(
                            "Handshake timed out. Reset the device and try again.",
                        ));
                    }
                }
            }
            ConnectionType::Da => {
                self.protocol = Some(self.init_da_protocol(conn).await?);
            }
        };

        self.connected = true;

        Ok(())
    }

    /// Enters DA mode by uploading the DA to the device.
    /// This is required for performing DA protocol operations.
    /// After entering DA mode, the device's partition information is read and stored in `dev_info`.
    ///
    /// # Examples
    /// ```rust
    /// use penumbra::{DeviceBuilder, find_mtk_port};
    ///
    /// let mtk_port = find_mtk_port().await.ok_or("No MTK port found")?;
    /// let da_data = std::fs::read("path/to/da/file").expect("Failed to read DA file");
    /// let mut device =
    ///     DeviceBuilder::default().with_mtk_port(mtk_port).with_da_data(da_data).build()?;
    ///
    /// device.init().await?;
    /// device.enter_da_mode().await?;
    /// assert_eq!(device.get_connection()?.connection_type, ConnectionType::Da);
    /// ```
    pub async fn enter_da_mode(&mut self) -> Result<()> {
        if !self.connected {
            return Err(Error::conn("Device is not connected. Call init() first."));
        }

        let conn_type = self.get_connection()?.connection_type;

        if self.protocol.is_none() {
            let conn =
                self.connection.take().ok_or_else(|| Error::conn("No connection available."))?;
            let protocol = self.init_da_protocol(conn).await?;
            self.protocol = Some(protocol);
        }

        let protocol = self.protocol.as_mut().unwrap();
        if conn_type != ConnectionType::Da {
            protocol.upload_da().await?;
            self.set_connection_type(ConnectionType::Da)?;
        }

        // Fallback to ensure we always have the partitions available.
        self.get_partitions().await;
        Ok(())
    }

    /// Internal helper to ensure the device enters DA mode before performing DA operations.
    async fn ensure_da_mode(&mut self) -> Result<&mut (dyn DAProtocol + Send)> {
        if !self.connected {
            return Err(Error::conn("Device is not connected. Call init() first."));
        }

        if self.protocol.is_none() {
            return Err(Error::conn("DA protocol is not initialized. DA data might be missing."));
        }

        if self.get_connection()?.connection_type != ConnectionType::Da {
            info!("Not in DA mode, entering now...");
            self.enter_da_mode().await?;
        }

        Ok(self.get_protocol().unwrap())
    }

    async fn init_da_protocol(&mut self, conn: Connection) -> Result<Box<dyn DAProtocol + Send>> {
        let da_bytes = self.da_data.clone().ok_or_else(|| {
            Error::conn("DA protocol is not initialized and no DA file was provided.")
        })?;

        let da_file = DAFile::parse_da(&da_bytes)?;
        let hw_code = self.dev_info.hw_code().await;
        let da = da_file.get_da_from_hw_code(hw_code).ok_or_else(|| {
            Error::penumbra(format!("No compatible DA for hardware code 0x{:04X}", hw_code))
        })?;

        let protocol: Box<dyn DAProtocol + Send> = match da.da_type {
            DAType::V5 => Box::new(XFlash::new(
                conn,
                da,
                self.dev_info.clone(),
                self.preloader_data.clone(),
                self.verbose,
                self.usb_log_channel,
                self.device_log.clone(),
            )),
            DAType::V6 => Box::new(Xml::new(
                conn,
                da,
                self.dev_info.clone(),
                self.verbose,
                self.usb_log_channel,
                self.device_log.clone(),
            )),
            _ => return Err(Error::penumbra("Unsupported DA type")),
        };

        self.get_partitions().await;
        Ok(protocol)
    }

    /// Returns the resolved [`ChipInfo`] for this device.
    pub fn chip(&self) -> &'static ChipInfo {
        self.dev_info.chip()
    }

    /// Returns a reference to the device log buffer
    pub fn device_log(&self) -> &DeviceLog {
        &self.device_log
    }

    /// Gets a mutable reference to the active connection.
    /// If the device is in DA mode, it retrieves the connection from the DA protocol.
    pub fn get_connection(&mut self) -> Result<&mut Connection> {
        match (&mut self.connection, &mut self.protocol) {
            (Some(conn), _) => Ok(conn),
            (None, Some(proto)) => Ok(proto.get_connection()),
            (None, None) => Err(Error::conn("No active connection available.")),
        }
    }

    /// Sets the connection type of the active connection.
    /// Note that this does not change the actual connection state, only the type metadata.
    /// This is mainly used for reinitialization after entering DA mode.
    pub fn set_connection_type(&mut self, conn_type: ConnectionType) -> Result<()> {
        let conn = self.get_connection()?;
        conn.connection_type = conn_type;
        Ok(())
    }

    /// Gets a mutable reference to the DA protocol handler, if available.
    /// Returns `None` if the device is not in DA mode.
    pub fn get_protocol(&mut self) -> Option<&mut (dyn DAProtocol + Send)> {
        self.protocol.as_deref_mut()
    }

    /// Retrieves the list of partitions from the device.
    /// If partitions have already been fetched, returns the cached list.
    /// Otherwise, queries the DA protocol for partition information and caches the result.
    ///
    /// Returns an empty list if no DA protocol is available.
    ///
    /// # Examples
    /// ```rust
    /// use penumbra::{DeviceBuilder, find_mtk_port};
    ///
    /// let mtk_port = find_mtk_port().await.ok_or("No MTK port found")?;
    /// let da_data = std::fs::read("path/to/da/file").expect("Failed to read DA file");
    /// let mut device =
    ///     DeviceBuilder::default().with_mtk_port(mtk_port).with_da_data(da_data).build()?;
    ///
    /// device.init().await?;
    /// device.enter_da_mode().await?;
    /// let partitions = device.get_partitions().await;
    /// for part in &partitions {
    ///     println!("{}: size={}", part.name, part.size);
    /// }
    /// ```
    pub async fn get_partitions(&mut self) -> Vec<Partition> {
        let cached = self.dev_info.partitions().await;
        if !cached.is_empty() {
            return cached;
        }

        let protocol = match self.get_protocol() {
            Some(p) => p,
            None => return Vec::new(),
        };

        info!("Retrieving partition information...");
        let partitions = protocol.get_partitions().await;

        self.dev_info.set_partitions(partitions.clone()).await;

        partitions
    }
}

#[async_trait::async_trait]
impl CryptoIO for Device {
    async fn read32(&mut self, addr: u32) -> u32 {
        let Some(protocol) = self.get_protocol() else {
            error!("No protocol available for read32 at 0x{:08X}!", addr);
            return 0;
        };

        match protocol.read32(addr).await {
            Ok(val) => val,
            Err(e) => {
                error!("Failed to read32 from protocol at 0x{:08X}: {}", addr, e);
                0
            }
        }
    }

    async fn write32(&mut self, addr: u32, val: u32) {
        let Some(protocol) = self.get_protocol() else {
            error!("No protocol available for write32 at 0x{:08X}!", addr);
            return;
        };

        if let Err(e) = protocol.write32(addr, val).await {
            error!("Failed to write32 to protocol at 0x{:08X}: {}", addr, e);
        }
    }
}
