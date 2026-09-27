---
name: qa-audit
description: Audit filer's test documentation and turn its checks into automated tests. Use for checking TESTING.md against the source, checking TESTING-KEYS.md against the keymap, or converting a TESTING.md section into `ui::harness::Screen` tests.
---

# QA audit

Three jobs. **Do one per run.** Each ends with a commit on `test/<topic>` and, when
the run produced something worth landing, an open pull request.

Your role and its limits are in [.claude/qa-role.md](../../qa-role.md). Read it
before anything else here. The short version: you write only inside
`#[cfg(test)]` modules, and anything else you find goes in `QA-REPORT.md` as a
proposal.

---

## Job 1 — TESTING.md against the code

TESTING.md has 45 sections and 495 checks, written from the code rather than from
use. So a row that does not match the program may be the row's mistake. That is
what this job looks for.

Work one section at a time. For each row:

1. **The keys it names** — are they still in `src/config/defaults/keymap.toml`,
   bound to what the row says? A row naming `<A-j>` when the keymap has moved it
   is a row that will waste someone's afternoon.
2. **The words it expects on screen** — grep `src/` for them. `"1 selected"`,
   `"too big to read"`, `"every file matches"` are all real strings in the source;
   a row quoting something that is not there is stale.
3. **The version it is tagged with** — `(v0.31.0)` in a heading should match when
   the feature actually landed, per CHANGELOG.md.

Then the file's own structure:

4. **Section numbers run 1..45 with no gaps**, and each check id is
   `<its section>.<n>`. A check numbered `13.2` inside section 12 is the failure
   mode the file warns about.
5. **No duplicate ids** within a section, counting the `1.9a` suffix form.
6. **The "what is covered automatically" paragraph** at the top matches what
   `cargo test` actually covers now.

**Write the findings to `QA-REPORT.md`; do not edit TESTING.md.** Renumbering a
section renumbers everything after it — TESTING.md says so, and it is why 11 is
followed by 28. Adding a *note* to a section (as v0.45.0 did for sections 2 and
45, recording which checks are now automated) is fine; moving a number is not.

Report format — one row per finding, so it can be worked through:

```markdown
## TESTING.md — section 13 (audited against 9f78db9)

| # | 見つけたこと | 根拠 | どちらの間違いか |
| --- | --- | --- | --- |
| 13.4 | `g` + `f` が keymap に無い | `keymap.toml` に `["g","f"]` の項が無い | TESTING.md（キーは v0.2x で消えた） |
```

---

## Job 2 — TESTING-KEYS.md against the keymap

TESTING-KEYS.md is generated from `src/config/defaults/keymap.toml` by
`examples/make-keycheck.rs`. It carries 207 keys and a human's ticks, and a
checklist that has drifted is worse than none: it certifies keys nobody tried.

```bash
cargo run --example make-keycheck -- --check
```

- **exit 0** — in sync. Nothing to do; say so.
- **exit 1** — it prints the keys that were added, removed, or whose description
  changed. Regenerate and commit:

```bash
cargo run --example make-keycheck      # no argument: writes the file
git diff TESTING-KEYS.md               # read it before committing
```

`--check` ignores the `[x]` ticks, because those are the human's and regeneration
preserves them. If the diff shows a tick moving, something is wrong with the
generator — report it, do not commit it.

CI runs `--check` on every push, so a drift found by hand means CI was not run on
the commit that caused it. Worth a line in the report.

---

## Job 3 — turn a section into `ui::harness::Screen` tests

This is the job with the most left in it. **One section per run.**

### The harness

`ui::harness` (in `src/ui/mod.rs`, `#[cfg(test)]`) runs the real drawing code with
no window and no GPU. One frame costs 0.01s.

```rust
use super::harness::Screen;

let mut s = Screen::open(crate::util::test_dir("my-label"));  // 1280x800
let mut s = Screen::open(dir).sized(1920.0, 1080.0);          // a wider window

let f = s.draw();               // one frame, nothing typed
let f = s.typed("jj");          // one frame, after `j` `j` as egui delivers text
let f = s.feed(vec![...]);      // one frame, after raw `egui::Event`s
let r = s.rect();               // the window, for asking where something was drawn

f.says("2 items")               // does any drawn string contain this
f.rects_in(area)                // every rectangle inside `area`
f.filled(theme.hovered_bg)      // every rectangle in exactly this colour
```

`s.app` is the `App`, so a test sets up state directly and then draws.

### Five things that will bite you

1. **Do not guess the expected string. Draw once and look.** Print
   `{:?}` of `f.texts`, read it, then write the assertion. When v0.45.0's tests
   were written, three guesses out of eight were wrong: the mode indicator says
   **`SELECT`**, not `VISUAL`; **`m` is the line-mode prefix**, not bookmarks; and
   at 1280px the preview pane is **54 columns, under the 56 the minimap needs**, so
   the minimap does not appear at the default size.
2. **The harness does not run the workers.** No scan, no preview job. Build the
   listing yourself with `Folder::from_entries(path, entries, true)` and set
   `s.app.preview.state` directly. This is deliberate: a scan landing mid-test
   would replace what the test just set up.
3. **`util::test_dir("label")` wipes its directory on every call.** Call it once
   and `join` from the result. Never build a temp path by hand — two tests sharing
   one deleted each other's fixtures during a parallel run, and that is what the
   helper exists to prevent.
4. **Text is readable, colour and glyph shape are not.** The harness gives you the
   strings and the rectangles. "It is drawn in the wrong shade of grey" is not
   something it can see; leave that row in TESTING.md.
5. **The minimap and other bands are rectangles, not text.** Assert them by
   counting `rects_in` a strip with the feature on and off, rather than picking an
   absolute threshold.

### Which sections are reachable

5, 6, 9, 10, 11, 12, 13, 18, 20, 21, 24, 27, 33, 34, 36, 38, 42, 43, 44.

Not reachable, because they need the OS, a process, or a hand on a mouse:
1, 3, 8, 14, 15, 19, 22, 23, 25, 26, 29, 30, 31, 32, 35, 37, 39, 40, and 41.8.

### When the section is done

Add a note under the section heading in TESTING.md naming which checks are now
automated and where the test lives — the form sections 2 and 45 already use. That
is the one edit to TESTING.md this role makes, and it moves no numbers.

---

## Finishing a run

```bash
cargo test
rustup update stable
cargo +stable clippy --all-targets -- -D warnings
cargo +stable clippy --all-targets --target x86_64-pc-windows-msvc -- -D warnings
git diff                # every added line inside a `#[cfg(test)]` module?
```

Then bump the PATCH version in `Cargo.toml`, run `cargo build` so `Cargo.lock`
follows, write the CHANGELOG entry, commit, push to `test/<topic>`, and open a
pull request. **Stop there.** Do not merge.

If verification does not pass and you cannot fix it inside a `#[cfg(test)]`
module, `git restore` / `git clean` and write why in `QA-REPORT.md`. A red commit
is worse than no commit.
