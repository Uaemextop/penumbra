; ============================================================================
; MTK USB Serial Driver — Inno Setup Installer Script
; Builds a single .exe installer for Windows 11 x64
; Note: Windows 11 (and WDK 10.0.26100) only supports x64 kernel-mode drivers
; ============================================================================

#define AppName "MTK Preloader USB Serial Driver"
#define AppVersion "1.0.0"
#define AppPublisher "MTK Loader Drivers Opensource"
#define AppURL "https://github.com/Uaemextop/mtk-loader-drivers-opensource-win11"

[Setup]
AppId={{E8D00003-MTK0-USB0-VCOM-PRELOADER001}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
DefaultDirName={autopf}\MTK USB Driver
DefaultGroupName={#AppName}
OutputBaseFilename=MTK_USB_Driver_Setup_{#AppVersion}
Compression=lzma2/ultra64
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64compatible
ArchitecturesAllowed=x64compatible
MinVersion=10.0.22000
PrivilegesRequired=admin
SetupIconFile=compiler:SetupClassicIcon.ico
UninstallDisplayName={#AppName}
LicenseFile=..\LICENSE
OutputDir=output

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[Messages]
english.WelcomeLabel2=This will install the MediaTek USB Serial driver on your computer.%n%nThis driver enables communication with MTK devices in Preloader, BROM, Download Agent (DA), and Meta modes.%n%nCompatible with SP Flash Tool and mtkclient.
spanish.WelcomeLabel2=Este instalador instalará el driver USB Serial de MediaTek en su computadora.%n%nEste driver habilita la comunicación con dispositivos MTK en modos Preloader, BROM, Download Agent (DA) y Meta.%n%nCompatible con SP Flash Tool y mtkclient.

[Files]
; x64 driver files
Source: "build\x64\Release\mtk_usb2ser.sys"; DestDir: "{app}\x64"; Flags: ignoreversion
Source: "build\x64\Release\mtk_usb2ser.inf"; DestDir: "{app}"; Flags: ignoreversion
Source: "build\x64\Release\mtk_usb2ser.cat"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "build\x64\Release\WdfCoInstaller01033.dll"; DestDir: "{app}\x64"; Flags: ignoreversion skipifsourcedoesntexist

; Common files
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\docs\INSTALL.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "install_driver.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "uninstall_driver.ps1"; DestDir: "{app}"; Flags: ignoreversion

[Run]
; Install driver using pnputil after file copy
Filename: "powershell.exe"; \
    Parameters: "-ExecutionPolicy Bypass -File ""{app}\install_driver.ps1"" -InfPath ""{app}\mtk_usb2ser.inf"""; \
    StatusMsg: "Installing MediaTek USB Serial driver..."; \
    Flags: runhidden waituntilterminated; \
    Check: ShouldInstallDriver

[UninstallRun]
Filename: "powershell.exe"; \
    Parameters: "-ExecutionPolicy Bypass -File ""{app}\uninstall_driver.ps1"""; \
    Flags: runhidden waituntilterminated

[Code]
function ShouldInstallDriver: Boolean;
begin
  Result := True;
end;

function InitializeSetup(): Boolean;
begin
  if not IsAdmin then
  begin
    MsgBox('This installer requires administrator privileges.' + #13#10 +
           'Please right-click and select "Run as administrator".', mbError, MB_OK);
    Result := False;
    Exit;
  end;
  Result := True;
end;
