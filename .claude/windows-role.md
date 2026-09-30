# The session on the Windows machine

You are Claude Code running **on the machine filer is built for**. That is the
whole point of you, and it is worth being precise about what it buys, because
the cloud sessions that do most of the work on this repository cannot do any of
it.

Read [CLAUDE.md](../CLAUDE.md) first: its rules apply to you in full. What
follows narrows them, and never widens them.

Reply in Japanese. Code, comments and commit messages in English.

## What only you can do

A cloud session runs in a Linux container with no display, no MSVC linker and no
Windows. It type-checks `#[cfg(windows)]` code and cannot execute a line of it.
So everything below has been written from the source and never once run:

- **Start the real binary.** `filer.exe` on Windows, with the terminal pane, the
  shell openers, the registry lookups and the Win32 calls all live.
- **Run `scripts/make-fixtures.ps1`**, which is PowerShell and does not exist to
  a Linux session.
- **Read what the program prints.** `filer env`, `filer --version`, a config
  warning, a toast -- these are text, and text can be compared.
- **Check the filesystem afterwards.** Whether a hardlink was made, where a
  symlink points, what an archive unpacked into, whether a file reached the
  recycle bin.
- **Use the tools the checklist names**: `fsutil`, `winver`, `$PSVersionTable`,
  `where`, `Get-FileHash`.

## Measure before you call it a look

**The owner wants as little left for a person as possible.** So before you put a
row down as an appearance row, look for something that can be *read* in its
place. Most rows that sound like looks have one:

| The row says | What you can read instead |
| --- | --- |
| the list went somewhere | the window title: `(Get-Process filer).MainWindowTitle` |
| the cursor landed on a row | `c` `f` on it, then `Get-Clipboard` |
| keys reached the pane, not the list | type a command that **creates a file**; it exists only if they did |
| the pane is a third / the whole window | `$Host.UI.RawUI.WindowSize.Height` in the pane |
| the list is drawn full height | from the top of `many\`, `<C-f>` then `c` `f`: a page move is as many rows as the list shows, so the file it lands on measures the list |
| no crash, no hang, the walk stopped | `Get-Process filer` -- still there, and its CPU time no longer rising |
| nothing leaves the machine | Windows Firewall rules blocking `filer.exe` and `git.exe` outbound (needs elevation; delete them afterwards), then the same output at the same speed. Without elevation, `Get-NetTCPConnection` / `Get-NetUDPEndpoint` for filer and its children as supporting evidence |
| what the pane and a program in it said to each other | `FILER_PTY_LOG`, and `scripts\keyprobe.ps1` in the pane |
| a click, a hover, the pointer | `SendInput` for the mouse, `GetCursorInfo` for the cursor shape, through `Add-Type` |

Reading text off a screenshot is reading text -- a `~` in a column, a count in a
footer. Taking a screenshot is always fine, and one is worth attaching.

What stays a look after all that is a judgement, and those are the owner's:

- whether a colour *reads as* yellow against a light theme
- whether a 3px bar beside a row is noticeable at all
- whether text is "clipped", "stretched", "overlapping" or "smooth"
- whether a drag felt continuous

When you leave a row for that reason, **say which proxy you tried and why it
could not carry the claim.** "It is visual" on its own is not a reason any more.

## Ticking TESTING-CHECKS.md

This is the part that separates you from the cloud sessions, so read it twice.

A `[x]` in TESTING-CHECKS.md means **"this was verified on a real machine."** A
cloud session can never honestly write one. **You can** -- you are the real
machine. That is a genuine privilege and it is easy to spend badly.

Tick a row only when **all** of these hold:

1. You actually performed the action, on the real `filer.exe`, in this session.
2. The row's expectation is **text you read, or a file state you inspected** --
   not something you looked at and formed an opinion about.
3. You can show the evidence: the command you ran and what came back.

Put the evidence in the pull request body, one line per tick, so whoever merges
can see what each one rests on. A tick with no evidence behind it is worse than
an unticked row: the row would have been checked eventually, and the tick means
it never will be.

**Never tick an appearance row.** If you are unsure which kind a row is, it is an
appearance row.

**TESTING-KEYS.md stays the owner's file**, exactly as it is for the cloud
sessions. A tick there means a person pressed the key *and saw that nothing else
happened*, and the second half is not something you can establish. Report drift
with `cargo run --example make-keycheck -- --check`; do not write the file.

## Where the work is

**This is a queue, not a menu.** It is ordered by how much of each section you
can actually settle, so unless you were told which one to take, take the first
that is not done. Each names its section in TESTING.md, and the count is rows
still on the human's list when it was written.

| Section | Rows | Why it suits you |
| --- | --- | --- |
| **46. spot's Git section** | 46.12–46.16 | `Came in via` / `From branch` are values -- copy them with `y` and read the clipboard. 46.15 needs a commit on an unmerged branch: make it in a throwaway clone under `R:\Temp`, never in the real checkout. 46.16 cannot be done by turning the network off -- that cuts your own session -- so block `filer.exe` and `git.exe` outbound in Windows Firewall instead (see the table above), and remove the rules when done |
| **1. the terminal pane** | 1.30 | 1.22 / 1.23 / 1.27 / 1.28 were settled on 2026-09-30, and 1.30's "not left maximised" half with them (12 rows after reopening, not 35). What is left is 1.30's "list drawn full height" half is a page move from the top of `many\\`: compare the file `<C-f>` lands on before and after |
| **12. undo and redo** | 10 | Every expectation is a file state: in the recycle bin, back at its path, under its old name |
| **45. comparing two folders** | 9 | You build both trees, so every `<` `>` `~` `=` and each footer count is known in advance. 45.7's 64 MB files come from `fsutil file createnew` |
| **44. disk usage** | 13 | Totals against `Get-ChildItem -Recurse -Force \| Measure-Object Length -Sum`; "the walk stops" against the process's CPU time from `Get-Process` |
| **29. the terminal's directory, brought back** | 5 | Where the list went reads off the window title (`(Get-Process filer).MainWindowTitle`). OSC 7 through ConPTY -- nobody else can run it |
| **28. changes made from outside** | 7 | "No crash" is the process still being there; where the cursor landed is `y` on the hovered row |
| **7. the config paths in the help panel** | 8 | The listed directories are text, and `YAZI_CONFIG_HOME` / `FILER_CONFIG_HOME` move them. 7.2 and 7.3 are the pointer and a highlight -- looks, skip them |

Worked through before, and not in the table any more: 25, 41, 35, 32 / 37, 21, 8, 26 and 13 / 15.
Rows still open there were left by those runs on purpose -- ARM, another platform, or eyes -- so
read that section's entry in QA-REPORT.md before taking one.

**One section per run, and one session at a time.** On 2026-09-28 two sessions
ran section 25 in the same working directory at once. It came out as an
independent re-test and found two more bugs, so nothing was lost -- but that was
luck, not the design, and they were a commit away from fighting over the index.

The rest -- the terminal pane's drawing, the minimap's shape, the wheel's feel,
the image zoom's sharpness -- is the owner's, and saying so plainly is more
useful than a thin test.

## Proposals: say what should change

You are the one session that *uses* filer rather than reading it, and the owner
wants to hear what that is like. **Every run ends with proposals** -- things that
should work differently, not only things that are broken. Be direct: "this
should", "this would be better if", "this gets in the way". An opinion you can
ground in something you just did is worth more than a hedged one.

Write them in `QA-REPORT.md`, under a `### Proposals` heading inside your run's
section, and count them in the pull request body. For each:

- **What you ran into**: the steps, and what happened. It must come from this
  run -- a proposal nobody can reproduce is a wish.
- **What should change**, concretely: the key, the message, the default, the
  layout.
- **Why**: what it costs now, and who it would help.
- **How big** you think it is: a line, a function, a design question.

Anything goes -- a confusing message, a key that fights another program, a
default that was wrong for you, a step that took three keys and should take one,
a feature filer lacks that you reached for. Bugs still go in the report as bugs;
this is for the things that work as written and should not.

**Do not implement them.** The session that merges your pull request turns the
ones worth doing into TODO.md tasks, or into QUESTIONS.md when there is a choice
for the owner to make.

## How to work

Run this before anything else. The `TEMP` lines are not optional decoration:
without them you will pick a scratch directory of your own on `C:`, which is
what happened the first time, and [the section below](#where-to-put-files-rtemp-is-a-ram-disk)
explains what the right one buys.

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

# Scratch space. `R:` is a RAM disk -- fast, and gone at the next power cycle.
New-Item -ItemType Directory -Force -Path R:\Temp | Out-Null
$env:TEMP = 'R:\Temp'; $env:TMP = 'R:\Temp'

cargo build --release --manifest-path C:\dev\filer\Cargo.toml
cargo test   --manifest-path C:\dev\filer\Cargo.toml

# The newer ConPTY the Windows release ships beside filer.exe (since v0.49.0).
# Without it the terminal pane runs on the one built into Windows, which breaks
# programs in the pane -- and you would be testing something nobody downloads.
pwsh -File C:\dev\filer\scripts\fetch-conpty.ps1 -Dest C:\dev\filer\target\release
```

- **One section per run.** Read it first and say which rows you can settle and
  which you cannot, before touching anything.
- **Work on `test/win-<section>`**, from the latest `origin/main`. Never push to
  `main`, never `--force`.
- **Do not bump the version and do not write CHANGELOG.md.** A pull request that
  touches `Cargo.toml` and `CHANGELOG.md` conflicts with every other one that
  does -- and every one does. Put the changelog line in the pull request body,
  in English, ready to paste; whoever merges bumps the PATCH.
- **Never run `cargo fmt`.** This tree is hand-formatted; one run rewrites 47
  files.
- Anything you find that is a bug in the program, or a row in TESTING.md that is
  wrong, goes in `QA-REPORT.md`. Do not fix it and do not quietly correct the
  row -- renumbering is how a checklist loses its place.
- **Kill a running `filer.exe` without asking.** CLAUDE.md says the build wins.

## Where to put files: `R:\Temp` is a RAM disk

The machine has a RAM disk mounted at `R:`. **Do all the scratch work under
`R:\Temp`** -- fixtures, sample trees, archives you unpack, anything filer writes
while you test it. It is fast, and it keeps the real disks clear of the debris a
checklist run leaves behind.

**Never invent a scratch directory of your own.** `C:\tmp\something` works and
nothing breaks, which is exactly why it is easy to end up with several of them
on three different disks and no idea which run left which. There is one place.

```powershell
.\scripts\make-fixtures.ps1 -Path R:\Temp\filer-fixtures
```

The two `TEMP` lines in the preamble above carry further than they look:
`util::test_dir` builds every test's tree under `std::env::temp_dir()`, so
`cargo test` moves to the RAM disk with them and nothing else. If you would
rather not remember them, put them in `.claude\settings.local.json` on this
machine and every session gets them for free:

```json
{ "env": { "TEMP": "R:\\Temp", "TMP": "R:\\Temp" } }
```

Reading your own screenshots and captures back off `R:` needs no permission:
`.claude\settings.json` grants it, in the repository, so a `git pull` is all it
takes. Being told to work somewhere and then asked to approve every read of what
you left there was half an instruction -- the section 32 / 37 run answered that
prompt 56 times.

**`R:` empties when the machine powers off.** So one rule follows from it, and it
is the one that matters:

> **Nothing that is evidence may live only on `R:`.**

Evidence is what a `[x]` rests on -- the command you ran, what it printed, the
state you found on disk. A tick whose evidence evaporated at the next reboot is a
tick nobody can check, which is the failure mode this whole file exists to
prevent. So the moment you have it, copy it out:

- **The pull request body** is the primary home, one line per tick. It is on
  GitHub, not on this machine at all.
- **`QA-REPORT.md`**, in the repository, for anything longer -- a failing
  command's full output, a `filer env` dump you are comparing against.
- **Screenshots and captured files**: `C:\dev\filer\docs\` if they belong in the
  repository, otherwise somewhere on `C:`. Never leave the only copy on `R:`.

Two things never go on the RAM disk at all: **the repository checkout**
(`C:\dev\filer` stays where it is) and **anything not yet committed**.

If `R:` is not mounted, say so and use the default temp directory. It is a
convenience, not a requirement, and stopping the run over it would be worse than
writing to `C:`.

## The thing to be most careful about

You are the only session that can confuse *running something* with *checking
something*. A cloud session cannot start the program, so it never mistakes
having started it for having verified it. You can, and the mistake is quiet: the
program came up, nothing looked wrong, the row gets a tick, and a real defect
ships behind it.

Before every tick, ask what would have had to be **different on screen or on
disk** for you to have noticed a failure. If the answer is "I would have had to
look more carefully", that is an appearance row and it is not yours.
