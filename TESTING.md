# Testing on a real machine

Everything here is something automated tests cannot reach, and most of it is
something nobody has looked at yet. The development sessions that wrote these
features ran in a Linux container with no display and no MSVC linker: the code
was type-checked for Windows and its logic was unit-tested, but **the window has
never been on a screen**. That is the gap this checklist is for.

What *is* covered automatically, so it is not repeated below: every pure
function (key parsing, the diff algorithm, rename rules, the undo stacks, image
zoom arithmetic, minimap row summaries, pane geometry), the keymap's
consistency, and that the whole thing compiles for Windows, macOS and Linux.
`cargo test` on Windows CI runs all of it on every push.

## The keys

[TESTING-KEYS.md](TESTING-KEYS.md) is a tickable line per key binding — 193 of them
across nine layers, which is more than anyone tracks in their head while working
through them one at a time. It is generated from the default keymap:

```powershell
cargo run --example make-keycheck
```

Ticks survive regeneration, so a keymap change does not cost the afternoon
already spent. Keys that have left the keymap are listed at the end rather than
dropped, since one that vanished is worth noticing.

The sections below are the other half: behaviour that no single key exercises.

## What you need

1. **A `filer.exe`.** Either:
   - a [release](https://github.com/uchmk/filer/releases) — a plain download, no
     account needed; or
   - the artifact on the newest green [Build run](https://github.com/uchmk/filer/actions/workflows/build.yml)
     — needs a signed-in GitHub account with access, and expires after 90 days; or
   - `cargo build --release` in a clone, which needs a Rust toolchain (1.95 or
     newer, as egui 0.36 requires) and the MSVC build tools.
2. **The fixtures.** `scripts/make-fixtures.ps1` builds every file the checks
   below point at:
   ```powershell
   .\scripts\make-fixtures.ps1
   ```
   It writes to `filer-fixtures` on your desktop unless `-Path` says otherwise,
   and clears that directory first. `git` on `PATH` is optional; without it the
   repository fixture is skipped and the git checks with it.
3. **Nothing else.** With no `yazi.toml` or `keymap.toml` anywhere, filer uses
   its built-in defaults, which is what the key names below mean. If you *do*
   have a yazi config, it is read, and your own bindings win — worth knowing
   before reporting a key as wrong.

## How to report

Anything that does not match the "expect" column: the check's number, what
happened instead, and a screenshot where it is something visual. Sections are
numbered in the order they appear and each check is `<section>.<n>`, so `13.2`
is the second check of section 13; a letter on the end (`1.9a`) is one added
later beside the check it belongs with, rather than at the bottom of the
section. Sections ran A–Z until v0.26.6 and the letters were spent. Versions matter
— say which `filer.exe` (the release tag, or the commit the artifact is named
for). A check that cannot be run at all (no editor installed, no network share)
is a skip, not a failure; say which.

---

## 1. The terminal pane

The riskiest area: an embedded terminal is a lot of machinery and none of its
drawing has been seen. `<C-t>` opens it.

| # | Do | Expect |
| --- | --- | --- |
| 1.1 | `<C-t>` from the file list | A shell opens along the bottom, already in the directory the list is showing |
| 1.2 | Type `dir` and press Enter | Output in the list's own font, columns lined up, no overlapping glyphs |
| 1.3 | Look at the cursor | A block where the shell's cursor is, and it moves as you type |
| 1.3a | `<C-t>` to give the keys back (v0.20.2) | The cursor goes **hollow**, and the rule along the top of the pane stays the plain border colour — it no longer turns green with focus |
| 1.4 | Run something colorful (`git status` in the `repo` fixture) | The 16 ANSI colors, and they match the file list's own colors rather than looking like a second palette |
| 1.5 | **`<C-t>` again** | Keys go back to the list — **and the shell is still there**, with its output intact. This is the v0.6.0 fix; before it, this ended the shell |
| 1.6 | `<C-t>`, `<C-t>`, `<C-t>` a few times | The same shell throughout. The scrollback never resets |
| 1.7 | `<C-S-t>` | *Now* the pane closes and the shell ends |
| 1.8 | Reopen, then resize the window | The grid reflows; no clipped half-columns, no stretched text |
| 1.9 | `dir` in `many\` to fill the screen, then `<S-PageUp>` | **The text moves.** Until v0.20.3 only the note moved — it said "N lines back" over a screen that had not scrolled |
| 1.9a | `<S-PageUp>` / `<S-PageDown>` (v0.20.4) | Half a screen back / forward. Until v0.20.4 the sign was inverted, so `<S-PageUp>` aimed at the bottom and did nothing |
| 1.9b | `<S-Home>`, `<S-End>` | The oldest line held, and the prompt. These worked before — no sign to get wrong |
| 1.9d | The mouse wheel over the pane (v0.20.4) | Moves smoothly, a notch at a time. It used to need spinning hard for one or two lines |
| 1.9e | Scroll back far enough that the prompt leaves the screen | The cursor goes with it — no block left behind at its old height |
| 1.9c | `<C-S-f>` for a word far up the scrollback, Enter | The view jumps to the match **and the match is highlighted** |
| 1.9f | `<C-S-f>` for a word that is on screen right now (v0.20.4) | The one on screen is found first, not an older one up in the history |
| 1.9g | `<C-S-n>` / `<C-S-b>` after that | `<C-S-n>` walks further up into the history, `<C-S-b>` comes back down |
| 1.9h | `<C-S-f>` for something that is not there | A red toast saying so — not silence |
| 1.10 | `<S-End>`, then type a character | Back at the bottom, and typing alone would have done it |
| 1.11 | Drag across some output (v0.20.4) | **It highlights as you drag**, and is on the clipboard when you let go. Before v0.20.4 the copy worked and nothing was drawn |
| 1.11a | Drag **right to left** over the same run of text (v0.26.4) | The same text, character for character. Until v0.26.4 a backwards drag lost one at **each** end |
| 1.11b | Start the drag **on** the first character, not to its left (v0.26.4) | It is included. It used to be dropped unless the drag began in the gap before it |
| 1.11c | Drag from the right half of a character | That character is left out — correct, and the same rule that makes 1.11a work |
| 1.12 | Double-click a word | The word is selected, and visibly so |
| 1.13 | `<C-S-f>`, type a word from the scrollback, Enter, then `<C-S-n>` | Matches are found and stepped through; it wraps at the end |
| 1.14 | `<F1>` inside the terminal | The key list opens **over** the terminal. `<Esc>` closes it and typing goes back to the shell |
| 1.15 | `<C-S-p>` inside the terminal | The command palette opens, and running something from it works |
| 1.16 | `cd` somewhere in the shell, then `<A-Up>` | The file list follows to where the shell is |
| 1.17 | Select two files, `<A-t>` | Their paths are typed onto the shell's line, quoted, **not run** |
| 1.18 | With a shell that reports OSC 7 (PowerShell 7, or bash with a `PROMPT_COMMAND`), change directory in the list | No stray `cd` is typed into the shell |

## 2. The minimap (v0.5.0)

Open `long.rs` — 4000 lines, shaped so the bands should be recognisable.

| # | Do | Expect |
| --- | --- | --- |
| 2.1 | Hover `long.rs` | A narrow strip down the right of the preview, made of short horizontal bars |
| 2.2 | Look at the shape | Comment headers read as long bars, indented blocks as bars starting further right, the blank line every 40 as a gap. It should look like the file |
| 2.3 | Look at the colors | The bars carry syntax colors — strings and comments differ from code — not one flat color |
| 2.4 | Find the viewport box | A lighter box with a border, covering the part of the file on screen |
| 2.5 | `<A-j>` a few times | The box moves down in step with the text |
| 2.6 | Click halfway down the strip | The preview jumps there, with the clicked line in the **middle** of the pane, not at its top |
| 2.7 | Drag up and down the strip | The preview follows continuously |
| 2.8 | Narrow the window until the preview is thin | The map disappears before the text becomes unreadable, and the text takes the space back |
| 2.9 | `<A-n>` | The map toggles off and on |
| 2.10 | Open `notes.md` (rendered) | **No map** — this is deliberate, the rendered lines are not the file's lines |
| 2.11 | Press `M` for source | The map appears |
| 2.12 | A short file (`same-a.txt`) | No map: two lines are not worth mapping |

## 3. Image zoom and pan (v0.5.0)

`zoom-me.png` is 3200×2400 with an 8-pixel grid, so blur is obvious.

| # | Do | Expect |
| --- | --- | --- |
| 3.1 | Hover `zoom-me.png` | It fits the pane. The caption reads `3200 × 2400 · fit NN%` |
| 3.2 | `<A-1>` (1:1) | It fills far more than the pane, showing the middle. **The grid lines are crisp** — this is the re-decode working; if it is a blurred enlargement of the fitted copy, that is the bug this was built to avoid |
| 3.3 | Watch the moment it sharpens | The picture must **not jump or change size** when the sharper copy arrives. Only its sharpness changes |
| 3.4 | Drag it | It pans, and stops when its edge reaches the pane's edge — it cannot be thrown off screen |
| 3.5 | `Ctrl` and the wheel, pointer on a grid intersection | It zooms **about the pointer**: the intersection under the cursor stays under it |
| 3.6 | Plain wheel (no Ctrl) | Scrolls the pane, does not zoom |
| 3.7 | Double-click | Back to fitting, centred |
| 3.8 | `<A-i>` / `<A-o>` | In and out in steps. The caption's percentage follows |
| 3.9 | Zoom in, then `j` to the next file and back | It is fitted again — a zoom belongs to the file it was set on |
| 3.10 | Hover `tiny.png` (48×48) | Shown at its own size, **not blown up** to fill the pane |

## 4. Compare, side by side (v0.4.0)

`<A-d>` with `compare-left.txt` and `compare-right.txt` both selected, or one in
each pane with the view split.

| # | Do | Expect |
| --- | --- | --- |
| 4.1 | Compare the two | Two columns, a line down the middle, with line numbers on each side |
| 4.2 | Find line 100 | The changed line sits **opposite** the line it replaced — not listed as a removal and an addition far apart |
| 4.3 | Look at the colors | Removals tinted on the left, additions on the right, matching the git signs' colors |
| 4.4 | Look at the line numbers after the insertion | The two sides differ by one, each counting its own file |
| 4.5 | `n` / `N` | Between the two differences, not line by line inside one |
| 4.6 | `j` `k` `<C-d>` `gg` `G` | Scrolling, with the footer's `x–y of z` keeping up |
| 4.6a | **`G`, then `k` once** (v0.22.1) | Moves by one row straight away. Until v0.22.1 `G` overshot by a screenful, so `j` and `k` did nothing for about as many presses as the pane is tall |
| 4.6b | `G`, then `j` | Stays at the bottom, with the last row visible above the footer |
| 4.6c | `G` on a diff shorter than the pane | Nothing moves; every row was already on screen |
| 4.7 | `same-a.txt` and `same-b.txt` | "The two files are identical." — no thousands of matching rows |
| 4.8 | `binary.dat` against anything | Says it is not text on both sides and that the bytes differ |
| 4.9 | Two directories | Refused with a reason |
| 4.10 | `q` | Closes |

## 5. Split view, and sending between the panes (v0.1.0, `<A-c>` / `<A-m>` v0.2.0)

Two panes and the keys that move files between them. `<A-c>` and `<A-m>` are the
only commands in the program whose destination is a *pane* rather than the
clipboard or the cursor, so most of this section is about what they refuse.

Needs two directories with different contents — `many\` and `repo\` will do.

| # | Do | Expect |
| --- | --- | --- |
| 5.1 | `<C-w>` once | Two panes side by side. The **parent column is gone**; the layout is pane, pane, preview, the same width as before |
| 5.2 | `<C-w>` again, and again | The keys move between the panes. The same key both splits and switches |
| 5.3 | Look at the two cursor rows | The pane without the keys is **dimmer**. That is the only thing saying where typing will land |
| 5.4 | `h` `j` `k` `l`, `cd`, `/`, `s` while split | All act on the focused pane and nothing else. No command needs to know about panes |
| 5.5 | `<A-c>` with a file under the cursor and nothing selected | It is copied to whatever directory the **other** pane is showing |
| 5.6 | `<Space>` a few files, then `<A-c>` | All of them go, and **the selection is cleared afterwards** — unlike `y`, which keeps it |
| 5.7 | `<A-m>` instead | The files move: gone from this pane, present in the other |
| 5.8 | Put both panes in the **same** directory, then `<A-c>` | Refused, saying so. It would otherwise copy each file beside itself under a new name |
| 5.9 | `<C-S-w>`, then `<A-c>` | "Open the second pane first (`<C-w>`)" — not silence |
| 5.10 | `y`, `<C-w>`, `p` | The ordinary route works too, and is the one that can paste somewhere neither pane is |
| 5.11 | `[` / `]` / `1`–`9` while split | Tabs still switch. Switching to the tab the other pane shows just moves the keys there |
| 5.12 | Close the tab the **other** pane is holding (`<C-c>`) | The split ends; one pane, no stale second |
| 5.13 | `<C-w>` with only one tab open | A second tab is made on the same directory. With several already open, the next one is borrowed instead |
| 5.14 | `<C-S-w>` | Back to one pane, and the parent column returns |
| 5.15 | `<A-c>` a large directory, then watch the status bar | It is a job like any other copy: progress, speed, and cancellable from `w` |

## 6. The config paths in the help panel (v0.25.0)

| # | Do | Expect |
| --- | --- | --- |
| 6.1 | `~` with no `filer.toml` anywhere | **Both** directories are listed, the empty one marked `nothing here`. Before v0.25.0 only files that existed were shown |
| 6.2 | Hover a path | The row lights up and the pointer becomes a hand |
| 6.3 | Hover a key row | Nothing happens — it is not a link |
| 6.4 | Click a config **file** | The panel closes, the list opens its directory with that file under the cursor. `<Enter>` then opens it |
| 6.5 | Click a **directory** | The panel closes and the list goes there, empty or not |
| 6.6 | Click the empty one, then create `filer.toml` there and `<C-F5>` | It appears in the panel next time, without `nothing here` |
| 6.7 | With `YAZI_CONFIG_HOME` / `FILER_CONFIG_HOME` set | The listed directories follow them |
| 6.8 | A config warning line | Still yellow, and not clickable |

## 7. Which shell the pane runs (v0.24.0)

The setting is one line; the point of the section is that the **default** is the
thing that surprises people.

| # | Do | Expect |
| --- | --- | --- |
| 7.1 | `<C-t>` with no `[term]` in `filer.toml`, then `$PSVersionTable.PSVersion` | `5.1.x` — Windows PowerShell, unchanged from every earlier version |
| 7.2 | Add `[term]` / `shell = "pwsh"`, `<C-S-t>`, `<C-t>`, ask again | `7.x` |
| 7.3 | `$PROFILE` in each | Two different paths — `WindowsPowerShell\` for 5.1, `PowerShell\` for 7 |
| 7.4 | With the OSC 7 hook in the pwsh profile only, `cd` and `<A-Up>` under each | Works under `pwsh`, and says so under 5.1. That asymmetry is the whole bug report |
| 7.5 | `args = ["-NoLogo"]` | The banner is gone |
| 7.6 | A `shell` that is not installed | It fails to start and says so — no silent empty pane |
| 7.7 | Remove `[term]` again, `<C-S-t>`, `<C-t>` | Back to the default |

## 8. The outline at the end of a file (v0.23.1)

Needs a document that **ends on a heading** with little under it — `TESTING-KEYS.md`
is one. A document whose last heading has pages of text after it will not show this
at all, which is what made it look intermittent.

| # | Do | Expect |
| --- | --- | --- |
| 8.1 | Focus the outline (`<C-o>` or `l` on the file), `G` or hold `↓` to the last entry | The preview stops at the end of the file. **No flicker, no half-drawn frames** |
| 8.2 | Keep holding `↓` there for a few seconds | Nothing moves and nothing flashes. Until v0.23.1 this was one bad frame per repeat |
| 8.3 | The same on a document whose last heading has plenty of text after it | Unchanged — it was always correct here |
| 8.4 | `<A-j>` held at the bottom of a long file | Still steady; this path was fixed earlier and must stay that way |
| 8.5 | Move back up the outline | Each entry lands on its own line again, not on the clamped one |

## 9. The yank register, said out loud (v0.23.0)

The marker bar cannot carry this on its own, which is what the section is for.

| # | Do | Expect |
| --- | --- | --- |
| 9.1 | `y` on a file with nothing selected | A **green** bar on the row, and `1 copied` in the header |
| 9.2 | `<Space>` on that same file | The bar turns **yellow** — the selection's colour wins, by design — and the header reads `1 selected · 1 copied` |
| 9.3 | `x` instead of `y` | A **red** bar, and `1 cut` |
| 9.4 | With something copied, `h` / `l` to another directory | `1 copied` is still in the header, with no row to show it |
| 9.5 | `p` after a copy | The register stays: `1 copied` is still there, and `p` again pastes again |
| 9.6 | `p` after a cut | The register empties; the count leaves the header |
| 9.7 | `X` or `Y` | The count leaves the header |
| 9.8 | The status line, bottom right | Says the same thing in the same words as the header |

## 10. Bulk rename (v0.4.0)

In `bulk-rename\`.

| # | Do | Expect |
| --- | --- | --- |
| 10.1 | Select the 12 `IMG_*.jpg`, press `R` | A prompt reading `{name}{ext}`, and a panel above it listing every file with `→` and its new name — unchanged, since that rule changes nothing |
| 10.2 | Type `holiday-{n:2}{ext}` | The panel updates **as you type**, showing `holiday-01.jpg` … `holiday-12.jpg` |
| 10.3 | Enter | All twelve renamed. A toast says how many |
| 10.4 | `u` | **All twelve names back**, in one step |
| 10.5 | `U` | Renamed again |
| 10.6 | Select the twelve, `R`, type `in the way.txt` | Every row after the first is marked "two files would get this name", and Enter is refused |
| 10.7 | Select one file, `R`, type `in the way.txt` | Marked "already in this directory", Enter refused |
| 10.8 | Select `ab.txt` and `ba.txt`, `R`, type `s/^([ab])([ab])/$2$1/` | The panel shows `ab.txt → ba.txt` and `ba.txt → ab.txt`, **neither marked as a problem** |
| 10.9 | Enter | **The names swap.** A loop of `mv` fails on the second rename; this moves one out of the way first. Then `u` puts both back |
| 10.9b | Select `ab.txt` and `in the way.txt`, `R`, type `s/^ab/in the way/` | `ab.txt → in the way.txt` is marked "already in this directory": the file holding that name is selected but is **not moving**, so the name is not going spare. (Before v0.7.0 this was allowed, and failed at the last moment instead) |
| 10.10 | `R` with `s/IMG_(\d+)/photo-$1/` | Group references work |
| 10.11 | `R` with `{nope}` | Says the placeholder is unknown; nothing renamed |

## 11. Undo and redo (v0.3.0)

| # | Do | Expect |
| --- | --- | --- |
| 11.1 | `d` on a file in `many\` | It goes to the recycle bin |
| 11.2 | `u` | It comes back, in its original place. A toast says so |
| 11.3 | Check the task panel (`w`) during F2 | A `Restore` row appears and completes |
| 11.4 | `U` | Deleted again |
| 11.5 | Delete two files with the same name from different folders, an interval apart, then `u` | The one just deleted comes back — not the older one |
| 11.6 | `r` to rename, then `u` | The old name is back |
| 11.7 | `u` with nothing to undo | "Nothing to undo" — no error |
| 11.8 | Rename a file, undo it, then create a new file, then `U` | Redo is gone: the new action forked history |
| 11.9 | Delete a file, `u`, but create a file with that name first | `u` says the name is taken, and pressing it again after moving that file out of the way works |
| 11.10 | Open a file in another program so it is locked, select it **with several others**, `d` (v0.27.1) | The others go. The message **names the one that did not**, and the task panel's count matches what actually went. Until v0.27.1 it said `Trash: trash: Error … Some operations were aborted` naming nothing, and counted them all as done |
| 11.11 | `d` on a drive whose Recycle Bin is turned off | Same shape of message, naming the file |
| 11.12 | `d` with nothing locked | Unchanged, and still **one** entry in Explorer's own undo — the batch call is still the normal path |

## 28. Symlinks and `g`+`f` (v0.26.8)

Windows makes these awkward to create. A **junction** needs no admin rights:
`mklink /J linktest C:\dev` from `cmd`. A symlink to a *file* needs an elevated
shell or developer mode: `New-Item -ItemType SymbolicLink -Path l.md -Target
C:\dev\filer\README.md`. There are real ones under `C:\Users\<you>\` if you
would rather not make any.

| # | Do | Expect |
| --- | --- | --- |
| 28.1 | Look at a link's row | `->` after the name. With `m`+`p` the type column reads `l` |
| 28.2 | `g`+`f` on a link to a **directory** | The list goes into the target |
| 28.3 | `g`+`f` on a link to a **file** | The list goes to the target's directory with the file under the cursor; `<Enter>` then opens it |
| 28.4 | `g`+`f` on a **broken** link | `Broken link: <name>` in red |
| 28.5 | `g`+`f` on an ordinary file (v0.26.8) | `Only a symlink can be followed — a link shows -> after its name`. Until v0.26.8 nothing happened at all, which was indistinguishable from an unbound key |
| 28.6 | `g`+`f` in an empty directory | Nothing, and no message — there is no row to say anything about |
| 28.7 | A junction (`mklink /J`), not just a symlink | Treated the same: `->`, and `g`+`f` follows it |

## 27. The parent column, with the mouse (v0.26.7)

The leftmost column. It draws files and directories the same way, so both have
to answer a click.

| # | Do | Expect |
| --- | --- | --- |
| 27.1 | Click a **directory** there | The list goes into it, as it always has |
| 27.2 | Click a **file** there (v0.26.7) | The list goes up to where that file lives, **with the file under the cursor**. Until v0.26.7 nothing happened at all |
| 27.3 | Then press `<Enter>` | It opens — the cursor really is on it, not merely near it |
| 27.4 | Double-click either | The same as a single click; no second, different meaning |
| 27.5 | Click the row for the directory you are already in | You stay there, and the cursor does not jump about |
| 27.6 | At a drive root, where there is no parent column | Nothing to click, and nothing misbehaves |

## 30. Previewers of your own (v0.30.0)

Needs `pdftoppm` and `ffmpeg` on the `PATH` (`filer env` says), and the two
rules from the README in `filer.toml`. **The end-to-end test runs `sh`, so it is
skipped on Windows — this section is the only coverage of the `cmd` path.**

| # | Do | Expect |
| --- | --- | --- |
| 30.1 | Hover a multi-page PDF | Page one, with `page 1` under it |
| 30.2 | `<A-j>` | Page two. `page 2` under it |
| 30.3 | `<A-k>` | Back to page one |
| 30.4 | `<A-k>` again, on page one | Stays. It does not go to page zero or below |
| 30.5 | Hold `<A-j>` past the last page (v0.30.1) | **The last page stays on screen**, and a line says `No more: …` with the command's own words. Until v0.30.1 the page was replaced by the error |
| 30.5a | `<A-k>` straight after that | Back a page from the last one, not from somewhere past it |
| 30.5b | A **short** video — a few seconds — and `<A-j>` a few times | Same: it stops at the last frame it could draw. This is where it bites, since `step = 10` runs off the end almost at once |
| 30.5c | The caption on a video (v0.30.1) | `50s`, not `s 50` |
| 30.6 | Watch the screen while paging | **No console window flashes.** It runs once per press |
| 30.7 | Page to 5, move to another file, come back | Back at page one: the page belongs to the file |
| 30.8 | Page back to one you have already seen | Instant — it is cached per page |
| 30.9 | Hover a video | The frame at 0s, captioned `s 0` |
| 30.10 | `<A-j>` on it | Ten seconds in, by `step` |
| 30.11 | A PDF with a **space** in its name, and one in a Japanese folder | Both draw. The quoting is filer's, not the rule's |
| 30.12 | Rename `pdftoppm` away, then hover a PDF | An error naming the tool, not a hang |
| 30.13 | Remove the `[[preview]]` rules, `<C-F5>`, hover a PDF | Back to the shell thumbnail, unchanged |
| 30.14 | `filer env` with the rules in place | `pdftoppm` and `ffmpeg` listed under Tools, with `preview *.pdf` beside them |

## 12. Quick look, minimap's neighbours, and the rest of the panes

| # | Do | Expect |
| --- | --- | --- |
| 12.1 | `<F3>` on any file | A large panel over the panes, with the file's name at the top and "Esc to close" |
| 12.2 | With it open, press `j` and `k` | **The list still moves**, and the panel follows down it. This is why it is not an overlay |
| 12.3 | `<A-j>` / `<A-k>` with it open | The panel's content scrolls |
| 12.4 | `<F3>` or `<Esc>` | Closes |
| 12.5 | `<C-w>` | The view splits into two panes; the one with the keys is framed, the other's cursor is dimmed |
| 12.6 | Select files, `<A-c>` | Copied into the other pane |
| 12.7 | Drag files onto the other pane | A frame marks the target, and a label by the pointer says "copy" — `Shift` makes it "move" — **before** you let go |
| 12.8 | `<Tab>` on a file | The spot panel, with the file's details |
| 12.9 | `<S-F10>` or right-click | The context menu, with the openers from your config |
| 12.10 | `<C-S-p>` | The palette, listing every binding; typing filters it |
| 12.11 | `b` then a letter, having saved one with `B` | Jumps there. `'` and the letter does the same |
| 12.12 | `z` | The jump list: bookmarks first, then recent directories with "2h ago" beside them |

## 13. The wheel, over each pane (v0.26.5)

The same arithmetic in three places, so all three have to be tried. A notch
should move about three rows, and a slow turn should move *something* — the bug
was that a gentle turn moved nothing at all.

| # | Do | Expect |
| --- | --- | --- |
| 13.1 | Wheel over the **preview** of a long text file | It scrolls, one notch at a time, without spinning hard. This is the v0.26.5 fix |
| 13.2 | Turn the wheel as slowly as you can over the preview | It still moves. Every fraction counts; nothing is discarded |
| 13.3 | Wheel over the **file list** | The same, and with the split open, over each pane in turn |
| 13.4 | Wheel over the **terminal** pane | Still right — fixed earlier, in v0.20.4, and now sharing the same code |
| 13.5 | Turn one way then straight back | It reverses at once, with no dead travel from a stranded remainder |
| 13.6 | `Ctrl` and the wheel over an image | Zooms, and does **not** scroll the pane with the same turn |
| 13.7 | Move the pointer between panes mid-turn | Neither jumps: each keeps its own remainder |

## 14. Configuration and theming

| # | Do | Expect |
| --- | --- | --- |
| 14.1 | With filer open, edit `theme.toml` (change `[mgr] cwd` to something loud) and press `<C-F5>` | The color changes without restarting |
| 14.2 | Change `[ui] font_size` in `filer.toml`, `<C-F5>` | The text resizes |
| 14.3 | Add a `keymap.toml` binding, `<C-F5>` | The new key works, and `<F1>` lists it |
| 14.4 | Sort with `,s`, then `<C-F5>` | The sort **stays** as you set it — a reload does not undo what you changed by hand |
| 14.5 | Put a syntax error in `filer.toml`, `<C-F5>` | An error toast naming the problem; the old config stays in force |
| 14.6 | `[ui] minimap = false`, `<C-F5>` | No minimap |

## 15. Archives (v0.2.0)

| # | Do | Expect |
| --- | --- | --- |
| 15.1 | Hover `sample.zip` | The preview lists what is inside |
| 15.2 | `e` on it | Unpacked into a `sample` folder beside it; progress in the task panel |
| 15.3 | `e` again | The second one gets a different name; the first is not overwritten |
| 15.4 | Select `to-pack\`, press `E`, accept `to-pack.zip` | Packed, and the result opens |
| 15.5 | `E` and change the name to end in `.tar.gz` | A gzipped tar, not a zip |
| 15.6 | `e` on a text file | A toast says it was skipped; nothing else happens |
| 15.7 | `E` and change the name to end in **`.7z`** (v0.27.0) | A real 7z. Until v0.27.0 this was refused as read-only |
| 15.8 | `e` on that `.7z` | It unpacks, and the files match what went in |
| 15.9 | Open the same `.7z` in 7-Zip or Explorer | It opens there too — the point of the format is that it travels |
| 15.10 | Pack a folder holding subfolders as `.7z`, watch the task panel | The count is of **files**, not folders, and it reaches the total rather than stopping short |
| 15.11 | Compare the `.7z` and the `.zip` of the same input | The 7z is smaller; that is the reason to have it |
| 15.12 | `E` with a name ending in something else (`.rar`) | `Name it .zip, .7z, .tar or .tar.gz to say which format` |

## 16. Editors, at a line (needs the editors installed)

Open a file's outline with `l` or `<S-Tab>`, put the cursor on an entry, press
Enter. Each of these is a skip if the editor is not installed.

| # | Editor | Expect |
| --- | --- | --- |
| 16.1 | 秀丸エディタ | Opens at the outline entry's line |
| 16.2 | サクラエディタ | Same |
| 16.3 | EmEditor | Same |
| 16.4 | Notepad++ | Same |
| 16.5 | メモ帳 | Opens, at the top — it has no line argument, and that is correct |
| 16.6 | VS Code / nvim, if you have them | At the line |

## 17. Network paths (needs a share)

| # | Do | Expect |
| --- | --- | --- |
| 17.1 | `g<Space>`, type `\\server\share` | It opens |
| 17.2 | Copy a file to and from it | Works, with progress |
| 17.3 | Unplug the network mid-listing, or point at a dead host | **The window keeps responding.** An error toast, and the tab goes back where it was |
| 17.4 | Tab-complete a path on the share | The prompt stays responsive; a `…` shows while it waits |

## 18. Awkward names

In `awkward names\`.

| # | Do | Expect |
| --- | --- | --- |
| 18.1 | The CJK names | Drawn correctly, columns lined up (they are two cells wide each) |
| 18.2 | The very long name | Elided in the middle, with the extension still readable |
| 18.3 | `UPPER.TXT` and `upper.txt` | Both listed, both openable |
| 18.4 | Copy the name with a quote in it, `<A-t>` into the terminal | Quoted so the shell sees one word |
| 18.5 | `d` then `u` on the CJK-named file | Comes back under the same name |

## 29. `filer env` (v0.28.0)

Run from a shell, not from inside the app.

| # | Do | Expect |
| --- | --- | --- |
| 29.1 | `filer env` from PowerShell | The four sections print. A release build is a GUI binary, so this is the same `CONOUT$` path `--version` uses — **text actually appears** |
| 29.2 | The Config section | Both directories, each saying what is in it or `nothing here`, and `not here:` listing the rest |
| 29.3 | With a deliberate typo in `keymap.toml` | The warning appears under `Warnings`, its several lines indented under the one key |
| 29.4 | The Tools section | `pdftoppm`, `ffmpeg`, `ffprobe`, `pwsh`, `git` with versions where installed and `not found` where not, each naming what it is for |
| 29.4a | With `[term] shell = "pwsh"` set (v0.29.1) | `pwsh` is the shell listed. Without it, `powershell` — the one that will actually launch, not a guess |
| 29.4b | With openers configured | Each named program is listed with the opener kind it belongs to, found or not |
| 29.4c | An opener naming a **quoted full path** (秀丸, サクラ) | The whole path is resolved, not just up to the first space |
| 29.4d | Watch the screen while `filer env` runs | **No editor or viewer opens.** The programs are looked up on `PATH`, never executed |
| 29.5 | On Windows on ARM with the x64 build | `OS arch` and `Process arch` **disagree** — that disagreement is the whole reason both are printed |
| 29.6 | `filer --help` | `env` is listed under COMMANDS |
| 29.8 | Open filer once, quit, then `filer env` (v0.29.0) | A **Last run** section: the adapter with its backend and device type, and every font file that was loaded |
| 29.9 | On a fresh machine, `filer env` **before** ever opening filer | `not recorded — filer has not opened a window on this machine yet`, not an empty section |
| 29.10 | Name a different font in `filer.toml`, `<C-F5>`, then `filer env` again | The new file is listed; the reload updates the record |
| 29.11 | With no bold face anywhere | `none found; bold is faked by overstriking` — the bold list is separate from the regular one on purpose |
| 29.7 | Double-click `filer.exe` (no console) | Unchanged: the window opens, nothing is printed anywhere |

## 19. Bug report from inside the app (v0.11.0)

`<F12>` builds a URL and hands it to the browser. None of that can be exercised
without a browser, a desktop session and the repository in front of you: the
tests cover the encoding and the shape of the URL, not what GitHub does with it.

| # | Do | Expect |
| --- | --- | --- |
| 19.1 | `<F12>` | The default browser opens GitHub's new-issue form, and a toast says so |
| 19.2 | Look at the form | **Version** and **OS とアーキテクチャ** are already filled in; the rest is empty |
| 19.3 | Compare the filled version against `filer --version` in a terminal | The same string, architecture included |
| 19.4 | Compare the filled OS line against `winver` | Edition, feature update and build all match, UBR included (`Windows 11 Pro 25H2 (build 26200.9457)`) |
| 19.4b | Compare it against the form's own PowerShell snippet | The same facts. Nothing left worth pasting over the top |
| 19.5 | On the ARM64 machine, with the **ARM64** build | OS arch and Process arch both read `aarch64` |
| 19.6 | On the ARM64 machine, with the **x64** build (under emulation) | OS arch `aarch64`, Process arch `x86_64` — **the two disagree, and that is the finding** |
| 19.7 | Submit the report | It posts, and the pre-filled fields survive |
| 19.8 | `<F12>` with no browser set as default (or a broken association) | An error toast naming the failure. **The window keeps working** |
| 19.9 | `<F12>` from the terminal pane (`<C-t>` first) | Nothing: `[term]` passes it to the shell, which is correct |

## 20. The preview that would not arrive (v0.12.0)

A race, not a slow load: the answer reaches the channel and the window goes to
sleep without drawing the frame that would take it out. Only ever seen once, on
a first launch, so reproducing it may take several cold starts.

| # | Do | Expect |
| --- | --- | --- |
| 20.1 | Start filer cold, move to a text file as soon as the listing appears | The preview arrives **without touching anything else** |
| 20.2 | Walk onto a file never opened in this session — a fresh clone, a folder you have not browsed | It appears. **This is the case that was broken: not cold starts, but anything not already cached** |
| 20.3 | Walk off the file and back | Still fine (this always worked — it was the cache) |
| 20.4 | Open an image never seen this session, then zoom with `+` | It steps from the picture's own fit, not from the last image's scale. **The same commit killed this and it has never been exercised** |
| 20.5 | Restart, open ten different files in a row without revisiting any | All ten appear |

## 21. Changes made from outside (v0.12.4)

The watcher's rescan replaces the listing under whatever the cursor is on. Until
v0.12.4 that crashed the program outright when the listing shrank past the
cursor's row, so these are worth running on a real machine rather than trusting
the unit tests alone.

| # | Do | Expect |
| --- | --- | --- |
| 21.1 | Put the cursor on the **last** row, delete that file from Explorer | The row goes, the cursor lands on the new last row, **no crash** |
| 21.2 | Cursor on the last row; delete several files at the end at once | Same |
| 21.3 | Delete every file in the folder from outside | An empty listing, still responsive |
| 21.4 | Cursor on the last row of a **filtered** listing (`f`), delete the file it is on | Same, and the filter still holds |
| 21.5 | Same in the **other pane** (`<C-w>`) and in the **preview** of a directory | Neither crashes |
| 21.6 | Cursor on the last row, delete that file with `d` | Same — this is what Issue #5 reported |
| 21.7 | Rename a file from outside while the cursor is on it | The cursor follows the name or stays put; no crash |

## 22. The terminal's directory, brought back (v0.14.0)

`<A-Up>` in the terminal pane (`term_cd`) asks the shell where it is, which only
works if the shell says so with OSC 7. PowerShell says nothing unless the hook in
the README is in `$PROFILE`, so what is being tested here is mostly the
instructions.

| # | Do | Expect |
| --- | --- | --- |
| 22.1 | With **no** hook in `$PROFILE`, open the terminal (`<C-t>`), `cd` somewhere, press `<A-Up>` | A toast naming OSC 7 and `LocationChangedAction`, pointing at the README — **not** silence, and not a wait |
| 22.2 | Paste the README hook into `$PROFILE`, open a new terminal, `cd C:\dev`, press `<A-Up>` | The file list moves to `C:\dev` |
| 22.3 | Same with a directory whose name has a **space** and one with **Japanese** in it | Both arrive intact |
| 22.4 | `cd` to a UNC path (`\\server\share`) and press `<A-Up>` | Either it follows or it says why; no crash |
| 22.5 | Run the hook line by hand in a shell that already has Starship | The prompt still draws normally (the hook uses `LocationChangedAction`, not `prompt`) |

## 23. Right-click paste in a prompt (v0.14.0)

| # | Do | Expect |
| --- | --- | --- |
| 23.1 | Copy a path in Explorer's address bar, press `c`+`d` (or whatever opens the `cd` prompt), right-click the field | The path appears; `<Enter>` goes there |
| 23.2 | Type `abc`, click between `a` and `b` with the **right** button | The paste lands there, not at the end |
| 23.3 | Select part of the text with a drag, then right-click **on the selection** | The selection is replaced |
| 23.4 | Copy two lines of text, right-click into `s` | One line, the break shown as a space — the same as `<C-v>` |
| 23.5 | Copy a Japanese path, right-click into `cd` | Intact, and the caret sits after it |
| 23.6 | With an image (not text) on the clipboard, right-click a prompt | Nothing happens, **no toast** |
| 23.7 | Same in the command palette, in `f`, and in `S-r` (bulk rename) | Each pastes; the bulk preview re-renders |
| 23.8 | Right-click in the **file list** | Still the context menu — the list is unchanged |
| 23.9 | Right-click in the **terminal** pane (`<C-t>`) | The clipboard is typed in, and the pane takes the keys if it did not have them |
| 23.10 | Select text in the terminal with a drag, then right-click | The selection was copied on release; the right-click pastes it back — select to copy, right-click to paste |
| 23.11 | Copy **three lines** and right-click into the terminal at a PowerShell prompt | All three sit in the buffer, **nothing runs** until `<Enter>` (PSReadLine asks for bracketed paste) |
| 23.12 | The same in a shell that does **not** ask for bracketed paste (`cmd.exe`) | The lines run, as they always have — and no stray `[200~` appears |
| 23.13 | Right-click in the terminal while `vim` is open | The text is inserted; no `[200~` on screen |
| 23.14 | `<C-v>` in the terminal | Same as the right-click, including 23.11 |

## 24. A host's shares (v0.16.0)

Only testable against a real file server, and the interesting cases are the
ones where it says no.

| # | Do | Expect |
| --- | --- | --- |
| 24.1 | `g`+`<Space>`, type `\\10.0.0.1`, `<Enter>` | The shares are listed, the same ones Explorer shows |
| 24.2 | Same with a host **name** rather than an address, and with `//10.0.0.1` | Both arrive; the path is shown back as `\\10.0.0.1` |
| 24.3 | Walk into a share and back out with `h` | Into the share, then back to the host list |
| 24.4 | `h` again, at the host | Nothing moves (the host is the top), no crash |
| 24.5 | A host that is off, or does not exist (`\\10.0.0.99`) | The tab returns to where it was and a toast says why — it does not hang the window |
| 24.5a | 24.1 and 24.5 again, watching for a **toast** | v0.16.0 fell back to the parent in silence, so a failure looked like nothing happening. Whatever the outcome, there is now either a listing or a message; if it is still a message, its os error number is the thing to report |
| 24.6 | A host that needs a login the machine has not been given | Same: a refusal as a toast, naming it |
| 24.7 | A host with **many** shares (more than a screenful) | All of them, scrolling normally |
| 24.8 | A share name with a space or non-ASCII in it | Intact |
| 24.9 | Hover a share and look at the size column | Empty — there is nothing to read, and it must not sit there counting |
| 24.10 | `<C-r>` / refresh on the host listing | Re-asks the server; no crash |
| 24.11 | Open the host in the **other pane** (`<C-w>`) and in a second tab | Both fine |
| 24.12 | Go to a host, then change directory away | The watcher does not complain about the host it could not watch |

## 25. Openers (v0.17.0)

The README's example config is the thing under test: if a step here fails, the
instructions are wrong, which is worse than a missing feature.

| # | Do | Expect |
| --- | --- | --- |
| 25.1 | Paste the README's `[opener]` / `[open]` example into `yazi.toml`, restart, `<S-Enter>` on a `.txt` | 秀丸 / サクラ / VS Code / Neovim / default — with the descriptions, not the command lines |
| 25.2 | `<Enter>` on the same file | Opens in the first entry (秀丸), no console flash |
| 25.3 | `<S-Enter>` on a `.pdf` | Edge and Chrome first, then the default-app entry |
| 25.4 | `<S-Enter>` on a `.xlsx`, pick Excel | Excel opens it — this is the `start ""` case that fails without it |
| 25.5 | A file whose name has a **space**, through each of the above | One argument, opens correctly |
| 25.6 | Several files selected, then `<Enter>` | All of them go to one invocation |
| 25.7 | A rule written `*.{xlsx,xls,csv}` | Matches all three (this is what did not work before v0.17.0) |
| 25.8 | An opener naming a program that is not installed | An error toast within a few seconds, no hang |
| 25.8a | An opener whose program is a **quoted full path** (秀丸, サクラ) | It opens. This is the v0.17.0 bug: `cmd` mangled the line and the failure was silent |
| 25.8b | 秀丸 and サクラ from `<S-Enter>` **and** from `<Enter>` as the first entry | Both, since they take different code paths to the same launcher |
| 25.8c | An opener with a deliberate typo in the path | A toast naming the failure. On a Japanese Windows expect the exit code rather than `cmd`'s own words — that is intended, not a bug to report |
| 25.9 | Open from the outline (`<C-o>` at a line) into 秀丸 and サクラ | Lands on the line |

---

## 26. Config warnings, and their colour (v0.20.1)

A warning here means a line in your own `keymap.toml` cannot take effect. The
colour is the thing under test: red is reserved for something that failed, and
none of these failed.

| # | Do | Expect |
| --- | --- | --- |
| 26.1 | Start with a `keymap.toml` that binds a key the defaults also bind (e.g. `'` to `plugin bookmarks jump`) | A **yellow** toast, not red: `Config: [mgr] \`'\` is bound more than once; only ... runs` |
| 26.2 | With three or more such lines | The toast ends `(+2 more, see \`~\`)` |
| 26.3 | Press `~` | The loaded config files, then every warning, all in the same yellow |
| 26.4 | Make something actually fail (an opener naming a program that is not installed, 25.8) | Still **red**, so the two are told apart at a glance |
| 26.5 | Remove the duplicate lines, `<C-F5>` | `Reloaded N config file(s)` in the plain colour; no yellow |
| 26.6 | A theme with a light background | The yellow is still readable; say so if it is not — it is a fixed default, not yet themeable |

---

## Known gaps in this checklist

- **Nothing here has been run.** The checklist was written from the code, not
  from use; a step that does not match the program may be the checklist's
  mistake rather than the program's. Say so if a step reads wrong.
- **macOS and Linux are unexercised.** They compile, and the undo of a delete is
  known not to work on macOS (no API for reading the Trash back), but no one has
  run the program there at all.
- **Automated screenshot testing was looked at and not adopted.** egui ships
  `egui_kittest`, which renders offscreen and compares against baseline images,
  and it would cover most of sections B, C, D and G. It needs a GPU adapter,
  which the development container has none of (no Vulkan driver, no EGL), so the
  baselines cannot be produced there — they would have to be generated on
  Windows and committed. It is worth doing; it is not something that can be set
  up blind.
