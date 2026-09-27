//! CSV and TSV as an aligned table.
//!
//! The layout is not this module's work: `markdown::table_lines` already
//! measures columns, pads them to their alignment, squeezes a table too wide for
//! the pane and wraps what is left. All that is needed here is to turn records
//! into its `Row`s.
//!
//! The payload is shaped like Markdown's -- the table in `doc`, the raw text in
//! `source` -- for two reasons. The minimap reads a row's visible width to size
//! its bands, so a `Payload::Text` full of padded table rows would draw one
//! uniform block and its viewport box would point at rendered rows rather than
//! at lines of the file. And it means `M` switches between the table and the
//! text it came from, which is the same gesture Markdown already uses.

use super::markdown::{self, Row};
use super::{Doc, DocLine, LineKind, Payload, Span};

/// Records read before stopping. Matches `text`'s own line cap: nobody reads a
/// million rows in a preview pane.
const MAX_RECORDS: usize = 4000;
/// Columns kept. A file with more than this is not a table anyone is reading.
const MAX_COLS: usize = 256;
/// Characters kept per cell, so one enormous field cannot cost the frame.
const MAX_CELL_CHARS: usize = 500;

/// `\t` for a `.tsv`, `,` for everything else.
pub fn delimiter(ext: Option<&str>) -> char {
    match ext {
        Some(e) if e.eq_ignore_ascii_case("tsv") => '\t',
        _ => ',',
    }
}

/// Split RFC 4180 records. A field wrapped in `"` may hold the delimiter, a
/// newline, and `""` for a literal quote -- which is why a record is not a line
/// and the count of physical lines is tracked separately.
///
/// Returns each record's fields with the line it started on.
fn records(text: &str, delim: char) -> (Vec<(Vec<String>, usize)>, bool) {
    let mut out: Vec<(Vec<String>, usize)> = Vec::new();
    let mut fields: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut line = 0usize;
    let mut started = 0usize;
    let mut chars = text.chars().peekable();
    let mut truncated = false;
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    cell.push('"');
                }
                '"' => quoted = false,
                '\n' => {
                    line += 1;
                    cell.push('\n');
                }
                _ => cell.push(c),
            }
            continue;
        }
        match c {
            '"' if cell.is_empty() => quoted = true,
            _ if c == delim => {
                fields.push(std::mem::take(&mut cell));
            }
            '\r' => {} // the \n behind it ends the record
            '\n' => {
                fields.push(std::mem::take(&mut cell));
                out.push((std::mem::take(&mut fields), started));
                line += 1;
                started = line;
                if out.len() >= MAX_RECORDS {
                    truncated = true;
                    break;
                }
            }
            _ => cell.push(c),
        }
    }
    // A last record with no newline behind it still counts.
    if !cell.is_empty() || !fields.is_empty() {
        fields.push(cell);
        out.push((fields, started));
    }
    (out, truncated)
}

/// Whether every non-empty cell of a column parses as a number, which is the one
/// case where right-aligning a column reads better than left.
fn numeric(records: &[(Vec<String>, usize)], col: usize, skip_head: bool) -> bool {
    let mut seen = false;
    for (fields, _) in records.iter().skip(usize::from(skip_head)) {
        let Some(v) = fields.get(col).map(|v| v.trim()) else { continue };
        if v.is_empty() {
            continue;
        }
        if v.replace(',', "").parse::<f64>().is_err() {
            return false;
        }
        seen = true;
    }
    seen
}

/// The table, plus the raw text it was read from.
pub fn render(
    bytes: &[u8],
    delim: char,
    cols: u16,
    dim: Option<[u8; 3]>,
    max_bytes: usize,
) -> Payload {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let (records, mut truncated) = records(text, delim);
    truncated |= bytes.len() >= max_bytes;
    let total_lines = text.lines().count();
    if records.is_empty() {
        return super::text::plain(text, truncated, total_lines);
    }

    let ncols = records.iter().map(|(f, _)| f.len()).max().unwrap_or(0).min(MAX_COLS);
    // The first record is a header when there is more than one: a lone record is
    // a row, and drawing a rule under it says something untrue about the file.
    let head = records.len() > 1;
    let aligns: Vec<pulldown_cmark::Alignment> = (0..ncols)
        .map(|j| {
            if numeric(&records, j, head) {
                pulldown_cmark::Alignment::Right
            } else {
                pulldown_cmark::Alignment::None
            }
        })
        .collect();

    let rows: Vec<Row> = records
        .iter()
        .enumerate()
        .map(|(i, (fields, src))| Row {
            cells: fields
                .iter()
                .take(ncols)
                .map(|v| {
                    // A newline inside a quoted field would break the row into
                    // two; the cell is one cell, so it is shown as one line.
                    let text: String =
                        v.replace(['\n', '\r'], " ").chars().take(MAX_CELL_CHARS).collect();
                    vec![(Span { text, ..Default::default() }, *src)]
                })
                .collect(),
            head: head && i == 0,
            src: *src,
        })
        .collect();

    let avail = usize::from(if cols == 0 { 80 } else { cols });
    let lines: Vec<DocLine> = markdown::table_lines(&rows, &aligns, avail, dim)
        .into_iter()
        .map(|(spans, src)| DocLine { spans, kind: LineKind::Text, indent: 0, src })
        .collect();

    let doc = Doc { lines, toc: Vec::new(), toc_cols: 0, body_cols: avail as u16 };
    let source = super::text::plain_lines(text);
    let map = super::minimap(&source);
    Payload::Markdown { doc, source, map, truncated, total_lines }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(text: &str, delim: char) -> Vec<Vec<String>> {
        records(text, delim).0.into_iter().map(|(f, _)| f).collect()
    }

    fn table(text: &str, delim: char, cols: u16) -> Vec<String> {
        match render(text.as_bytes(), delim, cols, None, usize::MAX) {
            Payload::Markdown { doc, .. } => doc
                .lines
                .iter()
                .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect::<String>())
                .collect(),
            other => panic!("a table is a Markdown-shaped payload, got {other:?}"),
        }
    }

    #[test]
    fn plain_records_split_on_the_delimiter() {
        assert_eq!(fields("a,b\n1,2\n", ','), vec![vec!["a", "b"], vec!["1", "2"]]);
        // TSV, and a comma inside a field it has no reason to split on.
        assert_eq!(fields("a\tb\n1,5\t2\n", '\t'), vec![vec!["a", "b"], vec!["1,5", "2"]]);
        // CRLF, and a last line with no newline behind it.
        assert_eq!(fields("a,b\r\n1,2", ','), vec![vec!["a", "b"], vec!["1", "2"]]);
        // An empty field is a field.
        assert_eq!(fields("a,,c\n", ','), vec![vec!["a", "", "c"]]);
    }

    #[test]
    fn a_quoted_field_may_hold_anything() {
        // The delimiter, a doubled quote, and a newline: one field, one record.
        let got = fields("\"a,b\",\"say \"\"hi\"\"\",\"two\nlines\"\n", ',');
        assert_eq!(got, vec![vec!["a,b".to_string(), "say \"hi\"".to_string(), "two\nlines".to_string()]]);
    }

    /// A quoted newline makes a record longer than a line, so the line a record
    /// started on has to be tracked rather than counted.
    #[test]
    fn a_record_that_spans_lines_keeps_the_line_it_started_on() {
        let (recs, _) = records("a,b\n\"one\ntwo\",x\nlast,y\n", ',');
        let starts: Vec<usize> = recs.iter().map(|(_, src)| *src).collect();
        assert_eq!(starts, vec![0, 1, 3], "the third record starts on line 3, not line 2");
        // `DocLine.src` has to be monotonically non-decreasing for the
        // source/render jump to work, and these feed it directly.
        assert!(starts.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn the_columns_are_aligned_and_the_header_gets_a_rule() {
        let got = table("name,n\na,10\nbb,2\n", ',', 40);
        assert_eq!(got, ["name │  n", "─────┼───", "a    │ 10", "bb   │  2"]);
    }

    /// The numbers go right, the words stay left, and the header is not counted
    /// as data when deciding.
    #[test]
    fn a_column_of_numbers_is_right_aligned() {
        let got = table("label,count\nx,5\nyy,1000\n", ',', 40);
        assert_eq!(got[2], "x     │     5");
        assert_eq!(got[3], "yy    │  1000");
        // A column with a word in it is not numeric, however many numbers it has.
        let got = table("a,b\n1,1\n2,x\n", ',', 40);
        assert_eq!(got[3], "2 │ x");
    }

    #[test]
    fn a_single_record_gets_no_rule() {
        let got = table("only,one\n", ',', 40);
        assert_eq!(got, ["only │ one"], "a lone row is a row, not a header");
    }

    #[test]
    fn a_bom_is_not_part_of_the_first_cell() {
        let got = table("\u{feff}a,b\n1,2\n", ',', 40);
        assert!(got[0].starts_with('a'), "{got:?}");
    }

    #[test]
    fn a_ragged_file_still_lays_out() {
        // Fewer cells than the widest row, and more.
        let got = table("a,b,c\n1\n2,3,4,5\n", ',', 60);
        assert_eq!(got.len(), 4, "{got:?}");
        assert!(got.iter().all(|l| !l.is_empty()));
    }

    /// The cell is one cell however it was written, so a newline inside it does
    /// not turn into a second row.
    #[test]
    fn a_newline_inside_a_cell_stays_on_one_row() {
        let got = table("a,b\n\"two\nlines\",x\n", ',', 60);
        assert_eq!(got.len(), 3, "header, rule, one row: {got:?}");
        assert!(got[2].contains("two lines"), "{got:?}");
    }

    #[test]
    fn an_empty_file_is_not_a_table() {
        assert!(matches!(render(b"", ',', 40, None, usize::MAX), Payload::Text { .. }));
    }

    #[test]
    fn the_delimiter_follows_the_extension() {
        assert_eq!(delimiter(Some("tsv")), '\t');
        assert_eq!(delimiter(Some("TSV")), '\t');
        assert_eq!(delimiter(Some("csv")), ',');
        assert_eq!(delimiter(None), ',');
    }
}
