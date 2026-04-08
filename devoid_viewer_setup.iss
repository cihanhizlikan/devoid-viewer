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

; Force 64-bit install path on 64-bit Windows.
ArchitecturesInstallIn64BitMode=x64compatible arm64

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "openwith";    Description: "Add to &Open With menu for all supported image formats";  GroupDescription: "File Associations:"; Flags: checkedonce
Name: "setdefault";  Description: "&Set as default image viewer for all supported formats";  GroupDescription: "File Associations:"; Flags: checkedonce
Name: "desktopicon"; Description: "Create a &desktop shortcut";                              GroupDescription: "Additional icons:"; Flags: checkedonce

[Files]
Source: "target\release\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion

[Dirs]
Name: "{app}"; Flags: uninsalwaysuninstall

[Icons]
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; \
    IconFilename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Registry]
; ── App Paths ────────────────────────────────────────────────────────────────
; Lets Windows and archivers (7-Zip etc.) find and launch the application
; via ShellExecuteEx from any context including files inside archives.
Root: HKLM; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\{#AppExeName}"; \
    ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName}"; \
    Flags: uninsdeletekey; Check: IsAdminInstallMode
Root: HKLM; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\{#AppExeName}"; \
    ValueType: string; ValueName: "Path"; ValueData: "{app}"; \
    Check: IsAdminInstallMode
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\{#AppExeName}"; \
    ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName}"; \
    Flags: uninsdeletekey; Check: not IsAdminInstallMode
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\{#AppExeName}"; \
    ValueType: string; ValueName: "Path"; ValueData: "{app}"; \
    Check: not IsAdminInstallMode

; ── ProgId ───────────────────────────────────────────────────────────────────
; HKA = HKLM on per-machine installs, HKCU on per-user installs.
Root: HKA; Subkey: "Software\Classes\DevoidViewer.ImageFile"; \
    ValueType: string; ValueName: ""; ValueData: "Image File"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\DevoidViewer.ImageFile\DefaultIcon"; \
    ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\DevoidViewer.ImageFile\shell\open\command"; \
    ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""

; ── Open With ────────────────────────────────────────────────────────────────
; Adds to Open With menu without changing the existing default.
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

; ── Set as Default ───────────────────────────────────────────────────────────
; Only writes to HKLM (requires per-machine install / admin).
; Does NOT write to HKCU\Software\Classes because that causes Windows to
; generate a UserChoice entry which survives uninstall and leaves a broken
; association pointing at a deleted ProgId.
Root: HKLM; Subkey: "Software\Classes\.jpg";  ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault; Check: IsAdminInstallMode
Root: HKLM; Subkey: "Software\Classes\.jpeg"; ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault; Check: IsAdminInstallMode
Root: HKLM; Subkey: "Software\Classes\.png";  ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault; Check: IsAdminInstallMode
Root: HKLM; Subkey: "Software\Classes\.bmp";  ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault; Check: IsAdminInstallMode
Root: HKLM; Subkey: "Software\Classes\.gif";  ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault; Check: IsAdminInstallMode
Root: HKLM; Subkey: "Software\Classes\.webp"; ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault; Check: IsAdminInstallMode
Root: HKLM; Subkey: "Software\Classes\.tiff"; ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault; Check: IsAdminInstallMode
Root: HKLM; Subkey: "Software\Classes\.tif";  ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault; Check: IsAdminInstallMode
Root: HKLM; Subkey: "Software\Classes\.ico";  ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault; Check: IsAdminInstallMode
Root: HKLM; Subkey: "Software\Classes\.jxl";  ValueType: string; ValueName: ""; ValueData: "DevoidViewer.ImageFile"; Flags: uninsdeletevalue; Tasks: setdefault; Check: IsAdminInstallMode

; ── UserChoice cleanup on uninstall ─────────────────────────────────────────
; Windows writes UserChoice entries when HKCU\Software\Classes\.ext is set.
; These survive uninstall and leave a broken association pointing at the
; now-deleted ProgId. Clean them up so no stale entries remain.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.jpg\UserChoice";  Flags: deletekey uninsdeletekey
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.jpeg\UserChoice"; Flags: deletekey uninsdeletekey
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.png\UserChoice";  Flags: deletekey uninsdeletekey
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.bmp\UserChoice";  Flags: deletekey uninsdeletekey
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.gif\UserChoice";  Flags: deletekey uninsdeletekey
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.webp\UserChoice"; Flags: deletekey uninsdeletekey
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.tiff\UserChoice"; Flags: deletekey uninsdeletekey
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.tif\UserChoice";  Flags: deletekey uninsdeletekey
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.ico\UserChoice";  Flags: deletekey uninsdeletekey
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.jxl\UserChoice";  Flags: deletekey uninsdeletekey

[Run]
Filename: "{sys}\ie4uinit.exe"; Parameters: "-ClearIconCache"; \
    Flags: runhidden nowait; StatusMsg: "Refreshing file type icons..."

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/f /im {#AppExeName}"; \
    RunOnceId: "KillDevoid"; Flags: runhidden skipifdoesntexist
Filename: "{sys}\ie4uinit.exe"; Parameters: "-ClearIconCache"; \
    RunOnceId: "RefreshIcons"; Flags: runhidden nowait
