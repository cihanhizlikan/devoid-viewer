# Devoid Viewer

An ultra-lightweight, production-ready image viewer written in Rust.

## Supported Formats

BMP · PNG · JPEG · GIF · WebP · TIFF · ICO · JPEG XL (`.jxl`) · SVG

## Building

### Prerequisites

- Rust toolchain (stable ≥ 1.75): https://rustup.rs
- On Windows: Visual Studio Build Tools (MSVC) or `x86_64-pc-windows-gnu` toolchain
- On Linux/macOS: standard C compiler + libGL

```sh
# Debug build (fast compile)
cargo build

# Release build (fast runtime, small binary)
cargo build --release
```

The binary is at `target/release/devoid_viewer` (or `.exe` on Windows).

## Usage

```sh
# Open a specific image
devoid_viewer path/to/image.png

# Start blank (drag & drop an image to open it)
devoid_viewer
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `→` / `↓` / `Page Down` | Next image in directory |
| `←` / `↑` / `Page Up`   | Previous image |
| `Delete`                 | Confirm & delete current image |
| `Q` / `Esc`              | Quit |

## Design Notes

### GUI: egui / eframe
Chosen for its near-instant startup (milliseconds to first frame), OpenGL-backed
rendering, and clean Rust-native drag-and-drop API. No native widget toolkit
dependencies beyond OpenGL.

### Threading
- Each image is decoded on a dedicated `std::thread` and sent back via a
  `crossbeam-channel`.
- Adjacent images (next & prev in the directory) are preloaded concurrently so
  navigation feels instant.
- The UI thread never blocks on I/O or decoding.

### SVG
`resvg` + `usvg` rasterize the SVG to a pixel buffer at its intrinsic size.
The result is unpremultiplied to straight RGBA before handing to egui.

### ICO
Handled by the `image` crate's built-in ICO decoder — it picks the largest
frame automatically.

### Window sizing
- First image opened (CLI arg): window resizes to image pixel dimensions.
- Subsequent navigation or drag-and-drop: image is scaled to fit the current
  window (height-first, then width if needed), window never resizes.
