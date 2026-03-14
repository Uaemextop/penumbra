# install_driver.ps1
# MediaTek USB Serial Driver installer script
# Compatible with Windows 11 x64 and x86
# Works with SP Flash Tool and mtkclient

param(
    [string]$InfPath = ""
)

$ErrorActionPreference = "Stop"

function Write-Header {
    Write-Host ""
    Write-Host "=============================================" -ForegroundColor Cyan
    Write-Host "  MTK USB Serial Driver Installer" -ForegroundColor Cyan
    Write-Host "  For Preloader / BROM DA / Meta Mode" -ForegroundColor Cyan
    Write-Host "  Compatible: SP Flash Tool, mtkclient" -ForegroundColor Cyan
    Write-Host "=============================================" -ForegroundColor Cyan
    Write-Host ""
}

function Test-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Find-InfFile {
    param([string]$BasePath)
    
    # Search order
    $candidates = @(
        (Join-Path $BasePath "mtk_usb2ser.inf"),
        (Join-Path $BasePath "driver\opensource\mtk_usb2ser.inf"),
        (Join-Path $BasePath "driver\CDC\mtk_preloader_opensource.inf")
    )
    
    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }
    return $null
}

function Install-MtkDriver {
    param([string]$InfFile)
    
    Write-Host "[1/4] Validating driver package..." -ForegroundColor Yellow
    if (-not (Test-Path $InfFile)) {
        throw "INF file not found: $InfFile"
    }
    Write-Host "  INF: $InfFile" -ForegroundColor Gray
    
    # Check architecture
    $arch = if ([Environment]::Is64BitOperatingSystem) { "x64" } else { "x86" }
    Write-Host "  Architecture: $arch" -ForegroundColor Gray
    Write-Host "  OS: Windows $([Environment]::OSVersion.Version)" -ForegroundColor Gray
    
    # Check for .sys file
    $infDir = Split-Path $InfFile -Parent
    $sysFile = Join-Path $infDir "$arch\mtk_usb2ser.sys"
    if (-not (Test-Path $sysFile)) {
        # Also check same directory
        $sysFile = Join-Path $infDir "mtk_usb2ser.sys"
    }
    if (Test-Path $sysFile) {
        $sysSize = (Get-Item $sysFile).Length
        Write-Host "  SYS: $sysFile ($sysSize bytes)" -ForegroundColor Gray
    }
    
    Write-Host ""
    Write-Host "[2/4] Adding driver to Windows Driver Store..." -ForegroundColor Yellow
    
    # Use pnputil to add the driver to the store
    $result = & pnputil.exe /add-driver $InfFile /install 2>&1
    $exitCode = $LASTEXITCODE
    
    foreach ($line in $result) {
        Write-Host "  $line" -ForegroundColor Gray
    }
    
    if ($exitCode -eq 0) {
        Write-Host ""
        Write-Host "[3/4] Driver installed successfully!" -ForegroundColor Green
    } else {
        Write-Host ""
        Write-Host "[3/4] pnputil returned code $exitCode" -ForegroundColor Yellow
        Write-Host "  Attempting alternative installation..." -ForegroundColor Yellow
        
        # Try with /subdirs flag
        $result2 = & pnputil.exe /add-driver $InfFile /install /subdirs 2>&1
        foreach ($line in $result2) {
            Write-Host "  $line" -ForegroundColor Gray
        }
    }
    
    Write-Host ""
    Write-Host "[4/4] Verifying installation..." -ForegroundColor Yellow
    
    # Check if driver is in the store
    $drivers = & pnputil.exe /enum-drivers 2>&1
    $found = $false
    foreach ($line in $drivers) {
        if ($line -match "mtk_usb2ser" -or $line -match "MediaTek") {
            Write-Host "  $line" -ForegroundColor Green
            $found = $true
        }
    }
    
    if ($found) {
        Write-Host ""
        Write-Host "Driver is ready!" -ForegroundColor Green
        Write-Host ""
        Write-Host "Connect your MTK device in Preloader/BROM mode." -ForegroundColor White
        Write-Host "A new COM port will appear in Device Manager." -ForegroundColor White
        Write-Host ""
        Write-Host "Supported modes:" -ForegroundColor White
        Write-Host "  - BROM (Boot ROM)     -> PID 0003" -ForegroundColor Gray
        Write-Host "  - Preloader           -> PID 2000" -ForegroundColor Gray
        Write-Host "  - Download Agent (DA) -> PID 2001" -ForegroundColor Gray
        Write-Host "  - Meta Mode           -> PID 2007+" -ForegroundColor Gray
    } else {
        Write-Host "  Driver entry not found in store (may need reboot)" -ForegroundColor Yellow
    }
}

# Main
Write-Header

if (-not (Test-Admin)) {
    Write-Host "ERROR: Administrator privileges required." -ForegroundColor Red
    Write-Host "Please right-click and 'Run as Administrator'." -ForegroundColor Red
    Read-Host "Press Enter to exit"
    exit 1
}

# Find INF file
if ([string]::IsNullOrEmpty($InfPath)) {
    $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
    $InfPath = Find-InfFile -BasePath $scriptDir
    if (-not $InfPath) {
        $InfPath = Find-InfFile -BasePath (Get-Location).Path
    }
}

if ([string]::IsNullOrEmpty($InfPath) -or -not (Test-Path $InfPath)) {
    Write-Host "ERROR: Could not find driver INF file." -ForegroundColor Red
    Write-Host "Usage: .\install_driver.ps1 -InfPath <path\to\mtk_usb2ser.inf>" -ForegroundColor Yellow
    Read-Host "Press Enter to exit"
    exit 1
}

try {
    Install-MtkDriver -InfFile $InfPath
    Write-Host ""
    Write-Host "Installation complete." -ForegroundColor Green
} catch {
    Write-Host ""
    Write-Host "ERROR: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}

Read-Host "Press Enter to exit"
