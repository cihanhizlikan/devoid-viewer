# Benchmarks

`scripts/bench.ps1` measures the release build from outside the process, so no instrumentation reaches the
shipped binary. Run it before and after any change that could add startup work, memory or binary size, and put
both tables in the commit body.

```powershell
./scripts/bench.ps1              # builds release, 15 runs per case
./scripts/bench.ps1 -NoBuild -Runs 30
```

**Time to visible** runs from process start until the viewer window is shown. With a path argument the window
starts hidden and is shown only once the first image has decoded, so this is time-to-image. Each fixture sits
alone in its own directory, so no sibling preloads run. The fixtures are generated into `%TEMP%\devoid-bench`
and are not committed.

**Not in CI:** shared GitHub runners vary by tens of percent from run to run, which would hide the regressions
this exists to catch. Compare on one machine.

## Probe trap

`Process.MainWindowHandle` is the wrong probe. winit's hidden helper window (class `Winit Thread Event Target`)
reports `WS_VISIBLE` within about 25 ms of launch, before the decode has even started. A first version of the
script used `MainWindowHandle` and reported a 12 MP JPEG opening as fast as the empty window. The script now waits
for a visible window of the process that is not the helper window.

## Baseline — 2026-11-01, `caeb287`

Windows 11 26300, MSVC release profile (thin LTO, `codegen-units = 1`, stripped). Medians of 15 runs; two
consecutive runs agreed within 1 ms.

| Case | Visible ms (median) | Visible ms (min) | Working set MB | Peak WS MB | Private MB |
|---|---:|---:|---:|---:|---:|
| No image (empty window) | 51.6 | 46.6 | 18.4 | 18.4 | 5.5 |
| PNG 1920×1080 | 87.0 | 81.1 | 31.8 | 31.8 | 18.6 |
| JPEG 4000×3000 (12 MP) | 148.5 | 140.8 | 67.9 | 100.8 | 54.8 |

Binary size: `DevoidViewer.exe` is 4,607,488 bytes.

### After `panic = "abort"` — 2026-11-01

| Case | Visible ms (median) | Visible ms (min) | Working set MB | Peak WS MB | Private MB |
|---|---:|---:|---:|---:|---:|
| No image (empty window) | 49.6 | 45.9 | 18.2 | 18.2 | 5.5 |
| PNG 1920×1080 | 82.2 | 72.9 | 31.5 | 31.5 | 18.5 |
| JPEG 4000×3000 (12 MP) | 141.6 | 137.7 | 67.6 | 100.6 | 54.7 |

Binary size: 3,595,776 bytes (−1,013,248, −22%). Startup and memory are unchanged within run-to-run noise.

The 12 MP JPEG peaks 33 MB above where it settles. A decoded RGB8 copy (36 MB) held alongside its RGBA8
conversion (48 MB) would explain that — confirmed and fixed below (*JPEG decode peak*).

### `H` help overlay — 2026-11-01

Before (`d5b8fdd`) and after, same session, 15 runs per case.

| Case | Visible ms (median) before → after | Visible ms (min) before → after | Working set MB | Private MB |
|---|---:|---:|---:|---:|
| No image (empty window) | 51.1 → 50.6 | 45.9 → 44.7 | 18.3 → 18.3 | 5.5 → 5.5 |
| PNG 1920×1080 | 89.7 → 87.7 | 79.9 → 82.8 | 31.6 → 31.6 | 18.6 → 18.6 |
| JPEG 4000×3000 (12 MP) | 151.2 → 156.5 | 143.8 → 144.1 | 67.6 → 67.6 | 54.7 → 54.7 |

Binary size: 3,599,360 → 3,608,576 bytes (+9,216). The overlay adds no startup work (it runs only when shown);
the time differences are within run-to-run noise, and memory is identical.

That first version used an embedded 8×8 bitmap font; the maintainer found it too pixelated to read. With Segoe
UI rendered by GDI instead, same session:

| Case | Visible ms (median) | Visible ms (min) | Working set MB | Private MB |
|---|---:|---:|---:|---:|
| No image (empty window) | 49.9 | 40.9 | 18.3 | 5.5 |
| PNG 1920×1080 | 83.5 | 75.9 | 31.6 | 18.6 |
| JPEG 4000×3000 (12 MP) | 153.2 | 146.3 | 67.6 | 54.7 |

Binary size: 3,607,040 bytes, +7,680 over `d5b8fdd` and 1,536 smaller than the bitmap-font version. Linking
`gdi32.dll` costs no measurable startup: `user32.dll`, which the viewer already loads, loads it too.

Dimming the whole window instead of a panel (same day) leaves startup untouched, since nothing runs until H. A
redraw with the overlay shown costs 3.0 ms on a 3840×2160 buffer at 150 % DPI (release, mean of 20 frames,
masks already rendered). Binary: 3,606,528 bytes.

### JPEG decode peak — 2026-11-01

`decode_raster` used `image::open(..).into_rgba8()`. The JPEG decoder (zune-jpeg) writes straight into the
buffer it is given, so the only second buffer was the RGBA copy that `into_rgba8` allocates while the RGB8
image is still alive: 36,000,000 bytes (34.3 MiB) on top of 48 MB, against the 33 MB measured. The decoder now
writes 8-bit RGB, grey and grey-alpha into the front of a buffer already sized for RGBA, and the pixels are
widened in place, back to front. 16-bit and float images still go through `DynamicImage`.

Same session, interleaved new → old → new, 15 runs per case, machine otherwise idle (an earlier attempt during
another project's `cargo` build read 210 ms for the empty window and was discarded). "Old" is the `d5b8fdd` exe,
whose decode path is the one replaced.

| Case | Visible ms (median) old → new | Peak WS MB old → new | Working set MB | Private MB |
|---|---:|---:|---:|---:|
| No image (empty window) | 44.9 → 46.1 / 43.7 | 17.9 → 17.9 | 17.9 → 17.9 | 5.5 → 5.5 |
| PNG 1920×1080 | 79.7 → 78.3 / 78.6 | 31.3 → 31.3 | 31.3 → 31.3 | 18.5 → 18.6 |
| JPEG 4000×3000 (12 MP) | 137.7 → 127.4 / 128.8 | **100.2 → 67.3** | 67.3 → 67.3 | 54.7 → 54.7 |

The peak now equals the settled working set: −32.9 MB. The JPEG also opens 7% sooner, since one 48 MB
allocation and copy is gone; an in-process A/B of the decode alone, alternating old and new, agreed in
all three runs although they ran during that other build (medians of 7: 89.7 → 83.8, 262.7 → 244.4, 255.0 →
239.8 ms). The PNG fixture was already RGBA, so it is unchanged. Binary: 3,608,064 bytes,
+1,536 over the previous commit.
