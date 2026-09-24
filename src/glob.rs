//! Minimal glob matching for the `name = "*.rs"` / `mime = "text/*"` rules in
//! yazi's config. Supports `*`, `?` and character classes.

pub fn matches(pattern: &str, text: &str, case_insensitive: bool) -> bool {
    if case_insensitive {
        let p = pattern.to_lowercase();
        let t = text.to_lowercase();
        imp(p.as_bytes(), t.as_bytes())
    } else {
        imp(pattern.as_bytes(), text.as_bytes())
    }
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
}
