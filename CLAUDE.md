# CLAUDE.md — Devoid Viewer

An ultralight, Windows-native, view-only image viewer in Rust: winit + softbuffer (CPU blit, no GPU pipeline),
`image` + `jxl-oxide` for decoding. **The one goal is to open an image instantly with the smallest possible
footprint**, so a change that adds startup work, memory or binary size has to earn it.

## Layout
- `src/main.rs` — event loop, window state (maximise/fullscreen/centre), background decode and preload threads,
  directory watcher, rendering, the Win32 dialogs.
- `src/loader.rs` — decoding to straight RGBA8, magic-byte sniffing, wrong-extension recovery.
- `src/win32.rs` — every Win32 call, behind safe functions; the only module where `unsafe` compiles.
- `src/help.rs` — the `H` key-bindings overlay, text in Segoe UI via `win32::render_text`; its table must match the
  README's.
- `src/nav.rs` — the sibling-file list for one directory, in Explorer's order (`win32::compare_logical`),
  wraparound navigation.
- `scripts/bench.ps1` — startup time and memory of the release build; baseline and method in
  [docs/benchmarks.md](docs/benchmarks.md). Run it before and after anything that touches startup or decoding.
- `build.rs` — embeds `devoid.ico` and version resources. `devoid_viewer_setup.iss` — Inno Setup installer.

## Critical rules
- **Read [docs/settled-decisions.md](docs/settled-decisions.md) before proposing a change to paths, formats,
  directory listing or window behaviour.** A decision there is re-opened by a new fact, never by an argument.
  Where a change genuinely moves one, revise the entry in the same commit.
- **[docs/roadmap.md](docs/roadmap.md) is the work queue, in order.** Update it in the commit that lands an item.
  A check only the maintainer can make is a row there, not a promise in a conversation.
- **Never work on `main`.** Branch off its tip (`feature/<slug>`, `fix/<slug>`, `chore/<slug>`); merging and
  pushing are the maintainer's.
- **A package runs through three skills**: `/wp-start` before the first edit, `/wp-verify` before calling anything
  green (report its numbers, not an impression), `/wp-finish` to commit, rebuild and merge. `/sweep` is the
  unattended audit, started by the maintainer only. Their repository facts are keys in
  [.claude/project-profile.md](.claude/project-profile.md), ported from Patina (2026-11-08).
- **CI (`.github/workflows/ci.yml`) runs the `/wp-verify` recipe** on every push to `main` and every pull request,
  with clippy at `-D warnings`. **Change both together**: a step added to one and not the other means a local green
  that CI reds, or the reverse. The one local-only step is compiling the installer (`/wp-verify` step 6): CI has no
  Inno Setup 7, and the tests that tie the script to the code run in both.
- **The installer moves with the code**: a change to what `devoid_viewer_setup.iss` encodes (extensions, exe name,
  ProgID, version source, its wording) updates the script in the same commit. Its version is never typed: it is read
  from the exe, which winres stamps from `Cargo.toml`.
- **After every commit, rebuild from clean**: `cargo clean`, then `cargo build --release --locked`, and report the
  exit code and `target\release\DevoidViewer.exe` size. The exe on disk is then always a fresh build of the latest
  commit, never a leftover. (`Cargo.toml` and `Cargo.lock` are source, not build output; they are never deleted.)
  A `/sweep` rebuilds once, after its last merge, rather than per step (its *Finishing* says why).
- **Devoid writes to the user's files** — rename, and delete to the Recycle Bin. A change on either path needs a
  test that shows what is on disk afterwards, not just that the call returned `Ok`.
- **Nothing is installed or uninstalled on the maintainer's machine unannounced** — the app, a cargo tool, a
  runtime. Say what and why first.
- **Documentation lives in this repository as Markdown**, where it can be grepped, diffed and changed in the same
  commit as the code it describes: [docs/architecture.md](docs/architecture.md) for how the pieces fit,
  [docs/installer.md](docs/installer.md) for the Inno Setup script.

## Building
- **Needs the MSVC linker**: Visual Studio's *Desktop development with C++* workload (or the standalone Build
  Tools). Without it every build, `clippy` and `test` fails with `link.exe not found`.
- **Run cargo from PowerShell, not Git Bash.** Git Bash puts coreutils' `/usr/bin/link` on `PATH`, rustc picks it
  up as the linker, and the build fails with `link: extra operand`.
- `cargo` lives in `~/.cargo/bin`, which may not be on the tool shell's `PATH` — prepend it.
- The toolchain is pinned in `rust-toolchain.toml`; architecture bans live in `clippy.toml` at `deny`.
- Dependency policy (advisories, licences, duplicates, sources) lives in `deny.toml`, checked by `cargo deny` in
  CI on every push, every pull request and weekly.

## Working style
- **Safe Rust only, `unsafe` only where Windows forces it.** Maintainer: *"If we're writing unmanaged code why
  are we writing code in Rust in the first place."* Before writing FFI, look for a maintained crate with a safe
  API. When FFI is genuinely unavoidable (a Win32 call no crate wraps), it goes in `src/win32.rs`: the `unsafe`
  block holds the one call, inside a safe function that takes and returns no raw pointers, with a `// SAFETY:`
  comment saying why it holds. `unsafe_code = "deny"` makes `unsafe` anywhere else a build error.

Carried over from the maintainer's other projects:
- **Evidence, not reasoning.** A fix is finished when it is measured, not when the code looks right. A hypothesis
  built from reading code is a candidate, never a conclusion. A measured "nothing to do" is a deliverable.
- **Take a position and argue it.** An open question wants a recommendation and what would change it, not a menu.
- **Quality outranks compatibility.** No shims or legacy fallbacks; fix the shape, not just the symptom.
- **Justify every new dependency unprompted** — its maintenance status and the first-party alternative. Here that
  also means its cost in binary size and startup time.
- **Correct a misused technical term** in one clause, then answer the actual question.
- **Hand over decisions, not inventories.** Triage a lint loop before reporting it; lead with what survives.
- **Commit bodies are the archive**: what changed, why it was not the obvious thing, and the evidence, in about
  1,500 bytes measured before committing (`/wp-finish`). Search them with `git log --grep=<term>`.
- **Check the instrument before believing it.** A confident negative is the result to distrust first: read what a
  search or probe matched, and whether its shape is the case in question, before concluding from how many.
- **A refusal is not a failure.** When a permission check refuses an action, never re-issue it in another form —
  narrower, reworded, split, through another tool; that takes the decision the refusal made. Say what was refused
  and leave it. Retrying an error nobody chose (a locked file, a timeout) is not that.
