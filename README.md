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
`find_arrow`, `filter`, `search`, `help`, `tasks_show`, `spot`, `noop`, plus `jump`, `toggle_render` and
`toggle_outline` (this project's own). In the `[spot]` section: `close`, `arrow`, `swipe` and
`copy cell`.

A few plugin invocations are mapped onto built-in behavior so common setups keep working:

| In `keymap.toml` | Effect |
| --- | --- |
| `plugin toggle-pane max-preview` | Maximize / restore the preview pane |
| `plugin toggle-pane min-parent` | Hide / show the parent pane |
| `plugin bookmarks save` / `jump` / `delete` / `delete_all` | Bookmarks (press the key to assign or jump) |
| `plugin smart-enter`, `plugin smart-filter` | `open` (a directory is entered), `filter --smart` |

Anything else parses cleanly, reports itself as unsupported in the help panel, and shows a toast
if you press it — it never breaks config loading. There is no Lua runtime.

### theme.toml

`[mgr]` colors, `[status]` modes, `[which]`, `[filetype].rules` and `[icon]` (`globs`, `dirs`,
`exts`, `files`, `conds`) are applied on top of a built-in dark theme. Colors may be ANSI names
(`lightblue`, `darkgray`, `reset`) or hex (`#7ab8f5`). `syntect_theme` selects the preview's
syntax theme.

### filer.toml (GUI-only settings)

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
`-g file:N` to VS Code / Cursor / Windsurf, and as `file:N` to Helix / Sublime / Zed; other
openers just open the file. `<Esc>`, `h` / `←` or `<S-Tab>` gives the keys back to the file list,
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
| `y` `x` `p` `P` `-` `_` | yank / cut / paste / paste-force / symlink / relative symlink |
| `d` `D` | recycle bin / permanent delete (with confirmation) |
| `a` `r` | create (trailing `/` makes a directory) / rename |
| `o` `O` `<Enter>` `<S-Enter>` | open / open with… / open (at the outline's line) / open with… |
| `/` `?` `n` `N` `f` | find next / previous / repeat / repeat back / filter |
| `s` `S` `<C-s>` | search by name / by content / stop |
| `z` | fuzzy-jump to a bookmark or recent directory |
| `.` `,…` `m…` | hidden files / sort menu / line-mode menu |
| `t` `1`–`9` `[` `]` `{` `}` | tabs |
| `;` `:` | shell command / blocking shell command |
| `<A-k>` `<A-j>` `M` | scroll the preview / Markdown rendered ↔ source |
| `<S-Tab>` | move the keys into the preview's outline and back |
| `<Tab>` | spot: details of the hovered file |
| `w` `q` | tasks / quit |

Mouse works too: click to move the cursor, double-click to open, wheel to scroll.

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

## Known limits

- Windows-first. The code compiles for Unix but only Windows is tested; `block = true` openers and
  the hidden-file attribute are Windows-specific paths.
- No Lua plugin runtime — see the plugin table above for what is emulated natively.
- Archives show a metadata card rather than a listing; woff / woff2 fonts aren't previewed. Video,
  PDF and HEIC previews rely on Windows thumbnail handlers (see [Other previews](#other-previews)).
- `[input]`, `[confirm]` and `[pick]` keymap layers are parsed for compatibility, but the prompts
  are native widgets (for IME and clipboard support), so only Enter / Esc / Tab are configurable.

## Layout

```
src/
  main.rs        window, fonts, CLI, input routing
  app.rs         state and the Act dispatcher — every key and click goes through it
  config/        yazi.toml, keymap.toml, theme.toml, key notation, command parsing
  core/          folder + cursor state, tabs, fuzzy matching
  fs/            entries, sorting, scan pool, file operations, watcher
  preview/       preview worker: text + syntect, Markdown layout, images, SVG, fonts, shell thumbnails
  ui/            painting: columns, preview pane, overlays
  search.rs      recursive name/content search
  exec.rs        openers and shell
```
