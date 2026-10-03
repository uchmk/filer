//! Write `TESTING-CHECKS.md`: the manual half of TESTING.md, in Japanese, with
//! a box to tick per check and the commands each section needs.
//!
//! TESTING.md is the source of truth and stays so. It is written in English,
//! from the code, as 46 tables of "Do / Expect" -- excellent for auditing
//! against the source, and awkward to *work through*: there is nowhere to
//! record that a row was done, no way to see how much is left, and the setup a
//! section needs is buried in its prose. This file is the other shape of the
//! same content: one tickable line per check, Japanese first, and each
//! section's preparation lifted out into a block you can paste.
//!
//! **Generated, for the reason TESTING-KEYS.md is generated.** A hand-kept
//! second copy of 516 checks drifts from the first within a week, and the
//! direction it drifts is the dangerous one -- rows that quietly vanish look
//! like rows that were done. So the ids, the section list and the English text
//! all come from TESTING.md on every run; only the Japanese comes from
//! `scripts/testcheck-ja.toml`, and the English is printed beside it so the two
//! can be compared without leaving the line.
//!
//! **Rows `cargo test` already covers are not listed as work.** A test names
//! the check it stands in for in its doc comment (`TESTING.md 45.2`, or
//! `/// 13.4:` at the start of one), and this walks `src/` for those. So the
//! count at the top is what is actually left for a person -- which is the
//! number the checklist exists to make visible, and the one TESTING.md cannot
//! give you.
//!
//! **Ticks survive regeneration**, carried over by check id.
//!
//!     cargo run --example make-testcheck
//!
//! With `--check` it writes nothing and reports whether the file still matches
//! TESTING.md and the tests, exiting 1 if not. That is what CI runs: a check
//! added to TESTING.md and never regenerated here is a check nobody will see.
//!
//! **`--lane linux` writes `TESTING-LINUX.md` instead** -- the same rows, with
//! ticks of its own, for the Linux lane (`.claude/linux-role.md`). A tick in
//! TESTING-CHECKS.md says "seen on a Windows machine", and a Linux run cannot
//! honestly add to that. The Linux file also knows a third mark, `[-]`: the
//! row does not apply on Linux (a UNC share, ConPTY, the recycle bin's
//! Windows half). It is carried over like a tick and counted apart from one.
//!
//! **The Windows file also knows `[~]`** (2026-10-03, the owner's call): an
//! appearance row an agent judged from a screenshot it took. It is carried
//! over like a tick and counted apart from one, and it is **not** done -- a
//! `[x]` still means read as text or a file state. The owner turns a `[~]`
//! into `[x]` after looking at the same picture.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

const SRC: &str = "TESTING.md";
const JA: &str = "scripts/testcheck-ja.toml";
const SRC_DIR: &str = "src";
const OUT_WINDOWS: &str = "TESTING-CHECKS.md";
const OUT_LINUX: &str = "TESTING-LINUX.md";

/// Which machine's checklist this run writes.
#[derive(Clone, Copy, PartialEq)]
enum Lane {
    Windows,
    Linux,
}

impl Lane {
    fn out(self) -> &'static str {
        match self {
            Lane::Windows => OUT_WINDOWS,
            Lane::Linux => OUT_LINUX,
        }
    }
}

/// One row of one of TESTING.md's tables.
struct Check {
    section: u32,
    id: String,
    /// The cells after the id, in order. Two normally (`Do`, `Expect`); three
    /// where the table qualifies them first, as sections 35 and 36 do.
    cells: Vec<String>,
}

struct Section {
    n: u32,
    /// The heading as TESTING.md writes it, version tag and all.
    title: String,
    checks: Vec<Check>,
}

/// What `scripts/testcheck-ja.toml` adds, and the only hand-written input here.
/// A key in the wrong place is silent otherwise: TOML reads a bare key written
/// after the first `[table]` as belonging to that table, so `manual` placed at
/// the foot of this file became `section.46.manual` and did nothing at all.
#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Notes {
    #[serde(default)]
    section: BTreeMap<String, SectionNote>,
    /// Japanese for one check, by id.
    #[serde(default)]
    row: BTreeMap<String, String>,
    /// Ids to keep on the human's list even though a test names them.
    ///
    /// For a test that covers the row on one platform only: `spot_link_section`
    /// is `#[cfg(unix)]`, because making a symlink on Windows is 13.8's
    /// privilege problem, so on the machine this checklist is *for* 13.10 to
    /// 13.12 are not covered at all. Deriving coverage from the tests cannot see
    /// that, and the error is in the direction that matters -- a row taken off
    /// the list is a row nobody runs.
    #[serde(default)]
    manual: Vec<String>,
}

#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct SectionNote {
    /// The Japanese heading. Falls back to TESTING.md's English.
    #[serde(default)]
    title: String,
    /// What to set up first, as a block to paste. Usually PowerShell.
    #[serde(default)]
    setup: String,
    /// A sentence of Japanese context, where the section needs one.
    #[serde(default)]
    note: String,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check = args.iter().any(|a| a == "--check");
    let lane = match args.iter().position(|a| a == "--lane").and_then(|i| args.get(i + 1)).map(String::as_str) {
        None | Some("windows") => Lane::Windows,
        Some("linux") => Lane::Linux,
        Some(other) => panic!("--lane takes `windows` or `linux`, not `{other}`"),
    };
    let out_path = lane.out();
    let sections = parse(&std::fs::read_to_string(SRC).expect("read TESTING.md"));
    let notes: Notes = match std::fs::read_to_string(JA) {
        Ok(t) => toml::from_str(&t).expect("parse the Japanese annotations"),
        // Not an error: without the file this is TESTING.md rearranged, which is
        // still a working checklist. The count of untranslated rows says so.
        Err(_) => Notes::default(),
    };
    let mut automated = automated_ids();
    for id in &notes.manual {
        automated.remove(id);
    }
    let marks = previous_marks(out_path, lane);
    // `done` is what counts as finished: ticked, or (on Linux) not applicable.
    // A `[~]` is a judgement from a picture, waiting on the owner, so it is
    // counted on its own and not as done.
    let done: BTreeSet<String> = marks.iter().filter(|(_, m)| **m != '~').map(|(id, _)| id.clone()).collect();

    let all: Vec<&Check> = sections.iter().flat_map(|s| s.checks.iter()).collect();
    let manual: Vec<&&Check> = all.iter().filter(|c| !automated.contains(&c.id)).collect();
    let ticked = manual.iter().filter(|c| marks.get(&c.id) == Some(&'x')).count();
    let skipped = manual.iter().filter(|c| marks.get(&c.id) == Some(&'-')).count();
    let looked = manual.iter().filter(|c| marks.get(&c.id) == Some(&'~')).count();
    let untranslated = manual.iter().filter(|c| !notes.row.contains_key(&c.id)).count();

    let mut out = String::new();
    preamble(&mut out, lane, &all, &manual, Counts { ticked, skipped, looked, untranslated });

    for s in &sections {
        let mine: Vec<&Check> = s.checks.iter().filter(|c| !automated.contains(&c.id)).collect();
        let auto: Vec<&str> =
            s.checks.iter().filter(|c| automated.contains(&c.id)).map(|c| c.id.as_str()).collect();
        let n = notes.section.get(&s.n.to_string());
        let title = match n.map(|n| n.title.as_str()).unwrap_or_default() {
            "" => s.title.clone(),
            ja => ja.to_owned(),
        };
        let d = mine.iter().filter(|c| done.contains(&c.id)).count();
        let seen = mine.iter().filter(|c| marks.get(&c.id) == Some(&'~')).count();
        let seen = if seen > 0 { format!("（画像で {seen}）") } else { String::new() };

        if mine.is_empty() {
            // Every row automated. Say so and move on: a heading with nothing
            // under it reads like a section someone forgot to fill in.
            writeln!(out, "\n## {}. {title} — 全 {} 件が自動\n", s.n, s.checks.len()).unwrap();
            writeln!(out, "`cargo test` が全部見ているので、押すものはありません。").unwrap();
            continue;
        }
        writeln!(out, "\n## {}. {title} — {d} / {}{seen}\n", s.n, mine.len()).unwrap();
        if let Some(note) = n.map(|n| n.note.trim()).filter(|s| !s.is_empty()) {
            writeln!(out, "{note}\n").unwrap();
        }
        if !auto.is_empty() {
            writeln!(out, "自動テスト済みなので下には出していない: {}\n", auto.join(", ")).unwrap();
        }
        // The setup blocks are PowerShell against the Windows fixtures; on the
        // Linux lane they would be instructions that cannot be followed.
        if let Some(setup) = n.map(|n| n.setup.trim()).filter(|s| !s.is_empty() && lane == Lane::Windows) {
            writeln!(out, "準備:\n\n{setup}\n").unwrap();
        }
        for c in mine {
            let mark = marks.get(&c.id).copied().unwrap_or(' ');
            let en = c.cells.join(" → ");
            match notes.row.get(&c.id) {
                Some(ja) => writeln!(out, "- [{mark}] **{}** {ja} — *{en}*", c.id).unwrap(),
                None => writeln!(out, "- [{mark}] **{}** {en} 〔未訳〕", c.id).unwrap(),
            }
        }
    }

    // A tick against an id TESTING.md no longer has is worth seeing: the row may
    // have been removed on purpose, or renumbered, and renumbering silently
    // moves a tick onto a different check -- which is the one failure this file
    // must not have.
    let live: BTreeSet<&str> = all.iter().map(|c| c.id.as_str()).collect();
    let orphans: Vec<&String> = marks.keys().filter(|id| !live.contains(id.as_str())).collect();
    if !orphans.is_empty() {
        writeln!(out, "\n## 済みだが TESTING.md に無い項目\n").unwrap();
        writeln!(
            out,
            "消えたか、番号が振り直されたか。**番号が動いた場合、上のチェックは別の項目に\n\
             付いている**ので、その節はもう一度通すこと。\n"
        )
        .unwrap();
        for o in orphans {
            writeln!(out, "- `{o}`").unwrap();
        }
    }

    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    // The separator between the Japanese and the English is how a later
    // `--check` tells them apart, so a gloss containing it would make that row
    // read as drifted forever.
    for (id, ja) in &notes.row {
        assert!(
            !ja.contains(" — *"),
            "the Japanese for {id} contains ` — *`, which separates it from the English",
        );
    }
    // An empty cell reads as an ordinary row with a short description, so
    // nothing about the file would say the parse lost one. Refuse to write it.
    for c in &all {
        assert!(
            c.cells.iter().all(|cell| !cell.trim().is_empty()),
            "check {} came out with an empty cell: the table parse dropped one",
            c.id,
        );
    }

    if check {
        // Byte comparison, and tick-insensitive for free: `out` carried the
        // existing ticks forward, so a tick can never be what differs.
        let old = std::fs::read_to_string(out_path).unwrap_or_default();
        if old == out {
            println!(
                "{out_path}: in sync with {SRC} ({ticked} / {} checked, {looked} looked at, {untranslated} untranslated)",
                manual.len(),
            );
            return;
        }
        report_drift(&all, &automated, &old);
        let flag = if lane == Lane::Linux { " -- --lane linux" } else { "" };
        eprintln!(
            "\n{out_path} is out of date. Regenerate it, read the diff, and commit it:\n\n    \
             cargo run --example make-testcheck{flag}\n"
        );
        std::process::exit(1);
    }
    std::fs::write(out_path, out).expect("write the checklist");
    println!(
        "{out_path}: {ticked} / {} checked, {looked} looked at ({} automated, {untranslated} untranslated)",
        manual.len(),
        all.len() - manual.len(),
    );
}

/// How the rows on the list stand, for the paragraph at the top.
struct Counts {
    ticked: usize,
    skipped: usize,
    looked: usize,
    untranslated: usize,
}

fn preamble(out: &mut String, lane: Lane, all: &[&Check], manual: &[&&Check], counts: Counts) {
    let Counts { ticked, skipped, looked, untranslated } = counts;
    let flag = if lane == Lane::Linux { " -- --lane linux" } else { "" };
    writeln!(out, "{}", match lane {
        Lane::Windows => "# 実機チェックリスト",
        Lane::Linux => "# Linux チェックリスト",
    })
    .unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "`{SRC}` から `cargo run --example make-testcheck{flag}` で生成している。\
         **正は {SRC}**（英語）で、\nこのファイルはそれを日本語で並べ替えたもの。\
         食い違ったら {SRC} を信じること。各行の\n末尾の *斜体* が {SRC} の原文で、\
         訳はその手前にある。\n\n\
         **チェック（`[x]`）だけは手で書いてよく、生成し直しても残る。**\
         それ以外を書き換えても次の\n生成で消える。"
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "**{ticked} / {} 済み。**（{SRC} の全 {} 件のうち、`cargo test` が見ている {} 件は\n\
         「押すもの」から外してある）",
        manual.len(),
        all.len(),
        all.len() - manual.len(),
    )
    .unwrap();
    if skipped > 0 {
        writeln!(out, "\nほかに {skipped} 件が `[-]`（Linux では対象外）。").unwrap();
    }
    if looked > 0 {
        writeln!(
            out,
            "\nほかに {looked} 件が `[~]`（Agent が画像で見て判断した。持ち主が同じ画像を見て `[x]` にするまで済みに数えない）。"
        )
        .unwrap();
    }
    if untranslated > 0 {
        writeln!(out, "\n未訳 {untranslated} 件は原文のまま `〔未訳〕` を付けて出している。").unwrap();
    }
    if lane == Lane::Linux {
        writeln!(
            out,
            "\n## 使い方\n\n\
             **このファイルの印は Linux で確かめたもの**で、[TESTING-CHECKS.md](TESTING-CHECKS.md)\
             （Windows 実機）とは別に\n数える。書き方と規則は `.claude/linux-role.md`。\n\n\
             - `[x]`: Linux（X11、Xvfb と CPU 描画）で操作し、期待値を**読めるテキストかファイルの状態**で確かめた。\n\
             - `[-]`: Linux では対象外（UNC、ConPTY、ごみ箱の Windows 側など）。理由は PR に書く。\n\
             - 見た目の行（色、滑らかさ、フォント）は付けない。CPU 描画では実機の代わりにならない。"
        )
        .unwrap();
        return;
    }
    writeln!(
        out,
        "\n## 使い方\n\n\
         1. `filer.exe` と、`scripts\\make-fixtures.ps1` が作るテスト用ファイルを用意する\
         （詳しくは {SRC} の\n   「What you need」）。\n\
         2. 節ごとに「準備」を走らせてから、上から押していく。\n\
         3. 期待どおりなら `[ ]` を `[x]` にする。違ったら `<F12>` で issue を出すか、\
         そのまま書き留める。\n\
         `[~]` は、見た目の行を Agent が画面の画像で判断したもの（証拠の画像は PR と QA-REPORT.md にある）。\
         同じ画像を見て正しければ `[x]` に変える。\n\
         4. 節の見出しの `3 / 12` は、その節で人が押す分の進捗。\n\n\
         キーの網羅は別ファイル（[TESTING-KEYS.md](TESTING-KEYS.md)）で、\
         こちらは「1 つのキーでは\n確かめられない振る舞い」の側。"
    )
    .unwrap();
}

/// TESTING.md's sections and the rows of their tables.
///
/// Only two shapes matter: `## N. Title` starts a section, and a line whose
/// first cell is `N.M` is a check. Everything else is prose, which this steps
/// over -- the prose is where the section's context lives, and it is carried
/// into the output by hand, through `scripts/testcheck-ja.toml`, because it is
/// the part that needs judgement rather than transcription.
fn parse(text: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            if let Some((num, title)) = rest.split_once(". ") {
                if let Ok(n) = num.parse::<u32>() {
                    out.push(Section { n, title: title.trim().to_owned(), checks: Vec::new() });
                    continue;
                }
            }
            continue;
        }
        let Some(row) = line.strip_prefix("| ") else { continue };
        let mut cells = row.trim_end().trim_end_matches('|').split(" | ").map(str::trim);
        let Some(id) = cells.next() else { continue };
        if !is_check_id(id) {
            continue; // the `| # | Do | Expect |` header, or the `| --- |` rule
        }
        let cells: Vec<String> = cells.map(str::to_owned).collect();
        // A section heading always precedes a table in this file; a row without
        // one would mean the heading did not parse, and silently dropping it is
        // how a whole section goes missing.
        let s = out.last_mut().unwrap_or_else(|| panic!("check {id} sits before any `## N.` heading"));
        s.checks.push(Check { section: s.n, id: id.to_owned(), cells });
    }
    // The id must name its own section. TESTING.md warns about this itself: a
    // row numbered `13.2` inside section 12 is the failure mode, and it would
    // land here as a tick on the wrong line.
    for s in &out {
        for c in &s.checks {
            let (sec, _) = c.id.split_once('.').expect("an id has a dot");
            assert_eq!(
                sec.parse::<u32>().ok(),
                Some(c.section),
                "check {} is inside section {} ({}): one of the two is wrong in {SRC}",
                c.id,
                s.n,
                s.title,
            );
        }
    }
    out
}

/// `13.4`, or `11.9b` -- a section number, a dot, a count, and an optional
/// letter for a row inserted after the fact.
fn is_check_id(s: &str) -> bool {
    let Some((sec, rest)) = s.split_once('.') else { return false };
    if sec.is_empty() || !sec.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let digits = rest.trim_end_matches(|c: char| c.is_ascii_lowercase());
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// The check ids some test claims to stand in for.
///
/// Read from the tests rather than from TESTING.md's own prose, which says
/// things like "all eight are automated" and "the footer half of 45.7" -- true,
/// and not something to parse. A test naming its id is a fact about the code,
/// so this number can only be wrong in the safe direction: a test that forgot
/// to say leaves its row on the human's list, which costs one redundant check.
/// The reverse -- a row marked done because prose said so -- is the one that
/// certifies something nobody ran.
///
/// Two spellings are accepted because both are in the tree: `TESTING.md 45.2`
/// inside a sentence, and `/// 13.4:` opening the comment.
///
/// **Only a doc comment that actually precedes a `#[test]` counts.** Production
/// code cites TESTING.md too -- `src/spot.rs` explains a `read_link` branch by
/// naming 13.7, a row that is emphatically *not* automated -- and counting that
/// as coverage takes a row off the human's list on the strength of a comment
/// about why the code is shaped the way it is.
fn automated_ids() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut files = Vec::new();
    collect_rs(Path::new(SRC_DIR), &mut files);
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        let mut pending = BTreeSet::new();
        let mut is_test = false;
        for line in text.lines() {
            let t = line.trim_start();
            if let Some(comment) = t.strip_prefix("///") {
                if let Some(at) = comment.find("TESTING.md ") {
                    ids_in(&comment[at + "TESTING.md ".len()..], &mut pending);
                }
                // `/// 13.4: …` and `/// 6.1, 6.2, …`: ids up to the first
                // colon or dash, which also has to be *present*. Without that
                // requirement a doc comment that merely wraps onto an id reads
                // as a claim about it -- and the sentence it wrapped from said
                // "12.9 to 12.12 stay with the machine", so the rows were taken
                // off the human's list by the words putting them on it.
                let head = comment.trim_start();
                if head.starts_with(|c: char| c.is_ascii_digit()) {
                    if let Some(end) = head.find([':', '-', '—']) {
                        ids_in(&head[..end], &mut pending);
                    }
                }
                continue;
            }
            if t.starts_with("#[") {
                // `#[cfg(test)]` counts as well as `#[test]`, on purpose: a
                // module's own doc comment is where a claim over several rows
                // belongs, and `diff_frame` makes one ("TESTING.md 45.1, 45.5
                // and 45.7") that no single test inside it could.
                is_test |= t.contains("test]") || t.contains("test)");
                continue;
            }
            if t.is_empty() || t.starts_with("//") {
                continue; // a blank line or a `//` aside inside the block
            }
            // Anything else ends the run of comments and attributes. What they
            // were attached to is now known.
            if is_test {
                out.append(&mut pending);
            }
            pending.clear();
            is_test = false;
        }
    }
    out
}

/// Every check id in `s`, stopping at the first word that is not one, a
/// separator, or a word joining two ("and", "to", "through").
fn ids_in(s: &str, out: &mut BTreeSet<String>) {
    for word in s.split([' ', ',', '/', '(', ')', '–', '—']).map(|w| w.trim_end_matches('.')) {
        if word.is_empty() || matches!(word, "and" | "to" | "through" | "&") {
            continue;
        }
        if is_check_id(word) {
            out.insert(word.to_owned());
        } else {
            return; // the prose has started
        }
    }
}

fn collect_rs(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_rs(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The marks already in the file on disk, by id: `x` for done, on the Linux
/// lane `-` for not applicable, and on the Windows lane `~` for an appearance
/// row judged from a screenshot. A `[-]` written into the Windows file is not
/// carried over -- every row there applies, so it would only hide one -- and a
/// `[~]` in the Linux file is not either: CPU rendering is no stand-in for a
/// machine's own drawing.
fn previous_marks(path: &str, lane: Lane) -> BTreeMap<String, char> {
    let Ok(text) = std::fs::read_to_string(path) else { return BTreeMap::new() };
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let (mark, rest) = if let Some(rest) = line.strip_prefix("- [x] **") {
            ('x', rest)
        } else if let (Some(rest), Lane::Linux) = (line.strip_prefix("- [-] **"), lane) {
            ('-', rest)
        } else if let (Some(rest), Lane::Windows) = (line.strip_prefix("- [~] **"), lane) {
            ('~', rest)
        } else {
            continue;
        };
        if let Some(end) = rest.find("**") {
            out.insert(rest[..end].to_owned(), mark);
        }
    }
    out
}

/// Name what makes the file stale: which checks arrived, left, changed wording,
/// or became automated. The exit code alone says "regenerate", which anyone can
/// guess; what is worth printing is which rows moved, because a row that
/// vanished may have been removed on purpose or lost in an edit, and from here
/// the two look identical.
fn report_drift(all: &[&Check], automated: &BTreeSet<String>, old: &str) {
    let now: BTreeMap<&str, String> =
        all.iter().map(|c| (c.id.as_str(), c.cells.join(" → "))).collect();
    let was = rows_of_file(old);
    let mut quiet = true;

    for (id, en) in &now {
        let auto = automated.contains(*id);
        match was.get(*id) {
            None if auto => {}  // automated rows are not listed, so absence is right
            None => {
                println!("+ {id} — {en}");
                quiet = false;
            }
            Some(_) if auto => {
                println!("* {id} — 自動テストが覆ったので一覧から外れる");
                quiet = false;
            }
            Some(before) if before != en => {
                println!("~ {id}\n    was: {before}\n    now: {en}");
                quiet = false;
            }
            Some(_) => {}
        }
    }
    for id in was.keys().filter(|id| !now.contains_key(**id)) {
        println!("- {id} (gone from {SRC})");
        quiet = false;
    }
    if quiet {
        // Every row matches, so what differs is the prose, a count in a
        // heading, or a Japanese gloss -- this file or the annotations, not
        // TESTING.md.
        println!("The checks all match; the difference is in the surrounding text or the Japanese.");
    }
}

/// The rows read back out of the generated file: id to the English that follows
/// it, which is the part [`report_drift`] can compare against TESTING.md.
fn rows_of_file(text: &str) -> BTreeMap<&str, String> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let Some(rest) = line
            .strip_prefix("- [x] **")
            .or_else(|| line.strip_prefix("- [ ] **"))
            .or_else(|| line.strip_prefix("- [-] **"))
            .or_else(|| line.strip_prefix("- [~] **"))
        else {
            continue;
        };
        let Some((id, body)) = rest.split_once("** ") else { continue };
        // The marker is checked first, and the split is from the *front*.
        // Splitting from the back found ` — *` inside an untranslated row's own
        // English -- 1.5 reads "list — **and the shell is still there**" -- and
        // reported a drift on a row nobody had touched. `main` refuses to write
        // a gloss containing the separator, which is what makes the front split
        // safe.
        let en = match body.strip_suffix(" 〔未訳〕") {
            Some(en) => en.to_owned(),
            None => match body.split_once(" — *") {
                // Exactly one `*`: the italic's closer. An English that itself
                // ends in bold ends the line in `***`, and trimming them all
                // took the bold's closer too -- every such row then read as
                // drifted in any failing `--check`, burying the real change.
                Some((_, en)) => en.strip_suffix('*').unwrap_or(en).to_owned(),
                None => continue, // not a row this generator wrote
            },
        };
        out.insert(id, en);
    }
    out
}
