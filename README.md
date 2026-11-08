# Devoid Viewer

An ultralight, Windows-native image viewer built in Rust.

Devoid Viewer is built around a single goal: **open an image instantly, with the smallest possible footprint.** No bloat, no heavy dependencies, no editing surface to get in the way — just a fast, no-friction way to look at images on Windows.

It's the deliberately minimal counterpart to [patina](https://github.com/cihanhizlikan/patina), my cross-platform image *editor*. Where patina trades weight for a full non-destructive editing workflow, Devoid Viewer trades features for raw speed and simplicity.

## Design goals
- **Instant startup** — minimal initialisation, ready the moment it opens
- **Tiny footprint** — lean memory and binary size by design
- **Windows-native** — built specifically for a smooth Windows experience
- **View-only focus** — does one job, and stays out of your way

## Install
Run `DevoidViewer_Setup.exe` on 64-bit Windows 11 or 10 (built from this repository, see
[Building from source](#building-from-source)). It installs for you alone with no administrator prompt, or for
every user of the PC if you choose that when it asks.

The installer never takes over your images. To make Devoid Viewer the app that opens them, tick *Choose Devoid
Viewer as the default in Windows Settings* on the last page, or later open **Settings → Apps → Default apps →
Devoid Viewer** and pick it for the types you want.

## Formats
| Format | Extensions |
|---|---|
| BMP | `.bmp` |
| PNG | `.png` |
| JPEG | `.jpg` `.jpeg` |
| GIF, animated too | `.gif` |
| WebP | `.webp` |
| TIFF | `.tiff` `.tif` |
| ICO | `.ico` |
| JPEG XL | `.jxl` |

The file's contents decide, not its name: a PNG saved as `.jpg` still opens, and Devoid Viewer offers to rename
it to `.png`. Images up to 32,768 pixels on a side open; a larger one is refused before anything is decoded.

## Keys
Press **H** in the viewer to show this table over the image; **H** or **Esc** hides it. Letter keys work whatever
the keyboard layout or Caps Lock state.

| Key | Action |
|---|---|
| Right · Down · PgDn | Next image |
| Left · Up · PgUp | Previous image |
| Mouse wheel | Next / previous image |
| Delete | Move to the Recycle Bin |
| F or F11 | Fullscreen on / off |
| Esc | Close help, leave fullscreen |
| Double-click | Maximise / restore |
| Drop a file | Open it |
| H | Show / hide this help |
| Q | Quit |

On laptops whose top row defaults to media and hotkeys, F11 needs **Fn**; **F** works either way.

## Tech stack
Rust · Windows

## Building from source
Needs [Rust](https://rustup.rs) (rustup installs the pinned toolchain from `rust-toolchain.toml` by itself) and
the MSVC linker: Visual Studio's *Desktop development with C++* workload, or the standalone
[Build Tools for Visual Studio](https://visualstudio.microsoft.com/visual-cpp-build-tools/). Run cargo from
PowerShell or a Developer prompt; Git Bash puts its own `link` first on `PATH` and the build fails.

```powershell
cargo build --release --locked
```
The viewer is `target\release\DevoidViewer.exe`. The installer needs [Inno Setup 7](https://jrsoftware.org/isinfo.php)
and the release build above:

```powershell
iscc devoid_viewer_setup.iss
```
It writes `Output\DevoidViewer_Setup.exe`; [docs/installer.md](docs/installer.md) says what it registers.

## Related project
- **[patina](https://github.com/cihanhizlikan/patina)** — cross-platform image viewer & non-destructive editor (RAW, EXIF/IPTC, XMP sidecar ratings, export presets).

## License
Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your
option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Devoid Viewer by
you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or
conditions.
