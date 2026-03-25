# Penumbra drivers script
This directory contains the NSIS script and related files used to build the driver installer for Penumbra on Windows.

Licensed under the GPL-3.0 License, see the license.txt file in this directory for details.

## Files

- `installer.nsi`: The NSIS script that defines how the drivers are installed.
- `penumbra_winusb.inf`: WinUSB INF file that maps MediaTek, LG, OPPO, and Sony USB device VID/PIDs to the Microsoft in-box WinUSB driver. No third-party binaries are needed since WinUSB ships with Windows and is already signed by Microsoft.
- `wdi-simple.exe`: (Legacy) libwdi example CLI installer, compiled from source from the [libwdi examples](https://github.com/pbatard/libwdi/tree/master/examples). No longer used by the installer since it creates unsigned driver packages that are blocked by Windows driver signature enforcement on 64-bit systems.

## How it works

The installer uses `pnputil /add-driver` to register the WinUSB INF file with Windows. When a matching USB device is connected, Windows automatically assigns the WinUSB driver to it. This approach avoids the "please contact the owner of this software" error that occurred with the previous `wdi-simple.exe` approach, because:

1. WinUSB is an in-box driver already signed by Microsoft
2. `pnputil` is the standard Windows tool for driver management
3. No self-signed catalogs or unsigned driver packages are created
