# The three screenshots the README is waiting for

A file manager is a thing you look at. The README is 1400 lines and, until these
exist, not one of them shows what the program puts on a screen — so a visitor has
to take the prose on faith and decide from it whether to download a binary. Most
will not.

The markdown for all three is already in README.md, commented out, immediately
after the `cargo run` line. Save the files here under the names below and delete
the comment markers around that block. Nothing else needs editing.

Capture at **1600×1000 or larger**, on the dark theme, with the window filling
the frame and no desktop behind it.

---

## `screenshot-main.png` — what the program is

The three columns with a text file under the cursor and its preview alongside.
Choose a directory with enough in it to look real: a source tree beats
`filer-fixtures`, because a reader recognises a source tree and cannot tell
whether fixtures are a demo.

Make sure the frame includes, because each answers a question the prose cannot:

- the **header**, so the path and the counts are visible
- the **status bar**, for the mode indicator and the size and date of the file
- **git marks** in the listing, if the directory is a repository — this is one of
  the things people notice and ask about
- the **preview with syntax colours**, not a blank or plain-text file

## `screenshot-keys.gif` — why it is worth using

**This is the one that matters.** The program's whole argument is that the
keyboard is faster, and a still frame cannot make that argument: it shows a
layout, not a way of working. A short loop showing hands-free movement does more
than any paragraph here.

Five to eight seconds, looping, no cursor:

1. `j` `j` `j` down the list, the preview keeping up with each row
2. `l` into a directory, `h` back out
3. a chord half-typed — `g` on its own — so the **which-key panel** appears and
   names what `g` could still become
4. finish it (`gg`, say) so the panel resolves into the jump
5. `<Tab>` for the spot panel, then `<Esc>`

Keep it under about 4 MB. A GIF that takes a moment to load is a GIF nobody
watches to the end.

## `screenshot-preview.png` — what it does that others do not

Whichever of these reads best on your machine:

- a **CSV as an aligned table**, which is the one people do not expect
- a **long source file with the minimap** down the right of the pane
- an **image preview**, zoomed 1:1 so the detail is visibly sharp

One frame, one idea. If the CSV table and the minimap are both on screen the
reader will not know which they were meant to notice.

---

## Two things to check before committing them

**Nothing private in the frame.** A path with a real name in it, a directory of
work files, a git branch naming something internal — these are easy to miss while
concentrating on the layout, and they are permanent once pushed. Look at the whole
frame, not just the part being demonstrated.

**The file sizes.** Three images are all a reader downloads before they have
decided they care. PNGs from a screenshot tool are usually two or three times
larger than they need to be; run them through any optimiser before committing.
