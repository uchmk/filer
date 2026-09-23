//! yazi-compatible keymap loading and chord resolution.
//!
//! Layering follows yazi: `prepend_keymap` entries come first, then either the
//! user's `keymap` (a full replacement) or the built-in defaults, then
//! `append_keymap`. The first exact match wins, which is what makes a
//! prepended single-key binding shadow a default prefix.

use serde::Deserialize;

use super::cmd::{self, Act};
use super::keys::Key;

#[derive(Clone, Debug)]
pub struct Binding {
    pub on: Vec<Key>,
    pub run: Vec<Act>,
    pub desc: String,
    /// The command text, shown in the help panel.
    pub raw: String,
}

#[derive(Default, Debug)]
pub struct Keymap {
    pub mgr: Vec<Binding>,
    pub input: Vec<Binding>,
    pub confirm: Vec<Binding>,
    pub pick: Vec<Binding>,
    pub help: Vec<Binding>,
    pub tasks: Vec<Binding>,
    pub spot: Vec<Binding>,
    /// Commands that parsed but aren't implemented, for the help panel.
    pub unsupported: Vec<String>,
}

pub enum Match<'a> {
    /// Run this binding.
    Exact(&'a Binding),
    /// More keys needed; these are the candidates to show in the which panel.
    Pending(Vec<&'a Binding>),
    None,
}

pub fn resolve<'a>(bindings: &'a [Binding], pending: &[Key]) -> Match<'a> {
    let mut candidates: Vec<&Binding> = Vec::new();
    for b in bindings {
        if b.on.len() >= pending.len() && b.on[..pending.len()] == *pending {
            candidates.push(b);
        }
    }
    match candidates.first() {
        None => Match::None,
        Some(first) if first.on.len() == pending.len() => Match::Exact(first),
        Some(_) => Match::Pending(candidates),
    }
}

// ---------------------------------------------------------------- TOML schema

#[derive(Deserialize, Debug, Default)]
struct KeymapFile {
    #[serde(default, alias = "manager")]
    mgr: Section,
    #[serde(default)]
    input: Section,
    #[serde(default)]
    confirm: Section,
    #[serde(default, alias = "select")]
    pick: Section,
    #[serde(default)]
    help: Section,
    #[serde(default)]
    tasks: Section,
    #[serde(default)]
    spot: Section,
    #[serde(default)]
    cmp: Section,
}

#[derive(Deserialize, Debug, Default)]
struct Section {
    #[serde(default)]
    keymap: Option<Vec<RawBinding>>,
    #[serde(default)]
    prepend_keymap: Vec<RawBinding>,
    #[serde(default)]
    append_keymap: Vec<RawBinding>,
}

#[derive(Deserialize, Debug, Clone)]
struct RawBinding {
    on: StrOrVec,
    #[serde(default)]
    run: StrOrVec,
    #[serde(default)]
    desc: String,
}

use super::StrOrVec;

/// Split a key string into tokens: `"gg"` and `"g g"` both become `[g, g]`,
/// while `"<C-a>"` stays a single key.
fn tokenize(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '<' {
            let mut tok = String::from('<');
            for c in it.by_ref() {
                tok.push(c);
                if c == '>' {
                    break;
                }
            }
            out.push(tok);
        } else if c.is_whitespace() {
            continue;
        } else {
            out.push(c.to_string());
        }
    }
    out
}

fn build(raw: &RawBinding, warnings: &mut Vec<String>) -> Option<Binding> {
    let tokens: Vec<String> = if raw.on.0.len() == 1 {
        tokenize(&raw.on.0[0])
    } else {
        raw.on.0.clone()
    };
    let mut on = Vec::with_capacity(tokens.len());
    for t in &tokens {
        match Key::parse(t) {
            Some(k) => on.push(k),
            None => {
                warnings.push(format!("unknown key `{t}`"));
                return None;
            }
        }
    }
    if on.is_empty() {
        return None;
    }
    let raw_text = raw.run.0.join("; ");
    let run: Vec<Act> = raw.run.0.iter().map(|s| cmd::parse(s)).collect();
    for (a, s) in run.iter().zip(&raw.run.0) {
        if let Act::Unsupported(_) = a {
            warnings.push(format!("unsupported command `{s}`"));
        }
    }
    Some(Binding { on, run, desc: raw.desc.clone(), raw: raw_text })
}

/// Apply one file's layers on top of what previous files produced.
fn fold(base: Vec<Binding>, s: &Section, warnings: &mut Vec<String>) -> Vec<Binding> {
    let mut out = Vec::new();
    for r in &s.prepend_keymap {
        out.extend(build(r, warnings));
    }
    match &s.keymap {
        Some(list) => {
            for r in list {
                out.extend(build(r, warnings));
            }
        }
        None => out.extend(base),
    }
    for r in &s.append_keymap {
        out.extend(build(r, warnings));
    }
    out
}

pub const DEFAULT_KEYMAP: &str = include_str!("defaults/keymap.toml");

impl Keymap {
    /// Fold the built-in defaults and each user keymap file in turn.
    pub fn load(user_tomls: &[&str]) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut files = vec![
            toml::from_str::<KeymapFile>(DEFAULT_KEYMAP).expect("built-in keymap must parse"),
        ];
        for text in user_tomls {
            match toml::from_str::<KeymapFile>(text) {
                Ok(k) => files.push(k),
                Err(e) => warnings.push(format!("keymap.toml: {e}")),
            }
        }

        let mut km = Keymap::default();
        for f in &files {
            km.mgr = fold(std::mem::take(&mut km.mgr), &f.mgr, &mut warnings);
            km.input = fold(std::mem::take(&mut km.input), &f.input, &mut warnings);
            km.confirm = fold(std::mem::take(&mut km.confirm), &f.confirm, &mut warnings);
            km.pick = fold(std::mem::take(&mut km.pick), &f.pick, &mut warnings);
            km.help = fold(std::mem::take(&mut km.help), &f.help, &mut warnings);
            km.tasks = fold(std::mem::take(&mut km.tasks), &f.tasks, &mut warnings);
            km.spot = fold(std::mem::take(&mut km.spot), &f.spot, &mut warnings);
            let _ = &f.cmp; // parsed for compatibility; completion is native here
        }
        km.unsupported = warnings.clone();
        (km, warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_keymap_loads() {
        let (km, warnings) = Keymap::load(&[]);
        assert!(km.mgr.len() > 50, "got {} bindings", km.mgr.len());
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn prepend_shadows_default_prefix() {
        let user = r#"
[[mgr.prepend_keymap]]
on = "m"
run = "plugin bookmarks save"
"#;
        let (km, _) = Keymap::load(&[user]);
        let m = Key::parse("m").unwrap();
        match resolve(&km.mgr, &[m]) {
            Match::Exact(b) => assert_eq!(b.run, vec![Act::BookmarkSave]),
            _ => panic!("prepended single key should win over the default prefix"),
        }
    }

    #[test]
    fn multi_key_chords_pend() {
        let (km, _) = Keymap::load(&[]);
        let g = Key::parse("g").unwrap();
        assert!(matches!(resolve(&km.mgr, &[g]), Match::Pending(_)));
    }
}
