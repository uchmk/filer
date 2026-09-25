# Filer

A keyboard-driven file manager for Windows, written in Rust with [egui](https://github.com/emilk/egui).
It keeps yazi's feel — three columns, vim keys, chords with a which-key panel, instant previews —
and reads **yazi's own config files**, so an existing `yazi.toml` / `keymap.toml` / `theme.toml`
works as-is.

```
cargo run --release -- C:\some\path
```

## Why it feels fast

Nothing that touches the disk runs on the UI thread.

| Concern | How it is handled |
| --- | --- |
| Directory listing | A small pool of scan threads (`[tasks] micro_workers`); results are routed by request id, stale ones are dropped |
| Revisiting a directory | An LRU of listings makes `h`/`l` instant; a background rescan refreshes behind the scenes |
| Huge directories | Only the visible rows are laid out and painted — a 200k-entry directory costs the same as a 20-entry one |
| Previews | One worker, newest-request-wins, plus a 40 ms debounce so scrolling never hits the disk; syntax highlighting happens in the worker |
| Images | Decoded and downscaled off-thread, uploaded once as a texture |
| File operations | A sequential job queue with progress and conflict prompts |
| Repaints | Event-driven: egui only redraws when something actually changed |
| External changes | Watched via `notify`, debounced 150 ms, then rescanned |

## Configuration

Files are read in this order — later ones win:

1. `%YAZI_CONFIG_HOME%` or `%APPDATA%\yazi\config` — `yazi.toml`, `keymap.toml`, `theme.toml`
2. `%FILER_CONFIG_HOME%` or `%APPDATA%\filer` — the same three, plus `filer.toml`

Press `~` or `F1` in the app: the help panel lists which config files were actually loaded, any
warnings, and every key binding in effect.

### yazi.toml

Honored: `[mgr]` (`ratio`, `sort_by`, `sort_reverse`, `sort_dir_first`, `sort_sensitive`,
`linemode`, `show_hidden`, `scrolloff`, `title_format`), `[preview]` (`wrap`, `tab_size`,
`max_width`, `max_height`), `[opener]`, `[open].rules`, `[tasks].micro_workers`.
`[manager]` is accepted as an alias for `[mgr]`. Unknown keys are ignored rather than rejected.

Opener placeholders `$@`, `$0`, `%*`, `%0` and `%s` all expand to the selected paths.
`block = true` gets its own console window (so `nvim` works); everything else starts without one.
Rule patterns take `*`, `?`, `[abc]` and `{jpg,png}`, which is what yazi's own rules are written
with.

### Openers — what `<Enter>` and `<S-Enter>` offer

`<Enter>` runs the first opener that applies; `<S-Enter>` shows all of them and lets you pick.
Both lists come from your own `yazi.toml` — nothing is built in, so an editor that is not in
there cannot appear. Two tables do the work: `[opener]` names the lists, `[open].rules` says
which list a file gets.

```toml
[opener]
# The list `<Enter>` reaches for on a text file. Order matters: the first entry wins.
edit = [
  { run = '"C:\Program Files\Hidemaru\Hidemaru.exe" %*', desc = "秀丸エディタ" },
  { run = '"C:\Program Files (x86)\sakura\sakura.exe" %*', desc = "サクラエディタ" },
  { run = 'code %*', desc = "VS Code" },
  { run = 'nvim %*', desc = "Neovim", block = true },
]

# Anything the OS already knows how to open.
open = [{ run = 'start "" %*', desc = "Open with the default app" }]

browser = [
  { run = 'start "" msedge %*', desc = "Edge" },
  { run = 'start "" chrome %*', desc = "Chrome" },
]

office = [
  { run = 'start "" excel %*', desc = "Excel" },
  { run = 'start "" winword %*', desc = "Word" },
]

[open]
rules = [
  { name = "*.pdf", use = ["browser", "open"] },
  { name = "*.{xlsx,xls,csv}", use = ["office", "open", "edit"] },
  { name = "*.{docx,doc}", use = ["office", "open"] },
  { name = "*.{txt,md,toml,rs,py,json,yml,yaml,ini,log}", use = ["edit", "open"] },
  { name = "*", use = ["open", "edit"] },        # the fallback, last
]
```

Every rule that matches contributes, in the order written, so the catch-all at the end adds
*Open with the default app* to everything without taking the top spot from a more specific rule.
`<S-Enter>` shows `desc` with the command line beside it, so name them however you think of them.

Two Windows details worth knowing, both of which turn "it does nothing" into "it works":

- **`start "" ` in front of a GUI program that is not on `PATH`.** Commands run through
  `cmd /C`, which searches `PATH` and nothing else; `excel.exe` and `msedge.exe` are not on it.
  `start` asks the shell instead, which knows where installed programs live. The empty `""` is
  the window title `start` would otherwise steal the program name for.
- **Full paths need the quotes shown above**, and the `%*` stays outside them. The paths filer
  substitutes are quoted for you, so a name with a space stays one argument either way.

An editor listed here also gets the line number when you open from the outline, if filer knows
its syntax — 秀丸, サクラ, EmEditor, Notepad++, VS Code and the vim family are known already, and
[line_args](#line_args-opening-an-editor-at-a-line) covers the rest.

### keymap.toml

Layering matches yazi: `prepend_keymap` → (`keymap` or the built-in defaults) → `append_keymap`,
and the first exact match wins. That is what lets a prepended single-key `m` shadow the built-in
`m`-prefixed chords.

`on` accepts a single token (`"T"`), a sequence string (`"gg"`) or an array (`["g", "g"]`).
Key notation is yazi's: `<C-a>`, `<A-S-Up>`, `<Enter>`, `<Space>`, `<F5>`, `<lt>`.

Commands implemented: `escape`, `quit`, `close`, `arrow`, `leave`, `enter`, `back`, `forward`,
`cd`, `reveal`, `follow`, `refresh`, `seek`/`peek`, `tab_create`, `tab_close`, `tab_switch`,
`tab_swap`, `toggle`, `toggle_all`, `visual_mode`, `open`, `yank`, `unyank`, `paste`, `link`,
`hardlink`, `remove`, `create`, `rename`, `copy`, `shell`, `hidden`, `linemode`, `sort`, `find`,
`find_arrow`, `filter`, `search`, `help`, `tasks_show`, `spot`, `noop`, plus `undo`, `redo`, `jump`,
`bulk_rename`, `compare`, `quick`, `zoom`, `minimap`, `config_reload`, `palette`,
`menu`, `extract`, `compress`, `send_pane`, `terminal`, `term_send`, `term_cd`, `term_find`, `term_scroll`, `task_toggle`, `task_cancel`, `task_top`,
`split`, `pane_focus`, `toggle_render`, `toggle_outline` and `bug-report` (this
project's own). `select` and `select_all` are accepted as `toggle --state=on` /
`toggle_all --state=on`. In the `[input]` section: `close --submit` (and the `*_do` spellings),
`close` and `complete`; in `[spot]`: `close`, `arrow`, `swipe` and `copy cell`; in `[term]`:
`close` and anything from `[mgr]`, with every other key going to the shell; in `[diff]`:
`close`, `arrow` and `find_arrow`.

A few plugin invocations are mapped onto built-in behavior so common setups keep working:

| In `keymap.toml` | Effect |
| --- | --- |
| `plugin toggle-pane max-preview` | Maximize / restore the preview pane |
| `plugin toggle-pane min-parent` | Hide / show the parent pane |
| `plugin bookmarks save` / `jump` / `delete` / `delete_all` | Bookmarks (press the key to assign or jump) |
| `plugin smart-enter`, `plugin smart-filter` | `open` (a directory is entered), `filter --smart` |

Anything else parses cleanly, reports itself as unsupported in the help panel, and shows a toast
if you press it — it never breaks config loading. There is no Lua runtime.

What a plugin is mostly used for — a custom action on the file under the cursor — is written here
as a `shell` binding or as an `[opener]` entry, and both show up on their own in the
[context menu](#context-menu) and the [command palette](#command-palette). Nothing has to be
registered: the menu is read back out of the config every time it opens.

### theme.toml

`[mgr]` colors, `[status]` modes, `[which]`, `[git]`, `[filetype].rules` and `[icon]` (`globs`,
`dirs`, `exts`, `files`, `conds`) are applied on top of a built-in dark theme. Colors may be ANSI names
(`lightblue`, `darkgray`, `reset`) or hex (`#7ab8f5`). `syntect_theme` selects the preview's
syntax theme.

### filer.toml (GUI-only settings)

[`filer.example.toml`](filer.example.toml) in this repository is a commented copy of the defaults —
copy it to `%APPDATA%\filer\filer.toml` and edit from there.

```toml
[ui]
font_size = 14.0
row_padding = 4.0
fonts = []                 # explicit font file paths, tried first
bold_fonts = []            # explicit bold font file paths, tried first
icons = "auto"             # auto | nerd | ascii | none
render_markdown = true     # start Markdown previews rendered (`M` toggles)
preview_debounce_ms = 40
max_text_bytes = 262144
max_history = 200
window_width = 1360.0
window_height = 860.0
```

Fonts are auto-detected: a Nerd Font from your user font directory (HackGen, FiraCode,
CaskaydiaCove, JetBrainsMono) first, then Meiryo / Yu Gothic for CJK coverage. If no Nerd Font is
found, icons fall back to plain ASCII automatically.

Bold text (headings, `**strong**`) uses a real bold face: the `-Bold` sibling of the regular font
(e.g. `HackGen35ConsoleNF-Bold.ttf`) or Meiryo / Yu Gothic Bold. Without one it is faked by
drawing the glyphs twice.

### line_args (opening an editor at a line)

Opening at a line (see [Outline](#outline-contents)) knows a list of editors by heart. Any other
editor — and any of the built-in ones you disagree with — can be given its own syntax here:

```toml
[line_args]
mikan = "-l {line} {path}"          # mikan.exe -l 123 "C:\a b\x.txt"
myedit = "{path}:{line}"            # myedit.exe "C:\a b\x.txt:123"
"notepad++" = "-n{line} {path}"     # the built-in entry, spelled out
```

The key is the program's file name, lowercased, without `.exe` / `.cmd` / `.bat`; the value is the
arguments, which must name `{path}` exactly once. They take the place of the opener's own path
placeholder, so the rest of its command line (`nvim -O %s`) is kept. The path is quoted for you,
together with whatever sits next to it in the same word — `{path}:{line}` comes out as
`"C:\a b\x.txt:123"`, never as a broken pair of words.

## Markdown preview

`.md` / `.mdx` files are rendered: headings, emphasis, lists and task lists, tables, block quotes
and GitHub alerts, with fenced code blocks highlighted by language. `M` switches between the
rendered view and the highlighted source, keeping your place.

## Outline (Contents)

The preview keeps an outline of the file: the headings of a rendered Markdown file, or the
functions, types, classes, modules and TOML tables of source code (taken from the same syntax
highlighting pass, so every language with a grammar works, nested by indentation). When the pane
is wide enough — about 80 columns for Markdown, 100 for code, e.g. with `max-preview` — it sits on
the right as a Contents column that tracks the scroll position; clicking an entry jumps there.

`l` / `→` on a file hands the keys to the outline (on a file without one it does nothing; files
are opened with `<Enter>`). `<S-Tab>` does the same and also toggles back, saying so in a toast
when the file has no outline. While the outline has the keys the file list's cursor dims,
`j` / `k` / arrows / `gg` / `G` / `<C-d>` move through the entries and the preview follows.
`<A-k>` / `<A-j>` still scroll freely.

`<Enter>` opens the file at the selected entry's line, and `<S-Enter>` does the same with the
editor you pick. The line is passed as `+N` to nvim / vim / nano / emacs / micro / kak, as
`-g file:N` to VS Code / Cursor / Windsurf, as `file:N` to Helix / Sublime / Zed, and on Windows
as `/jN` to Hidemaru, `-L=N` to Sakura, `/l N` to EmEditor and `-nN` to Notepad++; other openers
(Notepad among them) just open the file. Any editor can be taught the syntax — or an entry of the
list above overridden — with [`[line_args]` in filer.toml](#line_args-opening-an-editor-at-a-line). `<Esc>`, `h` / `←` or `<S-Tab>` gives the keys back to the file list,
and any other key does so too before doing its usual job. In a narrow pane the outline shows as an
overlay only while it has the keys.

## Spot

`<Tab>` opens yazi's spot panel for the hovered file:

- **File**: name, path, kind, MIME type, size (or item count), created / modified / accessed
  times and attributes, straight from the listing.
- **Preview**: what the preview found (line count, outline entries, whether only the head was read).
- Per type, read on a worker thread: a link's target, an image's real dimensions, format and color
  type, a font's family / style / version / weight / glyph count, a directory's file and
  subdirectory counts.

`j` / `k` / `↑` / `↓` select a row, `h` / `l` / `←` / `→` spot the previous / next file, `c` / `y`
copy the selected value, and `<Esc>` / `q` / `<Tab>` close it. Each kind of detail is one provider
function in `src/spot.rs`, so more (e.g. Windows property-system values like media length or EXIF)
can be added without touching the panel.

## Split view (two panes)

`<C-w>` splits the window in two and, from then on, moves the keys between the panes. The second
pane takes the parent column's place, so the layout stays three columns wide: pane, pane, preview.
`<C-S-w>` closes it (`split close`; `split` alone toggles, `split open` / `split close` are explicit).

The second pane is just another tab, shown side by side. That is what keeps everything else
working: the focused pane is always the current tab, so every command — `cd`, `yank`, `paste`,
filter, search — runs on it with no notion of panes at all, and copying between panes is the
ordinary `y` … `<C-w>` … `p`. Splitting with one tab open creates a second one on the same
directory; with several, it borrows the next tab. `[` / `]` / `1`–`9` still switch tabs, and
switching to the tab the other pane shows just moves the keys there. Closing or swapping tabs
keeps the panes pointed at the right ones; closing the tab the other pane holds ends the split.

`<A-c>` copies the selection into the other pane and `<A-m>` moves it — one key instead of
`y` `<C-w>` `p`. The other pane's directory is already on screen, so naming a destination is the
step worth removing; the yank register is left alone. Dragging does the same: drag a row (or a
selection) onto the other pane to copy it, with `Shift` held to move it, the way Explorer does.
The pane about to receive the drop is outlined and the pointer says which it will be.

Neither is on `F5` / `F6` on purpose. `F5` is refresh here as it is in every browser, so putting a
file operation there would write files for someone reaching for a reload; and `F6`/`F7` would
half-match Total Commander, where `F6` means *move* — worse, because the mismatch loses data
rather than just surprising. Rebind them in `keymap.toml` if your fingers disagree.

The pane with the keys is outlined and keeps the bright cursor; the other is dimmed. Clicking the
dim pane takes the keys first, so a click, a `Shift`+click or a double-click always lands on the
pane you aimed at. Both panes are watched for changes and rescanned, the passive one at a lower
priority so the focused directory is never made to wait behind it.

## Command palette

`<C-S-p>` (`Cmd`+`Shift`+`P` on macOS) lists every `mgr` binding — the built-in ones and whatever
your `keymap.toml` added — and runs the one you pick. Each row carries the description and the
command text, so `tasks_show` and `task manager` both find the task panel; the key that runs it is
shown on the right. A command bound to several keys appears once, under the first key the keymap
gives it, and commands the config left unsupported are left out.

`↑` / `↓` (or `<C-p>` / `<C-n>`) move, `<Enter>` runs, `<Esc>` closes, and a click runs the row
directly. Commands that need more input (`rename`, `filter`, `shell`, …) open their own prompt
as if the key had been pressed.

The openers `yazi.toml` lists for the file under the cursor ride along at the end of the list,
named `Open with <desc>`, so a custom action written as an opener is reachable without knowing
which key opens it.

## Context menu

Right-click a row — or press `<S-F10>`, or run `menu` — for what this config says can be done with
that file. There is no fixed list: the rows are read back out of your own configuration.

| Group | Where it comes from |
| --- | --- |
| Openers | `yazi.toml`'s `[opener]` entries that `[open].rules` selects for this file |
| Custom actions | every `keymap.toml` binding that runs `shell` |
| File commands | the bindings that act on a file — `open`, `yank`, `paste`, `rename`, `remove`, `link`, `copy`, `spot`, … |

Commands that only move the cursor or change what the view shows are left out, and anything the
config left unsupported never appears. Each row shows the key that also runs it, so the menu
doubles as a reminder of the keymap.

Right-clicking inside the selection keeps it, and the command acts on all of it — the title says
how many. Right-clicking outside the selection drops it and acts on that one row, the way
Explorer does. This is the nearest thing here to a plugin menu: a `shell` binding or an opener is
how a Lua plugin's action gets onto the screen, with no Lua runtime involved.

## Terminal

`<C-t>` opens a shell in a pane along the bottom, started in the directory you are looking at.
It is a real terminal: a PTY on Unix, ConPTY on Windows, with
[alacritty_terminal](https://crates.io/crates/alacritty_terminal) parsing the escape sequences,
so `vim`, `less` and anything else that paints the screen work as they do anywhere else.

While the pane has the keys, **every key goes to the shell** — that is what makes it a terminal
rather than another set of bindings. Only what the `[term]` keymap section binds is kept:

| Key | |
| --- | --- |
| `<C-t>` | give the keys back to the list, leaving the shell running |
| `<C-S-t>` | close the pane and end the shell |
| `<F1>` `<C-S-p>` | the key list / the command palette |
| `<A-Up>` | put the file list where the shell is |
| `<S-PageUp>` `<S-PageDown>` | half a screen back / forward through the scrollback |
| `<S-Home>` `<S-End>` | to the top of the scrollback / back to the bottom |
| `<C-S-f>` `<C-S-n>` `<C-S-b>` | find in the scrollback / next match / previous |

`<C-t>` is the way in and the way back out, and it leaves the shell alone: going to and fro is
something you do all day, while ending a shell is something you do a few times, so the destructive
one is the harder chord. The shell's own `<C-t>` — readline's transpose, or fzf's file widget — is
the cost of that, and moving it is one line of `keymap.toml` away.

`Shift` is what keeps those out of the shell's way: a program reading the keyboard sees `PageUp`,
never `Shift`+`PageUp`. Typing anything brings the view back to the bottom, and while it is not
there the pane says how far back it is.

The wheel walks the scrollback. Drag to select and the selection is copied when you let go —
that is what selecting means in a terminal, there is no second step — and a double-click takes
the word. **Right-click pastes**, which is the other half of that pair; `<C-v>` does the same.
Click the pane to take the keys back.

A paste is wrapped in the bracketed-paste markers when the program on the other end asks for
them — bash, zsh, fish, PSReadLine and vim all do. That is what keeps a clipboard holding three
lines from running as two commands and a half-typed third: inside the markers a line editor puts
the text in the buffer and waits. A shell that does not ask gets the text plain, where a newline
is Enter and always has been.

The grid is drawn with the list's own font and the theme's colors, so the 16 ANSI colors match
the rest of the window; the 256-color cube and true-color values are used as the program asked
for them.

It follows the pane: change directory and the shell is sent a `cd` for the new one. Typing is the
only way in — a shell takes no other instruction — so that is a line of input like any other,
harmless at a prompt and a nuisance in the middle of a command. It is therefore sent as rarely as
it can be: not when the pane has not moved, and not when the shell has already said it is there.

The saying is OSC 7, the escape a shell emits to report its directory; most send it out of the
box and some have to be told to. filer reads it off the PTY as the bytes go past. A shell that
sends it never hears a `cd` it does not need — including the one that would otherwise chase its
own. `<A-Up>` in the pane goes the other way: it puts the file list where the shell is, which is
what you want after a command has moved it somewhere the list knows nothing about.

`<A-t>` types the selected paths onto the shell's line, quoted so a path with a space in it
arrives as one word. Nothing is run: the line is left for you to put a command in front of.

## Tasks

Copying, moving, deleting, packing and unpacking all run as jobs on one worker, one at a time —
two copies on the same disk are slower than one. The status bar carries the one being worked on:

```
Copy  42%  18.4 M/s  1m12s  +2
```

the percentage, the speed, how long the rest should take, and how many other jobs are waiting.
The speed is measured over a window rather than between reports, and smoothed, so it stays
readable instead of flickering; the time left is only shown when the size is known, since a file
count says nothing about how big the files are.

`w` opens the panel, which is a list you can act on:

| Key | |
| --- | --- |
| `j` `k` (or `↑` `↓`) | move between jobs |
| `p` | pause the job, or set it going again |
| `x` | cancel it, queued or running |
| `t` | move a queued job to the front |
| `q` `<Esc>` | close |

Pausing lands between files, or between chunks of a large one, never mid-write. A paused job
holds the worker, so nothing behind it starts until it is resumed or cancelled — `t` is how you
change your mind about what should have gone first. The commands are `task_toggle`,
`task_cancel` and `task_top`, bound in the `[tasks]` keymap section.

## Bulk rename

Select some files and press `R`. The prompt takes one rule for all of them, and the panel above it
shows what every name is about to become while you type:

```
IMG_0431.jpg  →  holiday-01.jpg
IMG_0432.jpg  →  holiday-02.jpg
IMG_0433.jpg  →  holiday-03.jpg
```

The rule is a **template** — the new name, with pieces filled in:

| | |
| --- | --- |
| `{name}` | the name without its extension |
| `{ext}` | the extension, dot and all, or nothing where there is none |
| `{n}`, `{n:3}` | the row's number from 1, optionally zero-padded |

The prompt opens on `{name}{ext}`, which changes nothing, so you edit from where you are.
`holiday-{n:2}{ext}` gives the list above.

A rule starting with `s/` is a **regular expression** over the whole name instead, spelled as in
sed and vim: `s/pattern/replacement/`, with `g` for every match and `i` for case-insensitive, `$1`
for a group and `\/` for a literal slash. `s/ copy//g` drops every " copy". The engine is
`fancy-regex`, already in the build for syntax highlighting.

Nothing is renamed until Enter, and Enter is refused outright if any row is a problem — an empty
name, a separator in it, two files given the same name, or a name another file in the directory is
keeping. Those rows are marked in the preview, and the whole batch is one undo step, so `u` puts
every name back at once.

Files swapping names works: renaming `a` to `b` and `b` to `a` moves one aside first and puts it
back, rather than failing on the second rename the way a loop of `mv` would.

## Compare (side by side)

`<A-d>` puts two files next to each other and marks what differs — removals on the left, additions
on the right, in the same two colors the git signs use. The two files are the ones each pane is
standing on when the [view is split](#split-view-two-panes), or the two that are selected when it
is not.

| Key | |
| --- | --- |
| `j` `k` `<C-d>` `<C-u>` `gg` `G` | scroll |
| `n` `N` | to the next / previous difference |
| `q` `<Esc>` | close |

Each side carries its own line numbers, so a line found here can be found in the file. An edited
line sits opposite the line it replaced rather than being listed as a removal and an addition far
apart.

Reading the files and lining them up happens on a worker, so a big file or a slow share never holds
the window. Identical files say so rather than drawing thousands of matching rows, and two files
that are not both text report only whether the bytes match — lining up bytes is nobody's idea of a
diff. The matching top and bottom are peeled off before the work starts, which is what makes a
one-line change in a four-thousand-line file cost nothing; files with nothing in common at all are
laid side by side without being matched up, and say so.

## Quick look

`<F3>` shows the hovered file big, over the panes — the same preview, with room to read it. The
keys are not taken while it is up, so `j` and `k` keep walking the list and the panel follows them
down it; `<A-j>` / `<A-k>` scroll it. `<F3>` again or `<Esc>` closes it.

Worth knowing if macOS's Quick Look is what you have in your fingers: there the up and down keys
scroll the document and left and right step between files, and here it is the other way round.
The panel is built for flipping through a directory at full size rather than for settling into one
file, so the keys that move fastest are the ones that change file.

## Undo

`u` takes back the last thing that can be taken back, `U` does it again. Two things qualify:

| Step | `u` | `U` |
| --- | --- | --- |
| `d` — files sent to the recycle bin | puts them back where they were | sends them again |
| `r` — a rename | renames it back | renames it again |
| `R` — a bulk rename | puts every name back, in one step | renames them again |
| `x` then `p` — a move | puts the files back where they were | moves them again |

A move is here and a copy is not, which is the line the rest of the list follows: putting a moved
file back is a rename across directories and deletes nothing, while undoing a copy would mean
deleting the new files to tidy up — a worse thing to get wrong than the operation it was undoing.
`D` asks before it deletes and then means it, so it stays out too.

Undoing a move starts from where each file actually landed, not from where it was sent: a paste onto
a name already taken lands as `name_1`, and an undo built from the name you asked for would go
looking for a file that was never created.

Putting files back reads the recycle bin, which is a job like any other: it shows in the task panel
and can be cancelled. Each path is matched to the newest thing trashed under that name, so deleting
two files called `notes.txt` an hour apart and pressing `u` brings back the one that just went.

A step that will not go back stays on the stack rather than being thrown away — if something has
taken the name in the meantime, `u` says so, and pressing it again after moving that file out of the
way works. Doing something new after an undo drops what `U` would have redone, the way an editor
does. The stacks hold the last 50 steps and are not written to disk: undo is for the slip you just
made, not a log of the session.

On macOS `u` can still undo a rename, but not a delete: there is no API for reading the Trash back,
only the Finder's own ⌘Z. `u` says so instead of pretending, and the files are in the Trash either
way.

## Git status

In a repository, each row carries a sign for what git says about it:

| Sign | Meaning | Default color |
| :---: | --- | --- |
| `M` | changed in the working tree | yellow |
| `+` | staged, matching the index | green |
| `?` | untracked | gray |
| `D` | deleted, or a staged delete | red |
| `!` | an unfinished merge | red |

A directory carries the strongest state of anything inside it, so a conflict shows from the top of
the tree down. A file both staged and changed since reads as changed — that is the part still to be
committed. The colors come from `theme.toml`'s `[git]` section (`modified`, `added`, `untracked`,
`deleted`, `updated`), the same keys yazi's git plugin uses.

`git` on `PATH` is what answers: one `git status --porcelain` per listing, run on a worker, so a
big repository never holds up the window and whatever version of git is installed is the one that
decides. Nothing is linked in, so this costs no C dependency and no build step. Where there is no
repository — or no git — the rows simply carry no signs. The status refreshes with the listing, so
a file operation or a change the watcher catches updates the signs with it.

## Archives

`e` unpacks the selected archives, `E` packs the selection into one. Both run on the same worker
as copy and move, so a large archive never blocks the window and its progress shows in the task
panel (`w`) with the name of each entry as it goes past.

| Format | Read | Write |
| --- | :---: | :---: |
| `.zip` | ✓ | ✓ |
| `.tar` | ✓ | ✓ |
| `.tar.gz`, `.tgz` | ✓ | ✓ |
| `.7z` | ✓ | |

Everything is done in-process by pure-Rust crates (zip, tar, flate2, sevenz-rust): no 7-Zip
installation, no C toolchain, and the same behavior on x64 and ARM64.

`e` gives each archive a folder of its own, named after it with the extension dropped
(`report.tar.gz` unpacks into `report`), and steps the name past anything already there rather
than merging into it. A selection holding things that are not archives extracts the ones that are
and says how many it skipped.

`E` asks what to call the archive, prefilled with `<name>.zip`. **The extension you type decides
the format** — change it to `.tar.gz` and that is what you get. Names inside the archive are
relative to the directory you are in, so a folder keeps its shape. An archive that already exists
raises the same overwrite / rename prompt a paste does.

Entry names coming out of an archive are treated as untrusted: one that climbs out of the
destination with `..`, names an absolute path or carries a drive letter is refused and reported
rather than written.

Selecting an archive shows what is inside it in the preview pane — size and name, one entry a
line, scrolling like any other preview. Only the table of contents is read where the format has
one (a zip's central directory, a 7z's header), so nothing is decompressed to answer the
question; a tar has no index, so its entries are walked with the data skipped. The first 2000
entries are listed and the pane says when there are more.

## Scrolling the preview, and the minimap

The preview scrolls without the file list losing the cursor: `<A-k>` / `<A-j>`, or the wheel with the
pointer over it. Only the lines on screen are ever drawn, and only the first
256 KiB of a file is read at all (`max_text_bytes`), so a huge log opens as fast as a short one.

There is no scrollbar — this is a pane of lines, not a `ScrollArea`, because measuring ten thousand
lines to know how tall the content is would cost the frame that virtualizing just saved. The
**minimap** down the right takes its place:

```
    fn draw(…) {            ▏▔▔▔▔▔
        let rect = …;       ▏  ▄▄▄▄
        …                   ▏ ▟▓▓▓▓   ← where the pane is looking
```

Each band is one rectangle spanning the widest line in it, colored by what syntect said, so a block
of code reads as a block, a comment header as a lighter band, and a blank run as a gap. No glyphs
are drawn: at two pixels a line a letter is a smudge, and laying out ten thousand of them is exactly
the cost being avoided. The rows are summarised on the preview worker, six bytes a line.

Click or drag the minimap to go there — the line pointed at lands in the middle of the pane. It
appears where the pane is wide enough to spare seven columns, and `<A-n>` (or `[ui] minimap = false`
in `filer.toml`) turns it off. Rendered Markdown gets none: its lines are not the file's lines, so
the box would point at the wrong place, and its [Contents](#outline-contents) column already answers
"where am I". Switch it to source with `M` and the map comes back.

## Image previews

| Key | |
| --- | --- |
| `<A-i>` `<A-o>` | zoom in / out |
| `<A-0>` | fit the pane (where every image starts) |
| `<A-1>` | 1:1, one image pixel per point |

`Ctrl` and the wheel zoom about the pointer, so the thing being looked at stays under the cursor
instead of sliding away as it grows. Dragging pans, and a double-click goes back to fitting. The
caption carries the scale (`fit 34%`, `1:1`, `250%`), because whether what is on screen is the real
pixels is the first thing worth knowing. A zoom belongs to the file it was set on: walking to the
next image starts it fitted again.

The worker decodes into the pane's size, so magnifying would show a blurred copy of a small
texture. Instead the zoom asks for a **bigger decode** — the box size is part of the preview's
identity, so the newest-wins worker and the cache handle it with nothing added. The box steps in
powers of two and stops at 4096, so dragging the zoom about costs a handful of decodes rather than
one a frame, and the picture on screen stays put while the sharper copy is read: the geometry
follows the image's own dimensions, never the texture's.

## Other previews

No external tools (magick, ffmpeg, pdftoppm) are needed:

| Files | Preview |
| --- | --- |
| png, jpg, gif, bmp, ico, webp, tiff, qoi, pnm | decoded in-process, turned upright per EXIF orientation |
| svg | rendered with resvg, scaled to fill the pane |
| ttf, otf, ttc | a specimen sheet: name, alphabet (kana/kanji when the font has them), size waterfall; symbol fonts show a glyph grid |
| heic, avif, jxl, psd, video, audio, pdf | the Windows shell thumbnail — the same one Explorer shows |

Transparent images are laid over a checkerboard so dark icons stay visible on a dark theme.
Shell thumbnails need a handler for the format: HEIC / AVIF need the HEIF / AV1 Video extensions
from the Microsoft Store, PDF needs one from e.g. Acrobat Reader or PowerToys, and audio only shows
embedded cover art. Without one, a metadata card says what is missing.

## Default keys

`~` / `F1` shows the full list. The essentials:

| | |
| --- | --- |
| `h` `j` `k` `l` | parent / down / up / enter the directory or the file's outline (arrows work too) |
| `gg` `G` `<C-u>` `<C-d>` `<C-b>` `<C-f>` | top / bottom / half page / full page |
| `H` `L` | back / forward in history |
| `<Space>` `v` `V` `<C-a>` `<C-S-r>` | toggle / visual / visual-unset / select all / invert |
| `y` `x` `Y` `p` `P` `-` `_` `<C-->` | yank / cut / cancel the yank / paste / paste-force / symlink / relative symlink / hardlink |
| `d` `D` | recycle bin / permanent delete (with confirmation) |
| `u` `U` (or `<C-r>`) | undo the last rename or delete / do it again |
| `a` `r` | create (trailing `/` makes a directory) / rename |
| `R` | bulk rename: one rule over everything selected, previewed as you type |
| `<A-d>` | compare two files side by side |
| `<F3>` | quick look: the hovered file, big, over the panes |
| `e` `E` | extract the selected archives / compress the selection |
| `<A-c>` `<A-m>` | copy / move the selection to the other pane |
| `g…` | `gh` home, `gd` Downloads, `gD` Documents, `gc` config, `gt` temp, `g<Space>` type a path, `gf` follow the link |
| `c…` | `cc` copy the path, `cd` the parent, `cf` the file name, `cn` the name without its extension |
| `o` `O` `<Enter>` `<S-Enter>` | open / open with… / open (at the outline's line) / open with… |
| `/` `?` `n` `N` `f` | find next / previous / repeat / repeat back / filter |
| `s` `S` `<C-s>` | search by name / by content / stop |
| `z` | fuzzy-jump to a bookmark or recent directory |
| `b` (or `'`) `B` `<A-b>` | go to a bookmark / set one / delete one (then press its letter) |
| `.` `,…` `m…` | hidden files / sort menu / line-mode menu |
| `t` `1`–`9` `[` `]` `{` `}` `<C-c>` | new tab / switch / previous / next / move it left / right / close it (quits on the last) |
| `<F5>` `<C-F5>` | re-read the current directory / re-read the config files |
| `<C-w>` `<C-S-w>` | split the view in two panes / move between them, close the split |
| `;` `:` | shell command / blocking shell command |
| `<C-t>` `<C-S-t>` `<A-t>` | terminal: keys in and back out / end the shell / type the selection into it |
| `<A-k>` `<A-j>` | scroll the preview, without moving the list's cursor |
| `M` | Markdown rendered ↔ source |
| `<A-i>` `<A-o>` `<A-0>` `<A-1>` | image: zoom in / out / fit the pane / 1:1 |
| `<A-n>` | show or hide the preview's minimap |
| `<S-Tab>` | move the keys into the preview's outline and back |
| `<Tab>` | spot: details of the hovered file |
| `<C-S-p>` | command palette: fuzzy-search every key binding and run it |
| `<S-F10>` | context menu for the file under the cursor |
| `<F12>` | bug report, with the version, architecture and OS build filled in |
| `w` `q` | tasks (`p` pause, `x` cancel, `t` to the front) / quit |

### Coming from yazi, lf or vim

The defaults are yazi's wherever yazi has one, so a yazi user needs to learn almost nothing: the
movement, selection, yank and paste, search, tabs, sort and line-mode keys are all the same. The
movement is vim's too, and `<C-w>` moves between panes as it moves between windows.

Two places knowingly differ, both for the same reason — a reflex too widespread to give up:

- **`<C-r>` is redo**, not "invert the selection". That is yazi's key for inverting; it moves one
  modifier over, to `<C-S-r>`. `U` also redoes, which is what was here first.
- **`u` undoes.** In lf that key clears the selection.

The rest of the friction is lf's own vocabulary, which is a different family from yazi's. If your
fingers came from there, these six lines put them back — `prepend_keymap` is read before the
defaults, so nothing has to be deleted:

```toml
# ~/.config/filer/keymap.toml (or %APPDATA%\filer\keymap.toml)
[[mgr.prepend_keymap]]
on = "d"                 # lf: cut, not delete
run = "yank --cut"
[[mgr.prepend_keymap]]
on = "e"                 # lf: open in the editor, not extract
run = "open"
[[mgr.prepend_keymap]]
on = "u"                 # lf: clear the selection, not undo
run = "escape --select"
```

The differences worth knowing before you do that: here `d` sends to the recycle bin and `x` cuts
(lf has `d` cut and no delete), `f` filters the listing (lf and vim jump to a character), `;` runs
a shell command (lf repeats the character jump), and `e` / `E` unpack and pack. Every one of them is
one `prepend_keymap` entry away from whatever you would rather it was.

Mouse works too: click to move the cursor, double-click to open, right-click for the context
menu, drag onto the other pane to copy there, wheel to scroll. `Shift`+click selects from the cursor to the row you clicked, and
`Ctrl`+click (`Cmd` on macOS) adds or removes one row. Both share the selection with `<Space>`
and visual mode, so you can start a range with the mouse and finish it with the keyboard.

In a prompt — `cd`, `s`, `f`, rename, `;` / `:`, the command palette — **right-click pastes**, the
way a terminal does (and the way the terminal pane itself does), and `<C-v>` does the same from
the keyboard. The text lands where you clicked,
replacing whatever was selected; line breaks become spaces, since the prompt is one line. A path
copied out of Explorer's address bar therefore takes one click to get into `cd`, with no hand
leaving the mouse.

## Running a command on the selection

`;` runs a shell command and returns at once; `:` waits for it and gives it a console to write to.
Both hand the command what is selected, which is the point of them, so the prompt says so while you
type:

```
$@ all · $0 first · $1 second · no placeholder → appended    (3 files, returns at once)
```

| In the command | What it becomes |
| --- | --- |
| `$@`, `%*`, `%s` | every selected path |
| `$0`, `%1` | the first |
| `$1`, `%2` | the second |
| *(nothing)* | the paths are appended to the end |

Paths are quoted for you, so a name with a space in it stays one argument.

```
;  git add                          adds everything selected
;  magick mogrify -resize 50% $@    shrinks the selected images
:  pdftk $@ cat output merged.pdf   and waits, so you can read what it said
```

`cd` in there changes nothing here, and cannot: the command runs in a child process that ends with
it. For a shell whose directory sticks, open the terminal pane with `<C-t>` — that one lives as long
as you leave it open, and `<A-t>` sends it the hovered file's name.

### Bringing the terminal's directory back

`<A-Up>` in the terminal pane moves the list to wherever the shell now is. It does not guess: the
shell has to announce itself with **OSC 7**, and filer only believes what it is told. Without it
`<A-Up>` says so and does nothing.

PowerShell sends nothing by default. Most recipes for it replace `prompt`, which breaks Starship and
every other prompt generator; this hook runs on each `cd` instead and leaves the prompt alone. Put
it in `$PROFILE`:

```powershell
$ExecutionContext.SessionState.InvokeCommand.LocationChangedAction = {
    $p = $PWD.ProviderPath -replace '\\', '/'
    [Console]::Write("$([char]27)]7;file:///$p$([char]27)\")
}
```

Then `<C-t>`, `cd` somewhere, `<A-Up>`. Paths with spaces or non-ASCII characters work as they are —
percent-escapes are undone on the way in, so escaping them first is optional rather than required.

## Shell integration

`--cwd-file FILE` writes the final directory on exit, `--chooser-file FILE` writes the selection —
the same contract yazi uses, so a `cd`-on-exit wrapper works:

```powershell
function f {
    $tmp = New-TemporaryFile
    filer $args --cwd-file $tmp.FullName
    $dest = Get-Content $tmp
    if ($dest) { Set-Location $dest }
    Remove-Item $tmp
}
```

## Platform Support (Roadmap)

Development currently centers on Windows, but the goal is cross-platform support across the
major operating systems and architectures.

| OS | Architectures | Notes |
| :--- | :--- | :--- |
| **Windows** | x64 / ARM64 / x86 | UNC paths, integration with common editors |
| **macOS** | Apple Silicon (ARM64) / Intel (x64) | Cmd key support, Finder integration |
| **Linux** | x64 / ARM64 | X11 / Wayland |

CI builds Windows x64 and ARM64, and a release carries both. Both are cross-compiled on an x64
runner, so the ARM64 binary is built but never executed before it ships — the tests run on x64
only, because an x64 runner cannot execute an ARM64 binary. The two come from one source and one
set of `#[cfg]`s, so a passing test says a good deal about both, but anything that differs by
architecture has to be found on a real ARM64 machine.

Windows on ARM will happily run the x64 build under emulation, which makes it easy to test the
emulator by accident. `filer --version` prints the architecture it was built for, so it can say
which one is actually running.

## Network paths (UNC)

On Windows a UNC path is an ordinary path here — type `\\192.168.1.5\pub` (or a mapped drive
letter) into the `cd` prompt and browse it like any folder. Forward slashes work too
(`//192.168.1.5/pub`) and are shown back in the `\\host\share` spelling.

- `\\host` on its own lists the shares that host is offering, the way Explorer's network view
  does. They are not files and nothing on a disk here holds them — the network provider is asked
  what is being shared, over the same connection Explorer uses, so a host you can reach there you
  can reach here, with the credentials you already have. Shares show as folders with no size and
  no dates, because there are none to read.
- `h` from a share root therefore goes up to its host. (`\\host\share` has no parent as far as
  the path arithmetic is concerned — the host and the share are one prefix — so this is a
  deliberate step rather than a fallout.)
- A host that refuses says so: the tab returns to where it was and the reason appears as a toast.
  A login the machine has not been given is the usual one; open the host in Explorer once and it
  will work here too.
- A slow or disconnected share never blocks the window. Nothing on disk is checked before a jump —
  `is_dir` on a dead share can sit for half a minute — so the tab moves at once, shows *Loading*,
  and the scan pool has the last word. If the listing never arrives the tab returns to where it
  was and the error appears as a toast; the same undo covers a path that was deleted, refused or
  simply mistyped.
- Typing a file's path into the `cd` prompt still lands on its folder with that file under the
  cursor — that answer now comes from the scan rather than from a blocking check.
- The path on the command line (`filer \\host\share`) is opened the same way: the window goes up
  at once and the first listing decides. A path that names a file reveals it in its folder, and
  one that answers nothing falls back to the working directory with the error as a toast.
- `follow` (`gf`) on a link into a slow share is instant too — the target is read on the scan
  worker along with the rest of the entry, so the key never waits on `canonicalize`.
- A new tab (`t`, or `tab_create <path>`) opens the same way: it appears at once on the path it
  was given, and a listing that never arrives puts it back on the directory it was opened from.
  A path that names a file reveals that file in its folder.
- `Tab` in the `cd` prompt completes without waiting on disk. A directory that has been
  listed once answers from the cache — the folder you are in, its parent, anywhere the tab has
  been — and anything else is listed by the scan pool while the prompt stays live, with a `…`
  at the end of the line until the answer arrives. Typing on carries the prompt forward: an
  answer to a path you have moved past is dropped rather than pasted over what you typed.
- Change watching runs on its own thread. Registering a directory opens a handle to it
  (`ReadDirectoryChangesW` on Windows) and that call can hang on a dead share, so the window only
  posts the set of folders it wants watched. While a registration is stuck the panes still scroll
  and move; requests that pile up behind it collapse to the newest one.

## Known limits

- Windows-first. The code compiles for Unix but only Windows is tested; `block = true` openers and
  the hidden-file attribute are Windows-specific paths.
- No Lua plugin runtime — see the plugin table above for what is emulated natively, and the
  [context menu](#context-menu) for how a custom action reaches the screen without one.
- An archive's preview lists what is inside but does not browse it: no entering a folder, and
  no reading one file out. woff / woff2 fonts aren't previewed. Video,
  PDF and HEIC previews rely on Windows thumbnail handlers (see [Other previews](#other-previews)).
- `[input]`, `[confirm]` and `[pick]` keymap layers are parsed for compatibility, but the prompts
  are native widgets (for IME and clipboard support), so only Enter / Esc / Tab are configurable.
- Git signs need `git` on `PATH`; without it the rows are simply unmarked. Only the status and the
  branch name are shown — there is no staging, diffing or committing here.
- Undo covers renames (single and bulk) and trips to the recycle bin, nothing else, and on macOS
  only renames — see [Undo](#undo). It is not written to disk, so closing the window forgets it.
- Comparing is line-level and read-only: no word-level highlighting inside a changed line, no
  editing from the view, and no comparing directories.
- The minimap is a map, not a second view: no hover preview of the line under the pointer, and it
  stops where the file was cut off at `max_text_bytes` rather than describing the rest.
- Zooming an image asks for a sharper decode, but a small image has nothing sharper to give and a
  font specimen or a shell thumbnail is its own source, so those go soft past 1:1.
- `<C-F5>` re-reads the config, including fonts and the theme, but leaves what you have changed by
  hand since — the sort a `,` key chose, whether Markdown is rendered — as you set it. The window
  size is only read at startup.
- The terminal pane has no tabs and no split of its own, and `cd` following types a line into the
  shell, so it lands in whatever is running if something is — unless the shell reports its
  directory, in which case it is usually not sent at all.

## Layout

```
src/
  main.rs        window, fonts, icon, CLI, input routing
  app.rs         state and the Act dispatcher — every key and click goes through it
  config/        yazi.toml, keymap.toml, theme.toml, key notation, command parsing
  core/          folder + cursor state, tabs, fuzzy matching
  fs/            entries, sorting, scan pool, file operations, watcher, archives, git status, undelete
  rename.rs      bulk-rename rules and the order a batch of renames has to happen in
  diff.rs        comparing two files line by line, and the worker that reads them
  preview/       preview worker: text + syntect, Markdown layout, images, SVG, fonts, shell thumbnails, minimap rows
  terminal.rs    the embedded shell: PTY, key encoding
  ui/            painting: columns, preview pane, terminal pane, overlays
  search.rs      recursive name/content search
  exec.rs        openers and shell
```

## Testing

`cargo test` on Windows covers every pure function — key parsing, the diff
algorithm, the rename rules, the undo stacks, the image-zoom arithmetic, the
minimap's row summaries, pane geometry — and `ci.yml` runs it on every push.
Four tests fail on Linux and are meant to: they assert Windows path spellings.

What that cannot reach is whether the window looks right. Much of this was
written in a container with no display, so [TESTING.md](TESTING.md) is the
checklist of what has never been on a screen, and
`scripts/make-fixtures.ps1` builds the files it points at:

```powershell
.\scripts\make-fixtures.ps1
```

The icon is `assets/icon.svg`; `assets/README.md` says how the `.ico` beside it
is rebuilt, and that the artwork is not covered by the code's license.

Every key binding has a tickable line in [TESTING-KEYS.md](TESTING-KEYS.md),
generated from the default keymap by `cargo run --example make-keycheck` and
keeping its ticks when regenerated.

## Building

```
cargo build --release      # target\release\filer.exe
cargo test                 # the parsing, sorting and fuzzy-matching tests
```

Rust 1.95 or newer (`rust-version` in `Cargo.toml`) — the floor comes from egui 0.36, not from
this code. Everything the previews need is compiled in, so there is nothing else to install: no
magick, ffmpeg or pdftoppm. CI builds and tests on `windows-latest`, which is the platform the
code is written against; the handful of tests that assert Windows path and editor behavior only
pass there.

## Reporting a bug

Press `<F12>` in the app. It opens the report form in your browser with the version, the
architecture and the Windows build already filled in, which is the part of a report most
likely to be looked up wrongly or not at all — and on Windows on ARM the program is better
placed to answer than you are, since an x64 build running under emulation will tell the
shell it is on x64 while knowing perfectly well what it is.

Or open one by hand: <https://github.com/uchmk/filer/issues>. The bug report form asks for the version
(`filer --version`), your Windows build, and the smallest sequence of keys that shows the problem —
this is a keyboard-driven program, so the keys usually *are* the reproduction. It also asks what
kind of file or folder was involved, since a 3000-line source file and a 40-character filename break
different code paths, and for a backtrace when the program crashes, which is worth more than
everything else on the form put together:

```powershell
$env:RUST_BACKTRACE = 1
& "C:\path\to\filer.exe"
```

The `&` is not optional. A quoted path on its own line is a string, and PowerShell prints it rather
than running it.

The form is one template among the issue types; a plain task or question can still be opened
without it.

Writing `@claude` anywhere in the issue — the title or the body — hands it to Claude, which will
look at the report, work in the repository, and answer on the issue. `.github/workflows/claude.yml`
is what does that, and mentioning it again in a later comment brings it back. Leave the mention out
and nothing automated happens, which is the right choice for a report you want a person to read
first.

Two limits worth knowing. Claude only answers someone with write access to the repository, so a
mention from a reader who has none does nothing. And it runs on a Linux runner, where the
Windows-only code compiles but does not run: it can type-check against
`x86_64-pc-windows-msvc` and reason about the code, but the tests that assert Windows path and
editor behavior are `ci.yml`'s job on the push that follows.

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option. This is the usual arrangement in the Rust ecosystem, and it is what every
dependency here already offers: pick whichever of the two suits you, you do not need both.

`Cargo.toml` carries the same thing as `license = "MIT OR Apache-2.0"`, so tooling agrees with
these files.

### Contributing

Unless you state otherwise, any contribution you intentionally submit for inclusion in this work,
as defined in the Apache-2.0 license, is dual-licensed as above, with no additional terms or
conditions.

### Third-party code

filer links a number of crates, all under permissive licenses (MIT, Apache-2.0, BSD, Zlib, ISC,
Unlicense, CC0 and one MPL-2.0 file-level component in `option-ext`, reached through `dirs`). None
of them constrains the choice above. A binary you distribute still carries their notice
requirements: `cargo about` or `cargo bundle-licenses` will generate the attribution file.

Two of them also ship data rather than only code:

- **syntect** and **two-face** embed syntax definitions collected by [bat], which are third-party
  Sublime Text grammars under their own (mostly MIT) licenses. See two-face's acknowledgements for
  the list.
- **resvg** brings its own font handling; the fonts filer draws with are the ones already installed
  on your system and are not redistributed here.

[bat]: https://github.com/sharkdp/bat
