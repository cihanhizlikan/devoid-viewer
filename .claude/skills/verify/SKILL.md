---
name: verify
description: Verify a change before reporting it — build, clippy warning count, tests and the formatter, with this machine's known traps handled. Invoke before claiming any change is green.
---

# Verifying a change

**Report the numbers, not an impression.** One line at the end: build exit, clippy warnings/errors, tests
passed/failed, fmt diff count.

## 0. Shell and toolchain

Run every cargo command through **PowerShell**, never Git Bash: Git Bash's `/usr/bin/link` shadows the MSVC linker
and the build dies with `link: extra operand`. Prepend cargo to `PATH` in each call:

```powershell
$env:PATH = "$HOME\.cargo\bin;$env:PATH"
```

`link.exe not found` means the Visual Studio C++ build tools are missing. That is a blocker to report, not a step
to skip — nothing after it measures anything.

## 1. Build

```powershell
cargo build --all-targets --message-format=short *> "$env:TEMP\dv-build.txt"; "exit=$LASTEXITCODE"
```

**The exit code is the primary signal; read it first.** A non-zero exit has failed however the output looks.

## 2. Clippy

```powershell
cargo clippy --all-targets --message-format=short *> "$env:TEMP\dv-clippy.txt"; "exit=$LASTEXITCODE"
(Select-String -Path "$env:TEMP\dv-clippy.txt" -Pattern ': warning: ').Count
(Select-String -Path "$env:TEMP\dv-clippy.txt" -Pattern ': error: ').Count
```

Match `: warning: ` with the leading colon: in short format every diagnostic reads `src\main.rs:12:5: warning: …`,
while cargo's own summary line (`warning: \`devoid_viewer\` (bin) generated N warnings`) has no colon before it and
must not be counted. Cargo replays cached warnings for unchanged crates, so a no-op rebuild still reports them.

A `disallowed_methods` hit is an **error** by design — see `clippy.toml` and `docs/settled-decisions.md`.
Compare the warning count against the last commit's; a rise is a finding, triaged before it is reported.

## 3. Tests

```powershell
cargo test *> "$env:TEMP\dv-test.txt"; "exit=$LASTEXITCODE"
Select-String -Path "$env:TEMP\dv-test.txt" -Pattern '^test result:'
```

Sum the `test result:` lines — there is one per test binary.

## 4. Formatter

```powershell
cargo fmt --check *> "$env:TEMP\dv-fmt.txt"; (Select-String -Path "$env:TEMP\dv-fmt.txt" -Pattern '^Diff in').Count
```

**Report only, not a gate**, until the rustfmt open decision in `docs/settled-decisions.md` is taken.
