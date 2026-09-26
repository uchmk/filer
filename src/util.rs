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

/// A waiting time, in the shortest form that still reads: `45s`, `3m10s`,
/// `2h05m`. Long enough to be worth showing, short enough to sit in a status
/// bar beside everything else.
pub fn fmt_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m{:02}s", secs / 60, secs % 60),
        _ => format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60),
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

/// The server a share sits on: `\\host\share` → `\\host`.
///
/// Its own function because `std` folds the host and the share into a single
/// prefix, leaving a share root with no parent at all. That was right while a
/// host could not be listed, and is a dead end now that it can be: `h` from a
/// share would have nowhere to go.
///
/// Written against the shape of the string rather than the components, so it
/// can be reasoned about — and tested — on any platform. It answers only for a
/// share root: anything deeper has an ordinary parent, and `\\host` is already
/// the top.
pub fn unc_host(path: &Path) -> Option<PathBuf> {
    let s = path.to_str()?;
    let rest = s.strip_prefix(r"\\").or_else(|| s.strip_prefix("//"))?;
    let mut parts = rest.split(['\\', '/']).filter(|p| !p.is_empty());
    let host = parts.next()?;
    parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    Some(PathBuf::from(format!(r"\\{host}")))
}

/// The directory above `path`, as the view means it: what the parent pane
/// shows, and where `h` goes.
///
/// `Path::parent` is wrong at both ends of a UNC path, in opposite directions.
/// A share root `\\host\share` is one whole prefix to `std`, so it has *no*
/// parent, though the host above it is a real place now that its shares can be
/// listed. And `\\host` — which `std` does not recognise as a prefix at all —
/// parses as a root and one component, so its parent comes out as the bare
/// `\`: a different machine's drive, offered as the folder above a file
/// server. A host is the top; nothing is above it.
pub fn parent_dir(path: &Path) -> Option<PathBuf> {
    if host_only_unc(path) {
        return None;
    }
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => Some(p.to_path_buf()),
        _ => unc_host(path),
    }
}

/// `\\host` or `//host`: the start of a UNC path that names no share yet.
/// `\\host` with no share after it — a server, not a directory.
pub fn host_only_unc(path: &Path) -> bool {
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
        let val = match std::env::var(name) {
            Ok(v) if !v.is_empty() => v,
            // `%FILER_CONFIG_HOME%` and `%YAZI_CONFIG_HOME%` are usually unset,
            // and a path written with one still has to lead somewhere: they
            // stand for the directory filer would search. Every other unset
            // variable keeps expanding to nothing, as the shells do.
            _ => crate::config::config_dir_default(name)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        };
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
    if let Some(n) = path.file_name() {
        return n.to_string_lossy().into_owned();
    }
    // A share root is named after its share. `std` has no file name for one,
    // folding the host and the share into a single prefix, and the fallback
    // below — the path itself, which is what a drive root wants — listed every
    // share on a server under its full address instead of its name.
    if let Some(n) = unc_share(path) {
        return n;
    }
    // Root of a drive: `C:\` is what it is called.
    path.to_string_lossy().into_owned()
}

/// The share out of a share root: `\\host\share` → `share`. `None` for
/// anything else, including `\\host`, which names no share, and a path inside
/// a share, which has an ordinary file name.
///
/// Written against the string rather than the components, for the same reason
/// as [`unc_host`]: it can then be reasoned about and tested anywhere.
fn unc_share(path: &Path) -> Option<String> {
    let s = path.to_str()?;
    let rest = s.strip_prefix(r"\\").or_else(|| s.strip_prefix("//"))?;
    let mut parts = rest.split(['\\', '/']).filter(|p| !p.is_empty());
    parts.next()?;
    let share = parts.next()?;
    parts.next().is_none().then(|| share.to_owned())
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

    /// An unset config variable expands to the directory filer searches, and
    /// every other unset variable still expands to nothing.
    ///
    /// The distinction is the whole point: `gc` reads `cd %FILER_CONFIG_HOME%`,
    /// and if an unset variable vanished the way the others do, the key would
    /// walk to the filesystem root instead — which is exactly what the old
    /// `cd %APPDATA%/filer` did on anything but Windows.
    #[test]
    fn config_variables_expand_even_when_unset() {
        for var in crate::config::CONFIG_VARS {
            let want = crate::config::config_home(var).expect("a config directory");
            assert_eq!(expand(&format!("%{var}%")), want, "%{var}%");
            assert_eq!(expand(&format!("%{var}%/theme.toml")), want.join("theme.toml"));
        }
        // Not a name with a default, and almost certainly not set.
        assert_eq!(expand("%NO_SUCH_VARIABLE_HERE%"), PathBuf::from(""));
        assert_eq!(expand("%NO_SUCH_VARIABLE_HERE%/x"), PathBuf::from("/x"));
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["a10", "a2", "a1", "b1"];
        v.sort_by(|a, b| natural_cmp(a, b, false));
        assert_eq!(v, vec!["a1", "a2", "a10", "b1"]);
    }

    #[test]
    fn durations_read_at_a_glance() {
        use std::time::Duration;
        assert_eq!(fmt_duration(Duration::from_secs(0)), "0s");
        assert_eq!(fmt_duration(Duration::from_secs(45)), "45s");
        // A minute in, seconds are padded so the text stops jittering.
        assert_eq!(fmt_duration(Duration::from_secs(60)), "1m00s");
        assert_eq!(fmt_duration(Duration::from_secs(190)), "3m10s");
        assert_eq!(fmt_duration(Duration::from_secs(3599)), "59m59s");
        assert_eq!(fmt_duration(Duration::from_secs(3600)), "1h00m");
        assert_eq!(fmt_duration(Duration::from_secs(7_500)), "2h05m");
    }

    #[test]
    fn sizes() {
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(1024), "1.0 K");
        assert_eq!(human_size(1024 * 1024 * 20), "20 M");
    }

    /// Up from a share is the server. Up from anything else is `parent()`'s
    /// business, and this says so by declining.
    #[test]
    fn a_share_knows_which_server_it_is_on() {
        let host = |s: &str| unc_host(Path::new(s));
        assert_eq!(host(r"\\192.0.2.10\Backup"), Some(PathBuf::from(r"\\192.0.2.10")));
        assert_eq!(host(r"\\192.0.2.10\Backup\"), Some(PathBuf::from(r"\\192.0.2.10")));
        // Forward slashes are how the same path arrives from a config or a URL.
        assert_eq!(host("//server/pub"), Some(PathBuf::from(r"\\server")));
        // Deeper than a share root: `parent()` already answers that one.
        assert_eq!(host(r"\\192.0.2.10\Backup\2025"), None);
        // The host itself is the top; there is nothing above it.
        assert_eq!(host(r"\\192.0.2.10"), None);
        // Not UNC at all.
        assert_eq!(host(r"C:\dev\filer"), None);
        assert_eq!(host("/home/user"), None);
    }

    /// A server answers with full addresses; the column wants names. The
    /// share root is the case `std` has no answer for, so the fallback — the
    /// whole path, which is right for `C:\` — used to stand in, and every
    /// share on a server listed itself as `\\192.0.2.10\Backup\`. The header,
    /// which joins the directory to the hovered name, then read
    /// `\\192.0.2.10\\\192.0.2.10\Backup\`.
    #[test]
    fn a_share_root_is_named_after_its_share() {
        let share = |s: &str| unc_share(Path::new(s));

        assert_eq!(share(r"\\192.0.2.10\backup-user").as_deref(), Some("backup-user"));
        assert_eq!(share(r"\\192.0.2.10\Backup\").as_deref(), Some("Backup"));
        assert_eq!(share("//192.0.2.10/Media_Library").as_deref(), Some("Media_Library"));
        // An administrative share keeps its `$`; a share name may hold a space.
        assert_eq!(share(r"\\host\C$").as_deref(), Some("C$"));
        assert_eq!(share(r"\\host\My Files").as_deref(), Some("My Files"));
        // The host names no share.
        assert_eq!(share(r"\\192.0.2.10"), None);
        // Inside a share `std` has the answer, so this declines to give one.
        assert_eq!(share(r"\\192.0.2.10\Backup\2025"), None);
        assert_eq!(share(r"C:\dev"), None);
    }

    /// The same rule through the function that uses it. Windows-only because
    /// `\` is a separator only there: elsewhere `Path` reads the whole of
    /// `\\host\share` as one component and `file_name` answers before the
    /// share rule is reached.
    #[cfg(windows)]
    #[test]
    fn a_share_is_listed_under_its_name_and_a_drive_root_under_its_own() {
        let name = |s: &str| file_name(Path::new(s));

        assert_eq!(name(r"\\192.0.2.10\backup-user"), "backup-user");
        assert_eq!(name(r"\\192.0.2.10\Backup\"), "Backup");
        // The host is called after itself.
        assert_eq!(name(r"\\192.0.2.10"), "192.0.2.10");
        assert_eq!(name(r"\\192.0.2.10\Backup\2025\notes.txt"), "notes.txt");
        // A drive root still shows itself, which is what it is called.
        assert_eq!(name(r"C:\"), r"C:\");
    }

    /// Windows-only because `Path::parent` splits these paths differently
    /// elsewhere, and the two cases that matter are both about what `std`
    /// makes of a UNC path.
    #[cfg(windows)]
    #[test]
    fn a_file_server_is_the_top_and_a_share_sits_under_it() {
        let up = |s: &str| parent_dir(Path::new(s));

        // The one that showed: `std` reads `\\host` as a root plus one
        // component, so its parent is the bare `\` — which resolves to this
        // machine's current drive, offered as the folder above a server.
        assert_eq!(up(r"\\192.0.2.10"), None);
        assert_eq!(up("//192.0.2.10"), None);
        // A share root has no parent at all to `std`; the host is above it.
        assert_eq!(up(r"\\192.0.2.10\Backup"), Some(PathBuf::from(r"\\192.0.2.10")));
        // Deeper in, and off UNC entirely, it is `parent()`'s answer.
        assert_eq!(up(r"\\192.0.2.10\Backup\2025"), Some(PathBuf::from(r"\\192.0.2.10\Backup")));
        assert_eq!(up(r"C:\dev\filer"), Some(PathBuf::from(r"C:\dev")));
        // A drive root is a top too.
        assert_eq!(up(r"C:\"), None);
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
        let share = normalize(Path::new(r"\\192.0.2.10\pub"));
        assert_eq!(share, PathBuf::from(r"\\192.0.2.10\pub"));
        assert_eq!(share.parent(), None);
        // The path is kept whole; the name shown for it is the share's own.
        assert_eq!(file_name(&share), "pub");

        // Forward slashes spell the same share.
        assert_eq!(normalize(Path::new("//192.0.2.10/pub/x")), PathBuf::from(r"\\192.0.2.10\pub\x"));

        // A host with no share yet must not collapse to `\host`.
        assert_eq!(normalize(Path::new(r"\\192.0.2.10")), PathBuf::from(r"\\192.0.2.10"));
        assert_eq!(
            resolve_against(Path::new(r"C:\work"), r"\\192.0.2.10"),
            PathBuf::from(r"\\192.0.2.10")
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
