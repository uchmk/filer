//! The terminal pane, drawn by `tsumugi-pane`'s egui view in the theme's
//! colours. What only filer can do -- give the pane the keys, the clipboard,
//! and saying so when the clipboard will not open -- is done here with what
//! the view hands back.

use egui::{FontId, Rect, Stroke, Ui};

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
    }
}

pub fn draw(app: &mut App, ui: &mut Ui, rect: Rect, f: &FontId, row_h: f32) {
    let pal = palette(&app.cfg.theme);
    // A panel over the pane owns the wheel; see `Overlay::is_modal`.
    let opts = tsumugi_pane::ViewOptions { focused: app.term_focus, wheel: !app.overlay.is_modal() };
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
    // Letting go of a selection copies it, which is what a terminal means by
    // selecting: there is no other step.
    if let Some(text) = shown.copy {
        let _ = crate::exec::set_clipboard(&text);
    }
    // Right-click pastes. `<C-v>` reaches the pane as egui's paste event; a
    // right-click is not one, so the clipboard is read here.
    if shown.paste {
        match crate::exec::get_clipboard() {
            Ok(text) if !text.is_empty() => {
                if let Some(term) = &app.term {
                    term.paste(&text);
                }
            }
            Ok(_) => {}
            // Said out loud because the right-click looked like it did nothing.
            Err(e) => app.error(format!("Could not read the clipboard: {e}")),
        }
    }
}
