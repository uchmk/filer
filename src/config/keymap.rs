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
    /// The side-by-side compare view's own keys.
    pub diff: Vec<Binding>,
    /// The few keys the terminal pane keeps for itself; everything else it
    /// hears goes to the shell.
    pub term: Vec<Binding>,
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
    diff: Section,
    #[serde(default)]
    term: Section,
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
            km.diff = fold(std::mem::take(&mut km.diff), &f.diff, &mut warnings);
            km.term = fold(std::mem::take(&mut km.term), &f.term, &mut warnings);
            let _ = &f.cmp; // parsed for compatibility; completion is native here
        }
        for (name, bindings) in km.layers() {
            warnings.extend(unreachable(name, bindings));
        }
        km.unsupported = warnings.clone();
        (km, warnings)
    }

    /// Every layer with the name it is written under, for the checks and tests
    /// that have to treat them alike.
    fn layers(&self) -> [(&'static str, &[Binding]); 9] {
        [
            ("mgr", &self.mgr),
            ("input", &self.input),
            ("confirm", &self.confirm),
            ("pick", &self.pick),
            ("help", &self.help),
            ("tasks", &self.tasks),
            ("spot", &self.spot),
            ("term", &self.term),
            ("diff", &self.diff),
        ]
    }
}

/// Bindings in `bindings` that no key press can ever reach.
///
/// [`resolve`] takes the first binding that matches, so order decides: a key
/// bound on its own, before a chord that starts with it, ends the sequence
/// where it stands and the chord never runs. A user file's `prepend_keymap`
/// goes in front of everything, which is exactly how one line can silently
/// retire a whole prefix — binding `m` to a command, as the bookmarks plugins
/// for yazi do, takes `m`+`s`, `m`+`t` and the rest of the line-mode keys with
/// it, and nothing about pressing `m` suggests that is what happened.
///
/// One warning per key that shadows, not per binding shadowed: a prefix with
/// five chords under it is one mistake, and five lines would push everything
/// else out of the panel that shows them.
fn unreachable(layer: &str, bindings: &[Binding]) -> Vec<String> {
    let mut out = Vec::new();
    let mut lost = vec![false; bindings.len()];
    for i in 0..bindings.len() {
        // Something earlier already swallows this one, and said so.
        if lost[i] {
            continue;
        }
        let on = &bindings[i].on;
        let mut hidden: Vec<String> = Vec::new();
        let mut twice = false;
        for j in i + 1..bindings.len() {
            let other = &bindings[j].on;
            if other == on {
                twice = true;
            } else if other.len() > on.len() && other[..on.len()] == on[..] {
                hidden.push(super::keys::render_seq(other));
            } else {
                continue;
            }
            lost[j] = true;
        }
        let key = super::keys::render_seq(on);
        let run = &bindings[i].raw;
        if twice {
            out.push(format!("[{layer}] `{key}` is bound more than once; only `{run}` runs"));
        }
        if !hidden.is_empty() {
            let n = hidden.len();
            // Three is as many as the line can carry and still be read.
            hidden.truncate(3);
            let shown = hidden.join("`, `");
            let more = if n > 3 { format!(", and {} more", n - 3) } else { String::new() };
            out.push(format!(
                "[{layer}] `{key}` runs `{run}` on its own, so the {n} key(s) starting with it \
                 never run: `{shown}`{more}",
            ));
        }
    }
    out
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

    fn bound(section: &[Binding], key: &str) -> Vec<Act> {
        let k = Key::parse(key).unwrap_or_else(|| panic!("`{key}` is not key notation"));
        match resolve(section, &[k]) {
            Match::Exact(b) => b.run.clone(),
            _ => panic!("`{key}` is not bound to anything on its own"),
        }
    }

    /// The easy key must not be the destructive one. `<C-t>` is pressed all day
    /// to go to and fro; ending the shell — losing the scrollback and whatever
    /// is running — belongs on the harder chord.
    #[test]
    fn the_terminals_easy_key_keeps_the_shell_alive() {
        let (km, _) = Keymap::load(&[]);

        assert_eq!(bound(&km.term, "<C-t>"), vec![Act::Close]);
        assert_eq!(bound(&km.term, "<C-S-t>"), vec![Act::Terminal(Some(false))]);
    }

    /// Without these the help and the palette cannot be reached at all while
    /// the terminal holds the keys, and the shell has no use for either.
    #[test]
    fn the_terminal_layer_lets_the_help_and_the_palette_through() {
        let (km, _) = Keymap::load(&[]);

        assert_eq!(bound(&km.term, "<F1>"), vec![Act::Help]);
        assert_eq!(bound(&km.term, "<C-S-p>"), vec![Act::Palette]);
    }

    /// Reloading from inside the pane: before, `<C-F5>` went to the shell as
    /// `\e[15;5~` and trying a config change meant leaving the pane each time.
    #[test]
    fn the_terminal_layer_reloads_the_config() {
        let (km, _) = Keymap::load(&[]);

        assert_eq!(bound(&km.term, "<C-F5>"), bound(&km.mgr, "<C-F5>"));
        assert_eq!(bound(&km.term, "<C-F5>"), vec![Act::ConfigReload]);
    }

    /// The one place the defaults knowingly leave yazi's: `<C-r>` is redo, as
    /// it is nearly everywhere, and inverting the selection moves over one
    /// modifier. Worth a test, because the obvious "fix" is to put it back.
    #[test]
    fn ctrl_r_redoes_and_inverting_the_selection_moved() {
        let (km, _) = Keymap::load(&[]);

        assert_eq!(bound(&km.mgr, "<C-r>"), vec![Act::Redo]);
        assert_eq!(bound(&km.mgr, "U"), vec![Act::Redo]);
        assert_eq!(bound(&km.mgr, "<C-S-r>"), vec![Act::ToggleAll { state: None }]);
    }

    /// Jumping is `'`, the rest hangs off `b`. Both references agree on this
    /// split — vim jumps to a mark with `'`, and bookmarks.yazi jumps with `'`
    /// and deletes with `b`+`d` — and the earlier arrangement, where `b` and
    /// `'` were the same command, read in the key list as one of them being
    /// something else.
    #[test]
    fn bookmarks_jump_with_the_vim_key_and_are_managed_under_b() {
        let (km, _) = Keymap::load(&[]);
        let chord = |keys: &[&str]| {
            let ks: Vec<Key> = keys.iter().map(|k| Key::parse(k).expect("key notation")).collect();
            match resolve(&km.mgr, &ks) {
                Match::Exact(b) => b.run.clone(),
                _ => panic!("`{keys:?}` is not bound"),
            }
        };

        assert_eq!(bound(&km.mgr, "'"), vec![Act::BookmarkJump]);
        assert_eq!(chord(&["b", "b"]), vec![Act::BookmarkList]);
        assert_eq!(chord(&["b", "s"]), vec![Act::BookmarkSave]);
        assert_eq!(chord(&["b", "d"]), vec![Act::BookmarkDelete]);
        assert_eq!(chord(&["b", "D"]), vec![Act::BookmarkDeleteAll]);

        // `b` alone must stay a prefix: bound to a command of its own it would
        // shadow every chord above, which is how it used to behave.
        assert!(
            matches!(resolve(&km.mgr, &[Key::parse("b").unwrap()]), Match::Pending(_)),
            "`b` must lead somewhere, not do something",
        );

        // The keys these grew out of still work.
        assert_eq!(bound(&km.mgr, "B"), vec![Act::BookmarkSave]);
        assert_eq!(bound(&km.mgr, "<A-b>"), vec![Act::BookmarkDelete]);
        assert_eq!(bound(&km.mgr, "<A-B>"), vec![Act::BookmarkDeleteAll]);
    }

    /// No key may mean two things in one layer, and no single key may sit in
    /// front of a chord that starts with it: `resolve` takes the first exact
    /// match, so the chord would never be reached.
    #[test]
    fn no_layer_binds_a_key_twice_or_swallows_its_own_chords() {
        let (km, _) = Keymap::load(&[]);
        let layers: [(&str, &[Binding]); 9] = [
            ("mgr", &km.mgr),
            ("input", &km.input),
            ("confirm", &km.confirm),
            ("pick", &km.pick),
            ("help", &km.help),
            ("tasks", &km.tasks),
            ("spot", &km.spot),
            ("term", &km.term),
            ("diff", &km.diff),
        ];
        for (name, bindings) in layers {
            for (i, b) in bindings.iter().enumerate() {
                for (j, other) in bindings.iter().enumerate().skip(i + 1) {
                    assert_ne!(b.on, other.on, "[{name}] binds {:?} twice", b.on);
                    assert!(
                        !(b.on.len() == 1 && other.on.len() > 1 && other.on[0] == b.on[0]),
                        "[{name}] #{i} {:?} comes before #{j} {:?} and swallows it",
                        b.on,
                        other.on,
                    );
                }
            }
        }
    }

    /// The defaults being clean is only half of it: the arrangement that
    /// actually bites is a user file's, and it bites in silence. One
    /// `prepend_keymap` line binding `m` — which is what the bookmark plugins
    /// for yazi do, and what a config copied from one carries over — sits in
    /// front of every line-mode chord and ends the sequence at `m`. Pressing
    /// `m`+`s` then saves a bookmark under `s`, and nothing anywhere says the
    /// line-mode keys are gone.
    #[test]
    fn a_user_key_that_buries_a_whole_prefix_is_reported() {
        let user = r#"
            [[mgr.prepend_keymap]]
            on = "m"
            run = "plugin bookmarks save"
        "#;
        let (_, warnings) = Keymap::load(&[user]);

        let about_m: Vec<&String> = warnings.iter().filter(|w| w.contains("`m`")).collect();
        assert_eq!(about_m.len(), 1, "one line per prefix, not one per key lost: {warnings:?}");
        let w = about_m[0];
        assert!(w.contains("plugin bookmarks save"), "must name what took the key: {w}");
        assert!(w.contains("`ms`"), "must name a key that was lost: {w}");
        assert!(w.contains('5'), "must count them: {w}");
    }

    /// The same key twice is the other way a line goes missing, and it reads
    /// as the key being unbound rather than as the file disagreeing with
    /// itself.
    #[test]
    fn a_key_bound_twice_is_reported() {
        let user = r#"
            [[mgr.prepend_keymap]]
            on = "<F5>"
            run = "quit"
            [[mgr.prepend_keymap]]
            on = "<F5>"
            run = "close"
        "#;
        let (_, warnings) = Keymap::load(&[user]);

        let w = warnings.iter().find(|w| w.contains("more than once"));
        let w = w.unwrap_or_else(|| panic!("no duplicate reported: {warnings:?}"));
        assert!(w.contains("quit"), "must name the one that wins: {w}");
    }
}
