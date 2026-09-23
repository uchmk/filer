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
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
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
    if p.is_absolute() || has_windows_prefix(&p) {
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
    }

    #[test]
    fn ellipsis_keeps_tail() {
        let s = ellipsize_middle("averyveryverylongfilename.txt", 16);
        assert!(s.contains('…') && s.ends_with(".txt"));
    }
}
