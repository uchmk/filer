# The merge routine

A claude.ai Routine starts a new cloud session at :59 every hour, with
`uchmk/filer` attached, and its whole instruction is one line: read this file
and do what it says. This file is the one place the routine's own steps live
(CLAUDE.md, 自動実行モード); **what to merge and how is in
[merge-role.md](merge-role.md), and that is what you follow.** This file only
adds what a run needs around it. Where the two seem to differ, merge-role.md
wins.

Every run starts with an empty conversation and nobody is watching: never wait
for input. Reply in Japanese; commits in English. Read [CLAUDE.md](../CLAUDE.md)
and merge-role.md from `origin/main` at the start of every run.

## A run

1. `git fetch origin main && git checkout -B claude/merge-run origin/main`.
2. Do merge-role.md from 0 to 4: the votes, the open pull requests from
   `test/win-*`, `test/arm-*` and `test/linux-*` (oldest first), each one that
   may be merged, and the merger's share. GitHub from here: the GitHub tools
   your session has, or `gh` (`gh api "repos/uchmk/filer/pulls?state=open"`;
   merge with `gh api -X PUT repos/uchmk/filer/pulls/N/merge -f merge_method=merge
   -f sha=<the full head SHA>`).
   **First read TODO.md's "マージで止めている実機の PR" section: a pull request
   listed there was held by an earlier run and waits on the owner. Do not merge
   it, whatever its CI says, until the owner's answer is written there.** Each
   run starts with no memory, so that section is the only thing that carries a
   hold over; #269 was merged on 2026-10-05 by a run that did not read it, and
   1.41 went in ticked for a half nobody had pressed. GitHub names `uchmk` as
   the merger of every pull request, the routines' merges included, so
   `merged_by` never tells you the owner merged one: only the owner's own
   words (in TODO.md, QUESTIONS.md or the pull request) do.
   **Once the answer is there, carry it out in this run** (the owner asked for
   this on 2026-10-05: #273 sat a run longer, and the owner had to ask an
   interactive session to merge it). The answer is a line
   `- 持ち主の答え: …` under the item. One that an interactive session wrote
   (`対話のセッションで`) counts: the owner talks to Claude only there, and
   that session wrote down the owner's words. Read `main`'s TODO.md again
   right before you decide, since the answer may have landed after step 1.
   Then:
   - **Merge it** (the answer keeps some ticks and takes `main`'s side for
     others): when the pull request conflicts with `main`, merge `origin/main`
     into its branch with a merge commit, take `main`'s line for each row the
     answer names, keep the pull request's for the rest, regenerate with
     `cargo run --example make-testcheck` and run the three `--check`s of
     merge-role.md 2.3; push to the pull request's branch. When `main`
     already carries a tick the answer keeps (the other lane ticked it
     first), there is nothing to resolve for it. Then merge-role.md 2.2
     (wait for CI on the new head; subscribe) and 3, as for any pull request.
   - **Close it** (the answer says the next run presses those rows again):
     close the pull request with a one-line comment naming the answer, and
     check that its rows are still in that lane's "Re-tests of changed
     behaviour".
   - In the merger's share: take the item out of the section (leave
     `いまは無い` when the section is empty), and in the lane's queue say
     which rows the pull request settled and which stay open.
   An answer you cannot carry out as written (it names rows the pull request
   does not touch, or the merge leaves a conflict outside the rows it names):
   do not guess. Write under the item what stopped you, and reply
   `not merged #N: …`.
   **When you hold a pull request yourself**, write the item so the next run
   can carry out either answer without you: the rows on each side, what each
   choice would do to them, and the recommended one.
3. **Push the merger's share with `scripts/push-main.sh`**, in the background,
   and wait for it to finish (merge-role.md, 4). It replays your commit on top
   when the development routine pushed first, renumbers it, runs the checks,
   and waits for `main`'s CI. Never `git merge origin/main` + `git push`, never
   `--force`.
4. **A pull request whose CI is still running**: subscribe to it as
   merge-role.md says (2.2). This session stays after the run and is woken
   when the checks finish; do that pull request's merge and share then. The
   next hour's run is a different session and may meet the same pull request:
   one that is already merged is passed over, and the full head SHA on the
   merge keeps two sessions from merging two different heads.
5. Then the development routine's pushes (below).

## Watching the development routine's pushes

The development routine (`.claude/dev-routine.md`, Sonnet) pushes straight to
`main`. Every run, read each `vX.Y.Z:` commit that reached `main` since your
last run and is not a merger's share (`git log` since the previous merger's
share, `Co-Authored-By` naming Sonnet), and check:

- CLAUDE.md's rules held: no `cargo fmt` reformatting of files the change did
  not need; the version and CHANGELOG.md together; a behaviour change came with
  its TESTING.md row, its `scripts/testcheck-ja.toml` text, the untick, and the
  re-tests in both tables of `windows-role.md`; no default the owner decided
  was changed; no item marked `【人】` `【QA】` `【実機】` `【後】` `【pane】` or `要確認`
  was taken; an item it could not take was marked, not skipped.
- its CI is green:
  `gh api repos/uchmk/filer/commits/<sha>/check-runs --jq '.check_runs[] | "\(.name) \(.conclusion)"'`.
  A red Windows `test` comes first for the development routine; queue its
  cause (the test name, and what differs on Windows if you can tell) so the
  next development run finds it.

What you find: a Markdown-only fix goes into your share's commit; anything that
needs code goes near the top of TODO.md, in the "Sonnet の見張り" section, as
`（Sonnet の見張り）…`. Keep this watch until the owner takes it out of here.

## The reply

In Japanese:

- one line per pull request: `merged #N` / `waiting on CI for #N (subscribed)` /
  `not merged #N: why` (a held one with no answer yet: `not merged #N: 持ち主の答え待ち（TODO.md）`); nothing open: `nothing to do`;
- the votes counted, when any (`Q57: 多数決 1`);
- `Sonnet の push: N 件、問題 M 件（中身）`.
