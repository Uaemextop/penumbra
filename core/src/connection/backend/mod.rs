/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
#[cfg(all(feature = "libusb", not(feature = "libusb-exp")))]
pub mod libusb_backend;
#[cfg(all(feature = "libusb", feature = "libusb-exp"))]
pub mod libusb_backend_exp;
#[cfg(feature = "serial")]
pub mod serial_backend;
#[cfg(not(any(feature = "libusb", feature = "serial")))]
pub mod usb_backend;
#[cfg(all(feature = "libusb", not(feature = "libusb-exp")))]
pub use libusb_backend::UsbMTKPort;
#[cfg(all(feature = "libusb", feature = "libusb-exp"))]
pub use libusb_backend_exp::UsbMTKPort;
#[cfg(feature = "serial")]
pub use serial_backend::SerialMTKPort;
#[cfg(not(any(feature = "libusb", feature = "serial")))]
pub use usb_backend::UsbMTKPort;
