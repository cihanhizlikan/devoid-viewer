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

None yet: no `/sweep` has run.
