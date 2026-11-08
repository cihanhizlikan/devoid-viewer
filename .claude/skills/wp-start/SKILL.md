---
name: wp-start
description: Open a work package — confirm a clean main, branch from its tip, and name the branch for the task. Invoke this whenever the maintainer issues a package to build, before touching any file.
---

# Opening a work package

**Run this before the first edit of any package.** `CLAUDE.md` forbids working on `main`; this is how a branch is
opened correctly. Values marked `profile:<key>` are in `.claude/project-profile.md`.

## The steps

Devoid has one session at a time, so a package is a branch in the primary checkout — no worktree.

```powershell
git status --porcelain          # must be empty; if not, stop and report what is there
git branch --show-current       # must be main
git switch -c fix/<slug> main   # or feature/<slug>, chore/<slug>
git log --oneline -1            # must equal main's tip
```

Name the branch for the task: `feature/<slug>`, `fix/<slug>` or `chore/<slug>` — never `docs/`; a documentation
package is a `chore/`. A sweep step is `sweep-<n>-<subject>` (`/sweep`).

**Never `EnterWorktree`**: it branches from `origin/main`, which is behind local `main` whenever a merge has not
been pushed yet.

## What bites once you are in there

- **Cargo runs from PowerShell, never Git Bash** (`profile:shell`) — the linker trap `/wp-verify` step 0 names.
- **A build from clean takes minutes**; run long cargo commands with `run_in_background` and wait for the
  notification rather than a foreground timeout.
- **The app is checked with the RELEASE exe** (`profile:run_command`), and its real window is not
  `MainWindowHandle` — winit's hidden helper window is.
- **Startup or decoding touched?** Run `profile:benchmark` before the first edit, so there is a baseline from the
  same machine state to compare against.
- **Installing or uninstalling anything on the maintainer's machine** (the app, a cargo tool) is said to them
  before it is done, never after.

## What this skill does NOT do

It does not choose the package. **Work is ISSUED, never taken** — the next item is the one `docs/roadmap.md`
puts next, or the one the maintainer named.
