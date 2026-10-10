//! The terminal pane, drawn by `tsumugi-pane`'s egui view in the theme's
//! colours. What only filer can do -- give the pane the keys, the clipboard,
//! and saying so when the clipboard will not open -- is done here with what
//! the view hands back.

use egui::{pos2, vec2, CornerRadius, FontId, Rect, Stroke, Ui};

use crate::app::App;
use crate::config::theme::Theme;

/// The pane's colours out of the theme: the pane sits on `bg_alt`, a selection
/// takes the list's cursor-row background, and the cursor the `cwd` colour.
fn palette(theme: &Theme) -> tsumugi_pane::Palette {
    tsumugi_pane::Palette {
        bg: theme.bg_alt,
        fg: theme.fg,
        fg_dim: theme.fg_dim,
        selection: theme.hovered_bg,
        cursor: theme.cwd.fg.unwrap_or(theme.fg),
        on_cursor: theme.bg,
        ansi: None,
    }
}

pub fn draw(app: &mut App, ui: &mut Ui, rect: Rect, f: &FontId, row_h: f32) {
    let pal = palette(&app.cfg.theme);
    // A panel over the pane owns the wheel; see `Overlay::is_modal`. Letting
    // go of a selection always copies it (below), and filer has no triggers.
    let opts = tsumugi_pane::ViewOptions { focused: app.term_focus, wheel: !app.overlay.is_modal(), copy_on_select: true, highlights: &[] };
    let mut state = tsumugi_pane::ViewState { scroll_rows: app.term_scroll_rows };
    let shown = tsumugi_pane::show(ui, app.term.as_mut(), &mut state, rect, f, row_h, &pal, opts);
    app.term_scroll_rows = state.scroll_rows;
    if shown.focus {
        app.term_focus = true;
    }
    // Accented while the pane has the keys, the same as the outline's rule and
    // in the same colour -- see `super::focus_rule` for why it is shared.
    let rule = super::focus_rule(&app.cfg.theme, app.term_focus);
    ui.painter_at(rect).line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, rule));
    // A full-screen program covers every row, so the way back out is put in
    // the corner while it runs (Q77). On a plate, as the preview's zoom note
    // is: it sits over the program's first row and has to read over anything.
    if let Some(badge) = app.term_leave_badge() {
        let theme = &app.cfg.theme;
        let painter = ui.painter_at(rect);
        let g = painter.layout_no_wrap(badge, FontId::new(f.size * 0.85, f.family.clone()), theme.fg_dim);
        let at = pos2(rect.right() - g.size().x - 10.0, rect.top() + 4.0);
        let plate = Rect::from_min_size(at, g.size()).expand2(vec2(6.0, 1.0));
        painter.rect_filled(plate, CornerRadius::same(4), theme.bg.gamma_multiply(0.85));
        painter.galley(at, g, theme.fg_dim);
    }
    // Letting go of a selection copies it, which is what a terminal means by
    // selecting: there is no other step.
    if let Some(text) = shown.copy {
        let _ = crate::exec::set_clipboard(&text);
    }
    // Ctrl+click (Cmd on a Mac) on a link: a web address goes to the browser,
    // a path is shown in the list -- this is a file manager, and the list is
    // where the next thing to do with a file is one key away.
    match shown.open {
        Some(tsumugi_pane::Link::Url(url)) => {
            if let Err(e) = crate::exec::open_url(&url) {
                app.error(format!("Could not open {url}: {e}"));
            }
        }
        Some(tsumugi_pane::Link::Path { path, .. }) => {
            // Relative to the shell, which is where it was printed.
            let base = app.term.as_ref().and_then(|t| t.shell_cwd.clone());
            let target = match base {
                Some(dir) => crate::util::resolve_against(&dir, &path).to_string_lossy().into_owned(),
                None => path,
            };
            app.reveal(target);
        }
        None => {}
    }
    // Right-click pastes. `<C-v>` reaches the pane as egui's paste event; a
    // right-click is not one, so the clipboard is read here.
    if shown.paste {
        match crate::exec::get_clipboard() {
            Ok(text) if !text.is_empty() => app.paste_to_term(&text),
            Ok(_) => {}
            // Said out loud because the right-click looked like it did nothing.
            Err(e) => app.error(format!("Could not read the clipboard: {e}")),
        }
    }
}
