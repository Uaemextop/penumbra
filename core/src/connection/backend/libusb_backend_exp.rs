/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/

use std::sync::Arc;
use std::time::Duration;

use log::{debug, error, info, warn};
use rusb::{Context, Device, DeviceHandle, Direction, Recipient, RequestType, UsbContext};
use tokio::sync::Mutex;
use tokio::task::spawn_blocking;
use tokio::time::sleep;

use crate::connection::port::{ConnectionType, KNOWN_PORTS, MTKPort};
use crate::error::{Error, Result};

/// Default timeout for USB operations
const DEFAULT_TIMEOUT: Duration = Duration::from_millis(5000);

/// Short timeout for handshake operations
const HANDSHAKE_TIMEOUT: Duration = Duration::from_millis(100);

/// CDC Class-specific request codes
const CDC_SET_LINE_CODING: u8 = 0x20;
const CDC_SET_CONTROL_LINE_STATE: u8 = 0x22;

/// Control line state: DTR | RTS
const CDC_CONTROL_LINE_STATE: u16 = 0x03;

/// CDC control interface number
const CDC_CONTROL_INTERFACE: u8 = 0;

/// CDC data interface number
const CDC_DATA_INTERFACE: u8 = 1;

fn build_line_coding(baudrate: u32) -> [u8; 7] {
    let baud_bytes = baudrate.to_le_bytes();
    [
        baud_bytes[0],
        baud_bytes[1],
        baud_bytes[2],
        baud_bytes[3],
        0x00, // 1 stop bit
        0x00, // No parity
        0x08, // 8 data bits
    ]
}

#[derive(Debug, Clone, Copy)]
struct BulkEndpoints {
    in_addr: u8,
    in_max_packet_size: usize,
    out_addr: u8,
    out_max_packet_size: usize,
}

pub struct UsbMTKPort {
    vid: u16,
    pid: u16,
    bus_number: u8,
    device_address: u8,
    /// Device handle (None when closed)
    handle: Option<Arc<Mutex<DeviceHandle<Context>>>>,
    baudrate: u32,
    connection_type: ConnectionType,
    is_open: bool,
    port_name: String,
    endpoints: BulkEndpoints,
}

impl std::fmt::Debug for UsbMTKPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsbMTKPort")
            .field("vid", &format_args!("0x{:04X}", self.vid))
            .field("pid", &format_args!("0x{:04X}", self.pid))
            .field("connection_type", &self.connection_type)
            .field("baudrate", &self.baudrate)
            .field("is_open", &self.is_open)
            .field("port_name", &self.port_name)
            .finish()
    }
}

impl UsbMTKPort {
    fn find_bulk_endpoints(device: &Device<Context>) -> Option<BulkEndpoints> {
        let config = device.active_config_descriptor().ok()?;

        let mut in_addr = None;
        let mut in_size = None;
        let mut out_addr = None;
        let mut out_size = None;

        for interface in config.interfaces() {
            for desc in interface.descriptors() {
                for endpoint in desc.endpoint_descriptors() {
                    if endpoint.transfer_type() != rusb::TransferType::Bulk {
                        continue;
                    }

                    match endpoint.direction() {
                        rusb::Direction::In if in_addr.is_none() => {
                            in_addr = Some(endpoint.address());
                            in_size = Some(endpoint.max_packet_size() as usize);
                            debug!(
                                "Found bulk IN endpoint: 0x{:02X}, max packet size: {}",
                                endpoint.address(),
                                endpoint.max_packet_size()
                            );
                        }
                        rusb::Direction::Out if out_addr.is_none() => {
                            out_addr = Some(endpoint.address());
                            out_size = Some(endpoint.max_packet_size() as usize);
                            debug!(
                                "Found bulk OUT endpoint: 0x{:02X}, max packet size: {}",
                                endpoint.address(),
                                endpoint.max_packet_size()
                            );
                        }
                        _ => {}
                    }

                    if in_addr.is_some() && out_addr.is_some() {
                        break;
                    }
                }
            }
        }

        Some(BulkEndpoints {
            in_addr: in_addr?,
            in_max_packet_size: in_size?,
            out_addr: out_addr?,
            out_max_packet_size: out_size?,
        })
    }

    pub fn from_device(device: Device<Context>) -> Option<Self> {
        let descriptor = match device.device_descriptor() {
            Ok(d) => d,
            Err(e) => {
                debug!("Failed to get device descriptor: {:?}", e);
                return None;
            }
        };

        let vid = descriptor.vendor_id();
        let pid = descriptor.product_id();

        let connection_type = KNOWN_PORTS
            .iter()
            .find(|&&(kvid, kpid, _)| kvid == vid && kpid == pid)
            .map(|&(_, _, ct)| ct)?;

        debug!("Found known MTK device {:04X}:{:04X} ({:?})", vid, pid, connection_type);

        let endpoints = Self::find_bulk_endpoints(&device)?;

        let baudrate = match connection_type {
            ConnectionType::Brom => 115_200,
            ConnectionType::Preloader | ConnectionType::Da => 921_600,
        };

        let port_name = format!("USB:{:04X}:{:04X}", vid, pid);

        Some(Self {
            vid,
            pid,
            bus_number: device.bus_number(),
            device_address: device.address(),
            handle: None,
            baudrate,
            connection_type,
            is_open: false,
            port_name,
            endpoints,
        })
    }

    fn setup_cdc(handle: &DeviceHandle<Context>, baudrate: u32) -> Result<()> {
        let request_type =
            rusb::request_type(Direction::Out, RequestType::Class, Recipient::Interface);

        let line_coding = build_line_coding(baudrate);

        debug!("Setting CDC line coding for {} baud", baudrate);

        if let Err(e) = handle.write_control(
            request_type,
            CDC_SET_LINE_CODING,
            0,
            CDC_CONTROL_INTERFACE as u16,
            &line_coding,
            Duration::from_millis(100),
        ) {
            debug!("CDC Set Line Coding failed (may be OK): {:?}", e);
        }

        if let Err(e) = handle.write_control(
            request_type,
            CDC_SET_CONTROL_LINE_STATE,
            CDC_CONTROL_LINE_STATE,
            CDC_CONTROL_INTERFACE as u16,
            &[],
            Duration::from_millis(100),
        ) {
            debug!("CDC Set Control Line State failed (may be OK): {:?}", e);
        }

        debug!("CDC setup completed for {} baud", baudrate);
        Ok(())
    }

    fn claim_interface_sync(handle: &DeviceHandle<Context>, interface: u8) -> Result<()> {
        #[cfg(not(target_os = "windows"))]
        {
            if handle.set_auto_detach_kernel_driver(true).is_err() {
                match handle.kernel_driver_active(interface) {
                    Ok(true) => {
                        debug!("Detaching kernel driver from interface {}", interface);
                        if let Err(e) = handle.detach_kernel_driver(interface) {
                            if e != rusb::Error::NotFound && e != rusb::Error::NotSupported {
                                error!(
                                    "Failed to detach kernel driver on interface {}: {:?}",
                                    interface, e
                                );
                                return Err(Error::io(format!(
                                    "Failed to detach kernel driver: {:?}",
                                    e
                                )));
                            }
                        }
                    }
                    Ok(false) => {
                        debug!("No kernel driver active on interface {}", interface);
                    }
                    Err(e) => {
                        if e != rusb::Error::NotSupported {
                            warn!("Could not check kernel driver status: {:?}", e);
                        }
                    }
                }
            }
        }

        handle.claim_interface(interface).map_err(|e| {
            error!("Failed to claim interface {}: {:?}", interface, e);
            Error::io(format!("Failed to claim interface {}: {:?}", interface, e))
        })?;

        debug!("Claimed interface {}", interface);
        Ok(())
    }

    async fn bulk_read(&self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        let handle = self.handle.as_ref().ok_or_else(|| Error::io("Port not open"))?;
        let handle = handle.clone();
        let endpoint = self.endpoints.in_addr;
        let len = buf.len();

        let result = spawn_blocking(move || {
            let handle = handle.blocking_lock();
            let mut temp = vec![0u8; len];

            match handle.read_bulk(endpoint, &mut temp, timeout) {
                Ok(n) => Ok((temp, n)),
                Err(rusb::Error::Timeout) => Err(Error::io("USB bulk read timeout")),
                Err(rusb::Error::Pipe) => Err(Error::io("USB endpoint halted")),
                Err(rusb::Error::NoDevice) => Err(Error::io("USB device disconnected")),
                Err(e) => Err(Error::io(format!("USB bulk read error: {:?}", e))),
            }
        })
        .await
        .map_err(|e| Error::io(format!("Bulk read task panicked: {:?}", e)))??;

        let (temp, n) = result;
        if n > 0 {
            buf[..n].copy_from_slice(&temp[..n]);
        }
        Ok(n)
    }

    async fn bulk_write(&self, buf: &[u8], timeout: Duration) -> Result<usize> {
        let handle = self.handle.as_ref().ok_or_else(|| Error::io("Port not open"))?;
        let handle = handle.clone();
        let endpoint = self.endpoints.out_addr;
        let data = buf.to_vec();

        spawn_blocking(move || {
            let handle = handle.blocking_lock();

            match handle.write_bulk(endpoint, &data, timeout) {
                Ok(n) => Ok(n),
                Err(rusb::Error::Timeout) => Err(Error::io("USB bulk write timeout")),
                Err(rusb::Error::Pipe) => Err(Error::io("USB endpoint halted")),
                Err(rusb::Error::NoDevice) => Err(Error::io("USB device disconnected")),
                Err(e) => Err(Error::io(format!("USB bulk write error: {:?}", e))),
            }
        })
        .await
        .map_err(|e| Error::io(format!("Bulk write task panicked: {:?}", e)))?
    }
}

#[async_trait::async_trait]
impl MTKPort for UsbMTKPort {
    async fn open(&mut self) -> Result<()> {
        if self.is_open {
            debug!("Port {} already open", self.port_name);
            return Ok(());
        }

        info!("Opening USB MTK port: {}", self.port_name);

        let vid = self.vid;
        let pid = self.pid;
        let bus = self.bus_number;
        let addr = self.device_address;
        let baudrate = self.baudrate;

        let handle = spawn_blocking(move || -> Result<DeviceHandle<Context>> {
            let context = Context::new()
                .map_err(|e| Error::io(format!("Failed to create USB context: {:?}", e)))?;

            let devices = context
                .devices()
                .map_err(|e| Error::io(format!("Failed to enumerate devices: {:?}", e)))?;

            for device in devices.iter() {
                if device.bus_number() == bus && device.address() == addr {
                    let desc = device
                        .device_descriptor()
                        .map_err(|e| Error::io(format!("Failed to get descriptor: {:?}", e)))?;

                    if desc.vendor_id() == vid && desc.product_id() == pid {
                        let handle = device
                            .open()
                            .map_err(|e| Error::io(format!("Failed to open device: {:?}", e)))?;

                        Self::claim_interface_sync(&handle, CDC_CONTROL_INTERFACE)?;
                        Self::claim_interface_sync(&handle, CDC_DATA_INTERFACE)?;

                        #[cfg(target_os = "windows")]
                        Self::setup_cdc(&handle, baudrate)?;

                        return Ok(handle);
                    }
                }
            }

            Err(Error::io("Device not found"))
        })
        .await
        .map_err(|e| Error::io(format!("Open task panicked: {:?}", e)))??;

        let handle = Arc::new(Mutex::new(handle));

        self.handle = Some(handle);
        self.is_open = true;

        info!(
            "Opened USB MTK port: {} (endpoints: IN=0x{:02X}, OUT=0x{:02X}, baud: {})",
            self.port_name, self.endpoints.in_addr, self.endpoints.out_addr, self.baudrate
        );

        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        if !self.is_open {
            return Ok(());
        }

        info!("Closing USB MTK port: {}", self.port_name);

        // Drop the handle, this releases interfaces automatically
        self.handle = None;
        self.is_open = false;

        info!("Closed USB MTK port: {}", self.port_name);

        Ok(())
    }

    async fn read_exact(&mut self, buf: &mut [u8]) -> Result<usize> {
        if !self.is_open {
            return Err(Error::io("Port is not open"));
        }

        let mut total_read = 0;

        while total_read < buf.len() {
            match self.bulk_read(&mut buf[total_read..], DEFAULT_TIMEOUT).await {
                Ok(0) => {
                    sleep(Duration::from_millis(1)).await;
                    continue;
                }
                Ok(n) => {
                    total_read += n;
                }
                Err(e) => {
                    if total_read > 0 {
                        warn!(
                            "Partial read ({}/{} bytes) before error: {:?}",
                            total_read,
                            buf.len(),
                            e
                        );
                        return Ok(total_read);
                    }
                    return Err(e);
                }
            }
        }

        Ok(total_read)
    }

    async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        if !self.is_open {
            return Err(Error::io("Port is not open"));
        }

        let mut total_written = 0;

        while total_written < buf.len() {
            match self.bulk_write(&buf[total_written..], DEFAULT_TIMEOUT).await {
                Ok(n) if n > 0 => {
                    total_written += n;
                }
                Ok(_) => {
                    sleep(Duration::from_millis(1)).await;
                }
                Err(e) => {
                    error!("Write failed after {}/{} bytes: {:?}", total_written, buf.len(), e);
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    async fn handshake(&mut self) -> Result<()> {
        // DA mode doesn't require handshake
        if self.connection_type == ConnectionType::Da {
            return Ok(());
        }

        debug!(
            "Starting handshake (connection type: {:?}, baud: {})",
            self.connection_type, self.baudrate
        );

        // For non-BROM connections, send an initial 0xA0 to wake up the device
        if self.connection_type != ConnectionType::Brom {
            debug!("BROM mode: sending initial wake-up byte");
            self.write_all(&[0xA0]).await?;
        }

        const HANDSHAKE_CMD: [u8; 4] = [0xA0, 0x0A, 0x50, 0x05];
        const HANDSHAKE_RSP: [u8; 4] = [0x5F, 0xF5, 0xAF, 0xFA];

        let mut step = 0;
        let mut retry_count = 0;
        const MAX_RETRIES: usize = 100;

        while step < HANDSHAKE_CMD.len() {
            self.write_all(&[HANDSHAKE_CMD[step]]).await?;

            let mut response = [0u8; 1];

            match self.bulk_read(&mut response, HANDSHAKE_TIMEOUT).await {
                Ok(1) => {
                    let byte = response[0];

                    if byte == HANDSHAKE_CMD[0] && step == 0 {
                        debug!("Device already handshaken (echoed 0xA0)");
                        return Ok(());
                    }

                    if byte == HANDSHAKE_RSP[step] {
                        debug!(
                            "Handshake step {}: sent 0x{:02X}, got 0x{:02X} (OK)",
                            step, HANDSHAKE_CMD[step], byte
                        );
                        step += 1;
                        retry_count = 0;
                    } else {
                        debug!(
                            "Handshake step {}: sent 0x{:02X}, expected 0x{:02X}, got 0x{:02X} (retry)",
                            step, HANDSHAKE_CMD[step], HANDSHAKE_RSP[step], byte
                        );
                        step = 0;
                        retry_count += 1;
                        sleep(Duration::from_millis(5)).await;
                    }
                }
                Ok(0) | Err(_) => {
                    retry_count += 1;
                    if retry_count < MAX_RETRIES {
                        sleep(Duration::from_millis(10)).await;
                        continue;
                    }
                }
                Ok(n) => {
                    debug!("Unexpected read size: {} bytes", n);
                    retry_count += 1;
                }
            }

            if retry_count >= MAX_RETRIES {
                return Err(Error::io(format!(
                    "Handshake failed after {} retries at step {}",
                    MAX_RETRIES, step
                )));
            }
        }

        Ok(())
    }

    fn get_connection_type(&self) -> ConnectionType {
        self.connection_type
    }

    fn get_baudrate(&self) -> u32 {
        self.baudrate
    }

    fn get_port_name(&self) -> String {
        self.port_name.clone()
    }

    async fn find_device() -> Result<Option<Self>> {
        let devices = spawn_blocking(|| -> Result<Vec<(Device<Context>, u8, u8)>> {
            let context = Context::new()
                .map_err(|e| Error::io(format!("Failed to create USB context: {:?}", e)))?;

            let device_list = context
                .devices()
                .map_err(|e| Error::io(format!("Failed to enumerate USB devices: {:?}", e)))?;

            Ok(device_list
                .iter()
                .map(|d| {
                    let bus = d.bus_number();
                    let addr = d.address();
                    (d, bus, addr)
                })
                .collect())
        })
        .await
        .map_err(|e| Error::io(format!("USB enumeration task panicked: {:?}", e)))??;

        for (device, ..) in devices {
            let descriptor = match device.device_descriptor() {
                Ok(d) => d,
                Err(_) => continue,
            };

            let vid = descriptor.vendor_id();
            let pid = descriptor.product_id();

            let is_known = KNOWN_PORTS.iter().any(|(kvid, kpid, _)| *kvid == vid && *kpid == pid);

            if is_known {
                debug!("Found potential MTK device: {:04X}:{:04X}", vid, pid);

                if let Some(port) = UsbMTKPort::from_device(device) {
                    return Ok(Some(port));
                }
            }
        }

        Ok(None)
    }

    async fn ctrl_out(
        &mut self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
    ) -> Result<()> {
        let handle = self.handle.as_ref().ok_or_else(|| Error::io("Port not open"))?;
        let handle = handle.clone();
        let data = data.to_vec();

        spawn_blocking(move || {
            let handle = handle.blocking_lock();

            handle
                .write_control(request_type, request, value, index, &data, Duration::from_secs(1))
                .map_err(|e| {
                    error!("Control OUT transfer failed: {:?}", e);
                    Error::io(format!("Control OUT transfer failed: {:?}", e))
                })?;

            Ok(())
        })
        .await
        .map_err(|e| Error::io(format!("Control OUT task panicked: {:?}", e)))?
    }

    async fn ctrl_in(
        &mut self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        len: usize,
    ) -> Result<Vec<u8>> {
        let handle = self.handle.as_ref().ok_or_else(|| Error::io("Port not open"))?;
        let handle = handle.clone();

        spawn_blocking(move || {
            let handle = handle.blocking_lock();
            let mut buf = vec![0u8; len];

            let n = handle
                .read_control(request_type, request, value, index, &mut buf, Duration::from_secs(1))
                .map_err(|e| {
                    error!("Control IN transfer failed: {:?}", e);
                    Error::io(format!("Control IN transfer failed: {:?}", e))
                })?;

            buf.truncate(n);
            Ok(buf)
        })
        .await
        .map_err(|e| Error::io(format!("Control IN task panicked: {:?}", e)))?
    }
}
