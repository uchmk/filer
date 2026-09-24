//! Small shared helpers: natural ordering, formatting, path hygiene, LRU cache.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::Hash;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

/// Natural ("version") comparison: `a2` < `a10`, and digits never sort as text.
pub fn natural_cmp(a: &str, b: &str, case_sensitive: bool) -> Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ac), Some(bc)) => {
                if ac.is_ascii_digit() && bc.is_ascii_digit() {
                    let an = take_number(&mut ai);
                    let bn = take_number(&mut bi);
                    // Compare by value, then by written form so "01" and "1" stay stable.
                    match an.trim_start_matches('0').len().cmp(&bn.trim_start_matches('0').len()) {
                        Ordering::Equal => {}
                        o => return o,
                    }
                    match an.trim_start_matches('0').cmp(bn.trim_start_matches('0')) {
                        Ordering::Equal => {}
                        o => return o,
                    }
                    match an.len().cmp(&bn.len()) {
                        Ordering::Equal => {}
                        o => return o,
                    }
                } else {
                    ai.next();
                    bi.next();
                    let (x, y) = if case_sensitive {
                        (ac, bc)
                    } else {
                        (lower(ac), lower(bc))
                    };
                    match x.cmp(&y) {
                        Ordering::Equal => {}
                        o => return o,
                    }
                }
            }
        }
    }
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn take_number(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut s = String::new();
    while let Some(c) = it.peek().copied() {
        if c.is_ascii_digit() {
            s.push(c);
            it.next();
        } else {
            break;
        }
    }
    s
}

/// Case-insensitive-first alphabetical comparison, falling back to the
/// case-sensitive form so distinct names never compare equal.
pub fn alpha_cmp(a: &str, b: &str, case_sensitive: bool) -> Ordering {
    if case_sensitive {
        a.cmp(b)
    } else {
        let mut ai = a.chars().map(lower);
        let mut bi = b.chars().map(lower);
        loop {
            match (ai.next(), bi.next()) {
                (None, None) => return a.cmp(b),
                (None, Some(_)) => return Ordering::Less,
                (Some(_), None) => return Ordering::Greater,
                (Some(x), Some(y)) => match x.cmp(&y) {
                    Ordering::Equal => {}
                    o => return o,
                },
            }
        }
    }
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 7] = ["B", "K", "M", "G", "T", "P", "E"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1000.0 && i + 1 < UNITS.len() {
        v /= 1024.0;
        i += 1;
    }
    if v < 10.0 {
        format!("{v:.1} {}", UNITS[i])
    } else {
        format!("{v:.0} {}", UNITS[i])
    }
}

pub fn fmt_time(t: Option<SystemTime>, fmt: &str) -> String {
    match t {
        Some(t) => {
            let dt: chrono::DateTime<chrono::Local> = t.into();
            dt.format(fmt).to_string()
        }
        None => String::new(),
    }
}

/// Lexically normalize a path (resolve `.`/`..`) without touching the filesystem.
///
/// `..` never climbs past a root, so a drive root, a UNC share root (`\\host\share`)
/// and `/` all stay put — the same as the OS resolves them.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    // Roots pushed so far (a prefix and/or a separator), and the components
    // above them that `..` is allowed to pop.
    let mut rooted = false;
    let mut depth = 0usize;
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if depth > 0 {
                    out.pop();
                    depth -= 1;
                } else if !rooted {
                    // Relative path: keep climbing, there is nothing to pop.
                    out.push("..");
                }
                // At a root `..` is the root itself; drop it.
            }
            // Only Windows produces a prefix, and only there can it be spelled
            // with forward slashes (`//host/share`); settle on one spelling so
            // paths compare and display the same either way.
            Component::Prefix(_) => {
                out.push(backslashed(c.as_os_str()));
                rooted = true;
            }
            Component::RootDir => {
                out.push(c.as_os_str());
                rooted = true;
            }
            Component::Normal(s) => {
                out.push(s);
                depth += 1;
            }
        }
    }
    if out.as_os_str().is_empty() {
        return PathBuf::from(".");
    }
    if host_only_unc(path) {
        // `\\host` without a share is no prefix to `std`, which would leave us
        // with `\host` — a different place. Keep the pair the user typed.
        let mut s = std::ffi::OsString::from(r"\");
        s.push(out.as_os_str());
        return PathBuf::from(s);
    }
    out
}

/// Drop the `\\?\` that Windows puts on junction targets and canonical paths.
/// Verbatim UNC (`\\?\UNC\host\share`) keeps its prefix: the shorter spelling
/// is not a plain prefix strip, and the verbatim form works everywhere we pass
/// it on.
pub fn unverbatim(path: &Path) -> PathBuf {
    let Some(s) = path.to_str() else { return path.to_path_buf() };
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

fn backslashed(s: &std::ffi::OsStr) -> std::ffi::OsString {
    match s.to_str() {
        Some(t) if t.contains('/') => std::ffi::OsString::from(t.replace('/', r"\")),
        _ => s.to_os_string(),
    }
}

/// `\\host` or `//host`: the start of a UNC path that names no share yet.
fn host_only_unc(path: &Path) -> bool {
    if !cfg!(windows) {
        // A leading `//` is an ordinary path elsewhere.
        return false;
    }
    let b = path.as_os_str().as_encoded_bytes();
    b.len() > 2
        && matches!(b[0], b'\\' | b'/')
        && matches!(b[1], b'\\' | b'/')
        && !matches!(b[2], b'\\' | b'/')
        && !matches!(path.components().next(), Some(Component::Prefix(_)))
}

/// Expand `~`, `%VAR%` and `$VAR` in a user-supplied path string.
pub fn expand(input: &str) -> PathBuf {
    let mut s = input.trim().to_string();
    if s == "~" || s.starts_with("~/") || s.starts_with("~\\") {
        if let Some(home) = dirs::home_dir() {
            let rest = &s[1..];
            let rest = rest.trim_start_matches(['/', '\\']);
            let mut p = home;
            if !rest.is_empty() {
                p.push(rest);
            }
            return p;
        }
    }
    // %VAR%
    while let Some(start) = s.find('%') {
        let Some(end) = s[start + 1..].find('%').map(|i| start + 1 + i) else { break };
        let name = &s[start + 1..end];
        let val = std::env::var(name).unwrap_or_default();
        s.replace_range(start..=end, &val);
    }
    PathBuf::from(s)
}

/// Absolutize relative to `base`, then normalize.
pub fn resolve_against(base: &Path, input: &str) -> PathBuf {
    let p = expand(input);
    // `\\host` has a root but no prefix, so `is_absolute` says no; joining it
    // onto the base would quietly turn it into `<drive>\host`.
    if p.is_absolute() || has_windows_prefix(&p) || host_only_unc(&p) {
        normalize(&p)
    } else {
        normalize(&base.join(p))
    }
}

fn has_windows_prefix(p: &Path) -> bool {
    matches!(p.components().next(), Some(Component::Prefix(_)))
}

pub fn file_name(path: &Path) -> String {
    match path.file_name() {
        Some(n) => n.to_string_lossy().into_owned(),
        // Root of a drive / UNC share: show the prefix itself.
        None => path.to_string_lossy().into_owned(),
    }
}

pub fn extension(name: &str) -> Option<String> {
    let dot = name.rfind('.')?;
    if dot == 0 || dot + 1 == name.len() {
        return None;
    }
    Some(name[dot + 1..].to_ascii_lowercase())
}

pub fn stem_and_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// Truncate to `max_cols` display columns, inserting an ellipsis in the middle
/// so both the start and the extension stay readable.
pub fn ellipsize_middle(s: &str, max_cols: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    let total: usize = s.chars().map(|c| c.width().unwrap_or(0)).sum();
    if total <= max_cols || max_cols < 4 {
        return s.to_string();
    }
    let keep = max_cols - 1;
    let head_cols = keep * 2 / 3;
    let tail_cols = keep - head_cols;

    let mut head = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if w + cw > head_cols {
            break;
        }
        head.push(c);
        w += cw;
    }
    let mut tail: Vec<char> = Vec::new();
    let mut w = 0;
    for c in s.chars().rev() {
        let cw = c.width().unwrap_or(0);
        if w + cw > tail_cols {
            break;
        }
        tail.push(c);
        w += cw;
    }
    tail.reverse();
    format!("{head}…{}", tail.into_iter().collect::<String>())
}

/// A tiny insertion-ordered LRU. Big enough for folder and preview caches.
pub struct Lru<K: Eq + Hash + Clone, V> {
    map: HashMap<K, V>,
    order: Vec<K>,
    cap: usize,
}

impl<K: Eq + Hash + Clone, V> Lru<K, V> {
    pub fn new(cap: usize) -> Self {
        Self { map: HashMap::new(), order: Vec::new(), cap: cap.max(1) }
    }

    pub fn get(&mut self, k: &K) -> Option<&V> {
        if self.map.contains_key(k) {
            self.touch(k);
            self.map.get(k)
        } else {
            None
        }
    }

    pub fn peek(&self, k: &K) -> Option<&V> {
        self.map.get(k)
    }

    pub fn put(&mut self, k: K, v: V) {
        if self.map.insert(k.clone(), v).is_none() {
            self.order.push(k);
        } else {
            self.touch(&k);
        }
        while self.order.len() > self.cap {
            let oldest = self.order.remove(0);
            self.map.remove(&oldest);
        }
    }

    pub fn remove(&mut self, k: &K) -> Option<V> {
        self.order.retain(|x| x != k);
        self.map.remove(k)
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }

    fn touch(&mut self, k: &K) {
        if let Some(i) = self.order.iter().position(|x| x == k) {
            let k = self.order.remove(i);
            self.order.push(k);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        let mut v = vec!["a10", "a2", "a1", "b1"];
        v.sort_by(|a, b| natural_cmp(a, b, false));
        assert_eq!(v, vec!["a1", "a2", "a10", "b1"]);
    }

    #[test]
    fn sizes() {
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(1024), "1.0 K");
        assert_eq!(human_size(1024 * 1024 * 20), "20 M");
    }

    #[test]
    fn normalizes() {
        assert_eq!(normalize(Path::new(r"C:\a\b\..\c")), PathBuf::from(r"C:\a\c"));
        assert_eq!(normalize(Path::new("a/./b/../c")), PathBuf::from("a").join("c"));
        // Relative paths have nothing to pop, so `..` stays.
        assert_eq!(normalize(Path::new("../../a")), PathBuf::from("..").join("..").join("a"));
    }

    #[cfg(windows)]
    #[test]
    fn dot_dot_stops_at_a_root() {
        for (input, want) in [
            (r"C:\a\..\..", r"C:\"),
            (r"C:\..\..\x", r"C:\x"),
            (r"\\host\share\a\..\..", r"\\host\share\"),
            (r"\\host\share\..\..\x", r"\\host\share\x"),
        ] {
            assert_eq!(normalize(Path::new(input)), PathBuf::from(want), "{input}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn keeps_unc_paths_whole() {
        // A share root is its own parent, and it keeps its two leading slashes.
        let share = normalize(Path::new(r"\\192.168.1.5\pub"));
        assert_eq!(share, PathBuf::from(r"\\192.168.1.5\pub"));
        assert_eq!(share.parent(), None);
        assert_eq!(file_name(&share), r"\\192.168.1.5\pub\");

        // Forward slashes spell the same share.
        assert_eq!(normalize(Path::new("//192.168.1.5/pub/x")), PathBuf::from(r"\\192.168.1.5\pub\x"));

        // A host with no share yet must not collapse to `\host`.
        assert_eq!(normalize(Path::new(r"\\192.168.1.5")), PathBuf::from(r"\\192.168.1.5"));
        assert_eq!(
            resolve_against(Path::new(r"C:\work"), r"\\192.168.1.5"),
            PathBuf::from(r"\\192.168.1.5")
        );

        // Typing a share into the prompt is absolute, not relative to the tab.
        assert_eq!(
            resolve_against(Path::new(r"C:\work"), r"\\nas\media\photos"),
            PathBuf::from(r"\\nas\media\photos")
        );
        // ...and a plain rooted path still picks up the base's drive.
        assert_eq!(resolve_against(Path::new(r"D:\work"), r"\tmp"), PathBuf::from(r"D:\tmp"));
    }

    #[test]
    fn ellipsis_keeps_tail() {
        let s = ellipsize_middle("averyveryverylongfilename.txt", 16);
        assert!(s.contains('…') && s.ends_with(".txt"));
    }
}
