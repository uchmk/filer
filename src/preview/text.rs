//! Text previews, syntax-highlighted in the worker so the UI thread only ever
//! paints already-colored spans.

use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use super::{Payload, Request, Span};

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
            self.sets = Some((SyntaxSet::load_defaults_newlines(), ThemeSet::load_defaults()));
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
}

const MAX_LINES: usize = 4000;
const MAX_LINE_CHARS: usize = 2000;

pub fn render(bytes: &[u8], req: &Request, hl: &mut Highlighter) -> Payload {
    let text = String::from_utf8_lossy(bytes);
    let text = strip_bom(&text);
    let expanded = expand_tabs(text, req.tab_size.max(1) as usize);
    let total_lines = expanded.lines().count();
    let truncated = total_lines > MAX_LINES || bytes.len() >= req.max_bytes;

    hl.ensure(&req.syntect_theme);
    let Some((syntaxes, _)) = hl.sets.as_ref() else {
        return plain(&expanded, truncated, total_lines);
    };
    let Some(theme) = hl.theme.as_ref() else {
        return plain(&expanded, truncated, total_lines);
    };

    let syntax = req
        .ext
        .as_deref()
        .and_then(|e| syntaxes.find_syntax_by_extension(e))
        .or_else(|| {
            crate::mime::syntax_hint(req.mime, req.ext.as_deref())
                .and_then(|h| syntaxes.find_syntax_by_token(&h))
        })
        .or_else(|| syntaxes.find_syntax_by_first_line(expanded.lines().next().unwrap_or("")));

    let Some(syntax) = syntax else {
        return plain(&expanded, truncated, total_lines);
    };

    let mut h = HighlightLines::new(syntax, theme);
    let mut lines: Vec<Vec<Span>> = Vec::with_capacity(total_lines.min(MAX_LINES));
    for line in LinesWithEndings::from(&expanded).take(MAX_LINES) {
        match h.highlight_line(line, syntaxes) {
            Ok(regions) => {
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
                    });
                    if width >= MAX_LINE_CHARS {
                        break;
                    }
                }
                lines.push(spans);
            }
            Err(_) => lines.push(vec![Span {
                text: clip(line.trim_end_matches(['\n', '\r']), MAX_LINE_CHARS),
                color: None,
            }]),
        }
    }
    Payload::Text { lines, truncated, total_lines }
}

fn plain(text: &str, truncated: bool, total_lines: usize) -> Payload {
    let lines = text
        .lines()
        .take(MAX_LINES)
        .map(|l| vec![Span { text: clip(l, MAX_LINE_CHARS), color: None }])
        .collect();
    Payload::Text { lines, truncated, total_lines }
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
