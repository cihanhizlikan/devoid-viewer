# Devoid Viewer — Roadmap

The outstanding work, in the order it is to be done (started 2026-11-08). Each item is its own branch and commit;
when an item lands it is struck through here in that same commit, and what it decided goes into
[settled-decisions.md](settled-decisions.md). Work is issued by the maintainer, never taken: the next item is the
one at the top, or the one the maintainer names.

A check only the maintainer can make — a UAC prompt, a Settings page, a machine this repository does not have —
is a row here saying so, never a promise in a conversation.

## Checks only the maintainer can make

1. **Per-machine install.** Run the installer elevated ("Install for all users"), then confirm Devoid Viewer is
   listed in Settings → Default apps and that the finish-page link opens its page there
   (`registeredAppMachine`). Only the per-user install has been tried (2026-11-07). *Where*:
   `devoid_viewer_setup.iss`, `[Run]`; `docs/installer.md`.
2. **The finish-page link on Windows 10.** `ms-settings:defaultapps?registeredApp…` is a Windows 11 deep link; on
   Windows 10 it is expected to open the general Default apps page, with the app under *Set defaults by app*
   (`docs/installer.md`, *Windows 10*). Unverified: no Windows 10 machine was available. Consumer Windows 10 is
   out of support, so this is checked only if a user reports it.

## From sweeps

### Sweep 2026-11-08

1. ~~**The two writes to the user's files have no test that reads the disk afterwards.**~~ Done 2026-11-08 (`fix/user-file-writes`): `rename_to_free_name` and `recycle` split from their dialogs; the rename test reads the folder back, the Recycle Bin test is `#[ignore]`d (maintainer's choice).
   - *What*: the wrong-extension rename (`offer_rename`, `src/main.rs:822`, `std::fs::rename` at :843) and the
     delete (`request_delete`, `src/main.rs:535`, `trash::delete` at :548) are tested only around the edges:
     `unique_rename_target` picks the name, nothing checks that the file then has it, and nothing tests the delete.
     `CLAUDE.md` asks for a test that shows what is on disk.
   - *Why it is not the obvious thing*: both calls sit behind a blocking message box. A test needs the dialog split
     from the file operation, which moves code in `main.rs`. And a Recycle Bin test writes to the maintainer's real
     Recycle Bin, so Patina keeps its tests of that shape `#[ignore]`d, which here would raise
     `profile:baseline_ignored` from 0.
   - *Recommendation*: split each into a dialog-free `fn` returning `io::Result`, and test the rename in a tempdir
     (old name gone, new name present, same bytes). For the delete, test only that the file has left the directory,
     ignored and named in the profile, or take the cost of leaving it untested and say so in
     `docs/settled-decisions.md`.
   - *What would falsify it*: a test already exercising either path on disk.
   - *Origin*: sweep 2026-11-08 step 2, not yet committed to build.
2. **`trash` 5.3.0 is a compatible update that grows the exe by 11,264 bytes.**
   - *What*: `cargo update -p trash` (5.2.9 → 5.3.0, drops `urlencoding`) took `DevoidViewer.exe` from 3,608,064
     to 3,619,328 bytes (release, `--locked`, same machine); the suite was not run on it. The lockfile stays on
     5.2.9.
   - *Why it is not the obvious thing*: a minor update is normally taken, but the sweep takes a compatible update
     only when the exe does not grow. Nothing pins 5.2.9 either, so the next bulk `cargo update` takes 5.3.0
     silently. That same bulk update (`profile:update_command`, 80 packages) also adds `tokio`, `syn` 3 and
     `getrandom` 0.4 to the lockfile, through crates not identified here.
   - *Recommendation*: stay on 5.2.9 until 5.3's changelog shows a fix Devoid needs. Update one package at a time
     (`cargo update -p <crate>`), never in bulk, and check the exe size each time.
   - *What would falsify it*: the size difference vanishing after a clean build of each lockfile.
   - *Origin*: sweep 2026-11-08 step 3, not yet committed to build.
3. **`notify-debouncer-mini` is three majors behind (0.4.1; 0.7.0 is out).**
   - *What*: it pulls `notify` 6, which brings one of the two `windows-sys` 0.48 paths
     (`profile:duplicates_command`); winit 0.29 is the other.
   - *Why it is not the obvious thing*: a major update with an API change in the watcher setup (`src/main.rs`,
     `watch_dir`), and it removes no duplicate on its own while winit 0.29 still needs 0.48.
   - *Recommendation*: take it together with winit 0.30 (row 4), when both 0.48 paths go at once and the size win
     can be measured.
   - *What would falsify it*: notify 8 on a `windows-sys` other than 0.61 (a new duplicate rather than one fewer).
   - *Origin*: sweep 2026-11-08 step 3, not yet committed to build.
4. **`winit` is a major behind (0.29.15; 0.30.13 is out).**
   - *What*: 0.30 replaces the closure event loop with the `ApplicationHandler` trait. Every window, input and
     wake-up path in `src/main.rs` moves, along with `win32::install_system_command_hook`, which takes winit's
     `EventLoopBuilder`.
   - *Why it is not the obvious thing*: 0.29 works, and the move is a rewrite of the event loop. Its gain is the
     `windows-sys` 0.48 and `raw-window-handle` 0.5 duplicates (binary size) and staying on a maintained line.
   - *Recommendation*: a feature package of its own, benchmarked before and after (`profile:benchmark`), with
     row 3 in the same package.
   - *What would falsify it*: 0.30 measuring larger or slower to first window than 0.29.
   - *Origin*: sweep 2026-11-08 step 3, not yet committed to build.
5. **The non-Windows half is code that nothing compiles.**
   - *What*: 10 `#[cfg(not(windows))]` sites: the dialog stubs and the work-area fallback in `src/main.rs` (:791,
     :803, :849, :963), and in `src/nav.rs` the `String` sort key, `human_sort`, `consume_number` (:213–:279) and
     their test (:366).
   - *Why it is not the obvious thing*: CI builds only on `windows-latest`, and this machine has only the MSVC
     target. Nothing has compiled this half since the Windows-only work (`src/win32.rs`, `src/help.rs`, the `trash`
     COM feature), so it may already fail to build, and a deletion is a decision about the product's platforms.
   - *Recommendation*: delete it. The product is Windows-native (`CLAUDE.md`), `help` and `win32` are already
     `cfg(windows)`-only, and a fallback that is never built is not a fallback. `nav.rs` then loses about 60 lines
     and an untested sort. The other option is a `cargo check --target x86_64-unknown-linux-gnu` job in CI.
   - *What would falsify it*: a plan to ship on another platform.
   - *Origin*: sweep 2026-11-08 step 4, not yet committed to build.
6. ~~**Merge `sweep-6-rename-stem`?**~~ Done 2026-11-08: merged at the maintainer's word (`833ff7d`).
   - *What*: `unique_rename_target` (`src/main.rs:867`) builds the new name with `to_string_lossy`, so a stem that
     is not valid Unicode (a lone surrogate, legal on NTFS) is renamed to `U+FFFD…` along with the extension fix.
     The branch builds it as an `OsString`, with a test that fails without the fix.
   - *Why it is not the obvious thing*: it adds 512 bytes to the exe (3,604,992 → 3,605,504), and a sweep merges
     only corrections that do not grow it.
   - *Recommendation*: merge it. 512 bytes is one unit of PE file alignment, and the defect renames a user's file
     to a name they did not choose.
   - *What would falsify it*: a rule that the exe never grows, whatever the fix.
   - *Origin*: sweep 2026-11-08 step 6, committed on `refs/heads/sweep-6-rename-stem`.
7. ~~**Merge `sweep-6-size-cap`?**~~ Done 2026-11-08: merged at the maintainer's word (`81cc6c3`). Before it, a crafted file could abort the viewer. The fix was built, verified and committed
   (`81cc6c3`), but not merged.
   - *What*: only `decode_jxl` had a size cap, and it ran after `render_frame` had allocated the frame. A
     118-byte BMP claiming 60,000 × 60,000 px made `decode_to_rgba8` allocate 14 GB, and a GIF claiming a
     40,000 px screen decoded onto a full-size canvas. A failed allocation aborts the process; it never becomes the
     "Failed to decode" message. The branch applies the existing 32,768 px cap (`MAX_DIM`) in all three paths before
     allocation, with a test per format that fails without its check.
   - *Why it is not the obvious thing*: +512 bytes (3,604,992 → 3,605,504). The cap value is also a product rule:
     a real image over 32,768 px per side (a stitched panorama) will now be refused instead of opened.
   - *Recommendation*: merge it. The 32,768 px value is the one `decode_jxl` already used, and an abort on a
     crafted file is worse than refusing an image of over a gigapixel.
   - *What would falsify it*: a real user image over 32,768 px per side that opened before.
   - *Origin*: sweep 2026-11-08 step 6, committed on `refs/heads/sweep-6-size-cap`.
8. ~~**The wrong-extension rename can overwrite a file created while its dialog is open.**~~ Done 2026-11-08 (`fix/user-file-writes`): the free name is picked again after the dialog; the race is narrowed, not closed.
   - *What*: `offer_rename` (`src/main.rs:822`) picks a free name with `unique_rename_target`, shows a blocking
     *Rename?* box, then calls `std::fs::rename`, which on Windows replaces an existing target. A file given that
     name while the box is open, by a sync client or a download, is overwritten without a word.
   - *Why it is not the obvious thing*: the window is the dialog's lifetime, so it is rare, and a no-clobber rename
     needs either `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING` (new FFI in `src/win32.rs`) or
     `std::fs::hard_link` followed by `remove_file` (safe, same volume only, two steps).
   - *Recommendation*: re-pick the name after the dialog closes (no new code paths, narrows the window to
     microseconds), and say in the commit that the race is narrowed, not closed. Take the FFI only if a report
     shows it happening.
   - *What would falsify it*: `std::fs::rename` refusing to replace on this Windows build (a test in a tempdir
     settles it).
   - *Origin*: sweep 2026-11-08 step 6, not yet committed to build.
9. **The README shows a missing image, says nothing on formats or installing, and has an empty License section.**
   - *What*: `README.md:6` embeds `docs/screenshot.png`, which does not exist, so GitHub shows the alt text. The
     README never lists the formats it opens (BMP, PNG, JPEG, GIF including animation, WebP, TIFF, ICO, JPEG XL;
     `SUPPORTED_EXTENSIONS`), never mentions the installer or the wrong-extension rename, and *License* is a
     placeholder comment. There is no `LICENSE` file and no `license` in `Cargo.toml`.
   - *Why it is not the obvious thing*: the screenshot and the licence are the maintainer's to supply, and the
     section order (what it is → install → formats → keys → build) is an editorial choice. The sweep corrected
     none of it.
   - *Recommendation*: one `chore/readme` package. Capture the screenshot from the release exe on a bench fixture.
     Add *Install* (the setup exe, then Settings → Default apps) and *Formats* sections, the second tied to
     `SUPPORTED_EXTENSIONS` by a test the way the key table is tied to `BINDINGS`. Choose a licence, and add the
     MSVC Build Tools prerequisite to *Building from source*.
   - *What would falsify it*: a screenshot or licence kept elsewhere on purpose.
   - *Origin*: sweep 2026-11-08 step 9, not yet committed to build.

### Sweep 2026-11-08, second run

10. ~~**An image over the size cap is reported as corrupt.**~~ Done 2026-11-08 (`fix/load-error-message`): the message now ends with the decoder's reason.
    - *What*: opening a BMP that claims 60,000 × 60,000 px shows *"Cannot open image: Failed to decode .bmp
      file"* (measured with a temporary test calling `load_image`). `check_dimensions` returns *"image too large
      (W×H); maximum dimension is 32768"*, but `load_image` (`src/loader.rs:96`) drops the decode error when the
      sniffed format matches the extension and substitutes the generic message at :117. A real stitched panorama
      over 32,768 px would read as a broken file.
    - *Why it is not the obvious thing*: the fix needs `load_image` to tell "too large" apart from "corrupt",
      either by a typed error from the decoders (a new error enum through every path) or by keeping the first
      error's text. That is a choice about the error shape, and either version adds some bytes.
    - *Recommendation*: keep the first decode error and show it when the sniffed format matches the extension,
      since the decoder's own message ("too large", "unexpected end of file") is more useful than the generic
      line. Re-word the `a_truncated_file_is_unsupported_not_misnamed` assertion to match.
    - *What would falsify it*: the error dialog not showing `Unsupported`'s text (it does: `src/main.rs`,
      `drain_messages`, *"Cannot open image:\n{reason}"*).
    - *Origin*: sweep 2026-11-08 (second run) step 9, not yet committed to build.
