//! Text previews, syntax-highlighted in the worker so the UI thread only ever
//! paints already-colored spans.

use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{
    FontStyle, HighlightIterator, HighlightState, Highlighter as ThemeHighlighter, Style, Theme, ThemeSet,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use super::symbols::Collector;
use super::{Extent, Payload, Request, Span};

/// Syntax and theme sets, loaded on first use — a few hundred milliseconds
/// that would be wasted for someone who only ever previews images — and once
/// per process. Every `App` starts a worker, and each loading its own copy
/// made the test binary seven times slower: Windows CI timed out (v0.86.12).
/// A grammar's regexes are compiled inside the set, so they are shared too.
pub(super) fn sets() -> &'static (SyntaxSet, ThemeSet) {
    static SETS: OnceLock<(SyntaxSet, ThemeSet)> = OnceLock::new();
    // bat's collection: syntect's own bundle has no TOML, TypeScript,
    // Dockerfile, Kotlin and so on.
    SETS.get_or_init(|| (two_face::syntax::extra_newlines(), ThemeSet::load_defaults()))
}

#[derive(Default)]
pub struct Highlighter {
    theme_name: String,
    theme: Option<Theme>,
    /// Handed the first screen of a long text before the rest is colored, then
    /// asked with `None` every few lines after, and answers whether to go on:
    /// coloring every line is what a first look at a big file waited for
    /// (TODO.md took 0.65 s), and a cached second look did not. Set by the
    /// worker for each request.
    pub(super) early: Option<Box<dyn FnMut(Option<Payload>) -> bool>>,
    /// The last render stopped after its first screen because `early` said a
    /// newer request was waiting; what it returned is not worth sending.
    pub(super) abandoned: bool,
}

impl Highlighter {
    /// Parses a little Markdown, with fences of the languages most looked at,
    /// while `idle` says nothing is asked for. A grammar's rules are read and
    /// its regexes compiled the first time they are tried, which made the first
    /// Markdown file of a session take 0.15 s longer than the next one. Returns
    /// whether it got through; one stopped early starts again next time, the
    /// part already done costing nearly nothing, as it does for every worker
    /// after the first in a process.
    pub(super) fn warm(&mut self, idle: &dyn Fn() -> bool) -> bool {
        const SAMPLE: &str = include_str!("warm-up.md");
        let (syntaxes, _) = sets();
        let Some(md) = syntaxes.find_syntax_by_extension("md") else { return true };
        let mut parse = ParseState::new(md);
        for line in LinesWithEndings::from(SAMPLE) {
            if !idle() {
                return false;
            }
            if parse.parse_line(line, syntaxes).is_err() {
                return true;
            }
        }
        true
    }

    fn ensure(&mut self, theme_name: &str) {
        if self.theme.is_none() || self.theme_name != theme_name {
            let (_, themes) = sets();
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
/// The same for a deep preview (see `Key::deep`): a few megabytes of source.
pub(crate) const DEEP_MAX_LINES: usize = 100_000;
/// Lines of a deep preview that are coloured. The rest are shown plain: they
/// are there to be found and read, and colouring a hundred thousand lines is
/// seconds of the worker's time for lines nobody asked to see coloured.
const DEEP_COLOR_LINES: usize = if cfg!(test) { 200 } else { 40_000 };

fn max_lines(req: &Request) -> usize {
    if req.key.deep { DEEP_MAX_LINES } else { MAX_LINES }
}

fn color_lines(req: &Request) -> usize {
    if req.key.deep { DEEP_COLOR_LINES } else { MAX_LINES }
}

/// Lines colored before `Highlighter::early` is handed a first screen: about
/// what a full-height pane shows. Any further down are colored a moment later.
const HEAD_LINES: usize = 80;
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
    let extent = Extent { truncated: total_lines > max_lines(req) || cut, total: total_lines, cut, rows: None };

    hl.ensure(&req.syntect_theme);
    let (syntaxes, _) = sets();
    let Some(theme) = hl.theme.as_ref() else {
        return plain_to(&expanded, extent, max_lines(req));
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
        return plain_to(&expanded, extent, max_lines(req));
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
    let mut lines: Vec<Vec<Span>> = Vec::with_capacity(total_lines.min(max_lines(req)));
    // Laid out once: the first screen of a long Markdown file carries all of
    // it, which is cheap next to coloring the source beside it.
    let mut doc = None;
    let head = if is_markdown && req.markdown_rendered { 0 } else { HEAD_LINES };
    for (i, line) in LinesWithEndings::from(&expanded).take(max_lines(req)).enumerate() {
        if i >= color_lines(req) {
            lines.extend(LinesWithEndings::from(&expanded).take(max_lines(req)).skip(i).map(plain_line));
            break;
        }
        // Past the first screen, a newer request stops the rest.
        if i > head && i % 32 == 0 && total_lines > HEAD_LINES {
            if let Some(early) = hl.early.as_mut() {
                if !early(None) {
                    hl.abandoned = true;
                    return Payload::Error("superseded".into());
                }
            }
        }
        if i == head && total_lines > HEAD_LINES {
            if let Some(early) = hl.early.as_mut() {
                // Every line is there, the rest uncolored for now, so the
                // scroll range and the minimap's length do not change under
                // the reader when the colors arrive.
                let mut source = lines.clone();
                source.extend(LinesWithEndings::from(&expanded).take(max_lines(req)).skip(i).map(plain_line));
                let map = super::minimap(&source);
                let payload = if is_markdown {
                    let (d, clipped) = doc.get_or_insert_with(|| super::markdown::render(&expanded, req.key.cols, theme, syntaxes));
                    let extent = Extent { truncated: extent.truncated || *clipped, ..extent };
                    Payload::Markdown { doc: d.clone(), source, map, extent }
                } else {
                    Payload::Text { lines: source, map, extent, outline: symbols.so_far() }
                };
                if !early(Some(payload)) {
                    hl.abandoned = true;
                    return Payload::Error("superseded".into());
                }
            }
        }
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
        let (doc, clipped) = doc.unwrap_or_else(|| super::markdown::render(&expanded, req.key.cols, theme, syntaxes));
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
    plain_lines_to(text, MAX_LINES)
}

fn plain_lines_to(text: &str, max: usize) -> Vec<Vec<Span>> {
    text.lines()
        .take(max)
        .map(|l| vec![Span { text: clip(l, MAX_LINE_CHARS), ..Default::default() }])
        .collect()
}

pub fn plain(text: &str, extent: Extent) -> Payload {
    plain_to(text, extent, MAX_LINES)
}

fn plain_to(text: &str, extent: Extent, max: usize) -> Payload {
    let lines = plain_lines_to(text, max);
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
                deep: false,
            },
            mime,
            ext: Some(ext.into()),
            max_bytes: 1 << 20,
            tab_size: 4,
            preview: None,
            syntect_theme: "base16-ocean.dark".into(),
            markdown_rendered: false,
        }
    }

    /// The result of a content search reads as far as the search did, so a match
    /// near the end of a long file is on show to be coloured and walked to.
    #[test]
    fn a_deep_preview_keeps_the_lines_past_the_usual_cut() {
        let src = "line\n".repeat(MAX_LINES + 500);
        let count = |deep: bool| {
            let mut req = request("txt");
            req.key.deep = deep;
            match render(src.as_bytes(), &req, &mut Highlighter::default()) {
                Payload::Text { lines, .. } => lines.len(),
                other => panic!("unexpected payload {other:?}"),
            }
        };
        assert_eq!(count(false), MAX_LINES);
        assert_eq!(count(true), MAX_LINES + 500);
    }

    /// Past the coloured stretch a deep preview carries on in one plain colour:
    /// every line is there to be found, the far ones are not painted.
    #[test]
    fn a_deep_preview_colours_only_its_first_stretch() {
        let src = "fn a() { let x = 1; }
".repeat(DEEP_COLOR_LINES + 50);
        let mut req = request("rs");
        req.key.deep = true;
        let Payload::Text { lines, .. } = render(src.as_bytes(), &req, &mut Highlighter::default()) else {
            panic!("not text")
        };
        assert_eq!(lines.len(), DEEP_COLOR_LINES + 50);
        assert!(colors(&lines[10]).len() > 1, "{:?}", lines[10]);
        assert!(colors(&lines[DEEP_COLOR_LINES + 10]).is_empty(), "{:?}", lines[DEEP_COLOR_LINES + 10]);
        assert_eq!(lines[DEEP_COLOR_LINES + 10].iter().map(|s| s.text.as_str()).collect::<String>(), "fn a() { let x = 1; }");
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
            let lines = render_ext("[package]\nname = \"kura\"\n", ext);
            assert!(colors(&lines[1]).len() > 1, "{ext}: {:?}", lines[1]);
        }
        // A JSON lockfile is not TOML.
        let lines = render_ext("{\n  \"nodes\": { \"a\": 1 }\n}\n", "lock");
        assert!(colors(&lines[1]).len() > 1);
    }

    #[test]
    fn the_sets_are_loaded_once_per_process() {
        // Each worker loading its own made the tests time out on Windows CI.
        assert!(std::ptr::eq(sets(), sets()));
        let a = std::thread::spawn(|| sets() as *const _ as usize).join().unwrap();
        assert_eq!(a, sets() as *const _ as usize);
    }

    #[test]
    fn fence_languages_resolve() {
        let (syntaxes, _) = sets();
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

    /// The first screen of a long file goes out as soon as it is colored: every
    /// line is in it, the ones past the head still plain, and the whole one that
    /// follows colors them without moving anything.
    #[test]
    fn a_long_file_sends_its_first_screen_early() {
        let src: String = (0..HEAD_LINES * 2).map(|i| format!("fn f{i}() {{ let x = {i}; }}\n")).collect();
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut hl = Highlighter::default();
        let into = sent.clone();
        hl.early = Some(Box::new(move |p| {
            into.borrow_mut().extend(p);
            true
        }));
        let Payload::Text { lines: whole, outline, .. } = render(src.as_bytes(), &request("rs"), &mut hl) else {
            panic!("text expected")
        };
        assert!(!hl.abandoned);
        let sent = sent.borrow();
        assert_eq!(sent.len(), 1, "one first screen, then the whole");
        let Payload::Text { lines: head, outline: head_outline, .. } = &sent[0] else { panic!("text expected") };
        assert_eq!(head.len(), whole.len(), "the scroll range does not change");
        assert_eq!(head[..HEAD_LINES], whole[..HEAD_LINES], "the head is colored already");
        assert_eq!(head[HEAD_LINES].len(), 1, "the rest waits, plain: {:?}", head[HEAD_LINES]);
        assert_eq!(head[HEAD_LINES][0].color, None);
        assert_eq!(head_outline.len(), HEAD_LINES);
        assert_eq!(outline.len(), HEAD_LINES * 2);
    }

    /// Told a newer request is waiting, whether as the first screen goes out or
    /// while the rest is colored, it stops there.
    #[test]
    fn a_newer_request_stops_the_rest() {
        let src = "line\n".repeat(HEAD_LINES * 2);
        let mut hl = Highlighter { early: Some(Box::new(|_| false)), ..Default::default() };
        render(src.as_bytes(), &request("md"), &mut hl);
        assert!(std::mem::take(&mut hl.abandoned));

        let asked = std::rc::Rc::new(std::cell::Cell::new(0));
        let n = asked.clone();
        hl.early = Some(Box::new(move |p| {
            n.set(n.get() + usize::from(p.is_none()));
            p.is_some()
        }));
        render(src.as_bytes(), &request("md"), &mut hl);
        assert!(hl.abandoned);
        assert_eq!(asked.get(), 1, "stopped at the first question");
    }

    /// A short file has nothing to send early, and Markdown's first screen
    /// carries the whole laid-out document.
    #[test]
    fn a_short_file_is_sent_once_and_markdown_whole() {
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut hl = Highlighter::default();
        let n = calls.clone();
        hl.early = Some(Box::new(move |p| {
            n.set(n.get() + usize::from(p.is_some()));
            true
        }));
        render(b"# a\n\nb\n", &request("md"), &mut hl);
        assert_eq!(calls.get(), 0);

        let src: String = (0..HEAD_LINES).map(|i| format!("# H{i}\n\ntext\n\n")).collect();
        let first = std::rc::Rc::new(std::cell::RefCell::new(None));
        let into = first.clone();
        hl.early = Some(Box::new(move |p| {
            if p.is_some() {
                *into.borrow_mut() = p;
            }
            true
        }));
        let Payload::Markdown { doc, .. } = render(src.as_bytes(), &request("md"), &mut hl) else { panic!("markdown") };
        let Some(Payload::Markdown { doc: early, .. }) = first.borrow_mut().take() else { panic!("markdown first") };
        assert_eq!(early.toc.len(), HEAD_LINES);
        assert_eq!(early.toc.len(), doc.toc.len());
        assert_eq!(early.lines.len(), doc.lines.len());

        // Drawn laid out, its source is not on screen: none of it waits to be colored.
        let mut req = request("md");
        req.markdown_rendered = true;
        let into = first.clone();
        hl.early = Some(Box::new(move |p| {
            if p.is_some() {
                *into.borrow_mut() = p;
            }
            true
        }));
        render(src.as_bytes(), &req, &mut hl);
        let Some(Payload::Markdown { doc: early, source, .. }) = first.borrow_mut().take() else { panic!("markdown first") };
        assert_eq!(early.toc.len(), HEAD_LINES);
        assert!(source.iter().all(|l| l.iter().all(|s| s.color.is_none())));
    }
}
