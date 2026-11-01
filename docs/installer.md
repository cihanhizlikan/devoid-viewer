# Installer

`devoid_viewer_setup.iss`, Inno Setup 7 (7.1.1). Build the release binary, then run:

```powershell
& 'C:\Program Files\Inno Setup 7\ISCC.exe' devoid_viewer_setup.iss    # → Output\DevoidViewer_Setup.exe
```

It packages `target\release\DevoidViewer.exe`, an x86_64 build. `ArchitecturesAllowed=x64compatible` installs
it in 64-bit mode on x64 and on Arm64 Windows 11, and refuses 32-bit Windows, which cannot run it.

**The installer itself is 32-bit**, Inno Setup 7's default. A 64-bit one (`SetupArchitecture=x64`) installs the
same files and registry entries but measured 3,948,895 bytes against 3,202,996 (+745,890, +23%), for benefits
(no WOW64, long paths) this installer does not use.

## Why Inno Setup, not WiX 3

WiX 3 evaluates component conditions during MSI costing, before any UI is shown, so the user cannot switch
between a per-user and a per-machine install. Inno Setup does this with `IsAdminInstallMode` and the `HKA` root.
The alternatives were reviewed again on 2026-11-01 (dist, cargo-packager, Velopack, WiX v6/v7, MSIX); see
*The installer stays Inno Setup* in [settled-decisions.md](settled-decisions.md).

## Scope

`PrivilegesRequired=lowest` with `PrivilegesRequiredOverridesAllowed=dialog`: per-user by default with no UAC
prompt, per-machine (elevated) if the user picks it. `HKA` resolves to `HKCU` or `HKLM` to match.

## File associations

Ten extensions: `jpg jpeg png bmp gif webp tiff tif ico jxl`, listed three times in the script (Open With,
Default apps, `UserChoice` cleanup). A test (`loader.rs`, `installer_associates_exactly_the_supported_extensions`)
fails if any of the three and `loader::is_supported` disagree.

| Feature | Registry | Notes |
|---|---|---|
| ProgId | `HKA\Software\Classes\DevoidViewer.ImageFile` | Always installed. |
| Open With | `HKA\Software\Classes\.ext\OpenWithProgIds` | Adds the app without changing the default. |
| Default apps | `HKA\Software\DevoidViewer\Capabilities` + `HKA\Software\RegisteredApplications` | Lists the app and its ten types in Settings → Default apps. Always installed. |
| App Paths | `...\CurrentVersion\App Paths\DevoidViewer.exe` | Lets archivers launch it through `ShellExecuteEx`. |

**The installer never sets the default.** Windows resolves an extension from the user's `UserChoice` first, a
hashed key only Windows writes, then the machine-wide `HKLM\Software\Classes\.ext`, which belongs to Windows
(`.jpg` → `jpegfile`) and is read only where the user has made no choice; Windows 11 gives every new account a
`UserChoice` for Photos. The installer registers the app's capabilities instead, and the finish page offers to
open its own page in Settings (`ms-settings:defaultapps?registeredAppUser=…`, or `registeredAppMachine` for a
per-machine install), where the user picks it and Windows records that choice.

Until 2026-11-01 a "Set as default" task overwrote `HKLM\Software\Classes\.ext` and deleted the value on
uninstall, so `.jpg` lost `jpegfile` machine-wide, while changing nothing for any user with a `UserChoice`.
The same script deleted every user's `UserChoice` for all ten types at install (`deletekey`), whatever app it
named.

## Uninstall

- `taskkill` closes running instances before files are removed.
- The install folder is always removed (`uninsalwaysuninstall`).
- A `UserChoice` is deleted only if it names Devoid Viewer (`DevoidViewer.ImageFile` or
  `Applications\DevoidViewer.exe`), so the extension falls back to Windows' default rather than a deleted
  ProgId. A choice naming another app is never touched, at install or uninstall. This is `[Code]`, since
  `[Registry]` flags cannot test a value before deleting.
- **Measured on Windows 11 build 26300 (2026-11-01):** a default chosen in Settings did not appear in either
  `UserChoice` or `UserChoiceLatest` (both still named Photos, as did `AssocQueryString`), yet Explorer opened
  `.jpg` in Devoid Viewer. That build keeps the choice elsewhere, so the cleanup above finds nothing there. After
  uninstall, Explorer did not fail: it fell back to another handler registered for `.jpg` (Huawei's
  `HwPhotoViewer`, not Photos). The uninstaller cannot pick the fallback; only the user can set a default.
- `ie4uinit.exe -ClearIconCache` refreshes the shell's icons.

The installer version (`#define AppVersion`) must match `Cargo.toml`; `installer_version_matches_cargo` in
`main.rs` enforces it.
