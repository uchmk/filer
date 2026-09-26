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
happened instead, and a screenshot where it is something visual. Versions matter
— say which `filer.exe` (the release tag, or the commit the artifact is named
for). A check that cannot be run at all (no editor installed, no network share)
is a skip, not a failure; say which.

---

## A. The terminal pane

The riskiest area: an embedded terminal is a lot of machinery and none of its
drawing has been seen. `<C-t>` opens it.

| # | Do | Expect |
| --- | --- | --- |
| A1 | `<C-t>` from the file list | A shell opens along the bottom, already in the directory the list is showing |
| A2 | Type `dir` and press Enter | Output in the list's own font, columns lined up, no overlapping glyphs |
| A3 | Look at the cursor | A block where the shell's cursor is, and it moves as you type |
| A4 | Run something colorful (`git status` in the `repo` fixture) | The 16 ANSI colors, and they match the file list's own colors rather than looking like a second palette |
| A5 | **`<C-t>` again** | Keys go back to the list — **and the shell is still there**, with its output intact. This is the v0.6.0 fix; before it, this ended the shell |
| A6 | `<C-t>`, `<C-t>`, `<C-t>` a few times | The same shell throughout. The scrollback never resets |
| A7 | `<C-S-t>` | *Now* the pane closes and the shell ends |
| A8 | Reopen, then resize the window | The grid reflows; no clipped half-columns, no stretched text |
| A9 | `dir` in `many\` to fill the screen, then `<S-PageUp>` | The view goes back; a note says how many lines back you are |
| A10 | `<S-End>`, then type a character | Back at the bottom, and typing alone would have done it |
| A11 | Drag across some output | It highlights, and is on the clipboard when you let go — no second step |
| A12 | Double-click a word | The word is selected |
| A13 | `<C-S-f>`, type a word from the scrollback, Enter, then `<C-S-n>` | Matches are found and stepped through; it wraps at the end |
| A14 | `<F1>` inside the terminal | The key list opens **over** the terminal. `<Esc>` closes it and typing goes back to the shell |
| A15 | `<C-S-p>` inside the terminal | The command palette opens, and running something from it works |
| A16 | `cd` somewhere in the shell, then `<A-Up>` | The file list follows to where the shell is |
| A17 | Select two files, `<A-t>` | Their paths are typed onto the shell's line, quoted, **not run** |
| A18 | With a shell that reports OSC 7 (PowerShell 7, or bash with a `PROMPT_COMMAND`), change directory in the list | No stray `cd` is typed into the shell |

## B. The minimap (v0.5.0)

Open `long.rs` — 4000 lines, shaped so the bands should be recognisable.

| # | Do | Expect |
| --- | --- | --- |
| B1 | Hover `long.rs` | A narrow strip down the right of the preview, made of short horizontal bars |
| B2 | Look at the shape | Comment headers read as long bars, indented blocks as bars starting further right, the blank line every 40 as a gap. It should look like the file |
| B3 | Look at the colors | The bars carry syntax colors — strings and comments differ from code — not one flat color |
| B4 | Find the viewport box | A lighter box with a border, covering the part of the file on screen |
| B5 | `<A-j>` a few times | The box moves down in step with the text |
| B6 | Click halfway down the strip | The preview jumps there, with the clicked line in the **middle** of the pane, not at its top |
| B7 | Drag up and down the strip | The preview follows continuously |
| B8 | Narrow the window until the preview is thin | The map disappears before the text becomes unreadable, and the text takes the space back |
| B9 | `<A-n>` | The map toggles off and on |
| B10 | Open `notes.md` (rendered) | **No map** — this is deliberate, the rendered lines are not the file's lines |
| B11 | Press `M` for source | The map appears |
| B12 | A short file (`same-a.txt`) | No map: two lines are not worth mapping |

## C. Image zoom and pan (v0.5.0)

`zoom-me.png` is 3200×2400 with an 8-pixel grid, so blur is obvious.

| # | Do | Expect |
| --- | --- | --- |
| C1 | Hover `zoom-me.png` | It fits the pane. The caption reads `3200 × 2400 · fit NN%` |
| C2 | `<A-1>` (1:1) | It fills far more than the pane, showing the middle. **The grid lines are crisp** — this is the re-decode working; if it is a blurred enlargement of the fitted copy, that is the bug this was built to avoid |
| C3 | Watch the moment it sharpens | The picture must **not jump or change size** when the sharper copy arrives. Only its sharpness changes |
| C4 | Drag it | It pans, and stops when its edge reaches the pane's edge — it cannot be thrown off screen |
| C5 | `Ctrl` and the wheel, pointer on a grid intersection | It zooms **about the pointer**: the intersection under the cursor stays under it |
| C6 | Plain wheel (no Ctrl) | Scrolls the pane, does not zoom |
| C7 | Double-click | Back to fitting, centred |
| C8 | `<A-i>` / `<A-o>` | In and out in steps. The caption's percentage follows |
| C9 | Zoom in, then `j` to the next file and back | It is fitted again — a zoom belongs to the file it was set on |
| C10 | Hover `tiny.png` (48×48) | Shown at its own size, **not blown up** to fill the pane |

## D. Compare, side by side (v0.4.0)

`<A-d>` with `compare-left.txt` and `compare-right.txt` both selected, or one in
each pane with the view split.

| # | Do | Expect |
| --- | --- | --- |
| D1 | Compare the two | Two columns, a line down the middle, with line numbers on each side |
| D2 | Find line 100 | The changed line sits **opposite** the line it replaced — not listed as a removal and an addition far apart |
| D3 | Look at the colors | Removals tinted on the left, additions on the right, matching the git signs' colors |
| D4 | Look at the line numbers after the insertion | The two sides differ by one, each counting its own file |
| D5 | `n` / `N` | Between the two differences, not line by line inside one |
| D6 | `j` `k` `<C-d>` `gg` `G` | Scrolling, with the footer's `x–y of z` keeping up |
| D7 | `same-a.txt` and `same-b.txt` | "The two files are identical." — no thousands of matching rows |
| D8 | `binary.dat` against anything | Says it is not text on both sides and that the bytes differ |
| D9 | Two directories | Refused with a reason |
| D10 | `q` | Closes |

## E. Bulk rename (v0.4.0)

In `bulk-rename\`.

| # | Do | Expect |
| --- | --- | --- |
| E1 | Select the 12 `IMG_*.jpg`, press `R` | A prompt reading `{name}{ext}`, and a panel above it listing every file with `→` and its new name — unchanged, since that rule changes nothing |
| E2 | Type `holiday-{n:2}{ext}` | The panel updates **as you type**, showing `holiday-01.jpg` … `holiday-12.jpg` |
| E3 | Enter | All twelve renamed. A toast says how many |
| E4 | `u` | **All twelve names back**, in one step |
| E5 | `U` | Renamed again |
| E6 | Select the twelve, `R`, type `in the way.txt` | Every row after the first is marked "two files would get this name", and Enter is refused |
| E7 | Select one file, `R`, type `in the way.txt` | Marked "already in this directory", Enter refused |
| E8 | Select `ab.txt` and `ba.txt`, `R`, type `s/^([ab])([ab])/$2$1/` | The panel shows `ab.txt → ba.txt` and `ba.txt → ab.txt`, **neither marked as a problem** |
| E9 | Enter | **The names swap.** A loop of `mv` fails on the second rename; this moves one out of the way first. Then `u` puts both back |
| E9b | Select `ab.txt` and `in the way.txt`, `R`, type `s/^ab/in the way/` | `ab.txt → in the way.txt` is marked "already in this directory": the file holding that name is selected but is **not moving**, so the name is not going spare. (Before v0.7.0 this was allowed, and failed at the last moment instead) |
| E10 | `R` with `s/IMG_(\d+)/photo-$1/` | Group references work |
| E11 | `R` with `{nope}` | Says the placeholder is unknown; nothing renamed |

## F. Undo and redo (v0.3.0)

| # | Do | Expect |
| --- | --- | --- |
| F1 | `d` on a file in `many\` | It goes to the recycle bin |
| F2 | `u` | It comes back, in its original place. A toast says so |
| F3 | Check the task panel (`w`) during F2 | A `Restore` row appears and completes |
| F4 | `U` | Deleted again |
| F5 | Delete two files with the same name from different folders, an interval apart, then `u` | The one just deleted comes back — not the older one |
| F6 | `r` to rename, then `u` | The old name is back |
| F7 | `u` with nothing to undo | "Nothing to undo" — no error |
| F8 | Rename a file, undo it, then create a new file, then `U` | Redo is gone: the new action forked history |
| F9 | Delete a file, `u`, but create a file with that name first | `u` says the name is taken, and pressing it again after moving that file out of the way works |

## G. Quick look, minimap's neighbours, and the rest of the panes

| # | Do | Expect |
| --- | --- | --- |
| G1 | `<F3>` on any file | A large panel over the panes, with the file's name at the top and "Esc to close" |
| G2 | With it open, press `j` and `k` | **The list still moves**, and the panel follows down it. This is why it is not an overlay |
| G3 | `<A-j>` / `<A-k>` with it open | The panel's content scrolls |
| G4 | `<F3>` or `<Esc>` | Closes |
| G5 | `<C-w>` | The view splits into two panes; the one with the keys is framed, the other's cursor is dimmed |
| G6 | Select files, `<A-c>` | Copied into the other pane |
| G7 | Drag files onto the other pane | A frame marks the target, and a label by the pointer says "copy" — `Shift` makes it "move" — **before** you let go |
| G8 | `<Tab>` on a file | The spot panel, with the file's details |
| G9 | `<S-F10>` or right-click | The context menu, with the openers from your config |
| G10 | `<C-S-p>` | The palette, listing every binding; typing filters it |
| G11 | `b` then a letter, having saved one with `B` | Jumps there. `'` and the letter does the same |
| G12 | `z` | The jump list: bookmarks first, then recent directories with "2h ago" beside them |

## H. Configuration and theming

| # | Do | Expect |
| --- | --- | --- |
| H1 | With filer open, edit `theme.toml` (change `[mgr] cwd` to something loud) and press `<C-F5>` | The color changes without restarting |
| H2 | Change `[ui] font_size` in `filer.toml`, `<C-F5>` | The text resizes |
| H3 | Add a `keymap.toml` binding, `<C-F5>` | The new key works, and `<F1>` lists it |
| H4 | Sort with `,s`, then `<C-F5>` | The sort **stays** as you set it — a reload does not undo what you changed by hand |
| H5 | Put a syntax error in `filer.toml`, `<C-F5>` | An error toast naming the problem; the old config stays in force |
| H6 | `[ui] minimap = false`, `<C-F5>` | No minimap |

## I. Archives (v0.2.0)

| # | Do | Expect |
| --- | --- | --- |
| I1 | Hover `sample.zip` | The preview lists what is inside |
| I2 | `e` on it | Unpacked into a `sample` folder beside it; progress in the task panel |
| I3 | `e` again | The second one gets a different name; the first is not overwritten |
| I4 | Select `to-pack\`, press `E`, accept `to-pack.zip` | Packed, and the result opens |
| I5 | `E` and change the name to end in `.tar.gz` | A gzipped tar, not a zip |
| I6 | `e` on a text file | A toast says it was skipped; nothing else happens |

## J. Editors, at a line (needs the editors installed)

Open a file's outline with `l` or `<S-Tab>`, put the cursor on an entry, press
Enter. Each of these is a skip if the editor is not installed.

| # | Editor | Expect |
| --- | --- | --- |
| J1 | 秀丸エディタ | Opens at the outline entry's line |
| J2 | サクラエディタ | Same |
| J3 | EmEditor | Same |
| J4 | Notepad++ | Same |
| J5 | メモ帳 | Opens, at the top — it has no line argument, and that is correct |
| J6 | VS Code / nvim, if you have them | At the line |

## K. Network paths (needs a share)

| # | Do | Expect |
| --- | --- | --- |
| K1 | `g<Space>`, type `\\server\share` | It opens |
| K2 | Copy a file to and from it | Works, with progress |
| K3 | Unplug the network mid-listing, or point at a dead host | **The window keeps responding.** An error toast, and the tab goes back where it was |
| K4 | Tab-complete a path on the share | The prompt stays responsive; a `…` shows while it waits |

## L. Awkward names

In `awkward names\`.

| # | Do | Expect |
| --- | --- | --- |
| L1 | The CJK names | Drawn correctly, columns lined up (they are two cells wide each) |
| L2 | The very long name | Elided in the middle, with the extension still readable |
| L3 | `UPPER.TXT` and `upper.txt` | Both listed, both openable |
| L4 | Copy the name with a quote in it, `<A-t>` into the terminal | Quoted so the shell sees one word |
| L5 | `d` then `u` on the CJK-named file | Comes back under the same name |

## M. Bug report from inside the app (v0.11.0)

`<F12>` builds a URL and hands it to the browser. None of that can be exercised
without a browser, a desktop session and the repository in front of you: the
tests cover the encoding and the shape of the URL, not what GitHub does with it.

| # | Do | Expect |
| --- | --- | --- |
| M1 | `<F12>` | The default browser opens GitHub's new-issue form, and a toast says so |
| M2 | Look at the form | **Version** and **OS とアーキテクチャ** are already filled in; the rest is empty |
| M3 | Compare the filled version against `filer --version` in a terminal | The same string, architecture included |
| M4 | Compare the filled OS line against `winver` | Edition, feature update and build all match, UBR included (`Windows 11 Pro 25H2 (build 26200.9457)`) |
| M4b | Compare it against the form's own PowerShell snippet | The same facts. Nothing left worth pasting over the top |
| M5 | On the ARM64 machine, with the **ARM64** build | OS arch and Process arch both read `aarch64` |
| M6 | On the ARM64 machine, with the **x64** build (under emulation) | OS arch `aarch64`, Process arch `x86_64` — **the two disagree, and that is the finding** |
| M7 | Submit the report | It posts, and the pre-filled fields survive |
| M8 | `<F12>` with no browser set as default (or a broken association) | An error toast naming the failure. **The window keeps working** |
| M9 | `<F12>` from the terminal pane (`<C-t>` first) | Nothing: `[term]` passes it to the shell, which is correct |

## N. The preview that would not arrive (v0.12.0)

A race, not a slow load: the answer reaches the channel and the window goes to
sleep without drawing the frame that would take it out. Only ever seen once, on
a first launch, so reproducing it may take several cold starts.

| # | Do | Expect |
| --- | --- | --- |
| N1 | Start filer cold, move to a text file as soon as the listing appears | The preview arrives **without touching anything else** |
| N2 | Walk onto a file never opened in this session — a fresh clone, a folder you have not browsed | It appears. **This is the case that was broken: not cold starts, but anything not already cached** |
| N3 | Walk off the file and back | Still fine (this always worked — it was the cache) |
| N4 | Open an image never seen this session, then zoom with `+` | It steps from the picture's own fit, not from the last image's scale. **The same commit killed this and it has never been exercised** |
| N5 | Restart, open ten different files in a row without revisiting any | All ten appear |

## O. Changes made from outside (v0.12.4)

The watcher's rescan replaces the listing under whatever the cursor is on. Until
v0.12.4 that crashed the program outright when the listing shrank past the
cursor's row, so these are worth running on a real machine rather than trusting
the unit tests alone.

| # | Do | Expect |
| --- | --- | --- |
| O1 | Put the cursor on the **last** row, delete that file from Explorer | The row goes, the cursor lands on the new last row, **no crash** |
| O2 | Cursor on the last row; delete several files at the end at once | Same |
| O3 | Delete every file in the folder from outside | An empty listing, still responsive |
| O4 | Cursor on the last row of a **filtered** listing (`f`), delete the file it is on | Same, and the filter still holds |
| O5 | Same in the **other pane** (`<C-w>`) and in the **preview** of a directory | Neither crashes |
| O6 | Cursor on the last row, delete that file with `d` | Same — this is what Issue #5 reported |
| O7 | Rename a file from outside while the cursor is on it | The cursor follows the name or stays put; no crash |

## P. The terminal's directory, brought back (v0.14.0)

`<A-Up>` in the terminal pane (`term_cd`) asks the shell where it is, which only
works if the shell says so with OSC 7. PowerShell says nothing unless the hook in
the README is in `$PROFILE`, so what is being tested here is mostly the
instructions.

| # | Do | Expect |
| --- | --- | --- |
| P1 | With **no** hook in `$PROFILE`, open the terminal (`<C-t>`), `cd` somewhere, press `<A-Up>` | A toast naming OSC 7 and `LocationChangedAction`, pointing at the README — **not** silence, and not a wait |
| P2 | Paste the README hook into `$PROFILE`, open a new terminal, `cd C:\dev`, press `<A-Up>` | The file list moves to `C:\dev` |
| P3 | Same with a directory whose name has a **space** and one with **Japanese** in it | Both arrive intact |
| P4 | `cd` to a UNC path (`\\server\share`) and press `<A-Up>` | Either it follows or it says why; no crash |
| P5 | Run the hook line by hand in a shell that already has Starship | The prompt still draws normally (the hook uses `LocationChangedAction`, not `prompt`) |

## Q. Right-click paste in a prompt (v0.14.0)

| # | Do | Expect |
| --- | --- | --- |
| Q1 | Copy a path in Explorer's address bar, press `c`+`d` (or whatever opens the `cd` prompt), right-click the field | The path appears; `<Enter>` goes there |
| Q2 | Type `abc`, click between `a` and `b` with the **right** button | The paste lands there, not at the end |
| Q3 | Select part of the text with a drag, then right-click **on the selection** | The selection is replaced |
| Q4 | Copy two lines of text, right-click into `s` | One line, the break shown as a space — the same as `<C-v>` |
| Q5 | Copy a Japanese path, right-click into `cd` | Intact, and the caret sits after it |
| Q6 | With an image (not text) on the clipboard, right-click a prompt | Nothing happens, **no toast** |
| Q7 | Same in the command palette, in `f`, and in `S-r` (bulk rename) | Each pastes; the bulk preview re-renders |
| Q8 | Right-click in the **file list** | Still the context menu — the list is unchanged |
| Q9 | Right-click in the **terminal** pane (`<C-t>`) | The clipboard is typed in, and the pane takes the keys if it did not have them |
| Q10 | Select text in the terminal with a drag, then right-click | The selection was copied on release; the right-click pastes it back — select to copy, right-click to paste |
| Q11 | Copy **three lines** and right-click into the terminal at a PowerShell prompt | All three sit in the buffer, **nothing runs** until `<Enter>` (PSReadLine asks for bracketed paste) |
| Q12 | The same in a shell that does **not** ask for bracketed paste (`cmd.exe`) | The lines run, as they always have — and no stray `[200~` appears |
| Q13 | Right-click in the terminal while `vim` is open | The text is inserted; no `[200~` on screen |
| Q14 | `<C-v>` in the terminal | Same as the right-click, including Q11 |

## R. A host's shares (v0.16.0)

Only testable against a real file server, and the interesting cases are the
ones where it says no.

| # | Do | Expect |
| --- | --- | --- |
| R1 | `g`+`<Space>`, type `\\10.0.0.1`, `<Enter>` | The shares are listed, the same ones Explorer shows |
| R2 | Same with a host **name** rather than an address, and with `//10.0.0.1` | Both arrive; the path is shown back as `\\10.0.0.1` |
| R3 | Walk into a share and back out with `h` | Into the share, then back to the host list |
| R4 | `h` again, at the host | Nothing moves (the host is the top), no crash |
| R5 | A host that is off, or does not exist (`\\10.0.0.99`) | The tab returns to where it was and a toast says why — it does not hang the window |
| R5a | R1 and R5 again, watching for a **toast** | v0.16.0 fell back to the parent in silence, so a failure looked like nothing happening. Whatever the outcome, there is now either a listing or a message; if it is still a message, its os error number is the thing to report |
| R6 | A host that needs a login the machine has not been given | Same: a refusal as a toast, naming it |
| R7 | A host with **many** shares (more than a screenful) | All of them, scrolling normally |
| R8 | A share name with a space or non-ASCII in it | Intact |
| R9 | Hover a share and look at the size column | Empty — there is nothing to read, and it must not sit there counting |
| R10 | `<C-r>` / refresh on the host listing | Re-asks the server; no crash |
| R11 | Open the host in the **other pane** (`<C-w>`) and in a second tab | Both fine |
| R12 | Go to a host, then change directory away | The watcher does not complain about the host it could not watch |

## S. Openers (v0.17.0)

The README's example config is the thing under test: if a step here fails, the
instructions are wrong, which is worse than a missing feature.

| # | Do | Expect |
| --- | --- | --- |
| S1 | Paste the README's `[opener]` / `[open]` example into `yazi.toml`, restart, `<S-Enter>` on a `.txt` | 秀丸 / サクラ / VS Code / Neovim / default — with the descriptions, not the command lines |
| S2 | `<Enter>` on the same file | Opens in the first entry (秀丸), no console flash |
| S3 | `<S-Enter>` on a `.pdf` | Edge and Chrome first, then the default-app entry |
| S4 | `<S-Enter>` on a `.xlsx`, pick Excel | Excel opens it — this is the `start ""` case that fails without it |
| S5 | A file whose name has a **space**, through each of the above | One argument, opens correctly |
| S6 | Several files selected, then `<Enter>` | All of them go to one invocation |
| S7 | A rule written `*.{xlsx,xls,csv}` | Matches all three (this is what did not work before v0.17.0) |
| S8 | An opener naming a program that is not installed | An error toast within a few seconds, no hang |
| S8a | An opener whose program is a **quoted full path** (秀丸, サクラ) | It opens. This is the v0.17.0 bug: `cmd` mangled the line and the failure was silent |
| S8b | 秀丸 and サクラ from `<S-Enter>` **and** from `<Enter>` as the first entry | Both, since they take different code paths to the same launcher |
| S8c | An opener with a deliberate typo in the path | A toast naming the failure. On a Japanese Windows expect the exit code rather than `cmd`'s own words — that is intended, not a bug to report |
| S9 | Open from the outline (`<C-o>` at a line) into 秀丸 and サクラ | Lands on the line |

---

## T. Config warnings, and their colour (v0.20.1)

A warning here means a line in your own `keymap.toml` cannot take effect. The
colour is the thing under test: red is reserved for something that failed, and
none of these failed.

| # | Do | Expect |
| --- | --- | --- |
| T1 | Start with a `keymap.toml` that binds a key the defaults also bind (e.g. `'` to `plugin bookmarks jump`) | A **yellow** toast, not red: `Config: [mgr] \`'\` is bound more than once; only ... runs` |
| T2 | With three or more such lines | The toast ends `(+2 more, see \`~\`)` |
| T3 | Press `~` | The loaded config files, then every warning, all in the same yellow |
| T4 | Make something actually fail (an opener naming a program that is not installed, S8) | Still **red**, so the two are told apart at a glance |
| T5 | Remove the duplicate lines, `<C-F5>` | `Reloaded N config file(s)` in the plain colour; no yellow |
| T6 | A theme with a light background | The yellow is still readable; say so if it is not — it is a fixed default, not yet themeable |

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
