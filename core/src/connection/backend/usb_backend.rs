/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, error, info, warn};
use nusb::descriptors::TransferType;
use nusb::io::{EndpointRead, EndpointWrite};
use nusb::transfer::{Bulk, ControlIn, ControlOut, ControlType, Direction, In, Out, Recipient};
use nusb::{DeviceInfo, Interface};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::MTKPort;
use crate::connection::ConnectionType;
use crate::connection::port::KNOWN_PORTS;
use crate::error::{Error, Result};

/// USB operation timeout — longer on Windows to accommodate WinUSB driver latency
#[cfg(windows)]
const MAX_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(not(windows))]
const MAX_TIMEOUT: Duration = Duration::from_secs(2);

const BULK_IN_SZ: usize = 0x80000;
const BULK_OUT_SZ: usize = 0x80000;

/// Maximum number of retries for interface claiming on Windows
#[cfg(windows)]
const IFACE_CLAIM_RETRIES: u32 = 3;
/// Delay between interface claim retries on Windows
#[cfg(windows)]
const IFACE_CLAIM_RETRY_DELAY: Duration = Duration::from_millis(500);

pub struct UsbMTKPort {
    info: DeviceInfo,
    interface: Option<Interface>,
    ctrl_interface: Option<Interface>,
    reader: Option<EndpointRead<Bulk>>,
    writer: Option<EndpointWrite<Bulk>>,
    ep_out: u8,
    ep_in: u8,
    in_max_packet_size: usize,
    out_max_packet_size: usize,
    connection_type: ConnectionType,
    is_open: bool,
}

impl fmt::Debug for UsbMTKPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "UsbMTKPort {{ info: {:?}, connection_type: {:?}, is_open: {} }}",
            self.info, self.connection_type, self.is_open
        )
    }
}

impl UsbMTKPort {
    pub fn new(info: DeviceInfo, connection_type: ConnectionType) -> Self {
        Self {
            info,
            interface: None,
            ctrl_interface: None,
            writer: None,
            reader: None,
            ep_out: 0,
            ep_in: 0,
            in_max_packet_size: 0,
            out_max_packet_size: 0,
            connection_type,
            is_open: false,
        }
    }

    fn select_endpoints(&mut self, iface: &Interface) -> Result<()> {
        for alt in iface.descriptors() {
            let mut in_ep = None;
            let mut out_ep = None;

            for ep in alt.endpoints() {
                if !matches!(ep.transfer_type(), TransferType::Bulk) {
                    continue;
                }

                match ep.direction() {
                    Direction::In => {
                        in_ep = Some(ep.address());
                        self.in_max_packet_size = ep.max_packet_size();
                    }
                    Direction::Out => {
                        out_ep = Some(ep.address());
                        self.out_max_packet_size = ep.max_packet_size();
                    }
                }
            }

            if let (Some(i), Some(o)) = (in_ep, out_ep) {
                self.ep_in = i;
                self.ep_out = o;
                return Ok(());
            }
        }

        Err(Error::io("No bulk endpoints found"))
    }

    /// Build CDC line coding bytes for the given baudrate.
    /// Format: [baud_le(4 bytes), stop_bits(1), parity(1), data_bits(1)]
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

    async fn setup_cdc(&self) -> Result<()> {
        let iface = self.ctrl_interface.as_ref().ok_or(Error::io("Interface not open"))?;

        const CDC_INTERFACE_NUM: u16 = 0;
        const SET_LINE_CODING: u8 = 0x20;
        const SET_CONTROL_LINE_STATE: u8 = 0x22;
        const CONTROL_LINE_STATE: u16 = 0x03; // DTR | RTS

        // Use connection-type-aware baudrate for CDC line coding,
        // matching the MTK USB CDC ACM protocol:
        //   BROM: 115200 bps, Preloader/DA: 921600 bps
        let baudrate = match self.connection_type {
            ConnectionType::Brom => 115_200u32,
            ConnectionType::Preloader | ConnectionType::Da => 921_600u32,
        };
        let line_coding = Self::build_line_coding(baudrate);
        debug!("CDC Setup: baudrate={}, connection_type={:?}", baudrate, self.connection_type);

        iface
            .control_out(
                ControlOut {
                    control_type: ControlType::Class,
                    recipient: Recipient::Interface,
                    request: SET_LINE_CODING,
                    value: 0,
                    index: CDC_INTERFACE_NUM,
                    data: &line_coding,
                },
                MAX_TIMEOUT,
            )
            .await
            .map_err(|e| Error::io(format!("CDC Set Line Coding failed: {}", e)))?;

        iface
            .control_out(
                ControlOut {
                    control_type: ControlType::Class,
                    recipient: Recipient::Interface,
                    request: SET_CONTROL_LINE_STATE,
                    value: CONTROL_LINE_STATE,
                    index: CDC_INTERFACE_NUM,
                    data: &[],
                },
                MAX_TIMEOUT,
            )
            .await
            .map_err(|e| Error::io(format!("CDC Set Control Line State failed: {}", e)))?;

        debug!("CDC Setup complete");
        Ok(())
    }
}

#[async_trait]
impl MTKPort for UsbMTKPort {
    async fn open(&mut self) -> Result<()> {
        if self.is_open {
            return Ok(());
        }

        let device = self.info.open().await.map_err(|e| {
            #[cfg(windows)]
            error!(
                "Failed to open USB device. On Windows, ensure WinUSB or libusb drivers \
                 are installed (e.g. via Zadig or the Penumbra driver installer). \
                 The default MediaTek serial drivers (usb2ser/MT65xx) are not compatible \
                 with direct USB access. Error: {}",
                e
            );
            #[cfg(not(windows))]
            error!("Failed to open USB device: {}", e);
            Error::io(format!("Failed to open USB device: {}", e))
        })?;

        // On Windows, interface claiming can be racy when the WinUSB driver
        // has just been loaded. Retry with a delay to handle this.
        #[cfg(windows)]
        let (ctrl_iface, iface) = {
            let mut last_err = None;
            let mut result = None;
            for attempt in 0..IFACE_CLAIM_RETRIES {
                if attempt > 0 {
                    debug!("Retrying interface claim (attempt {}/{})", attempt + 1, IFACE_CLAIM_RETRIES);
                    tokio::time::sleep(IFACE_CLAIM_RETRY_DELAY).await;
                }
                match device.detach_and_claim_interface(0).await {
                    Ok(ctrl) => match device.detach_and_claim_interface(1).await {
                        Ok(data) => {
                            result = Some((ctrl, data));
                            break;
                        }
                        Err(e) => {
                            warn!("Failed to claim data interface (attempt {}): {}", attempt + 1, e);
                            last_err = Some(e);
                        }
                    },
                    Err(e) => {
                        warn!("Failed to claim control interface (attempt {}): {}", attempt + 1, e);
                        last_err = Some(e);
                    }
                }
            }
            match result {
                Some(r) => r,
                None => {
                    let err = last_err.unwrap();
                    error!(
                        "Failed to claim USB interfaces after {} attempts. \
                         On Windows, this usually means the device is still bound to the \
                         MediaTek serial driver (usb2ser.sys / MT65xx Preloader). \
                         Uninstall the MediaTek driver in Device Manager, then install \
                         WinUSB drivers using the Penumbra driver installer or Zadig. \
                         Error: {}",
                        IFACE_CLAIM_RETRIES, err
                    );
                    return Err(Error::io(format!(
                        "Failed to claim USB interfaces: {}. \
                         On Windows, ensure WinUSB drivers are installed \
                         (uninstall MediaTek serial drivers first).",
                        err
                    )));
                }
            }
        };

        #[cfg(not(windows))]
        let ctrl_iface = device.detach_and_claim_interface(0).await?;
        #[cfg(not(windows))]
        let iface = device.detach_and_claim_interface(1).await?;

        self.select_endpoints(&iface)?;

        // Windows WinUSB has limitations with multiple pending transfers that
        // can cause "Error 87" (ERROR_INVALID_PARAMETER). Use a single
        // transfer on Windows for reliability.
        #[cfg(windows)]
        let tr = 1;

        #[cfg(not(windows))]
        let tr = 8;

        let ep_in = iface.endpoint::<Bulk, In>(self.ep_in)?;
        let rdr = ep_in.reader(BULK_IN_SZ).with_num_transfers(tr).with_read_timeout(MAX_TIMEOUT);
        let ep_out = iface.endpoint::<Bulk, Out>(self.ep_out)?;
        let wr = ep_out.writer(BULK_OUT_SZ).with_num_transfers(tr).with_write_timeout(MAX_TIMEOUT);

        self.reader = Some(rdr);
        self.writer = Some(wr);
        self.interface = Some(iface);
        self.ctrl_interface = Some(ctrl_iface);

        // CDC setup is needed for preloader and DA modes on all platforms.
        // On Windows, also attempt CDC setup for BROM mode since WinUSB
        // may need explicit line coding configuration.
        #[cfg(windows)]
        {
            if let Err(e) = self.setup_cdc().await {
                debug!("CDC setup failed (may be ok for BROM): {:?}", e);
            }
        }

        #[cfg(not(windows))]
        {
            if self.connection_type != ConnectionType::Brom
                && let Err(e) = self.setup_cdc().await
            {
                debug!("CDC setup failed (may be ok): {:?}", e);
            }
        }

        self.is_open = true;
        info!("USB device opened successfully: {}", self.get_port_name());

        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        if !self.is_open {
            return Ok(());
        }

        // NUSB automatically releases interfaces on drop
        self.reader = None;
        self.writer = None;
        self.interface = None;
        self.is_open = false;

        Ok(())
    }

    async fn read_exact(&mut self, buf: &mut [u8]) -> Result<usize> {
        let reader = self.reader.as_mut().ok_or_else(|| Error::io("USB port is not open"))?;

        reader.read_exact(buf).await?;
        Ok(buf.len())
    }

    async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        let writer = self.writer.as_mut().ok_or_else(|| Error::io("USB port is not open"))?;

        writer.write_all(buf).await?;
        writer.flush().await?;
        Ok(())
    }

    /// USB doesn't need flushing
    async fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    async fn handshake(&mut self) -> Result<()> {
        let mut resp = [0u8; 1];

        loop {
            self.write_all(&[0xA0]).await?;
            self.read_exact(&mut resp).await?;
            let b = resp[0];

            if b == 0x5F {
                break;
            }

            // Already handshaken, so preloader just echoes
            if b == 0xA0 {
                return Ok(());
            }
        }

        const SEQ: [u8; 3] = [0x0A, 0x50, 0x05];

        for &byte in &SEQ {
            self.write_all(&[byte]).await?;
            self.read_exact(&mut resp).await?;

            if resp[0] != (byte ^ 0xFF) {
                return Err(Error::conn(format!(
                    "Handshake failed: sent 0x{:02X}, expected 0x{:02X}, got 0x{:02X}",
                    byte,
                    byte ^ 0xFF,
                    resp[0]
                )));
            }
        }

        Ok(())
    }

    fn get_connection_type(&self) -> ConnectionType {
        self.connection_type
    }

    fn get_baudrate(&self) -> u32 {
        0
    }

    fn get_port_name(&self) -> String {
        format!("USB {:04X}:{:04X}", self.info.vendor_id(), self.info.product_id())
    }

    async fn find_device() -> Result<Option<Self>> {
        let devices = nusb::list_devices().await?;

        for device in devices {
            if let Some((_, _, conn_type)) = KNOWN_PORTS
                .iter()
                .find(|(vid, pid, _)| device.vendor_id() == *vid && device.product_id() == *pid)
            {
                info!(
                    "Found MTK device: {:04X}:{:04X} ({:?})",
                    device.vendor_id(),
                    device.product_id(),
                    conn_type
                );
                return Ok(Some(UsbMTKPort::new(device, *conn_type)));
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
        let iface =
            self.ctrl_interface.as_ref().ok_or_else(|| Error::io("USB port is not open"))?;

        let control_type = match (request_type >> 5) & 0b11 {
            0 => ControlType::Standard,
            1 => ControlType::Class,
            2 => ControlType::Vendor,
            _ => ControlType::Standard,
        };

        let recipient = match request_type & 0b11111 {
            0 => Recipient::Device,
            1 => Recipient::Interface,
            2 => Recipient::Endpoint,
            _ => Recipient::Other,
        };

        iface
            .control_out(
                ControlOut { control_type, recipient, request, value, index, data },
                Duration::from_secs(1),
            )
            .await
            .map_err(|e| Error::io(format!("Control OUT transfer failed: {}", e)))?;

        Ok(())
    }

    async fn ctrl_in(
        &mut self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        len: usize,
    ) -> Result<Vec<u8>> {
        let iface =
            self.ctrl_interface.as_ref().ok_or_else(|| Error::io("USB port is not open"))?;

        let control_type = match (request_type >> 5) & 0b11 {
            0 => ControlType::Standard,
            1 => ControlType::Class,
            2 => ControlType::Vendor,
            _ => ControlType::Standard,
        };

        let recipient = match request_type & 0b11111 {
            0 => Recipient::Device,
            1 => Recipient::Interface,
            2 => Recipient::Endpoint,
            _ => Recipient::Other,
        };

        let buf = iface
            .control_in(
                ControlIn { control_type, recipient, request, value, index, length: len as u16 },
                Duration::from_secs(1),
            )
            .await
            .map_err(|e| Error::io(format!("Control IN transfer failed: {}", e)))?;

        Ok(buf)
    }
}
