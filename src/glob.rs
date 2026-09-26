//! Minimal glob matching for the `name = "*.rs"` / `mime = "text/*"` rules in
//! yazi's config. Supports `*`, `?`, character classes and `{a,b}` alternatives.

/// How many patterns one `{...}` pattern may become before it is given up on
/// and matched literally. Nested braces multiply, and a config is not worth
/// spending a second of the UI thread on.
const MAX_ALTERNATIVES: usize = 64;

pub fn matches(pattern: &str, text: &str, case_insensitive: bool) -> bool {
    expand(pattern)
        .iter()
        .any(|p| one(p, text, case_insensitive))
}

fn one(pattern: &str, text: &str, case_insensitive: bool) -> bool {
    if case_insensitive {
        let p = pattern.to_lowercase();
        let t = text.to_lowercase();
        imp(p.as_bytes(), t.as_bytes())
    } else {
        imp(pattern.as_bytes(), text.as_bytes())
    }
}

/// `*.{jpg,png}` into `*.jpg` and `*.png`.
///
/// Yazi's own rules are written this way, and a config copied from there used
/// to match nothing at all here — silently, since a rule that matches nothing
/// looks exactly like a file type nobody configured.
///
/// Braces are expanded before matching rather than handled inside the matcher:
/// the matcher backtracks, and alternatives that can themselves contain `*`
/// make that a much harder problem than repeating a linear match a few times.
fn expand(pattern: &str) -> Vec<String> {
    let b = pattern.as_bytes();
    let Some(open) = b.iter().position(|&c| c == b'{') else {
        return vec![pattern.to_owned()];
    };
    // The `}` that closes *this* `{`, counting the ones in between.
    let mut depth = 0usize;
    let mut close = None;
    for (i, &c) in b.iter().enumerate().skip(open) {
        match c {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    // Unbalanced: a literal brace, which is a legal character in a file name.
    let Some(close) = close else {
        return vec![pattern.to_owned()];
    };

    let (head, tail) = (&pattern[..open], &pattern[close + 1..]);
    let mut out = Vec::new();
    for alt in split_top_level(&pattern[open + 1..close]) {
        for rest in expand(&format!("{head}{alt}{tail}")) {
            if out.len() >= MAX_ALTERNATIVES {
                return vec![pattern.to_owned()];
            }
            out.push(rest);
        }
    }
    out
}

/// The commas that separate this brace's own alternatives, not a nested one's.
fn split_top_level(inner: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in inner.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&inner[start..]);
    out
}

fn imp(p: &[u8], t: &[u8]) -> bool {
    // Iterative backtracking matcher: linear for patterns without `*`.
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() {
            match p[pi] {
                b'*' => {
                    star = Some((pi, ti));
                    pi += 1;
                    continue;
                }
                b'?' => {
                    pi += 1;
                    ti += 1;
                    continue;
                }
                b'[' => {
                    if let Some((end, ok)) = class(&p[pi..], t[ti]) {
                        if ok {
                            pi += end;
                            ti += 1;
                            continue;
                        }
                    }
                }
                c if c == t[ti] => {
                    pi += 1;
                    ti += 1;
                    continue;
                }
                _ => {}
            }
        }
        match star {
            Some((sp, st)) => {
                pi = sp + 1;
                ti = st + 1;
                star = Some((sp, st + 1));
            }
            None => return false,
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

/// Returns (bytes consumed, matched) for a `[...]` class at the start of `p`.
fn class(p: &[u8], c: u8) -> Option<(usize, bool)> {
    let mut i = 1;
    let negate = p.get(i) == Some(&b'!') || p.get(i) == Some(&b'^');
    if negate {
        i += 1;
    }
    let mut hit = false;
    let mut first = true;
    while i < p.len() && (p[i] != b']' || first) {
        first = false;
        if i + 2 < p.len() && p[i + 1] == b'-' && p[i + 2] != b']' {
            if p[i] <= c && c <= p[i + 2] {
                hit = true;
            }
            i += 3;
        } else {
            if p[i] == c {
                hit = true;
            }
            i += 1;
        }
    }
    if i >= p.len() {
        return None; // unterminated class: treat as literal
    }
    Some((i + 1, hit != negate))
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn basics() {
        assert!(matches("*.rs", "main.rs", true));
        assert!(!matches("*.rs", "main.rss", true));
        assert!(matches("text/*", "text/plain", true));
        assert!(!matches("Cargo.???", "Cargo.toml", true));
        assert!(matches("Cargo.????", "Cargo.toml", true));
        assert!(matches("*", "anything", true));
        assert!(matches("[abc]x", "bx", true));
        assert!(!matches("[!abc]x", "bx", true));
    }

    /// The form yazi's own `[open]` rules are written in. Without it a config
    /// copied from there matches nothing, and looks like a file type no one
    /// configured.
    #[test]
    fn alternatives() {
        assert!(matches("*.{jpg,png}", "photo.png", true));
        assert!(matches("*.{jpg,png}", "photo.JPG", true));
        assert!(!matches("*.{jpg,png}", "photo.gif", true));
        assert!(matches("*.{xlsx,xls,csv}", "支払.csv", true));
        // Nested, and an empty alternative meaning "or nothing at all".
        assert!(matches("*.{tar.{gz,bz2},zip}", "src.tar.bz2", true));
        assert!(matches("*.{tar.{gz,bz2},zip}", "src.zip", true));
        assert!(matches("a{,b}c", "ac", true));
        assert!(matches("a{,b}c", "abc", true));
        // A brace is a legal character in a file name; an unbalanced one is
        // itself rather than a syntax error.
        assert!(matches("{unclosed", "{unclosed", true));
        assert!(matches("*}", "odd}", true));
    }

    /// `expand` stops rather than producing a pattern list the length of a
    /// combinatorial explosion; the pattern then stands for itself.
    #[test]
    fn a_runaway_pattern_is_not_expanded() {
        let big = "{a,b}{a,b}{a,b}{a,b}{a,b}{a,b}{a,b}"; // 128 alternatives
        assert_eq!(super::expand(big), vec![big.to_owned()]);
        assert!(matches("{a,b}{a,b}{a,b}", "aba", true)); // 8 is fine
    }
}
