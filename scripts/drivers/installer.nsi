!include "MUI2.nsh"
!include "nsDialogs.nsh"
!include "LogicLib.nsh"

!define PRODUCT_NAME "Penumbra drivers installer"
!define PRODUCT_DESCRIPTION "Installs USB drivers for various devices, for use with Penumbra."
!define PRODUCT_VERSION "2.0.0.0"

Name "${PRODUCT_NAME}"
OutFile "PenumbraDrivers.exe"
RequestExecutionLevel admin

VIProductVersion "${PRODUCT_VERSION}"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "ProductVersion" "${PRODUCT_VERSION}"
VIAddVersionKey "FileDescription" "${PRODUCT_DESCRIPTION}"

Var DriverType
Var RadioWinUSB
Var RadioLibUSB

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "license.txt"
Page custom DriverSelectPage DriverSelectPageLeave
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_LANGUAGE "English"

Function DriverSelectPage
  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 20u "Select the driver type to install:"
  Pop $0

  ${NSD_CreateRadioButton} 0 30u 100% 12u "&WinUSB (recommended for Penumbra and mtkclient)"
  Pop $RadioWinUSB

  ${NSD_CreateRadioButton} 0 50u 100% 12u "&LibUSB (legacy)"
  Pop $RadioLibUSB

  ; Default: WinUSB (type = 0)
  ${NSD_Check} $RadioWinUSB
  StrCpy $DriverType "0"

  nsDialogs::Show
FunctionEnd

Function DriverSelectPageLeave
  ${NSD_GetState} $RadioLibUSB $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $DriverType "1"
  ${Else}
    StrCpy $DriverType "0"
  ${EndIf}
FunctionEnd

Section "Install USB Drivers" SecInstall
  SetOutPath "$TEMP\PenumbraDrivers"
  File "wdi-simple.exe"

  DetailPrint "Installing drivers (type=$DriverType)..."
  DetailPrint "Type 0 = WinUSB (recommended), Type 1 = LibUSB (legacy)"

  ; MTK core boot modes
  nsExec::ExecToLog '"$TEMP\PenumbraDrivers\wdi-simple.exe" -v 0x0E8D -p 0x0003 -t $DriverType -n "MediaTek USB Port (BROM)"'
  nsExec::ExecToLog '"$TEMP\PenumbraDrivers\wdi-simple.exe" -v 0x0E8D -p 0x6000 -t $DriverType -n "MediaTek USB Port (Preloader)"'
  nsExec::ExecToLog '"$TEMP\PenumbraDrivers\wdi-simple.exe" -v 0x0E8D -p 0x2000 -t $DriverType -n "MediaTek USB Port (Preloader)"'
  nsExec::ExecToLog '"$TEMP\PenumbraDrivers\wdi-simple.exe" -v 0x0E8D -p 0x2001 -t $DriverType -n "MediaTek USB Port (DA)"'
  nsExec::ExecToLog '"$TEMP\PenumbraDrivers\wdi-simple.exe" -v 0x0E8D -p 0x20FF -t $DriverType -n "MediaTek USB Port (Preloader)"'
  nsExec::ExecToLog '"$TEMP\PenumbraDrivers\wdi-simple.exe" -v 0x0E8D -p 0x3000 -t $DriverType -n "MediaTek USB Port (Preloader)"'

  ; MTK Meta Mode / VCOM
  nsExec::ExecToLog '"$TEMP\PenumbraDrivers\wdi-simple.exe" -v 0x0E8D -p 0x2006 -t $DriverType -n "MediaTek USB VCOM Port (Meta)"'
  nsExec::ExecToLog '"$TEMP\PenumbraDrivers\wdi-simple.exe" -v 0x0E8D -p 0x2007 -t $DriverType -n "MediaTek USB VCOM Port (Meta)"'

  ; LG
  nsExec::ExecToLog '"$TEMP\PenumbraDrivers\wdi-simple.exe" -v 0x1004 -p 0x6000 -t $DriverType -n "LG USB Port (Preloader)"'

  ; OPPO
  nsExec::ExecToLog '"$TEMP\PenumbraDrivers\wdi-simple.exe" -v 0x22D9 -p 0x0006 -t $DriverType -n "OPPO USB Port (Preloader)"'

  ; Sony
  nsExec::ExecToLog '"$TEMP\PenumbraDrivers\wdi-simple.exe" -v 0x0FCE -p 0xF200 -t $DriverType -n "Sony USB Port (BROM)"'
  nsExec::ExecToLog '"$TEMP\PenumbraDrivers\wdi-simple.exe" -v 0x0FCE -p 0xD1E9 -t $DriverType -n "Sony XA1 USB Port (BROM)"'
  nsExec::ExecToLog '"$TEMP\PenumbraDrivers\wdi-simple.exe" -v 0x0FCE -p 0xD1E2 -t $DriverType -n "Sony USB Port (BROM)"'
  nsExec::ExecToLog '"$TEMP\PenumbraDrivers\wdi-simple.exe" -v 0x0FCE -p 0xD1EC -t $DriverType -n "Sony L1 USB Port (BROM)"'
  nsExec::ExecToLog '"$TEMP\PenumbraDrivers\wdi-simple.exe" -v 0x0FCE -p 0xD1DD -t $DriverType -n "Sony F3111 USB Port (BROM)"'

  MessageBox MB_OK "Driver installation finished.$\n$\nPenumbra and mtkclient should now detect your device.$\nNo need for Zadig or UsbDk."
  RMDir /r "$TEMP\PenumbraDrivers"
SectionEnd
