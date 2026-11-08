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

1. **The two writes to the user's files have no test that reads the disk afterwards.**
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
