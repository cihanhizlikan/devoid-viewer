# Project profile — Devoid Viewer

**What this file is.** The four workflow skills — `/sweep`, `/wp-start`, `/wp-verify`, `/wp-finish` — came from
Patina (2026-11-08), which ported them to Rust from the maintainer's C# repository, Yalbuz. Everything they need to
know that is true of THIS repository lives here, as named keys, and the skills read it by key (`profile:<key>`). **A
Rust repository adopting them copies the skills and writes its own profile**; the values below were measured here,
not copied from Patina's.

**An absent key is not a default.** A step whose key is missing, `none`, or names something that does not exist
reports **UNAVAILABLE (profile: `<key>`)** and does nothing else. It never reports "measured clean": a step that
read nothing has measured nothing.

**Values are facts about this repository, so they decay like any other.** The sweep's context-drift step checks
that every path below still resolves.

## The crate

| Key | Value |
|---|---|
| `crate` | one binary crate, `Cargo.toml` (`devoid_viewer`, exe `DevoidViewer.exe`), plus `build.rs` |
| `production_sources` | `src/*.rs` outside `#[cfg(test)]` modules; `build.rs`; `devoid.ico` (reached through `build.rs`); `devoid_viewer_setup.iss` (the installer) |
| `test_sources` | the `#[cfg(test)] mod tests` inside each `src/*.rs`; there is no `tests/` directory |
| `fixture_excludes` | none — tests build their images in memory and write them to `tempfile` directories |
| `host_types` | a winit 0.29 event loop drawing through softbuffer (CPU blit); background work on `std::thread::spawn` threads that report over one bounded `crossbeam_channel` and wake the loop with `WakeUp`; message boxes on their own threads; release builds `panic = "abort"`, so a panic ends the process and Windows logs it (no hook) |
| `co_located_products` | `devoid_viewer_setup.iss`, the Inno Setup 7 installer, tied to the code by `installer_reads_its_version_from_the_exe` and `installer_associates_exactly_the_supported_extensions`, and compiled by `profile:installer_build` |

## Conventions the build or suite already enforces

| Key | Value |
|---|---|
| `enforced_guards` | path bans (`clippy.toml` `disallowed_methods`, `deny` in `Cargo.toml`); `unsafe_code = "deny"`, one allowed module (`src/win32.rs`); rustfmt (`cargo fmt --check`, a gate); zero clippy warnings (`-D warnings` in CI); `cargo deny` (`deny.toml`) in CI |
| `dead_code_lint` | rustc `dead_code`, warn by default (an error in CI through `-D warnings`). Every module is private (`mod loader;`), so its `pub` items are crate-private and the lint sees them, even one only a test calls — the shape Patina measured 2026-11-08, which is this crate's shape too. What it cannot see is what is switched off: every `#[allow(dead_code)]` (0 on 2026-11-08); there is no other platform's half (deleted 2026-11-08; `compile_error!` on any non-Windows target) |
| `lint_config` | `Cargo.toml` `[lints.rust]` / `[lints.clippy]`, `clippy.toml`; clippy's default groups only, so every `pedantic`, `nursery` and `restriction` lint is OFF until a probe proves it (`cargo clippy --all-targets -- -W clippy::<lint>`) |
| `toolchain` | pinned, `rust-toolchain.toml` (1.96.0 with clippy and rustfmt) |
| `line_endings` | `core.autocrlf=true`, no `.gitattributes`: LF in the index, CRLF in the checkout, every tracked text file (`git ls-files --eol`, 2026-11-08). A scripted edit keeps each file's own endings; a whole-file `git diff --stat` means it did not. `sed -i` leaves a touched file LF in the checkout and git normalises it on `add` (the "LF will be replaced by CRLF" warning is that, not drift). Nothing enforces it |

## Documents the steps read

| Key | Value |
|---|---|
| `context_docs` | `CLAUDE.md`, `.claude/skills/*/SKILL.md`, this file, `docs/settled-decisions.md`, `docs/roadmap.md`, `docs/architecture.md`, `docs/installer.md`, `docs/benchmarks.md` |
| `structure_doc` | `CLAUDE.md`, section *Layout* — one line per module saying what it owns |
| `naming_rule` | none |
| `reviewer_carried_rules` | `CLAUDE.md`, section *Working style*: safe Rust, evidence, every new dependency justified (with its size and startup cost), quality over compatibility |
| `settled_decisions` | `docs/settled-decisions.md`; its *Open decisions* part is not settled |
| `config_behaviours` | none — the viewer has no configuration file; its tunables are `const`s in the source |
| `config_coverage_test` | none |
| `effective_config` | none |

## The plan

| Key | Value |
|---|---|
| `plan_rule` | **the plan is in the repository**: `docs/roadmap.md`, the work queue in order. A handed-over package is a row in it, committed on the step's branch, so it merges with the step — there is no plan file outside the tree and no plan-mode round trip |
| `plan_known_path` | `docs/roadmap.md` |
| `history_paths` | none |
| `offsite_register` | none — a check only the maintainer can make (a UAC prompt, a Settings click, a Windows 10 machine) is a roadmap row saying so |

## Verification

| Key | Value |
|---|---|
| `shell` | **PowerShell for every cargo command**, never Git Bash (its `/usr/bin/link` shadows the MSVC linker), with `$env:PATH = "$HOME\.cargo\bin;$env:PATH"` |
| `secrets_file` | none |
| `baseline_ignored` | **1** — `recycle_removes_the_file_from_its_folder` (`src/main.rs`; it writes to the Recycle Bin, run with `cargo test -- --ignored`, maintainer's choice 2026-11-08). Timing probes are written, run and removed in the same package |
| `warning_baseline` | **0** clippy warnings, any target; CI enforces it with `-D warnings` |
| `ci` | `.github/workflows/ci.yml`: the `/wp-verify` recipe on `windows-latest` (every cargo call `--locked`, clippy `-D warnings`) and `cargo deny --locked check` on `ubuntu-latest`, on every push to `main`, every pull request and weekly. Changes together with `/wp-verify` |
| `coverage_command` | none — `cargo-llvm-cov` is not installed (2026-11-08); never installed by an unattended run |
| `mutation_command` | none — `cargo-mutants` is not installed (2026-11-08) |
| `audit_command` | `cargo deny --locked check` — cargo-deny 0.20.2 against `deny.toml` (advisories, bans, licences, sources); fetches the RustSec database, so it needs the network. LIVE 2026-11-08: exit 0 |
| `update_command` | `cargo update --dry-run` — semver-compatible updates only, nothing installed; LIVE 2026-11-08 |
| `duplicates_command` | `cargo tree -d --target x86_64-pc-windows-msvc --edges normal` — known and deliberate (2026-11-08): `windows-sys` 0.48 (winit 0.29, notify) beside 0.61, and `raw-window-handle` 0.5 (winit) beside 0.6 |
| `clean_rebuild` | after every commit: `cargo clean`, then `cargo build --release --locked`; report the exit code and the size of `target\release\DevoidViewer.exe`. A sweep: once, after its last merge |
| `benchmark` | `scripts\bench.ps1` — time to visible window and memory of the release exe; method and every baseline in `docs/benchmarks.md`. Its numbers are only comparable on an otherwise idle machine (another project's `cargo` build tripled them on 2026-11-01) |
| `installer_build` | `& 'C:\Program Files\Inno Setup 7\ISCC.exe' /Q devoid_viewer_setup.iss`, after `cargo build --release --locked`: compiles `Output\DevoidViewer_Setup.exe` (gitignored), whose `ProductVersion` must equal `Cargo.toml`'s version. Compiling is not installing: the setup exe is never run by a session unasked. Inno Setup 7.1.1 is on the maintainer's machine; CI has none, so this check is local only. LIVE 2026-11-08: exit 0, 0.2.0 |
| `push_policy` | a session pushes `main` only when the maintainer's request says so |

## The surfaces people read

| Key | Value |
|---|---|
| `user_docs` | `README.md`; the help overlay's words (`BINDINGS` and `TITLE` in `src/help.rs`); dialog and error strings in `src/main.rs`; the installer's task and finish-page wording in `devoid_viewer_setup.iss` |
| `cultures` | `en` only |
| `wording_rules` | none |
| `ui_register` | none — the nearest are the *Help is an overlay* and *Small images are enlarged* entries in `docs/settled-decisions.md`, which are decisions, not a register of read/build rows |
| `run_command` | `target\release\DevoidViewer.exe <image>` — the RELEASE exe. The real window is found by enumerating the process's windows and skipping winit's hidden `Winit Thread Event Target` (`Process.MainWindowHandle` finds that one); `scripts\bench.ps1` does it |
| `render_harnesses` | the renderer writes into a plain `&mut [u32]`, so tests draw without a window: `render` in `src/main.rs`, `Help::draw` in `src/help.rs`, and `win32::render_text` for real Segoe UI glyphs |
| `theme_rules` | none — one background colour (`BG` in `src/main.rs`) and the help overlay's two text colours (`src/help.rs`) |
