# Penumbra on Windows — Setup Guide

Penumbra communicates with MediaTek devices using direct USB access (via the [nusb](https://crates.io/crates/nusb) library). On Windows, this requires the **WinUSB** driver to be bound to the device instead of the default MediaTek serial driver.

## Quick Setup

### Option 1: Install the Penumbra WinUSB Driver (Recommended)

This is the easiest method. The included WinUSB INF file maps MediaTek devices to WinUSB automatically:

```cmd
:: Run as Administrator
pnputil /add-driver drivers\windows\WinUSB\penumbra_mtk_winusb.inf /install
```

> **Note:** You may need to enable test signing if the INF is not signed:
> ```cmd
> bcdedit /set testsigning on
> ```
> Reboot after this command.

### Option 2: Use the NSIS Driver Installer

Run the Penumbra driver installer (from `scripts/drivers/`), which uses libwdi to install WinUSB or LibUSB drivers:

```
scripts\drivers\PenumbraDrivers.exe
```

### Option 3: Use Zadig (Manual)

1. Download [Zadig](https://zadig.akeo.ie/)
2. Connect your MTK device in Preloader/BROM mode
3. In Zadig, select the MediaTek device
4. Choose **WinUSB** as the target driver
5. Click **Replace Driver**

## How It Works

On Windows, USB devices need a kernel-mode driver to be accessible from user space:

| Driver | Access Type | Compatible Tools |
|--------|------------|-----------------|
| **WinUSB** (recommended) | Direct USB | Penumbra, mtkclient (with libusb), any libusb/nusb tool |
| **usb2ser.sys / MT65xx** | COM port (serial) | SP Flash Tool, serial-mode tools |
| **usbser.sys** (inbox CDC) | COM port (serial) | SP Flash Tool, serial-mode tools |

Penumbra uses **nusb** which requires **WinUSB**. The included driver INF files handle this automatically.

### Why Not LibUSB/UsbDk?

Previous solutions (like mtkclient) required installing LibUSB or UsbDk filter drivers. The WinUSB approach is better because:

- **WinUSB is built into Windows** — no third-party DLLs needed
- **No filter drivers** — cleaner, more stable, no UsbDk installation
- **Compatible with nusb and libusb** — both can use WinUSB as a backend
- **No Zadig needed** — the INF handles driver binding automatically

## Driver Options

This repository includes three driver options:

### 1. WinUSB INF (`drivers/windows/WinUSB/`)
- **Best for**: Penumbra, mtkclient, any direct-USB tool
- **No custom binary** — uses Windows inbox WinUSB.sys
- Provides raw USB access (bulk IN/OUT, control transfers)

### 2. Open-Source KMDF Driver (`drivers/windows/opensource/`)
- **Best for**: SP Flash Tool, legacy tools requiring COM ports
- Custom kernel driver (mtk_usb2ser.sys) based on KMDF 1.33
- Full CDC ACM implementation with 31 serial IOCTLs
- Requires WDK to build (or use pre-built from CI)

### 3. CDC-Only INF (`drivers/windows/CDC/`)
- **Best for**: Quick COM port setup without custom drivers
- Uses Windows inbox usbser.sys
- No build required

## Troubleshooting

### Device not detected by Penumbra

1. **Check Device Manager**: The device should appear under "Universal Serial Bus devices" (not under "Ports (COM & LPT)")
2. **Wrong driver**: If it shows as "MediaTek PreLoader USB VCOM (Android)" under Ports, the serial driver is installed instead of WinUSB
3. **Fix**: Uninstall the serial driver in Device Manager (check "Delete the driver software"), then install the WinUSB driver

### Error 87 (ERROR_INVALID_PARAMETER)

This usually means WinUSB interface claiming failed:
- Ensure only the WinUSB driver is installed (not both WinUSB and serial)
- Try running Penumbra as Administrator
- Try a different USB port (USB 2.0 ports are more reliable)

### Device appears briefly then disappears

This is normal for BROM mode — the device only stays in BROM for ~1 second:
- Start Penumbra first, then connect the device
- Penumbra will automatically detect the device when it appears

### mtkclient compatibility

The WinUSB driver also works with mtkclient. After installing the WinUSB INF:
- mtkclient's libusb backend will use WinUSB automatically
- No need for UsbDk or libusb-win32 filter drivers
- Set `LIBUSB_DEBUG=4` environment variable for debugging

### Switching back to serial drivers

If you need to use SP Flash Tool (which requires COM ports):
```cmd
:: Remove WinUSB driver
pnputil /delete-driver penumbra_mtk_winusb.inf /uninstall

:: Install the CDC serial driver
pnputil /add-driver drivers\windows\CDC\mtk_preloader_opensource.inf /install
```

## Building from Source

Penumbra on Windows is built the same way as on Linux:

```cmd
:: Default build (uses nusb backend, recommended for Windows)
cargo build --release

:: Alternative: libusb backend (requires libusb-1.0.dll)
cargo build --release --features libusb --no-default-features

:: Alternative: serial backend (for COM port access)
cargo build --release --features serial --no-default-features
```

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                    Penumbra (antumbra)                        │
│                      ↕ nusb (async USB)                      │
├──────────────────────────────────────────────────────────────┤
│                   WinUSB.sys (inbox)                          │
│            Bound via penumbra_mtk_winusb.inf                 │
│                      ↕ USB Driver Stack                      │
├──────────────────────────────────────────────────────────────┤
│               USB Host Controller                             │
│                      ↕ USB Cable                             │
├──────────────────────────────────────────────────────────────┤
│            MediaTek Device (VID 0x0E8D)                       │
│   BROM (0003) → PreLoader (2000) → DA (2001) → ...          │
└──────────────────────────────────────────────────────────────┘
```
