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

use crate::config::keys::{self, Code, Key};

/// `<Tab>C<C-S-t>gg` as the keys it names, in yazi's notation: `<…>` is one
/// key, anything else is one key per character.
pub fn parse(script: &str) -> Result<Vec<Key>, String> {
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
        let key = Key::parse(token).ok_or_else(|| format!("`{token}` is not a key"))?;
        out.push(key);
        rest = &rest[token.len()..];
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

    /// Named keys, chords and plain characters, in one run of text.
    #[test]
    fn a_script_splits_into_its_keys() {
        let got = parse("<Tab>C<C-S-t>gg").unwrap();
        let want: Vec<Key> = ["<Tab>", "C", "<C-S-t>", "g", "g"].iter().map(|t| Key::parse(t).unwrap()).collect();
        assert_eq!(got, want);
        assert!(parse("<Tab").is_err(), "an unclosed key is refused, not guessed at");
        assert!(parse("<Nonsense>").is_err());
        assert_eq!(parse("").unwrap(), Vec::new());
        // #110: a space is written `<Space>`; a plain one is refused, naming where.
        let err = parse("<C-t>echo hi<Enter>").unwrap_err();
        assert!(err.contains("<Space>") && err.contains("hi<Enter>"), "{err}");
        assert_eq!(parse("a<Space>b").unwrap().len(), 3);
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
        let keys = parse("<Tab>").unwrap();
        s.feed(events(&keys[0]).unwrap());
        assert!(matches!(s.app.overlay, crate::app::Overlay::Spot(_)), "`<Tab>` opened spot");
        let keys = parse("q").unwrap();
        s.feed(events(&keys[0]).unwrap());
        assert!(s.app.overlay.is_none(), "`q` closed it");
    }
}
