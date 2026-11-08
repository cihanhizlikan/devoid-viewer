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
Support was built and then removed, dropping `resvg`, `usvg` and `tiny-skia` (~290 lines of `Cargo.lock`).
**The viewer supports raster formats only** (maintainer, 2026-11-01): SVG is a vector format, and rendering it took
a stack of crates that bloated the executable, against the footprint goal. The same rule covers any other vector
format (PDF, EPS, AI). **Re-opened by:** the maintainer changing the raster-only scope, not by a lighter renderer.

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
`unwrap` (COM instantiation) used to be caught with `catch_unwind` so `deleting` could not stick; since release
builds abort on panic (below), a panic there ends the process instead and is logged by Windows.

## Files are sorted the way Explorer sorts them (2026-10-31)
`StrCmpLogicalW` — the comparison behind Explorer's default name order: numeric runs by value, case-insensitive,
locale-aware. The previous hand-written natural sort compared by code point, so `élan` sorted after `zebra`.
It is the case the maintainer allows `unsafe` for: a Win32 call no reimplementation can match, since the
ordering depends on the user's locale. One call, inside a safe function in `src/win32.rs` (see *`unsafe` lives
only in `src/win32.rs`*), with a `SAFETY` comment. The old natural sort stays as the non-Windows fallback.

## Logging: none
The minimalist goal rules out a logging framework, and the user-facing errors already reach a dialog. Unusual
failures reach the Event Log through Windows itself rather than code of ours — see *Release builds abort on
panic* below.

## Message boxes never run inside a winit callback (2026-11-01)
A message box runs a modal message loop. Inside a winit 0.29 callback, that loop receives the viewer's `WM_PAINT`,
which winit cannot deliver mid-callback; it validates and immediately re-posts it with `RedrawWindow`
(`winit-0.29.15/src/platform_impl/windows/event_loop.rs`, `WM_PAINT` arm). The queue is never idle, so the dialog,
which only shows once its loop goes idle, never appears, and the process spins. Measured before the fix: the
"Cannot open image" box was created but never visible, at 97 % of a core; resizing the viewer under the Delete
confirmation took it to 98 %. Every dialog flow now runs on its own thread via `App::spawn_dialog` (or
`show_error_dialog`) and reports back through the message channel; afterwards all four cases measured 0–7 %, every
dialog visible. **Consequence, accepted:** the viewer stays usable while a dialog is open, so a delete may complete
after the user has moved on, which `on_deleted` and `DirNavigator::remove` handle.

## Documentation is Markdown in `docs/` (2026-11-01)
`DevoidViewer_Codebase_Guide.pdf` could not be grepped or diffed, needed repeated "updated the pdf" commits, and
had drifted (it still described the pre-Explorer sort and a 2–3 MB binary). What it said that the code did not —
the threading and window-reveal design, directory-watching rationale, known limitations, and the installer's
reasoning — moved to `docs/architecture.md` and `docs/installer.md`, checked against the code, and the PDF was
deleted. Its Rust glossary was tutorial material and was dropped.

## Small images are enlarged to fill the window (2026-11-01)
`fit_scale` has no `.min(1.0)`: an image smaller than the window is scaled up (nearest-neighbour) like a larger
one is scaled down. Its doc comment used to say *"without upscaling"*; the maintainer chose the behaviour, so the
comment was corrected and `fit_scale_enlarges_a_small_image_to_fill_the_window` pins it.

## Release builds abort on panic (2026-11-01)
`panic = "abort"` in `[profile.release]`. This is the Event Log coverage the logging decision asks for, with no
code: a probe panic in a background thread ended the process with `0xC0000409`, and Windows logged Application
Error event 1000 (faulting module `DevoidViewer.exe`, fault offset) and Windows Error Reporting event 1001. It also
drops the unwind tables: release binary 4,609,024 → 3,595,776 bytes (−22%), with startup and memory unchanged
within noise (`docs/benchmarks.md`). The accepted cost: a panic in a decoder or in `trash` now closes the app
instead of killing one thread, so `request_delete` no longer wraps `trash::delete` in `catch_unwind`. Tests still
unwind; the setting applies to the release profile only.

## Formatting is default rustfmt, gated in CI (2026-11-01)
The code used to align `=`, struct fields and `match` arms by column. Stable rustfmt cannot keep that, and an
unenforced hand style drifts with every edit, so the maintainer chose the formatter: the whole tree was
reformatted once (135 diffs) and `cargo fmt --check` now fails CI and `/wp-verify`. No `rustfmt.toml`; defaults only. The reformat
commit is listed in `.git-blame-ignore-revs`, so `git blame` skips it.
**Re-opened by:** stable rustfmt gaining column alignment.

## `unsafe` lives only in `src/win32.rs` (2026-11-01)
`[lints.rust] unsafe_code = "deny"` in `Cargo.toml`, with a single `#![allow(unsafe_code)]` in `src/win32.rs`, so
the safe-Rust rule is enforced by the build. The module's functions take and return no raw pointers; the winit
message hook is installed from inside it, so the `MSG` pointer never leaves it. Verified by adding an `unsafe {}`
to `nav.rs`, which failed the build.

## jxl-oxide stays on 0.10 (2026-11-01)
RUSTSEC-2026-0151 (jxl-grid 0.5: out-of-bounds writes from a crafted JPEG XL) affects 32-bit targets only, and the
shipped binary is x86_64. The fix needs jxl-oxide 0.12, which compiled with no source changes and passed every
test, but grew the release binary by 323,584 bytes (+7%, 4,607,488 → 4,931,072). 0.12 always links JPEG bitstream
reconstruction (`jxl-jbr`, `brotli-decompressor`), and no feature turns it off. The advisory is ignored in
`deny.toml` with this reason. **Re-opened by:** a 32-bit build shipping, a jxl advisory that reaches 64-bit, or a
jxl-oxide release that makes reconstruction optional.

## Help is an overlay, not a pop-up window (2026-11-01)
`H` draws the key table into the viewer's own frame: the whole window dimmed to quarter brightness, the table
centred on it in Segoe UI, no panel. A pop-up was weighed and rejected: it takes focus, so the arrows, F and Q
stop working until it is dismissed; a message box cannot align the two columns; and a custom window needs far more
Win32 code. Its one advantage, that Narrator can read it, was set aside. The text is Segoe UI rendered by GDI
(`win32::render_text`), not an embedded font: the first version's 8×8 bitmap font was too pixelated to read, and
an embedded TrueType font with a rasteriser crate would add an estimated 100–300 KB. Segoe ships with Windows.
Evidence and costs are in `docs/benchmarks.md` and the `feat(help)` commit bodies. **Re-opened by:** a
screen-reader requirement, which a message box shown alongside the overlay would meet more cheaply than replacing
it.

## The installer stays Inno Setup, moving to version 7 (reviewed 2026-11-01)
Inno Setup was chosen over WiX 3 because WiX 3 cannot let the user choose a per-user or per-machine install
(`docs/installer.md`). Re-checked against what the Rust ecosystem offers now; that reason still decides it.

| Option | Why not |
|---|---|
| `dist` (cargo-dist) | Its MSI comes from bundled `cargo-wix`, built on WiX 3: the same scope limitation. Aimed at command-line tools (PATH, shell installers), not file associations. Its strength is the release pipeline. |
| `cargo-packager` | MSI through WiX 3.11.2, pinned in its source; its NSIS output is a sideways move from Inno with less control over the registry details below. |
| Velopack | Per-user only, and `VelopackApp::build().run()` must run at the top of `main`: a dependency in the exe and startup work, for auto-update a viewer does not need. |
| WiX v6 / v7 | v6 added dual-purpose MSIs, but only inside a Burn bundle (bootstrapper exe + MSI) — the one option that answers the original objection, at the cost of a .NET toolchain and the Open Source Maintenance Fee EULA, which v7 requires accepting (no fee below US$10,000/year of revenue from projects using it). |
| MSIX | Needs a code-signing certificate, virtualises the app's registry writes, and cannot register the `App Paths` entry archivers launch the viewer through. |

**Inno Setup 7** keeps the `.iss` as is (it compiled unchanged, without warnings) and is where support continues;
the installer now builds with 7.1.1. Its 64-bit installer was expected to be the gain, but measured 745,890
bytes larger (+23%) for the same files and registry entries, so **the installer stays 32-bit** and only the
install mode is x64 (`docs/installer.md`).
**Re-opened by:** a need for MSI deployment through Intune or Group Policy (then a WiX v7 bundle), or for
auto-update (then Velopack).

## The installer registers for Default apps; it never sets the default (2026-11-01)
Windows takes an extension's handler from the user's `UserChoice` (hashed, written only by Windows), and only
without one from `HKLM\Software\Classes\.ext`, which Windows owns. The old "Set as default" task overwrote the
latter, changed nothing for any user with a `UserChoice` (every Windows 11 account has one for Photos), and
deleted Windows' value on uninstall: on the maintainer's machine eight of ten such values were Windows' own
(`.jpg` → `jpegfile`). The installer now registers `Capabilities` and `RegisteredApplications`, so the app gets
its own page in Settings → Default apps, and offers to open that page; the user makes the choice. **Re-opened
by:** a Windows API that lets an installer set a default with the user's consent, which none does today.

---

# Open decisions

None. The five raised by the 2026-10-31 baseline setup were all decided on 2026-11-01 and are recorded above.
