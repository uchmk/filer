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
  `- 多数決: <option>（<today> に揃った）`, and in the same run set `状態` to
  `多数決で決定` and take `（要確認: Qn）` off its TODO.md task -- no waiting
  (the owner's word, 2026-10-03; there used to be 24 hours for the owner to
  answer first). An answer the owner writes later still wins. You do not
  implement it -- that is code, and the development session picks it up.
- **A lane votes again on a question it already voted on**: the newer vote
  replaces the older; copy it with the pull request's number and count again.
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
- **Every one that may be merged, oldest first** (since v0.73.15). Take them one
  at a time through 2 and 3, fetching `origin/main` again before each, since the
  one before moved it. A pull request that cannot be merged yet (CI running, a
  conflict to stop on, a tick without evidence) is passed over, not waited on:
  go on to the next. With three lanes and one merge an hour, pull requests
  queued behind each other while their lanes sat idle; since the reports and
  the checklists stopped conflicting (v0.73.14), two in one run no longer get in
  each other's way.

## 2. Decide whether it may be merged

All of these, or it is not merged:

1. **It only touches what the Windows session may write**: **one new file under
   `qa-reports/`** (its report; added, not edited -- another run's file is not
   its to change), `TESTING-CHECKS.md` (every changed line is a `[ ]` turned `[x]` or `[~]`, or a
   `[~]` turned `[x]` -- the last only by the owner, never by a run),
   `TESTING-KEYS.md` (ticks only: every changed line is a `[ ]` turned `[x]`), `.claude/windows-role.md` (its queue), and files under
   `docs/`. Anything else -- `src/`, `Cargo.toml`, `CHANGELOG.md`, TESTING.md, or
   any other change to TESTING-KEYS.md -- and you do not merge: comment on the pull request naming
   the files, and add a line to QUESTIONS.md so the owner sees it.
   **A `test/linux-*` pull request** may touch only its one new `qa-reports/`
   file, `TESTING-LINUX.md` and files under `docs/`.
   **Until 2026-10-03 every run appended its report to QA-REPORT.md instead.** A
   pull request opened before then (#198, #199) may still do that, adding at
   the end and changing nothing above it; one opened after it may not. A Linux run that changed
   TESTING-CHECKS.md or TESTING-KEYS.md claimed a Windows result: do not merge.
2. **CI is green on its head**: every check run on it is `success`. Which ones
   run depends on the files: a pull request that changes only the checklists,
   `qa-reports/` and other files in `ci.yml`'s `paths-ignore` runs `audit` and
   `checklists` (v0.73.15) and nothing else, by design -- those two green is
   green. Anything else runs `audit`, `clippy`, `smoke`, `test` and `test-linux` as well.
   **Still running: subscribe to the pull request** (`subscribe_pr_activity`,
   since v0.73.21) and go on to the next one. When its checks finish, the
   session is woken with the result: green, take it through 2 and 3 and do its
   share in 4 then and there; red, as below. Without this a pull request whose
   CI was still running waited for the next run, an hour, though its CI took
   minutes. **Unsubscribe** (`unsubscribe_pr_activity`) once it is merged, or
   once you stop on it for any other reason -- a subscription left behind wakes
   the session for nothing. A wake for a pull request that is already merged or
   closed: unsubscribe and stop. Red: read the
   log. A documentation-only pull request cannot break a build, so a red test is
   a flaky test on `main`. **Do not fix code from here** -- nobody reviews what an
   unattended run pushes to `main`. Write the failing test, the log line and your
   reading of the cause into TODO.md (commit that as in 4), comment on the pull
   request, and stop. Never merge over red.
3. **The checklists agree with their generators** on the pull request's head:
   `cargo run --example make-testcheck -- --check`,
   `cargo run --example make-testcheck -- --lane linux --check` and
   `cargo run --example make-keycheck -- --check`, all exit 0. Since v0.73.14 the
   checklists hold no counts (they come from `-- --stats`), so a pull request
   that only ticks changes only its tick lines. One made before that still
   edits the count lines and the section headings: that is the conflict below,
   and once it is resolved the check passes.
4. **Every new tick has its evidence line** in the pull request body, and none
   is an appearance row (`windows-role.md`, "Ticking TESTING-CHECKS.md"). In
   TESTING-LINUX.md every `[x]` needs its evidence line and every `[-]` its
   reason (`linux-role.md`). A tick
   in TESTING-KEYS.md needs both halves on its line: what the key changed, and
   the before/after snapshot of what it did not (`windows-role.md`,
   "TESTING-KEYS.md"). A `[~]` (an appearance row judged from a screenshot)
   needs the picture's path, what was seen, and the failure that was looked for
   and not found, and its row must be one that cannot be measured
   (`windows-role.md`, "Ticking TESTING-CHECKS.md"). A tick or a `[~]`
   you cannot match to evidence: comment, do not merge.

A conflict with `main` is not a reason to stop: merge `origin/main` into the
pull request's branch with a merge commit (never rebase or force-push it),
resolve, and push. A pull request that follows the current roles conflicts
only where two runs ticked the same row, which should not happen. The older
ones conflict in two places:

- **QA-REPORT.md** (a report appended there, before 2026-10-03): keep both
  sections whole -- restore the markers with `git checkout --conflict=merge` and
  read the boundary first. `main`'s side first, then the pull request's.
- **TESTING-CHECKS.md** (or TESTING-LINUX.md, with `-- --lane linux`): take
  `main`'s side, put the pull request's ticks back on it, regenerate with
  `cargo run --example make-testcheck`, and check that every tick from both
  sides is still there.

Then, which of two:

- **Only the checklists and QA-REPORT.md conflicted**, every QA-REPORT.md
  conflict was two whole sections meeting at the end (no marker inside a
  section, nothing above the pull request's section changed), and after
  resolving, the pull request's diff against `main` names the same files, the
  same tick lines and the same added section as before: **wait for CI on the
  new head in this run** -- subscribe to it as in 2.2 rather than polling -- and
  once all of 2.2 is green, go on to 3 and merge it now. Such a conflict is two
  appends meeting, or a count line `main` moved; making it wait an hour each
  time held #197 back three runs on 2026-10-03, and the lane with it. Red:
  stop, as below.
- **Anything else conflicted** (a marker inside a section, a tick line both
  sides changed, any other file): stop; the next run merges it once CI is green.

## 3. Merge

`merge_pull_request` with `merge_method: "merge"` and the **full 40-character**
head SHA as `expectedHeadSha`. Never squash, never rebase. Then back to 2 for
the next pull request in the list.

## 4. The merger's share, straight on `main`

One commit for the run, however many were merged, pushed to `main` after the
last merge (CLAUDE.md: one version per push). Each pull request gets its own
CHANGELOG line, proposals and queue edit inside it. It holds **Markdown, `Cargo.toml` and `Cargo.lock`
only** -- this is the one push to `main` an unattended run may make, and it is
allowed because nothing in it can break a build. **Push it with
`scripts/push-main.sh`, never `git merge origin/main` + `git push`** (v0.78.38):
when the development routine pushed first, it puts your commit on top of
theirs and gives it the next version -- in the subject, the CHANGELOG heading
and the lines you added -- then runs `scripts/verify.sh`, waits while main's CI
is still running (a push would cancel it), and pushes. Renumbering
in a merge commit left three commits titled `v0.78.31:` on 2026-10-05, and the
release notes are built from those titles. If it stops at a conflict, do what
it says (`git rebase origin/main`: the commit was never pushed) and run it again.

What goes in it:

- **Version**: PATCH up in `Cargo.toml`, `cargo build` for `Cargo.lock`.
- **CHANGELOG.md**: a new section, with each merged pull request's changelog line
  in the file's own style (Japanese), and its `（#NN）`.
- **Proposals and findings**: every item under the run's `### Proposals` and
  every bug in its report (its `qa-reports/` file, or its QA-REPORT.md section
  for a pull request from before 2026-10-03) goes somewhere -- TODO.md for what needs
  no decision (ending the line with `【QA】` for a TESTING.md wording change and
  `【実機】` for what only a Windows machine can measure, so the development
  session's "next item" passes over them; CLAUDE.md, 作業ルール), QUESTIONS.md (CLAUDE.md's format, with a recommendation) for a
  key, a default or a design choice. Merging without this is half the job.
  A new question that has an arguable technical answer goes out as `投票中`,
  with your own vote and no "（推奨）" (CLAUDE.md, "多数決で進める質問").
- **Votes**: copy each line under the run's `### Votes` into that question's
  `投票` field as `- win: …` or `- arm: …` with `（#NN）`, then count as in 0.
- **The queue of the pull request's lane** in `windows-role.md`: the section just
  run must be out of the table, or cut down to what is left and why. **The run
  cannot edit the table itself** (writes under `.claude/` are refused to it), so
  apply the `## Queue` section of its pull request body here, or work it out from
  its report if there is none -- otherwise the next run takes the same section.
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
- **New x64 ticks go to ARM64's last row** (#260 proposal 4): every row a merged
  `win` pull request turned `[x]` that ARM64 has not pressed is added to (1) of the
  ARM64 table's "When every row above is empty" row, and the rows an `arm` run
  pressed as a second machine come out of it. Without this the lane idles on
  `cargo test` alone once the rest of its table is empty.
- **An ARM64 result that differs from x64** is a bug report, whatever the run
  called it: it goes to TODO.md with both results side by side. So is **a Linux
  result that differs from Windows**.
- **The `linux` queue** (`linux-role.md`) -- **paused since 2026-10-04**: a `test/linux-*` pull request that is still open is merged as before, but do not refill the queue. Otherwise: apply the pull request's `## Queue`
  section. When it is empty, refill it from TESTING-LINUX.md with sections whose
  open rows apply on Linux and read as text or a file state.


## Never

- Merge a pull request that touches anything outside the list in 2.1.
- Tick a row yourself, or edit TESTING.md's rows or numbering.
- Vote for a lane, or count a question that is not `投票中`.
- Push to the pull request's branch except to resolve a conflict.
- Change code, workflows or scripts. Anything that needs it goes to TODO.md.
- Cut a release, run `cargo fmt`, or force-push anything.
