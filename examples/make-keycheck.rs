//! Write `TESTING-KEYS.md`: one tickable line per key binding.
//!
//! There are 193 of them across nine layers, which is more than anyone can
//! hold in their head while working through them one at a time — the checklist
//! exists because losing your place is the normal outcome, not a lapse.
//!
//! Generated rather than written because a hand-kept list of 193 keys drifts
//! from the keymap within a week, and a checklist that is quietly wrong is
//! worse than none: it certifies keys nobody tried.
//!
//! **Ticks survive regeneration.** The old file is read first and every `[x]`
//! is carried over by layer and key, so adding a binding does not cost you the
//! afternoon you already spent. Keys that have gone from the keymap are
//! listed at the end rather than dropped silently, since a key that vanished
//! is worth noticing.
//!
//!     cargo run --example make-keycheck
//!
//! With `--check` it writes nothing and instead reports whether the file still
//! matches the keymap, exiting 1 if it does not. That is what CI runs: a keymap
//! change with no regeneration leaves a checklist that certifies keys nobody
//! tried, which is worse than having none.
//!
//! **No counts in the file.** Every pull request that ticked a key rewrote the
//! total and its layer's heading, so two open at once always conflicted on
//! lines no tick was on. With `--stats` it prints them instead.
//!
//!     cargo run --example make-keycheck -- --stats

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;

const KEYMAP: &str = "src/config/defaults/keymap.toml";
const OUT: &str = "TESTING-KEYS.md";

struct Binding {
    layer: String,
    section: String,
    on: String,
    run: String,
    desc: String,
}

fn main() {
    let check = std::env::args().skip(1).any(|a| a == "--check");
    let stats = std::env::args().skip(1).any(|a| a == "--stats");
    let text = std::fs::read_to_string(KEYMAP).expect("read the default keymap");
    let bindings = parse(&text);
    let done = previous_ticks();

    let mut out = String::new();
    let ticked = bindings.iter().filter(|b| done.contains(&key_of(b))).count();

    writeln!(out, "# Key checklist").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "Generated from `{KEYMAP}` by `cargo run --example make-keycheck`. Ticks are kept\n\
         across regenerations, so re-running after a keymap change costs nothing already done.\n\
         Edit the ticks, not the rows: anything else here is overwritten."
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "{} keys. How many are checked is not written here, so that two pull requests ticking\n\
         keys do not conflict over a total: `cargo run --example make-keycheck -- --stats`.",
        bindings.len()
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "A key is checked when it did what the description says _and_ did nothing else —\n\
         `<A-m>` once ran its own command and the unmodified `m` as well, and both halves\n\
         looked correct on their own. Anything surprising goes in an issue (`<F12>`)."
    )
    .unwrap();

    let mut layer = String::new();
    let mut section = String::new();
    let mut progress = vec![format!("{ticked} / {} checked", bindings.len())];
    for b in &bindings {
        if b.layer != layer {
            layer = b.layer.clone();
            section.clear();
            let n = bindings.iter().filter(|x| x.layer == layer).count();
            let d = bindings
                .iter()
                .filter(|x| x.layer == layer && done.contains(&key_of(x)))
                .count();
            progress.push(format!("[{layer}] {d} / {n}"));
            writeln!(out, "\n## `[{layer}]`\n").unwrap();
            writeln!(out, "{}\n", layer_note(&layer)).unwrap();
        }
        if b.section != section {
            section = b.section.clone();
            if !section.is_empty() {
                writeln!(out, "\n### {section}\n").unwrap();
            }
        }
        let mark = if done.contains(&key_of(b)) { "x" } else { " " };
        writeln!(out, "- [{mark}] `{}` — {} · `{}`", b.on, b.desc, b.run).unwrap();
    }

    // A tick for a key that is no longer bound is worth seeing rather than
    // discarding: either the key was removed on purpose, or the keymap lost
    // something it should not have.
    let live: HashSet<String> = bindings.iter().map(key_of).collect();
    let orphans: Vec<&String> = done.iter().filter(|k| !live.contains(*k)).collect();
    if !orphans.is_empty() {
        writeln!(out, "\n## Checked, but no longer in the keymap\n").unwrap();
        writeln!(
            out,
            "Removed on purpose, or removed by accident. Worth a look either way.\n"
        )
        .unwrap();
        for o in orphans {
            writeln!(out, "- `{o}`").unwrap();
        }
    }

    // Written the way a Markdown formatter leaves it, so the two do not undo
    // each other: the ticks are edited by hand in an editor that reformats on
    // save, and a generator whose output differs cosmetically turns every
    // regeneration into a diff nobody can read. `_and_` above is the same
    // bargain — a formatter rewrites `*and*` to it.
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    // A row with no key on it is the parser having lost one, not the keymap
    // having a nameless binding — and it reads as an ordinary checklist line,
    // so nothing about the file says anything went wrong. Refuse to write it.
    for b in &bindings {
        assert!(
            !b.on.trim().is_empty(),
            "`{}` in layer `{}` came out with no key: the `on = ` parse dropped it",
            b.run,
            b.layer,
        );
    }
    if stats {
        println!("{OUT}:");
        for line in &progress {
            println!("  {line}");
        }
        return;
    }
    if check {
        // A byte comparison is the whole test, and it is tick-insensitive for
        // free: `out` carried the existing ticks forward, so a tick can never
        // be what differs. Anything that does differ came from the keymap.
        let old = std::fs::read_to_string(OUT).unwrap_or_default();
        if old == out {
            println!("{OUT}: in sync with {KEYMAP} ({ticked} / {} checked)", bindings.len());
            return;
        }
        report_drift(&bindings, &old);
        eprintln!(
            "\n{OUT} is out of date. Regenerate it, read the diff, and commit it:\n\n    \
             cargo run --example make-keycheck\n"
        );
        std::process::exit(1);
    }
    std::fs::write(OUT, out).expect("write the checklist");
    println!("{OUT}: {ticked} / {} checked", bindings.len());
}

/// Name the keys behind an out-of-date checklist: added, gone, or re-described.
///
/// The exit code alone would say "regenerate", which anyone can guess. What is
/// worth printing is *which* bindings moved, because that is the review: a key
/// that vanished may have been removed on purpose or lost by accident, and the
/// two look identical from here.
fn report_drift(bindings: &[Binding], old: &str) {
    let now = rows_of_generated(bindings);
    let was = rows_of_file(old);

    let mut quiet = true;
    for (key, (desc, run)) in &now {
        match was.get(key) {
            None => {
                println!("+ {key} — {desc} · {run}");
                quiet = false;
            }
            Some((d, r)) if (d, r) != (desc, run) => {
                println!("~ {key}\n    was: {d} · {r}\n    now: {desc} · {run}");
                quiet = false;
            }
            Some(_) => {}
        }
    }
    for key in was.keys().filter(|k| !now.contains_key(*k)) {
        println!("- {key} (gone from the keymap)");
        quiet = false;
    }
    if quiet {
        // Every binding matches, so what differs is the prose -- a wording
        // change in this file, not a keymap change.
        println!("The bindings all match; the difference is in the surrounding text.");
    }
}

/// The rows this run would write, keyed the way [`key_of`] keys them.
fn rows_of_generated(bindings: &[Binding]) -> BTreeMap<String, (String, String)> {
    bindings.iter().map(|b| (key_of(b), (b.desc.clone(), b.run.clone()))).collect()
}

/// The same rows read back out of the file, parsed from the line `main` writes.
fn rows_of_file(text: &str) -> BTreeMap<String, (String, String)> {
    let mut out = BTreeMap::new();
    let mut layer = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## `[") {
            if let Some(end) = rest.find("]`") {
                layer = rest[..end].to_owned();
            }
            continue;
        }
        let Some(rest) = line.strip_prefix("- [x] `").or_else(|| line.strip_prefix("- [ ] `"))
        else {
            continue; // prose, a heading, or the orphan list
        };
        let Some((on, rest)) = rest.split_once("` \u{2014} ") else { continue };
        // The command is the last backquoted word, so a description holding a
        // ` \u{b7} ` of its own does not steal the split.
        let Some((desc, run)) = rest.rsplit_once(" \u{b7} `") else { continue };
        out.insert(
            format!("{layer}/{on}"),
            (desc.to_owned(), run.trim_end_matches('`').to_owned()),
        );
    }
    out
}

fn key_of(b: &Binding) -> String {
    format!("{}/{}", b.layer, b.on)
}

/// The ticks from the file as it stands, by layer and key.
fn previous_ticks() -> HashSet<String> {
    let mut out = HashSet::new();
    let Ok(text) = std::fs::read_to_string(OUT) else {
        return out; // first run
    };
    let mut layer = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## `[") {
            if let Some(end) = rest.find("]`") {
                layer = rest[..end].to_owned();
            }
        } else if let Some(rest) = line.strip_prefix("- [x] `") {
            if let Some(end) = rest.find('`') {
                out.insert(format!("{layer}/{}", &rest[..end]));
            }
        } else if let Some(rest) = line.strip_prefix("- `") {
            // From the orphan list: keep carrying it so it is not lost on the
            // next run either.
            if let Some(end) = rest.find('`') {
                out.insert(rest[..end].to_owned());
            }
        }
    }
    out
}

/// A line parser rather than a TOML one, because the `# --- Section ---`
/// comments are the grouping and a TOML parse throws comments away.
fn parse(text: &str) -> Vec<Binding> {
    let mut out: Vec<Binding> = Vec::new();
    let mut section = String::new();
    let mut cur: Option<Binding> = None;

    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("# --- ") {
            section = rest.trim_end_matches(['-', ' ']).trim().to_owned();
        } else if let Some(rest) = t.strip_prefix("[[") {
            if let Some(end) = rest.find(".keymap]]") {
                if let Some(b) = cur.take() {
                    out.push(b);
                }
                cur = Some(Binding {
                    layer: rest[..end].to_owned(),
                    // Sections belong to the layer they were written in.
                    section: section.clone(),
                    on: String::new(),
                    run: String::new(),
                    desc: String::new(),
                });
            }
        } else if let Some(b) = cur.as_mut() {
            if let Some(v) = t.strip_prefix("on = ") {
                b.on = keys(v);
            } else if let Some(v) = t.strip_prefix("run = ") {
                b.run = unquote(v);
            } else if let Some(v) = t.strip_prefix("desc = ") {
                b.desc = unquote(v);
            }
        }
    }
    if let Some(b) = cur.take() {
        out.push(b);
    }
    out
}

/// `"k"` or `[ "g", "c" ]` — a chord is written the way it is pressed.
///
/// The split has to honour the quotes. `[ ",", "b" ]` is the sort prefix and
/// then `b`, and cutting the array text at every comma slices that first
/// element in half: the two halves unquote to nothing, and the chord comes out
/// as two spaces with a key after them. That is how eleven sorting keys came
/// to be listed with no key on them at all.
fn keys(v: &str) -> String {
    let v = v.trim();
    let Some(inner) = v.strip_prefix('[').and_then(|s| s.strip_suffix(']')) else {
        return unquote(v);
    };
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for c in inner.chars() {
        match c {
            _ if escaped => {
                cur.push(c);
                escaped = false;
            }
            '\\' if quoted => {
                cur.push(c);
                escaped = true;
            }
            '"' => {
                cur.push(c);
                quoted = !quoted;
            }
            ',' if !quoted => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    parts.push(cur);
    // A trailing comma leaves an empty tail; a key itself is never blank.
    parts.iter().filter(|p| !p.trim().is_empty()).map(|p| unquote(p)).collect::<Vec<_>>().join(" ")
}

/// Strip the quotes and undo TOML's escapes.
///
/// Trimming the quotes alone is not enough: a `desc` containing a Windows path
/// separator is written `\\` in the file and would reach the checklist with
/// both backslashes still on it.
fn unquote(v: &str) -> String {
    let inner = v.trim().trim_matches('"');
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            // `\\` and `\"` stand for themselves; anything else is left as it
            // was rather than guessed at.
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// What the layer is, in one line, since "why is `j` listed nine times" is the
/// first question the file raises.
fn layer_note(layer: &str) -> &'static str {
    match layer {
        "mgr" => "The file list: what is in front of you unless an overlay is.",
        "input" => "The one-line prompt — `cd`, rename, filter, search.",
        "spot" => "The details panel (`<Tab>`).",
        "term" => "While the terminal pane holds the keys. Everything not listed \
                   here goes to the shell.",
        "diff" => "The side-by-side comparison (`<A-d>`).",
        "tasks" => "The task manager (`w`).",
        "pick" => "A chooser — the command palette, the context menu.",
        "confirm" => "A yes/no prompt.",
        "help" => "This panel (`~` or `<F1>`).",
        _ => "",
    }
}
