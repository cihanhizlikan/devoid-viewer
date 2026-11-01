# CLAUDE.md — Devoid Viewer

An ultralight, Windows-native, view-only image viewer in Rust: winit + softbuffer (CPU blit, no GPU pipeline),
`image` + `jxl-oxide` for decoding. **The one goal is to open an image instantly with the smallest possible
footprint**, so a change that adds startup work, memory or binary size has to earn it.

## Layout
- `src/main.rs` — event loop, window state (maximise/fullscreen/centre), background decode and preload threads,
  directory watcher, rendering, the Win32 dialogs.
- `src/loader.rs` — decoding to straight RGBA8, magic-byte sniffing, wrong-extension recovery.
- `src/win32.rs` — every Win32 call, behind safe functions; the only module where `unsafe` compiles.
- `src/nav.rs` — the sibling-file list for one directory, natural sort, wraparound navigation.
- `scripts/bench.ps1` — startup time and memory of the release build; baseline and method in
  [docs/benchmarks.md](docs/benchmarks.md). Run it before and after anything that touches startup or decoding.
- `build.rs` — embeds `devoid.ico` and version resources. `devoid_viewer_setup.iss` — Inno Setup installer.

## Critical rules
- **Read [docs/settled-decisions.md](docs/settled-decisions.md) before proposing a change to paths, formats,
  directory listing or window behaviour.** A decision there is re-opened by a new fact, never by an argument.
  Where a change genuinely moves one, revise the entry in the same commit.
- **Never work on `main`.** Branch off its tip (`feature/<slug>`, `fix/<slug>`, `chore/<slug>`); merging and
  pushing are the maintainer's.
- **Verify with the `/verify` skill before calling anything green.** Report its numbers, not an impression.
  CI (`.github/workflows/ci.yml`) runs the same recipe on every push to `main` and every pull request, with clippy
  at `-D warnings`. **Change both together**: a step added to one and not the other means a local green that CI
  reds, or the reverse.
- **Documentation lives in this repository as Markdown**, where it can be grepped, diffed and changed in the same
  commit as the code it describes. `DevoidViewer_Codebase_Guide.pdf` is not the reference — see the open decision.

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
- **Commit bodies are the archive**: what changed, why it was not the obvious thing, and the evidence. Search them
  with `git log --grep=<term>`.
