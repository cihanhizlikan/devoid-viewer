; Devoid Viewer — Inno Setup 6 installer script
; Place this file in the project root alongside Cargo.toml.
; Build with: iscc devoid_viewer_setup.iss
; Output: Output\DevoidViewer_Setup.exe

#define AppName      "Devoid Viewer"
#define AppVersion   "0.1.0"
#define AppPublisher "Fatih Cihan Hizlikan"
#define AppExeName   "DevoidViewer.exe"
#define AppId        "{{42131F52-0DFF-4B4C-868B-4656FD6F3F89}"

[Setup]
AppId={#AppId}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL=https://github.com/yourusername/devoid-viewer
AppSupportURL=https://github.com/yourusername/devoid-viewer
AppUpdatesURL=https://github.com/yourusername/devoid-viewer

; Per-user by default, no UAC. User can choose "Install for all users"
; on the scope page which re-launches elevated and switches to per-machine.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog

; Default install paths — Inno picks the right one based on scope.
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes

; Output
OutputDir=Output
OutputBaseFilename=DevoidViewer_Setup
SetupIconFile=devoid.ico
UninstallDisplayIcon={app}\{#AppExeName}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern

; The installer is 64-bit aware but installs a native binary.
; Force 64-bit install path on 64-bit Windows.
ArchitecturesInstallIn64BitMode=x64compatible arm64

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
; These are the optional features shown on the installer's options page.
Name: "openwith";   Description: "Add to &Open With menu for all supported image formats";      GroupDescription: "File Associations:"; Flags: checkedonce
Name: "setdefault"; Description: "&Set as default image viewer for all supported formats";      GroupDescription: "File Associations:"; Flags: checkedonce
Name: "desktopicon"; Description: "Create a &desktop shortcut";                                 GroupDescription: "Additional icons:"; Flags: checkedonce

[Files]
Source: "target\release\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion
; If you add other files (e.g. a README), list them here.

[Icons]
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; \
    IconFilename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Registry]
; ── ProgId registration ──────────────────────────────────────────────────────
; HKA resolves to HKLM for admin (per-machine) installs and HKCU for per-user.
; This is Inno Setup's recommended approach — it automatically writes to the
; correct hive based on the install scope the user chose.
Root: HKA; Subkey: "Software\Classes\DevoidViewer.ImageFile"; \
    ValueType: string; ValueName: ""; ValueData: "Image File"; \
    Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\DevoidViewer.ImageFile\DefaultIcon"; \
    ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\DevoidViewer.ImageFile\shell\open\command"; \
    ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""

; ── Open With ────────────────────────────────────────────────────────────────
; Adds Devoid Viewer to the Open With menu for each format.
; HKA ensures the right hive is used. Tasks: condition makes these
; optional — only installed when the user ticks "Add to Open With".
Root: HKA; Subkey: "Software\Classes\.jpg\OpenWithProgIds";  ValueType: string; ValueName: "DevoidViewer.ImageFile"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\.jpeg\OpenWithProgIds"; ValueType: string; ValueName: "DevoidViewer.ImageFile"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\.png\OpenWithProgIds";  ValueType: string; ValueName: "DevoidViewer.ImageFile"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\.bmp\OpenWithProgIds";  ValueType: string; ValueName: "DevoidViewer.ImageFile"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\.gif\OpenWithProgIds";  ValueType: string; ValueName: "DevoidViewer.ImageFile"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\.webp\OpenWithProgIds"; ValueType: string; ValueName: "DevoidViewer.ImageFile"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\.tiff\OpenWithProgIds"; ValueType: string; ValueName: "DevoidViewer.ImageFile"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\.tif\OpenWithProgIds";  ValueType: string; ValueName: "DevoidViewer.ImageFile"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\.ico\OpenWithProgIds";  ValueType: string; ValueName: "DevoidViewer.ImageFile"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\.jxl\OpenWithProgIds";  ValueType: string; ValueName: "DevoidViewer.ImageFile"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\.svg\OpenWithProgIds";  ValueType: string; ValueName: "DevoidViewer.ImageFile"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith

; ── Set as Default ───────────────────────────────────────────────────────────
; Makes Devoid Viewer the default opener for each format.
; Only installed when the user ticks "Set as default image viewer".
Root: HKA; Subkey: "Software\Classes\.jpg";  ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault
Root: HKA; Subkey: "Software\Classes\.jpeg"; ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault
Root: HKA; Subkey: "Software\Classes\.png";  ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault
Root: HKA; Subkey: "Software\Classes\.bmp";  ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault
Root: HKA; Subkey: "Software\Classes\.gif";  ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault
Root: HKA; Subkey: "Software\Classes\.webp"; ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault
Root: HKA; Subkey: "Software\Classes\.tiff"; ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault
Root: HKA; Subkey: "Software\Classes\.tif";  ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault
Root: HKA; Subkey: "Software\Classes\.ico";  ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault
Root: HKA; Subkey: "Software\Classes\.jxl";  ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault
Root: HKA; Subkey: "Software\Classes\.svg";  ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault

[Run]
; Notify the shell that file associations have changed so Explorer
; updates file type icons immediately without requiring a reboot.
Filename: "{sys}\ie4uinit.exe"; Parameters: "-ClearIconCache"; \
    Flags: runhidden nowait; StatusMsg: "Refreshing file type icons..."

[UninstallRun]
Filename: "{sys}\ie4uinit.exe"; Parameters: "-ClearIconCache"; \
    RunOnceId: "RefreshIcons"; Flags: runhidden nowait
