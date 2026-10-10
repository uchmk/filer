# The merge routine

A claude.ai Routine starts a new cloud session once a day, at 10:01 JST (the
owner's choice, 2026-10-10), with
`uchmk/kura` attached, and its whole instruction is one line: read this file
and do what it says. This file is the one place the routine's own steps live
(CLAUDE.md, 自動実行モード); **what to do for each pull request is in
[merge-role.md](merge-role.md), and that is what you follow.** This file only
adds what a run needs around it. Where the two seem to differ, merge-role.md
wins.

**The routine no longer merges** (v0.86.6). The `Merge lanes` workflow merges
the lanes' pull requests, and since v0.86.9 also those that conflict only in
the checklists, and since v0.86.11 it also does the mechanical half of the
merger's share (the version, CHANGELOG.md, the reports' sections copied into
TODO.md with `【後】`) and starts `main`'s CI. The routine sorts those `【後】`
lines, resolves the other conflicts, and tells the owner about the ones the
workflow stopped on. The cloud session's auto mode refused the routine's merges of
#309 and #310 on 2026-10-10. Do not merge, and do not look for another way
to: a merge the workflow will not make is the owner's.

Every run starts with an empty conversation and nobody is watching: never wait
for input. Reply in Japanese; commits in English. Read [CLAUDE.md](../CLAUDE.md),
[docs/claude/lanes.md](../docs/claude/lanes.md), [docs/claude/questions.md](../docs/claude/questions.md)
and merge-role.md from `origin/main` at the start of every run.

## A run

1. `git fetch origin main && git checkout -B claude/merge-run origin/main`.
2. Do merge-role.md from 0 to 4: the votes; the open pull requests the
   workflow stopped on, and any merged one still without a share (1);
   conflicts and holds (3); the sorting of the `【後】` lines (4). GitHub from here: the GitHub tools your
   session has, or `gh api` (`gh api "repos/uchmk/kura/pulls?state=open"`,
   `gh api repos/uchmk/kura/issues/N/comments` for the workflow's
   comments; GraphQL may be unavailable).
   **First read TODO.md's "マージで止めている実機の PR" section**: each item
   there waits on the owner, and each run starts with no memory, so that
   section is the only thing that carries a hold over. An item whose answer
   is written: carry it out in this run (merge-role.md, 3).
3. **A conflict**: resolve it on the pull request's branch as merge-role.md 3
   says, in a worktree of its own (`git worktree add`, with `CARGO_TARGET_DIR`
   set to this checkout's `target`), and push the merge commit to the
   branch. Do not wait for its CI; the workflow merges it when that is green.
   The workflow does its share once it has merged it.
4. **Push what you sorted with `scripts/push-main.sh`**, in the background,
   and wait for it to finish (merge-role.md, 4). It replays your commit on top
   when the development routine pushed first, renumbers it, runs the checks,
   and waits for `main`'s CI. Never `git merge origin/main` + `git push`, never
   `--force`. Nothing to sort and nothing else to fix: no commit.
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
  `gh api repos/uchmk/kura/commits/<sha>/check-runs --jq '.check_runs[] | "\(.name) \(.conclusion)"'`.
  A red Windows `test` comes first for the development routine; queue its
  cause (the test name, and what differs on Windows if you can tell) so the
  next development run finds it.

What you find: a Markdown-only fix goes into your share's commit; anything that
needs code goes near the top of TODO.md, in the "Sonnet の見張り" section, as
`（Sonnet の見張り）…`. Keep this watch until the owner takes it out of here.

## The reply

In Japanese:

- one line per pull request: `sorted #N` (its `【後】` lines are sorted in this
  run's commit) / `shared #N` (a share the workflow missed, done by hand) /
  `resolved conflict on #N` / `held #N: why（TODO.md）` / `closed #N: …` /
  `not merged #N: why`; a held one with no answer yet:
  `held #N: 持ち主の答え待ち（TODO.md）`; nothing to do: `nothing to do`;
- the votes counted, when any (`Q57: 多数決 1`);
- `Sonnet の push: N 件、問題 M 件（中身）`.
