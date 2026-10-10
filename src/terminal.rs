//! The terminal pane's logic: the `ito-pane` crate, shared with tsumugi
//! (github.com/uchmk/ito, `crates/ito-pane`). It moved out of kura in
//! v0.78.125 (into tsumugi, then ito in v0.93.10); a fix to the pane goes into
//! that crate, and this file only keeps the old path `crate::terminal::…` working.

pub use ito_pane::*;
