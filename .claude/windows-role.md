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

## What you still cannot do, and must not pretend to

**TESTING.md is a checklist for a person, and most of it stays that way.** The
rows that need eyes need eyes:

- whether a colour *reads as* yellow against a light theme
- whether a 3px bar beside a row is noticeable at all
- whether text is "clipped", "stretched", "overlapping" or "smooth"
- whether a drag felt continuous
- whether a layout is *wrong* rather than merely present

You can screenshot these. **You cannot judge them**, and a screenshot you
describe is not a check that passed. Say what you saw and leave the row alone.

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
| ~~**25. `filer env`**~~ | ~~19~~ | **Done** (v0.47.25): 15 ticked, 4 left that need ARM, no bold face anywhere, or eyes |
| **41. spot panel providers** | 14 | Archive counts, encodings, architectures -- all values, not looks. 41.8 is the six release binaries, which is the only check the cross-builds have ever had. Skip 41.12 (appearance) |
| **35. config paths per platform** | 10 | Same tools as 25, already proven. Other platforms' rows are not yours |
| **32 / 37. openers** | 20 | Did the right program start, with the path intact as one argument. 25.13's window-title check is the pattern. Close what you open |
| **21. archives** | 9 | Pack, unpack, then look at what is on disk -- `Get-ChildItem -Recurse` is the evidence |
| **8. which shell the pane runs** | 7 | `$PSVersionTable`, `$PROFILE` -- strings. Small, but **ConPTY is code a cloud session cannot run a line of** |
| **26. bug report** | 10 | Compare filer's version and OS lines against `filer --version` and `winver` |
| **13 / 15. links** | ~8 | `fsutil hardlink list`, `New-Item -ItemType SymbolicLink` |

**One section per run, and one session at a time.** On 2026-09-28 two sessions
ran section 25 in the same working directory at once. It came out as an
independent re-test and found two more bugs, so nothing was lost -- but that was
luck, not the design, and they were a commit away from fighting over the index.

The rest -- the terminal pane's drawing, the minimap's shape, the wheel's feel,
the image zoom's sharpness -- is the owner's, and saying so plainly is more
useful than a thin test.

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
