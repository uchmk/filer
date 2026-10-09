//! `filer --keys "<Tab>C"`: keys pressed by filer itself, once it has started.
//!
//! The real-machine test sessions drive filer from outside, and every run
//! rebuilt the same Win32 scaffolding to do it -- find the window (not the one
//! `MainWindowHandle` names, which can be winit's event target), then
//! `SendInput`, which a screen saver silently swallows, or `PostMessage`. This
//! does it from inside, for a filer this command line starts, and for no other:
//! it opens no door into a filer that is already running (Q24).
//!
//! The keys go in as the `egui::Event`s a keyboard would have produced, through
//! `raw_input_hook`, so they take the same road as a real press -- the chord
//! rules in `handle_input`, the overlay's layer, the terminal's bytes. Which
//! events a key becomes is read back out of [`keys::from_egui`] rather than
//! written down a second time, so the two cannot drift apart.

use std::time::Duration;

use crate::config::keys::{self, Code, Key};

/// One step of a script: a key to press, or a pause.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Key(Key),
    /// `<Wait:500>`: this long after the key before it went in, before the
    /// next one does. A shell in the pane runs at its own pace, which filer
    /// cannot see, so "settled" says nothing about it; every real-machine run
    /// that drove the pane filled the gap with harmless keys instead and
    /// guessed how long they took (#93, #119 and five more).
    Wait(Duration),
    /// `<Now>`: the key after it goes in on the next frame, without waiting
    /// for what the key before started to settle. For the rows that measure
    /// something halfway -- a trash still running (12.14), two keys inside the
    /// preview's 40 ms debounce (47.2) -- which the settled wait otherwise
    /// waits out (Q41).
    Now,
    /// `<Shot:name@preview>` crops it to the preview pane (the name keeps the
    /// suffix). `<Shot:name>`: the window as it is now, saved as `name.png` beside the
    /// `FILER_KEYS_DONE` file, before the next key goes in. A check that
    /// compares the screen between two keys started filer once per picture
    /// and relied on the windows coming out the same size (Q42, #154).
    Shot(String),
    /// `<State:name>`: what `FILER_KEYS_DONE` would say now, written to
    /// `name.txt` beside it. That file is written when the script ends, so a
    /// script ending in `q` reports `overlay: none`, and reading a state
    /// halfway meant leaving the `q` off and stopping filer from outside (#230).
    State(String),
    /// `<Quit>`: end filer whatever is open, as the window's close button
    /// would. `q` is a key like any other, and the compare view, the pane
    /// and a prompt each take it for something else, so a script ending in
    /// `q` there never ended (#236).
    Quit,
    /// `<PaneText:name>`: what the terminal pane shows, as text, written to
    /// `name.pane.txt` beside the `FILER_KEYS_DONE` file. A full-screen
    /// program's footer or a prompt was read off a picture before (#229).
    PaneText(String),
    /// `<PreviewText:name>`: the text the preview pane holds, written to
    /// `name.preview.txt` beside the `FILER_KEYS_DONE` file (#165).
    PreviewText(String),
    /// `<Paste>` (and `<C-v>`): the clipboard as it is when the key goes in,
    /// as the paste event the platform makes of Ctrl+V. A real keyboard never
    /// sends the press, so a text field hears nothing from one (#260).
    Paste,
    /// `<Click:0.5,0.4>` and `<RClick:…>`: the left or right button at that
    /// place in the window, as fractions of its width and height. The right-click
    /// and wheel rows were each driven by a hundred lines of `SendInput`
    /// written afresh in every real-machine run (#260).
    /// `C-` / `A-` / `S-` before the place holds that key (`<Click:C-0.5,0.4>`).
    Click { right: bool, mods: u8, at: At },
    /// `<Hover:0.5,0.4>`: the pointer moved there with no button, so a hover
    /// style or a cursor shape can be read from `<State:>`.
    Hover { at: At },
    /// `<Wheel:-3@0.5,0.4>`: this many lines of wheel there (negative is down).
    /// A fraction is a part of a notch (`<Wheel:0.25@…>`), and `C-` / `A-` / `S-`
    /// before the number holds that modifier (`<Wheel:C-1@…>`, #280).
    Wheel { milli: i32, mods: u8, at: At },
    /// `<Drag:0.2,0.3-0.6,0.3>`: the left button down at the first place, moved
    /// to the second and let go there (#280; `SendInput` could not move the cursor).
    Drag { from: At, to: At },
}

/// `Step::Wheel`'s modifiers: these bits.
pub const MOD_CTRL: u8 = 1;
pub const MOD_ALT: u8 = 2;
pub const MOD_SHIFT: u8 = 4;

/// A place in the window, in thousandths of its width and height.
pub type At = (u16, u16);

/// What a step becomes in the frame loop.
#[derive(Clone, Debug, PartialEq)]
pub enum Press {
    Events(Vec<egui::Event>),
    Wait(Duration),
    Now,
    Shot(String),
    State(String),
    Quit,
    PaneText(String),
    PreviewText(String),
    Paste,
    /// Pointer steps wait for the window's size, known only in the frame loop.
    Click { right: bool, mods: u8, at: At },
    Hover { at: At },
    Wheel { milli: i32, mods: u8, at: At },
    Drag { from: At, to: At },
}

/// A step as the frame loop takes it; `None` for a key no keyboard can type.
pub fn press(step: &Step) -> Option<Press> {
    match step {
        Step::Key(k) if k.code == Code::Char('v') && k.ctrl && !k.alt && !k.sup => Some(Press::Paste),
        Step::Key(k) => events(k).map(Press::Events),
        Step::Paste => Some(Press::Paste),
        Step::PaneText(name) => Some(Press::PaneText(name.clone())),
        Step::PreviewText(name) => Some(Press::PreviewText(name.clone())),
        Step::Click { right, mods, at } => Some(Press::Click { right: *right, mods: *mods, at: *at }),
        Step::Hover { at } => Some(Press::Hover { at: *at }),
        Step::Wheel { milli, mods, at } => Some(Press::Wheel { milli: *milli, mods: *mods, at: *at }),
        Step::Drag { from, to } => Some(Press::Drag { from: *from, to: *to }),
        Step::Wait(d) => Some(Press::Wait(*d)),
        Step::Now => Some(Press::Now),
        Step::Shot(name) => Some(Press::Shot(name.clone())),
        Step::State(name) => Some(Press::State(name.clone())),
        Step::Quit => Some(Press::Quit),
    }
}

/// A step as it was written, for saying where a script stopped.
pub fn label(step: &Step) -> String {
    match step {
        Step::Key(k) => k.to_string(),
        Step::Wait(d) => format!("<Wait:{}>", d.as_millis()),
        Step::Now => "<Now>".into(),
        Step::Shot(name) => format!("<Shot:{name}>"),
        Step::State(name) => format!("<State:{name}>"),
        Step::Quit => "<Quit>".into(),
        Step::Paste => "<Paste>".into(),
        Step::PaneText(name) => format!("<PaneText:{name}>"),
        Step::PreviewText(name) => format!("<PreviewText:{name}>"),
        Step::Click { right, mods, at } => format!("<{}Click:{}{}>", if *right { "R" } else { "" }, held_text(*mods), at_text(*at)),
        Step::Hover { at } => format!("<Hover:{}>", at_text(*at)),
        Step::Wheel { milli, mods, at } => {
            format!("<Wheel:{}{}@{}>", held_text(*mods), *milli as f32 / 1000.0, at_text(*at))
        }
        Step::Drag { from, to } => format!("<Drag:{}-{}>", at_text(*from), at_text(*to)),
    }
}

fn held_text(mods: u8) -> String {
    [(MOD_CTRL, "C-"), (MOD_ALT, "A-"), (MOD_SHIFT, "S-")].iter().filter(|(bit, _)| mods & bit != 0).map(|(_, t)| *t).collect()
}

/// `C-A-` in front of `text`: the modifier bits and what is left.
fn held_prefix(mut text: &str) -> (u8, &str) {
    let mut mods = 0u8;
    while let Some((bit, rest)) = [(MOD_CTRL, "C-"), (MOD_ALT, "A-"), (MOD_SHIFT, "S-")].iter().find_map(|(bit, t)| text.strip_prefix(t).map(|rest| (*bit, rest))) {
        (text, mods) = (rest, mods | bit);
    }
    (mods, text)
}

fn modifiers(mods: u8) -> egui::Modifiers {
    egui::Modifiers { ctrl: mods & MOD_CTRL != 0, command: mods & MOD_CTRL != 0, alt: mods & MOD_ALT != 0, shift: mods & MOD_SHIFT != 0, ..Default::default() }
}

fn at_text((x, y): At) -> String {
    format!("{},{}", x as f32 / 1000.0, y as f32 / 1000.0)
}

/// `0.5,0.4` as a place in the window.
fn at(text: &str) -> Option<At> {
    let (x, y) = text.split_once(',')?;
    let part = |v: &str| v.parse::<f32>().ok().filter(|v| (0.0..=1.0).contains(v)).map(|v| (v * 1000.0).round() as u16);
    Some((part(x)?, part(y)?))
}

/// The pointer steps' tokens: `<Click:0.5,0.4>`, `<RClick:0.5,0.4>`,
/// `<Wheel:-3@0.5,0.4>`, `<Wheel:C-0.25@0.5,0.4>`, `<Drag:0.2,0.3-0.6,0.3>`. `None` for a token that is not one.
fn pointer(token: &str) -> Option<Result<Step, String>> {
    let inner = token.strip_prefix('<')?.strip_suffix('>')?;
    let (name, arg) = inner.split_once(':')?;
    let bad = || format!("`{token}` is not a pointer step; write a place as fractions of the window, as `<Click:0.5,0.4>`, `<Wheel:-3@0.5,0.4>` or `<Drag:0.2,0.3-0.6,0.3>`");
    Some(match name {
        "Click" | "RClick" => {
            let (mods, place) = held_prefix(arg);
            at(place).map(|at| Step::Click { right: name == "RClick", mods, at }).ok_or_else(bad)
        }
        "Hover" => at(arg).map(|at| Step::Hover { at }).ok_or_else(bad),
        "Wheel" => arg
            .split_once('@')
            .and_then(|(n, place)| {
                let (mods, n) = held_prefix(n);
                let lines = n.parse::<f32>().ok().filter(|n| n.is_finite() && n.abs() <= 1000.0)?;
                Some(Step::Wheel { milli: (lines * 1000.0).round() as i32, mods, at: at(place)? })
            })
            .ok_or_else(bad),
        "Drag" => arg
            .split_once('-')
            .and_then(|(from, to)| Some(Step::Drag { from: at(from)?, to: at(to)? }))
            .ok_or_else(bad),
        _ => return None,
    })
}

/// The events a pointer step becomes, in a window of `rect`.
pub fn pointer_events(press: &Press, rect: egui::Rect) -> Vec<egui::Event> {
    let place = |(x, y): At| rect.min + egui::vec2(rect.width() * x as f32 / 1000.0, rect.height() * y as f32 / 1000.0);
    match press {
        Press::Hover { at } => vec![egui::Event::PointerMoved(place(*at))],
        Press::Click { right, mods, at } => {
            let (pos, button) = (place(*at), if *right { egui::PointerButton::Secondary } else { egui::PointerButton::Primary });
            let modifiers = modifiers(*mods);
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton { pos, button, pressed: true, modifiers },
                egui::Event::PointerButton { pos, button, pressed: false, modifiers },
            ]
        }
        Press::Wheel { milli, mods, at } => {
            let modifiers = modifiers(*mods);
            vec![
                egui::Event::PointerMoved(place(*at)),
                egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: egui::vec2(0.0, *milli as f32 / 1000.0), phase: egui::TouchPhase::Move, modifiers },
            ]
        }
        Press::Drag { from, to } => {
            let (a, b, button, modifiers) = (place(*from), place(*to), egui::PointerButton::Primary, egui::Modifiers::NONE);
            vec![
                egui::Event::PointerMoved(a),
                egui::Event::PointerButton { pos: a, button, pressed: true, modifiers },
                egui::Event::PointerMoved(a.lerp(b, 0.5)),
                egui::Event::PointerMoved(b),
                egui::Event::PointerButton { pos: b, button, pressed: false, modifiers },
            ]
        }
        _ => Vec::new(),
    }
}

/// Ctrl+V as the platform delivers it: the clipboard's text, now.
pub fn paste_event() -> egui::Event {
    egui::Event::Paste(crate::exec::get_clipboard().unwrap_or_default())
}

/// How long nothing may be pressed, past any `<Wait:N>` due, before a script
/// counts as stuck. A key waits at most five seconds for things to settle, so
/// this is only reached when the frame loop itself has stopped running.
pub const STALL: Duration = Duration::from_secs(30);

/// What `FILER_KEYS_DONE` holds for a script that stopped part way (#168,
/// proposal 5): which keys went in, the last of them, and what was left.
/// Before this an unattended run that stalled left nothing at all, so the
/// result read as missing rather than as failed.
pub fn stalled_report(labels: &[String], left: usize, quiet: Duration) -> String {
    let pressed = labels.len().saturating_sub(left);
    let last = pressed.checked_sub(1).map_or("nothing yet".into(), |i| format!("`{}`", labels[i]));
    format!(
        "keys: stalled\nstalled: {} s with nothing pressed\npressed: {pressed} of {} (last: {last})\nleft: {}\n",
        quiet.as_secs(),
        labels.len(),
        labels[pressed..].join(" "),
    )
}

/// What a panic leaves for a script that is running: the done file reads
/// `keys: panicked`, with where and why, instead of never arriving or
/// reading as a stall. A run that vanished twice (#259, #173) could not say
/// whether filer had panicked or been ended from outside.
pub fn panic_report(info: &std::panic::PanicHookInfo<'_>) -> String {
    let why = match (info.payload().downcast_ref::<&str>(), info.payload().downcast_ref::<String>()) {
        (Some(m), _) => (*m).to_owned(),
        (_, Some(m)) => m.clone(),
        _ => "no message".to_owned(),
    };
    let at = info.location().map_or_else(|| "unknown".to_owned(), |l| format!("{}:{}", l.file().replace('\\', "/"), l.line()));
    let thread = std::thread::current().name().unwrap_or("unnamed").to_owned();
    format!("keys: panicked\nthread: {thread}\nat: {at}\nwhy: {}\n", why.replace('\n', " / "))
}

/// Before the window opens, for a run that writes `FILER_KEYS_DONE`: a panic
/// anywhere writes [`panic_report`] to that file and to `<file>.panic`, and
/// ends the process with a code that is not 0, whichever thread it was on.
pub fn install_panic_report(done: std::path::PathBuf) {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = panic_report(info);
        let _ = std::fs::write(&done, &report);
        let mut named = done.clone().into_os_string();
        named.push(".panic");
        let _ = std::fs::write(std::path::PathBuf::from(named), &report);
        before(info);
        std::process::exit(101);
    }));
}

/// The last line of `FILER_KEYS_DONE` when the app quit while the script ran
/// (`q` as its last key, say): the frame that would have written `keys: done`
/// never comes for a window that has closed, so the file was missing and read
/// as a stall. A quit with keys still to go says how many.
pub fn quit_report(left: usize) -> String {
    match left {
        // `keys: done` stays the last line, which is what readers look at.
        0 => "quit: yes\nkeys: done\n".to_owned(),
        n => format!("keys: quit\nleft: {n} not pressed\n"),
    }
}

/// The last line of `FILER_KEYS_DONE` when the window was closed from outside
/// (the title bar's ×, `WM_CLOSE`) while the script ran: a script waiting on
/// the file otherwise waited out its timeout, like for a vanished process.
pub fn closed_report(left: usize) -> String {
    match left {
        0 => "quit: window closed\nkeys: done\n".to_owned(),
        n => format!("keys: closed\nleft: {n} not pressed\n"),
    }
}

/// What `FILER_KEYS_DONE` holds for a script refused before any window opened
/// (#193, proposal 1): a run started detached never sees the message on the
/// command line, and without this the file simply never arrived.
pub fn refused_report(why: &str) -> String {
    format!("keys: refused\nwhy: {why}\n")
}

/// The name in `<Shot:name>` or `<State:name>`: letters, digits, `-` and `_`,
/// so it is a file name on every platform and cannot climb out of the folder
/// it is saved in. `what` is the tag, `Shot` or `State`.
fn named(token: &str, what: &str) -> Result<String, String> {
    let name = token.strip_prefix(&format!("<{what}:")).and_then(|t| t.strip_suffix('>')).unwrap_or("");
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        let noun = match what {
            "Shot" => "shot",
            "PaneText" => "pane text",
            "PreviewText" => "preview text",
            _ => "state",
        };
        return Err(format!("`{token}` is not a {noun}; name it with letters, digits, - and _, as `<{what}:before>`"));
    }
    Ok(name.to_owned())
}

/// The name in `<Shot:name>` or `<Shot:name@preview>`; the `@preview` stays on
/// the name, which `shot_crop` splits off where the picture is saved.
fn shot_name(token: &str) -> Result<String, String> {
    match token.strip_suffix("@preview>") {
        Some(head) => Ok(format!("{}{SHOT_PREVIEW}", named(&format!("{head}>"), "Shot")?)),
        None => named(token, "Shot"),
    }
}

/// The suffix that crops a shot to the preview pane.
pub const SHOT_PREVIEW: &str = "@preview";

/// `<Wait:500>` as a pause, in milliseconds.
fn wait(token: &str) -> Option<Result<Duration, String>> {
    let ms = token.strip_prefix("<Wait:")?.strip_suffix('>')?;
    Some(match ms.parse::<u64>() {
        Ok(n) if n <= 60_000 => Ok(Duration::from_millis(n)),
        _ => Err(format!("`{token}` is not a wait; write milliseconds up to 60000, as `<Wait:500>`")),
    })
}

/// `<Tab>C<C-S-t>gg` as the keys it names, in yazi's notation: `<…>` is one
/// key, anything else is one key per character. `<Wait:N>` pauses N ms, and
/// `<Now>` presses the next key without waiting for the last one to settle,
/// `<Shot:name>` saves the window as `name.png` and `<State:name>` the state
/// report as `name.txt`.
pub fn parse(script: &str) -> Result<Vec<Step>, String> {
    let mut out = Vec::new();
    let mut rest = script;
    while let Some(c) = rest.chars().next() {
        // A plain space never arrived as one: the rest of the script went to
        // the list instead of the pane, so `<C-t>echo hi<Enter>` typed `echo`
        // and walked the list (#110). Refused before the window opens.
        if c.is_whitespace() {
            return Err(format!("a plain space cannot be typed here; write `<Space>` (at `{rest}`)"));
        }
        let token = if c == '<' {
            match rest.find('>') {
                Some(end) => &rest[..=end],
                None => return Err(format!("`{rest}` has no closing `>`")),
            }
        } else {
            &rest[..c.len_utf8()]
        };
        let step = match wait(token) {
            Some(d) => Step::Wait(d?),
            None if token == "<Now>" => Step::Now,
            None if token == "<Quit>" => Step::Quit,
            None if token.starts_with("<Shot:") => Step::Shot(shot_name(token)?),
            None if token.starts_with("<State:") => Step::State(named(token, "State")?),
            None if token == "<Paste>" => Step::Paste,
            None if token.starts_with("<PaneText:") => Step::PaneText(named(token, "PaneText")?),
            None if token.starts_with("<PreviewText:") => Step::PreviewText(named(token, "PreviewText")?),
            None if pointer(token).is_some() => pointer(token).unwrap_or_else(|| unreachable!())?,
            None => Step::Key(Key::parse(token).ok_or_else(|| format!("`{token}` is not a key"))?),
        };
        rest = &rest[token.len()..];
        // `<C-+>*45`: the key that many times. Only after a `<…>` key, since a
        // bare `*` is a key of its own. A held-down key's rows have the count
        // as their answer, and a script of forty-five identical tokens is
        // easy to miscount (#216).
        let mut times = 1;
        if c == '<' && !matches!(step, Step::Wait(_) | Step::Now | Step::Quit | Step::Shot(_) | Step::State(_) | Step::PaneText(_) | Step::PreviewText(_)) {
            if let Some(after) = rest.strip_prefix('*') {
                let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
                match digits.parse::<usize>() {
                    Ok(n) if (1..=1000).contains(&n) => {
                        times = n;
                        rest = &after[digits.len()..];
                    }
                    _ => return Err(format!("`{token}*` has to be followed by a count from 1 to 1000, as `{token}*45`")),
                }
            }
        }
        for _ in 0..times {
            out.push(step.clone());
        }
    }
    // `<Now>` is about the key after it, so there has to be one.
    for (i, step) in out.iter().enumerate() {
        if *step == Step::Now && !matches!(out.get(i + 1), Some(Step::Key(_) | Step::State(_) | Step::Shot(_) | Step::PaneText(_) | Step::PreviewText(_) | Step::Paste | Step::Click { .. } | Step::Hover { .. } | Step::Wheel { .. } | Step::Drag { .. })) {
            return Err("`<Now>` has to come right before a key or a reading (`<State:x>`, `<Shot:x>`, `<PaneText:x>`), as `d<Now>w` or `<A-c><Now><State:mid>`".into());
        }
    }
    Ok(out)
}

/// The events one press of `key` arrives as. `None` for a key no keyboard
/// event can produce, which `--keys` refuses before the window opens.
pub fn events(key: &Key) -> Option<Vec<egui::Event>> {
    // A character typed without a chord arrives as text, which is how
    // `handle_input` expects it -- a key event for it would be dropped.
    if let Code::Char(c) = key.code {
        if !key.ctrl && !key.alt && !key.sup {
            return Some(vec![egui::Event::Text(c.to_string())]);
        }
    }
    // Everything else is a key event: the one, among every key and modifier
    // combination, that reads back as this key.
    for &k in egui::Key::ALL {
        for bits in 0..8u8 {
            let modifiers = egui::Modifiers {
                shift: bits & 1 != 0,
                ctrl: bits & 2 != 0,
                command: bits & 2 != 0,
                alt: bits & 4 != 0,
                ..Default::default()
            };
            if keys::from_egui(k, &modifiers).as_ref() == Some(key) {
                let press = |pressed| egui::Event::Key { key: k, physical_key: None, pressed, repeat: false, modifiers };
                return Some(vec![press(true), press(false)]);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    /// The two endings that used to leave no `FILER_KEYS_DONE` at all: a
    /// script whose last key quits, and one refused before the window opened.
    #[test]
    fn a_quit_or_a_refusal_still_says_how_the_script_ended() {
        assert_eq!(quit_report(0), "quit: yes\nkeys: done\n");
        assert_eq!(quit_report(3), "keys: quit\nleft: 3 not pressed\n");
        assert_eq!(closed_report(0), "quit: window closed\nkeys: done\n");
        assert_eq!(closed_report(2), "keys: closed\nleft: 2 not pressed\n");
        let why = parse("j k").unwrap_err();
        assert_eq!(refused_report(&why), format!("keys: refused\nwhy: {why}\n"));
    }

    use super::*;

    /// #168: a script that stopped says how far it got, in the notation it
    /// was written in.
    #[test]
    fn a_stalled_script_says_where_it_stopped() {
        let labels: Vec<String> = parse("d<Wait:9000>u<Shot:after>").unwrap().iter().map(label).collect();
        assert_eq!(labels, ["d", "<Wait:9000>", "u", "<Shot:after>"]);
        let said = stalled_report(&labels, 2, Duration::from_secs(39));
        assert_eq!(said, "keys: stalled\nstalled: 39 s with nothing pressed\npressed: 2 of 4 (last: `<Wait:9000>`)\nleft: u <Shot:after>\n");
        assert!(stalled_report(&labels, 4, STALL).contains("pressed: 0 of 4 (last: nothing yet)"));
    }

    /// Q42: `<Shot:name>` is a step of its own, and its name is a plain file name.
    #[test]
    fn a_shot_is_named() {
        let got = parse("j<Quit>").unwrap();
        assert_eq!(got[1], Step::Quit, "#236: `<Quit>` is a step, not a key");
        assert_eq!((label(&got[1]).as_str(), press(&got[1])), ("<Quit>", Some(Press::Quit)));
        let got = parse("j<State:mid>k").unwrap();
        assert_eq!(got[1], Step::State("mid".into()), "#230: `<State:name>` likewise");
        assert_eq!(label(&got[1]), "<State:mid>");
        for bad in ["<State:>", "<State:../x>"] {
            let err = parse(bad).unwrap_err();
            assert!(err.contains("<State:before>"), "{bad}: {err}");
        }
        let got = parse("j<Shot:after-j>k").unwrap();
        assert_eq!(got[1], Step::Shot("after-j".into()));
        assert_eq!(press(&got[1]), Some(Press::Shot("after-j".into())));
        assert_eq!(parse("<Shot:p@preview>").unwrap(), [Step::Shot("p@preview".into())]);
        assert!(parse("<Shot:@preview>").is_err() && parse("<Shot:a@b>").is_err());
        for bad in ["<Shot:>", "<Shot:../x>", "<Shot:a b>", "<Shot:a/b>"] {
            let err = parse(bad).unwrap_err();
            assert!(err.contains("<Shot:before>"), "{bad}: {err}");
        }
    }

    /// Q41: `<Now>` marks the key after it, and only a key can follow it.
    #[test]
    fn now_marks_the_next_key() {
        let got = parse("d<Now>w").unwrap();
        assert_eq!(got[1], Step::Now);
        assert!(matches!(got[2], Step::Key(_)));
        assert_eq!(press(&got[1]), Some(Press::Now));
        for bad in ["d<Now>", "<Now><Wait:100>x", "<Now><Now>x"] {
            let err = parse(bad).unwrap_err();
            assert!(err.contains("<Now>"), "{bad}: {err}");
        }
    }

    /// Named keys, chords and plain characters, in one run of text.
    #[test]
    fn a_script_splits_into_its_keys() {
        let got = parse("<Tab>C<C-S-t>gg").unwrap();
        let want: Vec<Step> =
            ["<Tab>", "C", "<C-S-t>", "g", "g"].iter().map(|t| Step::Key(Key::parse(t).unwrap())).collect();
        assert_eq!(got, want);
        assert!(parse("<Tab").is_err(), "an unclosed key is refused, not guessed at");
        assert!(parse("<Nonsense>").is_err());
        assert_eq!(parse("").unwrap(), Vec::new());
        // #110: a space is written `<Space>`; a plain one is refused, naming where.
        let err = parse("<C-t>echo hi<Enter>").unwrap_err();
        assert!(err.contains("<Space>") && err.contains("hi<Enter>"), "{err}");
        assert_eq!(parse("a<Space>b").unwrap().len(), 3);
    }

    /// `<Wait:N>` is a pause of N milliseconds between the keys either side.
    #[test]
    fn a_wait_is_a_pause_in_milliseconds() {
        let got = parse("<C-t><Wait:1500>x").unwrap();
        assert_eq!(got[1], Step::Wait(Duration::from_millis(1500)));
        assert_eq!(press(&got[1]), Some(Press::Wait(Duration::from_millis(1500))));
        assert!(matches!(press(&got[2]), Some(Press::Events(_))));
        for bad in ["<Wait:>", "<Wait:1.5s>", "<Wait:-1>", "<Wait:99999999>"] {
            let err = parse(bad).unwrap_err();
            assert!(err.contains("<Wait:500>"), "{bad}: {err}");
        }
    }

    /// Every key produces events that `from_egui` reads back as the same key --
    /// the round trip a real keyboard makes -- and a plain character is text.
    #[test]
    fn each_key_becomes_the_events_that_read_back_as_it() {
        for token in ["<Tab>", "<Enter>", "<Esc>", "<C-S-t>", "<A-d>", "<C-f>", "<F12>", "<Up>", "<S-Tab>", "<C-+>"] {
            let key = Key::parse(token).unwrap();
            let evs = events(&key).unwrap_or_else(|| panic!("{token} has no events"));
            match &evs[0] {
                egui::Event::Key { key: k, pressed: true, modifiers, .. } => {
                    assert_eq!(keys::from_egui(*k, modifiers).as_ref(), Some(&key), "{token}");
                }
                other => panic!("{token} should be a key press, got {other:?}"),
            }
        }
        let c = Key::parse("C").unwrap();
        assert_eq!(events(&c), Some(vec![egui::Event::Text("C".into())]));
    }

    /// The events reach the app through the door a keyboard uses: `<Tab>` opens
    /// the spot panel and `q` closes it, in a real frame loop.
    #[test]
    fn scripted_keys_drive_the_real_frame_loop() {
        let dir = crate::util::test_dir("keyscript");
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        let mut s = crate::ui::harness::Screen::open(dir);
        s.settle();
        let key = |t| Key::parse(t).unwrap();
        s.feed(events(&key("<Tab>")).unwrap());
        assert!(matches!(s.app.overlay, crate::app::Overlay::Spot(_)), "`<Tab>` opened spot");
        s.feed(events(&key("q")).unwrap());
        assert!(s.app.overlay.is_none(), "`q` closed it");
    }

    /// #260: `<C-v>` and `<Paste>` paste into a prompt, as the platform's
    /// paste event, with the clipboard as it is when they are pressed.
    #[test]
    fn ctrl_v_pastes_into_a_prompt() {
        let dir = crate::util::test_dir("keyscript-paste");
        let mut s = crate::ui::harness::Screen::open(dir);
        s.settle();
        let key = |t| Key::parse(t).unwrap();
        assert_eq!(press(&Step::Key(key("<C-v>"))), Some(Press::Paste));
        assert_eq!(parse("<Paste>").unwrap(), [Step::Paste]);
        crate::exec::fake_clipboard("pasted-name");
        s.feed(events(&key("g")).unwrap());
        s.feed(events(&key("<Space>")).unwrap());
        s.feed(vec![paste_event()]);
        match &s.app.overlay {
            crate::app::Overlay::Input(ov) => assert!(ov.text.contains("pasted-name"), "{:?}", ov.text),
            _ => panic!("the cd prompt is not open"),
        }
    }

    /// A click holds `C-` / `A-` / `S-` like a wheel does, and `<Hover:>` only moves the pointer.
    #[test]
    fn clicks_hold_keys_and_hover_moves() {
        let got = parse("<Click:C-S-0.5,0.5><Hover:0.1,0.2>").unwrap();
        assert_eq!(got[0], Step::Click { right: false, mods: MOD_CTRL | MOD_SHIFT, at: (500, 500) });
        assert_eq!(got[1], Step::Hover { at: (100, 200) });
        assert_eq!(got.iter().map(label).collect::<Vec<_>>(), ["<Click:C-S-0.5,0.5>", "<Hover:0.1,0.2>"]);
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
        let ev = pointer_events(&press(&got[0]).unwrap(), rect);
        assert!(matches!(ev[1], egui::Event::PointerButton { pressed: true, modifiers, .. } if modifiers.ctrl && modifiers.shift && !modifiers.alt), "{ev:?}");
        let ev = pointer_events(&press(&got[1]).unwrap(), rect);
        assert_eq!(ev, [egui::Event::PointerMoved(egui::pos2(10.0, 20.0))]);
    }

    /// #260: the pointer steps parse, keep their place, and become the events
    /// a mouse makes inside the window they are given.
    #[test]
    fn pointer_steps_are_places_in_the_window() {
        let got = parse("<Click:0.5,0.25><RClick:1,0><Wheel:-3@0.1,0.9>").unwrap();
        assert_eq!(got[0], Step::Click { right: false, mods: 0, at: (500, 250) });
        assert_eq!(got[1], Step::Click { right: true, mods: 0, at: (1000, 0) });
        assert_eq!(got[2], Step::Wheel { milli: -3000, mods: 0, at: (100, 900) });
        assert_eq!(got.iter().map(label).collect::<Vec<_>>(), ["<Click:0.5,0.25>", "<RClick:1,0>", "<Wheel:-3@0.1,0.9>"]);
        for bad in ["<Click:2,0.5>", "<Click:0.5>", "<Wheel:3>", "<Wheel:x@0.5,0.5>", "<RClick:a,b>"] {
            assert!(parse(bad).unwrap_err().contains("pointer step"), "{bad}");
        }
        let more = parse("<Wheel:C-0.25@0.5,0.5><Wheel:A-S--1@0,0><Drag:0.25,0.5-0.75,0.5>").unwrap();
        assert_eq!(more[0], Step::Wheel { milli: 250, mods: MOD_CTRL, at: (500, 500) });
        assert_eq!(more[1], Step::Wheel { milli: -1000, mods: MOD_ALT | MOD_SHIFT, at: (0, 0) });
        assert_eq!(more[2], Step::Drag { from: (250, 500), to: (750, 500) });
        assert_eq!(more.iter().map(label).collect::<Vec<_>>(), ["<Wheel:C-0.25@0.5,0.5>", "<Wheel:A-S--1@0,0>", "<Drag:0.25,0.5-0.75,0.5>"]);
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let drag = pointer_events(&press(&more[2]).unwrap(), rect);
        assert!(matches!(drag[1], egui::Event::PointerButton { pos, pressed: true, .. } if pos == egui::pos2(200.0, 300.0)), "{drag:?}");
        assert!(matches!(drag[4], egui::Event::PointerButton { pos, pressed: false, .. } if pos == egui::pos2(600.0, 300.0)), "{drag:?}");
        let held = pointer_events(&press(&more[0]).unwrap(), rect);
        assert!(matches!(held[1], egui::Event::MouseWheel { delta, modifiers, .. } if delta == egui::vec2(0.0, 0.25) && modifiers.ctrl), "{held:?}");
        let click = pointer_events(&press(&got[0]).unwrap(), rect);
        assert!(matches!(click[1], egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, .. } if pos == egui::pos2(400.0, 150.0)), "{click:?}");
        let wheel = pointer_events(&press(&got[2]).unwrap(), rect);
        assert!(matches!(wheel[1], egui::Event::MouseWheel { delta, .. } if delta == egui::vec2(0.0, -3.0)), "{wheel:?}");
    }

    /// #259, #173: a panic under a script is written down where the script's
    /// reader looks, with the thread, the place and the message.
    #[test]
    fn a_panic_is_reported_for_the_script_that_was_running() {
        use std::sync::{Arc, Mutex};
        let caught = Arc::new(Mutex::new(String::new()));
        let seen = caught.clone();
        let before = std::panic::take_hook();
        // Only this test's own thread: a panic elsewhere while the hook is
        // swapped must not be taken for the one under test.
        std::panic::set_hook(Box::new(move |info| {
            if std::thread::current().name() == Some("worker-x") {
                *seen.lock().unwrap() = panic_report(info);
            }
        }));
        let _ = std::thread::Builder::new().name("worker-x".into()).spawn(|| panic!("boom {}", 7)).unwrap().join();
        std::panic::set_hook(before);
        let report = caught.lock().unwrap().clone();
        // `Location::file()` is `src\keyscript.rs` on Windows.
        assert!(report.replace('\\', "/").starts_with("keys: panicked\nthread: worker-x\nat: src/keyscript.rs:"), "{report}");
        assert!(report.ends_with("why: boom 7\n"), "{report}");
    }

    /// #216: `<C-+>*3` is the key three times; a bare `*` is still a key.
    #[test]
    fn a_count_after_a_key_repeats_it() {
        let key = |t| Step::Key(Key::parse(t).unwrap());
        assert_eq!(parse("<C-+>*3x").unwrap(), [key("<C-+>"), key("<C-+>"), key("<C-+>"), key("x")]);
        assert_eq!(parse("j*").unwrap(), [key("j"), key("*")], "a bare star is a key");
        assert_eq!(parse("<Up>*1").unwrap(), [key("<Up>")]);
        for bad in ["<Up>*", "<Up>*0", "<Up>*1001", "<Up>*x"] {
            assert!(parse(bad).unwrap_err().contains("count from 1 to 1000"), "{bad}");
        }
    }

    /// #229: `<PaneText:name>` parses and keeps its name.
    #[test]
    fn pane_text_is_a_named_step() {
        assert_eq!(parse("<PaneText:footer>").unwrap(), [Step::PaneText("footer".into())]);
        assert_eq!(label(&Step::PaneText("footer".into())), "<PaneText:footer>");
        assert_eq!(press(&Step::PaneText("a".into())), Some(Press::PaneText("a".into())));
        assert!(parse("<PaneText:>").unwrap_err().contains("not a pane text"));
        assert!(parse("<PaneText:../x>").is_err());
    }

    /// #268: `<Now>` may come before a reading, to take it with a job still
    /// running; before a wait it still means nothing.
    #[test]
    fn now_may_come_before_a_reading() {
        assert!(parse("<A-c><Now><State:mid>").is_ok());
        assert!(parse("<Now><Shot:a>").is_ok());
        assert!(parse("<Now><Wait:5>x").is_err());
        assert!(parse("<Now>").is_err());
    }
}
