# MTK USB CDC ACM Driver for Windows — Penumbra

This directory contains the open-source MediaTek USB CDC ACM driver for Windows,
sourced from [github.com/Uaemextop/mtk-loader-drivers-opensource-win11](https://github.com/Uaemextop/mtk-loader-drivers-opensource-win11)
and modified for Penumbra compatibility.

## Overview

The driver enables Windows to communicate with MediaTek devices over USB serial
(CDC ACM) — including Preloader, BROM, Download Agent, Meta Mode, and various
composite endpoints. It is compatible with SP Flash Tool, mtkclient, and other
MTK flashing/debugging utilities.

## Installation Options

| Option | Directory | Driver Binary | Notes |
|--------|-----------|:------------:|-------|
| **KMDF Driver** (full features) | `opensource/` | Custom `mtk_usb2ser.sys` | WMI, power management, ring-buffered I/O |
| **CDC-Only INF** (no build required) | `CDC/` | Windows inbox `usbser.sys` | Simpler, no custom binary needed |

## Quick Install

### Option A: KMDF Driver (recommended)

```cmd
:: Run as Administrator
pnputil /add-driver opensource\mtk_usb2ser.inf /install
```

Or use the PowerShell installer:

```powershell
powershell -ExecutionPolicy Bypass -File installer\install_driver.ps1
```

### Option B: CDC-Only INF

```cmd
:: Run as Administrator
pnputil /add-driver CDC\mtk_preloader_opensource.inf /install
```

## Directory Structure

```
drivers/windows/
├── opensource/          KMDF driver source code and build files
│   ├── driver.c        DriverEntry and device add
│   ├── device.c        PnP/Power, USB config, symbolic links
│   ├── queue.c         I/O queue setup and dispatch
│   ├── serial.c        All IOCTL_SERIAL_* handlers
│   ├── usbtransfer.c   Ring buffer, USB transfer completions
│   ├── usbcontrol.c    CDC ACM control requests
│   ├── power.c         Selective suspend / idle
│   ├── wmi.c           WMI registration
│   ├── mtk_usb2ser.h   Main header
│   ├── version.h       Version constants
│   ├── mtk_usb2ser.inf KMDF driver INF
│   ├── mtk_usb2ser.rc  VERSIONINFO resource
│   ├── mtk_usb2ser.vcxproj  Visual Studio project
│   ├── mtk_usb2ser.sln      Visual Studio solution
│   └── Makefile.wdk          WDK build file
├── CDC/                CDC-only INF (uses inbox usbser.sys)
│   └── mtk_preloader_opensource.inf
├── installer/          Installation scripts
│   ├── install_driver.ps1
│   ├── uninstall_driver.ps1
│   └── setup.iss       Inno Setup script
├── docs/
│   └── INSTALL.md      Detailed installation guide
└── README.md           This file
```

## Building the KMDF Driver

Requires Windows Driver Kit (WDK) 10.0.26100 or later:

```cmd
msbuild opensource\mtk_usb2ser.vcxproj /p:Configuration=Release /p:Platform=x64
```

## Original Repository

This driver is based on the open-source MTK loader driver project:
**[github.com/Uaemextop/mtk-loader-drivers-opensource-win11](https://github.com/Uaemextop/mtk-loader-drivers-opensource-win11)**

Licensed under the MIT License. See the individual source files for details.
