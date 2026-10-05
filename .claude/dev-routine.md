# The development routine

A claude.ai Routine starts a new cloud session at :33 every hour, with
`uchmk/filer` attached, and its whole instruction is one line: read this file
and do what it says. This file is the one place the routine's steps live
(CLAUDE.md, 自動実行モード). To change what the routine does, change this file;
the Routine's own text can only be edited from its own conversation, so it
stays one line.

Every run starts with an empty conversation and nobody is watching: never wait
for input. Keep the run short: at most five rounds or 50 minutes (below), then
end; the next hour's session carries on from `main`. A session that ran for a
day reached 550,000 tokens, and every call re-read all of it (2026-10-05).

**A run is up to five rounds, not one.** The first three runs of this file
(2026-10-05, 05:39, 06:33 and 07:33 UTC) each pushed one version and ended
after 8 to 18 minutes, about $0.50 a run, with some 50 items open: a run that
stops after its first push leaves four rounds unused, and a week's work then
takes a month. Pushing is not the end of a run; step 10 says what is.

Read [CLAUDE.md](../CLAUDE.md); its rules apply in full, above all
"作業ルール", "自動実行モード", "Linux 上で作業する場合", "確認事項" and
"設計の約束事". Reply in Japanese; code, comments and commits in English.
Match the surrounding code (comment density, names, the one-line layout).
**Never run `cargo fmt`.**

## Start

1. `date -u` and keep the time: the 50 minutes count from here.
2. `git status`. Uncommitted changes in a new session are not expected; if
   there are any, CLAUDE.md's first rule for leftovers applies.

## A round

1. `git fetch origin main && git checkout -B claude/todo-run origin/main`.
2. Read CLAUDE.md again, every round: the merge routine edits it alongside you.
3. **A red `main` comes first.** Read the Windows `test` job of `main`'s newest
   commit:

   ```bash
   gh api "repos/uchmk/filer/commits/$(git rev-parse origin/main)/check-runs" \
       --jq '.check_runs[] | select(.name == "test") | "\(.status) \(.conclusion)"'
   ```

   `completed failure`: this round fixes that, before any new item. Nothing
   printed (the commit only touched files CI skips) or still running: use
   `scripts/push-main.sh --status`, which reports the newest finished run job
   by job and ends with `RED` and exit code 2 when it failed. The job log is
   not readable from here; the merge routine usually queues the cause under
   "Sonnet の見張り" in TODO.md, so read that first. Windows-only failures
   come from tests that are not `#[cfg(windows)]` yet behave differently there
   (path separators, most often), which `scripts/verify.sh` on Linux cannot see.
4. Otherwise take, in this order:
   - a QUESTIONS.md question that is `回答済み` or `多数決で決定` and not yet
     carried out;
   - from TODO.md, items with none of `【人】` `【QA】` `【実機】` `【後】`
     `要確認` (`scripts/todo-open.sh -v` lists them, top first): one section's
     open items, or up to three small items. A large item is cut into steps,
     written into TODO.md as sub-items, and only the first step done.
5. **An item you cannot take gets a mark, not a silent skip**, so the next run
   does not stop at it again. A key or a default: a `投票中` question in
   QUESTIONS.md (CLAUDE.md's format, your own vote with a reason, no
   recommendation) and `（要確認: Qn）` on the item. Looks, a new crate, a
   release, a workflow or security: an `未回答` question with a recommendation,
   and `（要確認: Qn）`. Only a Windows machine can measure it: `【実機】`. Only
   the owner can do it: `【人】`. Add a few words saying why. A round that only
   adds marks is still a round: commit and push it.
6. Do the work. When behaviour changes (CLAUDE.md, 作業ルール): check it on
   the virtual display (`scripts/xrun.sh`, `--keys`, `<State:name>`); add or fix
   the TESTING.md row and its Japanese in `scripts/testcheck-ja.toml`, then
   regenerate (`cargo run --example make-testcheck` and `-- --lane linux`);
   untick the changed rows in the checklists; queue them under "Re-tests of
   changed behaviour" in both tables of `.claude/windows-role.md` (x64 and
   ARM64; not `linux-role.md`, whose lane is paused). A fix that came from a
   machine's finding goes to the re-tests even when its row has no tick.
7. `scripts/verify.sh`. Everything passes, or you fix it.
8. Tick the TODO.md item as `- [x] （vX.Y.Z。what was done）`; bump
   `Cargo.toml` (PATCH, MINOR for a feature) and run `cargo build` for
   `Cargo.lock`; add a CHANGELOG.md section in Japanese, dated in JST
   (`TZ=Asia/Tokyo date +%F`); commit in English, subject `vX.Y.Z: …`, whose
   first paragraph reads on its own (it becomes the release notes), ending with
   the attribution lines your session gives you.
9. Push with `scripts/push-main.sh` only: never `git merge origin/main`, never
   `--force`. It takes up to about 13 minutes (the checks, then waiting for
   `main`'s CI), so run it in the background and wait for it to finish before
   anything else; never two at once. When `main` moved, it puts your commit on
   top and renumbers it. When it stops at a conflict, do what it says
   (`git rebase origin/main`, resolve, fix the version, the CHANGELOG heading
   and the subject) and run it again. When it says the checks changed files
   (a `Cargo.lock` left out), `git commit --amend` them in and run it again.
10. One line: `vX.Y.Z を push（SHA）: what was done`. Then run `date -u` and
    write one more line: `round N/5, M min since Start: next round` -- or, only
    when "When to end" below says so, `…: ending (why)`. No summary between
    rounds, and do not end the turn between rounds: in the same turn, go
    straight back to 1. A reply that reports a push and stops there is the
    mistake this file exists to prevent.

## When to end

- **After the fifth round, or when 50 minutes have passed since Start**, do not
  begin another round. A round already under way finishes its push first.
- **`ALL_DONE` only when `scripts/todo-open.sh` prints `0`** and no answered
  question is waiting to be carried out. Never on your own reading of what is
  left: on 2026-10-05 a session stopped on "the rest is mostly for the machine
  or the eye" with 70 such items open. When an item cannot be taken, step 5
  marks it, and the count goes down.
- Stop early only when `scripts/push-main.sh` stops at a conflict you cannot
  resolve, a push is refused (authentication), or the checks still fail after
  starting the round again from 1. Checks you cannot fix: `git restore` /
  `git clean`, and write why into TODO.md (that alone may be committed and
  pushed).

## The report

At the end, in Japanese, three to five lines:

- each version pushed in this run with its SHA, and what it fixed or added;
- why the run ended (five rounds, 50 minutes, `ALL_DONE`, or what stopped it);
- `未回答の確認事項 N 件（QUESTIONS.md）`, counting `未回答` and `投票中`.

With `ALL_DONE`, the last line is `ALL_DONE` alone.
