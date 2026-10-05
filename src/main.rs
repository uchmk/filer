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
mod shellhook;
mod spot;
mod terminal;
mod ui;
mod util;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
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
    keys: Vec<keyscript::Step>,
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
///
/// **What reaches it is up to the shell.** `cmd`'s `>` and a pipe into another
/// command hand it the file or the pipe. PowerShell does not wait for a
/// windowed program at the end of a pipeline, so its own `>` and a bare
/// `$v = & filer.exe env` connect nothing and come back empty, whatever this
/// does (#176, #183). That is why the release zip has `filer.com`, the console
/// front PowerShell does wait for (v0.71.0, `src/bin/filer-com.rs`).
#[cfg(windows)]
fn say(text: &str) {
    say_on(text, false);
}

/// [`say`] on standard error, for a line about the output rather than the
/// output itself: `filer: wrote <path>` on standard output was captured along
/// with everything else by `$p = & filer env --out x.txt` and handed on as a
/// path (#200). On a console the two look the same.
#[cfg(windows)]
fn say_err(text: &str) {
    say_on(text, true);
}

#[cfg(windows)]
fn say_on(text: &str, err: bool) {
    use std::io::Write;
    use windows::Win32::Storage::FileSystem::{GetFileType, FILE_TYPE_DISK, FILE_TYPE_PIPE};
    use windows::Win32::System::Console::{
        AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    };

    let which = if err { STD_ERROR_HANDLE } else { STD_OUTPUT_HANDLE };
    let redirected = unsafe { GetStdHandle(which) }
        .ok()
        .filter(|h| !h.is_invalid())
        .is_some_and(|h| {
            let kind = unsafe { GetFileType(h) };
            kind == FILE_TYPE_DISK || kind == FILE_TYPE_PIPE
        });
    if redirected {
        if err {
            let mut out = std::io::stderr().lock();
            let _ = writeln!(out, "{text}");
            let _ = out.flush();
        } else {
            let mut out = std::io::stdout().lock();
            let _ = writeln!(out, "{text}");
            let _ = out.flush();
        }
        return;
    }
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_ok() {
        if let Ok(mut out) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
            let _ = writeln!(out, "{text}");
            return;
        }
    }
    match err {
        true => eprintln!("{text}"),
        false => println!("{text}"),
    }
}

#[cfg(not(windows))]
fn say(text: &str) {
    println!("{text}");
}

#[cfg(not(windows))]
fn say_err(text: &str) {
    eprintln!("{text}");
}

/// PowerShell does not wait for a windowed program, so a script that calls
/// `filer.exe env --out` itself reads the file before it is there and sees
/// `$LASTEXITCODE` 0 (#188). `filer env --out` goes through `filer.com`,
/// which the shell waits for; this is for the other way.
#[cfg(windows)]
const ENV_OUT_WAIT: &str = "\n                     (filer.exe in a script: add | Out-Null to wait)";
#[cfg(not(windows))]
const ENV_OUT_WAIT: &str = "";

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
                    Ok(keys) => match keys.iter().find(|k| keyscript::press(k).is_none()) {
                        Some(k) => refuse_keys(&format!("{k:?} cannot be typed")),
                        None => cli.keys = keys,
                    },
                    Err(why) => refuse_keys(&why),
                }
            }
            "--help" | "-h" => {
                say(&format!(
                    "filer — a yazi-flavored file manager\n\n\
                     USAGE:\n    filer [PATH] [--cwd-file FILE] [--chooser-file FILE] [--keys KEYS]\n\n\
                     OPTIONS:\n    -h, --help       this text\n    \
                     -V, --version    the version and the architecture\n    \
                     --keys KEYS      press these keys once started, in keymap notation:\n                     \
                     \"<Tab>C\" opens spot and copies it; <Wait:500> pauses\n                     \
                     500 ms; <Now> presses the next key without waiting\n                     \
                     for the last to settle; <Shot:name> saves the window\n                     \
                     as name.png, <State:name> the state as name.txt;\n                     \
                     <Quit> ends filer whatever is open.\n                     \
                     For scripted checks\n\n\
                     COMMANDS:\n    env              config files, outside tools and environment,\n                     \
                     for pasting into a bug report\n    \
                     env --out FILE   the same, written to FILE as UTF-8{ENV_OUT_WAIT}\n    \
                     shell-hook [pwsh|bash|zsh]\n                     \
                     the lines that let <A-Up> in the terminal pane\n                     \
                     follow the shell; filer shell-hook | Add-Content $PROFILE\n\n\
                     Config is read from yazi's config directory, then from filer's own.\n\
                     Press ~ or F1 inside the app for the key list.",
                ));
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
            // `--out` writes the report itself, as UTF-8, because every way a
            // shell has of doing that went wrong somewhere: PowerShell's `>`
            // and a bare `$v = & filer env` get nothing from a windowed
            // program, and what does arrive is decoded with the console's code
            // page (#81, #84, #88, #93, #176, #183; Q55).
            "env" | "--env" => match env_out(args.next().as_deref(), args.next()) {
                Ok(None) => {
                    // `say` ends the line itself; the report's own newline
                    // would leave a blank one after it.
                    say(crate::envreport::text().trim_end_matches('\n'));
                    std::process::exit(0);
                }
                Ok(Some(path)) => match write_whole(&path, &crate::envreport::text()) {
                    Ok(()) => {
                        say_err(&format!("filer: wrote {}", std::path::absolute(&path).unwrap_or(path).display()));
                        std::process::exit(0);
                    }
                    Err(why) => {
                        say(&format!("filer: env --out {}: {why}", path.display()));
                        std::process::exit(1);
                    }
                },
                Err(why) => {
                    say(&format!("filer: env: {why}"));
                    std::process::exit(2);
                }
            },
            // The hook `<A-Up>` needs, printed to go straight into a profile
            // (Q50). With PowerShell that is `| Add-Content $PROFILE`: its
            // `>>`, like its `>`, gets nothing from a windowed program.
            "shell-hook" => match crate::shellhook::text(args.next().as_deref()) {
                Ok(hook) => {
                    say(hook.trim_end());
                    std::process::exit(0);
                }
                Err(why) => {
                    say(&format!("filer: shell-hook: {why}"));
                    std::process::exit(2);
                }
            },
            // The bug report's own line, so the two cannot drift apart (26.3).
            "--version" | "-V" => {
                say(&bugreport::version_line());
                std::process::exit(0);
            }
            other if !other.starts_with('-') => {
                if let Err(why) = take_path(&mut cli, other) {
                    say(&format!("filer: {why}"));
                    std::process::exit(2);
                }
            }
            _ => {}
        }
    }
    cli
}

/// What follows `env`: nothing, or `--out FILE`. Anything else is refused
/// rather than ignored, so a misspelt `--out` does not print to the screen
/// while its file never appears.
fn env_out(flag: Option<&str>, file: Option<String>) -> Result<Option<PathBuf>, String> {
    match (flag, file) {
        (None, _) => Ok(None),
        (Some("--out"), Some(f)) if !f.is_empty() => Ok(Some(PathBuf::from(f))),
        (Some("--out"), _) => Err("--out needs a file name".into()),
        (Some(other), _) => Err(format!("unknown argument {other:?} (try --out FILE)")),
    }
}

/// Writes beside the target and renames over it, so the file is never seen
/// half written. PowerShell does not wait for a windowed program, and the
/// next command reading the file can start before this one has finished.
fn write_whole(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    let mut part = path.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    std::fs::write(&part, text).and_then(|()| std::fs::rename(&part, path)).inspect_err(|_| {
        let _ = std::fs::remove_file(&part);
    })
}

/// The one path the command line may name. A second used to replace the
/// first in silence, and that is exactly what a path with a space in it looks
/// like when its quotes are forgotten: `filer C:\x\awkward names` opened
/// somewhere else with no word as to why (#126). The paths go in quotes as
/// typed: `{:?}` doubled every `\` of a Windows path (#194).
fn take_path(cli: &mut Cli, arg: &str) -> Result<(), String> {
    if let Some(first) = &cli.path {
        return Err(format!(
            "more than one path: \"{}\" and \"{arg}\" (a path with a space in it needs quotes)",
            first.display()
        ));
    }
    cli.path = Some(PathBuf::from(arg));
    Ok(())
}

fn main() -> eframe::Result<()> {
    // Before anything loads a DLL: `conpty.dll` only from beside filer.exe
    // (or System32), never the working folder or the `PATH` (#184).
    tsumugi_pane::restrict_dll_search();
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
    // Against the directory filer was started in, as `g<Space>` resolves what
    // is typed there: `filer .` and `filer ..\other` are how a shell names a
    // place, and a relative one left the tab with no parent column (#126).
    let start = cli.path.as_deref().map(|p| util::resolve_path(&home, p)).unwrap_or_else(|| home.clone());

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
    let mut cfg = cfg;
    let wgpu_options = wgpu_options(&mut cfg);
    let options = eframe::NativeOptions { viewport, wgpu_options, ..Default::default() };

    eframe::run_native(
        "Filer",
        options,
        Box::new(move |cc| {
            let mut cfg = cfg;
            // Filled as the run sets itself up, and written down at the end of
            // it: `filer env` cannot work either of these out for itself.
            let mut used = crate::runinfo::RunInfo {
                version: env!("CARGO_PKG_VERSION").into(),
                started: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs()),
                ..Default::default()
            };
            if let Some(rs) = cc.wgpu_render_state.as_ref() {
                let info = rs.adapter.get_info();
                used.adapter = info.name;
                used.backend = format!("{:?}", info.backend);
                used.device = format!("{:?}", info.device_type);
            }
            name_the_fallback(&mut cfg.warnings, &used.backend);
            let has_bold = apply_fonts(&cc.egui_ctx, &mut cfg, &mut used);
            crate::runinfo::save(&used);
            cc.egui_ctx.set_visuals(ui::visuals());
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
            // What earlier runs unpacked from archives and left (a copy `l`
            // opened is kept while its editor may hold it).
            util::sweep_archive_scratch();
            let mut a = App::new(cfg, start, cc.egui_ctx.clone());
            a.start_unproven(home);
            a.bold_font = has_bold;
            a.start_scaled(std::env::var("FILER_SCALE").ok().as_deref());
            a.cwd_file = cli.cwd_file;
            a.chooser_file = cli.chooser_file;
            let script_done =
                std::env::var_os("FILER_KEYS_DONE").filter(|_| !cli.keys.is_empty()).map(PathBuf::from);
            if let Some(done) = &script_done {
                keyscript::install_panic_report(done.clone());
            }
            let script: std::collections::VecDeque<_> = cli.keys.iter().filter_map(keyscript::press).collect();
            let script_watch = script_done.as_ref().map(|done| {
                let watch = Arc::new(Mutex::new(ScriptWatch {
                    left: script.len(),
                    at: std::time::Instant::now(),
                    due: Duration::ZERO,
                    finished: false,
                }));
                let labels = cli.keys.iter().filter(|s| keyscript::press(s).is_some()).map(keyscript::label).collect();
                watch_script(watch.clone(), labels, done.clone(), cc.egui_ctx.clone());
                watch
            });
            Ok(Box::new(Filer {
                app: a,
                title: String::new(),
                focused: true,
                last_input_frame: u64::MAX,
                last_geometry: None,
                script,
                script_at: (0, std::time::Instant::now()),
                script_now: false,
                script_shot: None,
                shot_dir: std::env::var_os("FILER_KEYS_DONE")
                    .and_then(|p| PathBuf::from(p).parent().map(std::path::Path::to_path_buf))
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or_else(|| PathBuf::from(".")),
                script_done,
                script_watch,
            }))
        }),
    )
}

/// A `--keys` script that cannot be pressed: said on the command line, and
/// written to `FILER_KEYS_DONE` for a run started detached, which has no
/// command line to read (#193).
fn refuse_keys(why: &str) -> ! {
    say(&format!("filer: --keys: {why}"));
    if let Some(done) = std::env::var_os("FILER_KEYS_DONE") {
        let _ = std::fs::write(done, keyscript::refused_report(why));
    }
    std::process::exit(2);
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
    candidates.extend(system_fonts());

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
    // A Japanese face for what the faces above cannot draw, behind them and
    // egui's own monospace: Noto Sans CJK in front made every ASCII name
    // proportional, and the columns stopped lining up.
    for path in fallback_fonts() {
        let name = font_stem(&path);
        if installed.contains(&name) || !load_face(&mut fonts, &path, &name) {
            continue;
        }
        for family in [egui::FontFamily::Monospace, egui::FontFamily::Proportional] {
            fonts.families.entry(family).or_default().push(name.clone());
        }
        loaded.push(path);
        break;
    }

    used.fonts = loaded.clone();

    // Bold: configured faces first, then the bold siblings of the regular
    // faces in use, then stock Windows faces.
    let mut bold_candidates: Vec<PathBuf> = cfg.ui.bold_fonts.iter().map(PathBuf::from).collect();
    for path in &loaded {
        bold_candidates.extend(bold_siblings(path));
    }
    bold_candidates.extend(system_bold_fonts());
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

/// The user's Nerd Fonts, where each platform keeps a user's fonts.
const NERD_FONTS: [&str; 6] = [
    "HackGen35ConsoleNF-Regular.ttf",
    "HackGenConsoleNF-Regular.ttf",
    "HackGen35Console-Regular.ttf",
    "FiraCodeNerdFont-Regular.ttf",
    "CaskaydiaCoveNerdFont-Regular.ttf",
    "JetBrainsMonoNerdFont-Regular.ttf",
];

/// Where to look for faces, in order: the user's Nerd Fonts, then Windows'
/// own. The faces found here go in front of egui's.
fn system_fonts() -> Vec<PathBuf> {
    let mut out = Vec::new();
    // `font_dir` is `~/.local/share/fonts` on Linux and `~/Library/Fonts` on
    // macOS; Windows has none, and keeps a user's fonts under LOCALAPPDATA.
    let user = dirs::font_dir()
        .or_else(|| dirs::data_local_dir().map(|d| d.join("Microsoft").join("Windows").join("Fonts")));
    if let Some(dir) = user {
        out.extend(NERD_FONTS.iter().map(|n| dir.join(n)));
    }
    if cfg!(windows) {
        for name in ["meiryo.ttc", "YuGothM.ttc", "YuGothR.ttc", "msgothic.ttc", "consola.ttf"] {
            out.push(PathBuf::from(r"C:\Windows\Fonts").join(name));
        }
    }
    out
}

/// A system face that covers Japanese on Linux and macOS, used only for what
/// the faces in front cannot draw. Only Windows' folders were searched before,
/// so on Linux a Japanese name was a row of boxes even with Noto CJK installed
/// (the Linux lane's first screenshot). Plain paths rather than fontconfig,
/// which would be a C dependency for one lookup.
fn fallback_fonts() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if cfg!(windows) {
        // Meiryo and friends are in `system_fonts`, in front, as they were.
    } else if cfg!(target_os = "macos") {
        for p in [
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        ] {
            out.push(PathBuf::from(p));
        }
    } else {
        // Debian and Ubuntu, Arch, Fedora; then IPA and Droid, which older
        // or smaller installs carry instead of Noto.
        for p in [
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf",
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        ] {
            out.push(PathBuf::from(p));
        }
    }
    out
}

/// The stock bold faces, after the bold siblings of the faces in use.
/// Windows' only: a CJK bold in front would draw ASCII bold proportional, the
/// same trap as the regular face; elsewhere bold is overstruck instead.
fn system_bold_fonts() -> Vec<PathBuf> {
    match cfg!(windows) {
        true => ["meiryob.ttc", "YuGothB.ttc", "consolab.ttf"]
            .iter()
            .map(|n| PathBuf::from(r"C:\Windows\Fonts").join(n))
            .collect(),
        false => Vec::new(),
    }
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
    match stem.strip_suffix("-Regular") {
        Some(base) => out.push(dir.join(format!("{base}-Bold.{ext}"))),
        // `DejaVuSansMono.ttf` beside `DejaVuSansMono-Bold.ttf`: many Linux
        // faces name the regular weight with no suffix at all (#131).
        None => out.push(dir.join(format!("{stem}-Bold.{ext}"))),
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
    last_geometry: Option<([f32; 2], f32, [usize; 2])>,
    /// `--keys`, as the events each press arrives as, still to be pressed.
    script: std::collections::VecDeque<keyscript::Press>,
    /// The frame and the moment the last scripted key went in.
    script_at: (u64, std::time::Instant),
    /// `<Now>` was read: the next key goes in a frame after the last, without
    /// the settled wait.
    script_now: bool,
    /// `<Shot:name>`: asked for in the frame loop (`Some(name, false)`), then
    /// sent and waited on (`true`) until the picture is on disk.
    script_shot: Option<(String, bool)>,
    /// Where `<Shot:name>` saves: beside `FILER_KEYS_DONE`, else the folder
    /// filer was started from.
    shot_dir: PathBuf,
    /// `FILER_KEYS_DONE`: a file to write once the last scripted key has been
    /// pressed and what it started has landed. A script that drives filer
    /// from outside (`scripts/xrun.sh`) waits for it instead of guessing how
    /// long the keys take -- a guess that read half-pressed results (#134).
    script_done: Option<PathBuf>,
    /// What the watchdog reads (`watch_script`). `None` without a script or
    /// without `FILER_KEYS_DONE`, where there is nobody to tell.
    script_watch: Option<Arc<Mutex<ScriptWatch>>>,
}

/// How far `--keys` has got, shared with the thread that watches it.
struct ScriptWatch {
    /// Steps not yet taken.
    left: usize,
    /// When the last one was.
    at: std::time::Instant,
    /// A `<Wait:N>` at the head of the script: how long the quiet is meant to be.
    due: Duration,
    /// The report is written; the thread can stop.
    finished: bool,
}

/// `--keys` runs in the frame loop, and a window that stops getting frames
/// stops pressing -- one ARM64 run sat 60 s with `u` never pressed and nothing
/// to show for it (#168, proposal 5). This thread nudges the loop once a
/// second, and if nothing is pressed for [`keyscript::STALL`] past any wait due,
/// writes `FILER_KEYS_DONE` with where the script stopped. A script that then
/// finishes after all overwrites it with the usual report.
fn watch_script(watch: Arc<Mutex<ScriptWatch>>, labels: Vec<String>, done: PathBuf, ctx: egui::Context) {
    std::thread::spawn(move || {
        let mut told = false;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let Ok(w) = watch.lock() else { return };
            if w.finished {
                return;
            }
            ctx.request_repaint();
            let quiet = w.at.elapsed();
            if !told && quiet > w.due + keyscript::STALL {
                told = true;
                let report = keyscript::stalled_report(&labels, w.left, quiet);
                let _ = std::fs::write(&done, &report);
                use std::io::Write;
                // Not `eprintln!`: started from `filer.com`, this is a pipe
                // nobody reads once the window is up, and that would panic.
                let _ = writeln!(std::io::stderr(), "filer --keys: {}", report.lines().collect::<Vec<_>>().join("; "));
            }
        }
    });
}

impl Filer {
    /// Tell the watchdog the script has moved.
    fn note_progress(&self) {
        let Some(watch) = &self.script_watch else { return };
        if let Ok(mut w) = watch.lock() {
            w.left = self.script.len();
            w.at = self.script_at.1;
            w.due = match self.script.front() {
                Some(keyscript::Press::Wait(d)) => *d,
                _ => Duration::ZERO,
            };
            w.finished = self.script_done.is_none();
        }
    }

    /// `<Shot:name>`: ask for the picture, then save it when it arrives.
    fn take_shot(&mut self, ctx: &egui::Context) {
        let Some((name, sent)) = self.script_shot.clone() else { return };
        if !sent {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(name.clone())));
            self.script_shot = Some((name, true));
            ctx.request_repaint();
            return;
        }
        let image = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { user_data, image, .. }
                    if user_data.data.as_ref().and_then(|d| d.downcast_ref::<String>()) == Some(&name) =>
                {
                    Some(image.clone())
                }
                _ => None,
            })
        });
        let Some(image) = image else {
            ctx.request_repaint();
            return;
        };
        let [w, h] = image.size;
        let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
        let path = self.shot_dir.join(format!("{name}.png"));
        let saved = image::RgbaImage::from_raw(w as u32, h as u32, rgba)
            .ok_or_else(|| "the picture had the wrong size".to_owned())
            .and_then(|img| img.save(&path).map_err(|e| e.to_string()));
        if let Err(e) = saved {
            self.app.error(format!("Shot {name}: {e}"));
        }
        self.script_shot = None;
    }

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
        // The pane's grid as well, which moves with the window and with `<C-S-Enter>`.
        let pane = self.app.term.as_ref().map_or([0, 0], |t| [t.size().lines, t.size().cols]);
        let key = (rounded.0, rounded.1, pane);
        if self.last_geometry == Some(key) {
            return;
        }
        self.last_geometry = Some(key);
        let mut used = crate::runinfo::load().unwrap_or_default();
        used.version = env!("CARGO_PKG_VERSION").into();
        used.window_pt = rounded.0;
        used.ppp = rounded.1;
        // A pane closed later in the run keeps the size it last had.
        if pane != [0, 0] {
            used.pane = pane;
        }
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
        if self.script.is_empty() && self.script_done.is_none() {
            return;
        }
        ctx.request_repaint();
        let frame = ctx.cumulative_frame_nr();
        let (last_frame, last_at) = self.script_at;
        let waited_long = last_at.elapsed() > Duration::from_secs(5);
        // A picture being taken holds the next key until it is saved, or for
        // five seconds if it never comes.
        if self.script_shot.is_some() {
            if !waited_long {
                return;
            }
            self.script_shot = None;
        }
        if self.script.front() == Some(&keyscript::Press::Now) {
            self.script.pop_front();
            self.script_now = true;
        }
        // `<Now>`: one frame after the last key is enough -- it has been
        // handed to the app, which is all the next one needs.
        let ready = if self.script_now {
            frame > last_frame
        } else {
            frame >= last_frame + 2 && (self.app.settled() || waited_long)
        };
        if !ready {
            return;
        }
        // The same wait as before a key: the last one has been drawn and has
        // settled, so what is on screen now is its result.
        if self.script.is_empty() {
            if let Some(done) = self.script_done.take() {
                // `keys: done` against the watchdog's `keys: stalled`.
                let _ = std::fs::write(done, state_report(&self.app) + "keys: done\n");
                self.note_progress();
            }
            return;
        }
        match self.script.pop_front() {
            // Counted from the key before it: the next key goes in at least
            // this long after that one, and still only once things settle.
            Some(keyscript::Press::Wait(d)) => {
                let left = d.saturating_sub(last_at.elapsed());
                if left.is_zero() {
                    self.script_at = (frame, std::time::Instant::now());
                } else {
                    self.script.push_front(keyscript::Press::Wait(d));
                    ctx.request_repaint_after(left);
                }
            }
            Some(keyscript::Press::Events(events)) => {
                raw_input.events.extend(events);
                self.script_at = (frame, std::time::Instant::now());
                self.script_now = false;
            }
            // Read as it goes in, not when the script was parsed: an earlier
            // key of the script may have put something on the clipboard.
            Some(keyscript::Press::Paste) => {
                raw_input.events.push(keyscript::paste_event());
                self.script_at = (frame, std::time::Instant::now());
                self.script_now = false;
            }
            Some(p @ (keyscript::Press::Click { .. } | keyscript::Press::Wheel { .. })) => {
                let rect = ctx.input(|i| i.viewport_rect());
                raw_input.events.extend(keyscript::pointer_events(&p, rect));
                self.script_at = (frame, std::time::Instant::now());
                self.script_now = false;
            }
            // Written now: the wait above is the one a key gets, so this is
            // the state the key before it left (#230).
            Some(keyscript::Press::State(name)) => {
                let path = self.shot_dir.join(format!("{name}.txt"));
                if let Err(e) = std::fs::write(&path, state_report(&self.app)) {
                    self.app.error(format!("State {name}: {e}"));
                }
                self.script_at = (frame, std::time::Instant::now());
            }
            // The pane's own text, beside the state: `name.pane.txt`, so a
            // `<State:name>` of the same name does not take its place (#229).
            Some(keyscript::Press::PaneText(name)) => {
                let path = self.shot_dir.join(format!("{name}.pane.txt"));
                let text = self.app.term.as_ref().map_or_else(|| "(no pane)\n".to_owned(), |t| t.screen_text());
                if let Err(e) = std::fs::write(&path, text) {
                    self.app.error(format!("PaneText {name}: {e}"));
                }
                self.script_at = (frame, std::time::Instant::now());
            }
            Some(keyscript::Press::PreviewText(name)) => {
                let path = self.shot_dir.join(format!("{name}.preview.txt"));
                if let Err(e) = std::fs::write(&path, preview_text(&self.app.preview.state)) {
                    self.app.error(format!("PreviewText {name}: {e}"));
                }
                self.script_at = (frame, std::time::Instant::now());
            }
            // Straight to the quit `ui` already handles, past anything that
            // would ask first or take `q` for itself (#236).
            Some(keyscript::Press::Quit) => {
                self.app.quit = true;
                self.script_at = (frame, std::time::Instant::now());
            }
            // Taken in `ui`, which is where a viewport command can be sent.
            Some(keyscript::Press::Shot(name)) => {
                self.script_shot = Some((name, false));
                self.script_at = (frame, std::time::Instant::now());
            }
            // Taken before the wait above; `parse` puts a key after every one.
            Some(keyscript::Press::Now) | None => {}
        }
        self.note_progress();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        self.app.drain_channels(&ctx);
        self.record_geometry(&ctx);
        self.take_shot(&ctx);
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
            // The frame that writes `keys: done` never comes for a closed
            // window, so a script ending in `q` is reported here instead.
            if let Some(done) = self.script_done.take() {
                let _ = std::fs::write(done, state_report(&self.app) + &keyscript::quit_report(self.script.len()));
                self.note_progress();
            }
            self.app.on_quit();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Keep the frame loop alive only while something is actually pending.
        if let Some(due) = self.app.rescan_due() {
            ctx.request_repaint_after(due);
        }
        if self.app.preview.pending_since.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        if !self.app.toasts.is_empty()
            || self.app.search.is_some()
            || self.app.term_waiting()
            || self.app.tasks.iter().any(|t| t.state == app::TaskState::Running)
        {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
    }
}

fn title_for(app: &App) -> String {
    let fmt = &app.cfg.yazi.mgr.title_format;
    let cwd = app.tab().cwd.display().to_string();
    if !fmt.contains("{cwd}") {
        return format!("Filer: {cwd}");
    }
    // `{rows}` and `{pane}` are filer's own: how many list rows are on screen,
    // and the terminal pane's grid (`12x159`, empty when closed). A test run
    // measured them with five key presses (TESTING.md 1.30); a title is read
    // with one call from outside.
    let pane = app.term.as_ref().map_or(String::new(), |t| format!("{}x{}", t.size().lines, t.size().cols));
    fmt.replace("{cwd}", &cwd).replace("{rows}", &app.tab().page_rows.to_string()).replace("{pane}", &pane)
}

/// `compare row 2: ~ the ⟦price⟧ is firm | the ⟦cost⟧ is firm`: the marker is
/// `=` for the same line, `-` and `+` for one that has no partner, `~` for an
/// edited pair, and the words that changed are in ⟦ ⟧.
fn compare_row_line(n: usize, row: &diff::Row) -> String {
    let side = |l: &Option<diff::Line>| match l {
        None => String::new(),
        Some(l) => {
            let mut out = String::new();
            for (i, c) in l.text.chars().enumerate() {
                if l.changed.iter().any(|r| r.start == i) {
                    out.push('⟦');
                }
                out.push(c);
                if l.changed.iter().any(|r| r.end == i + 1) {
                    out.push('⟧');
                }
            }
            out
        }
    };
    let marker = match (&row.left, &row.right, row.same) {
        (_, _, true) => '=',
        (Some(_), None, _) => '-',
        (None, Some(_), _) => '+',
        _ => '~',
    };
    format!("compare row {n}: {marker} {} | {}", side(&row.left), side(&row.right))
}

/// What the preview pane holds, as text: a text file's lines whole (not only
/// the rows on screen), a rendered Markdown document's lines, a hex dump's
/// rows, a card's `label: value` rows, the error or the caption. For
/// `<PreviewText:name>` -- the body of a preview was read from eleven
/// pictures, as words (#165).
fn preview_text(state: &app::PreviewState) -> String {
    let joined = |spans: &[preview::Span]| spans.iter().map(|s| s.text.as_str()).collect::<String>();
    let mut out = match state {
        app::PreviewState::Ready(preview::Payload::Text { lines, .. }) => {
            lines.iter().map(|l| joined(l)).collect::<Vec<_>>().join("\n")
        }
        app::PreviewState::Ready(preview::Payload::Markdown { doc, .. }) => {
            doc.lines.iter().map(|l| joined(&l.spans)).collect::<Vec<_>>().join("\n")
        }
        app::PreviewState::Ready(preview::Payload::Binary { lines, .. }) => lines.join("\n"),
        app::PreviewState::Ready(preview::Payload::Meta { rows }) => {
            rows.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("\n")
        }
        app::PreviewState::Ready(preview::Payload::Image { caption, .. }) => caption.clone(),
        app::PreviewState::Ready(preview::Payload::Error(e)) => e.clone(),
        _ => "(no preview)".to_owned(),
    };
    out.push('\n');
    out
}

/// What `FILER_KEYS_DONE` holds once `--keys` is done: the state a check reads
/// afterwards, one `name: value` per line. Reading it any other way meant
/// pressing a key, and a key changes what it reads (#103, proposal 3).
fn state_report(app: &App) -> String {
    let tab = app.tab();
    let overlay = match &app.overlay {
        app::Overlay::None => "none",
        app::Overlay::Input(_) => "input",
        app::Overlay::Confirm(_) => "confirm",
        app::Overlay::Pick(_) => "pick",
        app::Overlay::Help => "help",
        app::Overlay::Tasks(_) => "tasks",
        app::Overlay::Spot(_) => "spot",
        app::Overlay::Diff(_) => "diff",
    };
    let mut lines = vec![
        format!("cwd: {}", tab.cwd.display()),
        format!("hovered: {}", tab.current.hovered().map_or(String::new(), |e| e.path.display().to_string())),
        format!("selected: {}", tab.selected.len()),
        // The register, as the header says it (#237, #238): 10.x read it off
        // the screen before.
        match app.yank.paths.len() {
            0 => "yank: empty".to_owned(),
            n => format!("yank: {n} {}", if app.yank.cut { "cut" } else { "copied" }),
        },
        format!("tab: {} of {}", app.active + 1, app.tabs.len()),
        format!("overlay: {overlay}"),
        // What stands in for the listing: the cwd alone cannot tell a usage
        // view from the plain folder it was opened on.
        format!(
            "view: {}",
            if app.in_usage_view() {
                "usage"
            } else if app.in_archive_view() {
                "archive"
            } else if app.in_search_view() {
                "search"
            } else {
                "list"
            }
        ),
    ];
    // Which archive and which level of it (#251, #252): `hovered:` said it
    // only while a row was under the cursor, and an empty folder has none.
    if let Some(at) = app.archive_at() {
        lines.push(format!("archive: {}", at.display()));
    }
    if let app::Overlay::Input(ov) = &app.overlay {
        lines.push(format!("input: {}", ov.text));
    }
    // What a picker is offering, in the order on screen, and the row under
    // its cursor (#196): `overlay: pick` alone said a list was open, not what
    // was in it, and the README's opener order could only be read off a
    // picture. A long one -- the palette lists every binding -- is cut.
    if let app::Overlay::Pick(ov) = &app.overlay {
        const MAX: usize = 40;
        // With the note a row carries on its right, in brackets: the jump
        // list's `2h ago` was only in the picture (#208).
        // An opener list's notes are the commands, which are not descriptions:
        // they go on their own `pick runs:` line.
        let runs = matches!(ov.action, app::PickAction::OpenWith { .. });
        let shown: Vec<String> = ov
            .matches
            .iter()
            .map(|m| match ov.details.get(m.0).filter(|d| !d.is_empty() && !runs) {
                Some(d) => format!("{} ({d})", ov.items[m.0]),
                None => ov.items[m.0].clone(),
            })
            .collect();
        let mut line = shown.iter().take(MAX).cloned().collect::<Vec<_>>().join(" | ");
        if shown.len() > MAX {
            line.push_str(&format!(" | … +{} more", shown.len() - MAX));
        }
        lines.push(format!("pick: {line}"));
        if runs {
            let cmds: Vec<&str> = ov.matches.iter().take(MAX).map(|m| ov.details[m.0].as_str()).collect();
            lines.push(format!("pick runs: {}", cmds.join(" | ")));
        }
        lines.push(format!("query: {}", ov.query));
        lines.push(format!("picked: {}", ov.selected().map_or("", |i| ov.items[i].as_str())));
    }
    // What a confirm box asks and what it offers, shaped like `pick:`: 26.1
    // was ten lines read off a picture and compared with `filer env` by hand,
    // and every trash, junction, overwrite and link question was a picture
    // too (#230). Blank body lines are spacing and are left out.
    if let app::Overlay::Confirm(c) = &app.overlay {
        let body = c.body.iter().filter(|l| !l.trim().is_empty()).map(|l| l.as_str());
        lines.push(format!("confirm: {}", std::iter::once(c.title.as_str()).chain(body).collect::<Vec<_>>().join(" | ")));
        let keys: Vec<String> = (0..c.options.len()).map(|i| c.button_label(i)).collect();
        lines.push(format!("confirm keys: {}", keys.join(" | ")));
    }
    // The outline entry under the cursor while the outline has the keys, and
    // the source line `<Enter>` hands an editor: section 22 opens "the Nth
    // entry", which could only be counted in `j` presses or read off a
    // picture (#155, #198, #235).
    if let Some(k) = app.preview.outline {
        let entries = app.outline_entries();
        if let Some(e) = entries.get(k) {
            let line = app.outline_source_line(k).map_or(String::new(), |l| format!(" (line {l})"));
            lines.push(format!("outline: {}/{} {}{line}", k + 1, entries.len(), e.label.trim()));
        }
    }
    // The spot panel's row under the cursor, and where that is among all of
    // them: `spot row: Pull request`, `spot: 16 of 23` (#212, #224).
    if let app::Overlay::Spot(ov) = &app.overlay {
        let rows: Vec<(String, String)> = app.spot_sections().into_iter().flat_map(|s| s.rows).collect();
        if let Some((label, _)) = rows.get(ov.cursor) {
            lines.push(format!("spot row: {label}"));
        }
        lines.push(format!("spot: {} of {}", (ov.cursor + 1).min(rows.len()), rows.len()));
        lines.push(format!("spot top: {} of {}", ov.scroll, rows.len()));
    }
    // Two folders or two files, and the pair: `overlay: diff` is both.
    if let app::Overlay::Diff(ov) = &app.overlay {
        let what = if matches!(ov.outcome, Some(diff::Outcome::Tree { .. })) { "folders" } else { "files" };
        lines.push(format!("compare: {what} {} | {}", ov.left.display(), ov.right.display()));
        // The rows of a file comparison, as the view pairs them, with the
        // changed words in ⟦ ⟧: 5.11 read these off a picture with a hundred
        // lines of pixel script (#236). The first 60; `compare rows` is all.
        if let Some(diff::Outcome::Rows { rows, .. }) = &ov.outcome {
            const MAX: usize = 60;
            lines.push(format!("compare rows: {}", rows.len()));
            for (i, row) in rows.iter().take(MAX).enumerate() {
                lines.push(compare_row_line(i + 1, row));
            }
        }
    }
    lines.push(format!(
        "pane: {}",
        app.term.as_ref().map_or("closed".into(), |t| format!("{}x{}", t.size().lines, t.size().cols))
    ));
    // The report link `<F12>` opened or copied: the one key whose effect
    // leaves the program could not be read back (#221, #222).
    if let Some(u) = &app.last_report {
        lines.push(format!("report: {u}"));
    }
    // The parent column as drawn, each name cut to its width: 24.7's rows,
    // that no two neighbours read the same, could only be read off a picture
    // (#251, proposal 4). A long folder is cut, as `pick:` is.
    if !app.parent_shown.is_empty() {
        const MAX: usize = 40;
        let mut line = app.parent_shown.iter().take(MAX).cloned().collect::<Vec<_>>().join(" | ");
        if app.parent_shown.len() > MAX {
            line.push_str(&format!(" | … +{} more", app.parent_shown.len() - MAX));
        }
        lines.push(format!("parent: {line}"));
    }
    // What the last opener or shell command started, by PID: a check could
    // only `Stop-Process` by name, which closed the owner's own documents
    // too (#162, proposal 3). The PID is the shell's; the program is its child.
    if let Some(l) = &app.last_launch {
        lines.push(format!("launched: {l}"));
    }
    // Lines scrolled back into the pane's history, of how many it holds:
    // half of 19.4 could only be read off pictures (#209).
    if let Some(t) = &app.term {
        let (back, of) = t.scrollback();
        lines.push(format!("pane back: {back} of {of}"));
    }
    // Where the list and the preview are scrolled, and how the preview is
    // shown: what the wheel, zoom and minimap rows of TESTING.md move, read
    // as numbers instead of judged from a picture (2026-10-03).
    // Where a key goes now (#220, #249): a run had to type a file in the
    // pane to prove the keys had reached it.
    let focus = if !matches!(app.overlay, app::Overlay::None) {
        "overlay"
    } else if app.term_focus && app.term.is_some() {
        "pane"
    } else if app.preview.outline.is_some() {
        "outline"
    } else {
        "list"
    };
    lines.push(format!("focus: {focus}"));
    if let Some(t) = &app.term {
        let (col, row) = t.with_grid(terminal::cursor_cell);
        lines.push(format!("pane cursor: {col},{row}"));
    }
    // The panes that are not overlays (#165): `T` and `<F3>`'s quick look.
    lines.push(format!("max preview: {}", if app.max_preview { "on" } else { "off" }));
    lines.push(format!("quick: {}", if app.quick { "on" } else { "off" }));
    // What the settings came from, so a `<C-F5>` reload reads as a value
    // (#165): every file read, or `none`.
    let read: Vec<String> = app.cfg.loaded.iter().map(|p| p.display().to_string()).collect();
    lines.push(format!("config: {}", if read.is_empty() { "none".into() } else { read.join(" | ") }));
    // How many rows the list holds, and the filter on it: "a row more or
    // fewer" and "the filter stayed" were read off a picture (#231). `listing…`
    // while the folder is still being read, so an empty folder and one not yet
    // read tell apart.
    lines.push(format!(
        "items: {}",
        match &tab.current.state {
            core::folder::LoadState::Loading => "listing…".to_owned(),
            _ => tab.current.view.len().to_string(),
        }
    ));
    if let Some(f) = &tab.current.filter {
        lines.push(format!("filter: {}", f.query));
    }
    // What the preview says about itself, as the pane writes it under the
    // picture, or why there is none (#223).
    match &app.preview.state {
        app::PreviewState::Ready(preview::Payload::Image { caption, .. }) if !caption.is_empty() => {
            lines.push(format!("preview caption: {caption}"));
        }
        app::PreviewState::Ready(preview::Payload::Error(e)) => lines.push(format!("preview error: {}", e.replace('\n', " / "))),
        _ => {}
    }
    // The adapter this run drew with, as the record `filer env` reads has it
    // (#232): a run measuring the backend could only read it from there.
    if let Some(info) = crate::runinfo::load().filter(|i| !i.adapter.is_empty() && i.version == env!("CARGO_PKG_VERSION")) {
        lines.push(format!("adapter: {} ({}, {})", info.adapter, info.backend, info.device));
    }
    // The file jobs not yet over, one line each, and how many: a run had to
    // infer a copy was done from the wording of a toast (#241, #257).
    let active: Vec<&app::Task> = app
        .tasks
        .iter()
        .filter(|t| t.state.is_live())
        .collect();
    // What filer last put on the clipboard (`c` and friends), one line, cut
    // at 200 characters: read from outside it needed a second tool (#197).
    // Not what is on the clipboard, which may be someone else's.
    if let Some(text) = exec::last_set_clipboard() {
        let one: String = text.replace(['\r', '\n'], " / ").chars().take(200).collect();
        lines.push(format!("clipboard set: {one}"));
    }
    // How many frames were drawn: 47's question is whether an idle window
    // keeps drawing, which `(Get-Process).CPU` answered with a second tool
    // (#201).
    lines.push(format!("frames: {}", app.ctx.cumulative_frame_nr()));
    lines.push(format!("jobs: {} running", active.len()));
    // Every job of the run, the finished and stopped ones too, newest last:
    // `jobs: 0` could not tell one that ended from one that was stopped
    // (#268). The last ten.
    for t in app.tasks.iter().rev().take(10).collect::<Vec<_>>().into_iter().rev() {
        lines.push(format!("task: {} {} {}/{} files", t.label, t.state.label(), t.files_done, t.files));
    }
    for t in active {
        lines.push(format!("job: {} [{}] {}/{} files", t.label, t.state.label(), t.files_done, t.files));
    }
    // The cursor's place in the list, `4/500` (#233): counted in presses
    // before. Nothing while the folder is empty or still being read.
    if !tab.current.view.is_empty() {
        lines.push(format!("position: {}/{}", tab.current.cursor + 1, tab.current.view.len()));
    }
    lines.push(format!("list top: {}", tab.current.offset));
    // The pane the keys are not in, when split: 19.7's three cases each took
    // a start to see where the other one was scrolled (#216, #209).
    if let Some(other) = app.other_pane().filter(|&i| i < app.tabs.len()) {
        lines.push(format!("other list top: {}", app.tabs[other].current.offset));
    }
    lines.push(format!("preview top: {} of {}", tab.preview_offset, app.preview.max_offset));
    // How long the text on show is (`5237+` when the read was cut): the top
    // above is the last place to scroll to, which is not the line count
    // (#262).
    if let app::PreviewState::Ready(preview::Payload::Text { extent, .. } | preview::Payload::Markdown { extent, .. }) = &app.preview.state {
        lines.push(format!("preview lines: {}", extent.lines()));
    }
    // Which picture an external preview is on -- the `{n}` its command was
    // given, a page or a second (#166) -- while the file under the cursor
    // has one.
    if tab.current.hovered().is_some_and(|e| !e.is_dir_like() && app.is_external_preview(&e.path)) {
        lines.push(format!("picture: {}", app.preview.n));
        // And the one on screen, which is not that while a request for
        // another is out (#255): the key of what the pane last drew.
        if let Some(k) = app.preview.key.as_ref().filter(|k| k.path == tab.current.hovered().map_or_else(PathBuf::new, |e| e.path.clone())) {
            lines.push(format!("picture shown: {}", k.n));
        }
    }
    // Against the file's own size, as the caption says it (an SVG is laid
    // out larger than it is).
    let ppp = app.ctx.pixels_per_point();
    let scale = |z: f32| match &app.preview.state {
        app::PreviewState::Ready(preview::Payload::Image { own, vector, .. }) => preview::shown_scale(z, *own, *vector, ppp),
        _ => z,
    };
    // Fit with the scale it comes to, as the caption says it: `fit` alone is
    // a different size on every window, so two machines' runs could not be
    // compared (#211, proposal 3). Only for a picture; nothing else is scaled.
    let image = matches!(&app.preview.state, app::PreviewState::Ready(preview::Payload::Image { .. }));
    let fit = match image {
        true => format!("fit ({:.0}%)", scale(app.preview.fit) * 100.0),
        false => "fit".into(),
    };
    lines.push(format!("zoom: {}", app.preview.zoom.map_or(fit, |z| format!("{:.0}%", scale(z) * 100.0))));
    // The scale the run was measured at: filer's own (`<C-=>`) and what egui
    // made of it with the display's, which is what every size above was
    // drawn at. Before, it was read back from the `Scale N%` toasts or the
    // next run's `filer env` (#227, #228).
    lines.push(format!("scale: {:.0}% (ppp {})", app.scale * 100.0, (ppp * 1000.0).round() / 1000.0));
    let size = app.ctx.input(|i| i.viewport_rect().size());
    let window = crate::runinfo::RunInfo { window_pt: [size.x.round(), size.y.round()], ppp, ..Default::default() };
    lines.push(format!("window: {}", window.window_line().unwrap_or_default()));
    // The setting `<A-n>` flips. Whether a strip was actually drawn (it is
    // not on rendered Markdown, a two-line file or a narrow pane) is a
    // picture's question.
    lines.push(format!("minimap setting: {}", if app.cfg.ui.minimap { "on" } else { "off" }));
    // What the last frame did about it: drawn, with the lines its frame
    // covers, or why not. 2, 16.12 and 18 judged it from a picture (#261).
    if let Some(note) = app.ctx.data(|d| d.get_temp::<String>(egui::Id::new(ui::preview::MINIMAP_NOTE))) {
        lines.push(format!("minimap: {note}"));
    }
    lines.push(format!(
        "split: {}",
        app.split.map_or("no".into(), |s| format!("yes, keys {}", if s.right { "right" } else { "left" }))
    ));
    lines.push(format!("toast: {}", app.toasts.last().map_or("", |t| t.text.as_str())));
    // How many times that same line was raised, when more than once: the
    // `×8` the toast shows was only in the picture (#216).
    if let Some(n) = app.toasts.last().map(|t| t.count).filter(|n| *n > 1) {
        lines.push(format!("toast repeats: {n}"));
    }
    // Every toast of the run, the faded ones too, oldest first; a toast's own
    // line breaks become ` / ` so the report stays one line per name.
    let all: Vec<String> = app.toast_log.iter().map(|t| t.lines().collect::<Vec<_>>().join(" / ")).collect();
    lines.push(format!("toasts: {}", all.join(" | ")));
    lines.join("\n") + "\n"
}

/// The wgpu setup, with `[ui] backend` applied (Q70).
///
/// `WGPU_BACKEND` wins, as `FILER_TERM_SHELL` wins over `[term] shell`: it is
/// the one-run override. A backend this machine has no adapter for is not
/// handed on, because eframe would then fail to open any window at all and
/// say so only on a console nobody sees; it falls back to wgpu's own pick
/// and says why among the config warnings.
fn wgpu_options(cfg: &mut Config) -> eframe::WgpuConfiguration {
    use eframe::egui_wgpu::WgpuSetup;
    let mut options = eframe::WgpuConfiguration::default();
    if std::env::var_os("WGPU_BACKEND").is_some_and(|v| !v.is_empty()) {
        return options;
    }
    let (backends, warning) = pick_backends(cfg.ui.backend_name(), cfg!(windows), has_adapter);
    if let Some(w) = warning {
        cfg.warnings.push(w);
    }
    if let (Some(backends), WgpuSetup::CreateNew(setup)) = (backends, &mut options.wgpu_setup) {
        setup.instance_descriptor.backends = backends;
    }
    options
}

/// The backends to narrow wgpu to, and a warning to add, for `[ui] backend`.
///
/// `auto` is GL first on Windows, where it is all that stops a driver thread
/// spinning a core under Vulkan and DX12 on AMD (#232, #240); GL drew both
/// test machines, the ARM64 one through a translation layer (#239). A machine
/// without it gets wgpu's own pick, silently -- nobody asked for GL there (the
/// owner's word, 2026-10-04, over Q70). A name filer rejects, or one this
/// machine has no adapter for, falls back to that same `auto`: it used to fall
/// to wgpu's pick, so a typo in the setting put the spinning core back (#243,
/// #244). The rejected name was warned about when the file was read.
fn pick_backends(
    name: Result<Option<&str>, String>,
    windows: bool,
    has: impl Fn(eframe::wgpu::Backends) -> bool,
) -> (Option<eframe::wgpu::Backends>, Option<String>) {
    let name = name.ok().flatten();
    match tsumugi_pane::gpu::pick_backends(name, windows, has) {
        Ok(backends) => (backends, None),
        Err(auto) => (auto, Some(format!(
            "[ui] backend = \"{}\": this machine has no adapter for it; drawing with the default",
            name.unwrap_or_default()
        ))),
    }
}

/// Once the window is up, a `[ui] backend` warning can say what it fell back
/// to instead of "the default" (#244): `drawing with Gl instead`. `filer env`,
/// which opens no window, keeps the general words.
pub(crate) fn name_the_fallback(warnings: &mut [String], backend: &str) {
    if backend.is_empty() {
        return;
    }
    for w in warnings.iter_mut().filter(|w| w.contains("[ui] backend")) {
        if let Some(head) = w.strip_suffix("drawing with the default") {
            *w = format!("{head}drawing with {backend} instead");
        }
    }
}

/// What a `[ui] backend` warning names as the fallback when no window is up
/// (`filer env`): the backend `auto` would pick, which is only known on
/// Windows (GL when the machine has it). Empty elsewhere, where wgpu chooses.
pub(crate) fn predicted_fallback() -> String {
    auto_backends(cfg!(windows), || has_adapter(eframe::wgpu::Backends::GL)).map_or(String::new(), |_| "Gl".into())
}

use tsumugi_pane::gpu::{auto_backends, has_adapter};

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
                // `<C-n>` / `<C-p>` as in fzf, `<C-j>` / `<C-k>` as in vim; plain
                // `j` / `k` stay filter letters (Q80). Listed in `[pick]`.
                K::N | K::J if modifiers.ctrl => Some(1),
                K::P | K::K if modifiers.ctrl => Some(-1),
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

    /// `{rows}` and `{pane}` in `title_format`: what is on screen, readable
    /// from the window title, with no pane open an empty `{pane}`.
    #[test]
    fn the_title_can_say_how_much_is_on_screen() {
        let dir = crate::util::test_dir("title-rows");
        let mut app = App::new(crate::config::Config::load(), dir.clone(), egui::Context::default());
        app.cfg.yazi.mgr.title_format = "{cwd} [{rows}] <{pane}>".into();
        app.tabs[app.active].page_rows = 31;
        assert_eq!(title_for(&app), format!("{} [31] <>", app.tab().cwd.display()));
    }

    /// Q55: `env --out FILE` writes the report itself; a misspelt or
    /// half-given `--out` is refused rather than printing to the screen.
    #[test]
    fn env_takes_out_and_nothing_else() {
        assert_eq!(env_out(None, None), Ok(None));
        assert_eq!(env_out(Some("--out"), Some("r.txt".into())), Ok(Some(PathBuf::from("r.txt"))));
        assert!(env_out(Some("--out"), None).unwrap_err().contains("file name"));
        assert!(env_out(Some("--out"), Some(String::new())).is_err());
        assert!(env_out(Some("--outt"), Some("r.txt".into())).unwrap_err().contains("--outt"));
    }

    /// The report lands whole, as UTF-8, over whatever was there, and leaves
    /// no `.part` behind.
    #[test]
    fn env_out_replaces_the_file_whole() {
        let dir = crate::util::test_dir("env-out");
        let path = dir.join("レポート.txt");
        std::fs::write(&path, "old and longer than the new text").unwrap();
        write_whole(&path, "Filer\n  名前: ü\n").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), "Filer\n  名前: ü\n".as_bytes());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        assert!(write_whole(&dir.join("no-such-dir").join("r.txt"), "x").is_err());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    }

    /// #241, #257: a live file job is a line of the state, a finished one is
    /// not counted.
    #[test]
    fn a_live_job_is_a_line_of_the_state() {
        let dir = crate::util::test_dir("state-jobs");
        let mut app = App::new(crate::config::Config::load(), dir, egui::Context::default());
        let task = |state, files_done| app::Task::for_test(fs::ops::OpKind::Copy, "Copy 1 item(s) into x", state, 40, files_done);
        app.tasks.push(task(app::TaskState::Running, 12));
        let report = state_report(&app);
        assert!(report.lines().any(|l| l == "jobs: 1 running"), "{report}");
        assert!(report.lines().any(|l| l == "job: Copy 1 item(s) into x [running] 12/40 files"), "{report}");
        app.tasks[0].state = app::TaskState::Done;
        let report = state_report(&app);
        assert!(report.lines().any(|l| l == "jobs: 0 running") && !report.contains("job:"), "{report}");
        assert!(report.lines().any(|l| l == "task: Copy 1 item(s) into x done 12/40 files"), "a finished job stays in the list: {report}");
    }

    /// #212, #224: the spot panel's row under the cursor and its place.
    #[test]
    fn spot_reports_its_cursor_row() {
        let dir = crate::util::test_dir("state-spot");
        std::fs::write(dir.join("a.txt"), b"hi").unwrap();
        let mut app = App::new(crate::config::Config::load(), dir.clone(), egui::Context::default());
        let entries = vec![fs::Entry::from_path(dir.join("a.txt")).unwrap()];
        let active = app.active;
        app.tabs[active].current = core::folder::Folder::from_entries(dir, std::sync::Arc::new(entries), true);
        assert!(!state_report(&app).contains("spot row:"), "only while spot is open");
        app.overlay = app::Overlay::Spot(app::SpotOverlay { cursor: 1, scroll: 0 });
        let report = state_report(&app);
        let total = app.spot_sections().iter().map(|s| s.rows.len()).sum::<usize>();
        assert!(report.lines().any(|l| l == format!("spot: 2 of {total}")), "{report}");
        assert!(report.lines().any(|l| l.starts_with("spot row: ")) && report.lines().any(|l| l == format!("spot top: 0 of {total}")), "{report}");
    }

    /// #197, #201: what filer put on the clipboard, and the frames drawn.
    #[test]
    fn the_state_names_the_last_copy_and_the_frames() {
        let dir = crate::util::test_dir("state-clip");
        let app = App::new(crate::config::Config::load(), dir, egui::Context::default());
        assert!(state_report(&app).lines().any(|l| l.starts_with("frames: ")));
        exec::set_clipboard("one\r\ntwo").unwrap();
        assert!(state_report(&app).lines().any(|l| l == "clipboard set: one /  / two"), "{}", state_report(&app));
    }

    /// #216: with a split, the other pane's scroll is a line of the state too.
    #[test]
    fn a_split_reports_the_other_panes_scroll() {
        let dir = crate::util::test_dir("state-split");
        let mut app = App::new(crate::config::Config::load(), dir.clone(), egui::Context::default());
        assert!(!state_report(&app).contains("other list top"), "one pane has no other");
        let first = &app.tabs[0];
        let second = crate::core::tab::Tab::new(dir.clone(), first.sort, first.show_hidden, first.linemode);
        app.tabs.push(second);
        app.tabs[1].current.offset = 7;
        app.split = Some(app::Split { other: 1, right: false });
        assert!(state_report(&app).lines().any(|l| l == "other list top: 7"));
    }

    /// #216: the `×N` of a repeated toast is a line of the state.
    #[test]
    fn a_repeated_toast_says_how_often() {
        let dir = crate::util::test_dir("state-repeats");
        let mut app = App::new(crate::config::Config::load(), dir, egui::Context::default());
        app.toasts.clear();
        app.toast("rep");
        assert!(!state_report(&app).contains("toast repeats"), "once is not a repeat");
        app.toast("rep");
        assert!(state_report(&app).lines().any(|l| l == "toast repeats: 2"));
    }

    /// #165: the preview's body as text, in each shape it takes.
    #[test]
    fn the_preview_body_reads_as_text() {
        let span = |t: &str| preview::Span { text: t.into(), ..Default::default() };
        let text = app::PreviewState::Ready(preview::Payload::Text {
            lines: vec![vec![span("fn "), span("main")], vec![span("}")]],
            map: Vec::new(),
            extent: Default::default(),
            outline: Vec::new(),
        });
        assert_eq!(preview_text(&text), "fn main\n}\n");
        let meta = app::PreviewState::Ready(preview::Payload::Meta { rows: vec![("Name".into(), "a.bin".into())] });
        assert_eq!(preview_text(&meta), "Name: a.bin\n");
        let err = app::PreviewState::Ready(preview::Payload::Error("not found, os error 2".into()));
        assert_eq!(preview_text(&err), "not found, os error 2\n");
        assert_eq!(preview_text(&app::PreviewState::Loading), "(no preview)\n");
    }

    /// #251, proposal 4: the parent column's rows as drawn, so 24.7's cut
    /// names read as text; gone once the column is not drawn.
    #[test]
    fn the_state_file_lists_the_parent_column_as_drawn() {
        let inner = crate::util::test_dir("state-parent").join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        let mut s = crate::ui::harness::Screen::open(inner);
        let mut r = String::new();
        for _ in 0..500 {
            s.turn();
            r = state_report(&s.app);
            if r.lines().any(|l| l == "parent: inner") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(r.lines().any(|l| l == "parent: inner"), "{r}");
        s.app.max_preview = true;
        s.draw();
        assert!(!state_report(&s.app).contains("parent:"), "`T` hides the column, and the line goes with it");
    }

    /// #211, proposal 3: `zoom: fit` says what fit comes to for a picture, as
    /// the caption does, and stays bare for anything that is not scaled.
    #[test]
    fn zoom_fit_says_its_scale_for_a_picture() {
        let dir = crate::util::test_dir("state-zoom-fit");
        let mut app = App::new(crate::config::Config::load(), dir, egui::Context::default());
        let zoom = |a: &App| state_report(a).lines().find(|l| l.starts_with("zoom: ")).map(str::to_owned);
        assert_eq!(zoom(&app).as_deref(), Some("zoom: fit"), "no picture, no scale");
        app.preview.state = app::PreviewState::Ready(preview::Payload::Image {
            width: 1600,
            height: 1200,
            source: (1600, 1200),
            own: 1.0,
            vector: false,
            rgba: std::sync::Arc::new(Vec::new()),
            caption: String::new(),
        });
        app.preview.fit = 0.25 / app.ctx.pixels_per_point();
        assert_eq!(zoom(&app).as_deref(), Some("zoom: fit (25%)"));
        app.preview.zoom = Some(0.5 / app.ctx.pixels_per_point());
        assert_eq!(zoom(&app).as_deref(), Some("zoom: 50%"), "a zoom of its own is said as before");
    }

    /// #236: a compare row reads as a marker and the two sides, the changed
    /// words in ⟦ ⟧.
    #[test]
    fn a_compare_row_reads_with_its_changed_words() {
        let a: Vec<String> = ["same", "the price is firm", "gone"].map(String::from).into();
        let b: Vec<String> = ["same", "the cost is firm", "new"].map(String::from).into();
        let (rows, _) = diff::compare(&a, &b, 1_000_000);
        let said: Vec<String> = rows.iter().enumerate().map(|(i, r)| compare_row_line(i + 1, r)).collect();
        assert_eq!(said[0], "compare row 1: = same | same");
        assert_eq!(said[1], "compare row 2: ~ the ⟦price⟧ is firm | the ⟦cost⟧ is firm", "{said:?}");
        assert_eq!(said[2], "compare row 3: ~ gone | new", "nothing in common: the whole row is the change");
    }

    /// `FILER_KEYS_DONE`: what a check reads after `--keys`, without pressing
    /// anything more to read it.
    #[test]
    fn the_state_after_the_keys_reads_as_lines() {
        let dir = crate::util::test_dir("state-report");
        std::fs::write(dir.join("a.txt"), b"").unwrap();
        let mut app = App::new(crate::config::Config::load(), dir.clone(), egui::Context::default());
        app.toast("Copied a.txt");
        let report = state_report(&app);
        assert!(report.starts_with(&format!("cwd: {}\n", app.tab().cwd.display())), "{report}");
        for line in [
            "selected: 0",
            "focus: list",
            "max preview: off",
            "quick: off",
            "yank: empty",
            "tab: 1 of 1",
            "overlay: none",
            "pane: closed",
            "items: listing…",
            "jobs: 0 running",
            "list top: 0",
            "preview top: 0 of 0",
            "zoom: fit",
            "split: no",
            "toast: Copied a.txt",
        ] {
            assert!(report.lines().any(|l| l == line), "{line:?} in {report}");
        }
        assert!(!report.contains("job:"), "only while a job is live");
        // A read folder counts its rows, and a filter on it is named.
        let entries = vec![fs::Entry::from_path(dir.join("a.txt")).unwrap()];
        let active = app.active;
        app.tabs[active].current = core::folder::Folder::from_entries(dir.clone(), std::sync::Arc::new(entries), true);
        let report = state_report(&app);
        assert!(report.lines().any(|l| l == "items: 1") && !report.contains("filter:"), "{report}");
        assert!(report.lines().any(|l| l == "position: 1/1"), "{report}");
        app.tabs[active].current.filter = Some(core::folder::Filter { query: "zz".into(), ..Default::default() });
        assert!(state_report(&app).lines().any(|l| l == "filter: zz"));
        app.tabs[active].current.filter = None;
        let report = state_report(&app);
        assert!(!report.contains("input:"), "only while a prompt is open");
        assert!(report.lines().any(|l| l == "view: list"), "{report}");
        assert!(!report.contains("compare:"), "only while a comparison is open");
        assert!(!report.contains("pick:"), "only while a picker is open");
        // Ends with: a machine's own config may have raised a warning first.
        let toasts = |r: &str| r.lines().find(|l| l.starts_with("toasts: ")).map(str::to_owned).unwrap_or_default();
        assert!(toasts(&report).ends_with("Copied a.txt"), "{report}");

        // A toast that has already gone is still in `toasts:`, and a two-line
        // one stays on one line.
        app.toasts.clear();
        app.error("Open failed\nexit code 1");
        app.toasts.clear();
        let report = state_report(&app);
        assert!(report.lines().any(|l| l == "toast: "), "{report}");
        assert!(toasts(&report).ends_with("Copied a.txt | Open failed / exit code 1"), "{report}");

        // A picker lists what it offers, in its order, and the row under the cursor.
        let items: Vec<String> = ["Neovim", "VS Code", "サクラエディタ"].map(String::from).into();
        let mut ov = app::PickOverlay {
            title: "Open with".into(),
            details: vec![String::new(), "2h ago".into(), String::new()],
            items,
            query: String::new(),
            matches: Vec::new(),
            cursor: 1,
            action: app::PickAction::Jump { paths: Vec::new() },
            focused: true,
        };
        ov.refilter();
        app.overlay = app::Overlay::Pick(ov);
        let report = state_report(&app);
        assert!(report.lines().any(|l| l == "pick: Neovim | VS Code (2h ago) | サクラエディタ"), "{report}");
        assert!(report.lines().any(|l| l == "picked: VS Code"), "{report}");
        assert!(report.lines().any(|l| l == "query: "), "{report}");
        if let app::Overlay::Pick(ov) = &mut app.overlay {
            ov.query = "vs".into();
            ov.refilter();
        }
        let report = state_report(&app);
        assert!(report.lines().any(|l| l == "query: vs") && report.lines().any(|l| l == "pick: VS Code (2h ago)"), "{report}");

        // An opener list keeps its commands off `pick:`, on their own line (#275).
        let mut ov = app::PickOverlay {
            title: "Open with".into(),
            details: vec!["no-such-tool %s".into(), "nvim %s".into()],
            items: vec!["Missing tool (not found)".into(), "Neovim".into()],
            query: String::new(),
            matches: Vec::new(),
            cursor: 1,
            action: app::PickAction::OpenWith { paths: Vec::new(), runs: Vec::new(), line: None },
            focused: true,
        };
        ov.refilter();
        app.overlay = app::Overlay::Pick(ov);
        let report = state_report(&app);
        assert!(report.lines().any(|l| l == "pick: Missing tool (not found) | Neovim"), "{report}");
        assert!(report.lines().any(|l| l == "pick runs: no-such-tool %s | nvim %s"), "{report}");

        // A confirm box says what it asks and offers (#230), without its blank lines.
        app.overlay = app::Overlay::Confirm(app::ConfirmOverlay {
            title: "Report a bug".into(),
            body: vec!["filer 0.0.0".into(), String::new(), "Nothing is sent.".into()],
            options: vec![('o', "Open the form".into()), ('n', "Cancel".into())],
            action: app::ConfirmAction::BugReport { url: String::new() },
            dest: None,
        });
        let report = state_report(&app);
        assert!(report.lines().any(|l| l == "overlay: confirm"), "{report}");
        assert!(report.lines().any(|l| l == "confirm: Report a bug | filer 0.0.0 | Nothing is sent."), "{report}");
        assert!(report.lines().any(|l| l == "confirm keys: [o] / <Enter> Open the form | [n] Cancel"), "{report}");

        // The outline entry under the cursor while the outline has the keys,
        // with the 1-based line `<Enter>` opens an editor at (#155, #235).
        app.overlay = app::Overlay::None;
        let toc = |label: &str, line| preview::TocEntry { level: 1, label: label.into(), line };
        app.preview.state = app::PreviewState::Ready(preview::Payload::Text {
            lines: Vec::new(),
            map: Vec::new(),
            extent: Default::default(),
            outline: vec![toc("fn alpha", 3), toc("  fn beta", 41)],
        });
        assert!(state_report(&app).lines().any(|l| l == "preview lines: 0"), "the text's own count (#262)");
        assert!(!state_report(&app).contains("outline:"), "no line while the list has the keys");
        app.preview.outline = Some(1);
        let report = state_report(&app);
        assert!(report.lines().any(|l| l == "outline: 2/2 fn beta (line 42)"), "{report}");

        // The scale the run is at, filer's own beside egui's (#227, #228).
        assert!(report.lines().any(|l| l == "scale: 100% (ppp 1)"), "{report}");
        app.scale = 1.5;
        assert!(state_report(&app).lines().any(|l| l == "scale: 150% (ppp 1)"), "{report}");
    }

    /// Q70: `[ui] backend` names wgpu's backends, `auto` hands the choice to
    /// wgpu, and a name it does not know is a warning, not a window that
    /// fails to open.
    #[test]
    fn the_backend_setting_names_a_wgpu_backend() {
        let ui = |b: &str| config::Ui { backend: b.into(), ..Default::default() };
        assert_eq!(ui("auto").backend_name(), Ok(None));
        assert_eq!(ui("").backend_name(), Ok(None));
        assert_eq!(ui("GL").backend_name(), Ok(Some("gl")));
        assert_eq!(ui("opengl").backend_name(), Ok(Some("gl")));
        assert_eq!(ui("vulkan").backend_name(), Ok(Some("vulkan")));
        #[cfg(windows)]
        assert_eq!(ui("d3d12").backend_name(), Ok(Some("dx12")));
        #[cfg(target_os = "macos")]
        assert_eq!(ui("metal").backend_name(), Ok(Some("metal")));
        let err = ui("directx").backend_name().unwrap_err();
        assert!(err.contains("\"directx\"") && err.contains("auto, vulkan, dx12, metal, gl"), "{err}");

        // The window names what it fell back to; other warnings are left be.
        let mut w = vec![
            "x/filer.toml: [ui] backend = \"directx\" is not one of auto, vulkan, dx12, metal, gl; drawing with the default".to_owned(),
            "[mgr] `x` is bound twice; drawing with the default".to_owned(),
        ];
        name_the_fallback(&mut w, "Gl");
        assert!(w[0].ends_with("; drawing with Gl instead"), "{}", w[0]);
        assert!(w[1].ends_with("drawing with the default"), "not a backend warning: {}", w[1]);
        name_the_fallback(&mut w, "");
        assert!(w[0].ends_with("Gl instead"), "nothing recorded: left as it was");

        // A rejected name and one with no adapter both fall back to `auto`,
        // GL on Windows -- not wgpu's pick, which spun the core again (#243).
        use eframe::wgpu::Backends;
        let gl_only = |b: Backends| b == Backends::GL;
        assert_eq!(pick_backends(Err("bad".into()), true, gl_only), (Some(Backends::GL), None));
        let (b, w) = pick_backends(Ok(Some("vulkan")), true, gl_only);
        assert_eq!(b, Some(Backends::GL), "no Vulkan here: auto, which is GL");
        assert!(w.is_some_and(|w| w.contains("no adapter")));
        assert_eq!(pick_backends(Ok(Some("vulkan")), true, |_| true), (Some(Backends::VULKAN), None));
        assert_eq!(pick_backends(Ok(None), false, |_| true), (None, None), "auto off Windows: wgpu's pick");

        // `auto` is GL on Windows when it is there, wgpu's pick otherwise, and
        // does not even ask about GL elsewhere (2026-10-04, over Q70).
        assert_eq!(auto_backends(true, || true), Some(eframe::wgpu::Backends::GL));
        assert_eq!(auto_backends(true, || false), None, "no GL: wgpu's own pick, no warning");
        assert_eq!(auto_backends(false, || panic!("asked about GL off Windows")), None);

        // A backend of another platform says so, rather than "no adapter".
        let other = if cfg!(target_os = "macos") { "dx12" } else { "metal" };
        let err = ui(other).backend_name().unwrap_err();
        assert!(err.ends_with("only; drawing with the default"), "{err}");
    }

    /// #168, proposal 5: a script nothing has moved for longer than the stall
    /// allows, past the wait it was on, gets a report saying where it stopped;
    /// one still inside its wait does not.
    #[test]
    fn a_stalled_script_leaves_a_report() {
        let dir = crate::util::test_dir("keys-stalled");
        let labels: Vec<String> = ["j", "<Wait:2000>", "k"].map(String::from).into();
        let watched = |quiet: Duration, due: Duration, name: &str| {
            let done = dir.join(name);
            let at = std::time::Instant::now().checked_sub(quiet).unwrap();
            let watch = Arc::new(Mutex::new(ScriptWatch { left: 1, at, due, finished: false }));
            watch_script(watch.clone(), labels.clone(), done.clone(), egui::Context::default());
            std::thread::sleep(Duration::from_millis(1500));
            watch.lock().unwrap().finished = true;
            std::fs::read_to_string(done).ok()
        };
        let stuck = keyscript::STALL + Duration::from_secs(5);
        let said = watched(stuck, Duration::ZERO, "stuck.done").expect("a report");
        assert!(said.starts_with("keys: stalled\n"), "{said}");
        assert!(said.contains("pressed: 2 of 3 (last: `<Wait:2000>`)\nleft: k\n"), "{said}");
        assert_eq!(watched(stuck, Duration::from_secs(60), "waiting.done"), None, "still inside a <Wait:60000>");
    }

    /// #131: the bold face is found beside a regular face with no `-Regular`.
    #[test]
    fn a_bold_sibling_without_regular_in_the_name() {
        let dir = std::path::Path::new("/fonts");
        assert!(bold_siblings(&dir.join("DejaVuSansMono.ttf")).contains(&dir.join("DejaVuSansMono-Bold.ttf")));
        assert!(bold_siblings(&dir.join("LiberationMono-Regular.ttf")).contains(&dir.join("LiberationMono-Bold.ttf")));
    }

    /// The Linux lane's first screenshot: Japanese names were boxes, because
    /// only Windows' font folders were searched. Each platform now names its
    /// own Japanese face.
    #[test]
    fn each_platform_looks_for_a_japanese_face() {
        let all: Vec<PathBuf> = system_fonts().into_iter().chain(fallback_fonts()).collect();
        let want = if cfg!(windows) {
            r"C:\Windows\Fonts\meiryo.ttc"
        } else if cfg!(target_os = "macos") {
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc"
        } else {
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"
        };
        assert!(all.iter().any(|p| p == std::path::Path::new(want)), "{all:?}");
    }

    /// #126: one path, and a second refused with the likely reason rather than
    /// quietly taking the first one's place.
    #[test]
    fn a_second_path_is_refused() {
        let mut cli = Cli { path: None, cwd_file: None, chooser_file: None, keys: Vec::new() };
        assert!(take_path(&mut cli, "awkward").is_ok());
        let why = take_path(&mut cli, "names").unwrap_err();
        assert!(why.contains("\"awkward\"") && why.contains("\"names\"") && why.contains("quotes"), "{why}");
        assert_eq!(cli.path.as_deref(), Some(std::path::Path::new("awkward")), "the first is kept");
    }

    /// #194: a Windows path is named as typed, not with every `\` doubled.
    #[test]
    fn a_refused_path_keeps_its_backslashes() {
        let mut cli = Cli { path: None, cwd_file: None, chooser_file: None, keys: Vec::new() };
        assert!(take_path(&mut cli, r"C:\dev").is_ok());
        let why = take_path(&mut cli, r"C:\Windows").unwrap_err();
        assert!(why.starts_with(r#"more than one path: "C:\dev" and "C:\Windows" ("#), "{why}");
    }

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

/// TESTING.md section 26: what `<F12>` shows and what it decides, read off
/// the frame and the `<State:>` lines, entered through the same
/// `handle_input` the window uses. Nothing here answers the report with `o`,
/// `<Enter>` or `c`: those hand the link to the browser or the clipboard,
/// and a test run must not reach either.
#[cfg(test)]
mod bug_report_f12 {
    use super::state_report;
    use crate::app::{ConfirmAction, Overlay};
    use crate::config::{Config, Keymap};
    use crate::ui::harness::Screen;

    fn key(k: egui::Key) -> egui::Event {
        egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }
    }

    /// The built-in keys whatever the machine binds, no warnings, and two
    /// config files read from under a home directory with a name in it.
    fn screen(label: &str) -> Screen {
        let dir = crate::util::test_dir(label);
        let home = std::path::Path::new("/home/someone-in-a-test/.config");
        let loaded = vec![home.join("yazi").join("keymap.toml"), home.join("filer").join("filer.toml")];
        let cfg = Config { keymap: Keymap::load(&[]).0, loaded, warnings: Vec::new(), ..Config::load() };
        Screen::with_config(cfg, dir)
    }

    /// `<F12>` after `j` `k`, with an error raised before them.
    fn pressed(label: &str) -> (Screen, crate::ui::harness::Painted) {
        let mut s = screen(label);
        s.app.error("Copy: a.txt: denied");
        s.typed("jk");
        let f = s.feed(vec![key(egui::Key::F12)]);
        (s, f)
    }

    fn report_url(s: &Screen) -> String {
        match &s.app.overlay {
            Overlay::Confirm(c) => match &c.action {
                ConfirmAction::BugReport { url } => url.clone(),
                other => panic!("a confirm box, but not the report: {other:?}"),
            },
            _ => panic!("no Report a bug panel"),
        }
    }

    fn says_line(report: &str, line: &str) -> bool {
        report.lines().any(|l| l == line)
    }

    /// The panel half of row 26.1 (the browser opening is the machine's):
    /// `<F12>` shows the panel first, with every line the report carries and
    /// the first button naming `<Enter>` -- and opens nothing, copies nothing
    /// and says nothing until it is answered. Not named as the row's id on
    /// purpose: `make-testcheck` takes a row off the list for a test that
    /// names it, and half of this one is still for a person.
    #[test]
    fn f12_draws_the_report_and_opens_nothing() {
        let (s, f) = pressed("f12-draws");
        let mut lines = vec!["Report a bug".to_owned(), crate::bugreport::version_line()];
        lines.extend(crate::bugreport::os_line().lines().map(str::to_owned));
        lines.push("Last keys: j k".into());
        // Rendering and Scale come from this machine's last run, when there
        // was one; whatever `context` says is what the panel has to show.
        let context = crate::bugreport::context(Some("Copy: a.txt: denied"), &s.app.cfg.loaded);
        lines.extend(context.lines().map(str::to_owned));
        lines.push("The form opens with these filled in; nothing is sent until you submit it there.".into());
        for l in &lines {
            assert!(f.texts.iter().any(|t| t == l), "{l:?} is drawn: {:?}", f.texts);
        }
        let sep = std::path::MAIN_SEPARATOR;
        assert!(f.texts.iter().any(|t| *t == format!("Config: yazi{sep}keymap.toml, filer{sep}filer.toml")), "{:?}", f.texts);
        assert!(f.texts.iter().any(|t| t == "Last error: Copy: a.txt: denied"), "{:?}", f.texts);

        // The buttons, under the body, the first naming the key that picks it.
        let first = f.placed(" [o] / <Enter> Open the form in your browser ").expect("the first button");
        let body = f.placed(&crate::bugreport::version_line()).unwrap();
        assert!(body.y < first.y, "body above the buttons: {body:?} {first:?}");
        assert!(f.says(" [c] Copy the link ") && f.says(" [n] Cancel "), "{:?}", f.texts);

        // Nothing has left the program yet.
        assert!(s.app.last_report.is_none(), "no link opened or copied");
        let toasts: Vec<&str> = s.app.toasts.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(toasts, ["Copy: a.txt: denied"], "no toast of its own");

        // And `<State:>` reads the same panel back as text.
        let r = state_report(&s.app);
        assert!(says_line(&r, "overlay: confirm"), "{r}");
        let confirm = r.lines().find(|l| l.starts_with("confirm: ")).unwrap_or_default();
        assert!(confirm.starts_with(&format!("confirm: Report a bug | {} | ", crate::bugreport::version_line())), "{r}");
        assert!(confirm.contains(" | Last keys: j k | Last error: Copy: a.txt: denied | "), "{r}");
        assert!(says_line(&r, "confirm keys: [o] / <Enter> Open the form in your browser | [c] Copy the link | [n] Cancel"), "{r}");
        assert!(!r.lines().any(|l| l.starts_with("report:")), "no report line before one is opened: {r}");
    }

    /// Row 26.2, the half that is filer's: the link fills in the version, the
    /// OS, the keys and what filer knew, and nothing else; and the config
    /// files read from under a home directory are named without it (Q64).
    #[test]
    fn the_link_fills_four_fields_and_names_no_home() {
        let (s, _) = pressed("f12-fields");
        let url = report_url(&s);
        let query = url.split_once('?').map(|(_, q)| q).expect("a query string");
        let names: Vec<&str> = query.split('&').map(|p| p.split_once('=').map_or(p, |(n, _)| n)).collect();
        assert_eq!(names, ["template", "version", "os", "keys", "context"], "{url}");
        assert!(url.contains("&keys=Last%20keys%2C%20oldest%20first%3A%20j%20k&"), "the keys before <F12>: {url}");
        assert!(url.contains("Config%3A%20yazi"), "the config files by name: {url}");
        assert!(!url.contains("someone-in-a-test") && !url.contains("%2Fhome%2F"), "no home directory: {url}");
    }

    /// Row 26.11's `<Esc>` and `n`: the panel closes, nothing is opened or copied,
    /// and `<State:>` has no `report:` line. Once a link has gone (`c` and `o`
    /// both leave it in `last_report`, which `app`'s own tests read), the
    /// line carries exactly that link.
    #[test]
    fn esc_or_n_drops_the_report() {
        for (how, answer) in [("<Esc>", vec![key(egui::Key::Escape)]), ("n", vec![egui::Event::Text("n".into())])] {
            let (mut s, _) = pressed("f12-drop");
            let toasts = s.app.toasts.len();
            let f = s.feed(answer);
            assert!(matches!(s.app.overlay, Overlay::None), "{how} closes the panel");
            assert!(!f.says("Report a bug"), "{how}: not drawn any more: {:?}", f.texts);
            assert!(s.app.last_report.is_none(), "{how}: nothing opened or copied");
            assert_eq!(s.app.toasts.len(), toasts, "{how}: and nothing said");
            let r = state_report(&s.app);
            assert!(says_line(&r, "overlay: none") && !r.contains("report:"), "{how}: {r}");
        }
        let (mut s, _) = pressed("f12-report-line");
        let url = report_url(&s);
        s.app.last_report = Some(url.clone());
        assert!(says_line(&state_report(&s.app), &format!("report: {url}")));
    }

    /// #162, proposal 3: what a shell command started is in the state file,
    /// by PID and then the line, so a check stops that process and nothing
    /// else of the same name.
    #[test]
    fn the_state_file_names_what_was_launched_by_pid() {
        let mut s = screen("state-launched");
        assert!(!state_report(&s.app).contains("launched:"), "nothing launched yet");
        s.app.act(crate::config::cmd::Act::Shell { run: "exit 0".into(), block: false, confirm: false, orphan: false });
        let r = state_report(&s.app);
        let line = r.lines().find_map(|l| l.strip_prefix("launched: ")).unwrap_or_else(|| panic!("no launched line: {r}"));
        let (pid, rest) = line.split_once(' ').unwrap_or_else(|| panic!("a PID, then the line: {line}"));
        assert!(pid.parse::<u32>().is_ok_and(|p| p > 0), "the shell's PID: {line}");
        assert_eq!(rest, "exit 0");
    }

    /// 26.12: a key the panel does not offer, typed as the window delivers
    /// it, leaves the panel up and does nothing else; `n` then closes it.
    #[test]
    fn a_key_the_panel_does_not_offer_leaves_it_up() {
        let (mut s, _) = pressed("f12-stray");
        let toasts = s.app.toasts.len();
        let f = s.typed("(");
        assert!(f.says(" [o] / <Enter> Open the form in your browser "), "still drawn: {:?}", f.texts);
        let r = state_report(&s.app);
        assert!(says_line(&r, "overlay: confirm") && !r.contains("report:"), "{r}");
        assert!(s.app.last_report.is_none() && s.app.toasts.len() == toasts, "nothing opened, copied or said");
        s.typed("n");
        assert!(says_line(&state_report(&s.app), "overlay: none"));
    }

    /// Row 26.9, the half that is filer's: with the terminal pane holding the
    /// keys, `<F12>` is not the report -- the `[term]` layer does not claim
    /// it, so it goes to the shell. (No pane runs in a test, so where the
    /// key lands in the shell is the machine's half.)
    #[test]
    fn f12_in_the_terminal_pane_is_not_the_report() {
        let mut s = screen("f12-term");
        let f12 = crate::config::keys::Key::parse("<F12>").unwrap();
        assert!(!s.app.cfg.keymap.term.iter().any(|b| b.on.first() == Some(&f12)), "[term] leaves <F12> to the shell");
        s.app.term_focus = true;
        let f = s.feed(vec![key(egui::Key::F12)]);
        assert!(matches!(s.app.overlay, Overlay::None), "no panel");
        assert!(!f.says("Report a bug") && s.app.term_focus, "and the pane keeps the keys");
    }
}
