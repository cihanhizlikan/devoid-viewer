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

## Delete moves the file to the Recycle Bin (2026-10-31)
It was `std::fs::remove_file` — permanent, with no undo. It now goes through the `trash` crate, a safe wrapper over
the shell's `IFileOperation` with `FOF_ALLOWUNDO`; where a volume has no Recycle Bin, the shell warns before
destroying the file. **A crate, not our own COM calls**, because the maintainer wants safe Rust wherever a
maintained crate offers it. `trash` canonicalizes the parent directory internally and converts the verbatim path
back before handing it to the shell, so it does not reintroduce the `\\?\` problem above. Its one internal
`unwrap` (COM instantiation) is caught with `catch_unwind` so `deleting` cannot stick.

## Files are sorted the way Explorer sorts them (2026-10-31)
`StrCmpLogicalW` — the comparison behind Explorer's default name order: numeric runs by value, case-insensitive,
locale-aware. The previous hand-written natural sort compared by code point, so `élan` sorted after `zebra`.
**This is the one place `unsafe` was added**, and it is the case the maintainer allows: a Win32 call no
reimplementation can match, since the ordering depends on the user's locale. One call, inside a safe function,
with a `SAFETY` comment. The old natural sort stays as the non-Windows fallback.

## Logging: none
The minimalist goal rules out a logging framework, and the user-facing errors already reach a dialog. Unusual
failures should reach the Event Log through Windows itself rather than code of ours — see the `panic` open
decision below.

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
- **`panic = "abort"` in the release profile.** Windows Error Reporting then records any crash as Application
  Error event 1000 in the Event Log, with the faulting module and offset — Event Log coverage of the unusual cases
  for zero code — and the binary drops its unwind tables. The cost: a decoder panic today kills only its thread
  (and leaves the window waiting); under abort it closes the app, recorded. It also turns the `catch_unwind` in
  `request_delete` into a no-op. *Recommendation: adopt it.*
- **`fit_scale` upscales.** Its doc says *"without upscaling"*, but the formula has no `.min(1.0)`, so a small image
  in a maximised window is enlarged (nearest-neighbour). Decide which is intended; the tests deliberately pin
  neither.
- **Quarantine `unsafe` in one module.** Six `unsafe` sites, all Win32: three `MessageBoxW`, `SystemParametersInfoW`,
  the message hook and `StrCmpLogicalW`. Moving them into `src/win32.rs` would let `Cargo.toml` set
  `unsafe_code = "deny"` crate-wide with a single `#![allow]` there, making the safe-Rust rule build-enforced.
