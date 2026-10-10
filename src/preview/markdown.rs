//! Markdown laid out for reading, the way a viewer like leaf shows it:
//! wrapped paragraphs, list markers, quote bars, aligned tables, highlighted
//! code, and an outline of the headings for a side panel.
//!
//! Everything sits on the same monospace cell grid as the rest of the preview,
//! so widths are counted in columns (CJK = 2) rather than measured.

use std::ops::Range;

use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, MetadataBlockKind, Options, Parser, Tag, TagEnd,
};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Highlighter, Theme};
use syntect::parsing::{Scope, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;
use unicode_width::UnicodeWidthChar;

use super::text::{fence_syntax, find_token, spans_from};
use super::{Doc, DocLine, LineKind, Span, TocEntry};

/// Rendered lines kept; the source view still has the whole file.
const MAX_LINES: usize = 10_000;
/// Prose wider than this is hard to read, however wide the pane.
const MAX_BODY: u16 = 100;
/// Deeper headings stay out of the outline.
const TOC_LEVELS: u8 = 4;
/// Outline labels are elided to the column when drawn; this only bounds them.
const MAX_TOC_LABEL: usize = 64;
const BULLETS: [&str; 3] = ["• ", "◦ ", "▪ "];

/// Characters a line may not start with (Japanese line-breaking rules)...
const NO_START: &str = "、。，．・：；？！ー々ゝゞヽヾぁぃぅぇぉっゃゅょゎゕゖァィゥェォッャュョヮヵヶ）〕］｝〉》」』】〙〗’”,.:;?!)]}%";
/// ...and ones it may not end with.
const NO_END: &str = "（〔［｛〈《「『【〘〖‘“([{";

/// Lay out `text` for a pane `cols` cells wide. The flag is set when the
/// document was cut short.
pub fn render(text: &str, cols: u16, theme: &Theme, syntaxes: &SyntaxSet) -> (Doc, bool) {
    let opts = options();
    let cols = if cols == 0 { 80 } else { cols };

    // The outline's width decides how much room the body gets, so headings
    // are collected before anything is laid out.
    let headings = outline(text, opts);
    let top = headings.iter().map(|h| h.0).min().unwrap_or(1);
    let labels: Vec<String> = headings
        .iter()
        .map(|(level, title)| format!("{}{title}", "  ".repeat((level - top) as usize)))
        .collect();
    let named = headings.iter().filter(|h| !h.1.is_empty()).count();
    let toc_cols = if cols >= 80 && named >= 2 {
        super::outline_cols(labels.iter().map(|l| str_width(l)).max().unwrap_or(0), cols)
    } else {
        0
    };
    let body_cols = if toc_cols > 0 { cols - toc_cols - 3 } else { cols }.min(MAX_BODY);

    let mut b = Builder::new(text, theme, syntaxes, body_cols as usize);
    b.run(text, opts);

    // Kept without a column too: a narrow pane shows it over the text when
    // the outline is focused.
    let toc = b
        .toc_lines
        .iter()
        .zip(headings.iter().zip(&labels))
        .filter(|(_, ((_, title), _))| !title.is_empty())
        .map(|(&line, ((level, _), label))| TocEntry { level: *level, label: clip_width(label, MAX_TOC_LABEL), line })
        .collect();
    let clipped = b.clipped;
    (Doc { lines: b.lines, toc, toc_cols, body_cols }, clipped)
}

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
}

/// Level and plain-text title of every heading the outline can show.
fn outline(text: &str, opts: Options) -> Vec<(u8, String)> {
    let mut out = Vec::new();
    let mut open: Option<(u8, String)> = None;
    for event in Parser::new_ext(text, opts) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => open = Some((level as u8, String::new())),
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, title)) = open.take().filter(|h| h.0 <= TOC_LEVELS) {
                    out.push((level, title.split_whitespace().collect::<Vec<_>>().join(" ")));
                }
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, title)) = open.as_mut() {
                    title.push_str(&t);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, title)) = open.as_mut() {
                    title.push(' ');
                }
            }
            _ => {}
        }
    }
    out
}

/// Colors come from the syntax theme so the page matches the source view.
#[derive(Clone, Copy, Default)]
struct Palette {
    heading: Option<[u8; 3]>,
    code: Option<[u8; 3]>,
    link: Option<[u8; 3]>,
    quote: Option<[u8; 3]>,
    marker: Option<[u8; 3]>,
    dim: Option<[u8; 3]>,
    /// Note, Tip, Important, Warning, Caution.
    alerts: [Option<[u8; 3]>; 5],
}

impl Palette {
    fn new(theme: &Theme) -> Self {
        let hl = Highlighter::new(theme);
        let base = rgb(hl.get_default().foreground);
        // A scope the theme leaves at the default foreground falls through to
        // the next candidate.
        let pick = |scopes: &[&str]| {
            scopes.iter().find_map(|s| {
                let style = hl.style_for_stack(&[Scope::new(s).ok()?]);
                Some(rgb(style.foreground)).filter(|&c| c != base)
            })
        };
        Self {
            heading: pick(&["markup.heading", "entity.name.function"]),
            code: pick(&["markup.raw.inline", "markup.raw", "string"]),
            link: pick(&["markup.underline.link", "support.function", "entity.name.function"]),
            quote: pick(&["markup.quote", "comment"]),
            marker: pick(&["punctuation.definition.list_item", "markup.list", "keyword"]),
            dim: pick(&["comment"]),
            alerts: [
                pick(&["entity.name.function"]),
                pick(&["markup.inserted", "string"]),
                pick(&["keyword"]),
                pick(&["constant.numeric"]),
                pick(&["markup.deleted", "variable"]),
            ],
        }
    }

    fn alert(&self, kind: BlockQuoteKind) -> Option<[u8; 3]> {
        self.alerts[alert_index(kind)].or(self.quote)
    }
}

fn alert_index(kind: BlockQuoteKind) -> usize {
    match kind {
        BlockQuoteKind::Note => 0,
        BlockQuoteKind::Tip => 1,
        BlockQuoteKind::Important => 2,
        BlockQuoteKind::Warning => 3,
        BlockQuoteKind::Caution => 4,
    }
}

fn rgb(c: syntect::highlighting::Color) -> [u8; 3] {
    [c.r, c.g, c.b]
}

/// Something every line inside it is prefixed with.
enum Container {
    /// A bar in this color.
    Quote(Option<[u8; 3]>),
    /// A list marker on the first line, blanks of the same width after.
    Item { marker: Option<Vec<Span>>, width: usize },
}

impl Container {
    fn width(&self) -> usize {
        match self {
            Container::Quote(_) => 2,
            Container::Item { width, .. } => *width,
        }
    }
}

struct Code<'a> {
    text: String,
    syntax: Option<&'a SyntaxReference>,
    /// Source line of the first line of code.
    src: usize,
}

struct Table {
    aligns: Vec<Alignment>,
    rows: Vec<Row>,
}

/// One row of a table. `pub(super)` so a sibling previewer can lay a table of
/// its own out through [`table_lines`] -- see `csv.rs`.
pub(super) struct Row {
    /// One cell per column; each is styled spans paired with their source line.
    pub cells: Vec<Vec<(Span, usize)>>,
    pub head: bool,
    pub src: usize,
}

struct Builder<'a> {
    theme: &'a Theme,
    syntaxes: &'a SyntaxSet,
    pal: Palette,
    body: usize,
    line_starts: Vec<usize>,

    lines: Vec<DocLine>,
    /// First line of each heading the outline shows, in order.
    toc_lines: Vec<usize>,
    clipped: bool,

    containers: Vec<Container>,
    /// Next number for ordered lists, `None` for bullets.
    lists: Vec<Option<u64>>,
    /// Blank lines separate blocks, but only once the next block turns up.
    need_blank: bool,
    last_blank: bool,

    /// Inline content of the block being built, with each piece's source line.
    pending: Vec<(Span, usize)>,
    soft: bool,
    strong: u32,
    emph: u32,
    strike: u32,
    link: u32,
    html_bold: u32,
    html_italic: u32,
    html_strike: u32,
    heading: Option<u8>,

    /// Alt text and URL of an image being read.
    image: Option<(String, String)>,
    code: Option<Code<'a>>,
    html: Option<(String, usize)>,
    table: Option<Table>,
}

impl<'a> Builder<'a> {
    fn new(text: &str, theme: &'a Theme, syntaxes: &'a SyntaxSet, body: usize) -> Self {
        Self {
            theme,
            syntaxes,
            pal: Palette::new(theme),
            body,
            line_starts: std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1)).collect(),
            lines: Vec::new(),
            toc_lines: Vec::new(),
            clipped: false,
            containers: Vec::new(),
            lists: Vec::new(),
            need_blank: false,
            last_blank: false,
            pending: Vec::new(),
            soft: false,
            strong: 0,
            emph: 0,
            strike: 0,
            link: 0,
            html_bold: 0,
            html_italic: 0,
            html_strike: 0,
            heading: None,
            image: None,
            code: None,
            html: None,
            table: None,
        }
    }

    fn src_line(&self, offset: usize) -> usize {
        self.line_starts.partition_point(|&s| s <= offset).saturating_sub(1)
    }

    fn run(&mut self, text: &str, opts: Options) {
        for (event, range) in Parser::new_ext(text, opts).into_offset_iter() {
            if self.clipped {
                break;
            }
            let src = self.src_line(range.start);
            match event {
                Event::Start(tag) => self.start(tag, src),
                Event::End(tag) => self.end(tag, src),
                Event::Text(t) => self.text(&t, src),
                Event::Code(t) | Event::InlineMath(t) | Event::DisplayMath(t) => self.inline_code(&t, src),
                Event::Html(t) => match self.html.as_mut() {
                    Some((buf, _)) => buf.push_str(&t),
                    None => self.inline_html(&t, src),
                },
                Event::InlineHtml(t) => self.inline_html(&t, src),
                Event::FootnoteReference(label) => {
                    let span = Span { text: format!("[{label}]"), color: self.pal.link, ..Default::default() };
                    self.push(span, src);
                }
                Event::SoftBreak => match self.image.as_mut() {
                    Some((alt, _)) => alt.push(' '),
                    None => self.soft = !self.pending.is_empty(),
                },
                Event::HardBreak => self.push(Span { text: "\n".into(), ..Default::default() }, src),
                Event::Rule => {
                    self.flush(LineKind::Text);
                    self.push_line(Vec::new(), LineKind::Rule, src);
                    self.need_blank = true;
                }
                Event::TaskListMarker(done) => self.task(done, src),
            }
        }
        self.flush(LineKind::Text);
    }

    fn start(&mut self, tag: Tag<'_>, src: usize) {
        match tag {
            Tag::Paragraph => self.flush(LineKind::Text),
            Tag::Heading { level, .. } => {
                self.flush(LineKind::Text);
                self.heading = Some(level as u8);
            }
            Tag::BlockQuote(kind) => {
                self.flush(LineKind::Text);
                let color = kind.map_or(self.pal.quote, |k| self.pal.alert(k));
                self.containers.push(Container::Quote(color));
                if let Some(kind) = kind {
                    let label = ["Note", "Tip", "Important", "Warning", "Caution"][alert_index(kind)];
                    let span = Span { text: label.into(), color, bold: true, ..Default::default() };
                    self.push_line(vec![span], LineKind::Text, src);
                }
            }
            Tag::CodeBlock(kind) => {
                self.flush(LineKind::Text);
                let (syntax, first) = match kind {
                    CodeBlockKind::Fenced(info) => (fence_syntax(self.syntaxes, &info), src + 1),
                    CodeBlockKind::Indented => (None, src),
                };
                self.code = Some(Code { text: String::new(), syntax, src: first });
            }
            Tag::MetadataBlock(kind) => {
                self.flush(LineKind::Text);
                let lang = match kind {
                    MetadataBlockKind::YamlStyle => "yaml",
                    MetadataBlockKind::PlusesStyle => "toml",
                };
                let syntax = find_token(self.syntaxes, lang);
                self.code = Some(Code { text: String::new(), syntax, src: src + 1 });
            }
            Tag::HtmlBlock => {
                self.flush(LineKind::Text);
                self.html = Some((String::new(), src));
            }
            Tag::List(first) => {
                self.flush(LineKind::Text);
                self.lists.push(first);
            }
            Tag::Item => {
                self.flush(LineKind::Text);
                let depth = self.lists.len().saturating_sub(1);
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let m = format!("{n}. ");
                        *n += 1;
                        m
                    }
                    _ => BULLETS[depth % BULLETS.len()].to_owned(),
                };
                self.open_item(marker, self.pal.marker);
            }
            Tag::FootnoteDefinition(label) => {
                self.flush(LineKind::Text);
                self.open_item(format!("[{label}] "), self.pal.dim);
            }
            Tag::Table(aligns) => {
                self.flush(LineKind::Text);
                self.table = Some(Table { aligns, rows: Vec::new() });
            }
            Tag::TableHead => self.open_row(true, src),
            Tag::TableRow => self.open_row(false, src),
            Tag::TableCell => self.pending.clear(),
            Tag::Emphasis => self.emph += 1,
            Tag::Strong => self.strong += 1,
            Tag::Strikethrough => self.strike += 1,
            Tag::Link { .. } => self.link += 1,
            Tag::Image { dest_url, .. } => self.image = Some((String::new(), dest_url.into_string())),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd, src: usize) {
        match tag {
            TagEnd::Paragraph => {
                self.flush(LineKind::Text);
                self.need_blank = true;
            }
            TagEnd::Heading(level) => {
                let level = level as u8;
                self.settle(src);
                let first = self.lines.len();
                self.flush(LineKind::Heading(level));
                self.heading = None;
                if level <= TOC_LEVELS {
                    self.toc_lines.push(first);
                }
                self.need_blank = true;
            }
            TagEnd::BlockQuote(_) => {
                self.flush(LineKind::Text);
                self.containers.pop();
                self.need_blank = true;
            }
            TagEnd::CodeBlock | TagEnd::MetadataBlock(_) => {
                if let Some(code) = self.code.take() {
                    self.code_block(code);
                }
                self.need_blank = true;
            }
            TagEnd::HtmlBlock => {
                if let Some((html, src)) = self.html.take() {
                    self.html_block(&html, src);
                }
            }
            TagEnd::List(_) => {
                self.flush(LineKind::Text);
                self.lists.pop();
                // Nested lists run on into their parent item.
                if !self.containers.iter().any(|c| matches!(c, Container::Item { .. })) {
                    self.need_blank = true;
                }
            }
            TagEnd::Item | TagEnd::FootnoteDefinition => {
                self.flush(LineKind::Text);
                // An empty item still shows its marker.
                if let Some(Container::Item { marker: Some(_), .. }) = self.containers.last() {
                    self.push_line(Vec::new(), LineKind::Text, src);
                }
                self.containers.pop();
                if tag == TagEnd::FootnoteDefinition {
                    self.need_blank = true;
                }
            }
            TagEnd::Table => {
                if let Some(table) = self.table.take() {
                    self.table_block(table);
                }
                self.need_blank = true;
            }
            TagEnd::TableCell => {
                let cell = std::mem::take(&mut self.pending);
                self.soft = false;
                if let Some(row) = self.table.as_mut().and_then(|t| t.rows.last_mut()) {
                    row.cells.push(cell);
                }
            }
            TagEnd::Emphasis => self.emph = self.emph.saturating_sub(1),
            TagEnd::Strong => self.strong = self.strong.saturating_sub(1),
            TagEnd::Strikethrough => self.strike = self.strike.saturating_sub(1),
            TagEnd::Link => self.link = self.link.saturating_sub(1),
            TagEnd::Image => {
                if let Some((alt, url)) = self.image.take() {
                    let alt = alt.trim();
                    let name = if alt.is_empty() { url.rsplit(['/', '\\']).next().unwrap_or("") } else { alt };
                    let span = Span { text: format!("[image: {name}]"), color: self.pal.dim, ..self.style() };
                    self.push(span, src);
                }
            }
            _ => {}
        }
    }

    fn open_item(&mut self, marker: String, color: Option<[u8; 3]>) {
        let width = str_width(&marker);
        let marker = Some(vec![Span { text: marker, color, ..Default::default() }]);
        self.containers.push(Container::Item { marker, width });
    }

    fn open_row(&mut self, head: bool, src: usize) {
        if let Some(table) = self.table.as_mut() {
            table.rows.push(Row { cells: Vec::new(), head, src });
        }
    }

    /// Style for inline text at this point.
    fn style(&self) -> Span {
        let heading = self.heading;
        Span {
            text: String::new(),
            color: if self.link > 0 {
                self.pal.link
            } else if heading.is_some_and(|l| l <= 3) {
                self.pal.heading
            } else {
                None
            },
            bold: self.strong + self.html_bold > 0 || heading.is_some(),
            italic: self.emph + self.html_italic > 0,
            strike: self.strike + self.html_strike > 0,
            underline: self.link > 0,
            code: false,
        }
    }

    fn text(&mut self, t: &str, src: usize) {
        if let Some(code) = self.code.as_mut() {
            code.text.push_str(t);
            return;
        }
        if let Some((alt, _)) = self.image.as_mut() {
            alt.push_str(t);
            return;
        }
        let span = Span { text: t.replace('\r', ""), ..self.style() };
        self.push(span, src);
    }

    fn inline_code(&mut self, t: &str, src: usize) {
        if let Some((alt, _)) = self.image.as_mut() {
            alt.push_str(t);
            return;
        }
        let style = self.style();
        let color = if self.link > 0 { style.color } else { self.pal.code };
        self.push(Span { text: t.replace('\r', ""), color, code: true, ..style }, src);
    }

    fn inline_html(&mut self, html: &str, src: usize) {
        let Some(inner) = html.trim().strip_prefix('<') else {
            return;
        };
        let closing = inner.starts_with('/');
        let count = |n: &mut u32| {
            if closing {
                *n = n.saturating_sub(1);
            } else {
                *n += 1;
            }
        };
        match tag_name(inner).as_str() {
            "br" => self.push(Span { text: "\n".into(), ..Default::default() }, src),
            "img" => {
                let span = Span { text: image_label(inner), color: self.pal.dim, ..self.style() };
                self.push(span, src);
            }
            "b" | "strong" => count(&mut self.html_bold),
            "i" | "em" => count(&mut self.html_italic),
            "s" | "del" | "strike" => count(&mut self.html_strike),
            _ => {}
        }
    }

    fn task(&mut self, done: bool, src: usize) {
        let span = Span {
            text: if done { "[x] " } else { "[ ] " }.into(),
            color: if done { self.pal.alerts[1] } else { self.pal.dim },
            bold: done,
            ..Default::default()
        };
        // The checkbox takes the bullet's place.
        if self.pending.is_empty() {
            if let Some(Container::Item { marker: marker @ Some(_), width }) = self.containers.last_mut() {
                *width = 4;
                *marker = Some(vec![span]);
                return;
            }
        }
        self.push(span, src);
    }

    fn push(&mut self, span: Span, src: usize) {
        if span.text.is_empty() {
            return;
        }
        if std::mem::take(&mut self.soft) {
            let prev = self.pending.last().and_then(|(s, _)| s.text.chars().last());
            let next = span.text.chars().next();
            // Japanese is written without spaces, so a source line break
            // between two wide characters joins them directly.
            if !(prev.is_some_and(is_wide) && next.is_some_and(is_wide)) {
                self.pending.push((Span { text: " ".into(), ..self.style() }, src));
            }
        }
        self.pending.push((span, src));
    }

    /// Lay out the pending inline content as wrapped lines; the last one gets
    /// `last` as its kind.
    fn flush(&mut self, last: LineKind) {
        self.soft = false;
        self.html_bold = 0;
        self.html_italic = 0;
        self.html_strike = 0;
        let pieces = std::mem::take(&mut self.pending);
        if pieces.iter().all(|(s, _)| s.text.trim().is_empty()) {
            return;
        }
        let lines = wrap(&pieces, self.avail());
        let n = lines.len();
        for (i, (spans, src)) in lines.into_iter().enumerate() {
            let kind = if i + 1 == n { last } else { LineKind::Text };
            let before = self.lines.len();
            self.push_line(spans, kind, src);
            if i > 0 && self.lines.len() > before {
                self.lines[before].wrap = true;
            }
        }
    }

    fn prefix_width(&self) -> usize {
        self.containers.iter().map(Container::width).sum()
    }

    /// Columns left for content inside the current containers.
    fn avail(&self) -> usize {
        self.body.saturating_sub(self.prefix_width()).max(10)
    }

    /// Quote bars and list markers for the next line. Only a real line of
    /// content uses up a pending list marker.
    fn prefix(&mut self, consume: bool) -> (Vec<Span>, u16) {
        let mut spans = Vec::new();
        let mut width = 0;
        for c in &mut self.containers {
            match c {
                Container::Quote(color) => {
                    spans.push(Span { text: "│ ".into(), color: *color, ..Default::default() });
                }
                Container::Item { marker, width } => {
                    let m = if consume { marker.take() } else { None };
                    spans.extend(m.unwrap_or_else(|| vec![Span { text: " ".repeat(*width), ..Default::default() }]));
                }
            }
            width += c.width();
        }
        (spans, width as u16)
    }

    /// Emit the blank line owed from the previous block, if any.
    fn settle(&mut self, src: usize) {
        if !std::mem::take(&mut self.need_blank) || self.lines.is_empty() || self.last_blank {
            return;
        }
        let (mut spans, _) = self.prefix(false);
        while spans.last().is_some_and(|s| s.text.trim().is_empty()) {
            spans.pop();
        }
        // Blank lines belong to what precedes them, so jumping to a source
        // line lands on its content rather than the gap above it.
        let src = self.lines.last().map_or(src, |l| l.src);
        self.lines.push(DocLine { spans, kind: LineKind::Text, indent: 0, src, wrap: false });
        self.last_blank = true;
    }

    fn push_line(&mut self, content: Vec<Span>, kind: LineKind, src: usize) {
        if self.lines.len() >= MAX_LINES {
            self.clipped = true;
            return;
        }
        self.settle(src);
        let (mut spans, indent) = self.prefix(true);
        spans.extend(content);
        self.lines.push(DocLine { spans, kind, indent, src, wrap: false });
        self.last_blank = false;
    }

    fn code_block(&mut self, code: Code<'a>) {
        // One cell of padding inside the band on either side.
        let width = self.avail().saturating_sub(2).max(8);
        let mut hl = code.syntax.map(|s| HighlightLines::new(s, self.theme));
        for (i, line) in LinesWithEndings::from(&code.text).enumerate() {
            if self.clipped {
                break;
            }
            let spans = match hl.as_mut().map(|h| h.highlight_line(line, self.syntaxes)) {
                Some(Ok(regions)) => spans_from(regions),
                _ => {
                    let text = line.trim_end_matches(['\n', '\r']);
                    let span = Span { text: text.into(), color: self.pal.code, ..Default::default() };
                    if text.is_empty() { Vec::new() } else { vec![span] }
                }
            };
            for (j, piece) in hard_wrap(&spans, width).into_iter().enumerate() {
                let mut row = vec![Span { text: " ".into(), ..Default::default() }];
                row.extend(piece);
                let before = self.lines.len();
                self.push_line(row, LineKind::Code, code.src + i);
                if j > 0 && self.lines.len() > before {
                    self.lines[before].wrap = true;
                }
            }
        }
    }

    fn html_block(&mut self, html: &str, src: usize) {
        let pieces: Vec<(Span, usize)> = html_pieces(html, self.pal.dim).into_iter().map(|s| (s, src)).collect();
        let mut shown = false;
        for (spans, _) in wrap(&pieces, self.avail()) {
            if spans.iter().all(|s| s.text.trim().is_empty()) {
                continue;
            }
            self.push_line(spans, LineKind::Text, src);
            shown = true;
        }
        if shown {
            self.need_blank = true;
        }
    }

    fn table_block(&mut self, table: Table) {
        let (avail, dim) = (self.avail(), self.pal.dim);
        for (spans, src) in table_lines(&table.rows, &table.aligns, avail, dim) {
            self.push_line(spans, LineKind::Text, src);
        }
    }
}

fn cw(c: char) -> usize {
    c.width().unwrap_or(0)
}

fn is_wide(c: char) -> bool {
    cw(c) >= 2
}

/// The separator colour a table is drawn with, from the theme's own foreground.
/// Exposed so a sibling previewer's table looks like Markdown's.
pub(super) fn dim_of(theme: &Theme) -> Option<[u8; 3]> {
    Palette::new(theme).dim
}

/// Lay a table out as lines: columns measured, padded to their alignment, joined
/// by a dim separator, with a rule under any header row.
///
/// Free rather than a method so a previewer that is not Markdown can use it.
/// `aligns` is `pulldown_cmark`'s type because that is this module's existing
/// vocabulary; a parallel enum plus a conversion would be more code for the same
/// result. Returns each line with the source line it came from, which the caller
/// pushes -- `DocLine.src` has to stay monotonically non-decreasing for
/// `Doc::line_for_src` to work.
pub(super) fn table_lines(
    rows: &[Row],
    aligns: &[Alignment],
    avail: usize,
    dim: Option<[u8; 3]>,
) -> Vec<(Vec<Span>, usize)> {
    let ncols = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0).max(aligns.len());
    let mut out = Vec::new();
    if ncols == 0 {
        return out;
    }
    let mut widths = vec![0; ncols];
    for row in rows {
        for (j, cell) in row.cells.iter().enumerate() {
            widths[j] = widths[j].max(cell.iter().map(|(s, _)| str_width(&s.text)).sum());
        }
    }
    // Squeeze the widest columns until the table fits; their cells wrap.
    let seps = 3 * (ncols - 1);
    while widths.iter().sum::<usize>() + seps > avail {
        let (j, w) = widths.iter().copied().enumerate().max_by_key(|&(_, w)| w).unwrap_or((0, 0));
        if w <= 4 {
            break;
        }
        widths[j] = w - 1;
    }

    let sep = Span { text: " │ ".into(), color: dim, ..Default::default() };
    for row in rows {
        let cells: Vec<Vec<Vec<Span>>> = (0..ncols)
            .map(|j| {
                let mut cell = row.cells.get(j).cloned().unwrap_or_default();
                if row.head {
                    cell.iter_mut().for_each(|(s, _)| s.bold = true);
                }
                wrap(&cell, widths[j]).into_iter().map(|(spans, _)| spans).collect()
            })
            .collect();
        let height = cells.iter().map(Vec::len).max().unwrap_or(0).max(1);
        for k in 0..height {
            let mut line = Vec::new();
            for j in 0..ncols {
                if j > 0 {
                    line.push(sep.clone());
                }
                let content = cells[j].get(k).cloned().unwrap_or_default();
                let pad = widths[j].saturating_sub(content.iter().map(|s| str_width(&s.text)).sum());
                let (left, right) = match aligns.get(j) {
                    Some(Alignment::Right) => (pad, 0),
                    Some(Alignment::Center) => (pad / 2, pad - pad / 2),
                    _ => (0, pad),
                };
                if left > 0 {
                    line.push(Span { text: " ".repeat(left), ..Default::default() });
                }
                line.extend(content);
                if right > 0 && j + 1 < ncols {
                    line.push(Span { text: " ".repeat(right), ..Default::default() });
                }
            }
            out.push((line, row.src));
        }
        if row.head {
            let rule = widths.iter().map(|&w| "─".repeat(w)).collect::<Vec<_>>().join("─┼─");
            out.push((vec![Span { text: rule, color: dim, ..Default::default() }], row.src));
        }
    }
    out
}

fn str_width(s: &str) -> usize {
    super::cells(s)
}

fn clip_width(s: &str, max: usize) -> String {
    if str_width(s) <= max {
        return s.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = cw(c);
        if used + w + 1 > max {
            break;
        }
        used += w;
        out.push(c);
    }
    out.push('…');
    out
}

/// Where a line may break before `c`: after a space, or next to a wide
/// character unless that would start a line with closing punctuation or end
/// one with an opening bracket.
fn can_break(prev: char, c: char) -> bool {
    if c == ' ' {
        return false;
    }
    if prev == ' ' {
        return true;
    }
    (is_wide(prev) || is_wide(c)) && !NO_START.contains(c) && !NO_END.contains(prev)
}

/// Greedy line breaking into `width` columns. `\n` forces a break; a word
/// longer than a line is cut wherever it overflows.
fn break_lines(chars: &[char], width: usize) -> Vec<Range<usize>> {
    let width = width.max(1);
    let n = chars.len();
    let trim = |mut r: Range<usize>| {
        while r.start < r.end && chars[r.start] == ' ' {
            r.start += 1;
        }
        while r.end > r.start && chars[r.end - 1] == ' ' {
            r.end -= 1;
        }
        r
    };
    let mut out = Vec::new();
    let (mut start, mut i, mut used) = (0, 0, 0);
    let mut brk: Option<usize> = None;
    while i < n {
        let c = chars[i];
        if c == '\n' {
            out.push(trim(start..i));
            (start, i, used, brk) = (i + 1, i + 1, 0, None);
            continue;
        }
        if i > start && can_break(chars[i - 1], c) {
            brk = Some(i);
        }
        let w = cw(c);
        if used + w > width && i > start {
            // Spaces at the end of a line cost nothing.
            let end = if c == ' ' { i } else { brk.unwrap_or(i) };
            out.push(trim(start..end));
            start = end;
            while start < n && chars[start] == ' ' {
                start += 1;
            }
            (i, used, brk) = (start, 0, None);
            continue;
        }
        used += w;
        i += 1;
    }
    if start < n {
        out.push(trim(start..n));
    }
    out
}

/// Word-wrap styled pieces; each line keeps the source line of its first piece.
fn wrap(pieces: &[(Span, usize)], width: usize) -> Vec<(Vec<Span>, usize)> {
    let styles: Vec<&Span> = pieces.iter().map(|(s, _)| s).collect();
    let chars = flatten(&styles);
    let plain: Vec<char> = chars.iter().map(|c| c.0).collect();
    break_lines(&plain, width)
        .into_iter()
        .map(|r| {
            let src = chars.get(r.start).or(chars.last()).map_or(0, |&(_, p)| pieces[p].1);
            (regroup(&chars[r], &styles), src)
        })
        .collect()
}

/// Cut a line of code into `width`-column rows, ignoring word boundaries.
fn hard_wrap(spans: &[Span], width: usize) -> Vec<Vec<Span>> {
    let styles: Vec<&Span> = spans.iter().collect();
    let chars = flatten(&styles);
    let mut out = Vec::new();
    let (mut start, mut used) = (0, 0);
    for (i, &(c, _)) in chars.iter().enumerate() {
        let w = cw(c);
        if used + w > width && i > start {
            out.push(regroup(&chars[start..i], &styles));
            (start, used) = (i, 0);
        }
        used += w;
    }
    out.push(regroup(&chars[start..], &styles));
    out
}

/// Every character with the index of the span it came from.
fn flatten(spans: &[&Span]) -> Vec<(char, usize)> {
    spans.iter().enumerate().flat_map(|(i, s)| s.text.chars().map(move |c| (c, i))).collect()
}

fn regroup(chars: &[(char, usize)], styles: &[&Span]) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let mut last = usize::MAX;
    for &(c, p) in chars {
        if p != last {
            out.push(styles[p].clone_style());
            last = p;
        }
        if let Some(span) = out.last_mut() {
            span.text.push(c);
        }
    }
    out
}

impl Span {
    /// The same style with no text.
    fn clone_style(&self) -> Span {
        Span {
            text: String::new(),
            color: self.color,
            bold: self.bold,
            italic: self.italic,
            strike: self.strike,
            underline: self.underline,
            code: self.code,
        }
    }
}

/// Visible text of an HTML block: tags dropped, block-level tags turned into
/// line breaks, images into their alt text.
fn html_pieces(html: &str, dim: Option<[u8; 3]>) -> Vec<Span> {
    let mut out = Vec::new();
    let mut bold = 0u32;
    let mut rest = html;
    loop {
        let (before, tail) = rest.split_at(rest.find('<').unwrap_or(rest.len()));
        html_text(&mut out, before, bold > 0);
        if tail.is_empty() {
            break;
        }
        if let Some(body) = tail.strip_prefix("<!--") {
            rest = body.find("-->").map_or("", |e| &body[e + 3..]);
            continue;
        }
        let Some(gt) = tail.find('>') else {
            html_text(&mut out, tail, bold > 0);
            break;
        };
        let inner = &tail[1..gt];
        let closing = inner.starts_with('/');
        let name = tag_name(inner);
        let heading = matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6");
        if heading || matches!(name.as_str(), "b" | "strong" | "th") {
            bold = if closing { bold.saturating_sub(1) } else { bold + 1 };
        }
        let block = matches!(
            name.as_str(),
            "br" | "p" | "div" | "li" | "tr" | "table" | "ul" | "ol" | "summary" | "details" | "hr" | "pre"
                | "blockquote" | "center" | "picture" | "section"
        );
        if heading || block {
            out.push(Span { text: "\n".into(), ..Default::default() });
        }
        if name == "img" && !closing {
            out.push(Span { text: image_label(inner), color: dim, ..Default::default() });
        }
        rest = &tail[gt + 1..];
    }
    out
}

/// HTML text with runs of whitespace collapsed, as a browser shows it.
fn html_text(out: &mut Vec<Span>, text: &str, bold: bool) {
    let mut s = String::with_capacity(text.len());
    for word in text.split(|c: char| c.is_ascii_whitespace()) {
        if word.is_empty() {
            if !s.ends_with(' ') {
                s.push(' ');
            }
        } else {
            if !s.is_empty() && !s.ends_with(' ') {
                s.push(' ');
            }
            s.push_str(word);
        }
    }
    if !s.is_empty() {
        out.push(Span { text: decode_entities(&s), bold, ..Default::default() });
    }
}

fn tag_name(inner: &str) -> String {
    inner
        .trim_start_matches('/')
        .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn image_label(tag: &str) -> String {
    let name = attr(tag, "alt").filter(|a| !a.trim().is_empty()).or_else(|| {
        attr(tag, "src").map(|s| s.rsplit(['/', '\\']).next().unwrap_or("").to_owned())
    });
    format!("[image: {}]", name.unwrap_or_default().trim())
}

/// Value of an attribute inside a tag, quoted or not.
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        let whole = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let rest = lower[from..].trim_start();
        if !whole || !rest.starts_with('=') {
            continue;
        }
        let v = tag[lower.len() - rest.len() + 1..].trim_start();
        let value = match v.chars().next() {
            Some(q @ ('"' | '\'')) => v[1..].split(q).next().unwrap_or(""),
            _ => v.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or(""),
        };
        return Some(decode_entities(value));
    }
    None
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail.char_indices().take(12).find(|&(_, c)| c == ';').map(|(j, _)| j);
        match end.and_then(|e| entity(&tail[1..e]).map(|c| (e, c))) {
            Some((e, c)) => {
                out.push(c);
                rest = &tail[e + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "mdash" => '—',
        "ndash" => '–',
        "hellip" => '…',
        "middot" => '·',
        "times" => '×',
        "larr" => '←',
        "rarr" => '→',
        _ => {
            let num = name.strip_prefix('#')?;
            let code = match num.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => num.parse().ok()?,
            };
            return char::from_u32(code);
        }
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn doc(src: &str, cols: u16) -> Doc {
        let (syntaxes, themes) = super::super::text::sets();
        render(src, cols, &themes.themes["base16-ocean.dark"], syntaxes).0
    }

    fn line_text(l: &DocLine) -> String {
        l.spans.iter().map(|s| s.text.as_str()).collect()
    }

    fn texts(d: &Doc) -> Vec<String> {
        d.lines.iter().map(line_text).collect()
    }

    fn lines_of(s: &str, width: usize) -> Vec<String> {
        let chars: Vec<char> = s.chars().collect();
        break_lines(&chars, width).into_iter().map(|r| chars[r].iter().collect()).collect()
    }

    #[test]
    fn wraps_on_spaces() {
        assert_eq!(lines_of("hello world foo", 11), ["hello world", "foo"]);
        assert_eq!(lines_of("abcdefgh", 3), ["abc", "def", "gh"]);
        assert_eq!(lines_of("a\nb", 10), ["a", "b"]);
    }

    /// What the pane wrapped is searched as the paragraph it came from, and a
    /// quote bar or list marker in front of a line is never part of a match.
    #[test]
    fn marks_follow_a_wrapped_paragraph_and_skip_the_prefix() {
        let d = doc("- the automated run of the whole thing lists all its ids today\n\n> quoted text\n", 24);
        assert!(d.lines.iter().filter(|l| l.wrap).count() >= 2, "{:?}", texts(&d));
        let m = tsumugi_match::Matcher::new("auto.*ids").unwrap();
        let marks = d.marks(&m, 0, d.lines.len());
        let marked: Vec<usize> = (0..marks.len()).filter(|&i| !marks[i].is_empty()).collect();
        assert!(marked.len() >= 3, "{marked:?} in {:?}", texts(&d));
        for &i in &marked {
            let t = line_text(&d.lines[i]);
            assert!(!t[marks[i][0].clone()].contains('•') && !t[marks[i][0].clone()].starts_with(' '), "{t:?}");
        }
        let bars = tsumugi_match::Matcher::new("│").unwrap();
        let bm = d.marks(&bars, 0, d.lines.len());
        assert!(bm[4].is_empty(), "the quote bar of a line of text is not text: {bm:?}");
    }

    #[test]
    fn wraps_japanese_with_kinsoku() {
        // 。 may not start a line, so す comes down with it.
        assert_eq!(lines_of("日本語の文章です。", 8), ["日本語の", "文章で", "す。"]);
        // 「 may not end one.
        assert_eq!(lines_of("あ「い", 4), ["あ", "「い"]);
        // Latin runs and CJK may break against each other.
        assert_eq!(lines_of("Rust言語", 6), ["Rust言", "語"]);
        assert_eq!(lines_of("Rust言語", 5), ["Rust", "言語"]);
    }

    #[test]
    fn soft_breaks_join_japanese_without_a_space() {
        assert_eq!(texts(&doc("日本\n語\n\nab\ncd\n", 80)), ["日本語", "", "ab cd"]);
    }

    #[test]
    fn headings_feed_the_outline() {
        let d = doc("# Title\n\nintro\n\n## Part one\n\ntext\n\n### Deep\n\n##### Too deep\n", 100);
        assert!(d.toc_cols > 0);
        let labels: Vec<&str> = d.toc.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, ["Title", "  Part one", "    Deep"]);
        for e in &d.toc {
            assert_eq!(line_text(&d.lines[e.line]), e.label.trim());
        }
        assert_eq!(d.lines[d.toc[0].line].kind, LineKind::Heading(1));
        assert!(d.lines[0].spans.iter().all(|s| s.bold));
        // Narrow panes get no outline column, but keep the entries for the overlay.
        let narrow = doc("# a\n## b\n", 60);
        assert_eq!(narrow.toc_cols, 0);
        assert_eq!(narrow.toc.len(), 2);
    }

    #[test]
    fn lists_and_tasks() {
        let d = doc("- one\n- [ ] todo\n- [x] done\n  - nested\n\n1. first\n2. second\n", 80);
        assert_eq!(texts(&d), ["• one", "[ ] todo", "[x] done", "    ◦ nested", "", "1. first", "2. second"]);
    }

    #[test]
    fn tables_align_columns() {
        let d = doc("| name | n |\n|------|--:|\n| a | 10 |\n| bb | 2 |\n", 80);
        assert_eq!(texts(&d), ["name │  n", "─────┼───", "a    │ 10", "bb   │  2"]);
        assert!(d.lines[0].spans.iter().filter(|s| s.text.trim() == "name").all(|s| s.bold));
    }

    #[test]
    fn code_blocks_are_highlighted() {
        let d = doc("---\ntitle: x\n---\n\ntext\n\n```rust\nfn main() { let x = 1; }\n```\n", 80);
        let code: Vec<&DocLine> = d.lines.iter().filter(|l| l.kind == LineKind::Code).collect();
        assert_eq!(code.len(), 2, "{:?}", texts(&d));
        assert_eq!(code[0].src, 1);
        assert_eq!(code[1].src, 7);
        let colors: HashSet<[u8; 3]> = code[1].spans.iter().filter_map(|s| s.color).collect();
        assert!(colors.len() > 1, "{:?}", code[1].spans);
    }

    #[test]
    fn quotes_alerts_and_html() {
        let src = "> [!NOTE]\n> read this\n\n<p align=\"center\"><img alt=\"logo\" src=\"x.png\"> <b>filer</b></p>\n\n<!-- hidden -->\n\nA &amp; B<br>C\n";
        let d = doc(src, 80);
        assert_eq!(texts(&d), ["│ Note", "│ read this", "", "[image: logo] filer", "", "A & B", "C"]);
    }

    #[test]
    fn survives_any_width() {
        let src = "# 見出し Heading\n\n> [!WARNING]\n> 長い引用文がここに入ります。とても長い。\n\n\
                   - item **bold** `code` and a [link](http://x)\n  1. 入れ子の番号付きリスト\n\n\
                   | 列 | long column header |\n|:--|:-:|\n| 値 | `x` |\n\n\
                   ```rust\nfn main() { println!(\"こんにちは\"); }\n```\n\n\
                   Term[^1]\n\n[^1]: 脚注\n\n---\n\n<details><summary>more</summary>\n\ntext\n\n</details>\n";
        for cols in 0..=120 {
            let d = doc(src, cols);
            assert!(!d.lines.is_empty());
            for (i, e) in d.toc.iter().enumerate() {
                assert!(e.line < d.lines.len(), "cols {cols}, toc {i}");
            }
        }
    }

    #[test]
    fn lines_map_back_to_source() {
        let d = doc("# A\n\none\ntwo\n\n## B\n\nthree\n", 80);
        assert_eq!(texts(&d), ["A", "", "one two", "", "B", "", "three"]);
        let b = d.toc[1].line;
        assert_eq!(b, 4);
        assert_eq!(d.src_for_line(b), 5);
        assert_eq!(d.line_for_src(5), b);
        // "two" is the second source line of the "one two" paragraph.
        assert_eq!(d.line_for_src(3), 2);
        // A blank source line maps to the block above it.
        assert_eq!(d.line_for_src(4), 2);
        assert_eq!(d.line_for_src(0), 0);
        assert_eq!(d.src_for_line(99), 7);
    }
}
