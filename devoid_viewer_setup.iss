; Devoid Viewer — Inno Setup 7 installer script
; Place this file in the project root alongside Cargo.toml.
; Build with: iscc devoid_viewer_setup.iss, after cargo build --release
; Output: Output\DevoidViewer_Setup.exe

#define AppName      "Devoid Viewer"
#define AppExeName   "DevoidViewer.exe"
#define AppExePath   AddBackslash(SourcePath) + "target\release\" + AppExeName

; The version is read from the exe it packs, whose version block winres
; (build.rs) stamps from Cargo.toml's package.version: one source, never typed here.
#if !FileExists(AppExePath)
  #error target\release\DevoidViewer.exe is missing: run cargo build --release first
#endif
#define AppVersion   GetStringFileInfo(AppExePath, "ProductVersion")
#if AppVersion == ""
  #error DevoidViewer.exe carries no ProductVersion: its version block was not embedded
#endif
#define AppPublisher "Fatih Cihan Hizlikan"
#define AppRegName   "DevoidViewer"
#define AppNameUrl   "Devoid%20Viewer"
#define AppId        "{{42131F52-0DFF-4B4C-868B-4656FD6F3F89}"

[Setup]
AppId={#AppId}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL=https://github.com/cihanhizlikan/devoid-viewer
AppSupportURL=https://github.com/cihanhizlikan/devoid-viewer
AppUpdatesURL=https://github.com/cihanhizlikan/devoid-viewer

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

; The exe is x64 only: install in 64-bit mode on x64 Windows and on Arm64
; Windows 11, which runs x64 code, and refuse 32-bit Windows, which cannot.
; The installer itself stays 32-bit (Inno Setup 7's default): a 64-bit one
; (SetupArchitecture=x64) installs the same files and registry entries but
; is 745,890 bytes larger (docs/installer.md).
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "openwith";    Description: "Add to &Open With menu for all supported image formats";  GroupDescription: "File Associations:"; Flags: checkedonce
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

; ── Default apps registration ────────────────────────────────────────────────
; Lists Devoid Viewer in Settings → Apps → Default apps, with a page for its
; ten types, where the user makes it the default. The installer never sets the
; default itself: Windows keeps the user's choice in a hashed UserChoice key
; that only Windows writes, and the machine-wide Classes\.ext value it would
; otherwise overwrite belongs to Windows (.jpg → jpegfile) and is only read
; when the user has made no choice. Every key here is ours and goes on
; uninstall. HKA follows the install scope, so per-user installs work too.
Root: HKA; Subkey: "Software\{#AppRegName}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\{#AppRegName}\Capabilities"; \
    ValueType: string; ValueName: "ApplicationName"; ValueData: "{#AppName}"
Root: HKA; Subkey: "Software\{#AppRegName}\Capabilities"; \
    ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Ultralight, view-only image viewer"
Root: HKA; Subkey: "Software\{#AppRegName}\Capabilities"; \
    ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\{#AppRegName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".jpg";  ValueData: "DevoidViewer.ImageFile"
Root: HKA; Subkey: "Software\{#AppRegName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".jpeg"; ValueData: "DevoidViewer.ImageFile"
Root: HKA; Subkey: "Software\{#AppRegName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".png";  ValueData: "DevoidViewer.ImageFile"
Root: HKA; Subkey: "Software\{#AppRegName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".bmp";  ValueData: "DevoidViewer.ImageFile"
Root: HKA; Subkey: "Software\{#AppRegName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".gif";  ValueData: "DevoidViewer.ImageFile"
Root: HKA; Subkey: "Software\{#AppRegName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".webp"; ValueData: "DevoidViewer.ImageFile"
Root: HKA; Subkey: "Software\{#AppRegName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".tiff"; ValueData: "DevoidViewer.ImageFile"
Root: HKA; Subkey: "Software\{#AppRegName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".tif";  ValueData: "DevoidViewer.ImageFile"
Root: HKA; Subkey: "Software\{#AppRegName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".ico";  ValueData: "DevoidViewer.ImageFile"
Root: HKA; Subkey: "Software\{#AppRegName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".jxl";  ValueData: "DevoidViewer.ImageFile"
Root: HKA; Subkey: "Software\RegisteredApplications"; \
    ValueType: string; ValueName: "{#AppName}"; ValueData: "Software\{#AppRegName}\Capabilities"; \
    Flags: uninsdeletevalue

[Run]
Filename: "{sys}\ie4uinit.exe"; Parameters: "-ClearIconCache"; \
    Flags: runhidden nowait; StatusMsg: "Refreshing file type icons..."
; Finish-page checkbox: open Devoid Viewer's own page in Default apps.
Filename: "ms-settings:defaultapps?registeredAppUser={#AppNameUrl}"; \
    Description: "Choose Devoid Viewer as the default in Windows Settings"; \
    Flags: postinstall shellexec skipifsilent nowait; Check: not IsAdminInstallMode
Filename: "ms-settings:defaultapps?registeredAppMachine={#AppNameUrl}"; \
    Description: "Choose Devoid Viewer as the default in Windows Settings"; \
    Flags: postinstall shellexec skipifsilent nowait; Check: IsAdminInstallMode

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/f /im {#AppExeName}"; \
    RunOnceId: "KillDevoid"; Flags: runhidden skipifdoesntexist
Filename: "{sys}\ie4uinit.exe"; Parameters: "-ClearIconCache"; \
    RunOnceId: "RefreshIcons"; Flags: runhidden nowait

[Code]
// ── UserChoice cleanup on uninstall ─────────────────────────────────────────
// When the user picks Devoid Viewer as an extension's default (Open With →
// Always), Windows records it in a UserChoice key, which survives uninstall
// and leaves the extension pointing at a deleted ProgId. Remove a UserChoice
// only if it names Devoid Viewer: one naming another app is the user's
// choice and is never touched, at install or uninstall. Failure is ignored;
// Windows' UserChoice protection may refuse the delete.
procedure ForgetOurChoice(const Ext: String);
var
  Key, ProgId: String;
begin
  Key := 'Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.' + Ext + '\UserChoice';
  if RegQueryStringValue(HKCU, Key, 'ProgId', ProgId) and
     ((ProgId = 'DevoidViewer.ImageFile') or (ProgId = 'Applications\{#AppExeName}')) then
    RegDeleteKeyIncludingSubkeys(HKCU, Key);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    ForgetOurChoice('jpg');
    ForgetOurChoice('jpeg');
    ForgetOurChoice('png');
    ForgetOurChoice('bmp');
    ForgetOurChoice('gif');
    ForgetOurChoice('webp');
    ForgetOurChoice('tiff');
    ForgetOurChoice('tif');
    ForgetOurChoice('ico');
    ForgetOurChoice('jxl');
  end;
end;
