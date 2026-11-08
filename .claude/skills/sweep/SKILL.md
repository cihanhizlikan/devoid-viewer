---
name: sweep
description: A sequenced quality audit of the whole repository — structure, test health, dependencies, dead code, configuration, robustness, maintainability and footprint, context-document drift, the user-facing docs and UI drift. It runs UNATTENDED: it never stops and never asks, merging each step and writing anything that needs a decision into the plan instead. Project-specific values come from .claude/project-profile.md. The maintainer starts it and reads the report when they return.
disable-model-invocation: true
---

# The sweep

**One audit, ten steps, in an order that is itself the deliverable.** It corrects what is unambiguous, merges each
step, and hands everything else to the plan as a work package.

**Where this comes from.** It is Patina's sweep (2026-11-08), itself the Rust port of the one in the maintainer's
C# repository, Yalbuz, whose ten runs taught most of what is below. The RULES carried over unchanged; the DETECTORS
are this repository's — each step names its Rust instrument. **Keep the copies in step by rule, not by text**: a
lesson one repository learns is ported, re-measured, to the others.

**The line between correcting and handing over is not a count.** A step CORRECTS a site whose repair is
unambiguous and whose subject is its own; the moment a finding needs a POLICY over sites (which `let _ =` may stay,
which `pub` should narrow) it is a package however few sites it has.

**Devoid's one goal is an instant open with the smallest footprint** (`CLAUDE.md`). A correction that adds startup
work, memory or binary size is not unambiguous, whatever else it fixes: it is a package with its cost measured.

## The profile, and why this file names no path it can avoid

**Everything true of one repository lives in `.claude/project-profile.md`, as named keys**, referred to here as
`profile:<key>`. **An absent key is UNAVAILABLE, never clean**: a step whose input key is missing, `none`, or names
something that does not exist reports `UNAVAILABLE (profile: <key>)` on its report line and does nothing else —
"found nothing" reads exactly like "measured clean". **No profile at all: run preflight only, list the keys the
steps would have needed, and end.** Examples marked *(illustrative)* show a SHAPE to look for, not a list to check.

## Invocation

**The maintainer starts a sweep; a session never does** — hence `disable-model-invocation: true`. Work is ISSUED,
never taken. A session may propose one, with a reason.

## It runs UNATTENDED, and that is the whole design

**A sweep is started when the maintainer is leaving and expects to come back to finished work.**

- **NEVER stop to ask.** Not for a decision, a preference, permission, or to report progress. Do not call
  `AskUserQuestion`; do not end a turn with a question. **There is nobody there**, so a question is the run halting
  for hours and delivering nothing *(illustrative: a Yalbuz run that reported after step 1 was restarted by hand the
  next morning)*.
- **Anything that WOULD have been a question is written into the plan and SKIPPED**: what was measured, the
  options, which one is recommended and why — then move on.
- **Merge, delete and continue.** Each step's branch is merged into `main` and deleted by the sweep itself, before
  the next step opens. **This is an explicit exception** to *merging is the maintainer's* and *a merge instruction is
  per-request* (`/wp-finish`), adopted with this skill (2026-11-08) as Patina and Yalbuz grant it. **It has a
  limit**: a sweep merges work nobody has reviewed, and only the unambiguity bar keeps that honest. Widening the bar
  turns the exception into something it was never granted for.
- **Nothing is pushed** (`profile:push_policy`): a sweep's request is "audit", not "push".
- **Nothing is installed** — no cargo tool, no build of the installer run against the machine, no app install. A
  detector that needs one is UNAVAILABLE.
- **Report only at the END**, in one message covering every step. A mid-run progress report is a stop wearing a
  report's clothes.

### Ending a run is allowed; WAITING is not

A sweep may **finish early** — a red baseline, a tree that will not build, a session out of room — and that is a
complete outcome: record why in the plan, say so in the report, exit. It may never **sit there** having asked.

### The ways a run goes wrong mid-flight, and the one legal move for each

- **An action is REFUSED** (a permission classifier declines a delete, a navigation). **Treat it as a BLOCKED
  detector**: the exact refused command into the plan, and carry on without it. **Never route around a refusal** —
  re-issuing the refused operation in another form (narrower, reworded, split, through another tool) is taking the
  decision the refusal made. The one exception: a command refused for breaking a STATED rule of this repository is
  replaced by the form the rule requires. Unsure which → it is the first kind. **Retrying a FAILURE is not routing
  around a refusal**: an error nobody chose (a locked file, a timeout) is retried as it was.
- **The suite goes RED inside a step.** The step does not merge. **Capture the failing test's NAME first**
  (`/wp-verify` step 3), then run it alone several times:
  - fails alone and the step touched its subject → revert the step's changes (`/wp-verify`, *Experiments*), hand
    the finding over;
  - passes every lone run → load-sensitive; re-run the suite once, and if green the step merges and the test becomes
    a plan row for the next run's step 2, with the captured failure message;
  - fails alone and the step did not touch it → the tree moved under the run; that ends it as a red baseline does.
- **Starting the app or a screenshot is denied or times out.** That instrument is UNAVAILABLE for this run, named
  once in the report with the exact call. **Fall back to `profile:render_harnesses`.** Never call a step clean that
  rendered nothing.
- **Plan mode switches on mid-run.** No step can continue. Record it as the reason the run ended, leave no step
  half-applied (uncommitted work is committed on its branch and left UNMERGED, and the report names it), report, end.
- **The session is running out of room.** The likeliest ending, not a failure. **Finish the step in hand rather than
  opening the next**, and write into the plan which step stopped it and what the next one was.

### How a finding reaches the plan without blocking

`profile:plan_rule`. **Devoid's plan is in the repository** (`docs/roadmap.md`), so a handed-over package is a row
committed on the step's own branch and merged with it — durable the moment the step merges, versioned, and visible
to the maintainer in the tree. Rows go under the `## From sweeps` heading at the end of the roadmap, newest run
last. **Never start a second register** — a plan file outside the tree, a `findings.md` — for anything the roadmap
can hold.

---

# How a run works

## The order is the deliverable, and a step ends in a merge

**A step fixes what it can and hands over what it cannot. The line is the step's own subject.** The order is real
work: structure moves code that dead code and maintainability would otherwise write findings against, and a finding
raised before the move names paths that stop existing. **A step that produced only prose has not finished** —
either it measured nothing to do and says so, or it left work behind.

## Scope: what changed since the last sweep

- **The baseline**: every sweep merge subject starts `merge: sweep <YYYY-MM-DD> step <n>`, so the last one is
  `git log --grep='^merge: sweep ' -1 --format='%H %ad %s' --date=short`. Report it. **No marker → the scope is the
  whole tree, and the report says so** (true of the first run).
- **Changed files**: `git diff --name-only <baseline>..HEAD`.
- **Whole tree regardless**: step 2's repeated suite runs, step 3 (dependencies age without a commit), step 8's
  mechanical path check, and any step whose detector is newly LIVE this run.

## Preflight

1. **Clean tree, green `/wp-verify`.** A sweep on a red tree measures nothing. **If it is not green, the run ENDS**
   — what is broken into the plan, say so, exit. Do not repair it. **Green is necessary and not sufficient** — step
   2 exists because a suite can be green precisely because some tests cannot fail.
2. **The profile.** Read `.claude/project-profile.md`; list every key a step needs that is absent or `none`.
3. **Detector liveness.** Every detector a step names must be **proven to emit a value on this repository, by
   running it**. Each lands in exactly one state: **LIVE** (ran, produced a parseable value), **UNAVAILABLE** (did
   not run here, this time — reported once with the exact command), **BLOCKED** (should work, failed for a fixable
   repo-specific reason — into the plan with the command; carry on without it).

   **A lint is a detector, and a lint that is off reads exactly like a lint at zero.** `profile:lint_config` says
   which are on; every `pedantic`, `nursery` and `restriction` lint is OFF. Turn one on for a single run from the
   command line — `cargo clippy --all-targets --message-format=short -- -W clippy::<lint>` — which edits no file.
   **A zero from it is believed only after the same command has emitted that lint once** on a probe that breaks it,
   in a scratch crate under the scratchpad (never in the tree). **Probe the shape the lint documents**, not a guess
   at it.

   **An inferred zero is not a measurement, and neither is an unexamined non-zero.**
4. **The footprint baseline.** `profile:benchmark` once, on `main`'s release exe, before step 1 — the numbers every
   later step that touches startup or decoding is compared against. Not idle (another build running) → UNAVAILABLE,
   and step 7's footprint half with it.

## Derive, never cache

**Derive every list at the start of every run, from the tree, and write none of them down** — module lists, item
counts, call sites. Every cached list decays. The ways a derivation goes quietly wrong — shapes that look DEAD and
are alive. Check each before believing a "no caller" result:

- **A trait implementation the framework calls**: winit's event handling, `Drop` (the `Release` guards in
  `src/win32.rs`), `Default`, `Display` (reached by `format!`), `From` (reached by `?` and `.into()`). No call site
  names them.
- **A test function** (`#[test]`, found by the harness) and a `#[cfg(test)]` helper.
- **Reached through a macro or a build script**: `include_bytes!`/`include_str!` (a file used only by the path
  string inside one), and `build.rs`, which is its own crate and reaches `devoid.ico`.
- **Reached from the installer**: `devoid_viewer_setup.iss` names the exe, the extensions and the ProgID; tests in
  `src/loader.rs` and `src/main.rs` tie it to the code (`profile:co_located_products`).
- **Platform-gated code**: a `#[cfg(windows)]` or `#[cfg(not(windows))]` item is compiled on one platform only, so
  a Windows build is silent about the other half.
- **Read what the matches ARE, not how many there are**: a doc comment or a context file naming an item pads a
  count without moving the fact.

## What a finding looks like

**Hand over a DECISION, never an inventory.** Triage before the maintainer sees anything: read each site, mark it
take-or-decline with its reason, and lead with the count that SURVIVED. Each triaged item lands in exactly one place:

- **Corrected in this step** — inside the step's subject, the repair unambiguous, the suite proves it. The commit
  body is the record.
- **A work package in the plan** — needs a judgement the sweep cannot make, or reaches outside this step's subject.
- **A settled decision** (`profile:settled_decisions`) — refused on evidence, with the measurement and what would
  re-open it.
- **A check only the maintainer can make** (`profile:offsite_register`; `none` → a roadmap row saying so).

**When it is not obvious, it is a package rather than a correction.** Narrowing scope to avoid a decision counts
as deciding; widening it to avoid asking is the same move in reverse. **Neither is a reason to ask.**

Each item handed over carries: **What** (one sentence); **Where** (a path and line that resolve today); **Why it is
not the obvious thing**; **What would falsify it**; **Origin: sweep `<date>` step `<n>`, not yet committed to
build**. **Those fields are for a handed-over item, never a commit body**, which `/wp-finish` caps at ~1,500 bytes.

**A measured "nothing here" is a complete deliverable.** It opens no branch and writes no plan row; its record is
its report line. **Every number states what produced it** — which command, which targets, which scope.

---

# The ten steps

## 1. Structure and naming
**First, because every later finding is written against the tree this step would move.**

Do not re-detect anything in `profile:enforced_guards`. *The candidate set*: `profile:structure_doc` — each module's
line read against what the module actually holds (an item that belongs to another module's line is misplaced; a
module doing what no line says is undocumented; a Win32 call outside `src/win32.rs` is already a build error); and
`profile:naming_rule`. Either key absent → that half is UNAVAILABLE, never clean.

## 2. Test-suite health
**Second, out of blast-radius order, deliberately: every step after this leans on the suite.**

*The rule it enforces*: a test asserts a requirement, never a preference — *would a redesign nobody would call a
regression turn this red?* Over `profile:test_sources`.

*The candidate set, derived — searches, not judgements:*
- **A test that cannot fail by construction** — no `assert`; the only assertion inside a loop that can run zero
  times; a value compared with itself.
- **A preference-assertion** — a colour, a pixel offset, an exact layout coordinate, a font size. (A test that pins
  the help overlay's dim or the README table to `BINDINGS` asserts a decision recorded in
  `docs/settled-decisions.md`, or a contract between two files — read which before calling it a preference.)
- **An unbounded wait** — a `loop` or `recv()` with no bound or timeout. It hangs the suite instead of naming an
  assertion.
- **A name that no longer describes the body** — the tell for a test edited until it passed.
- **Load-sensitive tests** — run the WHOLE suite at least three times; any test that fails once is captured by name
  and run alone ten times. Passes alone, fails under load → a timing assumption, handed over with its message. The
  tempdir tests in `src/nav.rs` and `src/loader.rs` are the likeliest shape; the directory watcher has no test.
- **The ignored set** — `#[ignore]` against `profile:baseline_ignored` (1, named there). A second is a finding, and so is the named one if its reason stops being true.
- **A write to the user's files with no test reading the disk afterwards** — rename and Recycle Bin delete
  (`CLAUDE.md`).
- **Uncovered production code** — `profile:coverage_command`; `none` → UNAVAILABLE. **Prove it LIVE by checking the
  report exists and the run executed tests** *(illustrative: a Yalbuz collector its runner never loaded ran zero
  tests and still looked like a coverage step)*.

*Then revert-confirm-red the survivors*, each against the ACTUAL defect it claims to catch (`/wp-verify`,
*Experiments*). `profile:mutation_command` automates that; `none` → UNAVAILABLE, **never installed by an unattended
run**.

## 3. Dependency hygiene
**After test health, because a version bump changes runtime behaviour and needs a suite already shown to fail when
it should. Before everything that reads a lint**, because a clippy or toolchain change moves what steps 6 and 7
measure.

- **Policy** — `profile:audit_command` (advisories, bans, licences, sources against `deny.toml`). Read each
  advisory by KIND: a *vulnerability* or *unsound* advisory whose fixed version is semver-compatible is a correction
  (`cargo update -p <crate>`); one that needs a major bump of a direct dependency is a package naming the dependency
  that pulls it in (`cargo tree -i <crate>`); *unmaintained* and *yanked* are packages. **A transitive advisory is
  still Devoid's to name.** A platform-only advisory (a Unix handler, a 32-bit overflow) is triaged against the
  platform Devoid ships (64-bit Windows), and a decline says so — in `deny.toml` with its reason.
- **Compatible updates** — `profile:update_command`. A patch or minor update of a direct dependency is a correction
  only when the suite stays green **and the release exe did not grow** (size before and after, in the commit); a
  MAJOR is a package with its breaking changes named (winit 0.29 → 0.30 is one, and it is not small).
- **Duplicates** — `profile:duplicates_command`; a crate at two versions costs binary size. A known, deliberate pair
  is in the profile and not a finding.
- **Every new crate a correction would add is justified** per `CLAUDE.md` (*Justify every new dependency*): its
  maintenance, the first-party alternative, its size and startup cost. A correction that cannot say so is a package.
- **`--locked` holds**: `cargo build --release --locked` fails on a lockfile that moved without a commit.

## 4. Dead code
Needs the net step 2 just checked. **Refactor residue is the subject** — a helper left behind by a change, a seam
nothing uses, a name that stopped being true.

**rustc already finds most of it — read what it cannot.** `profile:dead_code_lint` says what the lint sees here;
with zero warnings, everything it sees is alive, so the candidate set is exactly what it was switched off for or
never compiled:

- **every `#[allow(dead_code)]`** — a suppression that claims a purpose (*"useful API"*, *"will be needed"*) with
  no production caller is residue claiming a future. Delete the attribute in the step's branch and build: rustc
  names what was hiding behind it;
- **the other platform's half** — a `#[cfg(not(windows))]` item is never compiled here, so its callers are read,
  not built. Devoid ships only on Windows; whether each stub still earns its place is a package, not a deletion;
- **`pub` items at the crate root**, which the lint exempts (none today — re-derive);
- **Cargo features and dependencies** nothing uses — an `image` feature for a format the loader no longer accepts
  is binary size (`docs/settled-decisions.md`: raster formats only);
- **files no build reaches** — an icon, a script, a doc no other file names.

**A production item whose only caller is a test is dead production code** — and the lint already says so, unless
an `allow` silences it; a deliberate test seam says so in its name or doc comment. **Check the probe's SHAPE before
generalising from it** (*Rules that bind every step*).

*Before configuration, because removing code is what orphans a tunable.*

## 5. Configuration knobs
**Triggered, not routine.** It runs when step 4 removed code, or when a tunable or a validation changed since the
baseline. `profile:config_behaviours` `none` → its line says *UNAVAILABLE (profile: config_behaviours)* and names the
tunables (`const`s) step 4 orphaned, if any, as step 4's to remove.

## 6. Robustness and resilience
**The surface is settled by now, so a guard goes where it belongs.** Read the build first (any lint on starts at
zero); then the shapes no default lint catches:

- **Errors that are lost, not returned.** `let _ = <a Result>` and `.ok()` on a Result whose failure the user would
  want to know about — a rename, a Recycle Bin delete, a decode. For each site read what is DISCARDED: a channel
  `send` to a receiver that may be gone, a `send_event(WakeUp)` to a closed loop, a `request_inner_size` the window
  manager may ignore, or a `present()` of a frame the next redraw replaces, is fine to drop; the outcome of a
  change to the user's file is not.
- **A dropped `JoinHandle`**: a `std::thread::spawn` whose panic nobody joins. Release builds are `panic = "abort"`
  (`profile:host_types`), so a panic on any thread ends the viewer — in debug it is a thread that dies and a loop
  that waits for a message that never comes.
- **A panic on untrusted input** — `unwrap`/`expect`, slice indexing and unchecked offset arithmetic in code that
  reads a FILE (magic-byte sniffing, header walking in `src/loader.rs`). Probe `clippy::unwrap_used`,
  `clippy::indexing_slicing` and **`clippy::string_slice`** from the command line (they apply crate-wide) and read
  only the hits in production code — a hit at or after its file's first `#[cfg(test)]` is a test's, so cut the
  list there before reading it. **`string_slice` is the one to read first**: `&s[..n]` on a `str` panics when `n`
  falls inside a character, and a user's path or filename (`fotoğraflar`) is where it does *(illustrative: it found
  the one real panic of Patina's sweep 2026-11-08, buried among 554 hits of the other three)*. An index that comes
  from `find` of an ASCII pattern is on a boundary and is declined as read. **`arithmetic_side_effects` is not
  probed**: on 64-bit `usize` offset sums it measured almost all noise in Patina.
- **A decode whose size comes from the file** — a header's width × height allocated before it is checked against
  `image`'s limits is an out-of-memory abort on a crafted file.
- **A change to the user's file whose test does not read the disk back** (`CLAUDE.md`).
- **The reviewer-carried rules** in `profile:reviewer_carried_rules`.

**A missing guard is a correction only when the code already has a path to route the failure to** (an error
dialog, the bad-file message); otherwise a package.

## 7. Maintainability, performance and footprint
**Late, because it operates on code whose shape and guards are now settled.**

**A performance finding needs a measurement**: a stated complexity change (`O(n·m)` → `O(n+m)`, named in the
commit), or `profile:benchmark` before and after on the same idle machine, or a timing probe written, run and
removed in the step. Anything else is speculation and is dropped.

**Footprint is a performance finding here**: the release exe's size against the last clean rebuild's, and the
benchmark's time-to-window and memory against preflight's. A rise since the baseline with no commit body that
explains it is handed over with the commits that span it.

**Complexity detectors**: `clippy::cognitive_complexity` and `clippy::too_many_lines` (off; probe from the command
line). **Never cut a diagnostic line before deduplicating it** — one site can carry two lints. **A suppression or
comment that STATES a number a tool computes is a cached derived value**: remove the number, keep the reason.

## 8. Context-document drift
**After every step that moves code, before the user-facing words.** Over `profile:context_docs`.

- **Mechanical, whole tree**: every backticked repository path must resolve. Triage each miss: **gitignored**
  (`git check-ignore -q <path>`) — expected; **deleted on purpose and named as history** — the sentence says so;
  **relative to a folder the sentence names** — resolve it there; a bare module name (`nav.rs`) resolves under
  `src/` before it counts as a miss. What is left is drift, and the fix is the path it
  moved to (`git log --follow --diff-filter=R`). The count of misses is never the finding. **Run the check from
  PowerShell** (`[regex]::Matches` over each file, then `Test-Path`): in Git Bash, `${p//\\//}` deletes every `/`
  rather than turning `\` into `/`, and the tool layer mangles a `sed` backslash, so the first run of sweep
  2026-11-08 reported `src/main.rs` missing. A miss on a file that plainly exists is the instrument, not drift.
- **Line references in `docs/roadmap.md`**: every `path:line` (and bare `:line` after one) a row cites must still
  point at the item it names. A merge above the line moves it without touching the row: the second run of
  2026-11-08 found two that were stale after one fix had merged.
- **Claims of STATE**: a sentence saying a thing "is off", "is not split yet", "about 60 lines", a size in bytes, a
  benchmark number, or a count. Read each against what decides it. **A claim decays by being OVERTAKEN, not
  contradicted.**
- **`CLAUDE.md` against CI**: a step `/wp-verify` runs that `.github/workflows/ci.yml` does not, or the reverse, is
  drift (`CLAUDE.md`: *change both together*).
- **This skill and the profile are in scope.**

## 9. The user-facing docs
**The words a reader meets** — `profile:user_docs` in `profile:cultures`.

1. **Truth first**: a feature named that the app does not have, a format listed that the loader refuses, a link or
   image that does not exist, an instruction that no longer works. The README's key table and `BINDINGS` are tied
   by a test; the installer's strings are not.
2. **Structure and order** — does the sequence follow how somebody approaches the product (install, open, keys),
   or where each feature happened to be added?
3. **Wording** — concise and plain; cut a sentence that restates its heading and a qualifier that rules nothing out.
   `profile:wording_rules`; **search for the CONSTRUCTION a rule forbids across every string**, not the one somebody
   complained about last.

## 10. UI drift, against the register
**Last, and a LOOK rather than a measurement.** `profile:ui_register` is the authority; `none` → UNAVAILABLE, and
the report says what the nearest stand-in is (the profile names it) and recommends a register as a package only if
the surfaces have grown past the settled decisions that already govern them.

When a register exists: take its `read` rows as the checklist (a `build` row is enforced by something); look at the
COUNTS first, they decay by being overtaken; **check the INSTRUMENT before believing a drift report**; a surface
that disagrees with a row is drift, a proposal that disagrees is an argument with a decision. **Render and look** —
`profile:run_command`, else `profile:render_harnesses`. Where a change moves a rule, revise the register in the same
commit. **Never re-draw a surface in the register**: to see a surface, render it.

---

# Rules that bind every step

**Order is not priority.** Robustness outranks maintainability and performance wherever they meet, by veto:

> No simplification or optimization may delete an error path, turn a `?` or a handled `Err` into an `unwrap`,
> drop a bound or a timeout, or make a previously-isolated failure propagate.

A step-7 finding that would do any of those is recorded `waived (robustness)` and dropped.

**Check the INSTRUMENT before believing what it says.** A probe is written in a minute and is wrong as easily as
the thing it measures, so **a confident negative is the shape to distrust first** — read WHAT a search matched
before concluding anything from HOW MANY, and a probe's SHAPE before generalising from it *(illustrative,
Patina 2026-11-08: a probe with its `pub fn` at the crate root "proved" `dead_code` blind to every `pub` item; the
crate's `pub` items live in private modules, and a probe of that shape showed the lint sees them all. Devoid,
2026-11-06: `AssocQueryString` with `ASSOCSTR_EXECUTABLE` read empty for an AppX handler, which looked like "no
default set")*.

**Never re-raise a settled decision.** `profile:settled_decisions` carries what earlier work ruled; a ruling is
re-opened by a new fact, never by an argument. Where a context document and a settled decision disagree, the
decision is the ruling and the document is step 8's drift.

**Line endings are `profile:line_endings`.** A scripted edit (PowerShell `Get-Content -Raw`/`Set-Content`) keeps
each file's OWN endings — read whether it holds CRLF before writing it back, never normalise every file to one. **A
whole-file `git diff --stat` on a few-line change means the endings moved**: put them back before committing
*(illustrative: Patina's sweep 2026-11-08, a 9,929-line diff on main.rs, caught there)*.

---

# Finishing

**Each step with something to correct opens its own branch with `/wp-start`**, is verified with `/wp-verify`,
closed with `/wp-finish` (its commit body and size cap, then onward — `/wp-finish`'s *merge only on the maintainer's
word* is this skill's exception), merged, and deleted; then the next opens. **A step with nothing to correct opens
no branch.**

- **Branch `sweep-<n>-<subject>`; merge with `git merge --no-ff -m "merge: sweep <YYYY-MM-DD> step <n> — <subject>"`.**
  The merge commit is how the NEXT run finds its baseline; a fast-forward leaves no marker and makes the next
  run's scope the whole tree.
- **`profile:clean_rebuild` ONCE, after the last merge of the run**, not after every step's commit — the one
  exception to `CLAUDE.md`'s per-commit rule, adopted 2026-11-08 as Patina adopted it: nobody runs the exe mid-run,
  and each step's `/wp-verify` still builds and tests its own commit. The report gives the final build's exit code
  and size, and the size against preflight's. A run that ends early rebuilds after its last merge all the same.
- **Corrections travel in the step's own commits**; handed-over packages are roadmap rows on the step's branch;
  anything refused on evidence goes to `profile:settled_decisions`.

## The report, which is the one thing the maintainer reads

One message, at the end, in a fixed shape so two runs can be compared:

- **The run's own facts, first**: the scope baseline (commit and date, or "whole tree — no marker"); each detector
  as LIVE, UNAVAILABLE or BLOCKED; every profile key that was absent or `none`; anything REFUSED, with the exact
  command.
- **One line per step, all ten, in order** — a step that found nothing still gets its line, saying **what it
  measured**. Each says which it did: corrected (the commit), handed over (the roadmap row), refused on evidence
  (the settled entry), measured clean, not triggered, or UNAVAILABLE (the key or the instrument).
- **What the maintainer has to do** — the rows needing a decision, each with its recommendation already written.
- **What was left behind** — an unmerged branch, a step that ended the run early and why, the release build's exit
  code and size.

**A recommendation is not permission**: nothing in the report is a request to act; it is a record of what was done
and what is now the maintainer's.
