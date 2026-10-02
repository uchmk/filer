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

**Start with `--keys`.** Since v0.54.0 filer presses keys itself: `filer <dir> --keys "<Tab>C"`
opens spot on the first row and copies the whole panel, and `Get-Clipboard` reads it. No window
to find, no `SendInput` for a screen saver to swallow. Reach for `PostMessage` / `SendInput` only
for what `--keys` cannot do -- the mouse, or a sequence that depends on what you read in between.
**Since v0.59.0 `<Wait:N>` pauses N ms** after the key before it, for the shell or a program in the
pane: `--keys "<C-t><Wait:1500>git<Space>status<Enter><Wait:1000><C-S-Enter>"`. Do not pad with
harmless keys any more; a wait says how long it waited.
**Since v0.65.0 `<Now>`** presses the next key without waiting for the last to settle (`d<Now>w`, `j<Now>j`), and
**since v0.67.0 `<Shot:name>`** saves the window as `name.png` beside the `FILER_KEYS_DONE` file, so the picture between
two keys comes from the same run (`<Shot:focused><C-t><Shot:unfocused>`).
**Since v0.67.12** a key waits for a file job it started (trash, restore, copy, link) to finish, so `u<Shot:x>` shows the
toast without a `<Wait:>` -- use `<Now>` to catch a job mid-run. And the `FILER_KEYS_DONE` file always comes: its last line
is `keys: done`, or it starts `keys: stalled` with the keys pressed, the last one and the rest, when nothing went in for
30 s past any wait. **A stalled file is not a result** -- say so in QA-REPORT.md with its lines.

| The row says | What you can read instead |
| --- | --- |
| the list went somewhere | the window title -- of **filer's own window**: class `Window Class`, title starting `Filer:`. Not `(Get-Process filer).MainWindowHandle`, which can be winit's untitled `Winit Thread Event Target` window, whose empty title reads like a failure (#88) |
| the cursor landed on a row | `c` `f` on it, then `Get-Clipboard` |
| keys reached the pane, not the list | type a command that **creates a file**; it exists only if they did |
| the pane is a third / the whole window | `$Host.UI.RawUI.WindowSize.Height` in the pane |
| the list is drawn full height | from the top of `many\`, `<C-f>` then `c` `f`: a page move is as many rows as the list shows, so the file it lands on measures the list |
| no crash, no hang, the walk stopped | `Get-Process filer` -- still there, and its CPU time no longer rising |
| nothing leaves the machine | Windows Firewall rules blocking `filer.exe` and `git.exe` outbound (needs elevation; delete them afterwards), then the same output at the same speed. Without elevation, `Get-NetTCPConnection` / `Get-NetUDPEndpoint` for filer and its children as supporting evidence |
| what the pane and a program in it said to each other | `FILER_PTY_LOG`, and `scripts\keyprobe.ps1` in the pane |
| a click, a hover, the pointer | `SendInput` for the mouse, `GetCursorInfo` for the cursor shape, through `Add-Type`. **Never `PostMessage` for the mouse**: a posted click, right-click or `WM_MOUSEWHEEL` reaches egui not at all, though `PostMessageW` returns `True` (#100). A 64-bit `INPUT` is **40 bytes**; padded to 48 for a `MOUSEINPUT`, `SendInput` returns 0 and moves nothing (`KEYBDINPUT` does need padding to reach 40) |
| a key or a paste reached the program in the pane | `FILER_PTY_LOG`: `in key` / `in paste` lines are the bytes sent, so `\e[200~` around a paste, `\eOA` against `\e[A`, or no line at all, can be read |
| a program was started, and how | `Get-CimInstance Win32_Process` for filer's children: the `CommandLine` shows the editor and the line number it was given |
| a toast or a warning said something | a screenshot, read as text. Also `filer env`, which prints the config warnings |
| the app kept running / quit | `Get-Process filer` after the key |

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

**After ticking, run `cargo run --example make-testcheck`** -- it keeps your
ticks and rewrites the counts, which a hand edit misses (the first ARM64 run
fixed the section heading and not the total, and CI went red on it). Then
`-- --check` must say `in sync`.

Put the evidence in the pull request body, one line per tick, so whoever merges
can see what each one rests on. A tick with no evidence behind it is worse than
an unticked row: the row would have been checked eventually, and the tick means
it never will be.

**Never tick an appearance row.** If you are unsure which kind a row is, it is an
appearance row.

**TESTING-KEYS.md: you may tick it too (since 2026-10-01, the owner's call).**
A tick there means the key *did what its description says and nothing else* --
`<A-m>` once ran its own command and the plain `m` as well, and each half looked
right alone. "Nothing else" is the half a picture cannot give, so it is read the
same way as everything else here: as state, before and after.

- **Press it with `--keys`**, never `PostMessage` (a posted plain character
  arrives twice, #109; a posted modifier not at all, #107).
- **Snapshot before and after** the one key, everything it must *not* change:
  the window title (where the list is), `c` `f` / `c` `c` for the hovered entry,
  the header's `N selected · M items`, the clipboard (armed with a sentinel and
  read back first), which overlay is open, the scratch directory's listing with
  hashes, and for a key pressed in the pane the `in key` lines of
  `FILER_PTY_LOG` (a key the pane keeps must add none). Only what the key is
  meant to change may differ.
- **In the pull request, one line per key**: the key, what changed (the
  description's half), and the snapshot that did not (the "nothing else" half).
- Tick only with both halves. Then `cargo run --example make-keycheck -- --check`
  must still say `in sync`; never regenerate the file, only flip `[ ]` to `[x]`.

## Where the work is

**This is a queue, not a menu.** It is ordered by how much of each section you
can actually settle, so unless you were told which one to take, take the first
that is not done. Each names its section in TESTING.md, and the count is rows
still on the human's list when it was written.

| Section | Rows | Why it suits you |
| --- | --- | --- |
| **Re-tests of changed behaviour** | 4 | First, always: these rows were changed by a fix, so an earlier result no longer stands (an old `[x]` was taken off). (8.x, 12.17, 12.18, 16.x, 20.5, 44.10 and 45.18 were settled on x64 by #182; 29.x by #176 on ARM64; 21.14 and 21.15 by #174.) **34.15** (v0.67.16): `<F1>C` -- the toast is `Copied the help panel: N keys` (`FILER_KEYS_DONE`). #180 and #182 found the content right but the row's own check, `Get-Clipboard | Select-String "^j\t"`, finds nothing, because the copy is LF-only and `Get-Clipboard` returns one string; v0.67.24 (Q52) writes CRLF on Windows, so the row's check should now find the `j` line as written: run it unchanged. **13.8a and 13.8b** go to **the ARM64 laptop only**: they need a machine **without** Developer Mode and not elevated, and this x64 machine has it on (#180, #182). Take the row out of this list in your `## Queue` once it is settled. **1.38** (v0.67.25, Q53, new): `<C-t>` back to the list, `<C-S-t>` -- `pane: closed` and toast `Ended the shell` in `FILER_KEYS_DONE`; `<C-S-t>` again -- toast `No terminal to close`. In TESTING-KEYS.md the new `[mgr]` `<C-S-t>` row is the same key. **25.6 and 25.19b** (v0.68.0, Q55, 25.19b new): `filer --help` lists `env --out FILE`; `filer env --out out3.txt` from a folder with Japanese in its path writes the whole report, readable with `Get-Content -Encoding utf8`, and `--out` without a name / `--outt x` exit 2 and write nothing. Text, one command each. **29.1, 29.8 and 29.10** (v0.69.0, Q50, 29.10 new): the `<A-Up>` toast with no hook now names the command that adds it -- `filer shell-hook | Add-Content $PROFILE, then <C-S-t> and <C-t>`, with the full path in place of `filer` when the zip's exe is not on the `PATH` -- and for 5.1 says it cannot have one. Run exactly what the toast says, reopen the pane, `cd C:\dev`, `<A-Up>`: `cwd:` in `FILER_KEYS_DONE`. Start from a `$PROFILE` without the hook (save and restore the real one). |
| **48.2 and 48.6 on real x64** | 2 | Ticked from the ARM64 laptop under x64 emulation (#151), where `--version` already said `(x86_64)` and the pane loaded the zip's own `conpty.dll`. Read the two again on this machine from the newest release's x64 zip -- `--version`, and `(Get-Process filer).Modules` after `<C-t>` -- and record them in QA-REPORT.md under an x64 heading; the boxes are already ticked. 48.1 / 48.3 / 48.4 / 48.5 are file inspections that do not depend on the machine |
| **v0.55.0 on x64** | -- | ARM64 ticked 29.2, 29.6, 40.14-40.16 and 45.16 (#107); press the Q27 ones again on x64 -- 1.31-1.34, 40.15, 40.16 -- and record them in QA-REPORT.md under an x64 heading, since ConPTY's input path is where the two machines could differ. 30.1 / 30.15 wait on the right-click fix in TODO.md |
| **47. an idle window uses no CPU** | 4 | First, because a fix waits on it: v0.54.2 stopped a redraw loop that is the likeliest cause of the 1.0 CPU-s/s #86 measured. Every row is `(Get-Process filer).CPU` read twice, 10 s apart; 47.2 is the sequence that used to start the loop. **Since v0.65.0 `--keys` can do 47.2**: `j<Now>j` puts the second key in on the next frame instead of waiting for `App::settled()`, which is false while the preview debounce is pending. One frame is well inside the 40 ms debounce while the script runs (it asks for a frame every frame); if you want the gap as a number, the `PostMessageW` + `Stopwatch` way still works (#103 got 22-25 ms). `sort_dir_first` is on by default, so start on a file above a folder only with it off in a run-only `FILER_CONFIG_HOME`. An open prompt used to read 0.14-0.27 CPU-s per 10 s from the caret's blink; since v0.59.3 the caret is steady, and 47.5 checks that it now reads 0. Take a positive control (keys at 100 ms for 10 s) so a 0 is a reading. On ARM64 all three rows read 0 (#103). If it still rises, 47.4 says which thread |
| **29. the terminal's directory, brought back** | 5 | Where the list went reads off the window title (`(Get-Process filer).MainWindowTitle`). OSC 7 through ConPTY -- nobody else can run it |
| **14. the parent column, with the mouse** | 6 | Click with `SendInput`; the window title says where the list went and `c` `f` which row the cursor is on |
| **33. config warnings** | 10 | The warning lines are text: a screenshot, and `filer env`. 33.6 (does yellow read on a light theme) and 33.9 (boxes do not overlap) are looks |
| **31. a host's shares** | 5 left | #105 ticked 8 on ARM64 against a Samba server on the LAN, `\\192.168.0.150` (four shares) -- usable only if `net view \\192.168.0.150` answers. Left: 31.2's host-name half (Windows refused `proxmox.local`; try this machine's own name), 31.7 / 31.8 (a screenful of shares, a share with a space or Japanese -- `New-SmbShare`, elevated), 31.5a and 31.9 wait on TODO.md |
| **22. opening an editor at a line** | 6 | Only the editors installed here: `Get-CimInstance Win32_Process` shows the command line filer built, `-n42` or `+42` or `--goto`. Say which ones were not installed |

Worked through before, and not in the table any more: 25, 41, 35, 32 / 37, 21, 8, 26, 13 / 15, 46, 1, 12, 45, 40 (40.7, 40.8 and 40.12 left for the reasons in TODO.md; #100) 39 (39.9 is a bug in TODO.md; #102) 30 (30.1, 30.3, 30.4 and 30.11 are bugs or wording in TODO.md; #104) 28 (all seven, ARM64, #108) 44 (44.7 waits on the budget fix in TODO.md; #109) and 24 (24.2 waits on Q34, 24.3 on case-sensitive folders; #111) (45.11 waits on the symlink fix in TODO.md; `fx45.ps1` in the run's evidence rebuilds its tree).
46.16 is still open: it needs the firewall rules, so an elevated run -- or a person. So is 13.17's `mklink /D` half (its junction half passed on ARM64, #136), and so is 45.11:
symbolic links need elevation or developer mode (#98 passed its junction form on ARM64).
Rows still open there were left by those runs on purpose -- ARM, another platform, or eyes -- so
read that section's entry in QA-REPORT.md before taking one.

**One section per run, and one session at a time** -- and that includes an
unattended run: `auto-wintest.ps1` only knows about the runs it started itself,
so do not start one by hand while it may fire. On 2026-09-30 two sessions shared
the clipboard, and each one's `c` landed in the other's capture. On 2026-09-28 two sessions
ran section 25 in the same working directory at once. It came out as an
independent re-test and found two more bugs, so nothing was lost -- but that was
luck, not the design, and they were a commit away from fighting over the index.

The rest -- the terminal pane's drawing, the minimap's shape, the wheel's feel,
the image zoom's sharpness -- is the owner's, and saying so plainly is more
useful than a thin test.

## Unattended runs

`scripts/auto-wintest.ps1` starts you with no one watching, when `main` has
changed this file, TESTING.md or TESTING-CHECKS.md and no pull request from
your lane (`test/win-*` or `test/arm-*`, see [the ARM64 lane](#the-arm64-lane))
request is open. Everything above still holds. What changes is that **nobody
will answer a question**, so:

- **Take the first section in your lane's queue** (the prompt names it). Do not
  ask which one.
- **Count parentless shells around `cargo test`** (#180): before and after it, count the
  `OpenConsole`, `pwsh` and `powershell` processes whose parent is gone
  (`Get-CimInstance Win32_Process` and a parent id that no process has). Any increase
  is a finding, with the counts. v0.67.21 fixed the one #180 found (#182 counted 89 before and 88
  after); this is how a new one would show.
- **Never wait for input.** A choice that is the owner's goes in QA-REPORT.md,
  as a finding or a proposal, and the run carries on with what it can settle.
- **Your checkout is the worktree the prompt names**, not `C:\dev\filer`: read
  every path in this file with that swap. Make your branch there with
  `git checkout -B test/<lane>-<section> origin/main`; if git refuses because the
  branch is checked out in another worktree, add `-auto` to the name.
- **Elevation**: check `([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole('Administrators')`.
  If you are not elevated, rows that need it (the firewall rules) are left, and
  the report says that was the reason.
- **The screen can lock under you, or a screen saver can take it.** If
  `Get-Process LogonUI` appears, input stopped reaching the window: nothing
  measured after that counts. Stop driving it and report how far you got.
  **A screen saver is not caught by that check** -- the ARM64 laptop's (ASUS OLED
  Care) held the input desktop with no `LogonUI` process, and `SendInput` kept
  returning success while reaching nothing (#88). Ask the input desktop instead:
  `OpenInputDesktop` + `GetUserObjectInformation(UOI_NAME)` must say `Default`,
  and `SystemParametersInfo(SPI_GETSCREENSAVERRUNNING)` must be false.
- **Prefer `PostMessage` to `SendInput`** for keys (`WM_KEYDOWN` / `WM_CHAR` /
  `WM_KEYUP` to filer's own window). It needs no foreground, works under a
  screen saver, and cannot leak a keystroke into another window. **Put the scan
  code in `lParam`**: `1 | (MapVirtualKey(vk, 0) << 16)`. winit reads the key from
  bits 16-23, so with `lParam = 0` the press is dropped -- that, not PostMessage,
  is what #93 ran into (found by #96). **Plain characters go through `--keys`,
  not `PostMessage`**: a posted `WM_KEYDOWN` + `WM_CHAR` + `WM_KEYUP` for `c` ran as
  `c` `c` (#109), while `WM_CHAR` alone and `WM_KEYDOWN` + `WM_KEYUP` alone reach
  nothing. Posting is for named keys (`<Esc>`, `<Enter>`, `<Space>`) and for timing
  `--keys` cannot hit. Modifiers do not arrive by posting at all (#107). `SendInput` is
  still the tool for the mouse and for anything that must come through the real
  input queue -- then check the input desktop first. `PrintWindow` with
  `PW_RENDERFULLCONTENT` captures the window under a screen saver too.
- **After a change made from outside, wait half a second before reading the
  listing.** Since v0.57.2 an idle window reads a flagged directory by itself
  (#108 found it did not); 28.8 is the row that checks it. On an older build,
  send two keys about a second apart instead.
- **In a `--keys` script, write a space as `<Space>`.** Since v0.57.2 a plain
  space is refused before the window opens (#110 sent the shell `echo` and
  walked the list with the rest).
- **A minimised window does not act on posted keys** until it is restored
  (`SW_RESTORE`); read anything after restoring. **Stopping a screen saver takes
  its process**: `Stop-Process -Name` misses `OLED Care Screensaver.scr`
  silently; `Get-Process | Where-Object ProcessName -match 'OLED Care' |
  Stop-Process -Force` works, and `OpenInputDesktop` says `Default` at once (#103).
- **A person's config files are not scratch.** Before touching `$PROFILE`, a
  shell rc or anything outside the scratch directory: record whether it is a link
  and where to (`(Get-Item $f).LinkTarget`), its size and hash, and take a copy.
  **Append, never overwrite** -- `Set-Content` writes through a symlink into
  whatever it points at (#101 replaced 128 lines of a profile kept in a git tree,
  and only `git restore` brought it back). At the end, restore and show the hash
  matches; put all three in the pull request.
- **Do not edit this file; say how the queue should change instead.** An
  unattended run is refused writes under `.claude/` (the permission guard
  treats it as sensitive -- found on 2026-09-30, the first unattended run), so
  put a `## Queue` section in the pull request body: which row to delete, or
  what to cut it down to and why. Whoever merges applies it in the same step.
  It matters: merging is what starts the next run, and a queue that still
  lists your section sends the next run to it again.
- **Finish the run yourself**: commit, `git push -u origin <your branch>`, and
  `gh pr create --base main` with the body this file asks for. Never merge,
  never push to `main`.
- **Close every `filer.exe` you started** before you finish.
- **The last line you print** is one of these, alone, so the script can log it:
  - `WINTEST_DONE <pull request URL>`
  - `WINTEST_NOTHING` -- the queue is empty, or every section left needs a person
  - `WINTEST_FAILED <one line: why>` -- and commit nothing in that case

## The ARM64 lane

A second machine, a Windows laptop on ARM64, runs the same role with
`auto-wintest.ps1 -Lane arm`. Everything in this file applies to it, with
these differences:

- **Its branches are `test/arm-<section>`**, and it waits only on its own open
  pull requests, so the two machines never hold each other up.
- **Its queue is the table below**, not the one in "Where the work is". Say in the
  pull request's `## Queue` section what to change in *this* table.
- **There is no RAM disk.** The script picks the scratch directory, sets `TEMP`
  and `TMP` to it, and names it in the prompt: read `R:\Temp` in this file as
  that directory.
- **Check what you are running first**: `filer env` must say `Process arch
  aarch64` for the native build. A run that tested the x64 build by accident
  proved nothing about ARM64.
- **Ticks.** TESTING-CHECKS.md has one box per row. A row already `[x]` from
  the x64 machine stays as it is: record the ARM64 result in QA-REPORT.md
  under a heading that says ARM64, one line per row with its evidence -- a
  row that *fails* on ARM64 is a finding, and the most valuable kind this lane
  can produce. Tick only rows still `[ ]` that you verified here.

- **An x64 build of current `main` is one command away**, so no row has to wait
  for a release to see "the x64 build under emulation":
  `rustup target add x86_64-pc-windows-msvc`, then
  `cargo build --release --target x86_64-pc-windows-msvc` (the VS Build Tools here
  carry the x64 libraries; no `vcvarsall` needed, 2m31s -- found by #84). Check
  the PE machine (`0x8664`) before believing which binary ran.

| Section | Rows | What it is on ARM64 |
| --- | --- | --- |
| **25.19, 25.19a** | 2 open | Text, one command each (v0.67.26, Q54 split the row): `filer env \| Select-String arch` prints exactly the two arch lines (#183 already saw it pass); `$v = & filer env \| Write-Output; $v.Count` is the report's line count (48 on #183's build), not 0. Run each in its own `pwsh -NoProfile -File`, as #183 did |
| **13.8a, 13.8b** | 2 open | Only here: this laptop has no Developer Mode (check `AllowDevelopmentWithoutDevLicense` is 0 and you are not elevated). `y` a folder, `-` elsewhere: the error toast ends `A junction needs neither: mklink /J "…" "…"` (13.8a), then the question `Make a junction instead?` (`overlay: confirm` in `FILER_KEYS_DONE`); `y` makes it -- `(Get-Item <link>).LinkType` reads `Junction`, toast `Made a junction <name> — u to undo` -- `u` removes it with the folder untouched and `U` makes it again (13.8b, v0.67.19) |
| **the test suite** | -- | `cargo test` natively on ARM64, every run. Green at 0.51.1 (493 / 0, #81), 0.51.3 (494 / 0, #84), 0.52.3 (499 / 0, #88), 0.53.1 (502 / 0, #91), 0.54.0 (505 / 0, #93), 0.54.3 (506 / 0, #96) and 0.54.5 (509 / 0, #98) 0.54.9 (509 / 0, #100 and #101), 0.54.10 (509 / 0, #102), 0.54.12 (509 / 0, #103), 0.54.13 (509 / 0, #104), 0.54.14 (509 / 0, #105), 0.55.1 (523 / 0, #107), 0.55.2 (523 / 0, #108), 0.55.3 (523 / 0, #109), 0.55.4 (523 / 0, #110), 0.55.5 (523 / 0, #111), 0.56.2 (525 / 0, #114), 0.57.2 (538 / 0, #119) and 0.58.2 (553 / 0, #122; one run of `ending_a_busy_shell_asks_first` failed under load, steadied in 0.59.2 -- report it if it comes back) and 0.59.2 (558 / 0, #126). Any failure here and not on the x64 runner is the finding; paste the test name and the panic 0.65.2 (591 / 0, #154 -- the first ARM64 run where #136's help test passed against the machine's real config), 0.67.3 (594 / 0, #163), 0.67.4 (594 / 0, #164, #165), 0.67.5 (594 / 0, #166), 0.67.7 (594 / 0, #168), 0.67.9 (595 / 0, #171), 0.67.13 (602 / 0, #173), 0.67.14 (602 / 0, #174). 1.2 and 1.8 are all that is left of section 1, and both are looks (#154) |

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
- **Work on `test/win-<section>`** (`test/arm-<section>` on the ARM64 machine), from the latest `origin/main`. Never push to
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

**Nothing that goes through the Recycle Bin can run on `R:`.** The RAM disk has
no bin, and `d` fails there before it reaches the shell (`canonicalize` cannot
read the volume). Section 12, and any row that presses `d` and expects the bin,
runs in a directory on `C:` instead -- `%LOCALAPPDATA%\Temp\filer-<section>` --
and cleans up after itself. (The section 12 run found this, 2026-09-30.)

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
