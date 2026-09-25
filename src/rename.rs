//! The rules behind bulk rename, and the order the renames have to happen in.
//!
//! All of this is pure: the overlay draws a preview of it on every keystroke, so
//! nothing here may touch the disk. What is already in the directory is passed
//! in from the listing the app is holding anyway.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use crate::util;

/// What the text typed at the prompt means.
#[derive(Clone, Debug)]
pub enum Rule {
    /// A whole new name, with `{name}`, `{ext}` and `{n}` filled in.
    Template(Vec<Part>),
    /// A regular expression over the whole name, sed's spelling.
    Subst { re: fancy_regex::Regex, rep: String, all: bool },
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Part {
    Lit(String),
    /// The name without its extension.
    Stem,
    /// The extension including its dot, or nothing where there is none.
    Ext,
    /// The row's number, 1-based, zero-padded to this width.
    Num(usize),
}

/// Every form the prompt's legend advertises, for a test to hold it to.
///
/// The legend is drawn next to the prompt and is the only place the syntax is
/// stated where it is being typed. A line that promises `{n:3}` after the
/// parser stopped taking it would be worse than no legend at all, so the two
/// are tied together here rather than by remembering.
#[cfg(test)]
pub const LEGEND_EXAMPLES: &[&str] =
    &["{name}{ext}", "{n}", "shot-{n:3}{ext}", "s/a/b/", "s/a/b/g", "s/a/b/i"];

/// Read the prompt. Text starting with `s/` is a substitution, as in sed and
/// vim; anything else is a template, so the common case — typing a new name
/// with `{n}` in it — needs no punctuation.
pub fn parse_rule(text: &str) -> Result<Rule, String> {
    if let Some(rest) = text.strip_prefix("s/") {
        let parts = split_unescaped(rest);
        if parts.len() < 2 {
            return Err("substitution needs s/pattern/replacement/".into());
        }
        let flags = parts.get(2).map(String::as_str).unwrap_or("");
        for f in flags.chars() {
            if f != 'g' && f != 'i' {
                return Err(format!("unknown flag `{f}`; only g and i"));
            }
        }
        let pattern = match flags.contains('i') {
            true => format!("(?i){}", parts[0]),
            false => parts[0].clone(),
        };
        let re = fancy_regex::Regex::new(&pattern).map_err(|e| format!("{e}"))?;
        return Ok(Rule::Subst { re, rep: parts[1].clone(), all: flags.contains('g') });
    }
    Ok(Rule::Template(parse_template(text)?))
}

fn parse_template(text: &str) -> Result<Vec<Part>, String> {
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        lit.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            return Err("a `{` with no `}`".into());
        };
        let body = &after[..close];
        let part = match body {
            "name" => Part::Stem,
            "ext" => Part::Ext,
            "n" => Part::Num(1),
            _ => match body.strip_prefix("n:").map(|w| w.parse::<usize>()) {
                Some(Ok(w)) if w <= 12 => Part::Num(w),
                Some(_) => return Err(format!("`{{{body}}}`: the width must be 1 to 12")),
                None => return Err(format!("unknown `{{{body}}}`; use name, ext or n")),
            },
        };
        if !lit.is_empty() {
            out.push(Part::Lit(std::mem::take(&mut lit)));
        }
        out.push(part);
        rest = &after[close + 1..];
    }
    lit.push_str(rest);
    if !lit.is_empty() {
        out.push(Part::Lit(lit));
    }
    Ok(out)
}

/// Split on `/`, letting `\/` stand for a literal one. At most three fields;
/// the rest of the text belongs to the last, so a replacement may hold a `/`
/// after the closing one is found.
fn split_unescaped(s: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut escaped = false;
    for c in s.chars() {
        match (escaped, c) {
            (true, '/') => {
                out.last_mut().expect("never emptied").push('/');
                escaped = false;
            }
            (true, other) => {
                let cur = out.last_mut().expect("never emptied");
                cur.push('\\');
                cur.push(other);
                escaped = false;
            }
            (false, '\\') => escaped = true,
            (false, '/') if out.len() < 3 => out.push(String::new()),
            (false, other) => out.last_mut().expect("never emptied").push(other),
        }
    }
    if escaped {
        out.last_mut().expect("never emptied").push('\\');
    }
    // A trailing `/` leaves an empty flags field, which is the usual spelling.
    out
}

/// What one file becomes. `problem` is `Some` when the new name cannot be used,
/// which the preview shows and the apply step refuses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub from: PathBuf,
    pub to: String,
    pub problem: Option<String>,
}

/// Work out the new name of every file, and what is wrong with any of them.
///
/// `taken` is the names already in the directory, from the listing in memory —
/// a name that is in the way is only a problem when it belongs to a file that is
/// not itself moving out of it. Being in the selection is not enough: a rule
/// that leaves a file's name alone leaves the name occupied, so the new names
/// are all worked out first and only then judged against each other.
pub fn plan(paths: &[PathBuf], text: &str, taken: &BTreeSet<String>) -> Result<Vec<Row>, String> {
    let rule = parse_rule(text)?;
    let named: Vec<(String, String)> = paths
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let name = util::file_name(p);
            let to = apply(&rule, &name, i + 1);
            (name, to)
        })
        .collect();
    // The names this batch gives up. Only a file that actually moves does.
    let vacated: BTreeSet<&str> =
        named.iter().filter(|(from, to)| from != to).map(|(from, _)| from.as_str()).collect();

    let mut rows: Vec<Row> = Vec::with_capacity(paths.len());
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for (from, (name, to)) in paths.iter().zip(&named) {
        let (name, to) = (name.as_str(), to.clone());
        let problem = if to.is_empty() {
            Some("empty name".into())
        } else if to.contains('/') || to.contains('\\') {
            Some("a name cannot hold a path separator".into())
        } else if to == "." || to == ".." {
            Some("reserved name".into())
        } else if !seen.insert(to.clone()) {
            Some("two files would get this name".into())
        } else if to != name && taken.contains(&to) && !vacated.contains(to.as_str()) {
            Some("already in this directory".into())
        } else {
            None
        };
        rows.push(Row { from: from.clone(), to, problem });
    }
    Ok(rows)
}

fn apply(rule: &Rule, name: &str, n: usize) -> String {
    match rule {
        Rule::Subst { re, rep, all } => match all {
            true => re.replace_all(name, rep.as_str()).into_owned(),
            false => re.replace(name, rep.as_str()).into_owned(),
        },
        Rule::Template(parts) => {
            let (stem, ext) = util::stem_and_ext(name);
            let mut out = String::new();
            for p in parts {
                match p {
                    Part::Lit(s) => out.push_str(s),
                    Part::Stem => out.push_str(stem),
                    Part::Ext => out.push_str(ext),
                    Part::Num(w) => out.push_str(&format!("{n:0w$}")),
                }
            }
            out
        }
    }
}

/// One move in a batch. Renaming `a` to `b` while `b` is itself on its way
/// somewhere else only works in the right order, and two files swapping names
/// have no right order at all — one of them has to step aside first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    /// Move row `i` out of the way, to a name nothing else wants.
    Park(usize),
    /// Move row `i` to the name it is meant to have.
    Rename(usize),
}

/// The order to carry out `rows` in. Rows whose name does not change are left
/// out entirely.
pub fn order(rows: &[(String, String)]) -> Vec<Step> {
    let mut pending: Vec<usize> = (0..rows.len()).filter(|&i| rows[i].0 != rows[i].1).collect();
    // The batch's own files, as long as they are still under their old names.
    let mut live: HashSet<&str> = pending.iter().map(|&i| rows[i].0.as_str()).collect();
    let mut parked: HashSet<usize> = HashSet::new();
    let mut out = Vec::new();

    while !pending.is_empty() {
        match pending.iter().position(|&i| !live.contains(rows[i].1.as_str())) {
            Some(at) => {
                let i = pending.remove(at);
                live.remove(rows[i].0.as_str());
                out.push(Step::Rename(i));
            }
            // Everything left wants a name another of them still holds: a
            // cycle. Moving one aside frees its name and breaks it.
            None => {
                let Some(&i) = pending.iter().find(|i| !parked.contains(i)) else {
                    // Unreachable: parking every row empties `live`, and then
                    // every destination is free. Stopping beats looping.
                    break;
                };
                parked.insert(i);
                live.remove(rows[i].0.as_str());
                out.push(Step::Park(i));
            }
        }
    }
    out
}

/// A name for a file to wait under while a cycle is untangled. Long and dull on
/// purpose: it is only on disk between two renames, and it must not be mistaken
/// for something worth keeping if a crash leaves it there.
pub fn park_name(dir: &Path, i: usize) -> PathBuf {
    dir.join(format!(".filer-bulk-rename-{i}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<PathBuf> {
        v.iter().map(|n| PathBuf::from("/d").join(n)).collect()
    }

    fn plan_of(files: &[&str], text: &str) -> Vec<Row> {
        let taken: BTreeSet<String> = files.iter().map(|s| s.to_string()).collect();
        plan(&names(files), text, &taken).expect("the rule parses")
    }

    #[test]
    fn a_template_numbers_the_rows_and_keeps_the_extension() {
        let rows = plan_of(&["b.txt", "a.md", "plain"], "shot-{n:3}{ext}");

        let got: Vec<&str> = rows.iter().map(|r| r.to.as_str()).collect();
        assert_eq!(got, ["shot-001.txt", "shot-002.md", "shot-003"]);
        assert!(rows.iter().all(|r| r.problem.is_none()));
    }

    #[test]
    fn the_identity_template_changes_nothing() {
        let rows = plan_of(&["a.txt", "noext"], "{name}{ext}");

        assert_eq!(rows[0].to, "a.txt");
        assert_eq!(rows[1].to, "noext");
        assert!(order(&[("a.txt".into(), "a.txt".into())]).is_empty());
    }

    #[test]
    fn a_substitution_rewrites_the_whole_name() {
        let rows = plan_of(&["draft copy.txt", "x copy copy.md"], "s/ copy//g");

        assert_eq!(rows[0].to, "draft.txt");
        assert_eq!(rows[1].to, "x.md");
    }

    #[test]
    fn a_substitution_can_use_groups_flags_and_an_escaped_slash() {
        let rows = plan_of(&["IMG_12.jpg"], r"s/img_(\d+)/photo-$1/i");
        assert_eq!(rows[0].to, "photo-12.jpg");

        let rows = plan_of(&["a-b"], r"s/-/\//");
        assert_eq!(rows[0].to, "a/b");
        assert!(rows[0].problem.is_some(), "a separator in a name is refused");
    }

    #[test]
    fn a_bad_rule_says_what_is_wrong_rather_than_renaming_anything() {
        let taken = BTreeSet::new();
        for (text, wanted) in [
            ("{nope}", "unknown `{nope}`"),
            ("{name", "a `{` with no `}`"),
            ("s/a/b/q", "unknown flag `q`"),
            ("s/only", "substitution needs"),
        ] {
            let err = plan(&names(&["f.txt"]), text, &taken).unwrap_err();
            assert!(err.contains(wanted), "{text}: got {err}");
        }
        // A broken pattern is reported in the regex engine's own words, which
        // say more about it than anything this could add.
        assert!(plan(&names(&["f.txt"]), "s/(/x/", &taken).is_err());
    }

    #[test]
    fn two_files_given_the_same_name_are_both_flagged() {
        let rows = plan_of(&["a.txt", "b.txt"], "same.txt");

        assert!(rows[0].problem.is_none(), "the first one can have it");
        assert_eq!(rows[1].problem.as_deref(), Some("two files would get this name"));
    }

    /// A name already in the directory is in the way unless the file holding it
    /// is moving out of it. Being in the selection is not enough — a rule that
    /// does not match a file leaves that file exactly where it was.
    #[test]
    fn a_name_in_use_is_a_problem_unless_its_owner_actually_moves() {
        let taken: BTreeSet<String> =
            ["a.txt", "b.txt", "bystander.txt"].iter().map(|s| s.to_string()).collect();

        let rows = plan(&names(&["a.txt"]), "bystander.txt", &taken).unwrap();
        assert!(rows[0].problem.is_some(), "nothing frees bystander.txt");

        // `b.txt` is selected, but this rule does not match it, so it keeps its
        // name and `a.txt` cannot have it. Letting this through meant the
        // preview promised a rename that failed at the last moment.
        let rows = plan(&names(&["a.txt", "b.txt"]), "s/a\\.txt/b.txt/", &taken).unwrap();
        assert_eq!(rows[0].to, "b.txt");
        assert_eq!(rows[0].problem.as_deref(), Some("already in this directory"));

        // Now b.txt does move, so its name is going spare and a.txt may take it.
        let rows = plan(&names(&["a.txt", "b.txt"]), "s/([ab])\\.txt/{$1}.txt/", &taken).unwrap();
        assert_eq!(rows[0].to, "{a}.txt");
        let rows = plan(&names(&["a.txt", "b.txt"]), "s/^a/b/", &taken).unwrap();
        assert_eq!(rows[0].to, "b.txt");
        assert_eq!(rows[1].to, "b.txt");
        assert_eq!(rows[1].problem.as_deref(), Some("two files would get this name"));
    }

    /// The one rule shape that produces a true swap, and the reason
    /// [`order`] has to park a file: `ab` and `ba` trading names.
    #[test]
    fn a_rule_that_swaps_two_names_is_allowed_and_ordered() {
        let taken: BTreeSet<String> = ["ab.txt", "ba.txt"].iter().map(|s| s.to_string()).collect();

        let rows = plan(&names(&["ab.txt", "ba.txt"]), r"s/^([ab])([ab])/$2$1/", &taken).unwrap();

        assert_eq!(rows[0].to, "ba.txt");
        assert_eq!(rows[1].to, "ab.txt");
        assert!(rows.iter().all(|r| r.problem.is_none()), "each frees what the other wants");

        let pairs: Vec<(String, String)> =
            rows.iter().map(|r| (util::file_name(&r.from), r.to.clone())).collect();
        let steps = order(&pairs);
        assert_eq!(steps.iter().filter(|s| matches!(s, Step::Park(_))).count(), 1);
    }

    #[test]
    fn a_rename_into_a_name_the_batch_frees_happens_second() {
        // a -> b, b -> c: `b` has to get out of the way before `a` arrives.
        let rows = vec![
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "c".to_string()),
        ];

        assert_eq!(order(&rows), [Step::Rename(1), Step::Rename(0)]);
    }

    #[test]
    fn two_files_swapping_names_park_one_of_them_first() {
        let rows = vec![
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "a".to_string()),
        ];

        let steps = order(&rows);
        assert_eq!(steps.len(), 3, "one park and two renames: {steps:?}");
        assert_eq!(steps[0], Step::Park(0));
        assert!(steps.contains(&Step::Rename(0)) && steps.contains(&Step::Rename(1)));
    }

    /// Three files rotating is the same problem one turn further round.
    #[test]
    fn a_longer_cycle_still_comes_out_with_every_file_renamed_once() {
        let rows = vec![
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "c".to_string()),
            ("c".to_string(), "a".to_string()),
        ];

        let steps = order(&rows);
        for i in 0..3 {
            assert_eq!(
                steps.iter().filter(|s| **s == Step::Rename(i)).count(),
                1,
                "row {i} renamed exactly once: {steps:?}"
            );
        }
        assert_eq!(steps.iter().filter(|s| matches!(s, Step::Park(_))).count(), 1);
    }
}

#[cfg(test)]
mod legend_is_true {
    /// Every form the legend shows has to parse. If a placeholder is renamed
    /// or a flag dropped, this fails rather than leaving the prompt advertising
    /// syntax that no longer works.
    #[test]
    fn the_prompt_accepts_everything_it_advertises() {
        for ex in super::LEGEND_EXAMPLES {
            assert!(
                super::parse_rule(ex).is_ok(),
                "the legend offers `{ex}`, which the parser refuses",
            );
        }
    }
}
