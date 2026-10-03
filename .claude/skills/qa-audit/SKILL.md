---
name: qa-audit
description: Audit filer's test documentation and turn its checks into automated tests. Use for checking TESTING.md against the source, checking TESTING-KEYS.md against the keymap, or converting a TESTING.md section into `ui::harness::Screen` tests.
---

# QA audit

Three jobs. **Do one per run.** Each ends with a commit on `test/<topic>` and, when
the run produced something worth landing, an open pull request.

Your role and its limits are in [.claude/qa-role.md](../../qa-role.md). Read it
before anything else here. The short version: you write only inside
`#[cfg(test)]` modules, and anything else you find goes in your report (`qa-reports/<date>-<branch>.md`, a file per run; `.claude/qa-role.md`) as a
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
6. **Can the row actually be run, by someone holding only this file?** This is the
   check that found the most on 2026-09-27, when the whole checklist met a real
   machine for the first time. Four rows failed it in four different ways:
   - **15.5** named `<C-S-->`, a chord v0.45.6 had moved -- and said only "the
     hardlink, in its new place", with no way to tell whether a hardlink had been
     made. A hardlink has no marker anywhere; the answer is `fsutil hardlink list`,
     and the row did not say so.
   - **15.2** said `<C-=>` is the one "without shift". True on a US keyboard; on
     JIS `=` is shift+minus. A row that assumes a layout sends half its readers
     looking for a key that is not there.
   - **13** explained how to make a symlink by hand with `mklink`, and never said
     that filer's own `-` and `_` need Developer Mode too. The section had no row
     for either key at all.
   - **18.11** and **36.17** simply disagreed with the code.

   So: does the row name a key that still exists, under the layout the reader has?
   Does it say what state the machine must be in first? And **when it says
   something happened, does it say how to see that it happened?** A row that
   cannot be checked is not a check.
6. **The "what is covered automatically" paragraph** at the top matches what
   `cargo test` actually covers now.

**Write the findings to your report** (`qa-reports/`, never QA-REPORT.md). What you may change in TESTING.md is in
`.claude/qa-role.md` and only there: the note under a section heading, never a
number or a row. Renumbering a section renumbers everything after it — TESTING.md
says so, and it is why 11 is followed by 28.

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
`examples/make-keycheck.rs`. It carries 226 keys and a human's ticks, and a
checklist that has drifted is worse than none: it certifies keys nobody tried.

**The file belongs to its owner and you never write it**, not even by regenerating
it. A tick there means someone pressed the key on a real machine; a run of the
generator is not that, and cannot be. Your part is `--check` and a report.

```bash
cargo run --example make-keycheck -- --check
```

- **exit 0** — in sync. Nothing to do; say so.
- **exit 1** — it prints the keys that were added, removed, or whose description
  changed. **Put that list in your report verbatim** and stop. Running the
  generator without `--check` writes the file, so do not run it that way at all.

`--check` ignores the `[x]` ticks, because those are the owner's. Name in the report
which keys drifted and what the keymap says now, so the regeneration is a one-liner
for whoever owns the file.

CI runs `--check` on every push, so a drift found by hand means CI was not run on
the commit that caused it. Worth a line in the report.

---

## Job 3 — turn a section into `ui::harness::Screen` tests

This is the job with the most left in it. **One section per run, or up to three
when they are neighbours in the same source file** -- section 10 took 168k of a
1M-token budget, so three related sections fit with room to spare, and three
modules appended to one file in one commit is one merge instead of three
conflicting ones. Do not mix a list section with an overlay section in the same
run: they land in different files and share nothing to reuse.

### First, read the section and decide what is reachable

Before writing anything. A section can look automatable in a list of section
numbers and turn out not to be, and finding that out after the tests are written
is the expensive order. Three things put a row out of reach:

- **It needs a real disk, a real machine, or a real program.** Section 44 says so
  in its own preamble -- "what needs a machine is a real disk" -- and a 300k-file
  tree, a junction and a network share are not things a test builds.
- **It needs the terminal.** ConPTY is `#[cfg(windows)]`, so on Linux it is not
  compiled at all, and five of section 38's six rows are about it.
- **It needs a worker.** The harness draws frames; it does not run the scan,
  preview, archive or usage workers. A row whose expectation only appears after a
  worker answers cannot be driven without building that state by hand first.

**Say what you found in your report either way.** A section that turns out to
be two rows deep rather than ten is a finding: it moves the rest back to the
person at the machine honestly, instead of leaving them on a list labelled
"automatable" that nobody has checked. Do not stretch a test to cover a row it
cannot really reach -- a test that asserts something weaker than the row says is
worse than no test, because the row then looks covered.

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
automated and where the test lives — the form sections 2, 10 and 45 already use.
`.claude/qa-role.md` says why this one edit is yours and nothing else in the file
is.

---

## Finishing a run

```bash
cargo test
rustup update stable
cargo +stable clippy --all-targets -- -D warnings
cargo +stable clippy --all-targets --target x86_64-pc-windows-msvc -- -D warnings
git diff                # every added line inside a `#[cfg(test)]` module?
```

Then commit, push to `test/<topic>`, and open a pull request. **Stop there.** Do
not merge, and do not watch it -- `.claude/qa-role.md` says what "stop there"
covers and why. The short of it: the session goes idle at the open pull request,
and everything after that is the merger's.

**Leave `Cargo.toml` and `CHANGELOG.md` alone** -- a PR that bumps the version
conflicts with every other PR that does. Write the CHANGELOG line in the pull
request body instead, in English and ready to paste; whoever merges bumps the
PATCH.

If verification does not pass and you cannot fix it inside a `#[cfg(test)]`
module, `git restore` / `git clean` and write why in your report. A red commit
is worse than no commit.

---

## If you are the one launching a QA session, not running as one

The rest of this file is written for the session doing the work. This last part
is for whoever sends it: the role ends at an open pull request, so **from that
point nobody is on it but you.**

- **Arrange the watch when you launch, not when you remember.** One session per
  watch. CLAUDE.md's QA section has the rule and the two ways it has already gone
  wrong -- including a poller that ran for twenty minutes printing `[$i] $R`
  because the variables never expanded inside a backgrounded subshell. A monitor
  that is quietly broken is worse than none: it looks like coverage.
- **Run the whole suite yourself before merging, green CI or not.** CI runs the
  tests on a Windows runner with an empty temp directory. That is one machine's
  luck, and #25 passed it while holding a test that fails wherever `/tmp` has a
  few thousand files in it.
- **Expect the tail of every shared file to conflict**, because each session
  appends its module there. Do not resolve it by deleting the markers and keeping
  both sides: the closing braces can sit *outside* the conflict, shared, and the
  result will not compile. `git checkout --conflict=merge <file>` puts the markers
  back so the boundaries can be read.

