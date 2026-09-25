# Key checklist

Generated from `src/config/defaults/keymap.toml` by `cargo run --example make-keycheck`. Ticks are kept
across regenerations, so re-running after a keymap change costs nothing already done.
Edit the ticks, not the rows: anything else here is overwritten.

**75 / 196 checked.**

A key is checked when it did what the description says _and_ did nothing else —
`<A-m>` once ran its own command and the unmodified `m` as well, and both halves
looked correct on their own. Anything surprising goes in an issue (`<F12>`).

## `[mgr]` — 75 / 142

The file list: what is in front of you unless an overlay is.

- [x] `<Esc>` — Exit visual mode, clear selection, or cancel search · `escape`
- [x] `q` — Quit the process · `quit`
- [x] `<C-q>` — Quit the process · `quit`
- [x] `<C-c>` — Close the current tab, or quit if it is the last · `close`

### Navigation

- [x] `k` — Move cursor up · `arrow -1`
- [x] `j` — Move cursor down · `arrow 1`
- [x] `<Up>` — Move cursor up · `arrow -1`
- [x] `<Down>` — Move cursor down · `arrow 1`
- [x] `<C-u>` — Move cursor up half page · `arrow -50%`
- [x] `<C-d>` — Move cursor down half page · `arrow 50%`
- [x] `<C-b>` — Move cursor up one page · `arrow -100%`
- [x] `<C-f>` — Move cursor down one page · `arrow 100%`
- [x] `<PageUp>` — Move cursor up one page · `arrow -100%`
- [x] `<PageDown>` — Move cursor down one page · `arrow 100%`
- [x] `g g` — Move cursor to the top · `arrow top`
- [x] `G` — Move cursor to the bottom · `arrow bot`
- [x] `<Home>` — Move cursor to the top · `arrow top`
- [x] `<End>` — Move cursor to the bottom · `arrow bot`
- [x] `h` — Go back to the parent directory · `leave`
- [x] `l` — Enter the directory, or focus the file's outline · `enter`
- [x] `<Left>` — Go back to the parent directory · `leave`
- [x] `<Right>` — Enter the directory, or focus the file's outline · `enter`
- [x] `<Backspace>` — Go back to the parent directory · `leave`
- [x] `H` — Go back to the previous directory · `back`
- [x] `L` — Go forward to the next directory · `forward`
- [x] `<F5>` — Re-read the current directory · `refresh`
- [x] `<C-F5>` — Read the config files again (theme, icons, keys) · `config_reload`
- [x] `<F3>` — Quick look: the hovered file, big, over the panes · `quick`
- [ ] `<A-k>` — Scroll the preview up · `seek -5`
- [ ] `<A-j>` — Scroll the preview down · `seek 5`
- [x] `<A-i>` — Zoom into the image · `zoom in`
- [x] `<A-o>` — Zoom out of the image · `zoom out`
- [x] `<A-0>` — Fit the image to the pane · `zoom fit`
- [x] `<A-1>` — Show the image at 1:1 · `zoom actual`
- [x] `<A-n>` — Show or hide the preview's minimap · `minimap`
- [x] `M` — Switch Markdown preview between rendered and source · `toggle_render`
- [ ] `<BackTab>` — Focus the preview's outline (functions / headings) · `toggle_outline`
- [ ] `<Tab>` — Spot hovered file · `spot`

### Selection

- [x] `<Space>` — Toggle the current selection state · `[ "toggle", "arrow 1" ]`
- [x] `<C-a>` — Select all files · `toggle_all --state=on`
- [x] `<C-S-r>` — Invert selection of all files · `toggle_all`
- [ ] `v` — Enter visual mode (selection mode) · `visual_mode`
- [ ] `V` — Enter visual mode (unset mode) · `visual_mode --unset`

### Operations

- [x] `o` — Open the selected files · `open`
- [x] `O` — Open the selected files interactively · `open --interactive`
- [x] `<Enter>` — Open the selected files · `open`
- [x] `<S-Enter>` — Open the selected files interactively · `open --interactive`
- [x] `y` — Yank the selected files (copy) · `yank`
- [ ] `x` — Yank the selected files (cut) · `yank --cut`
- [x] `Y` — Cancel the yank status · `unyank`
- [x] `X` — Cancel the yank status · `unyank`
- [x] `p` — Paste the files · `paste`
- [x] `P` — Paste the files (overwrite if the destination exists) · `paste --force`
- [ ] `-` — Symlink the absolute path of yanked files · `link`
- [ ] `_` — Symlink the relative path of yanked files · `link --relative`
- [ ] `<C-->` — Hardlink the yanked files · `hardlink`
- [ ] `<A-c>` — Copy the selection to the other pane · `send_pane`
- [ ] `<A-m>` — Move the selection to the other pane · `send_pane --cut`
- [ ] `e` — Extract the selected archives · `extract`
- [ ] `E` — Compress the selection into an archive · `compress`
- [x] `d` — Move the files to the recycle bin · `remove`
- [x] `D` — Permanently delete the files · `remove --permanently`
- [x] `u` — Undo the last rename, or put the last deleted files back · `undo`
- [x] `U` — Redo what undo took back · `redo`
- [x] `<C-r>` — Redo what undo took back · `redo`
- [x] `a` — Create a file; end with / or \ for a directory · `create`
- [x] `r` — Rename the file or directory · `rename`
- [x] `R` — Rename everything selected by one rule · `bulk_rename`
- [ ] `<A-d>` — Compare two files side by side · `compare`
- [ ] `;` — Run a shell command · `shell --interactive`
- [ ] `:` — Run a shell command (block until finished) · `shell --interactive --block`
- [x] `.` — Toggle the visibility of hidden files · `hidden`
- [x] `c c` — Copy the absolute path · `copy path`
- [x] `c d` — Copy the path of the parent directory · `copy dirname`
- [x] `c f` — Copy the name of the file · `copy filename`
- [x] `c n` — Copy the name of the file without extension · `copy name_without_ext`

### Find / filter / search

- [x] `/` — Find next file · `find --smart`
- [x] `?` — Find previous file · `find --previous --smart`
- [x] `n` — Go to the next found file · `find_arrow`
- [x] `N` — Go to the previous found file · `find_arrow --previous`
- [x] `f` — Filter the files · `filter --smart`
- [x] `s` — Search files by name, recursively · `search --via=name`
- [x] `S` — Search files by content, recursively · `search --via=content`
- [ ] `<C-s>` — Cancel the ongoing search · `escape --search`
- [x] `z` — Jump to a bookmark or a recently visited directory · `jump`
- [x] `'` — Go to the bookmark under a letter · `plugin bookmarks jump`
- [x] `b b` — List the bookmarks, and go to one · `plugin bookmarks list`
- [x] `b s` — Bookmark this directory under a letter · `plugin bookmarks save`
- [x] `b d` — Delete the bookmark under a letter · `plugin bookmarks delete`
- [x] `b D` — Delete every bookmark · `plugin bookmarks delete_all`
- [x] `B` — Bookmark this directory under a letter (same as `bs`) · `plugin bookmarks save`
- [x] `<A-b>` — Delete the bookmark under a letter (same as `bd`) · `plugin bookmarks delete`
- [x] `<A-B>` — Delete every bookmark (same as `bD`) · `plugin bookmarks delete_all`

### Sorting

- [ ] `  m` — Sort by modified time (newest first) · `sort mtime --reverse`
- [ ] `  M` — Sort by modified time (oldest first) · `sort mtime --no-reverse`
- [ ] `  b` — Sort by created time (newest first) · `sort btime --reverse`
- [ ] `  B` — Sort by created time (oldest first) · `sort btime --no-reverse`
- [ ] `  e` — Sort by extension · `sort extension --no-reverse`
- [ ] `  E` — Sort by extension (reverse) · `sort extension --reverse`
- [ ] `  a` — Sort alphabetically · `sort alphabetical --no-reverse`
- [ ] `  A` — Sort alphabetically (reverse) · `sort alphabetical --reverse`
- [ ] `  n` — Sort naturally · `sort natural --no-reverse`
- [ ] `  N` — Sort naturally (reverse) · `sort natural --reverse`
- [ ] `  s` — Sort by size (largest first) · `sort size --reverse`
- [ ] `  S` — Sort by size (smallest first) · `sort size --no-reverse`
- [ ] `  r` — Sort randomly · `sort random --no-reverse`

### Line mode

- [ ] `m s` — Line mode: size · `linemode size`
- [ ] `m t` — Line mode: modified time · `linemode mtime`
- [ ] `m b` — Line mode: created time · `linemode btime`
- [ ] `m p` — Line mode: permissions · `linemode permissions`
- [ ] `m n` — Line mode: none · `linemode none`

### Goto

- [ ] `g h` — Go to the home directory · `cd ~`
- [ ] `g d` — Go to the downloads directory · `cd ~/Downloads`
- [ ] `g D` — Go to the documents directory · `cd ~/Documents`
- [ ] `g c` — Go to the config directory · `cd %APPDATA%/yazi/config`
- [ ] `g t` — Go to the temporary directory · `cd %TEMP%`
- [ ] `g <Space>` — Jump interactively · `cd --interactive`
- [ ] `g f` — Follow the hovered symlink · `follow`

### Tabs

- [ ] `t` — Create a new tab with the current directory · `tab_create --current`
- [ ] `1` — Switch to the first tab · `tab_switch 0`
- [ ] `2` — Switch to the second tab · `tab_switch 1`
- [ ] `3` — Switch to the third tab · `tab_switch 2`
- [ ] `4` — Switch to the fourth tab · `tab_switch 3`
- [ ] `5` — Switch to the fifth tab · `tab_switch 4`
- [ ] `6` — Switch to the sixth tab · `tab_switch 5`
- [ ] `7` — Switch to the seventh tab · `tab_switch 6`
- [ ] `8` — Switch to the eighth tab · `tab_switch 7`
- [ ] `9` — Switch to the ninth tab · `tab_switch 8`
- [ ] `[` — Switch to the previous tab · `tab_switch -1 --relative`
- [ ] `]` — Switch to the next tab · `tab_switch 1 --relative`
- [ ] `{` — Swap the current tab with the previous one · `tab_swap -1`
- [ ] `}` — Swap the current tab with the next one · `tab_swap 1`

### Split view

- [ ] `<C-w>` — Split the view, or move to the other pane · `pane_focus`
- [ ] `<C-S-w>` — Close the second pane · `split close`

### Panels

- [ ] `w` — Show the task manager · `tasks_show`
- [ ] `~` — Open help · `help`
- [ ] `<F1>` — Open help · `help`
- [ ] `<F12>` — Open a pre-filled bug report in the browser · `bug-report`
- [ ] `<C-S-p>` — Open the command palette · `palette`
- [ ] `<S-F10>` — Open the context menu · `menu`
- [ ] `<C-t>` — Open the terminal pane · `terminal`
- [ ] `<A-t>` — Type the selected paths into the terminal · `term_send`

## `[term]` — 0 / 12

While the terminal pane holds the keys. Everything not listed here goes to the shell.

### Terminal pane

- [ ] `<C-t>` — Give the keys back to the list (the shell keeps running) · `close`
- [ ] `<C-S-t>` — Close the terminal and end the shell · `terminal close`
- [ ] `<F1>` — Show the key list · `help`
- [ ] `<C-S-p>` — Command palette · `palette`
- [ ] `<A-Up>` — Put the pane where the shell is · `term_cd`
- [ ] `<S-PageUp>` — Scroll back half a screen · `term_scroll -50%`
- [ ] `<S-PageDown>` — Scroll forward half a screen · `term_scroll 50%`
- [ ] `<S-Home>` — To the top of the scrollback · `term_scroll top`
- [ ] `<S-End>` — Back to the bottom · `term_scroll bot`
- [ ] `<C-S-f>` — Find in the scrollback · `term_find`
- [ ] `<C-S-n>` — Find the next match · `term_find --repeat`
- [ ] `<C-S-b>` — Find the previous match · `term_find --repeat --prev`

## `[input]` — 0 / 3

The one-line prompt — `cd`, rename, filter, search.

### Input line

- [ ] `<Enter>` — Submit · `close --submit`
- [ ] `<Esc>` — Cancel · `close`
- [ ] `<Tab>` — Complete the path · `complete`

## `[confirm]` — 0 / 2

A yes/no prompt.

### Input line

- [ ] `<Enter>` — Confirm · `close --submit`
- [ ] `<Esc>` — Cancel · `close`

## `[pick]` — 0 / 2

A chooser — the command palette, the context menu.

### Input line

- [ ] `<Enter>` — Submit · `close --submit`
- [ ] `<Esc>` — Cancel · `close`

## `[help]` — 0 / 1

This panel (`~` or `<F1>`).

### Input line

- [ ] `<Esc>` — Close help · `close`

## `[tasks]` — 0 / 9

The task manager (`w`).

### Input line

- [ ] `<Esc>` — Close the task manager · `close`
- [ ] `q` — Close the task manager · `close`
- [ ] `j` — Next task · `arrow 1`
- [ ] `k` — Previous task · `arrow -1`
- [ ] `<Down>` — Next task · `arrow 1`
- [ ] `<Up>` — Previous task · `arrow -1`
- [ ] `p` — Pause / resume the task · `task_toggle`
- [ ] `x` — Cancel the task · `task_cancel`
- [ ] `t` — Move the task to the front of the queue · `task_top`

## `[spot]` — 0 / 13

The details panel (`<Tab>`).

### Input line

- [ ] `<Esc>` — Close the spotter · `close`
- [ ] `<Tab>` — Close the spotter · `close`
- [ ] `q` — Close the spotter · `close`
- [ ] `k` — Previous line · `arrow -1`
- [ ] `j` — Next line · `arrow 1`
- [ ] `h` — Swipe to the previous file · `swipe -1`
- [ ] `l` — Swipe to the next file · `swipe 1`
- [ ] `c` — Copy the selected value · `copy cell`
- [ ] `<Up>` — Previous line · `arrow -1`
- [ ] `<Down>` — Next line · `arrow 1`
- [ ] `<Left>` — Swipe to the previous file · `swipe -1`
- [ ] `<Right>` — Swipe to the next file · `swipe 1`
- [ ] `y` — Copy the selected value · `copy cell`

## `[diff]` — 0 / 12

The side-by-side comparison (`<A-d>`).

### Compare (side by side)

- [ ] `q` — Close the comparison · `close`
- [ ] `<Esc>` — Close the comparison · `close`
- [ ] `k` — Up one line · `arrow -1`
- [ ] `j` — Down one line · `arrow 1`
- [ ] `<Up>` — Up one line · `arrow -1`
- [ ] `<Down>` — Down one line · `arrow 1`
- [ ] `<C-u>` — Up half a page · `arrow -50%`
- [ ] `<C-d>` — Down half a page · `arrow 50%`
- [ ] `g g` — To the top · `arrow top`
- [ ] `G` — To the bottom · `arrow bot`
- [ ] `n` — To the next difference · `find_arrow`
- [ ] `N` — To the previous difference · `find_arrow --previous`
