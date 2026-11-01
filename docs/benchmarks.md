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
conversion (48 MB) would explain that, but this is a candidate, not a measurement; confirm it before changing
the decode path.

### `H` help overlay — 2026-11-01

Before (`d5b8fdd`) and after, same session, 15 runs per case.

| Case | Visible ms (median) before → after | Visible ms (min) before → after | Working set MB | Private MB |
|---|---:|---:|---:|---:|
| No image (empty window) | 51.1 → 50.6 | 45.9 → 44.7 | 18.3 → 18.3 | 5.5 → 5.5 |
| PNG 1920×1080 | 89.7 → 87.7 | 79.9 → 82.8 | 31.6 → 31.6 | 18.6 → 18.6 |
| JPEG 4000×3000 (12 MP) | 151.2 → 156.5 | 143.8 → 144.1 | 67.6 → 67.6 | 54.7 → 54.7 |

Binary size: 3,599,360 → 3,608,576 bytes (+9,216). The overlay adds no startup work (it runs only when shown);
the time differences are within run-to-run noise, and memory is identical.
