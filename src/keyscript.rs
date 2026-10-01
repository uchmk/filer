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
}

/// What a step becomes in the frame loop.
#[derive(Clone, Debug, PartialEq)]
pub enum Press {
    Events(Vec<egui::Event>),
    Wait(Duration),
    Now,
}

/// A step as the frame loop takes it; `None` for a key no keyboard can type.
pub fn press(step: &Step) -> Option<Press> {
    match step {
        Step::Key(k) => events(k).map(Press::Events),
        Step::Wait(d) => Some(Press::Wait(*d)),
        Step::Now => Some(Press::Now),
    }
}

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
/// `<Now>` presses the next key without waiting for the last one to settle.
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
            None => Step::Key(Key::parse(token).ok_or_else(|| format!("`{token}` is not a key"))?),
        };
        out.push(step);
        rest = &rest[token.len()..];
    }
    // `<Now>` is about the key after it, so there has to be one.
    for (i, step) in out.iter().enumerate() {
        if *step == Step::Now && !matches!(out.get(i + 1), Some(Step::Key(_))) {
            return Err("`<Now>` has to come right before a key, as `d<Now>w`".into());
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
    use super::*;

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
}
