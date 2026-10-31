# Devoid Viewer — Settled Decisions

What has already been decided and must not be re-argued. **A decision is re-opened by a new fact, never by an
argument.** Each entry says what was decided, why, and where the evidence is. Where the original reason was never
recorded, the entry says so instead of inventing one — the maintainer fills it in.

## Paths are used as given — no `canonicalize`, no `\\?\` prefix
`std::fs::canonicalize` on Windows returns a verbatim `\\?\C:\…` path. The viewer keeps paths exactly as they
arrive (command line, drag-and-drop, directory listing) so that files opened from archivers keep working
(`src/main.rs` module doc: *"Archiver-safe path handling"*). **Build-enforced**: both `canonicalize` functions are
banned in `clippy.toml`, and `disallowed_methods` is `deny`.

## SVG is not supported (2026-04-07, `8282b3a`)
Support was built and then removed, dropping `resvg`, `usvg` and `tiny-skia` (~290 lines of `Cargo.lock`). **The
reason was not recorded in the commit** — maintainer to fill in. Do not re-add SVG without that reason in hand.

## The directory listing never stats a file (2026-04-05, `8713c6b`)
`DirNavigator::rebuild` filters with `DirEntry::file_type()`, which `read_dir` already returned, instead of
`Path::is_file()`, which costs one syscall per entry — thousands of disk hits on a cold cache for a large folder.
This was the cold-start fix. Keep any new filter on data the directory entry already carries.

## Rendering is a CPU blit through softbuffer
No GPU pipeline: it follows from the instant-startup and tiny-footprint goals in the README. A GPU path would be a
change to the product's premise, not an optimisation.

## Maximise has one code path
The Win32 message hook suppresses `SC_MAXIMIZE`, so the title-bar button, the title-bar double-click and the
client-area double-click all go through `App::toggle_maximize`. The same hook re-posts `SC_CLOSE` as `WM_CLOSE`,
because winit 0.29 does not turn the taskbar thumbnail's close button into `CloseRequested`. Restoring from
maximise always re-centres rather than restoring the old position, which avoids off-screen placement after a
monitor change.

## The extension is checked against the magic bytes before decoding
`decode_raster` reads the header and fails fast when it disagrees with the extension, because every `image` API
that takes a path sniffs the header itself and would decode a misnamed file silently — and the rename offer would
never fire. Only `Ok` outcomes are preloaded; a wrong-extension file must reach the UI so it can offer the rename.

## GIF frame delays are clamped
A zero-denominator delay becomes 100 ms, and every delay is at least 20 ms: some encoders emit 0-delay frames,
which would otherwise busy-loop the event loop.

---

# Open decisions

Raised by the 2026-10-31 baseline setup. Each needs the maintainer's call; none is settled.

- **rustfmt.** `cargo fmt --check` reports 104 diffs on the current tree, mostly because the code aligns `=`,
  struct fields and `match` arms by column, and stable rustfmt cannot keep that alignment. Either adopt rustfmt
  and lose the alignment, or keep the hand style and leave formatting unenforced. Until decided, `/verify` reports
  the count and does not gate on it. *Recommendation: adopt rustfmt* — the alignment is real readability, but an
  unenforced style drifts with every edit, and a formatter removes a whole class of review comment.
- **The codebase guide PDF.** `DevoidViewer_Codebase_Guide.pdf` cannot be grepped or diffed and has needed
  repeated "updated the pdf" commits. *Recommendation:* move anything it says that the code and these docs do not
  into Markdown here, then delete it.
- **Delete is permanent.** `request_delete` calls `std::fs::remove_file`. Most viewers send the file to the Recycle
  Bin. Decide which one Devoid does, and record why.
- **Sort order vs Explorer.** `human_sort` lowercases and compares by `char`; Explorer uses `StrCmpLogicalW`.
  Unmeasured whether they disagree on real folders; worth a test before deciding.
