# Architecture

How the pieces fit and why. Per-function detail lives in the doc comments; decisions that must not drift live in
[settled-decisions.md](settled-decisions.md). Carried over from the retired `DevoidViewer_Codebase_Guide.pdf`
(2026-11-01), checked against the code at the time.

## Threads and the one channel

The UI thread owns all state (`App` in `main.rs`) and never blocks on I/O. Work happens on short-lived threads that
report back through **one bounded crossbeam channel of capacity 32**:

- **Decode** — `open_image` spawns one per image not in the preload cache.
- **Preload** — `preload_adjacent` decodes both neighbours speculatively. Only `LoadOutcome::Ok` is cached: a
  wrong-extension file must go through the rename prompt, which a cache hit would skip.
- **Dialogs** — every message box runs on its own thread (`App::spawn_dialog`, `show_error_dialog`), never inside
  a winit callback; see the settled decision. The Delete thread confirms, then recycles the file; the rename thread
  asks, then renames. Each posts `Deleted`, `DeleteCancelled` or `Renamed`.
- **Directory watcher** — `notify-debouncer-mini`'s own thread, coalescing bursts over `DIR_WATCH_DEBOUNCE`
  (50 ms), sends a payload-free `Msg::DirectoryChanged`.

Every sender posts its `Msg`, then `proxy.send_event(WakeUp)`. On each `WakeUp` the UI drains the channel with
`try_recv`. The bound throttles background threads if the UI falls behind. A `send` only fails when the receiver
is gone, i.e. the process is exiting, so the failures are ignored.

**Exit is `std::process::exit(0)`**: background and watcher threads hold senders, and a graceful shutdown would
wait for them. The viewer has no state to flush.

## Window reveal: two WakeUps

With a path argument, the window is created hidden, painted the background colour once (`paint_background`,
overwriting the white DWM fill), and shown only when the first image is ready:

hidden → decode thread → `WakeUp` → resize to the image → `pending_center = true` → self-`WakeUp` → next
`WakeUp`: centre → `set_visible` → `RedrawRequested` paints.

The self-`WakeUp` matters for a lone image in an archiver's temp folder, where no preload thread would send a
second one. Measured in [benchmarks.md](benchmarks.md): the window becomes visible after the decode, not before.

## Directory watching

`watched_dir` is tracked apart from the navigator's `current_dir` on purpose. `open_image` calls
`navigator.set_current()` before the decode finishes, so by the time `apply_image` runs the navigator already
reports the new directory; comparing against it would never install a watcher. `watched_dir` changes only inside
`watch_dir`, so `apply_image`'s comparison is reliable.

If watching fails (network shares, virtual file systems), both fields go to `None` and navigation falls back to a
static sibling list.

**External deletion has two layers, and neither is enough alone:**
1. The watcher triggers `on_directory_changed`: clear the preload cache, `rebuild_current(current_path)`, and move
   off the current file if it is gone.
2. `navigate` checks `path.exists()` on each candidate, at most `navigator.len()` times, removing dead entries. This
   covers the debounce window and file systems with no watcher.

`rebuild_current` takes `current_path` as a parameter because the navigator's own idea of "current" is already
stale by the time it is called.

## Decoding

`loader::load_image` decodes in two passes. The fast path trusts the extension but first checks the magic bytes
(`magic_to_format`, the single source of truth for format detection). If they disagree it fails at once; the
caller then sniffs the content and returns `WrongExtension`, and the image shows immediately with a rename offer
after the drain. The explicit check exists because `image::open` and `with_guessed_format` both sniff the header
and succeed silently on a misnamed file.

| Format | Magic bytes |
|---|---|
| JPEG | `FF D8` |
| PNG | `89 50 4E 47` |
| BMP | `42 4D` |
| GIF | `47 49 46 38` |
| TIFF | `49 49 2A 00` or `4D 4D 00 2A` |
| ICO | `00 00 01 00` |
| WebP | `52 49 46 46 ?? ?? ?? ?? 57 45 42 50` |

JPEG XL is sniffed separately. `decode_jxl` rejects dimensions over 32,768 px and guards the buffer size with
`checked_mul`.

## Rendering

`render` writes straight into the softbuffer surface. Rows wholly above or below the image, and the margins left
and right of it, are filled with `slice::fill`. `1 / scale` is computed once per frame, and `composite`
short-circuits fully opaque and fully transparent pixels. Sampling is nearest-neighbour, and small images are
enlarged to fill the window.

The `H` help overlay (`help.rs`) is drawn into the same buffer after the image: a panel at quarter brightness
with text in Segoe UI, which GDI renders greyscale-antialiased into a mask (`win32::render_text`) that is blended
in. The em is 15 px times the DPI scale, smaller if the window cannot fit the panel. It costs nothing until it
is shown, and the masks are kept until the size they need changes. Keys
are matched by physical position (`KeyCode`), so letter keys ignore layout and Caps Lock.

## Known limitations

- No status bar: the title shows the file name only, not index or dimensions.
- Animation is GIF only. Animated WebP and JXL render their first frame as a still image.
- JXL HDR is clamped to 1.0 when converted to 8-bit.
- The work area outside Windows is a 90 % × 85 % heuristic.
- Routing the title-bar double-click through `MouseInput` relies on observed winit behaviour, not a documented
  guarantee (see *Maximise has one code path* in the settled decisions).
