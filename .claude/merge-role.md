# Merging the Windows machine's pull requests

A scheduled cloud session reads this and does what a person used to ask for by
hand: take a pull request from a Windows machine's session
(`.claude/windows-role.md`), check it, merge it, and do the merger's share. The
merge is also what starts that machine's next run (`scripts/auto-wintest.ps1`
waits while one of its lane's is open), so a pull request left sitting stops
the loop.

There are three lanes: `test/win-*` from the x64 machine, `test/arm-*` from the
ARM64 laptop, and `test/linux-*` from a cloud session running filer on a
virtual X display (`.claude/linux-role.md`). The two Windows lanes have their
queues in `windows-role.md` -- "Where the work is" for `win`, "The ARM64 lane"
for `arm`; the Linux lane's is in `linux-role.md`, "Where the work is".

Read [CLAUDE.md](../CLAUDE.md) first; its rules apply in full. Reply in Japanese;
code, comments and commits in English. Nobody is watching: never wait for input.

## 0. Count the votes (every run, before anything else)

CLAUDE.md, "多数決で進める質問". For each question in QUESTIONS.md on `main`
whose `状態` is `投票中`:

- **Two votes on the same option, no `多数決` line yet**: add
  `- 多数決: <option>（<today> に揃った。<today + 1 day> から進めてよい）`.
- **A `多数決` line whose date has come, and the `回答` field still empty**: set
  `状態` to `多数決で決定` and take `（要確認: Qn）` off its TODO.md task. You
  do not implement it -- that is code, and the development session picks it up.
- **Three votes, all different**: set `状態` back to `未回答` and add
  `- 多数決: 割れた（<today>）。持ち主を待つ`.
- **The owner wrote an answer**: that wins; the question leaves the vote.
- An `owner` vote counts for no option, and so does a Windows vote whose reason
  does not rest on something seen on the machine (copy it with `（数えない: 理由が実機に無い）`).

Changes here are Markdown: commit them as in 4 (with the merger's share when a
pull request is merged in this run, alone otherwise) and push to `main`. The
reply names what changed (`Q57: 多数決 1`).

## 1. Find the work

- List open pull requests whose head branch starts with `test/win-`,
  `test/arm-` or `test/linux-`. None: stop here and say so in one line (after
  the votes in 0). That is most runs.
- **One per run, oldest first.** The next run takes the next one; two merged in
  one run conflict with each other at the end of QA-REPORT.md.

## 2. Decide whether it may be merged

All of these, or it is not merged:

1. **It only touches what the Windows session may write**: `QA-REPORT.md`,
   `TESTING-CHECKS.md`, `TESTING-KEYS.md` (ticks only: every changed line is a
   `[ ]` turned `[x]`), `.claude/windows-role.md` (its queue), and files under
   `docs/`. Anything else -- `src/`, `Cargo.toml`, `CHANGELOG.md`, TESTING.md, or
   any other change to TESTING-KEYS.md -- and you do not merge: comment on the pull request naming
   the files, and add a line to QUESTIONS.md so the owner sees it.
   **A `test/linux-*` pull request** may touch only `QA-REPORT.md`,
   `TESTING-LINUX.md` and files under `docs/`. A Linux run that changed
   TESTING-CHECKS.md or TESTING-KEYS.md claimed a Windows result: do not merge.
2. **CI is green on its head**: `audit`, `clippy`, `smoke` and `test` all
   `success`. A pull request that only changes files in `ci.yml`'s
   `paths-ignore` (QA-REPORT.md, `.claude/**`, ...) runs `audit` alone, by
   design: that is green. Still running: stop, the next run will look again. Red: read the
   log. A documentation-only pull request cannot break a build, so a red test is
   a flaky test on `main`. **Do not fix code from here** -- nobody reviews what an
   unattended run pushes to `main`. Write the failing test, the log line and your
   reading of the cause into TODO.md (commit that as in 4), comment on the pull
   request, and stop. Never merge over red.
3. **The checklists agree with their generators** on the pull request's head:
   `cargo run --example make-testcheck -- --check`,
   `cargo run --example make-testcheck -- --lane linux --check` and
   `cargo run --example make-keycheck -- --check`, all exit 0. One exception:
   when `make-testcheck --check` says *"The checks all match; the difference is
   in the surrounding text"*, the ticks are right and only a count is stale --
   merge, then regenerate on `main` as part of 4 (CI's clippy job runs the same
   check, so it is red for this reason too; that red is not a reason to wait).
4. **Every new tick has its evidence line** in the pull request body, and none
   is an appearance row (`windows-role.md`, "Ticking TESTING-CHECKS.md"). In
   TESTING-LINUX.md every `[x]` needs its evidence line and every `[-]` its
   reason (`linux-role.md`). A tick
   in TESTING-KEYS.md needs both halves on its line: what the key changed, and
   the before/after snapshot of what it did not (`windows-role.md`,
   "TESTING-KEYS.md"). A tick
   you cannot match to evidence: comment, do not merge.

A conflict with `main` is not a reason to stop: merge `origin/main` into the
pull request's branch with a merge commit (never rebase or force-push it),
resolve, and push. QA-REPORT.md: keep both sections whole -- restore the markers
with `git checkout --conflict=merge` and read the boundary first. TESTING-CHECKS.md (or TESTING-LINUX.md, with `-- --lane linux`):
take either side, then regenerate with `cargo run --example make-testcheck` (the
ticks survive, the counts are rewritten) and check that every tick from both
sides is still there.

Then, which of two:

- **Only the generated checklists conflicted** (TESTING-CHECKS.md and/or
  TESTING-LINUX.md, nothing else), and after resolving, the pull request's diff
  against `main` names the same files and the same tick lines as before: **wait
  for CI on the new head in this run** -- look every few minutes, for up to 30
  minutes -- and once all of 2.2 is green, go on to 3 and merge it now. Such a
  conflict is only a count line that `main` moved; making it wait an hour each
  time held one pull request back three times over on 2026-10-02. Red, or not
  finished within the 30 minutes: stop, as below.
- **Anything else conflicted** (QA-REPORT.md included, whose boundary you had to
  read): stop; the next run merges it once CI is green.

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
  A new question that has an arguable technical answer goes out as `投票中`,
  with your own vote and no "（推奨）" (CLAUDE.md, "多数決で進める質問").
- **Votes**: copy each line under the run's `### Votes` into that question's
  `投票` field as `- win: …` or `- arm: …` with `（#NN）`, then count as in 0.
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
  called it: it goes to TODO.md with both results side by side. So is **a Linux
  result that differs from Windows**.
- **The `linux` queue** (`linux-role.md`): apply the pull request's `## Queue`
  section. When it is empty, refill it from TESTING-LINUX.md with sections whose
  open rows apply on Linux and read as text or a file state.


## Never

- Merge a pull request that touches anything outside the list in 2.1.
- Tick a row yourself, or edit TESTING.md's rows or numbering.
- Vote for a lane, or count a question that is not `投票中`.
- Push to the pull request's branch except to resolve a conflict.
- Change code, workflows or scripts. Anything that needs it goes to TODO.md.
- Cut a release, run `cargo fmt`, or force-push anything.
