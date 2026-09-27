//! Word, Excel and PowerPoint, read without Word, Excel or PowerPoint.
//!
//! The shell's thumbnail for these comes from Office itself, so on a machine
//! without Office there is nothing to show and the file falls through to a hex
//! dump. But the formats are not opaque: `.docx`, `.xlsx` and `.pptx` are zip
//! archives of XML, and filer already reads zip. What is wanted from a preview
//! is the text, and the text is right there.
//!
//! So this does not try to draw the document. It reads it out as lines, which
//! is better than a picture would be anyway: the existing text preview then
//! scrolls it, searches it, shows a minimap of it and gives a spreadsheet's
//! sheets an outline.
//!
//! The XML is read with a scanner rather than a parser. These parts are
//! machine-written and the shapes wanted are small -- the text inside `<t>`,
//! the cells of a row -- and a full parser is a dependency for something that
//! is a page of code.

use std::io::Read;
use std::path::Path;

use crate::preview::TocEntry;

/// What kind of Office file, from the extension.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Word,
    Sheet,
    Slides,
}

impl Kind {
    pub fn of(ext: Option<&str>) -> Option<Self> {
        match ext?.to_ascii_lowercase().as_str() {
            "docx" | "docm" | "dotx" => Some(Self::Word),
            "xlsx" | "xlsm" | "xltx" => Some(Self::Sheet),
            "pptx" | "pptm" | "potx" => Some(Self::Slides),
            // The pre-2007 formats are not zip and not XML; a different
            // problem, and not one a page of code solves.
            _ => None,
        }
    }
}

/// The document's text, and an outline where it has one.
#[derive(Debug)]
pub struct Read1 {
    pub lines: Vec<String>,
    pub outline: Vec<TocEntry>,
    /// Whether it stopped early.
    pub truncated: bool,
}

/// How much is read before stopping. A spreadsheet can hold a million rows and
/// nobody previews a million rows.
const MAX_LINES: usize = 5_000;

pub fn read(path: &Path, kind: Kind, _max_bytes: usize) -> Result<Read1, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| match e {
        // The most useful thing to say about a `.docx` that is not a zip is
        // that it is not one: an old `.doc` renamed is the common cause.
        zip::result::ZipError::InvalidArchive(_) => {
            "not an Office XML file (a pre-2007 .doc/.xls/.ppt renamed?)".to_string()
        }
        other => other.to_string(),
    })?;
    match kind {
        Kind::Word => word(&mut zip),
        Kind::Sheet => sheet(&mut zip),
        Kind::Slides => slides(&mut zip),
    }
}

fn part(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<String> {
    let mut f = zip.by_name(name).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s)
}

/// How much of a property part is read. These run to a few hundred bytes in
/// practice; the cap is only so a hand-built file cannot ask for a gigabyte.
const MAX_PROP_BYTES: u64 = 64 << 10;

/// `docProps/core.xml` and `docProps/app.xml`, in the order a property sheet
/// wants them. Empty when the file carries no properties, or is not OOXML.
///
/// The XML is scanned rather than parsed, for the reason this module's own
/// header gives. (`roxmltree` is in fact reachable, as `resvg::usvg::roxmltree`
/// -- but through the SVG renderer's private dependency tree, with no version
/// of its own in `Cargo.toml`, which is a worse bargain than the scanner below.)
pub fn properties(path: &Path) -> Vec<(&'static str, String)> {
    // The same gate `read` uses, kept here so the caller needs to know nothing
    // about which extensions are OOXML: `.doc` and `.xls` decline themselves.
    let ext = path.extension().and_then(|e| e.to_str());
    if Kind::of(ext).is_none() {
        return Vec::new();
    }
    let Ok(file) = std::fs::File::open(path) else { return Vec::new() };
    let Ok(mut zip) = zip::ZipArchive::new(file) else { return Vec::new() };
    let core = part_capped(&mut zip, "docProps/core.xml", MAX_PROP_BYTES).unwrap_or_default();
    let app = part_capped(&mut zip, "docProps/app.xml", MAX_PROP_BYTES).unwrap_or_default();
    let mut out: Vec<(&'static str, String)> = Vec::new();
    let mut put = |key: &'static str, v: Option<String>| {
        if let Some(v) = v.filter(|v| !v.trim().is_empty()) {
            out.push((key, v));
        }
    };
    put("Title", tag_text(&core, "dc:title"));
    put("Author", tag_text(&core, "dc:creator"));
    put("Last saved by", tag_text(&core, "cp:lastModifiedBy"));
    put("Created", tag_text(&core, "dcterms:created").map(stamp));
    put("Modified", tag_text(&core, "dcterms:modified").map(stamp));
    put("Revision", tag_text(&core, "cp:revision"));
    let app_name = match (tag_text(&app, "Application"), tag_text(&app, "AppVersion")) {
        (Some(a), Some(v)) => Some(format!("{a} {v}")),
        (a, _) => a,
    };
    put("Application", app_name);
    put("Pages", tag_text(&app, "Pages"));
    put("Words", tag_text(&app, "Words"));
    put("Slides", tag_text(&app, "Slides"));
    out
}

fn part_capped(zip: &mut zip::ZipArchive<std::fs::File>, name: &str, max: u64) -> Option<String> {
    let f = zip.by_name(name).ok()?;
    let mut s = String::new();
    f.take(max).read_to_string(&mut s).ok()?;
    Some(s)
}

/// The text of the first `<tag>` … `</tag>`.
///
/// Three things this has to get right. The name must end at the tag: a search
/// for `<dc:t` must not find `<dc:title>`, so the byte after it has to be `>`
/// or whitespace. `<tag/>` is an empty element, not an opening one -- reading
/// past it to the next `</` would swallow the rest of the document. And the
/// body stops at the next `<` rather than at `</tag>`, which fails safe on a
/// part that unexpectedly has children.
fn tag_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let mut from = 0;
    loop {
        let at = from + xml.get(from..)?.find(&open)?;
        let after = at + open.len();
        let next = xml.as_bytes().get(after)?;
        if !matches!(next, b'>' | b' ' | b'\t' | b'\r' | b'\n' | b'/') {
            from = after; // `<dc:titlepage`, not `<dc:title`
            continue;
        }
        let close = after + xml.get(after..)?.find('>')?;
        if xml.as_bytes().get(close.wrapping_sub(1)) == Some(&b'/') {
            return None; // `<tag/>`: present and empty
        }
        let body = xml.get(close + 1..)?;
        let end = body.find('<').unwrap_or(body.len());
        return Some(unescape(&body[..end]));
    }
}

/// `2026-09-01T12:34:56Z` as `2026-09-01 12:34:56 UTC`. OOXML stores these in
/// UTC while every other time in the panel is local, so the suffix is not
/// decoration.
fn stamp(s: String) -> String {
    if s.len() == 20 && s.ends_with('Z') && s.as_bytes()[10] == b'T' {
        format!("{} {} UTC", &s[..10], &s[11..19])
    } else {
        s
    }
}

// ----------------------------------------------------------------- Word

fn word(zip: &mut zip::ZipArchive<std::fs::File>) -> Result<Read1, String> {
    let xml = part(zip, "word/document.xml").ok_or("no word/document.xml in it")?;
    let mut lines = Vec::new();
    let mut outline = Vec::new();
    for para in xml.split("<w:p ").skip(1).chain(xml.split("<w:p>").skip(1)) {
        let body = para.split("</w:p>").next().unwrap_or("");
        let text = text_of(body);
        // A heading names itself in its paragraph properties, which is the
        // only structure Word leaves behind that is worth an outline.
        if let Some(level) = heading_level(body) {
            if !text.trim().is_empty() {
                outline.push(TocEntry { level, label: text.clone(), line: lines.len() });
            }
        }
        lines.push(text);
        if lines.len() >= MAX_LINES {
            return Ok(Read1 { lines, outline, truncated: true });
        }
    }
    Ok(Read1 { lines, outline, truncated: false })
}

/// `<w:pStyle w:val="Heading2"/>` → 2.
fn heading_level(para: &str) -> Option<u8> {
    let at = para.find("w:pStyle")?;
    let val = attr(&para[at..], "w:val")?;
    let n = val.strip_prefix("Heading").or_else(|| val.strip_prefix("heading"))?;
    n.parse().ok().filter(|l| (1..=9).contains(l))
}

// ----------------------------------------------------------------- Slides

fn slides(zip: &mut zip::ZipArchive<std::fs::File>) -> Result<Read1, String> {
    // Sorted by number, not by the order the zip happens to hold them: a deck
    // read out of order is worse than no deck.
    let mut names: Vec<String> = (0..zip.len())
        .filter_map(|i| Some(zip.by_index(i).ok()?.name().to_owned()))
        .filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml"))
        .collect();
    names.sort_by_key(|n| slide_number(n));
    if names.is_empty() {
        return Err("no slides in it".into());
    }

    let mut lines = Vec::new();
    let mut outline = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let Some(xml) = part(zip, name) else { continue };
        outline.push(TocEntry {
            level: 1,
            label: format!("Slide {}", i + 1),
            line: lines.len(),
        });
        lines.push(format!("--- Slide {} ---", i + 1));
        // One line per text shape, which is roughly one line per bullet.
        for shape in xml.split("<a:p>").skip(1) {
            let text = text_of(shape.split("</a:p>").next().unwrap_or(""));
            if !text.trim().is_empty() {
                lines.push(text);
            }
        }
        lines.push(String::new());
        if lines.len() >= MAX_LINES {
            return Ok(Read1 { lines, outline, truncated: true });
        }
    }
    Ok(Read1 { lines, outline, truncated: false })
}

fn slide_number(name: &str) -> u32 {
    name.trim_start_matches("ppt/slides/slide").trim_end_matches(".xml").parse().unwrap_or(0)
}

// ----------------------------------------------------------------- text

/// Every `<...:t>` run in a part, joined.
///
/// Word and PowerPoint both put the readable text in `t` elements and
/// everything else in attributes, so a scan for those is the whole job.
fn text_of(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(at) = rest.find(":t>").or_else(|| rest.find("<t>")) {
        // Step over the tag: `<w:t>` and `<a:t>` both end at the `>` found.
        let after = &rest[at..];
        let Some(gt) = after.find('>') else { break };
        let body = &after[gt + 1..];
        let Some(end) = body.find("</") else { break };
        out.push_str(&unescape(&body[..end]));
        rest = &body[end..];
    }
    out
}

/// The five XML entities, which is all these parts use.
fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The value of the attribute `name` in `s`.
///
/// The name has to start a word and be followed by `=`, or `s` on a cell would
/// be found inside `t="s"` and `t` inside a `Target=`. Matching loosely is how
/// the first version of this read every cell as unformatted and every sheet as
/// its filename: it took the name with the `=` already attached, so nothing
/// matched at all and the fallbacks quietly covered for it.
fn attr<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let mut from = 0;
    loop {
        let at = from + s[from..].find(name)?;
        from = at + name.len();
        if at > 0 && !s[..at].ends_with([' ', '\t', '\n', '\r', '<', '/']) {
            continue;
        }
        let after = s[from..].trim_start();
        let Some(v) = after.strip_prefix('=') else { continue };
        let v = v.trim_start();
        let Some(quote) = v.chars().next() else { continue };
        return v[quote.len_utf8()..].split(quote).next();
    }
}

// ----------------------------------------------------------------- Excel

/// A worksheet as rows of tab-separated cells.
///
/// Tabs because the text preview already aligns nothing and a spreadsheet's
/// columns are ragged: a fixed width would be wrong for every sheet but one.
fn sheet(zip: &mut zip::ZipArchive<std::fs::File>) -> Result<Read1, String> {
    let shared = shared_strings(zip);
    let dates = date_styles(zip);

    let mut names: Vec<(String, String)> = sheet_names(zip);
    if names.is_empty() {
        // No workbook part, or one this scanner did not understand: fall back
        // to whatever worksheets are in the zip, in order.
        names = (0..zip.len())
            .filter_map(|i| Some(zip.by_index(i).ok()?.name().to_owned()))
            .filter(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml"))
            .map(|n| (n.clone(), n.clone()))
            .collect();
        names.sort();
    }
    if names.is_empty() {
        return Err("no worksheets in it".into());
    }

    let mut lines = Vec::new();
    let mut outline = Vec::new();
    for (title, part_name) in &names {
        let Some(xml) = part(zip, part_name) else { continue };
        outline.push(TocEntry { level: 1, label: title.clone(), line: lines.len() });
        lines.push(format!("--- {title} ---"));
        for row in xml.split("<row").skip(1) {
            let row = row.split("</row>").next().unwrap_or("");
            let mut cells: Vec<String> = Vec::new();
            for c in row.split("<c ").skip(1) {
                cells.push(cell_value(c, &shared, &dates));
            }
            // Trailing empties carry no information and make every row as wide
            // as the widest one.
            while cells.last().is_some_and(|c| c.is_empty()) {
                cells.pop();
            }
            lines.push(cells.join("\t"));
            if lines.len() >= MAX_LINES {
                return Ok(Read1 { lines, outline, truncated: true });
            }
        }
        lines.push(String::new());
    }
    Ok(Read1 { lines, outline, truncated: false })
}

/// One cell: `<c r="A1" s="3" t="s"><v>7</v></c>`.
///
/// `t` says how to read `v`: `s` is an index into the shared strings, `inlineStr`
/// puts the text right there, and anything else is a number -- which may be a
/// date wearing a number's clothes, which is what `dates` is for.
fn cell_value(c: &str, shared: &[String], dates: &[bool]) -> String {
    let ty = attr(c, "t").unwrap_or("");
    let body = c.split_once('>').map(|(_, b)| b).unwrap_or("");
    if ty == "inlineStr" {
        return text_of(body);
    }
    let Some(v) = body.split("<v>").nth(1).and_then(|v| v.split("</v>").next()) else {
        return String::new();
    };
    match ty {
        "s" => v.parse::<usize>().ok().and_then(|i| shared.get(i).cloned()).unwrap_or_default(),
        "str" | "e" => unescape(v),
        _ => {
            let style: usize = attr(c, "s").and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
            match dates.get(style).copied().unwrap_or(false) {
                true => v.parse::<f64>().map(serial_date).unwrap_or_else(|_| v.to_owned()),
                false => unescape(v),
            }
        }
    }
}

/// `xl/sharedStrings.xml`: every string in the workbook, once.
fn shared_strings(zip: &mut zip::ZipArchive<std::fs::File>) -> Vec<String> {
    let Some(xml) = part(zip, "xl/sharedStrings.xml") else { return Vec::new() };
    xml.split("<si>").skip(1).map(|si| text_of(si.split("</si>").next().unwrap_or(""))).collect()
}

/// Which cell styles mean "this number is a date".
///
/// A date in a spreadsheet is a number with a format attached, so without this
/// every date reads as five digits. The built-in format ids for dates and
/// times are 14–22 and 45–47; anything else is a date only if the workbook
/// defined it as one, which shows in its format code.
fn date_styles(zip: &mut zip::ZipArchive<std::fs::File>) -> Vec<bool> {
    let Some(xml) = part(zip, "xl/styles.xml") else { return Vec::new() };

    let mut custom: Vec<(u32, bool)> = Vec::new();
    for f in xml.split("<numFmt ").skip(1) {
        let Some(id) = attr(f, "numFmtId").and_then(|v| v.parse::<u32>().ok()) else { continue };
        let code = attr(f, "formatCode").unwrap_or("");
        // A format code is a date's if it positions any date or time part.
        // The quoted literals inside one can hold anything, so they go first.
        let bare: String = code.split('"').step_by(2).collect();
        let looks = bare.chars().any(|c| matches!(c, 'y' | 'd' | 'h' | 's' | 'Y' | 'D' | 'H' | 'S'))
            || bare.contains("mm");
        custom.push((id, looks));
    }

    let Some(xfs) = xml.split("<cellXfs").nth(1) else { return Vec::new() };
    xfs.split("<xf ")
        .skip(1)
        .map(|xf| {
            let id = attr(xf, "numFmtId").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
            matches!(id, 14..=22 | 45..=47)
                || custom.iter().any(|(c, looks)| *c == id && *looks)
        })
        .collect()
}

/// Excel's day number as a date.
///
/// Day 1 is 1900-01-01, and day 60 is 1900-02-29 -- a day that did not exist.
/// The bug is part of the format and every reader has to keep it, so 1900 is
/// treated as a leap year and dates from March 1900 onwards line up.
fn serial_date(serial: f64) -> String {
    let days = serial.trunc() as i64;
    if days < 1 {
        return serial.to_string();
    }
    // Days since 1900-01-01, with the phantom 29th February taken back out.
    // Taking it out *and* keeping 1900 as a leap year below would remove it
    // twice, which is how this first read 45000 as the 14th of March.
    let mut d = days - 1 - i64::from(days > 60);
    let (mut y, mut m) = (1900, 1);
    loop {
        let len = month_days(y, m);
        if d < len {
            break;
        }
        d -= len;
        m += 1;
        if m > 12 {
            m = 1;
            y += 1;
        }
    }
    let date = format!("{y:04}-{m:02}-{:02}", d + 1);
    let frac = serial.fract();
    match frac > 0.0 {
        false => date,
        true => {
            let secs = (frac * 86_400.0).round() as i64;
            format!("{date} {:02}:{:02}", secs / 3600 % 24, secs / 60 % 60)
        }
    }
}

fn month_days(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        // The real rule. Excel's extra day in 1900 is subtracted above
        // instead, which keeps the calendar here honest.
        _ => match y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            true => 29,
            false => 28,
        },
    }
}

/// The sheets in the order the workbook lists them, with their titles.
///
/// The title is what the tabs at the bottom say, and `sheet1.xml` is very
/// often not the first tab.
fn sheet_names(zip: &mut zip::ZipArchive<std::fs::File>) -> Vec<(String, String)> {
    let Some(book) = part(zip, "xl/workbook.xml") else { return Vec::new() };
    let rels = part(zip, "xl/_rels/workbook.xml.rels").unwrap_or_default();
    book.split("<sheet ")
        .skip(1)
        .filter_map(|s| {
            let name = unescape(attr(s, "name")?);
            let rid = attr(s, "r:id")?;
            let target = rels
                .split("<Relationship ")
                .find(|r| attr(r, "Id") == Some(rid))
                .and_then(|r| attr(r, "Target"))?;
            let target = target.trim_start_matches("/xl/").trim_start_matches("xl/");
            Some((name, format!("xl/{target}")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build a zip with the parts named, which is all these formats are.
    fn make(path: &Path, parts: &[(&str, &str)]) {
        let f = std::fs::File::create(path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, body) in parts {
            z.start_file(*name, opts).unwrap();
            z.write_all(body.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("filer-office-{name}"));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn the_extension_says_which_reader() {
        assert_eq!(Kind::of(Some("docx")), Some(Kind::Word));
        assert_eq!(Kind::of(Some("XLSX")), Some(Kind::Sheet));
        assert_eq!(Kind::of(Some("pptm")), Some(Kind::Slides));
        // The pre-2007 formats are not zip and are left to the hex dump.
        assert_eq!(Kind::of(Some("doc")), None);
        assert_eq!(Kind::of(Some("txt")), None);
    }

    /// Word: paragraphs become lines, headings become an outline.
    #[test]
    fn a_word_document_reads_out_as_its_paragraphs() {
        let p = tmp("a.docx");
        make(&p, &[(
            "word/document.xml",
            r#"<w:document><w:body>
               <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Title</w:t></w:r></w:p>
               <w:p><w:r><w:t>Hello </w:t></w:r><w:r><w:t>world</w:t></w:r></w:p>
               <w:p><w:r><w:t>Caf&#233; &amp; bar</w:t></w:r></w:p>
               </w:body></w:document>"#,
        )]);

        let doc = read(&p, Kind::Word, 1 << 20).unwrap();
        // Runs inside one paragraph join into one line, which is what a
        // paragraph is: Word splits a sentence into runs at every change of
        // formatting, and one line per run would be shredded prose.
        assert!(doc.lines.contains(&"Hello world".to_string()), "{:?}", doc.lines);
        assert!(doc.lines.contains(&"Caf&#233; & bar".to_string()), "{:?}", doc.lines);
        assert_eq!(doc.outline.len(), 1);
        assert_eq!(doc.outline[0].label, "Title");
        assert_eq!(doc.outline[0].level, 1);

        let _ = std::fs::remove_file(&p);
    }

    /// Excel: shared strings resolve, sheets keep their tab names and order.
    #[test]
    fn a_workbook_reads_out_as_rows() {
        let p = tmp("b.xlsx");
        make(&p, &[
            ("xl/workbook.xml", r#"<workbook><sheets>
                <sheet name="Totals" r:id="rId9"/><sheet name="Raw" r:id="rId2"/>
              </sheets></workbook>"#),
            ("xl/_rels/workbook.xml.rels", r#"<Relationships>
                <Relationship Id="rId9" Target="worksheets/sheet2.xml"/>
                <Relationship Id="rId2" Target="worksheets/sheet1.xml"/>
              </Relationships>"#),
            ("xl/sharedStrings.xml", r#"<sst><si><t>Name</t></si><si><t>Ada</t></si></sst>"#),
            ("xl/worksheets/sheet2.xml", r#"<worksheet><sheetData>
                <row><c r="A1" t="s"><v>0</v></c><c r="B1"><v>42</v></c></row>
              </sheetData></worksheet>"#),
            ("xl/worksheets/sheet1.xml", r#"<worksheet><sheetData>
                <row><c r="A1" t="s"><v>1</v></c></row>
              </sheetData></worksheet>"#),
        ]);

        let doc = read(&p, Kind::Sheet, 1 << 20).unwrap();
        let text = doc.lines.join("\n");
        assert!(text.contains("Name\t42"), "shared strings resolve: {text:?}");
        assert!(text.contains("Ada"), "{text:?}");
        // The workbook's order, not the zip's: `sheet1.xml` is the second tab
        // here, and a reader that trusted the filename would swap them.
        let names: Vec<&str> = doc.outline.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(names, ["Totals", "Raw"]);

        let _ = std::fs::remove_file(&p);
    }

    /// A date is a number with a format on it, and has to read as a date.
    ///
    /// Without this every date in every spreadsheet shows as five digits,
    /// which is the single most common thing in a real workbook.
    #[test]
    fn a_dated_cell_reads_as_a_date() {
        let p = tmp("c.xlsx");
        make(&p, &[
            ("xl/styles.xml", r#"<styleSheet>
                <numFmts><numFmt numFmtId="164" formatCode="yyyy\-mm\-dd"/></numFmts>
                <cellXfs count="3">
                  <xf numFmtId="0"/><xf numFmtId="14"/><xf numFmtId="164"/>
                </cellXfs></styleSheet>"#),
            ("xl/worksheets/sheet1.xml", r#"<worksheet><sheetData><row>
                <c r="A1" s="0"><v>45000</v></c>
                <c r="B1" s="1"><v>45000</v></c>
                <c r="C1" s="2"><v>45000.5</v></c>
              </row></sheetData></worksheet>"#),
        ]);

        let doc = read(&p, Kind::Sheet, 1 << 20).unwrap();
        let row = doc.lines.iter().find(|l| l.contains('\t')).expect("a row");
        let cells: Vec<&str> = row.split('\t').collect();
        assert_eq!(cells[0], "45000", "no format means it is just a number");
        assert_eq!(cells[1], "2023-03-15", "built-in date format");
        assert_eq!(cells[2], "2023-03-15 12:00", "a custom one, with its time");

        // Both ends of the range, and the 1900 hole in the middle. Excel's
        // day 60 is the 29th of February 1900, a day that never happened, so
        // any answer for it is arbitrary; what matters is that the days on
        // either side of it are right.
        assert_eq!(serial_date(1.0), "1900-01-01");
        assert_eq!(serial_date(59.0), "1900-02-28");
        assert_eq!(serial_date(61.0), "1900-03-01");
        assert_eq!(serial_date(45001.0), "2023-03-16");

        let _ = std::fs::remove_file(&p);
    }

    /// PowerPoint: slides in their own order, each announced.
    #[test]
    fn a_deck_reads_out_slide_by_slide() {
        let p = tmp("d.pptx");
        let slide = |t: &str| format!(r#"<p:sld><a:p><a:r><a:t>{t}</a:t></a:r></a:p></p:sld>"#);
        make(&p, &[
            ("ppt/slides/slide10.xml", &slide("Tenth")),
            ("ppt/slides/slide2.xml", &slide("Second")),
            ("ppt/slides/slide1.xml", &slide("First")),
        ]);

        let doc = read(&p, Kind::Slides, 1 << 20).unwrap();
        let text = doc.lines.join("\n");
        // Sorted by number, not by name: `slide10` sorts before `slide2` as a
        // string, and a deck read in that order is nonsense.
        let order: Vec<usize> = ["First", "Second", "Tenth"]
            .iter()
            .map(|w| text.find(w).unwrap_or_else(|| panic!("{w} missing in {text:?}")))
            .collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "out of order: {text:?}");
        assert_eq!(doc.outline.len(), 3);

        let _ = std::fs::remove_file(&p);
    }

    /// An old `.doc` renamed to `.docx` is not a zip, and says so.
    #[test]
    fn something_that_is_not_a_zip_says_what_it_is_not() {
        let p = tmp("e.docx");
        std::fs::write(&p, b"\xd0\xcf\x11\xe0not a zip at all").unwrap();
        let e = read(&p, Kind::Word, 1 << 20).unwrap_err();
        assert!(e.contains("pre-2007"), "{e}");
        let _ = std::fs::remove_file(&p);
    }
}
