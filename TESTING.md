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

Since v0.45.0 that also includes **whole frames**: `ui::harness::Screen` runs the
real drawing code with no window, feeding events through the same `handle_input`
the window uses, and reads back every string and rectangle the frame painted. So
"the status bar counts the files", "the chord did not also type its letter" and
"the comparison drew its signs" are checked by `cargo test` now, and a check
below that reads like one of those is a check that ran. What it cannot see is
colour and glyph shape — a layout that is present but wrong still needs an eye,
which is the last note at the bottom of this file.

## The keys

[TESTING-KEYS.md](TESTING-KEYS.md) is a tickable line per key binding — 226 of them
across nine layers, which is more than anyone tracks in their head while working
through them one at a time. It is generated from the default keymap:

```powershell
cargo run --example make-keycheck
```

Ticks survive regeneration, so a keymap change does not cost the afternoon
already spent. Keys that have left the keymap are listed at the end rather than
dropped, since one that vanished is worth noticing.

**CI checks that it is in sync** (`make-keycheck -- --check`, since v0.45.1), so
the list can be trusted to name the keys the program actually has. It could not
before: when that check went in it found nine keys missing and two descriptions
stale, the `[help]` layer still listing one binding after v0.34.0 gave it five.

The sections below are the other half: behaviour that no single key exercises.

## Working through it

[TESTING-CHECKS.md](TESTING-CHECKS.md) is this file turned into something you can
tick: one line per check, in Japanese, with each section's setup lifted out of its
prose into a block to paste.

```powershell
cargo run --example make-testcheck
```

**This file stays the source of truth** — the generator takes the ids, the section
list and the English from here on every run, and the English is printed beside the
Japanese on every line, so the two cannot drift apart unnoticed. Ticks survive
regeneration, and CI runs `--check` for the same reason it does for the keys.

What it adds is the subtraction: a check that `cargo test` already covers is not
listed as work. A test says which check it stands in for in its doc comment
(`TESTING.md 45.2`), and the generator reads those, so the count at the top is what
is actually left for a person. That is **359 of the 516 below** — the number this
file cannot give you, because from in here every row looks equally undone.

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
section. Sections ran A–Z until v0.26.6 and the letters were spent.

A new section goes wherever it reads best, next to what it is about — and then
**everything after it is renumbered**, along with the check ids inside those
sections. The numbers say where you are in the document; they are not names, and
nothing outside this file should refer to a section by number. Adding a section
at the end to avoid the renumbering is how 11 came to be followed by 28, 27, 32,
31, 30 and then 12, which is what this note exists to prevent.

Versions matter
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
| 1.9i | `<C-S-n>` past the oldest match (v0.57.4) | It starts again from the newest, and a `Wrapped` toast says so. Until v0.57.4 the jump from `433` to `442 lines back` came without a word |
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
| 1.19 | Run something slow in the pane (`sleep 30`) and press `<C-c>` (v0.47.34) | The command stops and the prompt comes back. **filer is still open** — until v0.47.34 this ran `[mgr]` `close`, so the tab went and the last one took filer and the shell with it |
| 1.20 | `<A-t>` on a file with a `'` in its name, in each shell `[term] shell` can name (v0.47.34) | The line is one word the shell can read: `''` doubled for PowerShell, `'\''` for bash, plain `"…"` for cmd. **No `>>` continuation prompt** |
| 1.21 | Walk the list into a directory with a `'` in its name, with the pane open (v0.47.34) | The `cd` lands and the prompt returns. The same quoting as 1.20, on the path filer types for itself |
| 1.22 | With `[term] shell` set to Git Bash's full path, walk the list into an **ordinary** directory — no `'`, no space (v0.48.1) | The `cd` lands and the prompt is in that directory. **Not `bash: cd: R:Tempfiler-fixtures: No such file or directory`** — an unquoted `\` is an escape to a POSIX shell, so until v0.48.1 no ordinary Windows path could be walked into at all. 1.20 and 1.21 both name a `'`, which is why they missed it |
| 1.23 | The same shell, `<A-t>` on a file with an **ordinary** name (v0.48.1) | The path arrives whole, backslashes and all. This shares `quote()` with 1.22 and was only ever inferred from it, never pressed |
| 1.24 | In the pane, run a full-screen TUI — `gh dash`, or `lazygit` (v0.48.2) | It draws: alternate screen, colours, box drawing, its own split panes. Seen once already; this row is for keeping it seen |
| 1.25 | Drive that TUI, then quit it (`j` / `k` to move, `q` to leave) | The keys reach it, and quitting gives the pane back with a working prompt. **Drawing and driving are separate claims** — 1.24 passing says nothing about this one, and a TUI that cannot be left would strand the pane |
| 1.26 | With the pane open, `<C-S-Enter>` (v0.48.4) | The pane takes the window **to the top edge** — no header, no list, only the status bar below it. A third of the height is right for a shell and too little for a full-screen program |
| 1.27 | `<C-S-Enter>` again, from inside the pane, with a TUI running in it | Back to a third. The key has to work **while the terminal holds the keys** — that is the state a TUI puts you in, and the only one where this matters |
| 1.28 | Maximise from the **list** side, then type | The keystrokes go to the pane, not to the hidden list. Maximising hands the pane the keys, because a list nobody can see is not somewhere to aim them |
| 1.29 | Maximise, then `<C-t>` | The keys go back to the list **and the pane returns to a third** in one press. Leaving the pane and giving the window back are the same intent |
| 1.30 | Maximise, then `<C-S-t>` (end the shell) | The pane goes, and the list is drawn full height rather than under a gap. Nothing is left maximised with no pane in it |
| 1.31 | In the pane, `lazygit`, then `?` to open its key list, then `Esc` (v0.48.6) | **The list closes.** Until v0.48.6 it never did, however often `Esc` was pressed — the same in gh-dash, or any tcell program. Windows Terminal is the control: it has always closed there |
| 1.32 | At the pwsh prompt in the pane, type `abc` without Enter, then `Esc` | The line empties. PSReadLine was never affected; this row is there so the change that fixed 1.31 is seen not to have broken it |
| 1.33 | With the bundled ConPTY beside filer.exe, `pwsh -File scripts\keyprobe.ps1 -Query` in the pane (v0.49.0) | The primary DA reply reads `\e[?6c` — filer's own answer, passed through — and no `{up:…}` appears between characters. `\e[?61;6;7;22;23;24;28;32;42c` means the ConPTY built into Windows answered instead: the two files are missing, or not beside filer.exe |
| 1.34 | Then `lazygit` in the pane | It opens on its usual view with **no menu open**. On the ConPTY built into Windows it started with its copy menu showing, a key nobody pressed |
| 1.35 | Run `lazygit` (or any long command) in the pane, then `<C-S-t>` (v0.52.0) | A dialog asks **End the shell?** and names what is running. `n` keeps the shell and the program; `y` ends both, and a toast says **Ended the shell** |
| 1.36 | At a bare prompt with nothing running, `<C-S-t>` | **No dialog**: the pane goes at once, and the toast says **Ended the shell** — so it no longer looks like `<C-t>` merely hiding it |
| 1.37 | With the pane **closed**, select a file and `<A-t>` (v0.57.0) | The pane opens and, once the shell's prompt is up, the quoted path is on its line -- not "The terminal is not open", and not lost to a shell still loading its profile. The keys are in the pane |
| 1.38 | With a shell open, `<C-t>` back to the list, then `<C-S-t>`; then `<C-S-t>` again (v0.67.25, Q53) | The first ends the shell from the list: the pane goes and the toast is **Ended the shell** (with a program running, the **End the shell?** question first, as in 1.35). The second, with no pane left, says **No terminal to close** — it used to do nothing at all |

## 2. The minimap (v0.5.0)

Open `long.rs` — 4000 lines, shaped so the bands should be recognisable.

2.1 and 2.8 are automated (`ui::whole_frame`): that a long file puts bands down
the strip, and that a pane too narrow for them draws none. What is left below is
everything about how the picture *looks* and how it answers the mouse.

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

## 4. SVG, and the text inside it (v0.33.6)

resvg 0.48 replaced usvg's text engine: rustybuzz and ttf-parser out, harfrust
and skrifa in. Shaping is what turns characters into positioned glyphs, so this
is the part of an SVG that could look different without anything here changing.
Nothing in this section can be checked without a screen, and there was no SVG
section at all before this.

`scripts/make-fixtures.ps1` does not make these; any `.svg` will do, and a file
saved from Inkscape or Illustrator is a better test than a hand-written one.

Run once on 2026-09-26 against v0.33.6: SVG and `.ico` both drew, with nothing
reported wrong. That answers "does it render". **4.8 is still open**, and it is
the one that would catch a regression — it needs a v0.33.5 build to hold the new
one against, and none was kept. If one is ever wanted, `build.yml`'s artifact
for commit `f2b30c5` is it, for as long as the 90 days last.

| # | Do | Expect |
| --- | --- | --- |
| 4.1 | Hover an SVG with no text in it (an icon, a logo) | Drawn, scaled to fill the pane, sharp at any pane size |
| 4.2 | An SVG containing **text** | The text is drawn, in the right place, at the right size — **not missing, not boxes, not overlapping** |
| 4.3 | An SVG with **Japanese** text | Same. A font with kana and kanji is picked, rather than the text vanishing |
| 4.4 | An SVG naming a font that is **not installed** | A fallback is used and something readable appears; it does not fail the whole render |
| 4.5 | An SVG with **bold** or *italic* text | The weight and slant are there, not flattened to regular |
| 4.6 | An SVG using a font **file next to it** rather than a system font | Loaded from the directory, as `resources_dir` intends |
| 4.7 | A **malformed** SVG (truncate one) | `bad SVG: …` on the preview, and the window keeps working |
| 4.8 | Compare 4.2 and 4.3 against v0.33.5's build | Any difference in the glyphs is the new shaper; say what changed and attach both |

## 5. Compare, side by side (v0.4.0)

`<A-d>` with `compare-left.txt` and `compare-right.txt` both selected, or one in
each pane with the view split.

5.1 through 5.8 and 5.10 are automated (`ui::overlay::compare_frame`): both gutters numbering
their own file across an insertion, a replacement drawn opposite what it replaced and tinted in
the git signs' own theme colours, the footer's `x–y of z` keeping up with `j` `k` `<C-d>` `<C-u>`
`gg` `G` — including the three ways the bottom can be got wrong — `n` and `N` stepping over a
five-line block rather than through it, both sentences that stand in for a view, and `q`. What is
left for an eye is that the tints read as red and green, that the hairline down the middle is
drawn at all (a line is not a rectangle, so the harness cannot see it), and the real pair of
files. 5.9 does not match the program any more; it is written up in QA-REPORT.md.

| # | Do | Expect |
| --- | --- | --- |
| 5.1 | Compare the two | Two columns, a line down the middle, with line numbers on each side |
| 5.2 | Find line 100 | The changed line sits **opposite** the line it replaced — not listed as a removal and an addition far apart |
| 5.3 | Look at the colors | Removals tinted on the left, additions on the right, matching the git signs' colors |
| 5.4 | Look at the line numbers after the insertion | The two sides differ by one, each counting its own file |
| 5.5 | `n` / `N` | Between the two differences, not line by line inside one |
| 5.6 | `j` `k` `<C-d>` `gg` `G` | Scrolling, with the footer's `x–y of z` keeping up |
| 5.6a | **`G`, then `k` once** (v0.22.1) | Moves by one row straight away. Until v0.22.1 `G` overshot by a screenful, so `j` and `k` did nothing for about as many presses as the pane is tall |
| 5.6b | `G`, then `j` | Stays at the bottom, with the last row visible above the footer |
| 5.6c | `G` on a diff shorter than the pane | Nothing moves; every row was already on screen |
| 5.7 | `same-a.txt` and `same-b.txt` | "The two files are identical." — no thousands of matching rows |
| 5.8 | `binary.dat` against anything | Says it is not text on both sides and that the bytes differ |
| 5.9 | Two directories | Refused with a reason |
| 5.10 | `q` | Closes |
| 5.11 | Compare two files where one line changes a single word (`price` → `cost`), and another a Japanese word (`太郎` → `花子`) (v0.62.0) | On each changed row only that word is painted stronger, red on the left and green on the right, and the mark sits exactly under the word -- the Japanese one too. A line changed completely keeps only the row tint |

## 6. Split view, and sending between the panes (v0.1.0, `<A-c>` / `<A-m>` v0.2.0)

Two panes and the keys that move files between them. `<A-c>` and `<A-m>` are the
only commands in the program whose destination is a *pane* rather than the
clipboard or the cursor, so most of this section is about what they refuse.

Needs two directories with different contents — `many\` and `repo\` will do.

6.1 through 6.14 are automated (`ui::split_panes_frame`): that the two panes come out the same
width with the parent column gone, and that it is back after `<C-S-w>`; that the pane without the
keys draws its cursor in the other of two theme colours; that `<A-c>` and `<A-m>` really land in
the directory the other pane is showing and spend the selection doing it; the wording of both
refusals; and what becomes of the split when a tab is switched or closed. What is left is 6.15 — a
job's progress, its speed, and cancelling it from `w` all need a copy big enough to watch happen.

| # | Do | Expect |
| --- | --- | --- |
| 6.1 | `<C-w>` once | Two panes side by side. The **parent column is gone**; the layout is pane, pane, preview, the same width as before |
| 6.2 | `<C-w>` again, and again | The keys move between the panes. The same key both splits and switches |
| 6.3 | Look at the two cursor rows | The pane without the keys is **dimmer**. That is the only thing saying where typing will land |
| 6.4 | `h` `j` `k` `l`, `cd`, `/`, `s` while split | All act on the focused pane and nothing else. No command needs to know about panes |
| 6.5 | `<A-c>` with a file under the cursor and nothing selected | It is copied to whatever directory the **other** pane is showing |
| 6.6 | `<Space>` a few files, then `<A-c>` | All of them go, and **the selection is cleared afterwards** — unlike `y`, which keeps it |
| 6.7 | `<A-m>` instead | The files move: gone from this pane, present in the other |
| 6.8 | Put both panes in the **same** directory, then `<A-c>` | Refused, saying so. It would otherwise copy each file beside itself under a new name |
| 6.9 | `<C-S-w>`, then `<A-c>` | "Open the second pane first (`<C-w>`)" — not silence |
| 6.10 | `y`, `<C-w>`, `p` | The ordinary route works too, and is the one that can paste somewhere neither pane is |
| 6.11 | `[` / `]` / `1`–`9` while split | Tabs still switch. Switching to the tab the other pane shows just moves the keys there |
| 6.12 | Close the tab the **other** pane is holding (`<C-c>`) | The split ends; one pane, no stale second |
| 6.13 | `<C-w>` with only one tab open | A second tab is made on the same directory. With several already open, the next one is borrowed instead |
| 6.14 | `<C-S-w>` | Back to one pane, and the parent column returns |
| 6.15 | `<A-c>` a large directory, then watch the status bar | It is a job like any other copy: progress, speed, and cancellable from `w` |

## 7. The config paths in the help panel (v0.25.0)

| # | Do | Expect |
| --- | --- | --- |
| 7.1 | `~` with no `filer.toml` anywhere | **Both** directories are listed, the empty one marked `nothing here`. Before v0.25.0 only files that existed were shown |
| 7.2 | Hover a path | The row lights up and the pointer becomes a hand |
| 7.3 | Hover a key row | Nothing happens — it is not a link |
| 7.4 | Click a config **file** | The panel closes, the list opens its directory with that file under the cursor. `<Enter>` then opens it |
| 7.5 | Click a **directory** | The panel closes and the list goes there, empty or not |
| 7.6 | Click the empty one, then create `filer.toml` there and `<C-F5>` | It appears in the panel next time, without `nothing here` |
| 7.7 | With `YAZI_CONFIG_HOME` / `FILER_CONFIG_HOME` set | The listed directories follow them |
| 7.8 | A config warning line | Still yellow, and not clickable |

## 8. Which shell the pane runs (v0.24.0)

The setting is one line; the point of the section is that the **default** is the
thing that surprises people. filer reads the config only at start and on `<C-F5>`
(Q49), and a pane already running keeps the shell it started with, so a change to
`[term]` takes `<C-F5>`, then `<C-S-t>` to end the old shell, then `<C-t>`.

| # | Do | Expect |
| --- | --- | --- |
| 8.1 | `<C-t>` with no `[term]` in `filer.toml`, then `$PSVersionTable.PSVersion` | `7.x` — `pwsh`, the default since v0.55.0 wherever it is installed (Q29). `5.1.x`, Windows PowerShell, only on a machine without `pwsh` |
| 8.2 | With the pane open, add `[term]` / `shell = "powershell"`, `<C-F5>`, then `<C-S-t>`, `<C-t>`, ask again (v0.67.17) | The `<C-F5>` toast ends `— the pane keeps its shell until <C-S-t> closes it`; after `<C-S-t>` `<C-t>`, `5.1.x` |
| 8.3 | `$PROFILE` in each | Two different paths — `WindowsPowerShell\` for 5.1, `PowerShell\` for 7 |
| 8.4 | With the OSC 7 hook in the pwsh profile only, `cd` and `<A-Up>` under each | Works under `pwsh`, and says so under 5.1. That asymmetry is the whole bug report |
| 8.5 | `args = ["-NoLogo"]` | The banner is gone |
| 8.6 | A `shell` that is not installed | It fails to start and says so — no silent empty pane |
| 8.7 | Remove `[term]` again, `<C-F5>`, `<C-S-t>`, `<C-t>` (v0.67.17) | Back to the default (`7.x` where `pwsh` is installed). Without the `<C-F5>`, `<C-S-t>` `<C-t>` starts the old shell again: nothing has re-read the file |

## 9. The outline at the end of a file (v0.23.1)

Needs a document that **ends on a heading** with little under it — `TESTING-KEYS.md`
is one. A document whose last heading has pages of text after it will not show this
at all, which is what made it look intermittent.

All five are automated (`ui::outline_end_frame`): the clamp itself, on a fixture whose
last heading is on line 52 in a pane that stops at 40, and the frame not changing on
repeat — which is what "no flicker" is, one frame at a time. 9.3 is there as the
control (a heading with the file still under it goes to its own line) and 9.4 asks the
same ceiling through `seek`. What is left for an eye is a held key at speed: the tests
press once per frame, and a repeat rate no test sets is exactly what made this visible.
The key is `<BackTab>`, not the `<C-o>` 9.1 names — see QA-REPORT.md.

| # | Do | Expect |
| --- | --- | --- |
| 9.1 | Focus the outline (`<C-o>` or `l` on the file), `G` or hold `↓` to the last entry | The preview stops at the end of the file. **No flicker, no half-drawn frames** |
| 9.2 | Keep holding `↓` there for a few seconds | Nothing moves and nothing flashes. Until v0.23.1 this was one bad frame per repeat |
| 9.3 | The same on a document whose last heading has plenty of text after it | Unchanged — it was always correct here |
| 9.4 | `<A-j>` held at the bottom of a long file | Still steady; this path was fixed earlier and must stay that way |
| 9.5 | Move back up the outline | Each entry lands on its own line again, not on the clamped one |

## 10. The yank register, said out loud (v0.23.0)

The marker bar cannot carry this on its own, which is what the section is for.

All eight are automated (`ui::yank_frame`): the words in the header and in the status line, and
which of the three marker colours the row's bar is filled with — including the selection's winning
over the yank, the register surviving a move to another directory, and `p` spending a cut but not a
copy. What is left for an eye is that the colours read as green / yellow / red and that a 3px bar
beside a row is noticeable at all, which is not something a frame can be asked.
10.10 (v0.57.3) is `app::said_out_loud`, which reads the toast.

| # | Do | Expect |
| --- | --- | --- |
| 10.1 | `y` on a file with nothing selected | A **green** bar on the row, and `1 copied` in the header |
| 10.2 | `<Space>` on that same file | The bar turns **yellow** — the selection's colour wins, by design — and the header reads `1 selected · 1 copied` |
| 10.3 | `x` instead of `y` | A **red** bar, and `1 cut` |
| 10.4 | With something copied, `h` / `l` to another directory | `1 copied` is still in the header, with no row to show it |
| 10.5 | `p` after a copy | The register stays: `1 copied` is still there, and `p` again pastes again |
| 10.6 | `p` after a cut | The register empties; the count leaves the header |
| 10.7 | `X` or `Y` | The count leaves the header |
| 10.8 | The status line, bottom right | Says the same thing in the same words as the header |
| 10.9 | Cut a file, then `p` into a directory that already holds that name, and answer **no** to the overwrite | The count still leaves the header — `paste()` empties a cut register when it *submits* the job, not when the job succeeds, so the files are neither moved nor still in the register |
| 10.10 | `c` `c` in an empty folder (v0.57.3) | A toast says `Nothing to copy`, and the clipboard keeps whatever it held — until v0.57.3 nothing was said, so the last path copied looked like this one |

## 11. Bulk rename (v0.4.0)

In `bulk-rename\`.

All of it is automated (`ui::overlay::bulk_frame`), 11.9b included: the panel the prompt opens
with, the rows following the rule as it is typed, both refusals with the reason in brackets and
the count that says Enter will not go, the swap going through and coming back, group references,
an unknown placeholder, and the whole batch applied and undone in one step — a rename runs on the
UI thread, so a frame can watch one happen. What is left for an eye is that the panel is readable
where it sits, and that the prompt opens with its rule **selected**: it does not, so every rule
below has to be typed over a field the person clears first. That is in QA-REPORT.md.

| # | Do | Expect |
| --- | --- | --- |
| 11.1 | Select the 12 `IMG_*.jpg`, press `R` | A prompt reading `{name}{ext}`, and a panel above it listing every file with `→` and its new name — unchanged, since that rule changes nothing |
| 11.2 | Type `holiday-{n:2}{ext}` | The panel updates **as you type**, showing `holiday-01.jpg` … `holiday-12.jpg` |
| 11.3 | Enter | All twelve renamed. A toast says how many |
| 11.4 | `u` | **All twelve names back**, in one step |
| 11.5 | `U` | Renamed again |
| 11.6 | Select the twelve, `R`, type `in the way.txt` | Every row after the first is marked "two files would get this name", and Enter is refused |
| 11.7 | Select one file, `R`, type `in the way.txt` | Marked "already in this directory", Enter refused |
| 11.8 | Select `ab.txt` and `ba.txt`, `R`, type `s/^([ab])([ab])/$2$1/` | The panel shows `ab.txt → ba.txt` and `ba.txt → ab.txt`, **neither marked as a problem** |
| 11.9 | Enter | **The names swap.** A loop of `mv` fails on the second rename; this moves one out of the way first. Then `u` puts both back |
| 11.9b | Select `ab.txt` and `in the way.txt`, `R`, type `s/^ab/in the way/` | `ab.txt → in the way.txt` is marked "already in this directory": the file holding that name is selected but is **not moving**, so the name is not going spare. (Before v0.7.0 this was allowed, and failed at the last moment instead) |
| 11.10 | `R` with `s/IMG_(\d+)/photo-$1/` | Group references work |
| 11.11 | `R` with `{nope}` | Says the placeholder is unknown; nothing renamed |

## 12. Undo and redo (v0.3.0)

12.6 and 12.7 are automated (`ui::undo_frame`, five tests): `r` then `u` puts the old name back and
the toast names it, and `u` / `U` on an empty stack each say so in their own words. Everything else
in this section starts with `d`, and a delete is a job on the ops worker put back by reading the
trash, so 12.1 to 12.5 and 12.9 to 12.12 stay here. Three of those rows state a rule a **rename**
obeys as well, and the module drives each in that form: history forking on a fresh action (12.8),
`U` walking the step forward again under its own sentence (12.4), and an undo blocked by a name
taken in the meantime keeping the step, so a second press works (12.9). **The rows themselves are
still unchecked** — nothing here has been through the recycle bin. 12.5's newest-of-two rule is
`fs::restore`'s own unit test. 12.8 used to fork history by creating a file, which records no undo
step; it now renames a second file (#83).

| # | Do | Expect |
| --- | --- | --- |
| 12.1 | `d` on a file in `many\` | It goes to the recycle bin |
| 12.2 | `u` | It comes back, in its original place. A toast says so |
| 12.3 | Check the task panel (`w`) during F2 | A `Restore` row appears and completes |
| 12.4 | `U` | Deleted again |
| 12.5 | Delete two files with the same name from different folders, an interval apart, then `u` | The one just deleted comes back — not the older one |
| 12.6 | `r` to rename, then `u` | The old name is back |
| 12.7 | `u` with nothing to undo | "Nothing to undo" — no error |
| 12.8 | Rename a file, undo it, then rename **another** file, then `U` | Redo is gone: the new rename forked history. Creating a file records no undo step, so a new file leaves the redo in place (#83) |
| 12.9 | Delete a file, `u`, but create a file with that name first | `u` says the name is taken, and pressing it again after moving that file out of the way works |
| 12.10 | Open a file in another program so it is locked, select it **with several others**, `d` (v0.27.1) | The others go. The message **names the one that did not**, and the task panel's count matches what actually went. Until v0.27.1 it said `Trash: trash: Error … Some operations were aborted` naming nothing, and counted them all as done |
| 12.11 | `d` on a drive whose Recycle Bin is turned off | Same shape of message, naming the file |
| 12.12 | `d` with nothing locked | Unchanged, and still **one** entry in Explorer's own undo — the batch call is still the normal path |
| 12.13 | `d` on one file, then on two (v0.57.3) | A toast each time: `Trashed <name> — u to undo`, then `Trashed 2 item(s) — u to undo`. Until v0.57.3 `d` said nothing, so it looked the same as `D` |
| 12.14 | `d` on five files, and `w` while it runs (v0.58.1) | The row reads `Trash 5 item(s)  [running]` -- the verb **once** -- and the line under it `0/5 files`, with no `0 B / 0 B` |
| 12.15 | As 12.10 -- one file of five held open elsewhere, `d` on all five -- then `u` (v0.59.7) | The error names the held file with `it is open in another program`, and `u` brings back the **four** that went. Until v0.59.7 the error said only `Some operations were aborted` and `u` said `Nothing to undo` |
| 12.16 | 12.9 again: `d` a file, make a new file by that name, then `u` (v0.59.7) | The error reads `a file by that name is already there. Move it away and press u again` -- not `RestoreCollision { … TrashItem { id: "C:\$Recycle.Bin…` -- and after moving the new file away, `u` works |
| 12.17 | `a`, type `new/deep/note.txt`, `<Enter>`, then `u`; then `U` (v0.60.0) | `u` removes `note.txt` and both folders made for it, toast `Removed note.txt and 2 folder(s)` (v0.67.10; before, `Removed note.txt` said nothing of the folders); `U` makes all three again. Write something into the file and press `u`: it stays, and the error says it has been written to since |
| 12.18 | Yank a file, `-` in another folder, then `u`; then `U` (v0.60.0) | `-` says `Linked <name> — u to undo` (v0.67.10; before, the yank's toast stayed up). `u` removes the link and only the link: the source file and its contents are untouched. `U` makes the link again. On Windows, also with `=` (hardlink) and with a folder (`-` on a directory) |

## 13. Symlinks and `g`+`f` (v0.26.8)

Windows makes these awkward to create. A **junction** needs no admin rights:
`mklink /J linktest C:\dev` from `cmd`. A symlink to a *file* needs an elevated
shell or developer mode: `New-Item -ItemType SymbolicLink -Path l.md -Target
C:\dev\filer\README.md`. There are real ones under `C:\Users\<you>\` if you
would rather not make any.

13.1 to 13.6 are automated (`ui::link_rows`): the `->` after the name, the `l` in `m`+`p`'s column,
and what `g`+`f` does with a link to a directory, a link to a file, a broken link, an ordinary file
and an empty directory. Those rows are built in the test rather than made on disk, so what they do
not cover is the scan worker reading a real `read_link` -- which is all 13.7 (a junction), 13.8 and
13.9 (`-` and `_` writing one) are about.

13.10 to 13.13 and 13.15 are automated (`ui::overlay::spot_link_section`), running the real
provider: `Kind` / `Target` / `Resolves`, the relative form's two rows disagreeing, a broken link's
`no (…)`, the hardlink's count, and no section at all on a file with one name. The symlink three are
`#[cfg(unix)]` -- creating one on Windows is 13.8's privilege problem -- so **on Windows 13.10 to
13.12 still need a hand.** 13.14 (`Also at`) is Windows-only and 13.16 needs another program.

For 13.16, that other program can be PowerShell. In one window, take an exclusive write lock and
leave it held -- `FileShare.None` means nothing else may so much as open the file:

```powershell
cd $HOME\Desktop\filer-fixtures
"x" | Out-File locked.txt
fsutil hardlink create locked-2.txt locked.txt
$fs = [IO.File]::Open("$PWD\locked.txt", 'Open', 'Write', 'None')
```

Leave that window alone, press `<Tab>` on `locked.txt` in filer, then come back and run `$fs.Close()`.
While the lock is held, `Get-Content locked.txt` fails -- worth running once, so you know the lock is
real and the section answering anyway is the finding. The previous wording named `hiberfil.sys`,
which has one link and therefore draws no Link section at all (13.15): nothing to see, on a file most
machines do not have.

| # | Do | Expect |
| --- | --- | --- |
| 13.1 | Look at a link's row | `->` after the name. With `m`+`p` the type column reads `l` |
| 13.2 | `g`+`f` on a link to a **directory** | The list goes into the target |
| 13.3 | `g`+`f` on a link to a **file** | The list goes to the target's directory with the file under the cursor; `<Enter>` then opens it |
| 13.4 | `g`+`f` on a **broken** link | `Broken link: <name>` in red |
| 13.5 | `g`+`f` on an ordinary file (v0.26.8) | `Only a symlink can be followed — a link shows -> after its name`. Until v0.26.8 nothing happened at all, which was indistinguishable from an unbound key |
| 13.6 | `g`+`f` in an empty directory | Nothing, and no message — there is no row to say anything about |
| 13.7 | A junction (`mklink /J`), not just a symlink | Treated the same: `->`, and `g`+`f` follows it |
| 13.8 | `y`, then `-` in another directory | The symlink appears. **On Windows this needs Developer Mode on** (Settings > System > For developers) — without it, and without running filer elevated, it fails with `os error 1314` and the toast says which two remedies there are. The privilege is the OS's, not the app's: `std` already passes `SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE`, which is what makes Developer Mode enough |
| 13.8a | Without Developer Mode and not elevated: `y` on a **folder**, then `-` in another directory (v0.67.11) | The same refusal, and after it `A junction needs neither: cmd /d /c mklink /J "<the link>" "<the folder>"`, both paths absolute (`cmd /d /c` since v0.72.6, #193). Pasting that into `cmd` **and** into `pwsh` (filer's own pane, `<C-t>`) makes a junction that `g` `f` follows. `-` on a file says nothing of junctions (they are folders only). Since v0.67.19 a question follows: `Make a junction instead?`, naming both paths and saying a junction is not relative and cannot reach a network location; `n` leaves nothing behind |
| 13.8b | As 13.8a, then `y` (v0.67.19, Q46) | Toast `Made a junction <name> — u to undo`; `(Get-Item <link>).LinkType` reads `Junction` and `g` `f` follows it. `u` removes the junction and only the junction (the folder and its files stay), saying `Removed the junction <name>`; `U` makes it again, still a junction, saying `Made the junction <name> again` (v0.70.3; before that the two said `link`, #185) |
| 13.8c | As 13.8a, then `c` at the question (v0.71.4, Q56) | The question offers `[c] Copy the mklink command` between `y` and `n`. `c` closes it, makes nothing (the destination folder is still empty), toasts `Copied the mklink command — it runs in cmd or PowerShell`, and `Get-Clipboard` holds exactly the `cmd /d /c mklink /J "<the link>" "<the folder>"` line the refusal shows. Pasted into filer's own pane (`<C-t>`, PowerShell), it makes the junction (v0.72.6; before, the bare `mklink` was not recognized there, #193) |
| 13.9 | `y`, then `_` in a **sibling** directory | The same link, written relative (`..\other\file`). `g`+`f` follows it, and it survives moving both directories together — which is the point of `_` over `-` |
| 13.10 | `<Tab>` on a symlink (v0.46.0) | A **Link** section: `Kind` reads `Symlink`, `Target` the stored path, `Resolves` where it lands |
| 13.11 | `<Tab>` on a link made with `_` | `Kind` reads `Symlink (relative)`, and `Target` is the relative path while `Resolves` is absolute — the two rows differ, which is the whole point of the pair |
| 13.12 | `<Tab>` on a **broken** link | `Resolves` reads `no (…)` with the OS's reason, and the section still appears |
| 13.13 | `<Tab>` on a hardlink (make one with `=`, or `fsutil hardlink create`) | `Kind` reads `Hardlink` and `Links` reads `2`. **This is the only place in the app a hardlink is visible** |
| 13.14 | The same, on Windows | `Also at` lists the other path. Check it against `fsutil hardlink list` — the same set, with the file's own path left out |
| 13.15 | `<Tab>` on an ordinary file with one name | **No Link section at all** — not a section saying "1", which would be noise on every file |
| 13.16 | Hardlink a file, then have another program hold it open for writing with no sharing, and `<Tab>` it (commands in the preamble above) | `Links` still reads `2` and `Also at` still lists the other name. The handle asks for **no** access rights, so an exclusive write lock does not hide the count |
| 13.17 | `<Tab>` on a junction (`mklink /J`) (v0.59.4) | `Kind` reads `Junction`, not `Symlink`. A symlink to the same folder (`mklink /D`) still reads `Symlink`. The list's `->` is unchanged for both (13.7) |

## 14. The parent column, with the mouse (v0.26.7)

The leftmost column. It draws files and directories the same way, so both have
to answer a click.

| # | Do | Expect |
| --- | --- | --- |
| 14.1 | Click a **directory** there | The list goes into it, as it always has |
| 14.2 | Click a **file** there (v0.26.7) | The list goes up to where that file lives, **with the file under the cursor**. Until v0.26.7 nothing happened at all |
| 14.3 | Then press `<Enter>` | It opens — the cursor really is on it, not merely near it |
| 14.4 | Double-click either | The same as a single click; no second, different meaning |
| 14.5 | Click the row for the directory you are already in | You stay there, and the cursor does not jump about |
| 14.6 | At a drive root, where there is no parent column | Nothing to click, and nothing misbehaves |

## 15. Window scale, and the key it took back (v0.32.0)

| # | Do | Expect |
| --- | --- | --- |
| 15.1 | `<C-->` with something yanked | **Only** the window shrinks. Until v0.32.0 it also made a hardlink — one press, two actions |
| 15.2 | `<C-+>`, and `<C-=>` | Both make it bigger. Which of the two needs shift depends on the layout — on US `+` is shift+equals, on JIS `+` is shift+semicolon and `=` is shift+minus — and both spellings are bound so either reaches it (v0.45.6) |
| 15.3 | `<C-0>` | Back to 100%, and a toast says so |
| 15.4 | Hold `<C-->` down | It stops at 20%, and the toast's count adds up: 8 steps down from 100% and the rest at the floor (`Scale 20% (minimum) ×N`). `<C-+>` held stops at 500% |
| 15.4a | The same, watching the window rather than the toast | It shrinks **smoothly** while held, with no flicker or blank frames between steps |
| 15.5 | `=` with something yanked, in a directory **on the same drive** | The hardlink, in its new place. No *row* says so — a hardlink is another entry pointing at the same data, so the listing has no marker for it. Since v0.46.0 the spot panel does: `<Tab>` on it reads `Kind: Hardlink` and `Links: 2`, which is 13.13. Confirm from outside with `fsutil hardlink list <the new path>`, which lists every path sharing the data; or write to one and read the other. Across drives it must fail: NTFS hardlinks cannot leave their volume. Was `<C-S-->` until v0.45.6, a chord no keyboard can produce |
| 15.6 | `<A-i>` / `<A-o>` on an image | Still the **image** zoom, unaffected — `zoom` and `scale` are different commands |
| 15.7 | `~` | `scale in` / `scale out` / `scale reset` are listed, like any other command |
| 15.8 | Hold `<C-+>` until it stops, then `<C-->` until it stops (v0.57.3) | The toast reads `Scale 500% (maximum)`, then `Scale 20% (minimum)` — the `×N` alone could not tell stopped from still moving |
| 15.9 | `=` with a file yanked from another drive (`R:` → `C:`) (v0.59.4) | The error reads `hardlinks can't cross drives (R: → C:). Use p to copy instead`, not Windows' "cannot move the file to a different disk drive" |

## 16. Word, Excel and PowerPoint (v0.31.0)

**On a machine with no Office installed** — that is the case this is for.

| # | Do | Expect |
| --- | --- | --- |
| 16.1 | Hover a `.docx` | Its text, paragraph by paragraph. Not a hex dump, not a metadata card |
| 16.2 | A paragraph with mixed bold and plain in one sentence | **One line**, not one per run |
| 16.3 | A document with Heading 1/2 styles, then `<S-Tab>` | The headings are the outline, and `<Enter>` on one jumps to it |
| 16.4 | Hover a `.xlsx` | Rows as tab-separated cells, each sheet announced |
| 16.5 | A workbook whose **first tab is not `sheet1.xml`** | The tabs come out in the workbook's order, with their real names |
| 16.6 | A sheet holding dates | `2023-03-15`, **not** `45000` |
| 16.7 | A sheet holding a date **and** a time | The time follows the date |
| 16.8 | Hover a `.pptx` with ten or more slides | In order — slide 10 after slide 9, not after slide 1 |
| 16.9 | Japanese text in any of the three | Correct, and `&amp;` `&lt;` come through as `&` `<` |
| 16.10 | Rename an old `.doc` to `.docx` and hover it | A card saying it is not an Office XML file, naming the likely cause |
| 16.11 | A very large workbook | Stops at 5000 lines and says it is truncated; it does not hang |
| 16.12 | `/` and `n` inside one | Search works, because it is an ordinary text preview |

## 17. Previewers of your own (v0.30.0)

Needs `pdftoppm` and `ffmpeg` on the `PATH` (`filer env` says), and the two
rules from the README in `filer.toml`. **The end-to-end test runs `sh`, so it is
skipped on Windows — this section is the only coverage of the `cmd` path.**

| # | Do | Expect |
| --- | --- | --- |
| 17.1 | Hover a multi-page PDF | Page one, with `page 1` under it |
| 17.2 | `<A-j>` | Page two. `page 2` under it |
| 17.3 | `<A-k>` | Back to page one |
| 17.4 | `<A-k>` again, on page one | Stays. It does not go to page zero or below |
| 17.5 | Hold `<A-j>` past the last page (v0.30.1) | **The last page stays on screen**, and a line says `No more: …` with the command's own words. Until v0.30.1 the page was replaced by the error |
| 17.5a | `<A-k>` straight after that | Back a page from the last one, not from somewhere past it |
| 17.5b | A **short** video — a few seconds — and `<A-j>` a few times | Same: it stops at the last frame it could draw. This is where it bites, since `step = 10` runs off the end almost at once |
| 17.5c | The caption on a video (v0.30.1) | `50s`, not `s 50` |
| 17.6 | Watch the screen while paging | **No console window flashes.** It runs once per press |
| 17.7 | Page to 5, move to another file, come back | Back at page one: the page belongs to the file |
| 17.8 | Page back to one you have already seen | Instant — it is cached per page |
| 17.9 | Hover a video (v0.30.2) | **A frame appears.** Until v0.30.2 none ever did on Windows: `{out}.png` was quoted as `"…page".png`, which `cmd` hands to ffmpeg with the quotes in the filename |
| 17.9a | The same on a path with a space | Still draws — the quoting wraps the whole word, suffix included |
| 17.10 | `<A-j>` on it | Ten seconds in, by `step` |
| 17.11 | A PDF with a **space** in its name, and one in a Japanese folder | Both draw. The quoting is filer's, not the rule's |
| 17.12 | Rename `pdftoppm` away, then hover a PDF | An error naming the tool, not a hang |
| 17.13 | Remove the `[[preview]]` rules, `<C-F5>`, hover a PDF | Back to the shell thumbnail, unchanged |
| 17.14 | `filer env` with the rules in place | `pdftoppm` and `ffmpeg` listed under Tools, with `preview *.pdf` beside them |

## 18. Quick look, minimap's neighbours, and the rest of the panes

18.1 to 18.4, 18.8 to 18.10, 18.12 and the second half of 18.11 — `'` and a letter — are automated
(`ui::quick_look_frame`). 18.5 and 18.6 are the same ground as section 6, so
`ui::split_panes_frame` covers those. What is left is 18.7, where the drag and the label beside the
pointer both need a hand on a mouse, and the first half of 18.11, which does not match the keymap:
`b` is the prefix the bookmark *management* hangs off, so `b` and a letter jumps nowhere. See
QA-REPORT.md.

| # | Do | Expect |
| --- | --- | --- |
| 18.1 | `<F3>` on any file | A large panel over the panes, with the file's name at the top and "Esc to close" |
| 18.2 | With it open, press `j` and `k` | **The list still moves**, and the panel follows down it. This is why it is not an overlay |
| 18.3 | `<A-j>` / `<A-k>` with it open | The panel's content scrolls |
| 18.4 | `<F3>` or `<Esc>` | Closes |
| 18.5 | `<C-w>` | The view splits into two panes; the one with the keys is framed, the other's cursor is dimmed |
| 18.6 | Select files, `<A-c>` | Copied into the other pane |
| 18.7 | Drag files onto the other pane | A frame marks the target, and a label by the pointer says "copy" — `Shift` makes it "move" — **before** you let go |
| 18.8 | `<Tab>` on a file | The spot panel, with the file's details |
| 18.9 | `<S-F10>` or right-click | The context menu, with the openers from your config |
| 18.10 | `<C-S-p>` | The palette, listing every binding; typing filters it |
| 18.11 | `'` then a letter, having saved one with `B` | Jumps there. **`b` is the prefix bookmark *management* hangs off** (`bb` lists, `bs` saves, `bd` deletes), so `b` and a letter reaches nothing |
| 18.12 | `z` | The jump list: bookmarks first, then recent directories with "2h ago" beside them |

## 19. The wheel, over each pane (v0.26.5)

The same arithmetic in three places, so all three have to be tried. A notch
should move about three rows, and a slow turn should move *something* — the bug
was that a gentle turn moved nothing at all.

| # | Do | Expect |
| --- | --- | --- |
| 19.1 | Wheel over the **preview** of a long text file | It scrolls, one notch at a time, without spinning hard. This is the v0.26.5 fix |
| 19.2 | Turn the wheel as slowly as you can over the preview | It still moves. Every fraction counts; nothing is discarded |
| 19.3 | Wheel over the **file list** | The same, and with the split open, over each pane in turn |
| 19.4 | Wheel over the **terminal** pane | Still right — fixed earlier, in v0.20.4, and now sharing the same code |
| 19.5 | Turn one way then straight back | It reverses at once, with no dead travel from a stranded remainder |
| 19.6 | `Ctrl` and the wheel over an image | Zooms, and does **not** scroll the pane with the same turn |
| 19.7 | Move the pointer between panes mid-turn | Neither jumps: each keeps its own remainder |

## 20. Configuration and theming

| # | Do | Expect |
| --- | --- | --- |
| 20.1 | With filer open, edit `theme.toml` (change `[mgr] cwd` to something loud) and press `<C-F5>` | The color changes without restarting |
| 20.2 | Change `[ui] font_size` in `filer.toml`, `<C-F5>` | The text resizes |
| 20.3 | Add a `keymap.toml` binding, `<C-F5>` | The new key works, and `<F1>` lists it |
| 20.4 | Sort with `,s`, then `<C-F5>` | The sort **stays** as you set it — a reload does not undo what you changed by hand |
| 20.5 | Put a syntax error in `filer.toml`, `<C-F5>` | An error toast naming the problem; the old config stays in force -- **including what the broken file itself set** (v0.67.18, Q47: before, its `[ui]` fell back to the defaults), and the toast ends `(the last settings read from it stay in force until it parses again)` |
| 20.6 | `[ui] minimap = false`, `<C-F5>` | No minimap |
| 20.7 | In `keymap.toml`, `[[mgr.prepend_keymap]]` `on = "<F8>"`, `run = 'cd C:\Windows\System32'` -- no quotes inside the command (v0.59.0) | `<F8>` lands in `C:\Windows\System32`. Until v0.59.0 the backslashes were dropped and the error named `C:WindowsSystem32` |

## 21. Archives (v0.2.0)

Three of these are automated (`ui::preview::archive_frame`): 21.1, on a zip the test packs itself,
down to the size column and the `—` a folder inside an archive gets; 21.6; and 21.12. The other
nine are out of reach and will stay there — `e` and `E` both hand the work to the job queue, and
the harness runs no workers, so unpacking, packing, the task panel's counts and the sizes two
formats come out at all need the program running. 21.9 needs 7-Zip besides. QA-REPORT.md says which
row needs which. 21.6 says something other than what it says here, which is written up there too.

| # | Do | Expect |
| --- | --- | --- |
| 21.1 | Hover `sample.zip` | The preview lists what is inside |
| 21.2 | `e` on it | Unpacked into a `sample` folder beside it; progress in the task panel |
| 21.3 | `e` again | The second one gets a different name; the first is not overwritten |
| 21.4 | Select `to-pack\`, press `E`, accept `to-pack.zip` | Packed, and when the job is done the cursor is on `to-pack.zip` (v0.55.0). Had you moved to another folder meanwhile, it stays where you are |
| 21.5 | `E` and change the name to end in `.tar.gz` | A gzipped tar, not a zip |
| 21.6 | `e` on a text file | A toast says it was skipped; nothing else happens |
| 21.7 | `E` and change the name to end in **`.7z`** (v0.27.0) | A real 7z. Until v0.27.0 this was refused as read-only |
| 21.8 | `e` on that `.7z` | It unpacks, and the files match what went in |
| 21.9 | Open the same `.7z` in 7-Zip or Explorer | It opens there too — the point of the format is that it travels |
| 21.10 | Pack a folder holding subfolders as `.7z`, watch the task panel | The count is of **files**, not folders, and it reaches the total rather than stopping short |
| 21.11 | Compare the `.7z` and the `.zip` of the same input | The 7z is smaller; that is the reason to have it |
| 21.12 | `E` with a name ending in something else (`.rar`) | `Name it .zip, .7z, .tar or .tar.gz to say which format` |
| 21.13 | Pack `to-pack\` as `.zip`, then `7z l` the archive (v0.57.2) | Every entry carries its file's own date and time (to the even second), not `1980-01-01 00:00:00` |
| 21.14 | Give `to-pack\` files with old dates (`(Get-Item f).LastWriteTime = "2021-06-15 12:34:56"`), pack it with `E` as `.zip`, `.tar.gz` and `.7z`, then `e` each one (v0.65.7) | Every unpacked file has its original `LastWriteTime` back (a zip to the even second), not the moment it was unpacked. Before v0.65.7 every one read the time of the `e` (#156) |
| 21.15 | `E` on `to-pack\` alone (the archive's top level is one folder), then `e` on `to-pack.zip`; then the same with `sample.zip`, whose top level is loose files (v0.66.0) | `to-pack_1\` holds the files directly -- no `to-pack_1\to-pack\` (Q43). `sample.zip` still unpacks into its own `sample\` (or `sample_1\`) folder |

## 22. Editors, at a line (needs the editors installed)

Open a file's outline with `l` or `<S-Tab>`, put the cursor on an entry, press
Enter. Each of these is a skip if the editor is not installed.

| # | Editor | Expect |
| --- | --- | --- |
| 22.1 | 秀丸エディタ | Opens at the outline entry's line |
| 22.2 | サクラエディタ | Same |
| 22.3 | EmEditor | Same |
| 22.4 | Notepad++ | Same |
| 22.5 | メモ帳 | Opens, at the top — it has no line argument, and that is correct |
| 22.6 | VS Code / nvim, if you have them | At the line |

## 23. Network paths (needs a share)

| # | Do | Expect |
| --- | --- | --- |
| 23.1 | `g<Space>`, type `\\server\share` | It opens |
| 23.2 | Copy a file to and from it | Works, with progress |
| 23.3 | Unplug the network mid-listing, or point at a dead host | **The window keeps responding.** An error toast, and the tab goes back where it was |
| 23.4 | Tab-complete a path on the share | The prompt stays responsive; a `…` shows while it waits |
| 23.5 | `g<Space>`, type a path with a `\|` in a folder name partway down (`C:\Temp\a\|b\c\d`), `<Enter>` (v0.57.3) | **One** error toast, naming a whole path. Until v0.57.3 the parent columns each added their own, naming only a fragment (`b: …`, `c: …`) |
| 23.6 | `filer <a folder>\tpyo`, or `g<Space>` with a name that is not there (v0.57.4) | The folder above opens, as before, and a red toast says `No such file or folder: tpyo — showing <the folder>`. A name that *is* a file opens the folder with the file under the cursor and says nothing |

## 24. Awkward names

In `awkward names\`.

24.1 and 24.3 are automated (`ui::awkward_names`, three tests): a CJK name reaches the row as
itself and every character of it gets a glyph, and two names differing only in case stay two rows
with their case kept. What is left for an eye in 24.1 is the column arithmetic -- that the glyphs
are two cells wide and the rows line up. 24.2 is asserted by the harness (the long name is cut down
to its column and still ends in `name.txt`) since v0.57.0 cuts in the middle of the stem (Q34); the
ARM64 machine read the same on screen (#126). 24.4 needs the terminal pane, which is `#[cfg(windows)]`; the quoting it is really
about is `terminal::tests::a_path_reaches_the_shell_as_one_word`, over the same `'` the fixture
has. 24.5 needs the recycle bin.

| # | Do | Expect |
| --- | --- | --- |
| 24.1 | The CJK names | Drawn correctly, columns lined up (they are two cells wide each) |
| 24.2 | The very long name | Elided in the middle, with the extension still readable |
| 24.3 | `UPPER.TXT` and `upper.txt` | Both listed, both openable |
| 24.4 | Copy the name with a quote in it, `<A-t>` into the terminal | Quoted so the shell sees one word |
| 24.5 | `d` then `u` on the CJK-named file | Comes back under the same name |
| 24.6 | Run `scripts\make-fixtures.ps1` in a fresh folder (v0.59.1) | No warning, except on an ordinary (case-insensitive) NTFS folder: `awkward names: 5 entries on disk, expected 6`, naming `fsutil file setCaseSensitiveInfo` -- the reason 24.3 cannot be pressed there |

## 25. `filer env` (v0.28.0)

Run from a shell, not from inside the app.

| # | Do | Expect |
| --- | --- | --- |
| 25.1 | `filer env` from PowerShell | The five sections print (Filer, Config, Last run, Tools, Variables). A release build is a GUI binary, so this is the same `CONOUT$` path `--version` uses — **text actually appears** |
| 25.2 | The Config section | Both directories, each saying what is in it or `nothing here`, and `not here:` listing the rest |
| 25.3 | With a deliberate typo in `keymap.toml` | The warning appears under `Warnings`, its several lines indented under the one key |
| 25.4 | The Tools section | `git` with its version, the shell the terminal pane starts, and every program a `[[preview]]` rule or an opener names -- each with its path (or `not found`) and what it is for. Nothing filer does not run (no `pdftoppm`, `ffmpeg`) |
| 25.4a | With `[term] shell = "pwsh"` set (v0.29.1) | `pwsh` is the shell listed. Without it, `powershell` — the one that will actually launch, not a guess |
| 25.4b | With openers configured | Each named program is listed with the opener kind it belongs to, found or not |
| 25.4c | An opener naming a **quoted full path** (秀丸, サクラ) | The whole path is resolved, not just up to the first space |
| 25.4d | Watch the screen while `filer env` runs | **No editor or viewer opens.** The programs are looked up on `PATH`, never executed |
| 25.5 | On Windows on ARM with the x64 build | `OS arch` and `Process arch` **disagree** — that disagreement is the whole reason both are printed |
| 25.6 | `filer --help` | `env` is listed under COMMANDS, and `env --out FILE` under it (v0.68.0) |
| 25.7 | Double-click `filer.exe` (no console) | Unchanged: the window opens, nothing is printed anywhere |
| 25.8 | Open filer once, quit, then `filer env` (v0.29.0) | A **Last run** section: the adapter with its backend and device type, and every font file that was loaded |
| 25.9 | On a fresh machine, `filer env` **before** ever opening filer | `not recorded — filer has not opened a window on this machine yet`, not an empty section |
| 25.10 | Name a different font in `filer.toml`, `<C-F5>`, then `filer env` again | The new file is listed; the reload updates the record |
| 25.8a | Open filer, quit, `filer env`, and check the **Window** row against the screen (v0.47.33) | The pixels are the window you can see, and `pt x scale` multiplies out to them. **This is the row that settles a DPI argument** — what a script measures with `GetClientRect`, or a `PrintWindow` capture, depends on the DPI awareness of whatever did the measuring, and can disagree with the window while looking right |
| 25.8b | On a display at 150%, open filer, quit, then `filer env` | The Window row reads e.g. `2040 x 1290 px (1360 x 860 pt @ 1.5)` — the pixels are half again the points, and **nothing is cut off the right or bottom edge of the window** |
| 25.11 | With no bold face anywhere | `none found; bold is faked by overstriking` — the bold list is separate from the regular one on purpose |
| 25.12 | An opener starting with `start` (the default-app one) | **`built into cmd`**, not `not found`. It is one of `cmd`'s own commands and is never a file on the `PATH`, so the lookup every other row uses cannot see it (v0.33.12) |
| 25.13 | `<Enter>` on a file whose rule uses that opener | It really does open — the row and the behaviour agree |
| 25.14 | An opener naming a program that genuinely is not installed | Still **`not found`**. The exemption is for the shell's own names only |
| 25.15 | Break `yazi.toml` and read the Warnings row | The path is written **`…\filer\yazi.toml`**, all backslashes. It used to come out `…\filer/yazi.toml`, in the one message whose job is to name the file to edit (v0.33.12) |
| 25.16 | `filer <a folder with files> --keys "<Tab>C"`, then `Get-Clipboard` (v0.54.0) | The window opens, spot opens on the first row by itself, and the clipboard holds the whole panel as `Label<TAB>value` lines — `Name` and `Path` naming that first row |
| 25.17 | `filer --keys "<Tab"` and `filer --keys "<Bogus>"` | **No window**: one line naming the problem (`has no closing >` / `is not a key`), exit code 2 |
| 25.18 | Release build: `filer env \| Out-File out.txt`, then `Get-Content out.txt`; and `cmd /c "filer env > out2.txt"` (v0.54.4) | The whole report is **in both files**, and nothing is printed on screen. Before v0.54.4 both were empty. (PowerShell's own `filer env > out.txt` still gives an empty file: PowerShell does not connect a windowed program's output to a file. README says so) |
| 25.19 | `filer env \| Select-String arch` | **Only the two arch lines** (Windows; elsewhere there is one, `Process arch`), not the whole report |
| 25.19a | `$v = & filer env \| Write-Output; $v.Count` in PowerShell (v0.67.26, Q54) | The report's line count, not 0. **With nothing after it** -- `$v = & filer env` -- PowerShell does not wait for a windowed program at the end of a pipeline, and `$v` is empty: that is PowerShell, not filer (#183), and why the row has the `\| Write-Output` |
| 25.19b | `filer env --out out3.txt` from PowerShell, in a folder whose path has Japanese in it, then `Get-Content -Encoding utf8 out3.txt` (v0.68.0, Q55) | One line `filer: wrote <full path>`, and the file holds the whole report with the Japanese **readable**, whatever the console's code page. `filer env --out` with no name, and `filer env --outt x`, print one line naming the problem, exit code 2, and write nothing |
| 25.19c | With the zip's folder on the `PATH` (v0.71.0, Q44): `(Get-Command filer).Source`; `$v = & filer env; $v.Count`; `filer --version > v.txt; Get-Content v.txt`; `filer --keys "<Tab"; $LASTEXITCODE`; then `filer` alone | `Source` ends in `filer.com`. The count is the report's line count with **nothing after the call** (25.19a's form without `Write-Output`); `v.txt` holds the version line; the refusal prints one line and `$LASTEXITCODE` is 2. `filer` alone opens the window, and the prompt comes back while the window stays open |
| 25.20 | `filer env` with nothing redirected, and `filer --version` | Still printed on screen, as 25.1 has it — the console path is unchanged |
| 25.21 | `filer env` (v0.58.1) | An `Executable` row with the full path of the `.exe` that answered. On the ARM64 machine, the **x64** build's `Process arch` reads `x86_64 (emulated on aarch64)`; the ARM64 build's reads `aarch64` alone |
| 25.22 | `filer --keys "<C-t><Wait:2000>echo<Space>hi<Enter><Wait:1000><C-S-Enter>"` with `FILER_PTY_LOG` set (v0.59.0) | The shell's prompt is up before `echo` arrives (the log's `out` lines show it ahead of the `in key` lines), `hi` is printed, and the pane takes the window a second later. `filer --keys "<Wait:1.5s>"` is refused on the command line, naming `<Wait:500>` |
| 25.23 | Open the pane, `<C-S-Enter>`, close filer, then `filer env` (v0.59.4) | A `Terminal pane` row under `Last run` gives the grid as `N x M (lines x columns)`, the size it last had. After a run that never opened the pane: `not opened in that run` |
| 25.24 | From a shell in some folder, `filer .`, then `filer ..`, then `filer two words` unquoted (v0.59.5) | `.` opens that folder with its **absolute** path in the title and a parent column, and `h` goes up; `..` opens the one above. The unquoted pair is refused before any window: `filer: more than one path: "two" and "words" (a path with a space in it needs quotes)` |
| 25.25 | Open a file with an opener (`<Enter>` or `<S-Enter>`), run one `;` shell command, close filer, then `filer env` (v0.59.9) | A `Launched` row under `Last run` lists both command lines exactly as filer built them, newest last, at most five. After a run that launched nothing: `nothing in that run` |

## 26. Bug report from inside the app (v0.11.0)

`<F12>` builds a URL and hands it to the browser. None of that can be exercised
without a browser, a desktop session and the repository in front of you: the
tests cover the encoding and the shape of the URL, not what GitHub does with it.

| # | Do | Expect |
| --- | --- | --- |
| 26.1 | `<F12>` | The default browser opens GitHub's new-issue form, and a toast says so |
| 26.2 | Look at the form | **Version** and **OS とアーキテクチャ** are already filled in; the rest is empty |
| 26.3 | Compare the filled version against `filer --version` in a terminal | The same string, architecture included |
| 26.4 | Compare the filled OS line against `winver` | Edition, feature update and build all match, UBR included (`Windows 11 Pro 25H2 (build 26200.9457)`) |
| 26.4b | Compare it against the form's own PowerShell snippet | The same facts. Nothing left worth pasting over the top |
| 26.5 | On the ARM64 machine, with the **ARM64** build | OS arch and Process arch both read `aarch64` |
| 26.6 | On the ARM64 machine, with the **x64** build (under emulation) | OS arch `aarch64`, Process arch `x86_64` — **the two disagree, and that is the finding** |
| 26.7 | Submit the report | It posts, and the pre-filled fields survive |
| 26.8 | `<F12>` with no browser set as default (or a broken association) | An error toast naming the failure. **The window keeps working** |
| 26.9 | `<F12>` from the terminal pane (`<C-t>` first) | Nothing: `[term]` passes it to the shell, which is correct |
| 26.10 | `<F12>` with the browser association broken, as in 26.8 (v0.52.0) | The error toast also says the report's link is **on the clipboard**; pasting it into a browser opens the same pre-filled form. When the browser *does* open, the clipboard is left alone |

## 27. The preview that would not arrive (v0.12.0)

A race, not a slow load: the answer reaches the channel and the window goes to
sleep without drawing the frame that would take it out. Only ever seen once, on
a first launch, so reproducing it may take several cold starts.

27.2, 27.3, 27.4 and 27.5 are automated (`ui::preview_arrival_frame`): a file the cache
has never held shows `…` and then its own text, a revisit needs no worker at all, ten
first looks in a row all land, and an image walked onto for the first time zooms from
its own fit rather than the last picture's scale. These run the real preview thread and
take its answer through the real channel, which is why they can be about arriving.
What is left is 27.1 — a genuinely cold process, where the race the section is named
after would live. The zoom keys are `<A-i>` / `<A-o>`, not the `+` 27.4 names; see
QA-REPORT.md.

| # | Do | Expect |
| --- | --- | --- |
| 27.1 | Start filer cold, move to a text file as soon as the listing appears | The preview arrives **without touching anything else** |
| 27.2 | Walk onto a file never opened in this session — a fresh clone, a folder you have not browsed | It appears. **This is the case that was broken: not cold starts, but anything not already cached** |
| 27.3 | Walk off the file and back | Still fine (this always worked — it was the cache) |
| 27.4 | Open an image never seen this session, then zoom with `+` | It steps from the picture's own fit, not from the last image's scale. **The same commit killed this and it has never been exercised** |
| 27.5 | Restart, open ten different files in a row without revisiting any | All ten appear |

## 28. Changes made from outside (v0.12.4)

The watcher's rescan replaces the listing under whatever the cursor is on. Until
v0.12.4 that crashed the program outright when the listing shrank past the
cursor's row, so these are worth running on a real machine rather than trusting
the unit tests alone.

| # | Do | Expect |
| --- | --- | --- |
| 28.1 | Put the cursor on the **last** row, delete that file from Explorer | The row goes, the cursor lands on the new last row, **no crash** |
| 28.2 | Cursor on the last row; delete several files at the end at once | Same |
| 28.3 | Delete every file in the folder from outside | An empty listing, still responsive |
| 28.4 | Cursor on the last row of a **filtered** listing (`f`), delete the file it is on | Same, and the filter still holds |
| 28.5 | Same in the **other pane** (`<C-w>`) and in the **preview** of a directory | Neither crashes |
| 28.6 | Cursor on the last row, delete that file with `d` | Same — this is what Issue #5 reported |
| 28.7 | Rename a file from outside while the cursor is on it | The cursor follows the name or stays put; no crash |
| 28.8 | Leave the window alone -- no key, no mouse -- and delete a listed file from Explorer (v0.57.2) | The row goes within half a second, without a key being pressed. Until v0.57.2 the list kept it until the next key (#108) |

## 29. The terminal's directory, brought back (v0.14.0)

`<A-Up>` in the terminal pane (`term_cd`) asks the shell where it is, which only
works if the shell says so with OSC 7. PowerShell says nothing unless the hook
is in `$PROFILE` -- `filer shell-hook` prints it since v0.69.0, and the README shows
the same lines -- so what is being tested here is mostly the instructions.

| # | Do | Expect |
| --- | --- | --- |
| 29.1 | With **no** hook in `$PROFILE`, open the terminal (`<C-t>`), `cd` somewhere, press `<A-Up>` | A toast naming OSC 7 and the command that adds the hook, `filer shell-hook \| Add-Content $PROFILE, then <C-S-t> and <C-t>` (v0.69.0, Q50) — **not** silence, and not a wait. With the zip's `filer.exe` not on the `PATH`, `filer` is its full path: `& 'C:\…\filer.exe' shell-hook …` |
| 29.2 | Paste the README hook into `$PROFILE`, open a new terminal, `cd C:\dev`, press `<A-Up>` | The file list moves to `C:\dev` |
| 29.3 | Same with a directory whose name has a **space** and one with **Japanese** in it | Both arrive intact |
| 29.4 | `cd` to a UNC path (`\\server\share`) and press `<A-Up>` | Either it follows or it says why; no crash |
| 29.5 | Run the hook line by hand in a shell that already has Starship | The prompt still draws normally (the hook uses `LocationChangedAction`, not `prompt`) |
| 29.6 | With no `[term] shell` and PowerShell 7 installed (v0.55.0), `<C-t>` and `$PSVersionTable.PSVersion` | 7.x — the pane started `pwsh`, and `filer env` names `pwsh` as the pane's shell. With `shell = "powershell"` in `[term]`, 5.1 again |
| 29.7 | `<C-t>` with no `[term] shell`, then again with `shell = "powershell"` (v0.57.4) | The first toast names the shell: `Started pwsh — <C-t> back to the list`, then `Started powershell (Windows PowerShell 5.1) — …` (v0.65.3; before that the configured one said only `powershell`). It has to match what `$PSVersionTable.PSVersion` says |
| 29.8 | In a pane started as `powershell` (5.1) with no hook, `<A-Up>` (v0.59.4) | The red toast names the shell and says it **cannot** have the hook -- `` `powershell (Windows PowerShell 5.1)` has not said where it is (no OSC 7), and cannot: the hook needs PowerShell 7 (winget install Microsoft.PowerShell) `` (v0.69.0, Q50; before that it sent you to 5.1's `$PROFILE`, where the hook fails at every start) |
| 29.9 | Put a handler of another tool's in `$PROFILE` first (`mise activate pwsh`, or a stand-in: `$ExecutionContext.SessionState.InvokeCommand.LocationChangedAction = { param($s, $e) [Console]::Title = "other: $($e.NewPath)" }`), the README hook after it, open a new pane, `cd C:\dev`, `<A-Up>` (v0.64.2) | Both run: the list moves to `C:\dev` **and** the other tool's handler still does its job (the stand-in's title reads `other: C:\dev`). Before v0.64.2 the README hook replaced the other one |
| 29.10 | With no hook, in the pane (`pwsh`): exactly what the 29.1 toast says, then `<C-S-t>`, `<C-t>`, `cd C:\dev`, `<A-Up>` (v0.69.0, Q50) | The list moves to `C:\dev`. `Get-Content $PROFILE` ends with the lines `filer shell-hook` prints (a `# filer:` comment first), on lines of their own -- not glued to the profile's last line. `filer shell-hook powershell` and `filer shell-hook fish` print one line naming the problem, exit code 2 |
| 29.11 | Linux: `filer shell-hook bash >> ~/.bashrc` (and `zsh >> ~/.zshrc` with `[term] shell = "zsh"`), new pane, `cd` to a folder with a space and Japanese in its name, `<A-Up>` (v0.69.0, Q50) | The list moves there, both names intact. Without the hook the toast names `shell-hook bash >> ~/.bashrc` |
| 29.12 | With `[term] shell = "pwsh"` and `args` in `filer.toml`: `$env:FILER_TERM_SHELL = 'powershell'`, start filer, `<C-t>`, then `filer env` from the same shell; then `Remove-Item Env:FILER_TERM_SHELL` and again (v0.70.0, Q51) | With the variable: the toast is `Started powershell (Windows PowerShell 5.1) — …`, and `filer env` lists `powershell` as `terminal pane, from FILER_TERM_SHELL` with `FILER_TERM_SHELL` among the variables. Without it: `pwsh` and `from [term] shell` again, and the rest of `filer.toml` (font, theme) took effect both times |

## 30. Right-click paste in a prompt (v0.14.0)

| # | Do | Expect |
| --- | --- | --- |
| 30.1 | Copy a path in Explorer's address bar, press `c`+`d` (or whatever opens the `cd` prompt), right-click the field | The path appears; `<Enter>` goes there |
| 30.2 | Type `abc`, click between `a` and `b` with the **right** button | The paste lands there, not at the end |
| 30.3 | Select part of the text with a drag, then right-click **on the selection** | The selection is replaced |
| 30.4 | Copy two lines of text, right-click into `s` | One line, the break shown as a space — the same as `<C-v>` |
| 30.5 | Copy a Japanese path, right-click into `cd` | Intact, and the caret sits after it |
| 30.6 | With an image (not text) on the clipboard, right-click a prompt | Nothing happens, **no toast** |
| 30.7 | Same in the command palette, in `f`, and in `S-r` (bulk rename) | Each pastes; the bulk preview re-renders |
| 30.8 | Right-click in the **file list** | Still the context menu — the list is unchanged |
| 30.9 | Right-click in the **terminal** pane (`<C-t>`) | The clipboard is typed in, and the pane takes the keys if it did not have them |
| 30.10 | Select text in the terminal with a drag, then right-click | The selection was copied on release; the right-click pastes it back — select to copy, right-click to paste |
| 30.11 | Copy **three lines** and right-click into the terminal at a PowerShell prompt | All three sit in the buffer, **nothing runs** until `<Enter>` (PSReadLine asks for bracketed paste) |
| 30.12 | The same in a shell that does **not** ask for bracketed paste (`cmd.exe`) | The lines run, as they always have — and no stray `[200~` appears |
| 30.13 | Right-click in the terminal while `vim` is open | The text is inserted; no `[200~` on screen |
| 30.14 | `<C-v>` in the terminal | Same as the right-click, including 23.11 |
| 30.15 | `r` on `report.txt`, `R` on two files, `gSpace` (`cd`), `E` on `a.txt` (v0.55.0); then, with a path on the clipboard, right-click **on** the `cd` prompt's selection | Each opens with its text selected, so typing replaces it: `r` selects `report`, `R` all of `{name}{ext}`, `cd` the whole path, `E` the `a` before `.zip`. The right-click's path replaces the selection, and `<Enter>` goes there |

## 31. A host's shares (v0.16.0)

Only testable against a real file server, and the interesting cases are the
ones where it says no.

| # | Do | Expect |
| --- | --- | --- |
| 31.1 | `g`+`<Space>`, type `\\<your server's address>`, `<Enter>` | The shares are listed, the same ones Explorer shows |
| 31.2 | Same with a host **name** rather than an address, and with the `//` spelling | Both arrive; the path is shown back in the `\\host` spelling |
| 31.3 | Walk into a share and back out with `h` | Into the share, then back to the host list |
| 31.4 | `h` again, at the host | Nothing moves (the host is the top), no crash |
| 31.5 | A host that is off, or does not exist (an unused address on your own subnet) | The tab returns to where it was and a toast says why — it does not hang the window |
| 31.5a | 24.1 and 24.5 again, watching for a **toast** | v0.16.0 fell back to the parent in silence, so a failure looked like nothing happening. Whatever the outcome, there is now either a listing or a message; if it is still a message, its os error number is the thing to report |
| 31.6 | A host that needs a login the machine has not been given | Same: a refusal as a toast, naming it |
| 31.7 | A host with **many** shares (more than a screenful) | All of them, scrolling normally |
| 31.8 | A share name with a space or non-ASCII in it | Intact |
| 31.9 | Hover a share and look at the size column | Empty — there is nothing to read, and it must not sit there counting |
| 31.10 | `<C-r>` / refresh on the host listing | Re-asks the server; no crash |
| 31.11 | Open the host in the **other pane** (`<C-w>`) and in a second tab | Both fine |
| 31.12 | Go to a host, then change directory away | The watcher does not complain about the host it could not watch |
| 31.13 | A host that answers but shares nothing (v0.57.3) | The list says `(no shares)`, not `(empty)` |
| 31.14 | `g<Space>` an address on your subnet that nothing answers on, `<Enter>`, then `<Esc>` before it gives up (v0.58.1) | Back where you were at once, a toast `Stopped waiting for \\<address>`, `j` / `k` work again -- and nothing more is said when the abandoned attempt times out later |
| 31.15 | `g<Space>` an address that does not answer, `<Enter>`, and read the header before it gives up (v0.59.8) | The header's count reads `listing…` and the position `…` -- not `0 items` and `0/0`, which read as having arrived at an empty host. Once it answers, or `<Esc>` takes the tab back (31.14), the counts return |

## 32. Openers (v0.17.0)

The README's example config is the thing under test: if a step here fails, the
instructions are wrong, which is worse than a missing feature.

| # | Do | Expect |
| --- | --- | --- |
| 32.1 | Paste the README's `[opener]` / `[open]` example into `yazi.toml`, restart, `<S-Enter>` on a `.txt` | Neovim / VS Code / サクラエディタ / Open with the default app, in that order, by their descriptions rather than the command lines (the example's order since v0.71.11) |
| 32.2 | `<Enter>` on the same file | Opens in the first entry, Neovim, in a console window of its own (`block = true`): a `nvim` process whose command line ends in the file's quoted path |
| 32.3 | `<S-Enter>` on a `.pdf` | Edge and Chrome first, then the default-app entry |
| 32.4 | `<S-Enter>` on a `.xlsx`, pick Excel | Excel opens it — this is the `start ""` case that fails without it |
| 32.5 | A file whose name has a **space**, through each of the above | One argument, opens correctly |
| 32.6 | Several files selected, then `<Enter>` | All of them go to one invocation |
| 32.7 | A rule written `*.{xlsx,xls,csv}` | Matches all three (this is what did not work before v0.17.0) |
| 32.8 | An opener naming a program that is not installed | An error toast within a few seconds, no hang |
| 32.8a | An opener whose program is a **quoted full path** holding a space and parentheses: `"C:\Program Files (x86)\sakura\sakura.exe" %*` | It opens: `Win32_Process` shows `sakura.exe` with the file's quoted path as its one argument. This is the v0.17.0 bug: `cmd` mangled the line and the failure was silent |
| 32.8b | The same opener from `<S-Enter>` **and** from `<Enter>` with it moved to the top of `edit` | Both start `sakura.exe` on the file, since they take different code paths to the same launcher |
| 32.8c | An opener with a deliberate typo in the path | A toast naming the failure. On a Japanese Windows expect the exit code rather than `cmd`'s own words — that is intended, not a bug to report |
| 32.9 | `l` into a Markdown file's outline, `j` to a heading further down, `<S-Enter>` and pick サクラエディタ (v0.47.29 changed its switch from `-L=` to `-Y=`) | `Win32_Process` shows `sakura.exe -Y=<the heading's line> "<file>"`, the line counted from 1 as the preview counts it |
| 32.9a | The same, looking at サクラエディタ | Its caret sits on that heading's line. Sakura ignored `-L=`, so before v0.47.29 it opened at line 1 or where the file was last left. A look: the owner's, or a run that can read the editor's line another way |
| 32.10 | An opener whose program is misspelled (`run = 'Hidemruu.exe %s'`), `<S-Enter>` and pick it (v0.59.1) | The error reads ``Open failed: `Hidemruu.exe` was not found — …``, not `exit code 1`. An opener whose program exists but fails still gives its exit code |
| 32.11 | Linux: a `block = true` opener (`run = 'nvim %*'`; `vim` will do), `<Enter>` on a file whose folder and name hold a space and a `'` (v0.72.0) | A terminal window opens with the editor in it. `ps` shows the editor got the whole path as one argument, and `/proc/<pid>/cwd` is the list's folder |
| 32.12 | Linux: the same with `TERMINAL="xterm -title picked"` | The window is titled `picked` (`xdotool getwindowname`). `filer env` names `xterm -title picked` on the `block = true openers` row; with `TERMINAL` unset it names the first of the built-in list that is installed |
| 32.13 | Linux: a `block = true` opener naming a program that is not installed | The terminal stays open on `[exit 127] Press Enter to close.` instead of flashing shut; `<Enter>` in it closes it |
| 32.14 | macOS: a `block = true` opener, `<Enter>` on a file | Terminal.app comes forward with a new window running the editor in the list's folder. The first time, macOS asks whether filer may control Terminal; refused, a toast says why |

---

## 33. Config warnings, and their colour (v0.20.1)

A warning here means a line in your own `keymap.toml` cannot take effect. The
colour is the thing under test: red is reserved for something that failed, and
none of these failed.

33.1, 33.2, 33.3, 33.7 (the box half), 33.8 and 33.10 are automated
(`ui::config_warning_frame`): the wording, that the toast and the `~` rows are drawn in the
warning colour and that nothing in the frame is framed as a failure, the count the toast carries
when there are more, the files coming before the complaints in `~`, and the box staying inside
the window at full width and at a third of it, cut at eight lines. **33.4** is in the same module:
a real failure is raised beside the config warning and the two boxes come out in the two colours,
one each. 33.5 is automated only in the
part that does not depend on the machine — a warning the config no longer has leaves the panel —
because `<C-F5>` re-reads the real config files, so what its toast says depends on what is on the
machine. **33.9's expectation** is automated there too — five boxes at most, stacking downward,
each the height of its own text, none over the next — but **its recipe is not, because it cannot
be**: three broken config files raise one toast, not three (that is 33.2). The row needs rewording;
see QA-REPORT.md.

33.11 to 33.14 have unit tests of their own in `config::files`, including the two lines a file
holding both misplaced sections gets and the `belongs in yazi.toml` direction. **33.16, 33.17 and
33.18** are unit-tested in `ui::overlay::help_config_rows`, against `config_rows` with a real file
written into a temp directory: the marked row and its note, the marker coming off once the file is
among the ones read, and the note naming a rebound `<F9>`.

What is left for an eye: that the yellow *reads* as advice rather than as a failure at a glance
(33.4 — a test can say the two colours differ and which is which, not that a person tells them
apart), that it is legible on a light theme (33.6), the parse error's own wording (33.7), and
33.15, which needs the files really on disk, `filer env`, and a terminal pane that starts.

| # | Do | Expect |
| --- | --- | --- |
| 33.1 | Start with a `keymap.toml` that binds a key the defaults also bind (e.g. `'` to `plugin bookmarks jump`) | A **yellow** toast, not red: `Config: [mgr] \`'\` is bound more than once; only ... runs` |
| 33.2 | With three or more such lines | The toast ends `(+2 more, see \`~\`)` |
| 33.3 | Press `~` | The loaded config files, then every warning, all in the same yellow |
| 33.4 | Make something actually fail (an opener naming a program that is not installed, 25.8) | Still **red**, so the two are told apart at a glance |
| 33.5 | Remove the duplicate lines, `<C-F5>` | `Reloaded N config file(s)` in the plain colour; no yellow |
| 33.6 | A theme with a light background | The yellow is still readable; say so if it is not — it is a fixed default, not yet themeable |
| 33.7 | Put a real syntax error in `yazi.toml` (`[mgr` with no `]`) and start | A **five-line** parse error, naming the line and pointing at it. **Inside its box**: nothing over the header, nothing over the file list, nothing past either edge of the window (v0.33.11) |
| 33.8 | Narrow the window to about a third of the screen, with 33.7 still broken | The message wraps rather than running off; the box stays against the right edge |
| 33.9 | Break **three** config files at once | Up to five boxes stack downward, each sized to its own text, none overlapping the next |
| 33.10 | A single error longer than eight lines | Cut at eight with `…` on its own line, rather than filling the window |
| 33.11 | Put `[[preview]]` into `yazi.toml` (it belongs in `filer.toml`) and start | **One line**: `…\yazi.toml: [[preview]] belongs in filer.toml, and nothing in this file was read`. Not the old `invalid type: map, expected a string` (v0.33.13) |
| 33.12 | Put `[term]` into `yazi.toml` as well | A second line for it, same shape. Both say the file went unread, because it did |
| 33.13 | Put `[term]` into a `yazi.toml` that is otherwise fine (no `[[preview]]`) | `… belongs in filer.toml and was ignored` — *ignored*, not *unread*: the rest of the file did load |
| 33.14 | Put `[opener]` into `filer.toml` | The same warning the other way round: `belongs in yazi.toml` |
| 33.15 | Move both into the right files, `<C-F5>` | No warnings. `filer env` agrees, and the terminal pane now starts what `[term] shell` names |
| 33.16 | With filer **already running**, create `%APPDATA%\filer\filer.toml`, then press `~` | The file is a row of its own, in the warning colour, reading `on disk, not read yet — <C-F5> re-reads config`. The directory is **not** `nothing here` (v0.34.0) |
| 33.17 | `<C-F5>`, then `~` again | The row is now an ordinary loaded file, no marker |
| 33.18 | Rebind `config_reload` to `<F9>` and repeat 33.16 | The row names `<F9>`, not `<C-F5>` — it is read from the keymap, not written into the message |

---

## 34. The help panel's own scrolling (v0.34.0)

Everything here is `~` / `F1`. The list is long enough to scroll only if the
keymap is; the defaults are.

34.1–34.9 and 34.11–34.13 are automated (`ui::overlay::help_frame`), 34.10 for the task panel and
the spotter, and 34.14 in part. The panel measures its own height from the frame, so the distances
here are that height and not the file list's — in one 1280x800 window `<C-d>` moves the panel 13
lines and the list 15, which is the difference 34.2 is about. The stop with the last line at the
bottom, the immediate return from it, the wheel reaching the panel and not the list under it, the
one-row prompt that still lets the list scroll, the four closing keys and a rebound key are all in
`cargo test`. **34.14 is only half covered**: that a taller panel comes back to the new bottom is
asserted, but `<C-->` itself is not yet driven from a test. The `[help]` layer had no scale binding
at all until v0.45.9, which is why the key did nothing while the panel was open; it works now, and
the second half of 34.14 is there to be automated. See QA-REPORT.md. What is left for an eye is the pointer feel —
wheel speed, and the pointing-hand cursor over a config path — and that the text is legible at the
size the panel comes out.

| # | Do | Expect |
| --- | --- | --- |
| 34.1 | `j` / `k`, then the arrows | One line each way |
| 34.2 | `<A-j>` / `<A-k>` | Half the panel's height each way — not the file list's |
| 34.3 | `<C-d>` / `<C-u>` | The same distance as 34.2 |
| 34.4 | `<PageDown>` / `<PageUp>` | A whole panel each way |
| 34.5 | `G` | The **last line sits at the bottom** of the panel, with the panel full — not one line at the top of an empty panel |
| 34.6 | From there, one `k` | Moves immediately. Before v0.34.0 `j` ran the number off the end, so coming back took one dead press per overshoot |
| 34.7 | `gg` | Back to the top; another `k` does nothing |
| 34.8 | The wheel, pointer over the panel | Scrolls the panel (v0.34.0: it did nothing at all before) |
| 34.9 | Close the panel and look at the file list's cursor | **Unmoved.** The wheel used to reach the list underneath as well, which only showed up as a jump once the panel was closed |
| 34.10 | Open the spotter, the task list or a comparison and turn the wheel | Same: nothing underneath moves |
| 34.11 | With a filter or rename prompt open (`Overlay::Input`), turn the wheel over the list | The list **does** scroll — the prompt is one row, and the list above it is what is being read |
| 34.12 | `q`, then `~` again, then `<F1>`, then `<Esc>` | Each one closes the panel |
| 34.13 | Rebind: `[[help.keymap]]` with `on = "n"`, `run = "arrow 1"`, `<C-F5>` | `n` scrolls. Before v0.34.0 the panel's keys were read off the event loop and could not be rebound at all |
| 34.14 | Shrink the font with `<C-->` while parked at the bottom | Still parked at the bottom, panel full — more lines fit, so the stop moved |
| 34.15 | `C` (v0.67.16) | Toast `Copied the help panel: N keys`. The clipboard holds the panel as text: `config` and the paths, then `keys` on a line of its own, then one `keys<TAB>description<TAB>command` line per key -- `Get-Clipboard \| Select-String "^j\t"` finds `j<TAB>Move cursor down<TAB>arrow 1`. The panel stays open |

---

## 35. Where the config is looked for, per platform (v0.35.0)

`filer env` prints the two directories in effect and whether each file was
found, so most of this is readable without a GUI. The Linux and `XDG_CONFIG_HOME`
rows were verified in the development container; **the macOS rows cannot be, and
Windows needs confirming that nothing moved.**

| # | Platform | Do | Expect |
| --- | --- | --- | --- |
| 35.1 | Windows | `filer env` with both variables unset | `%APPDATA%\yazi\config` and `%APPDATA%\filer` — **unchanged from v0.34.0.** This is the row that must not have moved |
| 35.2 | Windows | Put `[mgr] sort_by = "mtime"` in `%APPDATA%\yazi\config\yazi.toml` | Read. yazi's own directory still shares with filer |
| 35.3 | macOS | `filer env` | `~/.config/yazi` and `~/.config/filer`, **not** `~/Library/Application Support/…` |
| 35.4 | macOS | Install yazi, run `yazi` once, put a `yazi.toml` where yazi reads it | filer reads the same file. This is the whole point of the change: before v0.35.0 filer looked under `~/Library/Application Support/yazi/config/`, which yazi never writes |
| 35.5 | macOS | Anyone upgrading with config in `~/Library/Application Support/filer/` | It is **no longer read** — `filer env` lists it as missing. Move it to `~/.config/filer/`. Called out as a 変更 in CHANGELOG |
| 35.6 | Linux | `filer env` | `~/.config/yazi` — **not** `~/.config/yazi/config` |
| 35.7 | Linux / macOS | `XDG_CONFIG_HOME=/tmp/x filer env` | `/tmp/x/yazi` and `/tmp/x/filer` |
| 35.8 | Linux / macOS | `XDG_CONFIG_HOME=relative filer env`, and again with it empty | Falls back to `~/.config/…`. XDG says a relative value is ignored |
| 35.9 | Any | `last-run.toml` | Still in the state directory (`data_dir()`), which this change did **not** touch. On Windows that is the same `%APPDATA%\filer`; on Linux `~/.local/share/filer` |
| 35.10 | Any | Symlink `filer.toml` into the config directory from elsewhere, then `<C-F5>` | Read through the link. Re-check after editing via the **link path** with an editor that saves by rename — that replaces the symlink with a regular file |

---

## 36. `T`, and how it differs from `<F3>` (v0.36.0)

The binding and its parse are covered by tests; what needs eyes is the drawing,
and the fact that these two are not the same thing at two sizes.

36.1 to 36.9, 36.11, and 36.12 to 36.16 of the `q` table are automated
(`ui::max_preview_frame`) — down to `<F3>`'s panel measuring 86% × 88% of the window and `T`
drawing no rectangle of that size at all, which is this section's own point put as an assertion.
What is left is 36.10 and 36.18, both of which need a rewritten `prepend_keymap` and a `<C-F5>`,
and half of 36.17: that `q` does not end the process is covered, but "Nothing happens" disagrees
with the code. See QA-REPORT.md.

| # | Do | Expect |
| --- | --- | --- |
| 36.1 | Hover a text file, press `T` | The preview column takes the whole body. The tab bar and status bar are **unchanged**, the background is **not** dimmed, and there is no frame or file name across the top |
| 36.2 | `T` again | The three columns come back at the `[mgr] ratio` widths |
| 36.3 | With `T` up, `j` / `k` | The cursor still walks the list and the preview follows, even though the list column is squeezed to nothing |
| 36.4 | With `T` up, `<A-j>` / `<A-k>` | Scrolls the preview |
| 36.5 | With `T` up, press `<Esc>` | The columns come back (v0.36.1). Before that `<Esc>` did nothing here |
| 36.5a | With `T` up, press `q`. Then `q` again | First `q` restores the columns, second quits. Before v0.36.1 the first `q` quit the app outright |
| 36.5b | Filter the list, then `T`, then `<Esc>` twice | First `<Esc>` restores the columns, second clears the filter. A maximized preview is the most visible state, so it goes first |
| 36.5c | With `T` up, run `escape --filter` from the command line (`:`) | The columns **stay** maximized — a targeted escape is still targeted |
| 36.6 | `<F3>` for comparison | Dimmed background, a framed panel at 86% × 88% with the file's name as its title and "Esc to close". A visibly different thing from 36.1 |
| 36.7 | `T`, then `<F3>`, then `<Esc>` | The panel closes and the **maximized column is still maximized** — the two flags are independent |
| 36.8 | Hide the parent pane (`toggle-pane max-parent`), then `T` on and `T` off | The parent pane is **back** — turning `T` on clears `hide_parent`, and toggling off does not restore it. Deliberate, but it means `T` is not quite a round trip |
| 36.9 | `T` on a directory, and on a file with no preview | No panic, no stuck layout; `T` still toggles back |
| 36.10 | Bind `<S-t>` instead of `T` in `prepend_keymap`, `<C-F5>` | **Nothing happens on any key** — the lesson the tests pin. No warning is printed either, because the notation is valid |
| 36.11 | `~` / `F1` | `T` is listed with its description, in the keymap the panel shows |

### `q` means the same thing everywhere (v0.36.1)

One `q` closes what is in front; only with nothing up does it quit. Walk all of
these — the point is that no panel is the odd one out.

| # | Panel | Do | Expect |
| --- | --- | --- | --- |
| 36.12 | quick look (`<F3>`) | `q` | Closes the panel. **The app is still running** — before v0.36.1 this quit |
| 36.13 | maximized preview (`T`) | `q` | Columns back, app still running |
| 36.14 | `help` (`~`), task list, spotter (`Tab`), comparison (`<A-d>`) | `q` in each | Closes, app still running (unchanged — these already had their own layer) |
| 36.15 | Nothing up | `q` | Quits on the first press |
| 36.16 | `<F3>` **and** `T` both on | `q`, `q`, `q` | Panel, then columns, then quit. Same three presses with `<Esc>`, `<Esc>`, `q` |
| 36.17 | A confirm prompt (delete something) or a pick list | `q` | **Cancels, exactly as `<Esc>` does** — `answer_confirm` takes any key it does not recognise as a cancel. The app must not quit, and does not: the prompt swallows the `q` |
| 36.18 | Rebind: `[[mgr.keymap]]` with `on = "Q"`, `run = "quit"`, then `Q` with `<F3>` up | Closes the panel first, like `q` — the behaviour is on the action, not the letter |

---

## 37. `start ""` openers actually launch (v0.36.2)

Reported from a real machine: `<Enter>` on a PDF opened a command prompt whose
title bar read `msedge C:\Users\…\x.pdf`, with no browser. Every opener beginning
`start ""` was affected, so walk the common ones. The quoting is unit-tested; what
needs a machine is that the program really starts.

| # | Do | Expect |
| --- | --- | --- |
| 37.1 | `<Enter>` on a `.pdf` with `browser = [{ run = 'start "" msedge %*' }]` first | **Edge opens the PDF.** No command prompt appears |
| 37.2 | `<Enter>` on `.xlsx` / `.docx` / `.pptx` with `start "" excel %*` and friends | The Office app opens the file |
| 37.3 | `<Enter>` on anything routed to `open = [{ run = 'start "" %*' }]` | The file's associated app opens it |
| 37.4 | A file whose **name contains a space**, through any of the above | Opens as one file, not two. The path keeps its quotes |
| 37.5 | An opener written `start "" msedge "%*"` (placeholder quoted by hand) | Same result as 37.1 — the pair around the placeholder is still absorbed |
| 37.6 | Select two PDFs, `<Enter>` | Both open as separate arguments, not one quoted blob |
| 37.7 | An opener given as a full path that does not go through `start`: サクラエディタ's `"C:\Program Files (x86)\sakura\sakura.exe" %*` | Unchanged by the `start` handling: `Win32_Process` shows `sakura.exe` started directly, with no `start` and no extra console |
| 37.8 | `O` on a PDF | The picker lists Edge, Chrome, the default app, then the editors; each entry launches what it says |

---

## 38. The focus rule, in both panes that draw one (v0.36.3)

Colour, so it needs eyes. The point is that the two panes agree — check them
side by side, not one at a time.

38.1 to 38.6, 38.9, and 38.7 and 38.8 in part are automated (`ui::preview::focus_rule_frame`):
the harness reads a stroke's colour from v0.47, so a one-pixel rule is findable, and the terminal
pane really is opened — `<C-t>` starts a shell in the test as it does on a machine, because the
pane is not laid out at all without one. What it checks is that exactly one rule is accent with
both panes on screen, either way round, that the mouse moves it as the key does, and that the cell
cursor is filled while the pane has the keys and outlined when it does not. 38.7 and 38.8 are
covered where they are about the *theme* — both rules read one `tab_active` entry, and clearing it
falls back to the foreground rather than to the border — but the entry is set in memory, not read
from a `theme.toml` and reloaded. What is left for an eye: that `#7ab8f5` and `border` are
distinguishable at one pixel, and that a `theme.toml` really reaches them through `<C-F5>`.

| # | Do | Expect |
| --- | --- | --- |
| 38.1 | `<C-t>` to open the terminal and give it the keys | The rule along the **top of the terminal** is accent-coloured (`#7ab8f5` by default), not grey |
| 38.2 | `<C-t>` again to hand the keys back to the list | The same rule goes grey (`border`) |
| 38.3 | Open a file with an outline, focus the Contents pane | Its vertical rule is accent — **the same colour** as 38.1, not a different one |
| 38.4 | Terminal focused **and** an outline on screen at once | Exactly one rule is accent: the terminal's. The outline's is grey |
| 38.5 | Focus the outline with the terminal open but unfocused | The other way round — outline accent, terminal grey |
| 38.6 | Click into the terminal with the mouse instead of `<C-t>` | The rule follows the click, same as the key |
| 38.7 | `theme.toml` with `[mgr] tab_active = { bg = "#ff0000" }`, then `<C-F5>` | **Both** rules turn red. They read one definition |
| 38.8 | A theme that sets no `tab_active` background | The focused rule falls back to the foreground colour and is still visibly different from the unfocused one |
| 38.9 | The terminal's own cell cursor | Still filled when focused, hollow when not. The rule is added to that, not a replacement — v0.20.2 removed the rule on the grounds that this was enough, and it was not |

---

## 39. `<A-j>` / `<A-k>` in the terminal pane (v0.37.0)

The binding and its direction are unit-tested; what needs a machine is that the
scrollback really moves, and that the keys no longer reach the shell.

| # | Do | Expect |
| --- | --- | --- |
| 39.1 | `<C-t>`, run something long (`dir /s` or `ls -R`), then `<A-k>` | The scrollback goes **up** five lines per press |
| 39.2 | `<A-j>` | Back **down** five lines. Same direction as in the file list, where these scroll the preview |
| 39.3 | Hold `<A-k>` to the top, then `<A-j>` back | Stops at each end without overshooting — no dead presses coming back |
| 39.4 | `<S-PageUp>` / `<S-PageDown>` / `<S-Home>` / `<S-End>`, and the wheel | Unchanged |
| 39.5 | With the terminal **unfocused** (`<C-t>` back to the list), `<A-j>` | Scrolls the **preview**, not the terminal. The layer decides, not the key |
| 39.6 | In the pane, run a program that reads Alt+j — `nvim` with `nnoremap <A-j> :m+1<CR>` | **It does see the key** from v0.38.0 — see section 40. Before that it did not |
| 39.7 | `[[term.prepend_keymap]]` binding `<A-j>` to `noop`, then `<C-F5>` | The key does nothing **and still does not reach the shell** — anything bound here is consumed. Handing it back needs a full `[term] keymap = [...]` replacement |
| 39.8 | Alt+b / Alt+f / Alt+d at the shell prompt | Still reach readline. Only j and k were taken |
| 39.9 | `<F1>` from inside the pane | The term layer's list shows `<A-j>` / `<A-k>` with their descriptions |

---

## 40. Full-screen programs get the scrolling gestures (v0.38.0)

The alternate-screen flag and the meta encoding are unit-tested, including the
premise the whole thing rests on — that screen really does have no scrollback.
What needs a machine is the handover, in a real `nvim` and a real pager.

| # | Do | Expect |
| --- | --- | --- |
| 40.1 | `<C-t>`, `nvim` a long file, `<A-j>` / `<A-k>` with `nnoremap <A-j> :m+1<CR>` bound | **nvim sees the key.** The v0.37.0 collision is gone |
| 40.2 | In the same nvim, `<S-PageUp>` / `<S-PageDown>` / `<S-Home>` / `<S-End>` | All reach nvim. Every `term_scroll` key is handed over, not just the two |
| 40.3 | In the same nvim, `<C-t>` | **Still filer's** — it leaves the pane, with nvim left running. Non-scrolling keys are never handed over |
| 40.4 | Quit nvim, then `<A-j>` / `<A-k>` at the shell prompt | Back to scrolling filer's scrollback. The handover follows the program, not a setting |
| 40.5 | The wheel inside nvim, and inside `less` | Scrolls the document. Before v0.38.0 it tried to walk a scrollback that does not exist, so nothing moved |
| 40.6 | The wheel at the shell prompt | Still walks the scrollback, unchanged |
| 40.7 | `less` a long file, `<S-PageUp>`, then `q` to quit, then `<S-PageUp>` again | Inside `less` it pages the document; after quitting it scrolls the pane's scrollback |
| 40.8 | In `less`, one notch up then one notch down (v0.55.0) | Lands back where it started — one notch is the same number of arrows each way. Until v0.55.0 the first notch after turning round was lost (#100) |
| 40.9 | A program using the alternate screen **and** application-cursor mode | The wheel's arrows arrive as SS3 (`ESC O A`), not CSI. nvim in insert mode is the easy check |

### `Alt`+letter reaches the shell at all (v0.38.0)

| # | Do | Expect |
| --- | --- | --- |
| 40.10 | At a `bash`/`zsh` prompt in the pane, type a few words, then `Alt-b` / `Alt-f` | The cursor moves **by word**. Before v0.38.0 nothing happened — the key was dropped with no bytes behind it |
| 40.11 | `Alt-d` at the same prompt | Deletes the word ahead |
| 40.12 | PowerShell (PSReadLine) in the pane, `Alt-b` / `Alt-f` | Same word motions |
| 40.13 | `Alt-j` / `Alt-k` at an ordinary prompt | **Still filer's scroll** — these two are bound in the `[term]` layer, and the prompt is not the alternate screen |
| 40.14 | The wheel inside nvim, with `FILER_PTY_LOG` set (v0.55.0) | nvim's view scrolls and **its cursor stays on the same line** (`:echo line('.')` before and after). The log shows `\e[<64;…M` / `\e[<65;…M`, not `\e[A` |
| 40.15 | lazygit in the pane (v0.55.0): about 300 `<S-End>` at 30 a second, with `?` then `<Esc>` in the middle of them, as #99 rebuilt #93 | The key list closes within a second of the `<Esc>`. Until v0.55.0 it stayed open for minutes: `<Esc>` sent as a record and `<S-End>` as `\e[1;2F` right behind it read to tcell as one sequence |
| 40.16 | With `FILER_PTY_LOG` set (v0.55.0): open the pane, type `ping -t localhost`, `<C-c>`, then `<C-Left>` over a typed word and `<Tab>` completion | The log's `out` lines hold `\e[?9001h` near the start, and the `in key` lines are records (`\e[…;…;…;1;…;1_`) rather than `\e[1;5D`; `<C-c>` stops the ping, `<C-Left>` moves by a word, `<Tab>` completes — the shell reads records as it reads a real keyboard |
| 40.17 | With `FILER_PTY_LOG` set, inside nvim, one notch of the wheel; then three (v0.58.1) | One `\e[<64;…M` (or `65`) per notch: 1, then 3. Until v0.58.1 three notches sent five, because the smoothed delta was counted |

---

## 41. The spot panel's four new providers (v0.39.0)

The parsing is unit-tested from bytes built by hand, so it reads the same on every target. What
needs a machine is real files, and the panel's own geometry.

| # | Do | Expect |
| --- | --- | --- |
| 41.1 | `<Tab>` on a `.zip` from the fixtures | An **Archive** section: format, entry and folder counts, unpacked size, ratio, `Encrypted: no` |
| 41.2 | `<Tab>` on a zip made **encrypted by 7-Zip** | `Encrypted: yes (entries need a password)`, and the counts are still there. **Cannot be unit-tested — this build of `zip` has no AES writer, so no encrypted fixture can be made in-tree** |
| 41.3 | `<Tab>` on a 7z made with "encrypt file names" | `Encrypted: yes (the listing itself)` and **no counts at all** (nothing below is known) |
| 41.4 | `<Tab>` on an archive with more than 20,000 entries | The panel arrives without the window stalling, and says `Scanned: first 20,000 entries` |
| 41.5 | `<Tab>` on a CRLF file saved by Notepad, then on an LF one | The `Line endings` row tells them apart, with counts |
| 41.6 | `<Tab>` on a Notepad "UTF-16 LE" save | `Encoding: UTF-16 LE`, `BOM: UTF-16 LE (FF FE)` — **not** treated as binary |
| 41.7 | `<Tab>` on a 2 GB log | Rows arrive promptly, `Scanned: first 1.0 M of …`, and **no `Final newline` row** (the end was never read) |
| 41.8 | `<Tab>` on each of the six release binaries | `Architecture` matches the triple the artifact is named for — `x86_64` / `aarch64` |
| 41.9 | `<Tab>` on `C:\Windows\explorer.exe`, then on a `.dll` | `Windows GUI` / `DLL` |
| 41.10 | `<Tab>` on a real `.docx` / `.xlsx` / `.pptx` saved by Office | Author, revision, times marked **UTC**, word / page / slide counts |
| 41.11 | `<Tab>` on an old `.doc` | **No Document section, and no error** |
| 41.12 | Look at the key column on every new section | No key runs into the value column (`overlay.rs` hard-codes `key_w = 130.0`) |
| 41.13 | `<A-j>` down into a new section's rows, then `y` | The right value is copied. **`Act::Copy` counts rows across every section, so the new sections shift the indices** |
| 41.14 | `<Tab>` on a folder on a slow network drive | The panel still follows the cursor; the spot worker is newest-wins |

## 42. The minimap's hover card (v0.40.0)

The geometry and the clamp are unit-tested. What needs a machine is the timing, the drag, and whether
it is legible against a real theme.

All of 42.1 to 42.13 except 42.6's "shrinks to the gutter" and 42.7's "no frame hitch" are automated
(`ui::preview::minimap_hover_frame`): the harness delivers egui's own pointer events and moves the
clock the delay is measured against, so the card can be hovered, dragged, clicked, themed and turned
off from a test. Note that the delay is egui's `tooltip_delay`, half a second by default rather than
the 0.4 s 42.1 says; the tests assert that there is no card a fifth of a second in and one after the
delay, not a number. What is left for an eye: that the card is legible where it lands, and that a
2000-character line costs no visible hitch.

| # | Do | Expect |
| --- | --- | --- |
| 42.1 | Hover the strip on a long source file and **hold still** | After about 0.4 s, a one-row card left of the strip: the line number, then that line in the body's own colours |
| 42.2 | Sweep the pointer along the strip | The card follows. It never crosses into the strip, and never leaves the pane at either end |
| 42.3 | Click where the card points | The preview jumps to **that** line, centred — the card was a preview of the click |
| 42.4 | Hover the very **bottom pixel** of the strip | The last line, not a blank card (the clamp this release fixed) |
| 42.5 | Press and **drag** up and down | The card **stays up and keeps following**, so you can find the place before letting go |
| 42.6 | Hover a blank run | The number alone, in a card that shrinks to the gutter |
| 42.7 | Hover a 2000-character line | Ellipsized on one row, no frame hitch |
| 42.8 | Move the pointer off the strip and back | The delay starts again; no card left behind |
| 42.9 | With a yazi `theme.toml` setting `[mgr] preview_hovered` (try `fg` and `bg`) | The card follows it. **This key never did anything before this release** |
| 42.10 | Hover with the outline column up | The card covers the outline briefly and leaves nothing behind |
| 42.11 | `M` on a Markdown file to get the source view, then hover | The card works there; in the rendered view there is no strip at all |
| 42.12 | Hover on a file truncated at `max_text_bytes` | The number matches the body's own numbering for that line |
| 42.13 | `<A-n>` to turn the minimap off | No strip, and so no card |

## 43. CSV / TSV as a table (v0.41.0)

The parsing and the layout are unit-tested from strings. What needs a machine is real files, from
real tools, at a real pane width.

43.1, 43.2, 43.3, 43.4, 43.5, 43.6, 43.7, 43.8, 43.10, 43.12 and 43.13 are automated
(`ui::csv_table_frame`): real files on disk, read by the real worker, laid out to the width the
frame measured — and 43.4 resizes the window between two frames and watches the table come back
at the new width, which is the fix v0.41.0 shipped and the one thing here no test from a string
could have. What is left is a file from Excel itself (43.1 and 43.7 use files written here, not
by Excel) and 43.9's 50 MB. 43.11 does not match the program: with the table up there is no
minimap at all — see QA-REPORT.md.

| # | Do | Expect |
| --- | --- | --- |
| 43.1 | Hover a `.csv` saved by Excel | An aligned table: header, rule, rows. Numeric columns right-aligned |
| 43.2 | Hover a `.tsv` | Split on tabs, not commas |
| 43.3 | `M` on a `.csv` | The raw text, with syntax-free plain lines; `M` again goes back to the table |
| 43.4 | **Resize the window** with a table up | The table **re-lays out** to the new width. Before v0.41.0's fix it would have stayed at 80 columns for ever |
| 43.5 | Narrow the pane until the table cannot fit | The widest columns squeeze and their cells wrap; nothing runs off the side |
| 43.6 | A file with a quoted field holding a comma and a newline | One cell, on one row — not split |
| 43.7 | A CSV saved by Excel as "CSV UTF-8" (has a BOM) | The first column's header is not prefixed with a stray character |
| 43.8 | A ragged file (rows with different column counts) | Lays out; short rows are padded, no panic |
| 43.9 | A 50 MB CSV | Opens promptly, cut at `max_text_bytes`, footer says truncated |
| 43.10 | A one-line CSV | One row and **no rule** under it |
| 43.11 | The minimap with a table up | It maps **the file**, not the table, and its hover card shows raw CSV lines |
| 43.12 | A `.csv` that is actually binary | Still a hex dump, as before |
| 43.13 | A CJK-heavy CSV | Columns line up (widths are measured in cells, not chars) |

## 44. Disk usage (v0.42.0)

The walk and the ordering are unit-tested on a small tree. What needs a machine is a real disk: a
`node_modules`, a Windows drive root, a network share.

| # | Do | Expect |
| --- | --- | --- |
| 44.1 | `gu` in a project with a `node_modules` | Children largest first, with bars; `node_modules` near the top with a total far bigger than its own entry |
| 44.2 | `gu`, then `<Esc>` | Back in the directory, cursor where it was. The walk stops (no CPU after leaving) |
| 44.3 | `gu` on a tree with 300k+ files | Finishes, and says the walk was cut short and the totals are floors |
| 44.4 | `gu` in a folder holding a `.gitignore`d `target/` or `build/` | It is **counted**, not skipped |
| 44.5 | `gu` where a hidden folder holds most of the space | It is counted, and visible |
| 44.6 | `gu` on a folder with a symlink/junction to a big tree | The link is one entry, not a second copy of the tree, and no hang |
| 44.7 | `gu` at `C:\` | Answers; the biggest folders are plausible against WizTree or Explorer's own |
| 44.8 | `gu` on a network share (UNC) | Answers or fails gracefully; `<Esc>` still gets out mid-walk |
| 44.9 | `gu`, then `j`/`k`, `y`, `d`, space to select | All the ordinary list keys work — this is the list, not a panel |
| 44.10 | `gu`, then `Enter` on a folder; then `h`, and `h` again (v0.63.0; before that `Enter` left the view) | `Enter`: the view stays and measures that folder (the header path is the folder). First `h`: back up, still in the view, the cursor on the folder it left. Second `h`, in the folder `gu` was pressed in: the view closes and the ordinary listing is back |
| 44.11 | `gu` while a usage view is already up | Refused with a message, not a view with no way back |
| 44.12 | `gu`, then `,` to re-sort | The order changes (as asked); `<Esc>` and `gu` again restores largest-first (`gu` inside the view is refused, 44.11) |
| 44.13 | Compare a folder's total against Explorer's own properties | Within rounding. **Hard links read high — that is documented, not a bug** |
| 44.14 | With the tab on `linemode mtime` (`m t`), `gu`, then `<Esc>` straight away (v0.56.0) | The rows show sizes (`1.5 M`, `6.0 K`), not dates; after `<Esc>` the list shows dates again, and the `Measuring…` toast is gone at once |
| 44.15 | `gu` on a small tree and let it finish (v0.57.3) | Only the total's toast is left; `Measuring…` goes when it arrives rather than sitting beside it |
| 44.16 | `gu` on a tree big enough to take seconds, and watch the header (v0.57.3) | `N measured so far`, growing, while it walks; `N items` once the total's toast is up |
| 44.17 | `gu`, then `m t` inside the view, then `m u` (v0.58.0) | `m t` swaps the numbers for dates with the bars left; `m u` brings the sizes back **without** walking again (no `Measuring…`). `<Esc>` still gives the tab its own mode back |
| 44.18 | In an ordinary listing (no `gu`), `m u` (v0.59.2) | Folders are **blank**, as under `m s`; files show their own size. Until v0.59.2 every folder read `0 B` |
| 44.19 | `gu` on a tree that takes seconds, then wait past the total's toast (v0.59.2) | The header reads `N items · <size> total` for as long as the view is up -- the one sign left that this is the usage view. And `filer --keys "gu<Wait:0>j"` on that tree moves the cursor only after the walk is done |

## 45. Comparing two folders (v0.43.0)

The pairing and the size/byte rules are unit-tested on small trees. What needs a machine is two real
trees, and the keys in the actual view.

45.1, 45.2, 45.5 and the footer half of 45.7 are automated (`ui::overlay::diff_frame`): the five signs
reach the screen, the footer counts each kind with "too big to read" separate from "match", an
identical pair says so, and the highlight follows `j`. What is left is the real trees.

| # | Do | Expect |
| --- | --- | --- |
| 45.1 | Select two folders, `<A-d>` | A list of paths with `<` `>` `~` `=` signs and a footer counting each |
| 45.2 | `j` / `k` in that view | The **selection** moves (a highlighted row), not the scroll |
| 45.3 | `n` / `N` | Walks between the rows that are not `=`, skipping matches. At the end it says so |
| 45.4 | `gg` / `G` | First and last row |
| 45.5 | Two identical copies of a tree | Every row `=`, or the "same paths, every file matches" line if empty |
| 45.6 | A tree where one file differs in its last byte only | That row is `~`, not `=` |
| 45.7 | A pair of same-sized files **over 64 MB** | `?`, and the footer counts it as "too big to read" — **not** reported as matching |
| 45.8 | A folder on one side where the other has a file of that name | `~` |
| 45.9 | Select one file and one folder, `<A-d>` | Refused with "compare two files, or two folders — not one of each" |
| 45.10 | Two `node_modules` (100k+ paths) | Answers, or says it was cut short; the window does not freeze |
| 45.11 | Two trees differing only in where a symlink points | The link row reads as differing |
| 45.12 | Split the view, stand on a folder in each pane, `<A-d>` | Compares those two |
| 45.13 | `q` / `<Esc>` | Closes, and two **files** still compare line by line as before |
| 45.14 | Compare two trees of hundreds of paths that differ in one file far down (v0.53.0) | The view opens with the cursor **on that file**, not on the first row. A pair with no differences opens at the top |
| 45.15 | `z`, then `j` / `n`, then `z` again | The `=` rows leave the list; the footer still counts them and adds `matches hidden (z)`; `j` and `n` step only over what is shown; the second `z` brings every row back with the cursor on the same path |
| 45.16 | Copy a folder holding a **junction** to a folder inside it (`mklink /J ln t1`), then compare the original with the copy (v0.55.0) | `= ln`: both links land on `t1` in their own tree, so the copies read as the same even though the two targets differ as text |
| 45.17 | Compare two folders of the same name in different places, one holding a subfolder (v0.59.4) | Under the title, both **full paths** (`…\left\proj  ↔  …\right\proj`), each cut in its middle if long so both ends stay readable. A folder row ends in `\` like its children's paths, not `/` |
| 45.18 | Compare two folders, put the cursor on a `≠` file row, `<Enter>`; then `q` (v0.61.0) | The two files open side by side, line by line, titled with both full paths. `q` goes back to the folder comparison **on the same row**, not closed and not at the top. `<Enter>` on a row that exists on one side only says `Compare: it is on one side only` and stays |

## Known gaps in this checklist

- **Nothing here has been run.** The checklist was written from the code, not
  from use; a step that does not match the program may be the checklist's
  mistake rather than the program's. Say so if a step reads wrong.
- **macOS and Linux are unexercised.** Since v0.33.0 `build.yml` builds and
  links all six targets on their own runners, so "it compiles" is now checked
  rather than assumed — but building is not running, and no one has started the
  program there. What is known without running it: the shell thumbnail
  (HEIC / AVIF / PDF / video) and the share listing return an error saying they
  are Windows-only, and the undo of a delete does not work on macOS (no API for
  reading the Trash back). The artifacts are on the Actions tab if a machine
  turns up.
- **The frame itself is now checked, without a screen.** This note used to say
  that automating the drawing needed a GPU and so could not be set up here. That
  was true of comparing *pixels* and wrong about everything else: egui's frame is
  laid out and tessellated on the CPU, and only turning the resulting meshes into
  pixels needs a driver. `ui::harness::Screen` (v0.45.0) stops after the first
  half — it feeds `egui::Event`s through the same `handle_input` the window uses,
  runs `ui::draw`, and hands back every string and rectangle the frame painted.
  One frame costs 0.01s and no new dependency.

  It found a bug on its first run: the header joined the hovered file's name to
  the directory with a literal `\`, so off Windows it read `/home/you\notes.md`.

  What it covers so far: the chrome (path, count, mode, rows), `SELECT`, that an
  `<A-m>` chord does not also type its letter (the v0.38.0 bug), that the minimap
  appears and that a narrow pane drops it, and the folder comparison's signs,
  tally and cursor. The sections it could still reach — the ones whose checks are
  about what is on screen rather than about the OS — are 5, 6, 9, 10, 11, 12, 13,
  18, 20, 21, 24, 27, 33, 34, 36, 38, 42, 43 and 44.

- **Pixel comparison is still not set up.** `egui_kittest`'s snapshots would
  catch what the harness above cannot: colours, glyph shapes, a layout that is
  present but wrong. That does need a wgpu adapter. `mesa-vulkan-drivers`
  (lavapipe) is installable even here, so it is no longer impossible — but font
  rasterisation differs per platform, so the baselines would split three ways.
  Lower value than filling in the sections above, and it should come second.

## 46. The spot panel's Git section (v0.47.0)

The listing's marks say what git *thinks of* a file now; this says what happened to it. Needs a
real repository with a history — this one will do.

| # | Do | Expect |
| --- | --- | --- |
| 46.1 | `<Tab>` on a committed file | A **Git** section: `Last change` is a short hash and `YYYY-MM-DD HH:MM`, then `Subject` and `Author` |
| 46.2 | Check it against `git log -1 -- <that file>` | The same commit. Not the repository's newest — **the newest that touched this path** |
| 46.3 | `<Tab>` on a file changed by more than one commit | `Commits` appears with the count |
| 46.4 | `<Tab>` on a file added by exactly one commit | **No `Commits` row** — one says nothing the date has not |
| 46.5 | `<Tab>` on a file in a history of 50+ commits touching it | `Commits` reads `50+`, not a wrong total. The cap is there so a directory near the root reads a page, not the whole history |
| 46.6 | `<Tab>` on a **directory** | The last commit that touched anything inside it |
| 46.7 | `<Tab>` on a file that is new and never committed (`git status` shows `?`) | **No Git section at all** — nothing in the history touches it |
| 46.8 | `<Tab>` somewhere that is not a repository | No Git section, and no pause before the panel draws |
| 46.9 | The same on a machine with no `git` on `PATH` | No Git section, no error, and the rest of the panel is unaffected |
| 46.10 | `<Tab>` on a file whose last subject has Japanese in it, or an emoji | Drawn intact, not mojibake — the format is NUL-separated so nothing needs quoting |
| 46.11 | Watch for a console window | **None flashes.** `git` is spawned with `CREATE_NO_WINDOW`, the same as the status worker |
| 46.12 | `<Tab>` on a file whose commit arrived through a merged pull request | `Came in via` reads `#<n>` then the merge's short hash, and `From branch` names the branch |
| 46.13 | Check `#<n>` against the pull request on GitHub | **The same number**, and the file is in that pull request's diff. The number is read out of the merge commit's subject — nothing is fetched, so this is the row that proves the subject is the source |
| 46.14 | `<Tab>` on a file whose last commit was pushed **straight to `main`** | **No `Came in via` and no `From branch`** — the history rows only. A merge that merely came later must not be credited |
| 46.15 | `<Tab>` on a file committed on the current branch and **not merged yet** | The same: history rows, no `Came in via`. It has not arrived anywhere to be asked about |
| 46.16 | Pull the network cable, turn off Wi-Fi, or block `filer.exe` and `git.exe` outbound in Windows Firewall — then repeat 46.12 | **Identical output, at the same speed.** Nothing here leaves the machine. The firewall form is for a session on the machine, which the other two would cut off |
| 46.17 | In the spot panel, `C` (v0.52.0) | Every row is on the clipboard as `Label<TAB>value`, under each section's title, sections a blank line apart. The toast counts the rows |
| 46.18 | On a 46.12 file in a clone of a GitHub repository, the `Pull request` row | It reads `https://github.com/<owner>/<repo>/pull/<n>` for the `#<n>` above it. `<Enter>` on it — or on `Came in via` — opens that page in the browser |
| 46.19 | On a 46.15 file (committed, not merged) in a clone that has `origin/HEAD` | A **`Not merged`** row: `not in origin/main yet` (the clone's own default branch). A 46.14 file (straight to main) has **no** such row, so the two no longer look alike |
| 46.20 | The same in a repository with no `origin/HEAD` (`git remote set-head origin -d`) | No `Not merged` row at all — filer does not guess the default branch |
| 46.21 | On a file that came in through a pull request, `<Tab>`, the cursor on `From branch`, `<Enter>` (v0.59.1) | The browser opens the branch's page (`…/tree/<branch>`), and the toast says `Opened …`. A branch deleted after the merge opens GitHub's own 404, which is still the right address |


## 47. An idle window uses no CPU (v0.54.2)

The Windows machine measured an idle, even minimised, filer at 1.0 CPU-second per second (#86).
v0.54.2 found one way to get there -- the preview's debounce timer, left running for good when the
cursor moved off a file onto a directory, a cached file or the file already shown, which kept the
window redrawing 60 times a second -- and fixed it. These rows are what says whether that was *the*
cause. Every expectation is a number from `Get-Process`.

| # | Do | Expect |
| --- | --- | --- |
| 47.1 | Open filer on a folder of files and subfolders, touch nothing for 10 s, then read `(Get-Process filer).CPU` twice, 10 s apart | The two readings differ by **well under 1 s** (a few hundredths is normal) |
| 47.2 | `j` onto a file and at once `j` onto a subfolder (inside the 40 ms debounce), then hands off; read the CPU twice, 10 s apart | The same: **no rise**. Before v0.54.2 this was the sequence that left it drawing for ever |
| 47.3 | The same as 47.2, then minimise the window | Still no rise while minimised |
| 47.4 | If 47.1-47.3 still rise: `Get-Process filer \| % Threads \| sort TotalProcessorTime -desc \| select -first 3 Id, TotalProcessorTime`, twice, 10 s apart | Report which thread's time grows, and its start address if a tool can name it. That thread is the next thing to look at |
| 47.5 | Open the `f` prompt, touch nothing for 10 s, and read the CPU before and after (v0.59.3) | No rise, as with no prompt open (47.1). The caret is steady rather than blinking. Until v0.59.3 the blink drew twice a second: 0.14-0.30 CPU-s per 10 s (#103, #110) |

## 48. The release zips (v0.64.0)

What a person downloads from the Releases page, checked as they would get it: nothing built here,
nothing fetched by `scripts/fetch-conpty.ps1`. Take the newest release whose tag is v0.64.0 or later
(48.5 needs its checksum table), download both Windows zips, and `Expand-Archive` each into an empty
folder. Every row is a file listing, a command's output or a hash.

The PE machine of a file, for 48.3:
`$b = [IO.File]::ReadAllBytes($f); '{0:X4}' -f [BitConverter]::ToUInt16($b, [BitConverter]::ToInt32($b, 0x3C) + 4)`.

| # | Do | Expect |
| --- | --- | --- |
| 48.1 | `Get-ChildItem -Recurse` in each extracted folder | One folder, `filer-<tag>-windows-x64` (or `-arm64`), holding exactly five files: `filer.exe`, `filer.com` (v0.71.0), `conpty.dll`, `OpenConsole.exe` and `ConPTY-LICENSE.txt`. Nothing else, and nothing at the top level beside the folder |
| 48.2 | `.\filer.exe --version` from each folder | `filer <version> (x86_64)` from the x64 zip and `filer <version> (aarch64)` from the ARM64 one, the version being the tag without its `v` |
| 48.3 | The PE machine (above) of all four binaries in each zip (`filer.com` is one since v0.71.0) | `8664` for all four in the x64 zip, `AA64` for all four in the ARM64 one. A mixed zip is the bug this row exists for: the ARM64 build with an x64 ConPTY would start and then misbehave in the pane |
| 48.4 | Read `ConPTY-LICENSE.txt` | Names the version `scripts/fetch-conpty.ps1` pins (`$version`) on the release's commit, and no `{VERSION}` is left in it |
| 48.5 | `Get-FileHash -Algorithm SHA256` on each zip and on each extracted file | Every hash equals the row for that file in the **SHA-256** table at the end of the release page. A missing table means the `sums` job did not run: say so |
| 48.6 | Start `filer.exe` from each folder, open the pane (`<C-t>`), and list the process's modules: `(Get-Process filer).Modules \| ? ModuleName -eq conpty.dll \| % FileName` | The full path is **that folder's** `conpty.dll`, the same folder as `filer.exe`. That is what the zip is for. A wrong answer is another program's copy (WezTerm's, say), not one under `C:\Windows`: Windows has no `conpty.dll` of its own (v0.70.3, #151 / #184) |
| 48.7 | Copy `filer.exe` **alone** into an empty folder. Put a `conpty.dll` and `OpenConsole.exe` (the zip's) in a second folder, `cd` there, and start the lone exe by its full path; `<C-t>`, then the modules as in 48.6. Again from a folder with none, with a `conpty.dll` somewhere on the `PATH` (v0.70.3, #184) | **No** `conpty.dll` among the modules either time: the pane runs on the ConPTY built into Windows. Before v0.70.3 the first loaded the working folder's copy and the second the `PATH`'s |
