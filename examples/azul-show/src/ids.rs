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

/// The prefix of an element's box in the show and in a preview:
/// `__azshow_el<element>`. A stable identity per object, so a rebuild per
/// frame of a transition or a build matches every box to itself (and never
/// one slide's title to the next slide's).
pub const ELEMENT_PREFIX: &str = "__azshow_el";

/// The show's stage: the slide's box on the black screen.
pub const STAGE: AzString = AzString::from_const_str("__azshow_stage");
/// The layer of the slide going out during a transition.
pub const LAYER_FROM: AzString = AzString::from_const_str("__azshow_layer_from");
/// The layer of the slide on screen (the one coming in during a transition).
pub const LAYER_TO: AzString = AzString::from_const_str("__azshow_layer_to");

/// The class of a shape's SVG picture (a triangle, an arrow ...).
pub const SHAPE_SVG_CLASS: &str = "__azshow_shape_svg";
