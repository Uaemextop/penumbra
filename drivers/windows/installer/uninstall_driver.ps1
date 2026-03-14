# uninstall_driver.ps1
# Uninstalls the MediaTek USB Serial Driver

$ErrorActionPreference = "SilentlyContinue"

Write-Host "Uninstalling MTK USB Serial Driver..." -ForegroundColor Yellow

# Find and remove the driver from the store
$drivers = & pnputil.exe /enum-drivers 2>&1
$oem = $null

for ($i = 0; $i -lt $drivers.Count; $i++) {
    if ($drivers[$i] -match "mtk_usb2ser\.inf") {
        # Look back for the OEM name
        for ($j = $i; $j -ge [Math]::Max(0, $i - 5); $j--) {
            if ($drivers[$j] -match "(oem\d+\.inf)") {
                $oem = $Matches[1]
                break
            }
        }
    }
}

if ($oem) {
    Write-Host "Found driver: $oem" -ForegroundColor Gray
    & pnputil.exe /delete-driver $oem /uninstall /force 2>&1 | ForEach-Object {
        Write-Host "  $_" -ForegroundColor Gray
    }
    Write-Host "Driver removed." -ForegroundColor Green
} else {
    Write-Host "Driver not found in store (may already be uninstalled)." -ForegroundColor Yellow
}

# Clean up SERIALCOMM entries
try {
    $serialComm = Get-ItemProperty -Path "HKLM:\HARDWARE\DEVICEMAP\SERIALCOMM" -ErrorAction SilentlyContinue
    if ($serialComm) {
        $serialComm.PSObject.Properties | Where-Object { $_.Name -match "cdcacm" } | ForEach-Object {
            Remove-ItemProperty -Path "HKLM:\HARDWARE\DEVICEMAP\SERIALCOMM" -Name $_.Name -ErrorAction SilentlyContinue
            Write-Host "Removed SERIALCOMM entry: $($_.Name)" -ForegroundColor Gray
        }
    }
} catch {}

Write-Host "Uninstallation complete." -ForegroundColor Green
