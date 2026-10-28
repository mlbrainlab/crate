; Inno Setup script for Crate on Windows (Inno Setup 6). Built by
; scripts/package-windows.ps1, which passes the version and the folder with
; the release binaries:
;
;   iscc /DVersion=0.2.0 /DBin=..\..\target\release packaging\windows\crate.iss
;
; Installs for the current user by default (no administrator rights needed);
; "Install for all users" is offered in the first page, or /ALLUSERS on the
; command line. Registry keys go under HKCU or HKLM to match (HKA).

#ifndef Version
  #error Pass /DVersion=x.y.z
#endif
#ifndef Bin
  #define Bin "..\..\target\release"
#endif

; The ProgID archives point at, and the name in Settings → Default apps.
#define ProgId "Crate.Archive"
#define AppKey "Software\Crate"

[Setup]
AppId={{6C1C2A4E-6E9B-4E39-9F0B-3A0F6D3C2B71}
AppName=Crate
AppVersion={#Version}
AppVerName=Crate {#Version}
AppPublisher=mlbrainlab
AppPublisherURL=https://github.com/mlbrainlab/crate
AppSupportURL=https://github.com/mlbrainlab/crate/issues
AppUpdatesURL=https://github.com/mlbrainlab/crate/releases
DefaultDirName={autopf}\Crate
DefaultGroupName=Crate
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir=..\..\target\dist
OutputBaseFilename=Crate-{#Version}-x64-setup
SetupIconFile=..\..\assets\icons\crate.ico
UninstallDisplayIcon={app}\crate-app.exe
UninstallDisplayName=Crate
LicenseFile=..\..\LICENSE
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
ChangesAssociations=yes
CloseApplications=yes

[Files]
Source: "{#Bin}\crate-app.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Bin}\crate.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Crate"; Filename: "{app}\crate-app.exe"; Comment: "Open, extract and create archives"

[Registry]
; The archive type: icon and what double-click does once Crate is the default.
Root: HKA; Subkey: "Software\Classes\{#ProgId}"; ValueType: string; ValueName: ""; ValueData: "Archive"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\{#ProgId}"; ValueType: string; ValueName: "FriendlyTypeName"; ValueData: "Archive"
Root: HKA; Subkey: "Software\Classes\{#ProgId}\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\crate-app.exe"",0"
Root: HKA; Subkey: "Software\Classes\{#ProgId}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\crate-app.exe"" ""%1"""

; "Open with" for the app itself.
Root: HKA; Subkey: "Software\Classes\Applications\crate-app.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Crate"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\crate-app.exe\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\crate-app.exe"" ""%1"""

; Settings → Default apps lists Crate under this name; the first run opens
; that page (ms-settings:defaultapps?registeredAppUser=Crate).
Root: HKA; Subkey: "{#AppKey}"; Flags: uninsdeletekey
Root: HKA; Subkey: "{#AppKey}\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Crate"
Root: HKA; Subkey: "{#AppKey}\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Open, extract and create archives"
Root: HKA; Subkey: "{#AppKey}\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: """{app}\crate-app.exe"",0"
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Crate"; ValueData: "{#AppKey}\Capabilities"; Flags: uninsdeletevalue

; For each archive extension: listed under "Open with", in Default apps, and
; Explorer's right-click entries (classic verbs; on Windows 11 they're under
; "Show more options").
#dim Exts[13] {".zip", ".7z", ".rar", ".tar", ".tgz", ".gz", ".txz", ".xz", ".tzst", ".zst", ".tbz2", ".bz2", ".001"}
#define I
#sub RegisterExt
Root: HKA; Subkey: "Software\Classes\{#Exts[I]}\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\crate-app.exe\SupportedTypes"; ValueType: string; ValueName: "{#Exts[I]}"; ValueData: ""
Root: HKA; Subkey: "{#AppKey}\Capabilities\FileAssociations"; ValueType: string; ValueName: "{#Exts[I]}"; ValueData: "{#ProgId}"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\{#Exts[I]}\shell\Crate.Extract"; ValueType: string; ValueName: ""; ValueData: "Extract with Crate"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\{#Exts[I]}\shell\Crate.Extract"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\crate-app.exe"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\{#Exts[I]}\shell\Crate.Extract\command"; ValueType: string; ValueName: ""; ValueData: """{app}\crate-app.exe"" --extract-each ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\{#Exts[I]}\shell\Crate.Open"; ValueType: string; ValueName: ""; ValueData: "Open with Crate"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\{#Exts[I]}\shell\Crate.Open"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\crate-app.exe"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\{#Exts[I]}\shell\Crate.Open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\crate-app.exe"" ""%1"""
#endsub
#for {I = 0; I < DimOf(Exts); I++} RegisterExt

; Any file or folder: "Compress with Crate…" (the New archive dialog).
Root: HKA; Subkey: "Software\Classes\*\shell\Crate.Compress"; ValueType: string; ValueName: ""; ValueData: "Compress with Crate…"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\*\shell\Crate.Compress"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\crate-app.exe"",0"
Root: HKA; Subkey: "Software\Classes\*\shell\Crate.Compress\command"; ValueType: string; ValueName: ""; ValueData: """{app}\crate-app.exe"" --compress ""%1"""
Root: HKA; Subkey: "Software\Classes\Directory\shell\Crate.Compress"; ValueType: string; ValueName: ""; ValueData: "Compress with Crate…"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Directory\shell\Crate.Compress"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\crate-app.exe"",0"
Root: HKA; Subkey: "Software\Classes\Directory\shell\Crate.Compress\command"; ValueType: string; ValueName: ""; ValueData: """{app}\crate-app.exe"" --compress ""%1"""

[Run]
Filename: "{app}\crate-app.exe"; Description: "Open Crate"; Flags: nowait postinstall skipifsilent
