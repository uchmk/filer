//! Text previews, syntax-highlighted in the worker so the UI thread only ever
//! paints already-colored spans.

use syntect::easy::HighlightLines;
use syntect::highlighting::{
    FontStyle, HighlightIterator, HighlightState, Highlighter as ThemeHighlighter, Style, Theme, ThemeSet,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use super::symbols::Collector;
use super::{Extent, Payload, Request, Span};

/// Syntax and theme sets are loaded on first use — a few hundred milliseconds
/// that would be wasted for someone who only ever previews images.
#[derive(Default)]
pub struct Highlighter {
    sets: Option<(SyntaxSet, ThemeSet)>,
    theme_name: String,
    theme: Option<Theme>,
}

impl Highlighter {
    fn ensure(&mut self, theme_name: &str) {
        if self.sets.is_none() {
            // bat's collection: syntect's own bundle has no TOML, TypeScript,
            // Dockerfile, Kotlin and so on.
            self.sets = Some((two_face::syntax::extra_newlines(), ThemeSet::load_defaults()));
        }
        if self.theme.is_none() || self.theme_name != theme_name {
            let (_, themes) = self.sets.as_ref().unwrap();
            let theme = themes
                .themes
                .get(theme_name)
                .or_else(|| themes.themes.get("base16-ocean.dark"))
                .or_else(|| themes.themes.values().next())
                .cloned();
            self.theme_name = theme_name.to_owned();
            self.theme = theme;
        }
    }

    /// The syntect theme, loaded if it is not already. For a previewer that does
    /// no highlighting of its own but still wants the theme's own colours.
    pub(super) fn theme(&mut self, theme_name: &str) -> Option<&Theme> {
        self.ensure(theme_name);
        self.theme.as_ref()
    }
}

pub(crate) const MAX_LINES: usize = 4000;
const MAX_LINE_CHARS: usize = 2000;

pub fn render(bytes: &[u8], req: &Request, hl: &mut Highlighter) -> Payload {
    render_cut(bytes, bytes.len() >= req.max_bytes, req, hl)
}

/// The same, told whether the read stopped short: for text decoded from
/// another encoding, whose length no longer says so.
pub fn render_cut(bytes: &[u8], cut: bool, req: &Request, hl: &mut Highlighter) -> Payload {
    let text = String::from_utf8_lossy(bytes);
    let text = strip_bom(&text);
    let expanded = expand_tabs(text, req.tab_size.max(1) as usize);
    let total_lines = expanded.lines().count();
    let extent = Extent { truncated: total_lines > MAX_LINES || cut, total: total_lines, cut, rows: None };

    hl.ensure(&req.syntect_theme);
    let Some((syntaxes, _)) = hl.sets.as_ref() else {
        return plain(&expanded, extent);
    };
    let Some(theme) = hl.theme.as_ref() else {
        return plain(&expanded, extent);
    };

    let syntax = req
        .ext
        .as_deref()
        .and_then(|e| syntaxes.find_syntax_by_extension(e))
        .or_else(|| {
            let hint = match req.ext.as_deref() {
                // Cargo.lock, poetry.lock and friends are TOML; composer.lock
                // and flake.lock are JSON.
                Some("lock") if expanded.trim_start().starts_with('{') => Some("json".into()),
                Some("lock") => Some("toml".into()),
                ext => crate::mime::syntax_hint(req.mime, ext),
            };
            hint.and_then(|h| find_token(syntaxes, &h))
        })
        .or_else(|| syntaxes.find_syntax_by_first_line(expanded.lines().next().unwrap_or("")));

    let Some(syntax) = syntax else {
        return plain(&expanded, extent);
    };

    // The Markdown grammar paints fenced code as one flat color, so fence
    // bodies are re-highlighted with the language named after the fence.
    let is_markdown = syntax.name == "Markdown";
    // The parse is kept apart from the coloring so the outline can read the
    // same scopes without parsing twice.
    let highlighter = ThemeHighlighter::new(theme);
    let mut parse = ParseState::new(syntax);
    let mut state = HighlightState::new(&highlighter, ScopeStack::new());
    let mut symbols = Collector::default();
    let mut fence: Option<Fence<'_>> = None;
    let mut lines: Vec<Vec<Span>> = Vec::with_capacity(total_lines.min(MAX_LINES));
    for (i, line) in LinesWithEndings::from(&expanded).take(MAX_LINES).enumerate() {
        // Always fed, even when its output is discarded, to keep its state in sync.
        let md = parse.parse_line(line, syntaxes).map(|ops| {
            if !is_markdown {
                symbols.feed(i, line, &ops);
            }
            HighlightIterator::new(&mut state, &ops, line, &highlighter).collect::<Vec<_>>()
        });

        let mut heading = false;
        if is_markdown {
            if let Some(active) = fence.as_mut() {
                let closes = fence_marker(line).is_some_and(|(ch, len, info)| {
                    ch == active.ch && len >= active.len && info.is_empty()
                });
                if closes {
                    fence = None;
                } else if let Some(sub) = active.hl.as_mut() {
                    lines.push(match sub.highlight_line(line, syntaxes) {
                        Ok(regions) => spans_from(regions),
                        Err(_) => plain_line(line),
                    });
                    continue;
                }
            } else if let Some((ch, len, info)) = fence_marker(line) {
                let hl = fence_syntax(syntaxes, info).map(|s| HighlightLines::new(s, theme));
                fence = Some(Fence { ch, len, hl });
            } else {
                heading = is_atx_heading(line);
            }
        }

        let mut spans = match md {
            Ok(regions) => spans_from(regions),
            Err(_) => plain_line(line),
        };
        // Bundled themes color headings but rarely embolden them.
        if heading {
            spans.iter_mut().for_each(|s| s.bold = true);
        }
        lines.push(spans);
    }
    if is_markdown {
        let (doc, clipped) = super::markdown::render(&expanded, req.key.cols, theme, syntaxes);
        let map = super::minimap(&lines);
        let extent = Extent { truncated: extent.truncated || clipped, ..extent };
        return Payload::Markdown { doc, source: lines, map, extent };
    }
    let map = super::minimap(&lines);
    Payload::Text { lines, map, extent, outline: symbols.finish() }
}

/// Resolve a language name as people write it after a fence or in a hint.
pub(super) fn find_token<'a>(syntaxes: &'a SyntaxSet, token: &str) -> Option<&'a SyntaxReference> {
    let lower = token.to_ascii_lowercase();
    let alias = match lower.as_str() {
        "text" | "txt" | "plain" | "plaintext" | "none" | "output" => return None,
        "shell" | "console" | "shellsession" | "sh-session" | "zsh" | "ksh" | "fish" => "bash",
        "jsonc" | "json5" | "jsonl" | "ndjson" => "json",
        "yml" => "yaml",
        "jsx" | "mjs" | "cjs" | "node" => "js",
        "docker" | "containerfile" => "dockerfile",
        "c++" => "cpp",
        "golang" => "go",
        "rs" => "rust",
        "py3" | "python3" => "python",
        "hcl" | "tf" => "terraform",
        "cmd" | "batch" => "bat",
        "patch" => "diff",
        "html5" | "xhtml" | "vue-html" => "html",
        "cfg" | "conf" | "dosini" => "ini",
        other => other,
    };
    syntaxes
        .find_syntax_by_token(alias)
        .or_else(|| syntaxes.find_syntax_by_token(token))
}

/// Syntax for a fence's info string (`rust`, `rust,ignore`, `{.python}`).
pub(super) fn fence_syntax<'a>(syntaxes: &'a SyntaxSet, info: &str) -> Option<&'a SyntaxReference> {
    let lang = info
        .split(|c: char| c.is_whitespace() || c == ',' || c == '{' || c == '}')
        .find(|s| !s.is_empty())?
        .trim_start_matches('.');
    find_token(syntaxes, lang)
}

struct Fence<'a> {
    ch: char,
    len: usize,
    hl: Option<HighlightLines<'a>>,
}

/// CommonMark fence line: up to 3 spaces, then 3+ backticks or tildes, then
/// an info string. Returns the fence char, its run length and the trimmed info.
fn fence_marker(line: &str) -> Option<(char, usize, &str)> {
    let line = line.trim_end_matches(['\n', '\r']);
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let ch = rest.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let len = rest.chars().take_while(|&c| c == ch).count();
    if len < 3 {
        return None;
    }
    let info = &rest[len..];
    if ch == '`' && info.contains('`') {
        return None;
    }
    Some((ch, len, info.trim()))
}

fn is_atx_heading(line: &str) -> bool {
    let line = line.trim_end_matches(['\n', '\r']);
    let rest = line.trim_start_matches(' ');
    if line.len() - rest.len() > 3 {
        return false;
    }
    let hashes = rest.bytes().take_while(|&b| b == b'#').count();
    (1..=6).contains(&hashes) && matches!(rest.as_bytes().get(hashes), None | Some(b' ' | b'\t'))
}

pub(super) fn spans_from(regions: Vec<(Style, &str)>) -> Vec<Span> {
    let mut spans = Vec::with_capacity(regions.len());
    let mut width = 0;
    for (style, piece) in regions {
        let piece = piece.trim_end_matches(['\n', '\r']);
        if piece.is_empty() {
            continue;
        }
        let piece = clip(piece, MAX_LINE_CHARS.saturating_sub(width));
        width += piece.chars().count();
        spans.push(Span {
            text: piece,
            color: Some([style.foreground.r, style.foreground.g, style.foreground.b]),
            bold: style.font_style.contains(FontStyle::BOLD),
            italic: style.font_style.contains(FontStyle::ITALIC),
            ..Default::default()
        });
        if width >= MAX_LINE_CHARS {
            break;
        }
    }
    spans
}

fn plain_line(line: &str) -> Vec<Span> {
    vec![Span { text: clip(line.trim_end_matches(['\n', '\r']), MAX_LINE_CHARS), ..Default::default() }]
}

/// Unstyled lines, capped and clipped the same way every other payload's are, so
/// a previewer that has its own rendering can still offer the raw text beside it
/// with a working minimap.
pub fn plain_lines(text: &str) -> Vec<Vec<Span>> {
    text.lines()
        .take(MAX_LINES)
        .map(|l| vec![Span { text: clip(l, MAX_LINE_CHARS), ..Default::default() }])
        .collect()
}

pub fn plain(text: &str, extent: Extent) -> Payload {
    let lines = plain_lines(text);
    let map = super::minimap(&lines);
    Payload::Text { lines, map, extent, outline: Vec::new() }
}

fn clip(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let mut out = String::with_capacity(s.len().min(max));
    for (i, c) in s.chars().enumerate() {
        if i >= max {
            out.push('…');
            break;
        }
        out.push(c);
    }
    out
}

fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{feff}').unwrap_or(s)
}

fn expand_tabs(s: &str, width: usize) -> String {
    if !s.contains('\t') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len() + 16);
    let mut col = 0;
    for c in s.chars() {
        match c {
            '\t' => {
                let n = width - (col % width);
                for _ in 0..n {
                    out.push(' ');
                }
                col += n;
            }
            '\n' => {
                out.push('\n');
                col = 0;
            }
            c => {
                out.push(c);
                col += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::Key;

    fn render_md(src: &str) -> Vec<Vec<Span>> {
        render_ext(src, "md")
    }

    fn render_ext(src: &str, ext: &str) -> Vec<Vec<Span>> {
        match render(src.as_bytes(), &request(ext), &mut Highlighter::default()) {
            Payload::Text { lines, .. } => lines,
            Payload::Markdown { source, .. } => source,
            other => panic!("unexpected payload {other:?}"),
        }
    }

    fn request(ext: &str) -> Request {
        let mime = match ext {
            "md" | "mdx" => "text/markdown",
            _ => "text/plain",
        };
        Request {
            id: 0,
            key: Key {
                path: format!("x.{ext}").into(),
                len: 0,
                mtime: None,
                box_size: (0, 0),
                cols: 80,
                n: 0,
            },
            mime,
            ext: Some(ext.into()),
            max_bytes: 1 << 20,
            tab_size: 4,
            preview: None,
            syntect_theme: "base16-ocean.dark".into(),
        }
    }

    #[test]
    fn source_files_carry_an_outline() {
        let src = "use std::fmt;\n\nstruct A;\n\nimpl A {\n\tfn go(&self) {}\n}\n";
        let Payload::Text { lines, outline, .. } = render(src.as_bytes(), &request("rs"), &mut Highlighter::default())
        else {
            panic!("not text")
        };
        let got: Vec<_> = outline.iter().map(|e| (e.line, e.label.as_str())).collect();
        assert_eq!(got, [(2, "struct A"), (4, "impl A"), (5, "  go")]);
        // Parsing once for both did not cost the colors.
        assert!(colors(&lines[5]).len() > 1, "{:?}", lines[5]);

        let Payload::Text { outline, .. } = render(b"plain words\n", &request("txt"), &mut Highlighter::default())
        else {
            panic!("not text")
        };
        assert!(outline.is_empty());
    }

    #[test]
    fn toml_and_lock_files_are_highlighted() {
        for ext in ["toml", "lock"] {
            let lines = render_ext("[package]\nname = \"filer\"\n", ext);
            assert!(colors(&lines[1]).len() > 1, "{ext}: {:?}", lines[1]);
        }
        // A JSON lockfile is not TOML.
        let lines = render_ext("{\n  \"nodes\": { \"a\": 1 }\n}\n", "lock");
        assert!(colors(&lines[1]).len() > 1);
    }

    #[test]
    fn fence_languages_resolve() {
        let mut hl = Highlighter::default();
        hl.ensure("base16-ocean.dark");
        let (syntaxes, _) = hl.sets.as_ref().unwrap();
        for (info, name) in [
            ("rust", "Rust"),
            ("rust,ignore", "Rust"),
            ("{.python}", "Python"),
            ("ts", "TypeScript"),
            ("tsx", "TypeScriptReact"),
            ("toml", "TOML"),
            ("console", "Bourne Again Shell (bash)"),
            ("sh", "Bourne Again Shell (bash)"),
            ("jsonc", "JSON"),
            ("yml", "YAML"),
            ("Dockerfile", "Dockerfile"),
            ("kt", "Kotlin"),
        ] {
            let got = fence_syntax(syntaxes, info).map(|s| s.name.as_str());
            assert_eq!(got, Some(name), "fence {info:?}");
        }
        assert!(fence_syntax(syntaxes, "text").is_none());
        assert!(fence_syntax(syntaxes, "").is_none());
    }

    fn colors(line: &[Span]) -> std::collections::HashSet<[u8; 3]> {
        line.iter().filter_map(|s| s.color).collect()
    }

    #[test]
    fn fenced_code_uses_its_language() {
        let lines = render_md("# t\n\n```rust\nfn main() { let x = \"s\"; }\n```\n\ntext\n");
        assert!(colors(&lines[3]).len() > 1, "rust body should be multi-colored: {:?}", lines[3]);
        // Unknown languages keep the markdown grammar's flat raw-block color.
        let lines = render_md("```nosuchlang\nfn main() { let x = \"s\"; }\n```\n");
        assert_eq!(colors(&lines[1]).len(), 1);
    }

    #[test]
    fn headings_are_bold() {
        let lines = render_md("## Head\n#nospace\n```sh\n# comment\n```\n");
        assert!(lines[0].iter().all(|s| s.bold));
        assert!(!lines[1].iter().any(|s| s.bold));
        assert!(!lines[3].iter().any(|s| s.bold));
    }

    #[test]
    fn mdx_highlights_as_markdown() {
        let lines = render_ext("# Title\n\ntext\n", "mdx");
        assert!(lines[0].iter().all(|s| s.color.is_some()));
    }

    #[test]
    fn fence_markers() {
        assert_eq!(fence_marker("```rust\n"), Some(('`', 3, "rust")));
        assert_eq!(fence_marker("   ~~~~ toml {x}\n"), Some(('~', 4, "toml {x}")));
        assert_eq!(fence_marker("    ```\n"), None);
        assert_eq!(fence_marker("``\n"), None);
        assert_eq!(fence_marker("``` a`b\n"), None);
    }
}
