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

Two other ways to the default work as well, both measured on Windows 11: "Open with → Always use this app" in
Explorer's prompt, and the Settings page. Both record the choice through Windows.

### Windows 10 (not tested: no Windows 10 machine was available)

- **Nothing meaningful was lost.** Windows 10 also resolves `UserChoice` first and gives new accounts Photos for
  the common types, so the old task did as little there as on Windows 11, and had the same uninstall damage.
- **Possible one-time prompt.** For types Windows 10 does not hand to Photos (at least `.jxl`; `.webp` and
  `.ico` depending on the build), the old task on a per-machine install made Devoid Viewer open them directly.
  Now Windows may ask "How do you want to open this file?" once per type.
- **The finish-page link is a Windows 11 feature.** The `registeredAppUser`/`registeredAppMachine` parameters
  open the app's own page on Windows 11. On Windows 10 the link is expected to open the general Default apps
  page, where the app is under **Set defaults by app → Devoid Viewer → Manage**; the registration is what lists
  it there. This is the one check to make on a Windows 10 machine.
- **Gains:** a per-user install can now be made the default (the old task was per-machine only), and the
  uninstall cleanup works as designed, since Windows 10 keeps the choice in `UserChoice`.

Consumer Windows 10 left extended security updates in October 2026, so Windows-10-only behaviour gets no
further work unless a user reports it.

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
