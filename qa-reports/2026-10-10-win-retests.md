# 2026-10-10 win-retests (x64, v0.85.11, unattended)

Machine: Windows 11 Pro 10.0.26200, x64, **not elevated**, AMD Radeon RX 9070 XT (GL), display at 100 %.
`filer 0.85.11`, release build of `7755275` (`test/win-retests`, branched from `main`). ConPTY loaded by the pane:
`C:\dev\filer-wintest\target\release\conpty.dll` (1.24.2607.10001, beside the `OpenConsole.exe` that was measured).
Scratch `R:\Temp\run-20261010-032004`. Evidence in `C:\dev\filer-evidence\win-retests-20261010\`.

The role file's "Re-tests of changed behaviour" row is mostly stale: nearly every row it lists is already `[x]` in TESTING-CHECKS.md.
This run pressed the five rows that were still open and not owner-only: 16.3b, 37.8b, 49.5, 49.7, 49.8. 1.44, 16.13 and 16.13a
were not pressed (waiting on fixes in TODO.md).

- `cargo test`: 743 passed, 0 failed. Parentless shells (`pwsh` / `cmd` / `OpenConsole` whose parent is gone): **0 before, 0 after**
  (two `cmd.exe` from 2026-10-03 -- AMD and Ollama launchers -- are not from this run).
- Recycle Bin: 0 items before, 0 removed (`d` was never pressed).
- Chrome: the owner has a window open, so tabs cannot be enumerated (only the active tab's title is readable). 49.5 opened one tab in
  it with a Ctrl+click; I closed it with Ctrl+W after checking the foreground title was `Example Domain - Google Chrome`. 37.8b skipped Chrome (owner's window open).
- Edge: nothing was open before; 37.8b started it, and session restore opened 30 tabs. After the run 12 windowless `msedge` background
  processes were left and were stopped (`Get-Process msedge` = 0). Word was opened by 16.3b and is closed.
- No `filer.exe` started by this run is still running (`Get-Process filer` = 0).
- The owner's config files were not touched. A throwaway config (`FILER_CONFIG_HOME`), `FILER_TERM_SHELL=pwsh`, `FILER_TERM_ARGS=-NoProfile`, `FILER_PTY_LOG` for the pane runs.

## Ticks

| Row | Result |
| --- | --- |
| 16.3b | `[x]` |
| 37.8b | `[x]` (Edge held; Chrome skipped with the reason) |
| 49.5 | `[x]` |
| 49.7 | stays `[ ]` -- **fails**: ConPTY drops the OSC 8 link, the pane never receives it (below) |
| 49.8 | `[~]` -- iTerm2 inline picture works; the sixel one does not; neither of the two named programs could be run (below) |

`cargo run --example make-testcheck` then `-- --check`: `496 / 538 checked, 7 looked at, in sync with TESTING.md`.

## 16.3b -- holds

A `.docx` with Heading 1 / Heading 2, `[opener] open = start "" %*`, `<S-Tab>` then `<Enter>` on the second heading. The toast ends
`(line 3 not passed: this opener takes no line)`; Word opened the document. Evidence: `<State:>` toast line, `docx\` in the scratch.

## 37.8b -- Edge holds, Chrome skipped

Edge with no Edge process running: `<Enter>` on the browser entry started a new `msedge` process with the PDF on its command line (`Win32_Process`).
Chrome: skipped, the owner's Chrome window is open, so the PDF would join it (the same skip as 37.8a).

## 49.5 -- holds

`echo https://example.com` in the pane, real mouse (`SetCursorPos` + `SendInput`, after checking the foreground and `WindowFromPoint` were filer):
- plain hover: cursor is the arrow, 0 teal underline pixels; Ctrl held: cursor is `IDC_HAND` (`GetCursorInfo`), 161 teal underline pixels.
- Ctrl+click: foreground became `Example Domain - Google Chrome`. A plain click opened nothing and only selected.
Evidence: `49-5-plain-hover.png`, `49-5-ctrl-hover.png`, `49-5-ctrl-up.png` and their `-crop.png`.

## 49.7 -- fails on this machine (ConPTY drops OSC 8)

`$e=[char]27; $b=[char]92; Write-Host "$e]8;;https://example.com$e${b}click me$e]8;;$e$b"` printed `click me` with no underline before or under Ctrl
(0 bright pixels below the text; identical pixels with and without Ctrl; the pointer stays an arrow under Ctrl). `FILER_PTY_LOG` shows why: the
echoed command line contains the sequence, but the output ConPTY hands to the pane is only `click me\r\n` -- the `ESC ] 8 ; ;` introducer and
its terminator are gone, so there is no link for `tsumugi-pane` to underline. conpty.dll 1.24.2607.10001 with pwsh 7.6.6.
Evidence: `49-7-osc8-no-underline.png`, `p5\pty.log` (scratch). Not a filer-side defect proven: the bytes never arrive. Left `[ ]`.

## 49.8 -- iTerm2 picture holds, sixel does not

`chafa` is not installed (not installed unattended). `wezterm imgcat tiny.png` **panics** in the pane
(`attempt to divide by zero`, `wezterm\src\main.rs:383:10`, which looks like a cell pixel size of 0 coming back from the console).
Stand-in: the iTerm2 inline sequence `ESC ] 1337 ; File=inline=1;width=8;height=4:<base64 png> BEL` printed from pwsh (that is what `imgcat` writes):
- drawn where printed: 4489 yellow pixels at y 624..690 (`49-8-drawn.png`);
- 12 lines of text push it out of the 12-row pane (0 pixels), `<S-PageUp>` brings it back, same 4489 pixels, 23 px higher (`pane back: 6 of 7`) (`49-8-scrolled.png`);
- `clear`: 0 pixels and `pane back: 0 of 0` (`49-8-cleared.png`).
- A sixel (`ESC P q #0;2;100;0;0 #0!40~- … ESC \`) is **not** drawn: ConPTY strips the DCS introducer and terminator and the pane prints the body as text
  (`q#0;2;100;0;0#0!40~-#0!40~-#0!40~`, `49-8-sixel-dropped.png`). So `chafa -f sixels` cannot work through this ConPTY.
`[~]` because the picture was judged from pixel counts, with a stand-in sequence rather than either program the row names.

### Proposals

1. **`--keys` modifier + click and hover steps** (TODO already notes it): 49.5 needed a hand-written SendInput script and a foreground check for what should be `<C-Click:x,y>` / `<Hover:x,y>`.
2. **49.7 / 49.8 rows**: reword 49.7 for ConPTY (OSC 8 is dropped, so the row cannot pass unless filer asks the console to pass sequences through, or tsumugi detects it), and 49.8 to name the iTerm2 sequence (the one that survives) instead of `chafa -f sixels`.
3. **`wezterm imgcat` divides by zero in the pane**: check what the pane reports for the cell pixel size (`CSI 16 t` / `CSI 14 t` replies, or `TIOCGWINSZ` pixel fields); a 0 there will break other image tools too.
4. **`--keys` quoting of `\"`**: a `\"` at the end of a `-ArgumentList` string is fragile (mangled once this run). `scripts\keys.ps1` could take a keys file instead of a string.
5. **Role file**: drop the already-`[x]` rows from "Re-tests of changed behaviour" so the queue's first row stops being a search.

### Votes

QUESTIONS.md on `origin/main` has no item in `投票中` (every item is `反映済み` / `反映済み（多数決）`), so there is nothing to vote on.
