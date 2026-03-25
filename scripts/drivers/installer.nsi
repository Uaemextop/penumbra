!include "MUI2.nsh"
!include "LogicLib.nsh"

!define PRODUCT_NAME "Penumbra Drivers Installer"
!define PRODUCT_DESCRIPTION "Installs WinUSB drivers for MediaTek, LG, OPPO, and Sony USB devices used by Penumbra."
!define PRODUCT_VERSION "1.0.0.0"

Name "${PRODUCT_NAME}"
OutFile "PenumbraDrivers.exe"
RequestExecutionLevel admin

VIProductVersion "${PRODUCT_VERSION}"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "ProductVersion" "${PRODUCT_VERSION}"
VIAddVersionKey "FileDescription" "${PRODUCT_DESCRIPTION}"
VIAddVersionKey "LegalCopyright" "GPL-3.0"
VIAddVersionKey "CompanyName" "Penumbra Project"
VIAddVersionKey "FileVersion" "${PRODUCT_VERSION}"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "license.txt"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_LANGUAGE "English"

Section "Install WinUSB Drivers" SecInstall
  SetOutPath "$TEMP\PenumbraDrivers"
  File "penumbra_winusb.inf"

  DetailPrint "Installing WinUSB drivers for Penumbra devices..."
  DetailPrint "Using pnputil to register the in-box WinUSB driver..."

  ; Install the INF using pnputil — this uses the Microsoft-signed in-box
  ; WinUSB driver so no third-party catalog is needed.  Windows may show a
  ; "Do you want to install this device software?" prompt; click Install.
  nsExec::ExecToLog 'pnputil /add-driver "$TEMP\PenumbraDrivers\penumbra_winusb.inf" /install'
  Pop $0

  ${If} $0 == 0
    DetailPrint "Drivers installed successfully."
    DetailPrint ""
    DetailPrint "Supported devices:"
    DetailPrint "  - MediaTek USB (BROM, Preloader, DA)"
    DetailPrint "  - LG USB (Preloader)"
    DetailPrint "  - OPPO USB (Preloader)"
    DetailPrint "  - Sony USB (BROM)"
  ${Else}
    DetailPrint "pnputil returned code: $0"
    DetailPrint "If installation failed, try the following:"
    DetailPrint "  1. Right-click the installer and select 'Run as administrator'"
    DetailPrint "  2. Make sure no MediaTek USB device is currently connected"
    DetailPrint "  3. Temporarily disable antivirus if it blocks the installation"
  ${EndIf}

  RMDir /r "$TEMP\PenumbraDrivers"
SectionEnd
