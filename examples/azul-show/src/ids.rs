//! Every id AzShow puts on a node, with the app's prefix (`__azshow_`, as
//! the widgets carry `__azul_`; user ruling 2026-10-02), each spelled ONCE
//! here. Scripts find the nodes by them (`#__azshow_canvas`).

use azul::str::String as AzString;

/// The slide on the canvas (File > Export takes its picture).
pub const SLIDE: AzString = AzString::from_const_str("__azshow_slide");
/// The canvas's scroll box.
pub const CANVAS: AzString = AzString::from_const_str("__azshow_canvas");
/// The notes field.
pub const NOTES: AzString = AzString::from_const_str("__azshow_notes");

/// The prefix of a text box's ids: `__azshow_tb<element>` is its host (the
/// contenteditable root), `__azshow_tb<element>-<paragraph>` a paragraph.
pub const TEXT_PREFIX: &str = "__azshow_tb";
