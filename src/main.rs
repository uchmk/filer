#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod bugreport;
mod envreport;
mod runinfo;
mod config;
mod core;
mod diff;
mod exec;
mod fs;
mod glob;
mod keyscript;
mod mime;
mod preview;
mod rename;
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
    /// `--keys`: pressed by filer itself once it has started (Q24).
    keys: Vec<Key>,
}

/// Put a line where whoever typed the command is looking.
///
/// A release build is a GUI-subsystem binary — see the attribute at the top of
/// this file, which is what keeps a console from flashing up behind the window.
/// The cost is that Windows hands it no standard output, so `println!` from
/// `--help` or `--version` writes to nothing and the command appears to do
/// nothing at all. Attaching to the console that launched us and writing to
/// `CONOUT$` puts the text back on the screen.
///
/// Every branch here is a real case. A debug build already owns a console, so
/// `AttachConsole` refuses and the ordinary path is correct. A release build
/// started by double-clicking has no parent console to attach to, and the text
/// goes nowhere — which is what should happen, since nobody asked for it.
///
/// **Redirected output comes first.** `CONOUT$` is the screen, not standard
/// output, so until v0.54.4 `filer env > out.txt` wrote an empty file and
/// `filer env | Select-String arch` printed the whole report unfiltered -- the
/// very captures a bug report is made of, silently lost. A GUI binary still
/// inherits whatever handles its parent redirected, so when standard output is
/// a file or a pipe, the text goes there; the console path is only for a real
/// console. Reported four times by the real-machine runs (#81, #84, #88, #93).
#[cfg(windows)]
fn say(text: &str) {
    use std::io::Write;
    use windows::Win32::Storage::FileSystem::{GetFileType, FILE_TYPE_DISK, FILE_TYPE_PIPE};
    use windows::Win32::System::Console::{
        AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_OUTPUT_HANDLE,
    };

    let redirected = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }
        .ok()
        .filter(|h| !h.is_invalid())
        .is_some_and(|h| {
            let kind = unsafe { GetFileType(h) };
            kind == FILE_TYPE_DISK || kind == FILE_TYPE_PIPE
        });
    if redirected {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{text}");
        let _ = out.flush();
        return;
    }
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_ok() {
        if let Ok(mut out) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
            let _ = writeln!(out, "{text}");
            return;
        }
    }
    println!("{text}");
}

#[cfg(not(windows))]
fn say(text: &str) {
    println!("{text}");
}

fn parse_cli() -> Cli {
    let mut cli = Cli { path: None, cwd_file: None, chooser_file: None, keys: Vec::new() };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--cwd-file" => cli.cwd_file = args.next().map(PathBuf::from),
            "--chooser-file" => cli.chooser_file = args.next().map(PathBuf::from),
            // Checked before any window opens: a script that cannot be typed
            // should say so on the command line, not do half of itself.
            "--keys" => {
                let script = args.next().unwrap_or_default();
                match keyscript::parse(&script) {
                    Ok(keys) => match keys.iter().find(|k| keyscript::events(k).is_none()) {
                        Some(k) => {
                            say(&format!("filer: --keys: {k:?} cannot be typed"));
                            std::process::exit(2);
                        }
                        None => cli.keys = keys,
                    },
                    Err(why) => {
                        say(&format!("filer: --keys: {why}"));
                        std::process::exit(2);
                    }
                }
            }
            "--help" | "-h" => {
                say(
                    "filer — a yazi-flavored file manager\n\n\
                     USAGE:\n    filer [PATH] [--cwd-file FILE] [--chooser-file FILE] [--keys KEYS]\n\n\
                     OPTIONS:\n    -h, --help       this text\n    \
                     -V, --version    the version and the architecture\n    \
                     --keys KEYS      press these keys once started, in keymap notation:\n                     \
                     \"<Tab>C\" opens spot and copies it. For scripted checks\n\n\
                     COMMANDS:\n    env              config files, outside tools and environment,\n                     \
                     for pasting into a bug report\n\n\
                     Config is read from yazi's config directory, then from filer's own.\n\
                     Press ~ or F1 inside the app for the key list.",
                );
                std::process::exit(0);
            }
            // A bug report needs to name a version, and a downloaded binary can
            // be renamed away from the one in the release asset's filename, so
            // the binary has to be able to say which it is. `-V` rather than
            // `-v`, which is conventionally verbosity.
            // The architecture is here because there are now two Windows
            // builds and a bug report has to say which one is running. The
            // release asset's filename carries it, but a file can be renamed
            // and an emulated x64 binary on an ARM64 machine will insist it is
            // on x64 -- which is exactly the confusion worth heading off.
            // Printed rather than opened in a browser, unlike `<F12>`: this
            // is text to paste into a report that already exists, and the
            // questions it answers are ones only the machine can.
            "env" | "--env" => {
                say(&crate::envreport::text());
                std::process::exit(0);
            }
            "--version" | "-V" => {
                say(&format!(
                    "filer {} ({})",
                    env!("CARGO_PKG_VERSION"),
                    std::env::consts::ARCH
                ));
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

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([cfg.ui.window_width, cfg.ui.window_height])
        .with_min_inner_size([520.0, 360.0])
        .with_title("Filer");
    // The title bar, Alt+Tab and the taskbar button. `filer.exe`'s own icon is
    // a resource compiled in by `build.rs`, from an `.ico` made of this same
    // drawing. Without an icon the window still opens.
    if let Some(icon) = app_icon(ICON_SVG, 256) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };

    eframe::run_native(
        "Filer",
        options,
        Box::new(move |cc| {
            let mut cfg = cfg;
            // Filled as the run sets itself up, and written down at the end of
            // it: `filer env` cannot work either of these out for itself.
            let mut used = crate::runinfo::RunInfo {
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            };
            if let Some(rs) = cc.wgpu_render_state.as_ref() {
                let info = rs.adapter.get_info();
                used.adapter = info.name;
                used.backend = format!("{:?}", info.backend);
                used.device = format!("{:?}", info.device_type);
            }
            let has_bold = apply_fonts(&cc.egui_ctx, &mut cfg, &mut used);
            crate::runinfo::save(&used);
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            // egui zooms on Ctrl +/-/0 of its own accord, at the end of the
            // frame, without consuming the key first. Every one of those is a
            // key filer binds, so both would run -- `<C-->` hardlinked *and*
            // shrank the window. Zoom is a filer command now, in the keymap
            // with everything else.
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
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
                last_geometry: None,
                script: cli.keys.iter().filter_map(keyscript::events).collect(),
                script_at: (0, std::time::Instant::now()),
            }))
        }),
    )
}

/// The window's icon, rasterized from SVG at `px` square.
///
/// The source stays vector so there is one file to change and no binary blob in
/// the tree, and `resvg` is already here for the SVG previews — an icon costs no
/// new dependency. `None` rather than an error: a window with the wrong icon is
/// worth having, a window that refuses to open is not.
///
/// This is the *window's* icon — the title bar, Alt+Tab and the taskbar button.
/// The icon Explorer draws on `filer.exe` itself is a resource compiled into the
/// binary, which is a separate thing and not this.
/// The artwork, kept as a vector so there is one file to change. `build.rs`
/// works from `assets/icon.ico`, which `cargo run --example make-icon` rebuilds
/// from this same file.
const ICON_SVG: &[u8] = include_bytes!("../assets/icon.svg");

fn app_icon(svg: &[u8], px: u32) -> Option<egui::IconData> {
    use resvg::{tiny_skia, usvg};

    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let size = tree.size();
    // Fit the square, keeping the aspect ratio, and centre what is left over.
    let scale = (px as f32 / size.width()).min(px as f32 / size.height());
    let dx = (px as f32 - size.width() * scale) / 2.0;
    let dy = (px as f32 - size.height() * scale) / 2.0;
    let mut pixmap = tiny_skia::Pixmap::new(px, px)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale).post_translate(dx, dy),
        &mut pixmap.as_mut(),
    );
    // `IconData` wants straight alpha; tiny-skia works premultiplied.
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Some(egui::IconData { rgba, width: px, height: px })
}

/// Install the fonts `cfg` asks for and fold the outcome back into it: without
/// a Nerd Font the icon glyphs would come out as boxes, and `icons = "none"`
/// asks for none either way. Returns whether a bold face was found.
///
/// Startup and `config_reload` both go through here, so a reloaded config gets
/// exactly the fonts a fresh start would have given it.
fn apply_fonts(ctx: &egui::Context, cfg: &mut Config, used: &mut crate::runinfo::RunInfo) -> bool {
    let (has_nerd, has_bold) = install_fonts(ctx, cfg, used);
    if (!has_nerd && cfg.ui.icons != "nerd") || cfg.ui.icons == "none" {
        // The one place the theme is changed after `Config::load` built it, and
        // it runs once at startup or on a reload -- so paying for a copy here is
        // what keeps every frame from paying for one.
        std::sync::Arc::make_mut(&mut cfg.theme).without_nerd_icons();
    }
    has_bold
}

/// Load a font that covers ASCII, CJK and (ideally) Nerd Font icons, plus
/// bold faces for the `bold` family when any can be found.
/// Returns whether the chosen font looks like a Nerd Font, and whether the
/// `bold` family was registered.
fn install_fonts(
    ctx: &egui::Context,
    cfg: &Config,
    used: &mut crate::runinfo::RunInfo,
) -> (bool, bool) {
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

    used.fonts = loaded.clone();

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
    let mut bold_used: Vec<PathBuf> = Vec::new();
    for path in bold_candidates {
        let name = format!("bold:{}", font_stem(&path));
        if bold.contains(&name) || !load_face(&mut fonts, &path, &name) {
            continue;
        }
        bold.push(name);
        bold_used.push(path);
        // A Latin face and a CJK face at most; the regular faces cover the
        // rest (icons, symbols) through the fallback chain below.
        if bold.len() >= 2 {
            break;
        }
    }
    // The paths rather than the family names, which is what a reader can go
    // and look at: "no bold" and "bold came from a face you did not expect"
    // are different complaints with the same symptom.
    used.bold = bold_used;
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
    /// The geometry last written to `last-run.toml`, so that writing it again
    /// costs nothing while nothing moves. A resize or a drag onto a monitor at
    /// another scale changes it; a frame does not.
    last_geometry: Option<([f32; 2], f32)>,
    /// `--keys`, as the events each press arrives as, still to be pressed.
    script: std::collections::VecDeque<Vec<egui::Event>>,
    /// The frame and the moment the last scripted key went in.
    script_at: (u64, std::time::Instant),
}

impl Filer {
    /// Put the window's own view of its size into `last-run.toml`.
    ///
    /// Only when it has moved: every frame would rewrite the file for nothing.
    /// The record is carried over rather than rebuilt, for the same reason the
    /// font reload carries it -- the adapter cannot be re-read without holding
    /// the render state, and a geometry change is no reason to lose it.
    fn record_geometry(&mut self, ctx: &egui::Context) {
        let (size, ppp) = ctx.input(|i| (i.viewport_rect().size(), i.pixels_per_point()));
        let now = ([size.x, size.y], ppp);
        // A window being dragged is resized every frame, and each one would be
        // a write. Round to the pixel before comparing: below that nobody is
        // reading this file anyway.
        let rounded = ([now.0[0].round(), now.0[1].round()], (now.1 * 1000.0).round() / 1000.0);
        if self.last_geometry == Some(rounded) {
            return;
        }
        self.last_geometry = Some(rounded);
        let mut used = crate::runinfo::load().unwrap_or_default();
        used.version = env!("CARGO_PKG_VERSION").into();
        used.window_pt = rounded.0;
        used.ppp = rounded.1;
        crate::runinfo::save(&used);
    }
}

impl eframe::App for Filer {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.app.cfg.theme.bg.to_normalized_gamma_f32()
    }

    /// `--keys`: the next scripted key, in the raw input a keyboard would have
    /// filled -- so it takes every road a real press takes.
    ///
    /// One key at a time, and only once what the last one started has landed
    /// (`App::settled`): `<Tab>C` copying a panel whose sections are still on
    /// the worker would copy half of it. Two frames apart at least, so what the
    /// key opened has been drawn once; and never more than five seconds behind,
    /// so a thing that never settles delays the script rather than stopping it.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if self.script.is_empty() {
            return;
        }
        ctx.request_repaint();
        let frame = ctx.cumulative_frame_nr();
        let (last_frame, last_at) = self.script_at;
        let waited_long = last_at.elapsed() > Duration::from_secs(5);
        if frame < last_frame + 2 || !(self.app.settled() || waited_long) {
            return;
        }
        if let Some(events) = self.script.pop_front() {
            raw_input.events.extend(events);
            self.script_at = (frame, std::time::Instant::now());
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        self.app.drain_channels(&ctx);
        self.record_geometry(&ctx);
        let frame_nr = ctx.cumulative_frame_nr();
        if frame_nr != self.last_input_frame {
            self.last_input_frame = frame_nr;
            handle_input(&mut self.app, &ctx);
        }
        // `config_reload` may have named different fonts, and only the frame
        // loop can install a face.
        if self.app.refont {
            self.app.refont = false;
            // The record follows: a reload can name different fonts, and the
            // whole point of writing it down is that it says what is in use
            // now. The adapter is carried over -- it cannot change without a
            // restart, and re-reading it here would mean holding the render
            // state for the life of the program to answer a question nobody
            // has yet asked.
            let mut used = crate::runinfo::load().unwrap_or_default();
            used.version = env!("CARGO_PKG_VERSION").into();
            self.app.bold_font = apply_fonts(&ctx, &mut self.app.cfg, &mut used);
            crate::runinfo::save(&used);
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

/// `pub(crate)` for [`crate::ui::harness`]: a test that drives the program with
/// `egui::Event`s has to enter through the same door the window does. The chord
/// rules below -- Windows sending a keypress *and* the character it would have
/// typed -- are only correct as a pair, and asserting on them from anywhere
/// else would be asserting on a copy.
pub(crate) fn handle_input(app: &mut App, ctx: &egui::Context) {
    let events = ctx.input(|i| i.events.clone());
    // Windows sends a chord *and* the character it would have typed: `<A-m>`
    // arrives as a key event with alt set and then as `Text("m")`, so one
    // keystroke ran `send_pane --cut` and went on to open "Save bookmark as…"
    // as well. The chord has already been dealt with by the time the text
    // turns up, so the text is dropped.
    //
    // Ctrl and Alt together is left alone. That combination is AltGr, which is
    // how a German keyboard types `@` and a French one `€`; there the
    // character is the whole point and there is no chord to have consumed it.
    let mut swallow_text = false;
    for ev in events {
        match ev {
            egui::Event::Key { key, pressed: true, modifiers, .. } => {
                swallow_text = modifiers.alt && !modifiers.ctrl;
                on_key_event(app, key, &modifiers);
            }
            egui::Event::Text(_) if swallow_text => swallow_text = false,
            // egui-winit turns the clipboard chords into these events and never
            // emits the keypress, so `<C-c>` and friends would never reach the
            // keymap. Put the chord back while no text field is focused.
            //
            // Through `on_key_event` rather than `feed_key`, because the
            // terminal is not an overlay: `Overlay::None` is still true while a
            // shell has the keys, and feeding the keymap there ran `[mgr]`
            // `close`. So `<C-c>` -- the one key everybody presses to stop a
            // command -- closed the tab, and on the last one quit filer and
            // took the shell with it. `on_key_event` is where the `term_focus`
            // arm turns the chord into the control code the shell is waiting
            // for, which `control_code` could already produce and nothing was
            // reaching.
            egui::Event::Copy if matches!(app.overlay, Overlay::None) => {
                on_key_event(app, egui::Key::C, &chord_ctrl());
            }
            egui::Event::Cut if matches!(app.overlay, Overlay::None) => {
                on_key_event(app, egui::Key::X, &chord_ctrl());
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
                // The four panels with a keymap layer of their own. Which layer
                // is the overlay's own business, so they share one arm.
                Overlay::Help | Overlay::Tasks(_) | Overlay::Spot(_) | Overlay::Diff(_) => {
                    for c in text.chars() {
                        app.feed_overlay_key(Key::char(c));
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}

/// The modifiers a real `Ctrl`+letter arrives with, for the chords egui-winit
/// swallows into `Copy` and `Cut` before any key event is made.
///
/// `command` alongside `ctrl` is how egui reports the press on Windows and
/// Linux, and the terminal reads either.
fn chord_ctrl() -> egui::Modifiers {
    egui::Modifiers { ctrl: true, command: true, ..Default::default() }
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
        Overlay::Help | Overlay::Tasks(_) | Overlay::Spot(_) | Overlay::Diff(_) => {
            if let Some(k) = keys::from_egui(key, modifiers) {
                app.feed_overlay_key(k);
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
            // Once the other end has asked for win32-input-mode (ConPTY does,
            // as it starts), every key whose VT form begins with ESC goes as a
            // key record, as Windows Terminal sends them: a tcell program then
            // cannot mistake `<Esc>` and the key after it for one sequence
            // (Q27, #99). Plain text still goes as text.
            let win32 = app.term.as_ref().is_some_and(|t| t.win32_input());
            let vt = match special(key, mods.shift) {
                // On Windows `Esc` goes as one win32-input-mode key press, not
                // as a plain ESC. ConPTY makes a press *and a release* out of a
                // plain ESC, and the release, reaching a tcell program right
                // behind the press, turns the key into an Alt prefix that never
                // resolves -- so `Esc` did nothing in lazygit or gh-dash. See
                // `terminal::win32_key`.
                Some(terminal::Special::Escape) if cfg!(windows) => {
                    Some(terminal::win32_key(0x1b, 1, 0x1b, mods))
                }
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
                // The same goes for Alt without Ctrl: no Text event arrives for
                // it, so a key that the `[term]` layer does not claim used to
                // reach the shell as nothing at all. `Alt-b` and `Alt-f` are
                // readline's word motions, and they never moved.
                None if mods.alt => keys::printable(key).map(|c| {
                    let c = if mods.shift { c.to_ascii_uppercase() } else { c };
                    terminal::meta_char(c)
                }),
                None => None,
            };
            // A chord with no record form (`Ctrl+[`, `Ctrl+Space`) keeps its
            // old bytes rather than going missing.
            let bytes = match (win32, special(key, mods.shift)) {
                (true, Some(s)) => Some(terminal::special_record(s, mods)),
                (true, None) if mods.ctrl || mods.alt => keys::printable(key)
                    .or_else(|| key.name().chars().next())
                    .and_then(|c| terminal::char_record(c, mods))
                    .or(vt),
                _ => vt,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A square icon out of a non-square drawing: the art keeps its shape and
    /// the leftover is transparent, rather than being stretched to fit.
    #[test]
    fn the_icon_is_square_and_keeps_the_drawing_in_proportion() {
        // 40 wide, 20 tall, filled edge to edge.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20">
            <rect width="40" height="20" fill="#ff8800"/></svg>"##;

        let icon = app_icon(svg, 64).expect("a valid SVG rasterizes");

        assert_eq!((icon.width, icon.height), (64, 64));
        assert_eq!(icon.rgba.len(), 64 * 64 * 4);

        let at = |x: usize, y: usize| {
            let i = (y * 64 + x) * 4;
            (icon.rgba[i], icon.rgba[i + 1], icon.rgba[i + 2], icon.rgba[i + 3])
        };
        // The middle row is the drawing, scaled to the full width.
        assert_eq!(at(32, 32), (0xff, 0x88, 0x00, 0xff));
        // Above and below it, the padding is clear rather than stretched paint.
        assert_eq!(at(32, 2).3, 0, "top band is transparent");
        assert_eq!(at(32, 61).3, 0, "bottom band is transparent");
    }

    #[test]
    fn a_broken_icon_does_not_stop_the_window_opening() {
        assert!(app_icon(b"not an svg at all", 64).is_none());
        assert!(app_icon(b"", 64).is_none());
    }
}
