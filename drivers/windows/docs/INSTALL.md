# Installation Guide — MTK USB Serial Driver for Windows 11

## Prerequisites

- **Windows 11** (x64) — Windows 10 x64 also supported
- An MTK-based device (phone, tablet, SoC board)
- Administrator privileges

## Installation Options

There are two ways to install the driver:

| Option | Driver | Custom Binary | Best For |
|--------|--------|:------------:|----------|
| **A. KMDF Driver** (recommended) | `mtk_usb2ser.sys` | Yes | Full features, WMI, power management |
| **B. CDC-Only INF** | Windows inbox `usbser.sys` | No | Quick setup, no build required |

---

## Option A: Install the KMDF Driver (Recommended)

### Using the Installer

If you downloaded the installer package from [Releases](https://github.com/Uaemextop/mtk-loader-drivers-opensource-win11/releases):

```powershell
# Run as Administrator
.\MTK_USB_Driver_Setup_1.0.0.exe
```

The installer will copy all driver files and register the driver automatically.

### Using PowerShell

```powershell
# Run as Administrator
powershell -ExecutionPolicy Bypass -File installer\install_driver.ps1
```

### Using pnputil

```cmd
:: Run as Administrator
pnputil /add-driver driver\opensource\mtk_usb2ser.inf /install
```

### Using Device Manager

1. Connect your MTK device in preloader/BROM mode
2. Open **Device Manager** (Win+X → Device Manager)
3. Find the unknown device (under "Other Devices" with ⚠️ icon)
4. Right-click → **Update driver**
5. Choose **Browse my computer for drivers**
6. Navigate to the `driver\opensource\` folder (or the build output folder)
7. Click **Next** → accept any warnings

### Driver Signature Note

If the driver is not signed with a trusted production certificate, you need to enable test signing:

```cmd
:: Run as Administrator
bcdedit /set testsigning on
```

Reboot after this command. To disable later: `bcdedit /set testsigning off`

**Alternative (temporary):** Hold **Shift** → click **Restart** → Troubleshoot → Advanced Options → Startup Settings → Restart → Press **7** to disable driver signature enforcement (lasts until next reboot).

---

## Option B: Install the CDC-Only INF (No Build Required)

This option uses the Windows built-in `usbser.sys` driver — no custom kernel binary needed:

```cmd
:: Run as Administrator
pnputil /add-driver driver\CDC\mtk_preloader_opensource.inf /install
```

Or install via Device Manager by browsing to the `driver\CDC\` folder.

---

## Verify Installation

1. Connect your MTK device
2. Open **Device Manager**
3. Under **Ports (COM & LPT)** you should see:
   - **MediaTek USB Port (BROM)** — for Boot ROM mode (PID 0003)
   - **MediaTek PreLoader USB VCOM Port** — for preloader mode (PID 2000)
   - **MediaTek DA USB VCOM Port** — for download agent mode (PID 2001)
4. Note the COM port number (e.g., COM3)
5. Use this COM port in your flash tool

## Using with Flash Tools

### SP Flash Tool
1. Install the driver using any method above
2. Open SP Flash Tool
3. Load your scatter file
4. The tool will auto-detect the COM port when you connect the device
5. Click **Download** → connect the device in BROM/preloader mode

### mtkclient
```bash
# mtkclient will auto-detect the COM port
python mtk r boot1 boot1.img
python mtk w boot1 boot1_patched.img
```

### SN Write Tool / MAUI META
1. Set the device to Meta mode
2. The Meta VCOM port will appear under COM & LPT
3. Select the COM port in the tool

## Supported Hardware IDs

| PID | Mode | Description |
|-----|------|-------------|
| `0003` | BROM | Boot ROM — primary flash mode |
| `2000` | Preloader | Preloader VCOM |
| `2001` | DA | Download Agent VCOM |
| `2006`–`2007` | Meta | Meta mode VCOM |
| `200A`–`205F` | Various | Composite, ETS, ELT, Modem, AT ports |

## Uninstalling

```powershell
# Run as Administrator
powershell -ExecutionPolicy Bypass -File installer\uninstall_driver.ps1
```

Or manually via Device Manager: right-click the device → **Uninstall device** → check **Delete the driver software**.

## Troubleshooting

### Device not recognized
- Ensure the device is in preloader/BROM mode (hold **Vol+** or **Vol−** while connecting USB)
- Try a different USB cable (short, high-quality cables work best)
- Use a USB 2.0 port directly (avoid USB 3.0 hubs)

### Driver won't install
- Make sure test signing is enabled or signature enforcement is disabled
- Run the install command as **Administrator**
- Check Device Manager for error codes (Code 52 = unsigned driver)

### COM port appears briefly then disappears
- This is normal for BROM mode — the device only stays in this mode for ~1 second
- Use your flash tool's **auto-detect** or **scan** feature to catch the port automatically
- In SP Flash Tool, click Download first, then connect the device

### Error Code 52 (Unsigned driver)
- Enable test signing: `bcdedit /set testsigning on` and reboot
- Or use the CDC-only INF (Option B) which uses Windows' signed inbox driver
