# Merging the Windows machine's pull requests

A scheduled cloud session reads this and does what a person used to ask for by
hand: take a pull request from a Windows machine's session
(`.claude/windows-role.md`), check it, merge it, and do the merger's share. The
merge is also what starts that machine's next run (`scripts/auto-wintest.ps1`
waits while one of its lane's is open), so a pull request left sitting stops
the loop.

There are two lanes, one per machine: `test/win-*` from the x64 machine and
`test/arm-*` from the ARM64 laptop. Each has its own queue in
`windows-role.md` -- "Where the work is" for `win`, "The ARM64 lane" for `arm`.

Read [CLAUDE.md](../CLAUDE.md) first; its rules apply in full. Reply in Japanese;
code, comments and commits in English. Nobody is watching: never wait for input.

## 1. Find the work

- List open pull requests whose head branch starts with `test/win-` or
  `test/arm-`. None: stop here and say so in one line. That is most runs.
- **One per run, oldest first.** The next run takes the next one; two merged in
  one run conflict with each other at the end of QA-REPORT.md.

## 2. Decide whether it may be merged

All of these, or it is not merged:

1. **It only touches what the Windows session may write**: `QA-REPORT.md`,
   `TESTING-CHECKS.md`, `.claude/windows-role.md` (its queue), and files under
   `docs/`. Anything else -- `src/`, `Cargo.toml`, `CHANGELOG.md`, TESTING.md,
   TESTING-KEYS.md -- and you do not merge: comment on the pull request naming
   the files, and add a line to QUESTIONS.md so the owner sees it.
2. **CI is green on its head**: `audit`, `clippy`, `smoke` and `test` all
   `success`. Still running: stop, the next run will look again. Red: read the
   log. A documentation-only pull request cannot break a build, so a red test is
   a flaky test on `main`. **Do not fix code from here** -- nobody reviews what an
   unattended run pushes to `main`. Write the failing test, the log line and your
   reading of the cause into TODO.md (commit that as in 4), comment on the pull
   request, and stop. Never merge over red.
3. **The checklists agree with their generators** on the pull request's head:
   `cargo run --example make-testcheck -- --check` and
   `cargo run --example make-keycheck -- --check`, both exit 0.
4. **Every new tick has its evidence line** in the pull request body, and none
   is an appearance row (`windows-role.md`, "Ticking TESTING-CHECKS.md"). A tick
   you cannot match to evidence: comment, do not merge.

A conflict with `main` is not a reason to stop: merge `origin/main` into the
pull request's branch with a merge commit (never rebase or force-push it),
resolve, and push. QA-REPORT.md: keep both sections whole -- restore the markers
with `git checkout --conflict=merge` and read the boundary first. TESTING-CHECKS.md:
take either side, then regenerate with `cargo run --example make-testcheck` (the
ticks survive, the counts are rewritten) and check that every tick from both
sides is still there. Then stop; the next run merges it once CI is green.

## 3. Merge

`merge_pull_request` with `merge_method: "merge"` and the **full 40-character**
head SHA as `expectedHeadSha`. Never squash, never rebase.

## 4. The merger's share, straight on `main`

One commit, pushed to `main`. It holds **Markdown, `Cargo.toml` and `Cargo.lock`
only** -- this is the one push to `main` an unattended run may make, and it is
allowed because nothing in it can break a build:

- **Version**: PATCH up in `Cargo.toml`, `cargo build` for `Cargo.lock`.
- **CHANGELOG.md**: a new section, with the pull request's changelog line in
  the file's own style (Japanese), and `（#NN）`.
- **Proposals and findings**: every item under the run's `### Proposals` and
  every bug in its QA-REPORT.md section goes somewhere -- TODO.md for what needs
  no decision, QUESTIONS.md (CLAUDE.md's format, with a recommendation) for a
  key, a default or a design choice. Merging without this is half the job.
- **The queue of the pull request's lane** in `windows-role.md`: the section just
  run must be out of the table, or cut down to what is left and why. **The run
  cannot edit the table itself** (writes under `.claude/` are refused to it), so
  apply the `## Queue` section of its pull request body here, or work it out from
  QA-REPORT.md if there is none -- otherwise the next run takes the same section.
- **The `win` queue is empty**: refill it. Read TESTING-CHECKS.md for sections with
  unticked rows that are not in the table or the "worked through" line, and add
  the ones whose rows can be read as text, a file state or a process state,
  each with how to measure it -- the way the existing rows are written. When
  nothing measurable is left, say so in TODO.md and leave the table empty; the
  Windows run then answers `WINTEST_NOTHING`.
- **The `arm` queue is empty**: refill it with rows that are native code or
  architecture-specific (ConPTY, the shell, archives, openers, `filer env`),
  re-run against their x64 result -- not with appearance rows, which look the
  same on both.
- **An ARM64 result that differs from x64** is a bug report, whatever the run
  called it: it goes to TODO.md with both results side by side.


## Never

- Merge a pull request that touches anything outside the list in 2.1.
- Tick a row yourself, or edit TESTING.md's rows or numbering.
- Push to the pull request's branch except to resolve a conflict.
- Change code, workflows or scripts. Anything that needs it goes to TODO.md.
- Cut a release, run `cargo fmt`, or force-push anything.
