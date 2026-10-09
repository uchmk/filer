# The merger's share for the machines' pull requests

A scheduled cloud session reads this and does what a person used to do around
a pull request from a machine's session (`.claude/windows-role.md`): check
what the run found, and do the merger's share. **It does not merge.** Since
v0.86.6 the `Merge lanes` workflow (`.github/workflows/merge-lanes.yml`,
`scripts/merge-lanes.py`) merges each lane pull request itself, with a merge
commit pinned to its head, once it keeps to the rules in 2 and its checks are
green. The routine used to merge, and the cloud session's auto mode refused
the merge as one nobody had reviewed (#309 and #310, 2026-10-10); a workflow
whose rules are code does not need a reviewer for each one.

A lane's next run (`scripts/auto-wintest.ps1`) waits while one of its pull
requests is open, and also while its latest merged one is not named in
`main`'s CHANGELOG.md. **So the share is what lets the lane go on**: a merged
pull request left without it stops the loop as surely as an open one did.

There are three lanes: `test/win-*` from the x64 machine, `test/arm-*` from the
ARM64 laptop, and `test/linux-*` from a cloud session running filer on a
virtual X display (`.claude/linux-role.md`). The two Windows lanes have their
queues in `windows-role.md` -- "Where the work is" for `win`, "The ARM64 lane"
for `arm`; the Linux lane's is in `linux-role.md`, "Where the work is".

Read [CLAUDE.md](../CLAUDE.md) first, with [docs/claude/lanes.md](../docs/claude/lanes.md) and
[docs/claude/questions.md](../docs/claude/questions.md); their rules apply in full. Reply in Japanese;
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

- **Merged, without a share**: the lane pull requests (head branch
  `test/win-*`, `test/arm-*` or `test/linux-*`) merged in the last 7 days
  whose `#N` is nowhere in `origin/main`'s CHANGELOG.md
  (`gh api "repos/uchmk/filer/pulls?state=closed&sort=updated&direction=desc&per_page=50"`,
  keep those with `merged_at` set). Each one gets its share in 4. Every
  merged lane pull request's `（#N）` must end up in CHANGELOG.md: that is how
  the next run, and the lane's script, know it was done.
- **Open, and stopped**: the open lane pull requests the workflow commented
  on. Its comments end in a hidden marker `<!-- merge-lanes:<kind>:<sha> -->`,
  one per head and kind: `rules` (it breaks 2), `red` (a required check
  failed), `conflict` (it conflicts with `main`). One whose marker names an
  older head than the pull request's is stale; look at the pull request as it
  is now. These go through 3.
- Open lane pull requests with no such comment are the workflow's: their
  checks are running, or it will merge them on its next pass (it runs when a
  check workflow finishes, and at :17 every hour). Leave them.

Nothing in either list: stop here and say so in one line (after the votes in
0). That is most runs.

## 2. What the workflow merges

`scripts/merge-lanes.py` holds the rules; this is what they say, so that you
can tell the owner why a pull request stopped. All of these, or it is not
merged:

1. **It only touches what the run may write.** `test/win-*` and `test/arm-*`:
   **exactly one new file under `qa-reports/`** (its report; added, not edited),
   `TESTING-CHECKS.md` (every changed line is a `[ ]` turned `[x]` or `[~]`, the
   row's text unchanged) and `TESTING-KEYS.md` (every changed line a `[ ]`
   turned `[x]`). `test/linux-*`: one new `qa-reports/` file and
   `TESTING-LINUX.md` (`[ ]` to `[x]` or `[-]`). Anything else -- `.claude/`,
   `docs/`, `src/`, `CHANGELOG.md`, TESTING.md, a heading, a count line -- is
   not merged. (Until v0.86.5 the routine also merged a run's edits to
   `windows-role.md` and `docs/`; a run cannot write under `.claude/` and
   applies its queue through the `## Queue` section of its body instead.)
2. **Every new mark has its evidence line in the pull request body**: a line
   that names the row (`**49.8**`, `49.8`, a range like `49.7-49.9`, or for
   TESTING-KEYS.md the key in backticks) and says something beyond the name.
   A `[~]` (an appearance row judged from a screenshot) also needs the
   picture's file name on that line. What the line must say beyond that
   (`windows-role.md`, "Ticking TESTING-CHECKS.md" and "TESTING-KEYS.md") is
   yours to read in 4; the workflow checks only that it is there.
3. **The required checks are green on its head** (`checklists`, which runs
   the three `--check`s; any other check that ran must not have failed).
4. **It does not conflict with `main`**, and it is not listed under TODO.md's
   "マージで止めている実機の PR".

## 3. A pull request the workflow stopped on

- **`conflict`**: since v0.86.9 the workflow resolves a conflict that is only
  in the checklists itself (`main`'s file with the pull request's marks made
  again on the rows `main` still has unticked and unchanged, checked to be
  marks only, pushed to `main` as the merge commit). A mark whose row `main`
  reworded is dropped and named in a comment
  (`<!-- merge-lanes:dropped:<sha> -->`): say so in the share as below. So a
  `conflict` comment means another file conflicted (usually two reports with
  one name). Merge `origin/main` into the pull request's branch with a
  merge commit (never rebase or force-push it), resolve, and push to the
  branch. The workflow merges it once CI is green on the new head; do not
  wait for that. The resolution:
  - **TESTING-CHECKS.md** (or TESTING-LINUX.md, with `-- --lane linux`, or
    TESTING-KEYS.md with `make-keycheck`): take `main`'s side, put the pull
    request's ticks back on it, regenerate with
    `cargo run --example make-testcheck`, and run the three `--check`s
    (`cargo run --example make-testcheck -- --check`,
    `cargo run --example make-testcheck -- --lane linux --check`,
    `cargo run --example make-keycheck -- --check`).
  - **A tick line both sides changed** (the owner's word, 2026-10-05: settle
    it here, do not ask). `main` rewrote the row after the run (a fix changed
    its expectation, and unticked it): **take `main`'s line, unticked**; the
    run checked the old expectation, so its tick does not stand for the new
    one, and the row stays in the lane's "Re-tests of changed behaviour". Say
    so in the share (`#N settled A and B; C was rewritten by vX.Y.Z after the
    run, so it stays open`). The same text on both sides, both ticked: take
    either; the tick stands.
  - **Anything else conflicted**: do not guess. Hold it (below).
- **`rules`**: the workflow will not merge it as it is, and you do not change
  a run's ticks or its report. Hold it (below), naming each problem the
  comment lists, what each choice would do, and the recommended one (merge
  as it is, or close and let the next run press those rows again).
- **`red`**: read the log. A pull request of Markdown cannot break a build, so
  a red check is a flaky test or a broken `main`. **Do not fix code from
  here** -- nobody reviews what an unattended run pushes to `main`. Write the
  failing check, the log line and your reading of the cause into TODO.md
  (commit that as in 4). A `checklists` failure on a pull request that only
  ticks is a mark the generator does not accept: hold it as for `rules`.

**Holding** is a line under TODO.md's "マージで止めている実機の PR" (take out
`いまは無い`): the pull request's number as `#N`, why it stopped, the choices
and the recommended one, written so the next run can carry out either answer
without you. The workflow passes over every `#N` listed there. Commit it as in
4. The owner answers with a line `- 持ち主の答え: …` under the item; one an
interactive session wrote (`対話のセッションで`) counts, since the owner
talks to Claude only there. GitHub names `uchmk` as the merger of every pull
request, the workflow's merges included, so `merged_by` never tells you the
owner merged one: only the owner's own words do. **Once the answer is there,
carry it out in this run** (read `main`'s TODO.md again right before you
decide):

- **Merge it**: resolve what the answer settles on the pull request's branch
  as above, push, and take the item out of the section, in this run's share.
  The workflow then merges it when its checks are green. When it stopped on
  `rules` and the answer keeps it as it is, the workflow will still refuse
  it: say so under the item and reply `not merged #N: the workflow's rules
  (…); the owner merges it by hand`.
- **Close it**: close the pull request with a one-line comment naming the
  answer, check that its rows are still in that lane's "Re-tests of changed
  behaviour", and take the item out.
- An answer you cannot carry out as written: write under the item what
  stopped you, and reply `not merged #N: …`.

## 4. The merger's share, straight on `main`

One commit for the run, for every pull request found in 1 without its share
(CLAUDE.md: one version per push). Each pull request gets its own
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
- **The evidence, read as a merger would**: the workflow checked only that
  each mark has a line naming its row. Read those lines against
  `windows-role.md` ("Ticking TESTING-CHECKS.md", "TESTING-KEYS.md"). A mark
  whose line does not hold up (no before/after for a key, a `[~]` on a row
  that can be measured): untick it in this commit, put the row back in the
  lane's "Re-tests of changed behaviour", and say why in the CHANGELOG line.
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
- **A queue row that waits on a TODO.md item** ("skip this until ... lands"): when
  that item is ticked, edit the row in the same share so it no longer reads as
  blocked (#275 proposal 3: ARM64's (2) still said "skip" 56 versions after the
  panic hook landed).
- **An ARM64 result that differs from x64** is a bug report, whatever the run
  called it: it goes to TODO.md with both results side by side. So is **a Linux
  result that differs from Windows**.
- **The `linux` queue** (`linux-role.md`) -- **paused since 2026-10-04**: a `test/linux-*` pull request that is still open is merged as before, but do not refill the queue. Otherwise: apply the pull request's `## Queue`
  section. When it is empty, refill it from TESTING-LINUX.md with sections whose
  open rows apply on Linux and read as text or a file state.


## Never

- Merge a pull request, or ask anything else to: that is the workflow's.
- Tick a row yourself, or edit TESTING.md's rows or numbering.
- Vote for a lane, or count a question that is not `投票中`.
- Push to the pull request's branch except to resolve a conflict (3).
- Change code, workflows or scripts. Anything that needs it goes to TODO.md.
- Cut a release, run `cargo fmt`, or force-push anything.
