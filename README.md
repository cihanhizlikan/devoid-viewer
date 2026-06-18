# Devoid Viewer

An ultralight, Windows-native image viewer built in Rust.

<!-- A screenshot here sells the "clean and minimal" pitch instantly — add one. -->
![Devoid Viewer screenshot](docs/screenshot.png)

Devoid Viewer is built around a single goal: **open an image instantly, with the smallest possible footprint.** No bloat, no heavy dependencies, no editing surface to get in the way — just a fast, no-friction way to look at images on Windows.

It's the deliberately minimal counterpart to [patina](https://github.com/cihanhizlikan/patina), my cross-platform image *editor*. Where patina trades weight for a full non-destructive editing workflow, Devoid Viewer trades features for raw speed and simplicity.

## Design goals
- **Instant startup** — minimal initialisation, ready the moment it opens
- **Tiny footprint** — lean memory and binary size by design
- **Windows-native** — built specifically for a smooth Windows experience
- **View-only focus** — does one job, and stays out of your way

## Tech stack
Rust · Windows

## Building from source
```bash
cargo build --release
```
The release binary will be in `target/release/`.

## Related project
- **[patina](https://github.com/cihanhizlikan/patina)** — cross-platform image viewer & non-destructive editor (RAW, EXIF/IPTC, XMP sidecar ratings, export presets).

## License
<!-- Add your license here, e.g. MIT or Apache-2.0 -->
