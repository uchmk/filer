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
`find_arrow`, `filter`, `search`, `help`, `tasks_show`, `spot`, `noop`, plus `jump`, `palette`,
`menu`, `extract`, `compress`, `task_toggle`, `task_cancel`, `task_top`, `split`, `pane_focus`,
`toggle_render` and `toggle_outline` (this
project's own). `select` and `select_all` are accepted as `toggle --state=on` /
`toggle_all --state=on`. In the `[input]` section: `close --submit` (and the `*_do` spellings),
`close` and `complete`; in `[spot]`: `close`, `arrow`, `swipe` and `copy cell`.

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
| `<Space>` `v` `V` `<C-a>` `<C-r>` | toggle / visual / visual-unset / select all / invert |
| `y` `x` `Y` `p` `P` `-` `_` `<C-->` | yank / cut / cancel the yank / paste / paste-force / symlink / relative symlink / hardlink |
| `d` `D` | recycle bin / permanent delete (with confirmation) |
| `a` `r` | create (trailing `/` makes a directory) / rename |
| `e` `E` | extract the selected archives / compress the selection |
| `g…` | `gh` home, `gd` Downloads, `gD` Documents, `gc` config, `gt` temp, `g<Space>` type a path, `gf` follow the link |
| `c…` | `cc` copy the path, `cd` the parent, `cf` the file name, `cn` the name without its extension |
| `o` `O` `<Enter>` `<S-Enter>` | open / open with… / open (at the outline's line) / open with… |
| `/` `?` `n` `N` `f` | find next / previous / repeat / repeat back / filter |
| `s` `S` `<C-s>` | search by name / by content / stop |
| `z` | fuzzy-jump to a bookmark or recent directory |
| `.` `,…` `m…` | hidden files / sort menu / line-mode menu |
| `t` `1`–`9` `[` `]` `{` `}` `<C-c>` | new tab / switch / previous / next / move it left / right / close it (quits on the last) |
| `<F5>` | re-read the current directory |
| `<C-w>` `<C-S-w>` | split the view in two panes / move between them, close the split |
| `;` `:` | shell command / blocking shell command |
| `<A-k>` `<A-j>` `M` | scroll the preview / Markdown rendered ↔ source |
| `<S-Tab>` | move the keys into the preview's outline and back |
| `<Tab>` | spot: details of the hovered file |
| `<C-S-p>` | command palette: fuzzy-search every key binding and run it |
| `<S-F10>` | context menu for the file under the cursor |
| `w` `q` | tasks (`p` pause, `x` cancel, `t` to the front) / quit |

Mouse works too: click to move the cursor, double-click to open, right-click for the context
menu, wheel to scroll. `Shift`+click selects from the cursor to the row you clicked, and
`Ctrl`+click (`Cmd` on macOS) adds or removes one row. Both share the selection with `<Space>`
and visual mode, so you can start a range with the mouse and finish it with the keyboard.

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

Development currently centers on Windows (x64), but the goal is cross-platform support across the
major operating systems and architectures.

| OS | Architectures | Notes |
| :--- | :--- | :--- |
| **Windows** | x64 / ARM64 / x86 | UNC paths, integration with common editors |
| **macOS** | Apple Silicon (ARM64) / Intel (x64) | Cmd key support, Finder integration |
| **Linux** | x64 / ARM64 | X11 / Wayland |

## Network paths (UNC)

On Windows a UNC path is an ordinary path here — type `\\192.168.1.5\pub` (or a mapped drive
letter) into the `cd` prompt and browse it like any folder. Forward slashes work too
(`//192.168.1.5/pub`) and are shown back in the `\\host\share` spelling.

- A share root is the top of the tree: `..` / `h` stop there instead of climbing into the host.
- `\\host` on its own names no share, so there is nothing to list; filer reports the host you
  typed rather than silently dropping you at `\host` on the current drive. Enumerating a host's
  shares is not implemented — give the share name.
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
- An archive's *preview* is a metadata card rather than a listing of what is inside — `e` unpacks
  it, but the pane does not browse it. woff / woff2 fonts aren't previewed. Video,
  PDF and HEIC previews rely on Windows thumbnail handlers (see [Other previews](#other-previews)).
- `[input]`, `[confirm]` and `[pick]` keymap layers are parsed for compatibility, but the prompts
  are native widgets (for IME and clipboard support), so only Enter / Esc / Tab are configurable.
- Git signs need `git` on `PATH`; without it the rows are simply unmarked. Only the status is
  shown — there is no staging, diffing or committing here, and no branch in the status bar yet.

## Layout

```
src/
  main.rs        window, fonts, CLI, input routing
  app.rs         state and the Act dispatcher — every key and click goes through it
  config/        yazi.toml, keymap.toml, theme.toml, key notation, command parsing
  core/          folder + cursor state, tabs, fuzzy matching
  fs/            entries, sorting, scan pool, file operations, watcher, archives, git status
  preview/       preview worker: text + syntect, Markdown layout, images, SVG, fonts, shell thumbnails
  ui/            painting: columns, preview pane, overlays
  search.rs      recursive name/content search
  exec.rs        openers and shell
```

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
