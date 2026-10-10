# Key checklist

Generated from `src/config/defaults/keymap.toml` by `cargo run --example make-keycheck`. Ticks are kept
across regenerations, so re-running after a keymap change costs nothing already done.
Edit the ticks, not the rows: anything else here is overwritten.

285 keys. How many are checked is not written here, so that two pull requests ticking
keys do not conflict over a total: `cargo run --example make-keycheck -- --stats`.

A key is checked when it did what the description says _and_ did nothing else —
`<A-m>` once ran its own command and the unmodified `m` as well, and both halves
looked correct on their own. Anything surprising goes in an issue (`<F12>`).

## `[mgr]`

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
- [x] `l` — Enter the directory or archive, or focus the file's outline · `enter`
- [x] `<Left>` — Go back to the parent directory · `leave`
- [x] `<Right>` — Enter the directory or archive, or focus the file's outline · `enter`
- [x] `<Backspace>` — Go back to the parent directory · `leave`
- [x] `H` — Go back to the previous directory · `back`
- [x] `L` — Go forward to the next directory · `forward`
- [x] `<A-Left>` — Go back to the previous directory · `back`
- [x] `<A-Right>` — Go forward to the next directory · `forward`
- [x] `<F5>` — Re-read the current directory · `refresh`
- [x] `<C-F5>` — Read the config files again (theme, icons, keys) · `config_reload`
- [x] `<F3>` — Quick look: the hovered file, big, over the panes · `quick`
- [x] `T` — Maximize or restore the preview pane · `plugin toggle-pane max-preview`
- [x] `<A-k>` — Scroll the preview up · `seek -5`
- [x] `<A-j>` — Scroll the preview down · `seek 5`
- [x] `<A-g>` — Scroll the preview to its top · `seek top`
- [x] `<A-G>` — Scroll the preview to its end · `seek bot`
- [x] `<A-i>` — Zoom into the image · `zoom in`
- [x] `<A-o>` — Zoom out of the image · `zoom out`
- [x] `<A-0>` — Fit the image to the pane · `zoom fit`
- [x] `<A-1>` — Show the image at 1:1 · `zoom actual`
- [x] `<A-n>` — Show or hide the preview's minimap · `minimap`
- [x] `M` — Switch Markdown preview between rendered and source · `toggle_render`
- [x] `<BackTab>` — Focus the preview's outline (functions / headings) · `toggle_outline`
- [x] `<Tab>` — Spot hovered file · `spot`

### Selection

- [x] `<Space>` — Toggle the current selection state · `[ "toggle", "arrow 1" ]`
- [x] `<C-a>` — Select all files · `toggle_all --state=on`
- [x] `<C-S-r>` — Invert selection of all files · `toggle_all`
- [x] `v` — Enter visual mode (selection mode) · `visual_mode`
- [x] `V` — Enter visual mode (unset mode) · `visual_mode --unset`

### Operations

- [x] `o` — Open the selected files · `open`
- [x] `O` — Open the selected files interactively · `open --interactive`
- [x] `<Enter>` — Open the selected files · `open`
- [x] `<S-Enter>` — Open the selected files interactively · `open --interactive`
- [x] `y` — Yank the selected files (copy) · `yank`
- [x] `x` — Yank the selected files (cut) · `yank --cut`
- [x] `Y` — Cancel the yank status · `unyank`
- [x] `X` — Cancel the yank status · `unyank`
- [x] `p` — Paste the files · `paste`
- [x] `P` — Paste the files (overwrite if the destination exists) · `paste --force`
- [x] `-` — Symlink the absolute path of yanked files · `link`
- [x] `_` — Symlink the relative path of yanked files · `link --relative`
- [x] `=` — Hardlink the yanked files · `hardlink`
- [x] `<C-+>` — Make everything bigger · `scale in`
- [x] `<C-=>` — Make everything bigger · `scale in`
- [ ] `<C-;>` — Make everything bigger · `scale in`
- [x] `<C-->` — Make everything smaller · `scale out`
- [x] `<C-0>` — Back to the original size · `scale reset`
- [x] `<A-c>` — Copy the selection to the other pane · `send_pane`
- [x] `<A-m>` — Move the selection to the other pane · `send_pane --cut`
- [x] `e` — Extract the selected archives · `extract`
- [x] `E` — Compress the selection into an archive · `compress`
- [x] `d` — Move the files to the recycle bin · `remove`
- [x] `D` — Permanently delete the files · `remove --permanently`
- [x] `u` — Undo the last rename, or put the last deleted files back · `undo`
- [x] `U` — Redo what undo took back · `redo`
- [x] `<C-r>` — Redo what undo took back · `redo`
- [x] `a` — Create a file; end with / or \ for a directory · `create`
- [x] `r` — Rename the file or directory · `rename`
- [x] `R` — Rename everything selected by one rule · `bulk_rename`
- [x] `<A-d>` — Compare two files side by side · `compare`
- [x] `;` — Run a shell command · `shell --interactive`
- [x] `:` — Run a shell command in a new console · `shell --interactive --block`
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
- [ ] `F` — Search files fuzzily, recursively, best match first · `search --via=fuzzy`
- [x] `<C-s>` — Cancel the ongoing search · `escape --search`
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

- [x] `, m` — Sort by modified time (newest first) · `sort mtime --reverse`
- [x] `, M` — Sort by modified time (oldest first) · `sort mtime --no-reverse`
- [x] `, b` — Sort by created time (newest first) · `sort btime --reverse`
- [x] `, B` — Sort by created time (oldest first) · `sort btime --no-reverse`
- [x] `, e` — Sort by extension · `sort extension --no-reverse`
- [x] `, E` — Sort by extension (reverse) · `sort extension --reverse`
- [x] `, a` — Sort alphabetically · `sort alphabetical --no-reverse`
- [x] `, A` — Sort alphabetically (reverse) · `sort alphabetical --reverse`
- [x] `, n` — Sort naturally · `sort natural --no-reverse`
- [x] `, N` — Sort naturally (reverse) · `sort natural --reverse`
- [x] `, s` — Sort by size (largest first) · `sort size --reverse`
- [x] `, S` — Sort by size (smallest first) · `sort size --no-reverse`
- [x] `, r` — Sort randomly · `sort random --no-reverse`

### Line mode

- [x] `m s` — Line mode: size · `linemode size`
- [x] `m t` — Line mode: modified time · `linemode mtime`
- [x] `m b` — Line mode: created time · `linemode btime`
- [x] `m p` — Line mode: permissions · `linemode permissions`
- [x] `m u` — Line mode: disk usage (gu) · `linemode usage`
- [x] `m n` — Line mode: none · `linemode none`

### Goto

- [x] `g h` — Go to the home directory · `cd ~`
- [x] `g d` — Go to the downloads directory · `cd ~/Downloads`
- [x] `g D` — Go to the documents directory · `cd ~/Documents`
- [x] `g c` — Go to kura's config directory · `cd %KURA_CONFIG_HOME%`
- [x] `g y` — Go to yazi's config directory · `cd %YAZI_CONFIG_HOME%`
- [x] `g t` — Go to the temporary directory · `cd %TEMP%`
- [x] `g <Space>` — Jump interactively · `cd --interactive`
- [x] `g f` — Follow the hovered symlink · `follow`
- [x] `g u` — Measure what is taking up the space here · `usage`

### Tabs

- [x] `t` — Create a new tab at the hovered folder, or the current directory · `tab_create --current`
- [x] `1` — Switch to the first tab · `tab_switch 0`
- [x] `2` — Switch to the second tab · `tab_switch 1`
- [x] `3` — Switch to the third tab · `tab_switch 2`
- [x] `4` — Switch to the fourth tab · `tab_switch 3`
- [x] `5` — Switch to the fifth tab · `tab_switch 4`
- [x] `6` — Switch to the sixth tab · `tab_switch 5`
- [x] `7` — Switch to the seventh tab · `tab_switch 6`
- [x] `8` — Switch to the eighth tab · `tab_switch 7`
- [x] `9` — Switch to the ninth tab · `tab_switch 8`
- [x] `[` — Switch to the previous tab · `tab_switch -1 --relative`
- [x] `]` — Switch to the next tab · `tab_switch 1 --relative`
- [x] `{` — Swap the current tab with the previous one · `tab_swap -1`
- [x] `}` — Swap the current tab with the next one · `tab_swap 1`

### Split view

- [x] `<C-w>` — Split the view, or move to the other pane · `pane_focus`
- [x] `<C-S-w>` — Close the second pane · `split close`

### Panels

- [x] `w` — Show the task manager · `tasks_show`
- [x] `~` — Open help · `help`
- [x] `<F1>` — Open help · `help`
- [ ] `<C-,>` — Open the settings screen · `settings`
- [x] `<F12>` — Open a pre-filled bug report in the browser · `bug-report`
- [x] `<C-S-p>` — Open the command palette · `palette`
- [x] `<S-F10>` — Open the context menu · `menu`
- [x] `<C-t>` — Open the terminal pane · `terminal`
- [x] `<C-S-t>` — Close the terminal and end the shell · `terminal close`
- [x] `<A-t>` — Type the selected paths into the terminal · `term_send`
- [x] `<C-S-Enter>` — Give the terminal pane the window (the same key in the pane gives it back) · `term_max`

## `[term]`

While the terminal pane holds the keys. Everything not listed here goes to the shell.

### Terminal pane

- [x] `<C-t>` — Give the keys back to the list (the shell keeps running) · `close`
- [x] `<C-S-t>` — Close the terminal and end the shell · `terminal close`
- [x] `<F1>` — Show the key list · `help`
- [x] `<C-S-p>` — Command palette · `palette`
- [x] `<C-F5>` — Read the config files again (theme, icons, keys) · `config_reload`
- [x] `<A-Up>` — Put the pane where the shell is · `term_cd`
- [x] `<C-S-Enter>` — Give the terminal pane the window, or hand it back · `term_max`
- [x] `<S-PageUp>` — Scroll back half a screen · `term_scroll -50%`
- [x] `<S-PageDown>` — Scroll forward half a screen · `term_scroll 50%`
- [x] `<S-Home>` — To the top of the scrollback · `term_scroll top`
- [x] `<S-End>` — Back to the bottom · `term_scroll bot`
- [x] `<A-k>` — Scroll the terminal up · `term_scroll -5`
- [x] `<A-j>` — Scroll the terminal down · `term_scroll 5`
- [x] `<C-S-f>` — Find in the scrollback · `term_find`
- [x] `<C-S-n>` — Find the next match · `term_find --repeat`
- [x] `<C-S-b>` — Find the previous match · `term_find --repeat --prev`
- [x] `<C-S-Up>` — Scroll to the prompt above · `term_prompt --prev`
- [x] `<C-S-Down>` — Scroll to the prompt below · `term_prompt`
- [x] `<C-S-l>` — Copy the last command's output · `term_copy_output`

## `[input]`

The one-line prompt — `cd`, rename, filter, search.

### Input line

- [x] `<Enter>` — Submit · `close --submit`
- [x] `<Esc>` — Cancel · `close`
- [x] `<Tab>` — Complete the path (E: next archive format) · `complete`

## `[confirm]`

A yes/no prompt.

### Input line

- [x] `<Enter>` — Confirm · `close --submit`
- [x] `<Esc>` — Cancel · `close`

## `[pick]`

A chooser — the command palette, the context menu.

### Input line

- [x] `<Enter>` — Submit · `close --submit`
- [x] `<Esc>` — Cancel · `close`
- [x] `<C-n>` — Next match · `arrow 1`
- [x] `<C-j>` — Next match · `arrow 1`
- [x] `<C-p>` — Previous match · `arrow -1`
- [x] `<C-k>` — Previous match · `arrow -1`

## `[help]`

This panel (`~` or `<F1>`).

### Input line

- [x] `<Esc>` — Close help · `close`
- [x] `q` — Close help · `close`
- [x] `~` — Close help · `help`
- [x] `<F1>` — Close help · `help`
- [x] `C` — Copy the whole list as text · `copy all`
- [x] `<C-F5>` — Read the config files again (theme, icons, keys) · `config_reload`
- [x] `k` — Up one line · `arrow -1`
- [x] `j` — Down one line · `arrow 1`
- [x] `<Up>` — Up one line · `arrow -1`
- [x] `<Down>` — Down one line · `arrow 1`
- [x] `<A-k>` — Up half a page · `arrow -50%`
- [x] `<A-j>` — Down half a page · `arrow 50%`
- [x] `<C-u>` — Up half a page · `arrow -50%`
- [x] `<C-d>` — Down half a page · `arrow 50%`
- [x] `<PageUp>` — Up a page · `arrow -100%`
- [x] `<PageDown>` — Down a page · `arrow 100%`
- [x] `g g` — To the top · `arrow top`
- [x] `G` — To the bottom · `arrow bot`
- [x] `<C-+>` — Make everything bigger · `scale in`
- [x] `<C-=>` — Make everything bigger · `scale in`
- [ ] `<C-;>` — Make everything bigger · `scale in`
- [x] `<C-->` — Make everything smaller · `scale out`
- [x] `<C-0>` — Back to the original size · `scale reset`

## `[tasks]`

The task manager (`w`).

### Input line

- [x] `<Esc>` — Close the task manager · `close`
- [ ] `<F1>` — Open help · `help`
- [x] `q` — Close the task manager · `close`
- [x] `j` — Next task · `arrow 1`
- [x] `k` — Previous task · `arrow -1`
- [x] `<Down>` — Next task · `arrow 1`
- [x] `<Up>` — Previous task · `arrow -1`
- [x] `p` — Pause / resume the task · `task_toggle`
- [x] `x` — Cancel the task · `task_cancel`
- [x] `t` — Move the task to the front of the queue · `task_top`
- [x] `<C-+>` — Make everything bigger · `scale in`
- [x] `<C-=>` — Make everything bigger · `scale in`
- [ ] `<C-;>` — Make everything bigger · `scale in`
- [x] `<C-->` — Make everything smaller · `scale out`
- [x] `<C-0>` — Back to the original size · `scale reset`

## `[spot]`

The details panel (`<Tab>`).

### Input line

- [x] `<Esc>` — Close the spotter · `close`
- [ ] `<F1>` — Open help · `help`
- [x] `<Tab>` — Close the spotter · `close`
- [x] `q` — Close the spotter · `close`
- [x] `k` — Spot the previous file · `swipe -1`
- [x] `j` — Spot the next file · `swipe 1`
- [x] `h` — Go back to the parent directory · `leave`
- [x] `l` — Enter the directory · `enter`
- [x] `<A-k>` — Previous line of the panel · `arrow -1`
- [x] `<A-j>` — Next line of the panel · `arrow 1`
- [x] `c` — Copy the selected value · `copy cell`
- [x] `C` — Copy the whole panel, labelled · `copy all`
- [x] `<Enter>` — Open the pull request or branch on its rows; enter the directory elsewhere · `enter`
- [x] `<Up>` — Spot the previous file · `swipe -1`
- [x] `<Down>` — Spot the next file · `swipe 1`
- [x] `<A-Up>` — Previous line of the panel · `arrow -1`
- [x] `<A-Down>` — Next line of the panel · `arrow 1`
- [x] `<Left>` — Go back to the parent directory · `leave`
- [x] `<Right>` — Enter the directory · `enter`
- [x] `y` — Copy the selected value · `copy cell`
- [x] `<C-+>` — Make everything bigger · `scale in`
- [x] `<C-=>` — Make everything bigger · `scale in`
- [ ] `<C-;>` — Make everything bigger · `scale in`
- [x] `<C-->` — Make everything smaller · `scale out`
- [x] `<C-0>` — Back to the original size · `scale reset`

## `[diff]`

The side-by-side comparison (`<A-d>`).

### Compare (side by side)

- [x] `q` — Close the comparison · `close`
- [ ] `<F1>` — Open help · `help`
- [x] `<Esc>` — Close the comparison · `close`
- [x] `k` — Up one line · `arrow -1`
- [x] `j` — Down one line · `arrow 1`
- [x] `<Up>` — Up one line · `arrow -1`
- [x] `<Down>` — Down one line · `arrow 1`
- [x] `<C-u>` — Up half a page · `arrow -50%`
- [x] `<C-d>` — Down half a page · `arrow 50%`
- [x] `g g` — To the top · `arrow top`
- [x] `G` — To the bottom · `arrow bot`
- [x] `n` — To the next difference · `find_arrow`
- [x] `N` — To the previous difference · `find_arrow --previous`
- [x] `<Enter>` — Compare the files on this row (folders) · `enter`
- [x] `z` — Hide or show the matching rows of a folder comparison · `hide_same`
- [x] `C` — Copy the rows on screen (folders): state<TAB>path · `copy all`
- [x] `<C-+>` — Make everything bigger · `scale in`
- [x] `<C-=>` — Make everything bigger · `scale in`
- [ ] `<C-;>` — Make everything bigger · `scale in`
- [x] `<C-->` — Make everything smaller · `scale out`
- [x] `<C-0>` — Back to the original size · `scale reset`

## `[settings]`

### Settings screen (`<C-,>`)

- [ ] `<Esc>` — Close the settings screen · `close`
- [ ] `<C-,>` — Close the settings screen · `settings`
- [ ] `<C-Tab>` — To the next page · `tab_switch 1 --relative`
- [ ] `<C-PageDown>` — To the next page · `tab_switch 1 --relative`
- [ ] `<C-BackTab>` — To the page before · `tab_switch -1 --relative`
- [ ] `<C-PageUp>` — To the page before · `tab_switch -1 --relative`
- [ ] `<C-f>` — Search the settings · `find`
- [ ] `<C-+>` — Make everything bigger · `scale in`
- [ ] `<C-=>` — Make everything bigger · `scale in`
- [ ] `<C-;>` — Make everything bigger · `scale in`
- [ ] `<C-->` — Make everything smaller · `scale out`
- [ ] `<C-0>` — Back to the original size · `scale reset`
