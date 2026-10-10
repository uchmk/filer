//! The file list column.
//!
//! Rows are painted directly rather than built from widgets: only the visible
//! window is touched, so a directory with 200k entries costs the same as one
//! with twenty.

use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, CornerRadius, FontId, Rect, Stroke, Ui, Vec2};

use crate::config::theme::{Style, Theme};
use crate::core::folder::{Folder, LoadState};
use crate::fs::Entry;
use crate::util;

pub struct ListStyle<'a> {
    pub theme: &'a Theme,
    pub font: FontId,
    pub row_h: f32,
    /// Dimmed columns (parent / preview) get a quieter cursor.
    pub active: bool,
    pub linemode: crate::fs::entry::Linemode,
    /// The largest total in the disk-usage view, which every row's bar is drawn
    /// against. Zero everywhere else, and no bar is drawn.
    pub usage_max: u64,
}

pub struct RowFlags {
    pub selected: bool,
    pub yanked: Option<bool>, // Some(true) = cut
    pub git: crate::fs::git::State,
}

pub struct ListResult {
    pub clicked: Option<usize>,
    pub double_clicked: Option<usize>,
    /// Right-click, which opens the context menu on that row.
    pub secondary_clicked: Option<usize>,
    /// The row a drag began on, the frame it began.
    pub drag_started: Option<usize>,
    /// The drag that began here has been let go, wherever the pointer is now.
    pub drag_stopped: bool,
    /// Wheel movement over this pane, in rows; the caller keeps the
    /// remainder, since it is the one that lives between frames.
    pub scroll_rows: f32,
    /// Modifiers held down for the click above.
    pub mods: egui::Modifiers,
    /// Each row's name as drawn, cut to the column, top to bottom.
    pub shown: Vec<String>,
}

pub fn draw(
    ui: &mut Ui,
    rect: Rect,
    folder: &Folder,
    st: &ListStyle<'_>,
    flags: &dyn Fn(&Entry) -> RowFlags,
    hits: bool,
    find: Option<&ito_match::Matcher>,
) -> ListResult {
    let painter = ui.painter_at(rect);
    let rows = ((rect.height() / st.row_h).floor() as usize).max(1);
    let mut out = ListResult {
        clicked: None,
        double_clicked: None,
        secondary_clicked: None,
        drag_started: None,
        drag_stopped: false,
        scroll_rows: 0.0,
        mods: egui::Modifiers::NONE,
        shown: Vec::new(),
    };

    match &folder.state {
        LoadState::Error(e) => {
            painter.text(
                rect.left_top() + Vec2::new(8.0, 6.0),
                Align2::LEFT_TOP,
                e,
                st.font.clone(),
                st.theme.progress_error,
            );
            return out;
        }
        LoadState::Loading if folder.entries.is_empty() => {
            painter.text(
                rect.left_top() + Vec2::new(8.0, 6.0),
                Align2::LEFT_TOP,
                "…",
                st.font.clone(),
                st.theme.fg_dim,
            );
            return out;
        }
        _ => {}
    }

    if folder.view.is_empty() {
        painter.text(
            rect.left_top() + Vec2::new(8.0, 6.0),
            Align2::LEFT_TOP,
            empty_label(&folder.path),
            st.font.clone(),
            st.theme.fg_dim,
        );
        return out;
    }

    let start = folder.offset.min(folder.view.len().saturating_sub(1));
    let end = (start + rows).min(folder.view.len());

    for (i, row) in (start..end).enumerate() {
        let Some(entry) = folder.at(row) else { continue };
        let y = rect.top() + i as f32 * st.row_h;
        let row_rect = Rect::from_min_size(
            egui::pos2(rect.left(), y),
            Vec2::new(rect.width(), st.row_h),
        );
        let f = flags(entry);
        let hovered = row == folder.cursor;

        if hovered {
            let bg = if st.active { st.theme.hovered_bg } else { st.theme.inactive_hovered_bg };
            painter.rect_filled(row_rect, CornerRadius::same(3), bg);
        }

        // Marker bar: selection state first, then the yank register.
        let marker = if f.selected {
            Some(st.theme.marker_selected)
        } else {
            f.yanked.map(|cut| if cut { st.theme.marker_cut } else { st.theme.marker_copied })
        };
        if let Some(color) = marker {
            painter.rect_filled(
                Rect::from_min_size(row_rect.left_top() + Vec2::new(1.0, 2.0), Vec2::new(3.0, st.row_h - 4.0)),
                CornerRadius::same(2),
                color,
            );
        }

        let mime = crate::mime::guess(entry);
        let style = st.theme.style_for(entry, mime);
        let base_color = style.fg.unwrap_or(st.theme.fg);
        let icon = st.theme.icon_for(entry);
        let mut x = row_rect.left() + 8.0;

        // An all-blank icon still holds the column: the ASCII fallback draws
        // plain files as a *space* (`Theme::without_nerd_icons`), and trimming
        // first would have dropped that column, starting file names one place
        // to the left of the folders' on the same list.
        if !icon.text.is_empty() {
            let g = painter.layout_no_wrap(
                icon.text.clone(),
                st.font.clone(),
                icon.fg.unwrap_or(base_color),
            );
            // Nerd Font glyphs differ in advance width, so they are padded to a
            // full em to line the names up. ASCII icons are monospace and line
            // up on their own; padding *those* to an em opens a gap wide enough
            // to read as indentation -- `/      config` instead of `/ config`.
            let w = if icon.text.is_ascii() { g.size().x } else { g.size().x.max(st.font.size) };
            painter.galley(
                egui::pos2(x, y + (st.row_h - g.size().y) / 2.0),
                g,
                base_color,
            );
            x += w + 8.0;
        }

        // Right-hand line mode text is laid out first so the name knows its budget.
        let right = linemode_text(entry, st.linemode);
        let mut right_w = 0.0;
        // The usage view's bar: the row's share of the biggest row, so the
        // largest folder fills it and the rest are read against that. Two
        // rectangles, the same pair the tasks panel draws its progress with.
        if st.usage_max > 0 {
            let frac = entry.usage_bytes() as f32 / st.usage_max as f32;
            let w = 44.0;
            let track = egui::Rect::from_min_size(
                egui::pos2(row_rect.right() - w - 6.0, y + (st.row_h - 4.0) / 2.0),
                egui::Vec2::new(w, 4.0),
            );
            painter.rect_filled(track, egui::CornerRadius::same(2), st.theme.border);
            if frac > 0.0 {
                painter.rect_filled(
                    egui::Rect::from_min_size(track.min, egui::Vec2::new(w * frac.min(1.0), 4.0)),
                    egui::CornerRadius::same(2),
                    st.theme.progress_fg,
                );
            }
            right_w = w + 12.0;
        }
        if !right.is_empty() {
            let g = painter.layout_no_wrap(right.clone(), st.font.clone(), st.theme.fg_dim);
            let g_w = g.size().x + 10.0;
            painter.galley(
                egui::pos2(row_rect.right() - right_w - g.size().x - 6.0, y + (st.row_h - g.size().y) / 2.0),
                g,
                st.theme.fg_dim,
            );
            right_w += g_w;
        }

        // The git sign sits between the name and the line mode, so it stays
        // put as the name grows and the two never collide.
        if let Some(mark) = f.git.mark() {
            let color = st.theme.git_color(f.git);
            let g = painter.layout_no_wrap(mark.to_string(), st.font.clone(), color);
            let w = g.size().x;
            painter.galley(
                egui::pos2(row_rect.right() - right_w - w - 2.0, y + (st.row_h - g.size().y) / 2.0),
                g,
                color,
            );
            right_w += w + 8.0;
        }

        let avail = (row_rect.right() - right_w - x - 6.0).max(16.0);
        // The filter's hits are worked out when the list is built; `/`, `?`
        // and the search views mark what they look for as the row is drawn.
        let found;
        let positions: &[usize] = match (hits, find) {
            (true, _) if !folder.hit_at(row).is_empty() => folder.hit_at(row),
            (_, Some(m)) => {
                found = m.positions(&entry.name);
                &found
            }
            (true, None) => folder.hit_at(row),
            (false, None) => &[],
        };
        // Too long for the row: cut inside the stem so the extension stays
        // readable (Q34, 24.2), and move the search hits along with the text.
        let width = |s: &str| painter.layout_no_wrap(s.to_owned(), st.font.clone(), base_color).size().x;
        let full = entry_name(&entry.name, &entry.kind);
        let (name, positions) = match elide_at(&full, avail, &width) {
            Some(cut) => {
                let near: Vec<&str> = [row.checked_sub(1), Some(row + 1)]
                    .into_iter()
                    .flatten()
                    .filter_map(|r| folder.at(r))
                    .map(|e| e.name.as_str())
                    .collect();
                elided(&full, positions, apart(&full, cut, &near))
            }
            None => (full, positions.to_vec()),
        };
        out.shown.push(name.clone());
        let job = name_job(&name, &positions, &st.font, base_color, &style, st.theme, avail);
        let galley = painter.layout_job(job);
        painter.galley(
            egui::pos2(x, y + (st.row_h - galley.size().y) / 2.0),
            galley,
            base_color,
        );
    }

    // Scrollbar hint when the list overflows.
    if folder.view.len() > rows {
        let track = Rect::from_min_size(
            egui::pos2(rect.right() - 3.0, rect.top()),
            Vec2::new(2.0, rect.height()),
        );
        let frac = rows as f32 / folder.view.len() as f32;
        let h = (track.height() * frac).max(16.0);
        let t = folder.offset as f32 / (folder.view.len() - rows) as f32;
        let y = track.top() + (track.height() - h) * t;
        painter.rect_filled(
            Rect::from_min_size(egui::pos2(track.left(), y), Vec2::new(2.0, h)),
            CornerRadius::same(1),
            st.theme.border,
        );
    }

    // Interaction
    let id = ui.id().with(("list", rect.left() as i32, rect.top() as i32));
    let resp = ui.interact(rect, id, egui::Sense::click_and_drag());
    // `hover_pos` is the fallback: a press that egui reports without an
    // interaction position still names the row the pointer is over.
    if let Some(pos) = resp.interact_pointer_pos().or_else(|| resp.hover_pos()) {
        let row = start + (((pos.y - rect.top()) / st.row_h).floor().max(0.0) as usize);
        if row < end {
            if resp.double_clicked() {
                out.double_clicked = Some(row);
            } else if resp.clicked() {
                out.clicked = Some(row);
            } else if resp.secondary_clicked() {
                out.secondary_clicked = Some(row);
            }

            if out.clicked.is_some() || out.double_clicked.is_some() {
                out.mods = ui.ctx().input(|i| i.modifiers);
            }
        }
    }
    // The row the button went down on, not the one under the pointer now:
    // egui calls it a drag only once the pointer has moved, and a quick pull
    // is rows away by then -- or past the last row, which started nothing.
    if resp.drag_started() {
        if let Some(at) = ui.ctx().input(|i| i.pointer.press_origin()) {
            let row = start + (((at.y - rect.top()) / st.row_h).floor().max(0.0) as usize);
            if row < end {
                out.drag_started = Some(row);
            }
        }
    }
    // Let go anywhere, not just over the row it started on.
    out.drag_stopped = resp.drag_stopped();
    if ui.rect_contains_pointer(rect) {
        let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
        out.scroll_rows = -scroll / st.row_h * 1.5;
    }
    out
}

/// The name as the row shows it: a link says so after its name.
fn entry_name(name: &str, kind: &crate::fs::Kind) -> String {
    match kind {
        crate::fs::Kind::Link { .. } => format!("{name}  ->"),
        _ => name.to_owned(),
    }
}

/// What an empty listing says. A host with no shares is a different answer
/// from a folder with no files, and `(empty)` read as the enumeration still
/// being out (#105).
fn empty_label(path: &std::path::Path) -> &'static str {
    if util::host_only_unc(path) { "(no shares)" } else { "(empty)" }
}

/// Where to cut a name that is wider than `max_width` (Q34): keep its first
/// `head` characters, then `…`, then everything from `tail` on -- the
/// extension and the last third of what is kept of the stem, so that
/// `report-2026-final.pdf` and `report-2026-final.docx` stay apart.
///
/// `None` when the name fits, and when it has no stem to speak of or the
/// extension is most of it; those are left to the plain cut at the end.
fn elide_at(name: &str, max_width: f32, width: &dyn Fn(&str) -> f32) -> Option<(usize, usize)> {
    if width(name) <= max_width {
        return None;
    }
    let chars: Vec<char> = name.chars().collect();
    let n = chars.len();
    let stem = crate::util::stem_and_ext(name).0.chars().count();
    let ext = n - stem;
    if stem < 2 || ext * 2 > n {
        return None;
    }
    // `keep` characters of the stem, two thirds at the front.
    let cut = |keep: usize| {
        let back = keep / 3;
        (keep - back, n - ext - back)
    };
    let shown = |(head, tail): (usize, usize)| -> String {
        chars[..head].iter().chain(['…'].iter()).chain(chars[tail..].iter()).collect()
    };
    let (mut lo, mut hi) = (1, stem - 1);
    if width(&shown(cut(lo))) > max_width {
        return None;
    }
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        match width(&shown(cut(mid))) <= max_width {
            true => lo = mid,
            false => hi = mid - 1,
        }
    }
    Some(cut(lo))
}

/// `cut` moved, within the same number of kept characters, so that `name`
/// no longer reads the same as a neighbouring row's name (Q67, #227: a
/// column of `notes…84` with the cursor on one of them). More of the head
/// is kept up to the first character the two differ in; failing that, more
/// of the tail. A neighbour that already reads differently, or one the
/// budget cannot tell apart, leaves the cut as it was.
fn apart(name: &str, (head, tail): (usize, usize), neighbours: &[&str]) -> (usize, usize) {
    let chars: Vec<char> = name.chars().collect();
    let n = chars.len();
    let ext = n - crate::util::stem_and_ext(name).0.chars().count();
    let keep = head + (n - ext - tail);
    let (mut head, mut back) = (head, n - ext - tail);
    for other in neighbours {
        let o: Vec<char> = other.chars().collect();
        let same = |head: usize, back: usize| {
            let tail = n - ext - back;
            o.len() >= head + (n - tail) && o[..head] == chars[..head] && o[o.len() - (n - tail)..] == chars[tail..]
        };
        if *other == name || !same(head, back) {
            continue;
        }
        let front = chars.iter().zip(&o).take_while(|(a, b)| a == b).count();
        let end = chars.iter().rev().zip(o.iter().rev()).take_while(|(a, b)| a == b).count();
        if front < keep && front + 1 > head {
            (head, back) = (front + 1, keep - front - 1);
        } else if end >= ext && end - ext < keep && end - ext + 1 > back {
            (head, back) = (keep - (end - ext + 1), end - ext + 1);
        }
    }
    (head, n - ext - back)
}

/// `name` cut where `elide_at` said, with the search hits that survived the
/// cut moved to where their characters now are.
fn elided(name: &str, hits: &[usize], (head, tail): (usize, usize)) -> (String, Vec<usize>) {
    let chars: Vec<char> = name.chars().collect();
    let text = chars[..head].iter().chain(['…'].iter()).chain(chars[tail..].iter()).collect();
    let moved = hits
        .iter()
        .filter_map(|&i| match i {
            i if i < head => Some(i),
            i if i >= tail => Some(head + 1 + (i - tail)),
            _ => None,
        })
        .collect();
    (text, moved)
}

fn name_job(
    name: &str,
    positions: &[usize],
    font: &FontId,
    color: Color32,
    style: &Style,
    theme: &Theme,
    max_width: f32,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = max_width;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');

    let fmt = |c: Color32, bg: Color32, underline: bool| TextFormat {
        font_id: font.clone(),
        color: c,
        background: bg,
        italics: style.italic,
        underline: if underline {
            Stroke::new(1.0, c)
        } else {
            Stroke::NONE
        },
        ..Default::default()
    };

    if positions.is_empty() {
        job.append(name, 0.0, fmt(color, Color32::TRANSPARENT, style.underline));
        return job;
    }

    let hl_fg = theme.find_keyword.fg.unwrap_or(color);
    let hl_bg = theme.find_keyword.bg.unwrap_or(Color32::TRANSPARENT);
    let mut buf = String::new();
    let mut buf_hl = false;
    for (i, ch) in name.chars().enumerate() {
        let is_hl = positions.contains(&i);
        if is_hl != buf_hl && !buf.is_empty() {
            let f = if buf_hl {
                fmt(hl_fg, hl_bg, false)
            } else {
                fmt(color, Color32::TRANSPARENT, style.underline)
            };
            job.append(&buf, 0.0, f);
            buf.clear();
        }
        buf_hl = is_hl;
        buf.push(ch);
    }
    if !buf.is_empty() {
        let f = if buf_hl {
            fmt(hl_fg, hl_bg, false)
        } else {
            fmt(color, Color32::TRANSPARENT, style.underline)
        };
        job.append(&buf, 0.0, f);
    }
    job
}

pub fn linemode_text(entry: &Entry, mode: crate::fs::entry::Linemode) -> String {
    use crate::fs::entry::Linemode as L;
    match mode {
        L::Size => entry.display_size().unwrap_or_default(),
        // The usage view's own mode: the measured total, files included, so a
        // folder and a file read on the same scale.
        // A folder nothing has measured -- `m u` outside `gu`'s view -- is
        // blank, as `size` leaves it: `0 B` read as an empty folder (#122).
        L::Usage if entry.usage.is_none() && entry.is_dir_like() => String::new(),
        L::Usage => match (entry.usage_cut, entry.usage_bytes()) {
            (true, 0) => "?".to_owned(),
            (true, n) => format!("≥ {}", crate::util::human_size(n)),
            (false, n) => crate::util::human_size(n),
        },
        L::Mtime => util::fmt_time(entry.modified, "%Y-%m-%d %H:%M"),
        L::Btime => util::fmt_time(entry.created, "%Y-%m-%d %H:%M"),
        L::Permissions => permissions(entry),
        // Asked for and not implemented -- see `Linemode::Owner`.
        L::Owner | L::None => String::new(),
    }
}

pub fn permissions(entry: &Entry) -> String {
    let mut s = String::new();
    s.push(match entry.kind {
        crate::fs::Kind::Dir => 'd',
        crate::fs::Kind::Link { .. } => 'l',
        crate::fs::Kind::File => '-',
    });
    s.push('r');
    s.push(if entry.readonly { '-' } else { 'w' });
    if entry.hidden {
        s.push('h');
    }
    s
}

/// The icon column, which is the one part of a row that has no words in it.
///
/// Reported from a screenshot rather than from [TESTING.md]: with no Nerd Font
/// installed the list read `/      config`, a gap wide enough that the folders
/// looked indented under the files. Nothing in the drawn strings says so, which
/// is what `Painted::placed` is for.
///
/// [TESTING.md]: ../../TESTING.md
#[cfg(test)]
mod icon_column {
    use crate::ui::harness::Screen;

    /// kura's config as it comes out on a machine with no icon font: what
    /// `apply_fonts` does to the theme when `install_fonts` finds no Nerd Font.
    fn without_icon_font() -> crate::config::Config {
        let mut cfg = crate::config::Config::load();
        std::sync::Arc::make_mut(&mut cfg.theme).without_nerd_icons();
        cfg
    }

    /// A folder and a plain file, listed in that order.
    fn one_of_each(label: &str) -> (std::path::PathBuf, std::sync::Arc<Vec<crate::fs::Entry>>) {
        let dir = crate::util::test_dir(label);
        std::fs::create_dir_all(dir.join("config")).unwrap();
        std::fs::write(dir.join("notes.txt"), "1").unwrap();
        let entries = std::sync::Arc::new(vec![
            crate::fs::Entry::from_path(dir.join("config")).unwrap(),
            crate::fs::Entry::from_path(dir.join("notes.txt")).unwrap(),
        ]);
        (dir, entries)
    }

    fn listing(cfg: crate::config::Config, dir: &std::path::Path, entries: std::sync::Arc<Vec<crate::fs::Entry>>) -> Screen {
        let mut s = Screen::with_config(cfg, dir.to_path_buf());
        s.app.tabs[s.app.active].current =
            crate::core::folder::Folder::from_entries(dir.to_path_buf(), entries, true);
        s
    }

    /// The ASCII fallback's icons are one monospace character, so the column is
    /// one character wide -- not one em, which is nearly twice that.
    ///
    /// The width is measured rather than asserted against a number: a second
    /// frame widens the folder's icon to `//`, and the distance the name moves
    /// is exactly what one character costs in whatever font the test ran with.
    #[test]
    fn an_ascii_icon_takes_one_character_of_room() {
        let (dir, entries) = one_of_each("icon-column");

        let one = listing(without_icon_font(), &dir, entries.clone()).draw();
        let (icon, name) = (
            one.placed("/").expect("the folder's fallback icon is drawn"),
            one.placed("config").expect("and its name"),
        );

        let mut wider = without_icon_font();
        std::sync::Arc::make_mut(&mut wider.theme).icon_dir_default.text = "//".into();
        let two = listing(wider, &dir, entries).draw();
        let per_char = two.placed("config").expect("the name moves right").x - name.x;
        assert!(per_char > 0.0, "widening the icon by a character moved the name: {per_char}");

        // One character, then the 8px pad the columns are separated by. What the
        // bug put in between was the difference between a character's advance
        // and a full em -- a monospace character is about 0.6 em, so the gap
        // came out around half again as wide as it should be.
        let gap = name.x - icon.x;
        assert!(
            (gap - (per_char + 8.0)).abs() < 0.5,
            "the name sits one character plus the pad past the icon: \
             gap {gap}, character {per_char}",
        );
    }

    /// Files and folders start their names in the same place.
    ///
    /// `without_nerd_icons` gives plain files a *space* rather than an empty
    /// string, on purpose: the column is still theirs. Trimming before asking
    /// whether there was an icon threw that away and left the file names one
    /// place to the left of the folders' on the same list.
    #[test]
    fn a_blank_icon_still_holds_its_column() {
        let (dir, entries) = one_of_each("icon-blank");
        let f = listing(without_icon_font(), &dir, entries).draw();

        let folder = f.placed("config").expect("the folder's name");
        let file = f.placed("notes.txt").expect("the file's name");
        assert!(
            (folder.x - file.x).abs() < 0.01,
            "both names start at the same x: folder {}, file {}",
            folder.x,
            file.x,
        );
    }
}

/// Q34 / 24.2: a long name is cut inside its stem, never through its
/// extension. Widths here are one per character, which is enough to pin where
/// the cut goes; the renderer passes the font's own measure.
#[cfg(test)]
mod elision {
    use super::{apart, elide_at, elided};

    fn chars(s: &str) -> f32 {
        s.chars().count() as f32
    }

    fn shown(name: &str, max: f32) -> String {
        match elide_at(name, max, &chars) {
            Some(cut) => elided(name, &[], cut).0,
            None => name.to_owned(),
        }
    }

    #[test]
    fn the_extension_survives_the_cut() {
        let long = format!("very-{}name.txt", "long-".repeat(30));
        let s = shown(&long, 30.0);
        assert!(s.ends_with("name.txt"), "the end of the stem and the extension: {s}");
        assert!(s.starts_with("very-long-"), "and the start: {s}");
        assert!(s.contains('…'));
        assert!(s.chars().count() <= 30, "{} chars: {s}", s.chars().count());
        // Two files that differ only in what they are stay apart.
        assert_ne!(shown("report-2026-final-draft.pdf", 16.0), shown("report-2026-final-draft.docx", 16.0));
    }

    #[test]
    fn short_or_odd_names_are_left_alone() {
        assert_eq!(elide_at("a.txt", 30.0, &chars), None, "it fits");
        assert_eq!(elide_at("x.averyveryverylongextension", 10.0, &chars), None, "the extension is most of it");
        assert_eq!(elide_at("ab.txt", 2.0, &chars), None, "no room even for one character and the extension");
    }

    /// Q67: rows that would read the same keep more of where they differ,
    /// within the same width; rows that already differ are left alone.
    #[test]
    fn neighbours_that_would_read_the_same_are_told_apart() {
        let show = |name: &str, near: &[&str]| {
            let cut = elide_at(name, 12.0, &chars).unwrap();
            elided(name, &[], apart(name, cut, near)).0
        };
        let a = "notes-archive-x-15484.log";
        let b = "notes-test-yy-15484.log";
        assert_eq!(shown(a, 12.0), "notes…84.log");
        assert_eq!(shown(b, 12.0), "notes…84.log", "the case: the two read the same");
        assert_eq!(show(a, &[b]), "notes-a….log");
        assert_eq!(show(b, &[a]), "notes-t….log");
        assert_eq!(show(a, &[b]).chars().count(), shown(a, 12.0).chars().count(), "no wider");
        // They differ near the end: more of the tail instead.
        let c = "build-output-run-1-x.log";
        let d = "build-output-run-2-x.log";
        assert_eq!(shown(c, 12.0), "build…-x.log");
        assert_eq!(show(c, &[d]), "buil…1-x.log");
        // Already apart: unchanged.
        assert_eq!(show(a, &["zzzzzzzzzzzzzzzzzzzzzz.log"]), shown(a, 12.0));
    }

    /// The search highlight moves with the characters it marked, and a hit
    /// inside the cut is dropped rather than landing on the wrong letter.
    #[test]
    fn hits_follow_their_characters() {
        let (text, hits) = elided("abcdefghij.txt", &[0, 5, 11], (3, 8));
        assert_eq!(text, "abc…ij.txt");
        assert_eq!(hits, vec![0, 7], "`a` stays, `f` was cut, the `t` of `.txt` moved from 11 to 7");
    }
}

#[cfg(test)]
mod empty {
    use super::empty_label;

    #[test]
    fn a_folder_with_nothing_in_it_is_empty() {
        assert_eq!(empty_label(&std::env::temp_dir().join("nothing")), "(empty)");
    }

    #[cfg(windows)]
    #[test]
    fn a_host_with_nothing_shared_says_so() {
        use std::path::Path;
        assert_eq!(empty_label(Path::new(r"\\fileserver")), "(no shares)");
        assert_eq!(empty_label(Path::new(r"\\fileserver\share")), "(empty)");
    }
}

#[cfg(test)]
mod usage_column {
    use super::linemode_text;
    use crate::fs::entry::Linemode;
    use crate::fs::Entry;

    /// TESTING.md 44.18 — `m u` outside `gu`'s view: a folder nothing has
    /// measured is blank rather than `0 B`, and a file still shows its size.
    #[test]
    fn an_unmeasured_folder_is_blank_not_empty() {
        let dir = crate::util::test_dir("usage-column");
        std::fs::create_dir_all(dir.join("big")).unwrap();
        std::fs::write(dir.join("big").join("x"), vec![b'x'; 2048]).unwrap();
        std::fs::write(dir.join("note.txt"), vec![b'x'; 1024]).unwrap();
        let mut big = Entry::from_path(dir.join("big")).unwrap();
        let note = Entry::from_path(dir.join("note.txt")).unwrap();
        assert_eq!(linemode_text(&big, Linemode::Usage), "");
        assert_eq!(linemode_text(&note, Linemode::Usage), "1.0 K");
        big.usage = Some(2048);
        assert_eq!(linemode_text(&big, Linemode::Usage), "2.0 K", "measured, it shows");
    }
}
