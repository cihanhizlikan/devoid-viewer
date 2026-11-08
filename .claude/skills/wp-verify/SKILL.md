---
name: wp-verify
description: Verify a work package before reporting it — build, clippy warning count, the test suite with failures named, the formatter and the dependency policy, with this machine's known traps handled. Invoke before claiming any change is green. (Build and lint verification; the bundled /verify, which drives the running app, is a different skill.)
---

# Verifying a work package

**Every step is written the way it is because the obvious way measured nothing.** Report one line at the end:
build exit, clippy warnings/errors, tests passed/failed/ignored, fmt exit, and deny exit when step 5 ran. Report
the numbers, not an impression. Values marked `profile:<key>` are in `.claude/project-profile.md`.

**CI runs this same recipe** (`profile:ci`) and is stricter in one way: clippy runs with `-D warnings`, so any
warning here fails the pull request there. Every cargo call there passes `--locked`. **Keep the two in step** — a
change to one is a change to both, in the same commit.

## 0. Shell and toolchain

Run every cargo command through **PowerShell**, never Git Bash (`profile:shell`): Git Bash's `/usr/bin/link`
shadows the MSVC linker and the build dies with `link: extra operand`. Prepend cargo to `PATH` in each call:

```powershell
$env:PATH = "$HOME\.cargo\bin;$env:PATH"
```

`link.exe not found` means the Visual Studio C++ build tools are missing. That is a blocker to report, not a step
to skip — nothing after it measures anything.

## 1. Build, into a file

```powershell
cargo build --all-targets --message-format=short *> "$env:TEMP\dv-build.txt"; "exit=$LASTEXITCODE"
```

**The exit code is the primary signal; read it first.** A non-zero exit has failed however the output looks, and
a failed `build.rs` (the resource compiler) stops the crate before a single `.rs` diagnostic: its silence is not
a clean result.

## 2. Clippy

```powershell
cargo clippy --all-targets --message-format=short *> "$env:TEMP\dv-clippy.txt"; "exit=$LASTEXITCODE"
(Select-String -Path "$env:TEMP\dv-clippy.txt" -Pattern ': warning: ').Count
(Select-String -Path "$env:TEMP\dv-clippy.txt" -Pattern ': error').Count
```

Match `: warning: ` with the leading colon: in short format every diagnostic reads `src\main.rs:12:5: warning: …`,
while cargo's own summary line (`warning: \`devoid_viewer\` (bin) generated N warnings`) has no colon before it and
must not be counted. Cargo replays cached warnings for unchanged crates, so a no-op rebuild still reports them.

A `disallowed_methods` hit is an **error** by design — see `clippy.toml` and `docs/settled-decisions.md`.
**Zero warnings is the bar** (`profile:warning_baseline`): any warning is this package's to explain, fixed in the
same commit or allowed at the site with a comment saying why. CI would fail it anyway.

## 3. Tests, into a file, failures by NAME

```powershell
cargo test *> "$env:TEMP\dv-test.txt"; "exit=$LASTEXITCODE"
Select-String -Path "$env:TEMP\dv-test.txt" -Pattern '^test result:'
Select-String -Path "$env:TEMP\dv-test.txt" -Pattern '^test .* FAILED$|^---- .* stdout ----'
```

Sum the `test result:` lines — there is one per test binary. **`profile:baseline_ignored` is 1**, named in the profile; a second ignored
test is a finding.

**Capture to a file and grep the failure lines; never pipe the run straight to a tail.** The summary says how many
failed and never which, and an intermittent failure you cannot name is one you cannot fix. **Record the failing
test's name before re-running anything**, then run it alone, several times: `cargo test <name>`.

## 4. Formatter (a gate)

```powershell
cargo fmt --check *> "$env:TEMP\dv-fmt.txt"; "exit=$LASTEXITCODE"; (Select-String -Path "$env:TEMP\dv-fmt.txt" -Pattern '^Diff in').Count
```

Default rustfmt (`docs/settled-decisions.md`); CI fails on any diff. A non-zero exit fails verification: run
`cargo fmt` and include the result in the same commit, rather than hand-aligning.

## 5. Dependencies (when `Cargo.toml`, `Cargo.lock` or `deny.toml` changed)

`profile:audit_command`. Needs `cargo install cargo-deny --locked` once — said to the maintainer before it is run.

```powershell
cargo deny --locked check *> "$env:TEMP\dv-deny.txt"; "exit=$LASTEXITCODE"
Select-String -Path "$env:TEMP\dv-deny.txt" -Pattern '^(error|warning)\[|advisories '
```

A gate: the exit must be 0. The duplicate-version warnings are known (`profile:duplicates_command`); a new one is a
finding. An advisory exception goes in `deny.toml` with its reason and, where it trades against size or startup,
an entry in `docs/settled-decisions.md`. CI also runs this weekly, since advisories arrive without any change here.

## Experiments

**Never `git checkout` or `git restore` a file to undo an experiment** — it discards every uncommitted change in
it, not just the experiment. To show a test fails without a fix (revert-confirm-red), save the file's text, make
the one change, run the one test, and write the saved text back; then `git diff --stat` must show only the
package's own changes.
