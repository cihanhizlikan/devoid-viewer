# Installer

`devoid_viewer_setup.iss`, Inno Setup 6. Build the release binary, then run:

```powershell
iscc devoid_viewer_setup.iss    # → Output\DevoidViewer_Setup.exe
```

It packages `target\release\DevoidViewer.exe`, an x86_64 build.

## Why Inno Setup, not WiX 3

WiX 3 evaluates component conditions during MSI costing, before any UI is shown, so the user cannot switch
between a per-user and a per-machine install. Inno Setup does this with `IsAdminInstallMode` and the `HKA` root.

## Scope

`PrivilegesRequired=lowest` with `PrivilegesRequiredOverridesAllowed=dialog`: per-user by default with no UAC
prompt, per-machine (elevated) if the user picks it. `HKA` resolves to `HKCU` or `HKLM` to match.

## File associations

Ten extensions: `jpg jpeg png bmp gif webp tiff tif ico jxl`. A test (`loader.rs`,
`installer_associates_exactly_the_supported_extensions`) fails if this list and `loader::is_supported` disagree.

| Feature | Registry | Notes |
|---|---|---|
| ProgId | `HKA\Software\Classes\DevoidViewer.ImageFile` | Always installed. |
| Open With | `HKA\Software\Classes\.ext\OpenWithProgIds` | Adds the app without changing the default. |
| Set as default | `HKLM\Software\Classes\.ext` | Per-machine installs only. |
| App Paths | `...\CurrentVersion\App Paths\DevoidViewer.exe` | Lets archivers launch it through `ShellExecuteEx`. |

**The default is written to `HKLM` only.** Writing `HKCU\Software\Classes\.ext` makes Windows create a
`UserChoice` entry, which carries a hash Windows checks on every open, survives uninstall, and leaves a broken
association behind.

## Uninstall

- `taskkill` closes running instances before files are removed.
- The install folder is always removed (`uninsalwaysuninstall`).
- The `UserChoice` keys for all ten extensions are deleted.
- `ie4uinit.exe -ClearIconCache` refreshes the shell's icons.

The installer version (`#define AppVersion`) must match `Cargo.toml`; `installer_version_matches_cargo` in
`main.rs` enforces it.
