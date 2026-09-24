#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod core;
mod exec;
mod fs;
mod glob;
mod mime;
mod preview;
mod search;
mod spot;
mod terminal;
mod ui;
mod util;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use app::{App, Overlay};
use config::cmd::{Act, EscapeWhat, Step};
use config::keys::{self, Key};
use config::Config;

struct Cli {
    path: Option<PathBuf>,
    cwd_file: Option<PathBuf>,
    chooser_file: Option<PathBuf>,
}

fn parse_cli() -> Cli {
    let mut cli = Cli { path: None, cwd_file: None, chooser_file: None };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--cwd-file" => cli.cwd_file = args.next().map(PathBuf::from),
            "--chooser-file" => cli.chooser_file = args.next().map(PathBuf::from),
            "--help" | "-h" => {
                println!(
                    "filer — a yazi-flavored file manager\n\n\
                     USAGE:\n    filer [PATH] [--cwd-file FILE] [--chooser-file FILE]\n\n\
                     Config is read from yazi's config directory, then from filer's own.\n\
                     Press ~ or F1 inside the app for the key list."
                );
                std::process::exit(0);
            }
            other if !other.starts_with('-') => cli.path = Some(PathBuf::from(other)),
            _ => {}
        }
    }
    cli
}

fn main() -> eframe::Result<()> {
    let cli = parse_cli();
    let cfg = Config::load();

    // Where the window opens if the command line named nothing usable. The
    // path it did name is taken on faith: `is_dir` on a share that stopped
    // answering would hold the window back for half a minute, so the first
    // listing is what decides (see `App::start_unproven`).
    let home = std::env::current_dir()
        .ok()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let start = cli.path.as_deref().map(util::normalize).unwrap_or_else(|| home.clone());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([cfg.ui.window_width, cfg.ui.window_height])
            .with_min_inner_size([520.0, 360.0])
            .with_title("Filer"),
        ..Default::default()
    };

    eframe::run_native(
        "Filer",
        options,
        Box::new(move |cc| {
            let mut cfg = cfg;
            let (has_nerd, has_bold) = install_fonts(&cc.egui_ctx, &cfg);
            if !has_nerd && cfg.ui.icons != "nerd" {
                cfg.theme.without_nerd_icons();
            }
            if cfg.ui.icons == "none" {
                cfg.theme.without_nerd_icons();
            }
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            cc.egui_ctx.all_styles_mut(|s| {
                s.animation_time = 0.0;
                s.interaction.tooltip_delay = 0.4;
                s.spacing.item_spacing = egui::vec2(0.0, 0.0);
            });
            let mut a = App::new(cfg, start, cc.egui_ctx.clone());
            a.start_unproven(home);
            a.bold_font = has_bold;
            a.cwd_file = cli.cwd_file;
            a.chooser_file = cli.chooser_file;
            Ok(Box::new(Filer {
                app: a,
                title: String::new(),
                focused: true,
                last_input_frame: u64::MAX,
            }))
        }),
    )
}

/// Load a font that covers ASCII, CJK and (ideally) Nerd Font icons, plus
/// bold faces for the `bold` family when any can be found.
/// Returns whether the chosen font looks like a Nerd Font, and whether the
/// `bold` family was registered.
fn install_fonts(ctx: &egui::Context, cfg: &Config) -> (bool, bool) {
    let mut candidates: Vec<PathBuf> = cfg.ui.fonts.iter().map(PathBuf::from).collect();
    if let Some(local) = dirs::data_local_dir() {
        let user_fonts = local.join("Microsoft").join("Windows").join("Fonts");
        for name in [
            "HackGen35ConsoleNF-Regular.ttf",
            "HackGenConsoleNF-Regular.ttf",
            "HackGen35Console-Regular.ttf",
            "FiraCodeNerdFont-Regular.ttf",
            "CaskaydiaCoveNerdFont-Regular.ttf",
            "JetBrainsMonoNerdFont-Regular.ttf",
        ] {
            candidates.push(user_fonts.join(name));
        }
    }
    for name in ["meiryo.ttc", "YuGothM.ttc", "YuGothR.ttc", "msgothic.ttc", "consola.ttf"] {
        candidates.push(PathBuf::from(r"C:\Windows\Fonts").join(name));
    }

    let mut fonts = egui::FontDefinitions::default();
    let mut installed: Vec<String> = Vec::new();
    let mut loaded: Vec<PathBuf> = Vec::new();
    let mut has_nerd = false;

    for path in candidates {
        let name = font_stem(&path);
        if installed.contains(&name) || !load_face(&mut fonts, &path, &name) {
            continue;
        }
        let lower = name.to_lowercase();
        if lower.contains("nf") || lower.contains("nerd") {
            has_nerd = true;
        }
        installed.push(name);
        loaded.push(path);
        // One icon-capable font plus one CJK fallback is enough.
        if installed.len() >= 3 {
            break;
        }
    }

    for family in [egui::FontFamily::Monospace, egui::FontFamily::Proportional] {
        let list = fonts.families.entry(family).or_default();
        for (i, name) in installed.iter().enumerate() {
            list.insert(i, name.clone());
        }
    }

    // Bold: configured faces first, then the bold siblings of the regular
    // faces in use, then stock Windows faces.
    let mut bold_candidates: Vec<PathBuf> = cfg.ui.bold_fonts.iter().map(PathBuf::from).collect();
    for path in &loaded {
        bold_candidates.extend(bold_siblings(path));
    }
    for name in ["meiryob.ttc", "YuGothB.ttc", "consolab.ttf"] {
        bold_candidates.push(PathBuf::from(r"C:\Windows\Fonts").join(name));
    }
    let mut bold: Vec<String> = Vec::new();
    for path in bold_candidates {
        let name = format!("bold:{}", font_stem(&path));
        if bold.contains(&name) || !load_face(&mut fonts, &path, &name) {
            continue;
        }
        bold.push(name);
        // A Latin face and a CJK face at most; the regular faces cover the
        // rest (icons, symbols) through the fallback chain below.
        if bold.len() >= 2 {
            break;
        }
    }
    let has_bold = !bold.is_empty();
    if has_bold {
        let mut list = bold;
        list.extend(fonts.families[&egui::FontFamily::Monospace].iter().cloned());
        fonts.families.insert(egui::FontFamily::Name("bold".into()), list);
    }

    ctx.set_fonts(fonts);
    (has_nerd, has_bold)
}

fn font_stem(path: &std::path::Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "font".into())
}

fn load_face(fonts: &mut egui::FontDefinitions, path: &std::path::Path, name: &str) -> bool {
    let Ok(bytes) = std::fs::read(path) else { return false };
    let mut data = egui::FontData::from_owned(bytes);
    data.index = 0; // .ttc collections: take the first face
    fonts.font_data.insert(name.to_owned(), Arc::new(data));
    true
}

/// Where the bold face of a regular font usually lives.
fn bold_siblings(path: &std::path::Path) -> Vec<PathBuf> {
    let Some(dir) = path.parent() else { return Vec::new() };
    let stem = font_stem(path);
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let mut out = Vec::new();
    if let Some(base) = stem.strip_suffix("-Regular") {
        out.push(dir.join(format!("{base}-Bold.{ext}")));
    }
    match stem.to_lowercase().as_str() {
        "meiryo" => out.push(dir.join("meiryob.ttc")),
        "yugothm" | "yugothr" => out.push(dir.join("YuGothB.ttc")),
        "consola" => out.push(dir.join("consolab.ttf")),
        _ => {}
    }
    out
}

struct Filer {
    app: App,
    title: String,
    focused: bool,
    /// egui may run several layout passes per frame, all seeing the same input
    /// events. Keys must only be acted on once.
    last_input_frame: u64,
}

impl eframe::App for Filer {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.app.cfg.theme.bg.to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        self.app.drain_channels(&ctx);
        let frame_nr = ctx.cumulative_frame_nr();
        if frame_nr != self.last_input_frame {
            self.last_input_frame = frame_nr;
            handle_input(&mut self.app, &ctx);
        }
        self.app.kick_scans();
        self.app.request_preview(false);

        // A file may have changed while another program had focus.
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if focused && !self.focused {
            self.app.act(Act::Refresh);
        }
        self.focused = focused;

        let title = title_for(&self.app);
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.app.cfg.theme.bg))
            .show(ui, |ui| {
                ui::draw(&mut self.app, ui);
            });

        // egui walks focus across clickable rows on Tab / Shift+Tab, and a
        // focused row then takes Space / Enter as a click. The keymap owns
        // those keys, so nothing keeps focus outside of a text field or a
        // picker (the spot panel takes Tab to close).
        if !matches!(self.app.overlay, Overlay::Input(_) | Overlay::Pick(_)) {
            if let Some(id) = ctx.memory(|m| m.focused()) {
                ctx.memory_mut(|m| m.surrender_focus(id));
            }
        }

        if self.app.quit {
            self.app.on_quit();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Keep the frame loop alive only while something is actually pending.
        if self.app.preview.pending_since.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        if !self.app.toasts.is_empty()
            || self.app.search.is_some()
            || self.app.tasks.iter().any(|t| t.state == app::TaskState::Running)
        {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
    }
}

fn title_for(app: &App) -> String {
    let fmt = &app.cfg.yazi.mgr.title_format;
    let cwd = app.tab().cwd.display().to_string();
    if fmt.contains("{cwd}") {
        fmt.replace("{cwd}", &cwd)
    } else {
        format!("Filer: {cwd}")
    }
}

fn handle_input(app: &mut App, ctx: &egui::Context) {
    let events = ctx.input(|i| i.events.clone());
    for ev in events {
        match ev {
            egui::Event::Key { key, pressed: true, modifiers, .. } => {
                on_key_event(app, key, &modifiers);
            }
            // egui-winit turns the clipboard chords into these events and never
            // emits the keypress, so `<C-c>` and friends would never reach the
            // keymap. Put the chord back while no text field is focused.
            egui::Event::Copy if matches!(app.overlay, Overlay::None) => {
                app.feed_key(Key::ctrl('c'));
            }
            egui::Event::Cut if matches!(app.overlay, Overlay::None) => {
                app.feed_key(Key::ctrl('x'));
            }
            // The terminal takes a paste as text for the shell; everywhere
            // else it is the yank register's `p`.
            egui::Event::Paste(text) if app.term_focus => {
                if let Some(t) = &app.term {
                    t.paste(&text);
                }
            }
            egui::Event::Paste(_) if matches!(app.overlay, Overlay::None) => {
                app.feed_key(Key::ctrl('v'));
            }
            egui::Event::Text(text) => match &app.overlay {
                // Typing into the terminal is the text itself, not a keymap
                // lookup: the shell wants the characters.
                Overlay::None if app.term_focus => {
                    if let Some(t) = &app.term {
                        t.send(text.clone().into_bytes());
                    }
                }
                Overlay::None => {
                    for c in text.chars() {
                        app.feed_key(Key::char(c));
                    }
                }
                // Only the first character answers; the rest of the burst is dropped.
                Overlay::Confirm(_) => {
                    if let Some(c) = text.chars().next() {
                        app.answer_confirm(c);
                    }
                }
                Overlay::Help => {
                    for c in text.chars() {
                        match c {
                            'j' => app.help_scroll += 1,
                            'k' => app.help_scroll = app.help_scroll.saturating_sub(1),
                            'q' => app.overlay = Overlay::None,
                            _ => {}
                        }
                    }
                }
                Overlay::Tasks(_) => {
                    for c in text.chars() {
                        app.feed_tasks_key(Key::char(c));
                    }
                }
                Overlay::Spot(_) => {
                    for c in text.chars() {
                        app.feed_spot_key(Key::char(c));
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn on_key_event(app: &mut App, key: egui::Key, modifiers: &egui::Modifiers) {
    use egui::Key as K;
    match &app.overlay {
        Overlay::Input(_) => match key {
            K::Escape => app.cancel_input(),
            K::Enter => app.submit_input(),
            K::Tab => app.act(Act::Complete),
            _ => {}
        },
        Overlay::Pick(_) => {
            let step: Option<i64> = match key {
                K::ArrowDown => Some(1),
                K::ArrowUp => Some(-1),
                K::N if modifiers.ctrl => Some(1),
                K::P if modifiers.ctrl => Some(-1),
                K::PageDown => Some(10),
                K::PageUp => Some(-10),
                _ => None,
            };
            if let Some(d) = step {
                if let Overlay::Pick(p) = &mut app.overlay {
                    let len = p.matches.len() as i64;
                    if len > 0 {
                        p.cursor = (p.cursor as i64 + d).clamp(0, len - 1) as usize;
                    }
                }
                return;
            }
            match key {
                K::Escape => app.overlay = Overlay::None,
                K::Enter => app.submit_pick(),
                _ => {}
            }
        }
        Overlay::Confirm(_) => match key {
            K::Escape => app.overlay = Overlay::None,
            K::Enter => {
                let first = match &app.overlay {
                    Overlay::Confirm(c) => c.options.first().map(|(k, _)| *k),
                    _ => None,
                };
                if let Some(ch) = first {
                    app.answer_confirm(ch);
                }
            }
            _ => {}
        },
        Overlay::Help => match key {
            K::Escape => app.overlay = Overlay::None,
            K::ArrowDown => app.help_scroll += 1,
            K::ArrowUp => app.help_scroll = app.help_scroll.saturating_sub(1),
            K::PageDown => app.help_scroll += 20,
            K::PageUp => app.help_scroll = app.help_scroll.saturating_sub(20),
            _ => {}
        },
        Overlay::Tasks(_) => {
            if let Some(k) = keys::from_egui(key, modifiers) {
                app.feed_tasks_key(k);
            }
        }
        Overlay::Spot(_) => {
            if let Some(k) = keys::from_egui(key, modifiers) {
                app.feed_spot_key(k);
            }
        }
        // The terminal hears every key. The `[term]` layer keeps the few that
        // are the pane's own; the rest become the bytes a shell expects.
        Overlay::None if app.term_focus => {
            let Some(k) = keys::from_egui(key, modifiers) else { return };
            let mods = terminal::Mods {
                ctrl: modifiers.command || modifiers.ctrl,
                alt: modifiers.alt,
                shift: modifiers.shift,
            };
            let bytes = match special(key, mods.shift) {
                Some(s) => {
                    let app_cursor =
                        app.term.as_ref().is_some_and(|t| t.with_grid(terminal::app_cursor));
                    Some(terminal::encode(s, mods, app_cursor))
                }
                // A letter with Ctrl held is a control code; egui sends no
                // Text event for those, so this is where they are made.
                None if mods.ctrl => key.name().chars().next().and_then(|c| {
                    terminal::control_code(c.to_ascii_lowercase(), mods.alt)
                }),
                None => None,
            };
            app.feed_term_key(k, bytes);
        }
        Overlay::None => {
            if let Some(k) = keys::from_egui(key, modifiers) {
                app.feed_key(k);
            }
        }
    }
}

/// The egui keys a terminal has an escape sequence for. Anything else is
/// either text (which arrives as its own event) or nothing a shell wants.
fn special(key: egui::Key, shift: bool) -> Option<terminal::Special> {
    use egui::Key as K;
    use terminal::Special as S;
    Some(match key {
        K::Enter => S::Enter,
        K::Backspace => S::Backspace,
        // Shift+Tab is its own sequence, not Tab with a modifier on it.
        K::Tab if shift => S::BackTab,
        K::Tab => S::Tab,
        K::Escape => S::Escape,
        K::ArrowUp => S::Up,
        K::ArrowDown => S::Down,
        K::ArrowRight => S::Right,
        K::ArrowLeft => S::Left,
        K::Home => S::Home,
        K::End => S::End,
        K::PageUp => S::PageUp,
        K::PageDown => S::PageDown,
        K::Insert => S::Insert,
        K::Delete => S::Delete,
        K::F1 => S::F(1),
        K::F2 => S::F(2),
        K::F3 => S::F(3),
        K::F4 => S::F(4),
        K::F5 => S::F(5),
        K::F6 => S::F(6),
        K::F7 => S::F(7),
        K::F8 => S::F(8),
        K::F9 => S::F(9),
        K::F10 => S::F(10),
        K::F11 => S::F(11),
        K::F12 => S::F(12),
        _ => return None,
    })
}

// Keep these referenced so the compiler checks them even before every command
// is wired to a key.
#[allow(dead_code)]
fn _unused(app: &mut App) {
    app.act(Act::Escape(EscapeWhat::default()));
    app.act(Act::Arrow(Step::Rel(1)));
}
