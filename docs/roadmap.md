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
