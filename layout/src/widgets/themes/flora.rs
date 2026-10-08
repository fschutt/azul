use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    callbacks::{CoreCallbackData, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{
        Dom, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec, NodeType,
        TabIndex,
    },
    geom::{LogicalPosition, LogicalRect},
    refany::RefAny,
};
use azul_css::{css::BoxOrStatic, AzString};
#[allow(clippy::wildcard_imports)]
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

use crate::widgets::button::{Button, ButtonOnClick};

// Colors extracted from flora.css
// Light mode colors
pub const LIGHT_PG: ColorU = ColorU {
    r: 230,
    g: 228,
    b: 223,
    a: 255,
};
pub const LIGHT_SUR: ColorU = ColorU {
    r: 242,
    g: 241,
    b: 237,
    a: 255,
};
pub const LIGHT_DESK: ColorU = ColorU {
    r: 221,
    g: 219,
    b: 213,
    a: 255,
};
pub const LIGHT_STRIP: ColorU = ColorU {
    r: 233,
    g: 231,
    b: 226,
    a: 255,
};
pub const LIGHT_TRACK: ColorU = ColorU {
    r: 231,
    g: 229,
    b: 224,
    a: 255,
};
pub const LIGHT_BD: ColorU = ColorU {
    r: 198,
    g: 195,
    b: 187,
    a: 255,
};
pub const LIGHT_BD2: ColorU = ColorU {
    r: 180,
    g: 177,
    b: 169,
    a: 255,
};
pub const LIGHT_BD3: ColorU = ColorU {
    r: 156,
    g: 152,
    b: 144,
    a: 255,
};
pub const LIGHT_BD4: ColorU = ColorU {
    r: 210,
    g: 207,
    b: 200,
    a: 255,
};
pub const LIGHT_BD5: ColorU = ColorU {
    r: 165,
    g: 161,
    b: 153,
    a: 255,
};
pub const LIGHT_SEP: ColorU = ColorU {
    r: 216,
    g: 213,
    b: 206,
    a: 255,
};
pub const LIGHT_SEP2: ColorU = ColorU {
    r: 224,
    g: 221,
    b: 215,
    a: 255,
};
pub const LIGHT_INK: ColorU = ColorU {
    r: 38,
    g: 37,
    b: 33,
    a: 255,
};
pub const LIGHT_INK2: ColorU = ColorU {
    r: 46,
    g: 44,
    b: 38,
    a: 255,
};
pub const LIGHT_INTRO: ColorU = ColorU {
    r: 78,
    g: 76,
    b: 69,
    a: 255,
};
pub const LIGHT_SOFT1: ColorU = ColorU {
    r: 102,
    g: 100,
    b: 92,
    a: 255,
};
pub const LIGHT_SOFT2: ColorU = ColorU {
    r: 130,
    g: 127,
    b: 118,
    a: 255,
};
pub const LIGHT_SOFT3: ColorU = ColorU {
    r: 116,
    g: 113,
    b: 106,
    a: 255,
};
pub const LIGHT_ICON: ColorU = ColorU {
    r: 86,
    g: 84,
    b: 76,
    a: 255,
};
pub const LIGHT_RT: ColorU = ColorU {
    r: 250,
    g: 249,
    b: 245,
    a: 255,
};
pub const LIGHT_RB: ColorU = ColorU {
    r: 236,
    g: 234,
    b: 228,
    a: 255,
};
pub const LIGHT_HT: ColorU = ColorU {
    r: 254,
    g: 253,
    b: 250,
    a: 255,
};
pub const LIGHT_HB: ColorU = ColorU {
    r: 241,
    g: 239,
    b: 233,
    a: 255,
};
pub const LIGHT_PT: ColorU = ColorU {
    r: 228,
    g: 225,
    b: 218,
    a: 255,
};
pub const LIGHT_PB: ColorU = ColorU {
    r: 238,
    g: 236,
    b: 230,
    a: 255,
};
pub const LIGHT_FLD: ColorU = ColorU {
    r: 251,
    g: 250,
    b: 246,
    a: 255,
};
pub const LIGHT_FLD2: ColorU = ColorU {
    r: 239,
    g: 237,
    b: 231,
    a: 255,
};
pub const LIGHT_DISBG: ColorU = ColorU {
    r: 235,
    g: 233,
    b: 227,
    a: 255,
};
pub const LIGHT_DISTX: ColorU = ColorU {
    r: 163,
    g: 160,
    b: 153,
    a: 255,
};
pub const LIGHT_QT: ColorU = ColorU {
    r: 110,
    g: 99,
    b: 73,
    a: 255,
};
pub const LIGHT_QT2: ColorU = ColorU {
    r: 79,
    g: 70,
    b: 51,
    a: 255,
};
pub const LIGHT_ACC: ColorU = ColorU {
    r: 47,
    g: 74,
    b: 133,
    a: 255,
};
pub const LIGHT_DEEP: ColorU = ColorU {
    r: 30,
    g: 50,
    b: 96,
    a: 255,
};
pub const LIGHT_SOFT: ColorU = ColorU {
    r: 224,
    g: 228,
    b: 238,
    a: 255,
};
pub const LIGHT_GLOW: ColorU = ColorU {
    r: 122,
    g: 147,
    b: 198,
    a: 255,
};
pub const LIGHT_ON_ACC: ColorU = ColorU {
    r: 244,
    g: 242,
    b: 234,
    a: 255,
};

// Dark mode colors
pub const DARK_PG: ColorU = ColorU {
    r: 26,
    g: 26,
    b: 26,
    a: 255,
};
pub const DARK_SUR: ColorU = ColorU {
    r: 35,
    g: 35,
    b: 35,
    a: 255,
};
pub const DARK_DESK: ColorU = ColorU {
    r: 18,
    g: 18,
    b: 18,
    a: 255,
};
pub const DARK_STRIP: ColorU = ColorU {
    r: 31,
    g: 31,
    b: 31,
    a: 255,
};
pub const DARK_TRACK: ColorU = ColorU {
    r: 21,
    g: 21,
    b: 21,
    a: 255,
};
pub const DARK_BD: ColorU = ColorU {
    r: 63,
    g: 63,
    b: 63,
    a: 255,
};
pub const DARK_BD2: ColorU = ColorU {
    r: 74,
    g: 74,
    b: 74,
    a: 255,
};
pub const DARK_BD3: ColorU = ColorU {
    r: 97,
    g: 97,
    b: 97,
    a: 255,
};
pub const DARK_BD4: ColorU = ColorU {
    r: 52,
    g: 52,
    b: 52,
    a: 255,
};
pub const DARK_BD5: ColorU = ColorU {
    r: 16,
    g: 16,
    b: 16,
    a: 255,
};
pub const DARK_SEP: ColorU = ColorU {
    r: 56,
    g: 56,
    b: 56,
    a: 255,
};
pub const DARK_SEP2: ColorU = ColorU {
    r: 46,
    g: 46,
    b: 46,
    a: 255,
};
pub const DARK_INK: ColorU = ColorU {
    r: 231,
    g: 231,
    b: 231,
    a: 255,
};
pub const DARK_INK2: ColorU = ColorU {
    r: 220,
    g: 220,
    b: 220,
    a: 255,
};
pub const DARK_INTRO: ColorU = ColorU {
    r: 188,
    g: 188,
    b: 188,
    a: 255,
};
pub const DARK_SOFT1: ColorU = ColorU {
    r: 168,
    g: 168,
    b: 168,
    a: 255,
};
pub const DARK_SOFT2: ColorU = ColorU {
    r: 140,
    g: 140,
    b: 140,
    a: 255,
};
pub const DARK_SOFT3: ColorU = ColorU {
    r: 154,
    g: 154,
    b: 154,
    a: 255,
};
pub const DARK_ICON: ColorU = ColorU {
    r: 190,
    g: 190,
    b: 190,
    a: 255,
};
pub const DARK_RT: ColorU = ColorU {
    r: 51,
    g: 51,
    b: 51,
    a: 255,
};
pub const DARK_RB: ColorU = ColorU {
    r: 41,
    g: 41,
    b: 41,
    a: 255,
};
pub const DARK_HT: ColorU = ColorU {
    r: 63,
    g: 63,
    b: 63,
    a: 255,
};
pub const DARK_HB: ColorU = ColorU {
    r: 51,
    g: 51,
    b: 51,
    a: 255,
};
pub const DARK_PT: ColorU = ColorU {
    r: 31,
    g: 31,
    b: 31,
    a: 255,
};
pub const DARK_PB: ColorU = ColorU {
    r: 38,
    g: 38,
    b: 38,
    a: 255,
};
pub const DARK_FLD: ColorU = ColorU {
    r: 29,
    g: 29,
    b: 29,
    a: 255,
};
pub const DARK_FLD2: ColorU = ColorU {
    r: 36,
    g: 36,
    b: 36,
    a: 255,
};
pub const DARK_DISBG: ColorU = ColorU {
    r: 36,
    g: 36,
    b: 36,
    a: 255,
};
pub const DARK_DISTX: ColorU = ColorU {
    r: 102,
    g: 102,
    b: 102,
    a: 255,
};
pub const DARK_QT: ColorU = ColorU {
    r: 196,
    g: 181,
    b: 142,
    a: 255,
};
pub const DARK_QT2: ColorU = ColorU {
    r: 222,
    g: 211,
    b: 180,
    a: 255,
};
pub const DARK_ACC: ColorU = ColorU {
    r: 47,
    g: 74,
    b: 133,
    a: 255,
};
pub const DARK_DEEP: ColorU = ColorU {
    r: 30,
    g: 50,
    b: 96,
    a: 255,
};
pub const DARK_SOFT: ColorU = ColorU {
    r: 224,
    g: 228,
    b: 238,
    a: 255,
};
pub const DARK_GLOW: ColorU = ColorU {
    r: 122,
    g: 147,
    b: 198,
    a: 255,
};
pub const DARK_ON_ACC: ColorU = ColorU {
    r: 244,
    g: 242,
    b: 234,
    a: 255,
};

// ---------------------------------------------------------------------------
// GRADIENTS
// ---------------------------------------------------------------------------
//
// flora.css's `linear-gradient(..)` declarations, transcribed with the angles
// and stops the CSS has; an `rgba(..)` alpha is rounded the way the CSS parser
// rounds it, `(a * 255).round()`. Everything here is `const`: `LinearGradient`
// and its stop vec have const constructors, so a gradient can sit in a `const`
// style slice as well as in a runtime `vec!`. The stop slices are named consts
// rather than inline borrows because a gradient value has drop glue (its stop
// vec implements `Drop`), and a borrowed temporary of such a value is not
// promotable — the shape `tabs.rs` uses for the same reason.
//
// The `RT`/`RB`, `HT`/`HB` and `PT`/`PB` tokens above are the two stops of the
// raised, hovered and pressed control faces — that is what they exist for —
// and the six `*_FACE_*` gradients are those pairs. `flat.rs` keeps flat fills:
// its stop pairs are equal values, which is what lets the same widget code read
// correctly under both themes.
//
// Layer order: a `StyleBackgroundContentVec` is painted first-to-last, so a base
// colour goes FIRST and a translucent overlay after it — the reverse of a CSS
// comma list, whose first layer is on top.
//
// Not transcribed, and why:
// * `--fl-fibre`: the one-direction raster (`--fl-grain`, both directions, is the flora ground -
//   `linen_ground`, now that a stop can sit at a length).
// * The `mask-image` gradients (not backgrounds) and the `.docs-card::after` sheen (a keyframe
//   animation).
// * `--fl-band`, `--fl-rolled`, `--fl-gem-sunken` and the scrollbar thumb: they belong with the
//   widget that gets one, not as consts nothing declares. The tab metal - `--fl-rule-metal-bg`,
//   `--fl-rolled-tab` (its `calc()` stop a percentage plus a length) and the `.fl-tab-runout`
//   pieces - lives with the tabs (`RULE_METAL`, `ROLLED_TAB`, `RUNOUT_LEFT` / `RUNOUT_RIGHT`).

/// The CSS default direction: `linear-gradient(a, b)` with no angle runs top to
/// bottom.
const TO_BOTTOM: Direction = Direction::FromTo(DirectionCorners {
    dir_from: DirectionCorner::Top,
    dir_to: DirectionCorner::Bottom,
});

/// `<n>deg`, as the CSS writes an explicit angle.
const fn deg(degrees: isize) -> Direction {
    Direction::Angle(AngleValue::const_deg(degrees))
}

/// A colour stop at `offset` percent.
const fn stop(offset: isize, color: ColorU) -> NormalizedLinearColorStop {
    NormalizedLinearColorStop::new(PercentageValue::const_new(offset), color)
}

/// A `background` of these layers, painted first to last.
fn layers(list: Vec<StyleBackgroundContent>) -> CssProperty {
    CssProperty::BackgroundContent(StyleBackgroundContentVec::from_vec(list).into())
}

// -- the raised face --------------------------------------------------------

const RAISED_FACE_LIGHT_STOPS: &[NormalizedLinearColorStop] =
    &[stop(0, LIGHT_RT), stop(100, LIGHT_RB)];

/// `.btn-secondary`: raised paper, the standard command at rest.
///
/// `linear-gradient(var(--fl-rT), var(--fl-rB))` — [`LIGHT_RT`] over
/// [`LIGHT_RB`]. Also `.navbar`, `.btn-hero-secondary`, `.pill`, and the
/// `padding-box` layer of `.btn-hero-primary`, `.hero-badge` and `.pill-soon`.
pub const RAISED_FACE_LIGHT: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: TO_BOTTOM,
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(RAISED_FACE_LIGHT_STOPS),
    });

const RAISED_FACE_DARK_STOPS: &[NormalizedLinearColorStop] =
    &[stop(0, DARK_RT), stop(100, DARK_RB)];

/// [`RAISED_FACE_LIGHT`] under `:root[data-theme="dark"]`, where the tokens
/// take their dark values. [`DARK_RT`] over [`DARK_RB`].
pub const RAISED_FACE_DARK: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: TO_BOTTOM,
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(RAISED_FACE_DARK_STOPS),
    });


// -- the hovered face -------------------------------------------------------

const HOVER_FACE_LIGHT_STOPS: &[NormalizedLinearColorStop] =
    &[stop(0, LIGHT_HT), stop(100, LIGHT_HB)];

/// `.btn-secondary:hover`: the raised face lifted toward the light.
///
/// `linear-gradient(var(--fl-hT), var(--fl-hB))` — [`LIGHT_HT`] over
/// [`LIGHT_HB`]. Also `.nav-links a:hover`, `.btn-hero-secondary:hover` and
/// `.mobile-menu a:focus`.
pub const HOVER_FACE_LIGHT: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: TO_BOTTOM,
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(HOVER_FACE_LIGHT_STOPS),
    });

const HOVER_FACE_DARK_STOPS: &[NormalizedLinearColorStop] = &[stop(0, DARK_HT), stop(100, DARK_HB)];

/// [`HOVER_FACE_LIGHT`] in dark mode. [`DARK_HT`] over [`DARK_HB`].
pub const HOVER_FACE_DARK: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: TO_BOTTOM,
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(HOVER_FACE_DARK_STOPS),
    });

// -- the pressed face -------------------------------------------------------

const PRESSED_FACE_LIGHT_STOPS: &[NormalizedLinearColorStop] =
    &[stop(0, LIGHT_PT), stop(100, LIGHT_PB)];

/// `.btn-secondary:active`: the face pushed in.
///
/// `linear-gradient(var(--fl-pT), var(--fl-pB))` — [`LIGHT_PT`] over
/// [`LIGHT_PB`]: darker at the top, where the near lip shades it. Also
/// `.nav-links a:active`.
pub const PRESSED_FACE_LIGHT: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: TO_BOTTOM,
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(PRESSED_FACE_LIGHT_STOPS),
    });

const PRESSED_FACE_DARK_STOPS: &[NormalizedLinearColorStop] =
    &[stop(0, DARK_PT), stop(100, DARK_PB)];

/// [`PRESSED_FACE_LIGHT`] in dark mode. [`DARK_PT`] over [`DARK_PB`].
pub const PRESSED_FACE_DARK: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: TO_BOTTOM,
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(PRESSED_FACE_DARK_STOPS),
    });

// -- the stone: a coloured command's depth -----------------------------------
//
// flora.css draws its accent command as a stone and lays a "depth rig" over it
// in two pseudo-elements: `::after` shades the edges the light does not reach
// and `::before` rakes a specular streak across the face. Their fills are
// translucent, so they go OVER the command's own colour and work for any
// colour — which is how a Success or Danger button, a stone in its own colour,
// gets the same depth as the Primary one. The pseudo-elements' `opacity`
// (0.85 at rest, 1 on hover) is not a background property and is not
// reproduced; the stops are the CSS's own.

const STONE_RIG_TOP_STOPS: &[NormalizedLinearColorStop] = &[
    stop(0, ColorU::new(12, 10, 4, 87)),
    stop(32, ColorU::new(12, 10, 4, 0)),
];

/// `.btn-primary::after`, second layer: the shadow the top edge throws down
/// the face.
///
/// `linear-gradient(180deg, rgba(12, 10, 4, 0.34) 0%, rgba(12, 10, 4, 0) 32%)`.
pub const STONE_RIG_TOP: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: deg(180),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(STONE_RIG_TOP_STOPS),
    });

const STONE_RIG_LEFT_STOPS: &[NormalizedLinearColorStop] = &[
    stop(0, ColorU::new(12, 10, 4, 56)),
    stop(15, ColorU::new(12, 10, 4, 0)),
];

/// `.btn-primary::after`, third layer: the same shadow, off the left edge.
///
/// `linear-gradient(90deg, rgba(12, 10, 4, 0.22) 0%, rgba(12, 10, 4, 0) 15%)`.
pub const STONE_RIG_LEFT: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: deg(90),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(STONE_RIG_LEFT_STOPS),
    });

const STONE_STREAK_STOPS: &[NormalizedLinearColorStop] = &[
    stop(6, ColorU::new(255, 248, 215, 0)),
    stop(15, ColorU::new(255, 248, 215, 51)),
    stop(22, ColorU::new(255, 248, 215, 15)),
    stop(32, ColorU::new(255, 248, 215, 0)),
];

/// `.btn-primary::before`: the specular streak raked across the face at rest.
///
/// `linear-gradient(115deg, rgba(255, 248, 215, 0) 6%, rgba(255, 248, 215,
/// 0.20) 15%, rgba(255, 248, 215, 0.06) 22%, rgba(255, 248, 215, 0) 32%)`.
pub const STONE_STREAK: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: deg(115),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(STONE_STREAK_STOPS),
    });

const STONE_STREAK_HOVER_STOPS: &[NormalizedLinearColorStop] = &[
    stop(4, ColorU::new(255, 248, 215, 0)),
    stop(14, ColorU::new(255, 248, 215, 87)),
    stop(23, ColorU::new(255, 248, 215, 26)),
    stop(34, ColorU::new(255, 248, 215, 0)),
];

/// `.btn-primary:hover::before`: the streak, brighter and wider, as the face
/// turns toward the light.
///
/// `linear-gradient(115deg, rgba(255, 248, 215, 0) 4%, rgba(255, 248, 215,
/// 0.34) 14%, rgba(255, 248, 215, 0.10) 23%, rgba(255, 248, 215, 0) 34%)`.
pub const STONE_STREAK_HOVER: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: deg(115),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(STONE_STREAK_HOVER_STOPS),
    });

// The stone pressed. flora.css's sunken stone (`.nav-links a.active::after`,
// `.lang-grid button.active::after`) is "lit from below the near edge instead
// of above it: the light falls INTO the well, so the top edge darkens and the
// bottom lifts. That inversion is the whole difference between pressed and
// raised" — so it is what a coloured command shows while it is held down.

const SUNKEN_RIG_TOP_STOPS: &[NormalizedLinearColorStop] = &[
    stop(0, ColorU::new(8, 6, 2, 122)),
    stop(42, ColorU::new(8, 6, 2, 0)),
];

/// `.nav-links a.active::after`, second layer: the near lip's shadow, deeper
/// than the raised rig's.
///
/// `linear-gradient(180deg, rgba(8, 6, 2, 0.48) 0%, rgba(8, 6, 2, 0) 42%)`.
pub const SUNKEN_RIG_TOP: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: deg(180),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(SUNKEN_RIG_TOP_STOPS),
    });

const SUNKEN_RIG_LEFT_STOPS: &[NormalizedLinearColorStop] = &[
    stop(0, ColorU::new(8, 6, 2, 77)),
    stop(18, ColorU::new(8, 6, 2, 0)),
];

/// `.nav-links a.active::after`, third layer: the same shadow, off the left
/// edge.
///
/// `linear-gradient(90deg, rgba(8, 6, 2, 0.30) 0%, rgba(8, 6, 2, 0) 18%)`.
pub const SUNKEN_RIG_LEFT: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: deg(90),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(SUNKEN_RIG_LEFT_STOPS),
    });

const SUNKEN_RIG_BOTTOM_STOPS: &[NormalizedLinearColorStop] = &[
    stop(0, ColorU::new(255, 253, 238, 26)),
    stop(14, ColorU::new(255, 253, 238, 0)),
];

/// `.nav-links a.active::after`, fourth layer: the far wall's light lifting
/// the bottom edge.
///
/// `linear-gradient(0deg, rgba(255, 253, 238, 0.10) 0%, rgba(255, 253, 238, 0)
/// 14%)`.
pub const SUNKEN_RIG_BOTTOM: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: deg(0),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(SUNKEN_RIG_BOTTOM_STOPS),
    });

/// A coloured command is a stone: its own colour, then the rig
/// `.btn-primary::after` lays on it, then the streak of `::before`.
///
/// The colour is the base layer because layers paint first-to-last; the rig is
/// translucent, so the colour shows through it.
fn stone_face(color: ColorU, streak: StyleBackgroundContent) -> Vec<StyleBackgroundContent> {
    vec![
        StyleBackgroundContent::Color(color),
        STONE_RIG_TOP,
        STONE_RIG_LEFT,
        streak,
    ]
}

/// The stone held down: its pressed colour under the sunken rig.
fn sunken_stone_face(color: ColorU) -> Vec<StyleBackgroundContent> {
    vec![
        StyleBackgroundContent::Color(color),
        SUNKEN_RIG_TOP,
        SUNKEN_RIG_LEFT,
        SUNKEN_RIG_BOTTOM,
    ]
}

// -- the orb's gloss --------------------------------------------------------

const ORB_GLOSS_STOPS: &[NormalizedLinearColorStop] = &[
    stop(0, ColorU::new(255, 255, 255, 158)),
    stop(52, ColorU::new(255, 255, 255, 56)),
    stop(100, ColorU::new(255, 255, 255, 8)),
];

/// `.fl-orb-gloss`: "the specular cap. Hard along the top, dissolving at the
/// equator."
///
/// `linear-gradient(180deg, rgba(255, 255, 255, 0.62) 0%, rgba(255, 255, 255,
/// 0.22) 52%, rgba(255, 255, 255, 0.03) 100%)`. The CSS puts it on a separate
/// element covering the dome's upper half; here it spans the whole box it is
/// laid over, so it reads as a top-lit sheen on a domed knob.
pub const ORB_GLOSS: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: deg(180),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(ORB_GLOSS_STOPS),
    });

// ==== the house rig: motion, depth, rings, capitals, metal, gems (FLORA11) ====
//
// What flora.css and the Azlin design system (the "Interface Specimen", its
// widget set) build every control from, in one place, so the widgets below
// read as decisions. Where the two disagree flora.css wins (its neutral
// ground and ink; the specimen's parchment is not used).
//
// * MOTION - "one easing curve and three durations": `--fl-ease`, and
//   `--fl-dur-slow` (light travelling across a stone), `--fl-dur` (a state
//   change), `--fl-dur-fast` (a press, "the one fast movement").
// * DEPTH - a raised face has a lit lip (`inset 0 1px 0`), a shaded foot
//   (`inset 0 -2px 3px`) and casts `--fl-shadow-1`; pressed, it loses all
//   three to a well (`inset 0 1px 3px`). azul keeps four shadow slots per
//   node (`decl::ShadowSlot`): the lip in Top, the foot in Right, the cast
//   shadow in Bottom, Left free for a ring or a rim.
// * FOCUS - the keyboard ring is `outline: 2px solid var(--focus-color);
//   outline-offset: 2px` (a field's: offset 1px, its border in the accent).
//   azul draws no outline, so the ring is two spread shadows: the accent band
//   in Left UNDER the gap in Bottom - the leaf the control stands on - which
//   stands in for the cast shadow while the ring shows ([`double_ring`]).
// * CAPITALS - "every label set in capitals uses Garamond" (`--font-caps`,
//   `font-variant: all-small-caps`, tracked out). The bundled EB Garamond
//   (`text3::ui_fonts`) has no small capitals and azul's CSS no
//   `font-variant`, so a label is set in uppercase a size step down, bold,
//   tracked: the specimen's 13.5px small capitals are 11px capitals here
//   ([`caps`]).
// * METAL - brass lives on borders only. The leaf (`--fl-leaf-a/b`: two
//   radial passes clipped to the border box) runs ALONG the edge, under a
//   face on the padding box - a per-layer `background-clip`, which the tabs'
//   metal is cut with ([`over_metal`]); a leafed edge is still cut as four
//   brass tones, lit along the top and left where the light enters, falling
//   to the turn colour and its shade on the right and bottom ([`leaf_edge`]).
//   On hover "the metal edge comes up": a 1px gold rim and a gold bloom (`0 0
//   0 1px rgba(214,197,140,.55)`, `0 0 14px rgba(214,197,140,.32)`,
//   [`metal_comes_up`]).
// * STONES - the accent stone is `--fl-gem` (a radial cut lit at 30% 12%:
//   glow, stone, deep) under the rig flora.css lays on every stone (the bloom
//   off the upper-left corner, the shadow each lit edge casts, the far corner
//   falling away, the specular streak); pressed, it sinks to
//   `--fl-gem-sunken` under the sunken rig ([`raised_stone`],
//   [`sunken_stone`]). Every semantic stone (leaf, clay, amber, slate) is cut
//   the same way from its own colours, and the accent's is the theme's: a
//   spin (`flora:green`, ...) recuts it (`themes::spin`).

use super::decl::{no_shadow_in, shadow_in, ShadowSlot};

/// `--fl-dur-slow`: light travelling across a stone.
pub const FL_DUR_SLOW_MS: u32 = 1200;
/// `--fl-dur`: a state change.
pub const FL_DUR_MS: u32 = 420;
/// `--fl-dur-fast`: a press.
pub const FL_DUR_FAST_MS: u32 = 140;

/// `--fl-ease`: `cubic-bezier(0.25, 0.46, 0.45, 0.94)`, in permille.
pub const FL_EASE: azul_css::props::basic::animation::AnimationTiming =
    azul_css::props::basic::animation::AnimationTiming::CubicBezier(
        azul_css::props::basic::animation::AnimationTimingBezier {
            x1: 250,
            y1: 460,
            x2: 450,
            y2: 940,
        },
    );

/// What a flora control's face is made of - what its fade tweens: the fill,
/// the four border colours, the ink and the four shadow slots. Every one of
/// them passes through the values in between: a face tweens layer by layer
/// (or cross-fades into a face of another shape), a shadow tweens its
/// lengths and its colour, and comes or goes as a fade.
pub(crate) const FLORA_FACE: &[&str] = &[
    "background",
    "border-top-color",
    "border-right-color",
    "border-bottom-color",
    "border-left-color",
    "color",
    "-azul-box-shadow-left",
    "-azul-box-shadow-right",
    "-azul-box-shadow-top",
    "-azul-box-shadow-bottom",
];

/// The curve one property of a flora fade moves on.
type FadeCurve = azul_css::props::basic::animation::AnimationTiming;

/// What changes as light moves across a stone - or across the field of a
/// metal-edged command - and on which curve: flora.css's `.btn-primary` /
/// `.btn-hero-primary` transition, the face on `--fl-ease`, the edge and the
/// shadows (the lip, the gold rim, the bloom, the well) on `ease`. The ink is
/// not on the list (it is not on flora.css's either): a stone's ink is the
/// same in every state.
pub(crate) const LIT_FACE: &[(&str, FadeCurve)] = &[
    ("background", FL_EASE),
    ("border-top-color", FadeCurve::Ease),
    ("border-right-color", FadeCurve::Ease),
    ("border-bottom-color", FadeCurve::Ease),
    ("border-left-color", FadeCurve::Ease),
    ("-azul-box-shadow-left", FadeCurve::Ease),
    ("-azul-box-shadow-right", FadeCurve::Ease),
    ("-azul-box-shadow-top", FadeCurve::Ease),
    ("-azul-box-shadow-bottom", FadeCurve::Ease),
];

/// The fade a flora control declares - `decl::state_fade` on flora's curve:
/// `props` follow the pointer over `ms` on `--fl-ease`, and a press takes
/// `--fl-dur-fast` ("a face that was lit on top flips to lit on the bottom
/// in --fl-dur-fast, then eases back out over --fl-dur when released").
#[must_use]
pub(crate) fn flora_fade(props: &[&'static str], ms: u32) -> [CssPropertyWithConditions; 2] {
    let tweens: Vec<(&'static str, FadeCurve)> =
        props.iter().map(|name| (*name, FL_EASE)).collect();
    flora_fade_on_curves(&tweens, ms)
}

/// [`flora_fade`] with a curve of its own for each property (`(name,
/// curve)`, as [`LIT_FACE`]): over `ms` as the pointer comes and goes, over
/// `--fl-dur-fast` on the same curves while pressed - flora.css's
/// `.btn:active` changes the duration only.
#[must_use]
pub(crate) fn flora_fade_on_curves(
    tweens: &[(&'static str, FadeCurve)],
    ms: u32,
) -> [CssPropertyWithConditions; 2] {
    use azul_css::props::{
        basic::{
            animation::{AnimationIterationCount, StyleAnimation, StyleAnimationVec},
            time::CssDuration,
        },
        property::StyleAnimationVecValue,
    };
    let list = |duration: u32| {
        CssProperty::Animation(StyleAnimationVecValue::Exact(StyleAnimationVec::from_vec(
            tweens
                .iter()
                .map(|&(name, timing)| StyleAnimation {
                    name: AzString::from_const_str(name),
                    duration: CssDuration::from_millis(duration),
                    delay: CssDuration::from_millis(0),
                    iterations: AnimationIterationCount::Count(1),
                    timing,
                    clip: true,
                })
                .collect(),
        )))
    };
    [
        CssPropertyWithConditions::simple(list(ms)),
        CssPropertyWithConditions::on_active(list(FL_DUR_FAST_MS)),
    ]
}

/// White at `a`: a lit lip.
const fn white(a: u8) -> ColorU {
    ColorU::new(255, 255, 255, a)
}

/// One shadow in `slot` with its night twin right after it.
fn themed_shadow_in(
    slot: ShadowSlot,
    (offset_y, blur, spread): (isize, isize, isize),
    light: ColorU,
    dark: ColorU,
    inset: bool,
) -> [CssPropertyWithConditions; 2] {
    CssPropertyWithConditions::themed(
        shadow_in(slot, offset_y, blur, spread, light, inset),
        shadow_in(slot, offset_y, blur, spread, dark, inset),
    )
}

/// `--fl-lip` and `--fl-shadow-1`: a raised paper face's lit lip
/// (`inset 0 1px 0`), shaded foot (`inset 0 -2px 3px`) and cast shadow
/// (`0 1px 2px`), by day and at night.
#[must_use]
pub(crate) fn raised_depth() -> Vec<CssPropertyWithConditions> {
    let mut v = Vec::with_capacity(6);
    v.extend(themed_shadow_in(ShadowSlot::Top, (1, 0, 0), white(179), white(23), true));
    v.extend(themed_shadow_in(
        ShadowSlot::Right,
        (-2, 3, 0),
        ColorU::new(48, 45, 38, 26),
        ColorU::new(0, 0, 0, 102),
        true,
    ));
    v.extend(themed_shadow_in(
        ShadowSlot::Bottom,
        (1, 2, 0),
        ColorU::new(48, 45, 38, 36),
        ColorU::new(0, 0, 0, 140),
        false,
    ));
    v
}

/// A raised face pressed: the lip and the foot give way to a well
/// (`inset 0 1px 3px rgba(48,45,38,.18)`) and nothing is cast.
#[must_use]
pub(crate) fn pressed_depth() -> Vec<CssPropertyWithConditions> {
    let mut v = Vec::with_capacity(4);
    v.extend(CssPropertyWithConditions::themed_on_active(
        shadow_in(ShadowSlot::Top, 1, 3, 0, ColorU::new(48, 45, 38, 46), true),
        shadow_in(ShadowSlot::Top, 1, 3, 0, ColorU::new(0, 0, 0, 115), true),
    ));
    v.push(CssPropertyWithConditions::on_active(no_shadow_in(ShadowSlot::Right)));
    v.push(CssPropertyWithConditions::on_active(no_shadow_in(ShadowSlot::Bottom)));
    v
}

/// The keyboard ring: a 2px accent band `gap` px off the border - `--fl-acc`
/// by day, `--fl-glow` at night (flora.css's night `--focus-color`: the stone
/// itself stands 1.8:1 off the night leaf) - over a gap in the leaf's own
/// colour. 2 for a command (`outline-offset: 2px`), 1 for a field.
#[must_use]
pub(crate) fn double_ring(gap: isize) -> Vec<CssPropertyWithConditions> {
    let mut v = Vec::with_capacity(4);
    v.extend(CssPropertyWithConditions::themed_on_focus(
        shadow_in(ShadowSlot::Left, 0, 0, gap + 2, LIGHT_ACC, false),
        shadow_in(ShadowSlot::Left, 0, 0, gap + 2, DARK_GLOW, false),
    ));
    v.extend(CssPropertyWithConditions::themed_on_focus(
        shadow_in(ShadowSlot::Bottom, 0, 0, gap, LIGHT_SUR, false),
        shadow_in(ShadowSlot::Bottom, 0, 0, gap, DARK_SUR, false),
    ));
    v
}

/// `0 0 0 1px rgba(214, 197, 140, 0.55)`: the gold rim.
const GOLD_RIM: ColorU = ColorU::new(214, 197, 140, 140);
/// `0 0 14px rgba(214, 197, 140, 0.32)`: the gold bloom.
const GOLD_BLOOM: ColorU = ColorU::new(214, 197, 140, 82);

/// "The metal edge comes up as the face turns toward the light": on hover a
/// stone or a leafed command takes the gold rim (Left) and the gold bloom
/// (Bottom, in place of its cast shadow). The same by night - brass is the
/// one thing in the dark room still catching the light.
#[must_use]
pub(crate) fn metal_comes_up() -> Vec<CssPropertyWithConditions> {
    alloc::vec![
        CssPropertyWithConditions::on_hover(shadow_in(ShadowSlot::Left, 0, 0, 1, GOLD_RIM, false)),
        CssPropertyWithConditions::on_hover(shadow_in(
            ShadowSlot::Bottom,
            0,
            14,
            0,
            GOLD_BLOOM,
            false
        )),
    ]
}

/// A leafed edge by day: top, right, bottom, left - lit along the top and
/// left (`--fl-rolled`'s `#E4DCB8` falling to the turn colour), the turn
/// colour's shade on the right and the dark brass of `--fl-leaf-b` along the
/// bottom.
const LEAF_EDGE_LIGHT: [ColorU; 4] = [
    ColorU::rgb(0xD3, 0xC3, 0x8E),
    ColorU::rgb(0x8B, 0x80, 0x58),
    ColorU::rgb(0x7A, 0x70, 0x52),
    ColorU::rgb(0xB9, 0xA8, 0x74),
];

/// The leafed edge at night: "the brass warms up" (`--color-gold` #C4B58E).
const LEAF_EDGE_DARK: [ColorU; 4] = [
    ColorU::rgb(0xD6, 0xC6, 0x90),
    ColorU::rgb(0x9A, 0x8B, 0x5F),
    ColorU::rgb(0x8B, 0x7D, 0x55),
    ColorU::rgb(0xC4, 0xB5, 0x8E),
];

/// A border cut from the leaf (see the section note), each edge with its
/// night twin. Pair it with a 1px border.
#[must_use]
pub(crate) fn leaf_edge() -> Vec<CssPropertyWithConditions> {
    let [t, r, b, l] = LEAF_EDGE_LIGHT;
    let [dt, dr, db, dl] = LEAF_EDGE_DARK;
    let mut v = Vec::with_capacity(8);
    v.extend(super::decl::themed_border_top_color(t, dt));
    v.extend(super::decl::themed_border_right_color(r, dr));
    v.extend(super::decl::themed_border_bottom_color(b, db));
    v.extend(super::decl::themed_border_left_color(l, dl));
    v
}

const EB_GARAMOND_STR: AzString = AzString::from_const_str("EB Garamond");
const GEORGIA_STR: AzString = AzString::from_const_str("Georgia");
const SERIF_STR: AzString = AzString::from_const_str("serif");
const CAPS_FAMILIES: &[StyleFontFamily] = &[
    StyleFontFamily::System(EB_GARAMOND_STR),
    StyleFontFamily::System(GEORGIA_STR),
    StyleFontFamily::System(AzString::from_const_str("Times New Roman")),
    StyleFontFamily::System(SERIF_STR),
];

/// `--font-caps`: `'EB Garamond', Georgia, 'Times New Roman', serif` - the
/// UI hand flora sets its tabs, buttons and headings in; the bundled face
/// first (`text3::ui_fonts`), so it holds on every machine.
pub(crate) const FONT_CAPS: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(CAPS_FAMILIES);

/// A command's capitals: the specimen's 13.5px bold small capitals tracked
/// .06em, as 11px capitals.
pub(crate) const CAPS_COMMAND: (isize, f32) = (11, 0.07);
/// A group or section title (`.fl-label`, the specimen's `h3`): .12em.
pub(crate) const CAPS_TITLE: (isize, f32) = (11, 0.12);
/// A field's label over it (the specimen's 11.5px small capitals): .1em.
pub(crate) const CAPS_LABEL: (isize, f32) = (10, 0.1);

/// A label in flora's capitals, `(px, em)` one of the `CAPS_*` sizes: EB
/// Garamond, bold, uppercase, tracked out.
#[must_use]
pub(crate) fn caps((px, em): (isize, f32)) -> Vec<CssPropertyWithConditions> {
    alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_family(FONT_CAPS)),
        super::decl::font_size(px),
        super::decl::bold(),
        CssPropertyWithConditions::simple(CssProperty::TextTransform(
            StyleTextTransform::Uppercase.into(),
        )),
        super::decl::letter_spacing_em(em),
    ]
}

/// `style` with flora's capitals in place of its own face and size.
#[must_use]
pub(crate) fn in_caps(
    style: &[CssPropertyWithConditions],
    size: (isize, f32),
) -> Vec<CssPropertyWithConditions> {
    let mut v: Vec<CssPropertyWithConditions> = style
        .iter()
        .filter(|p| {
            !matches!(
                p.property.get_type(),
                CssPropertyType::FontFamily | CssPropertyType::FontSize
            )
        })
        .cloned()
        .collect();
    v.extend(caps(size));
    v
}

// -- the stone's rig: the two radial passes the transcription above lacked --

const STONE_BLOOM_STOPS: &[NormalizedLinearColorStop] = &[
    stop(0, ColorU::new(255, 253, 238, 82)),
    stop(45, ColorU::new(255, 253, 238, 0)),
];

/// `.btn-primary::after`, first layer: the bloom just off the upper-left
/// corner, `radial-gradient(ellipse 58% 150% at 2% -20%, rgba(255,253,238,
/// .32) 0%, transparent 68%)`. azul's radial sizes are keywords, so the
/// ellipse is the farthest side's, faded by 45% - about the CSS's reach.
pub const STONE_BLOOM: StyleBackgroundContent =
    StyleBackgroundContent::RadialGradient(RadialGradient {
        shape: Shape::Ellipse,
        size: RadialGradientSize::FarthestSide,
        position: StyleBackgroundPosition {
            horizontal: BackgroundPositionHorizontal::Exact(PixelValue::const_percent(2)),
            vertical: BackgroundPositionVertical::Exact(PixelValue::const_percent(-20)),
        },
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(STONE_BLOOM_STOPS),
    });

const STONE_FAR_CORNER_STOPS: &[NormalizedLinearColorStop] = &[
    stop(0, ColorU::new(12, 10, 4, 102)),
    stop(45, ColorU::new(12, 10, 4, 0)),
];

/// `.btn-primary::after`, last layer: the far corner falling away,
/// `radial-gradient(ellipse 72% 155% at 106% 126%, rgba(12,10,4,.40) 0%,
/// transparent 66%)`, sized as [`STONE_BLOOM`].
pub const STONE_FAR_CORNER: StyleBackgroundContent =
    StyleBackgroundContent::RadialGradient(RadialGradient {
        shape: Shape::Ellipse,
        size: RadialGradientSize::FarthestSide,
        position: StyleBackgroundPosition {
            horizontal: BackgroundPositionHorizontal::Exact(PixelValue::const_percent(106)),
            vertical: BackgroundPositionVertical::Exact(PixelValue::const_percent(126)),
        },
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(STONE_FAR_CORNER_STOPS),
    });

const SUNKEN_BLOOM_STOPS: &[NormalizedLinearColorStop] = &[
    stop(0, ColorU::new(255, 253, 238, 66)),
    stop(42, ColorU::new(255, 253, 238, 0)),
];

/// The sunken rig's bloom: `radial-gradient(ellipse 62% 170% at 4% -26%,
/// rgba(255,253,238,.26) 0%, transparent 62%)` - the light falling INTO the
/// well.
pub const SUNKEN_BLOOM: StyleBackgroundContent =
    StyleBackgroundContent::RadialGradient(RadialGradient {
        shape: Shape::Ellipse,
        size: RadialGradientSize::FarthestSide,
        position: StyleBackgroundPosition {
            horizontal: BackgroundPositionHorizontal::Exact(PixelValue::const_percent(4)),
            vertical: BackgroundPositionVertical::Exact(PixelValue::const_percent(-26)),
        },
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(SUNKEN_BLOOM_STOPS),
    });

/// `--fl-gem` cut from `stone`: `radial-gradient(ellipse 130% 100% at 30%
/// 12%, glow 0%, stone 48%, deep 100%)` - lit where the light enters, the
/// ellipse the farthest corner's (azul's radial sizes are keywords).
#[must_use]
pub fn gem(stone: FloraStone) -> StyleBackgroundContent {
    StyleBackgroundContent::RadialGradient(RadialGradient {
        shape: Shape::Ellipse,
        size: RadialGradientSize::FarthestCorner,
        position: StyleBackgroundPosition {
            horizontal: BackgroundPositionHorizontal::Exact(PixelValue::const_percent(30)),
            vertical: BackgroundPositionVertical::Exact(PixelValue::const_percent(12)),
        },
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_vec(alloc::vec![
            stop(0, stone.glow),
            stop(48, stone.stone),
            stop(100, stone.deep),
        ]),
    })
}

/// `--fl-gem-sunken` cut from `stone`: `linear-gradient(175deg, deep 0%,
/// stone 96%)` - a stone pressed into its well.
#[must_use]
pub fn gem_sunken(stone: FloraStone) -> StyleBackgroundContent {
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: deg(175),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_vec(alloc::vec![
            stop(0, stone.deep),
            stop(96, stone.stone),
        ]),
    })
}

/// A raised stone: the gem, then the rig over it - the bloom, the shadow of
/// the lit top and left edges, the far corner, and the specular streak
/// (brighter and wider on `hover`, as the face turns toward the light). The
/// same layers at rest and hovered, so the fade tweens stop by stop.
#[must_use]
pub fn raised_stone(stone: FloraStone, hover: bool) -> Vec<StyleBackgroundContent> {
    alloc::vec![
        gem(stone),
        STONE_BLOOM,
        STONE_RIG_TOP,
        STONE_RIG_LEFT,
        STONE_FAR_CORNER,
        if hover {
            STONE_STREAK_HOVER
        } else {
            STONE_STREAK
        },
    ]
}

/// A stone pressed into its well: the sunken gem under the sunken rig, lit
/// from below the near edge.
#[must_use]
pub fn sunken_stone(stone: FloraStone) -> Vec<StyleBackgroundContent> {
    alloc::vec![
        gem_sunken(stone),
        SUNKEN_BLOOM,
        SUNKEN_RIG_TOP,
        SUNKEN_RIG_LEFT,
        SUNKEN_RIG_BOTTOM,
    ]
}

/// A stone's raised depth: `inset 0 1px 0 rgba(255,255,255,.3)`, `inset 0
/// -2px 4px rgba(0,0,0,.3)` and `--fl-shadow-1` - the stone is its own
/// colour by night too, only the cast shadow deepens.
fn stone_depth() -> Vec<CssPropertyWithConditions> {
    let mut v = alloc::vec![
        CssPropertyWithConditions::simple(shadow_in(ShadowSlot::Top, 1, 0, 0, white(77), true)),
        CssPropertyWithConditions::simple(shadow_in(
            ShadowSlot::Right,
            -2,
            4,
            0,
            ColorU::new(0, 0, 0, 77),
            true
        )),
    ];
    v.extend(themed_shadow_in(
        ShadowSlot::Bottom,
        (1, 2, 0),
        ColorU::new(48, 45, 38, 36),
        ColorU::new(0, 0, 0, 140),
        false,
    ));
    v
}

/// `text-shadow: 0 1px 1px rgba(0, 0, 0, 0.3)`: the paper ink cut into a
/// stone.
fn stone_text_shadow() -> CssPropertyWithConditions {
    CssPropertyWithConditions::simple(CssProperty::TextShadow(
        azul_css::css::CssPropertyValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
            offset_x: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            offset_y: PixelValueNoPercent {
                inner: PixelValue::const_px(1),
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(1),
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            clip_mode: BoxShadowClipMode::Outset,
            color: ColorU::new(0, 0, 0, 77),
        })),
    ))
}

/// What a button IS in flora's vocabulary - its meaning, never a colour
/// (flora.css's BUTTONS block and the specimen's row): the accent is the
/// theme's (a spin's), not the button's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FloraButtonKind {
    /// `.btn-secondary`: raised paper, the everyday command (`Default`,
    /// `Secondary`).
    Standard,
    /// A stone: `.btn-primary` cut from the accent (`Primary` - one per view,
    /// the thing to do next), or a semantic stone cut the same way (`Success`
    /// leaf, `Danger` clay, `Warning` amber, `Info` slate).
    Stone(FloraStone),
    /// `.btn-hero-primary`: paper in a metal edge, rare (`Illuminated`).
    Illuminated,
    /// `.btn-quiet`: brass ink with a rule under it - "a note, not a
    /// control" (`Link`).
    Quiet,
}

impl FloraButtonKind {
    /// The kind a button type means.
    #[must_use]
    pub(crate) const fn of(t: crate::widgets::button::ButtonType) -> Self {
        use crate::widgets::button::ButtonType;
        match t {
            ButtonType::Default | ButtonType::Secondary => Self::Standard,
            ButtonType::Primary => Self::Stone(STONE_ACCENT),
            ButtonType::Success => Self::Stone(STONE_LEAF),
            ButtonType::Danger => Self::Stone(STONE_CLAY),
            ButtonType::Warning => Self::Stone(STONE_AMBER),
            ButtonType::Info => Self::Stone(STONE_SLATE),
            ButtonType::Illuminated => Self::Illuminated,
            ButtonType::Link => Self::Quiet,
        }
    }
}

/// A flora command's resting face, light value then night twin, property by
/// property: the face, the edge, the ink and the depth. `boxed`: whether a
/// quiet command has a label (an icon-only one is bare glyph - the media
/// controls' transport keys).
#[must_use]
fn flora_button_face(kind: FloraButtonKind, boxed: bool) -> Vec<CssPropertyWithConditions> {
    use super::decl;
    let mut v = Vec::with_capacity(24);
    match kind {
        FloraButtonKind::Standard => {
            v.extend(decl::themed_layers(
                alloc::vec![RAISED_FACE_LIGHT],
                alloc::vec![RAISED_FACE_DARK],
            ));
            v.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD2));
            v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
            v.extend(raised_depth());
        }
        FloraButtonKind::Stone(stone) => {
            v.push(CssPropertyWithConditions::simple(layers(raised_stone(stone, false))));
            v.extend(decl::border_colors(stone.deep).map(CssPropertyWithConditions::simple));
            v.push(CssPropertyWithConditions::simple(decl::ink(LIGHT_ON_ACC)));
            v.push(stone_text_shadow());
            v.extend(stone_depth());
        }
        FloraButtonKind::Illuminated => {
            v.extend(decl::themed_layers(
                alloc::vec![RAISED_FACE_LIGHT],
                alloc::vec![RAISED_FACE_DARK],
            ));
            v.extend(leaf_edge());
            v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
            v.extend(raised_depth());
        }
        FloraButtonKind::Quiet => {
            v.extend(decl::themed_ink(LIGHT_QT, DARK_QT));
            v.push(CssPropertyWithConditions::simple(CssProperty::TextDecoration(
                StyleTextDecoration::Underline.into(),
            )));
            if boxed {
                // `.btn-quiet`: the faintest paper (`--fl-rT` falling to
                // `--fl-fld2`) in a separator's hairline, a lit lip, no foot.
                v.extend(decl::border(1));
                v.extend(decl::themed_layers(
                    alloc::vec![decl::face(LIGHT_RT, LIGHT_FLD2)],
                    alloc::vec![decl::face(DARK_RT, DARK_FLD2)],
                ));
                v.extend(decl::themed_border_color(LIGHT_SEP, DARK_SEP));
                v.extend(themed_shadow_in(ShadowSlot::Top, (1, 0, 0), white(140), white(18), true));
            }
        }
    }
    v
}

/// A flora command's hover and pressed states (not its focus ring: that is
/// [`double_ring`], pushed after these so it wins the shared slots).
#[must_use]
fn flora_button_states(kind: FloraButtonKind, boxed: bool) -> Vec<CssPropertyWithConditions> {
    use super::decl;
    let mut v = Vec::with_capacity(20);
    match kind {
        FloraButtonKind::Standard => {
            v.extend(decl::hover_layers(
                alloc::vec![HOVER_FACE_LIGHT],
                alloc::vec![HOVER_FACE_DARK],
            ));
            v.extend(decl::hover_border_color(LIGHT_BD3, DARK_BD3));
            v.extend(decl::active_layers(
                alloc::vec![PRESSED_FACE_LIGHT],
                alloc::vec![PRESSED_FACE_DARK],
            ));
            v.extend(pressed_depth());
        }
        FloraButtonKind::Stone(stone) => {
            v.push(CssPropertyWithConditions::on_hover(layers(raised_stone(stone, true))));
            v.extend(metal_comes_up());
            v.push(CssPropertyWithConditions::on_active(layers(sunken_stone(stone))));
            // The well a pressed stone sits in: `inset 0 2px 5px
            // rgba(0,0,0,.45)`, nothing cast, no rim.
            v.push(CssPropertyWithConditions::on_active(shadow_in(
                ShadowSlot::Top,
                2,
                5,
                0,
                ColorU::new(0, 0, 0, 115),
                true,
            )));
            v.push(CssPropertyWithConditions::on_active(no_shadow_in(ShadowSlot::Right)));
            v.push(CssPropertyWithConditions::on_active(no_shadow_in(ShadowSlot::Bottom)));
            v.push(CssPropertyWithConditions::on_active(no_shadow_in(ShadowSlot::Left)));
        }
        FloraButtonKind::Illuminated => {
            v.extend(decl::hover_layers(
                alloc::vec![HOVER_FACE_LIGHT],
                alloc::vec![HOVER_FACE_DARK],
            ));
            v.extend(metal_comes_up());
            v.extend(decl::active_layers(
                alloc::vec![PRESSED_FACE_LIGHT],
                alloc::vec![PRESSED_FACE_DARK],
            ));
            v.extend(pressed_depth());
            v.push(CssPropertyWithConditions::on_active(no_shadow_in(ShadowSlot::Left)));
        }
        FloraButtonKind::Quiet => {
            v.extend(decl::hover_ink(LIGHT_QT2, DARK_QT2));
            if boxed {
                v.extend(decl::hover_fill(DIALOG_QUIET_WASH_LIGHT, DIALOG_QUIET_WASH_DARK));
            }
        }
    }
    v
}

/// The face a disabled flora command shows (`.btn[disabled]`): the disabled
/// paper (`--fl-disBg`), its ink (`--fl-disTx`) and the lightest edge
/// (`--fl-bd4`), flat - no lip, no cast shadow, no stone, no rim. A quiet
/// command only fades its ink.
#[must_use]
fn flora_disabled_face(kind: FloraButtonKind, boxed: bool) -> Vec<CssPropertyWithConditions> {
    use super::decl;
    let mut v = Vec::with_capacity(16);
    if kind == FloraButtonKind::Quiet && !boxed {
        v.extend(decl::themed_ink(LIGHT_DISTX, DARK_DISTX));
        return v;
    }
    v.extend(decl::themed_fill(LIGHT_DISBG, DARK_DISBG));
    v.extend(decl::themed_ink(LIGHT_DISTX, DARK_DISTX));
    v.extend(decl::themed_border_color(LIGHT_BD4, DARK_BD4));
    for slot in [ShadowSlot::Right, ShadowSlot::Top, ShadowSlot::Bottom] {
        v.push(CssPropertyWithConditions::simple(no_shadow_in(slot)));
    }
    v.push(CssPropertyWithConditions::simple(CssProperty::TextShadow(
        azul_css::css::CssPropertyValue::None,
    )));
    v
}

/// The flora command (flora.css's BUTTONS, the specimen's button row): one of
/// four kinds - raised paper, a stone, paper in a metal edge, a quiet note
/// ([`FloraButtonKind`]) - in five states: rest, hover (the face lifts; on a
/// stone the metal comes up), pressed (sunken, at once), focus (the double
/// ring) and disabled (the disabled paper). Its label is set in flora's
/// capitals. Every state change fades, at flora.css's pace: paper and the
/// quiet note over `--fl-dur` on `--fl-ease`; a stone and the metal-edged
/// command over `--fl-dur-slow`, the face on `--fl-ease` and the edge and the
/// glow on `ease` (`LIT_FACE`); a press over `--fl-dur-fast`.
#[must_use]
pub fn button(btn: Button) -> Dom {
    let callbacks = match btn.on_click.into_option() {
        Some(ButtonOnClick {
            refany: data,
            callback,
        }) => vec![CoreCallbackData {
            event: EventFilter::Hover(HoverEventFilter::Click),
            callback: azul_core::callbacks::CoreCallback {
                cb: callback.cb as *const () as usize,
                ctx: callback.ctx,
            },
            refany: data,
        }],
        None => Vec::new(),
    };

    let btn_type = btn.button_type;
    let kind = FloraButtonKind::of(btn_type);
    // The states `Button::with_disabled` / `with_toggled` asked for.
    let toggled_on = btn.toggled == azul_css::OptionBool::Some(true);
    let disabled = btn.is_disabled();
    let type_class = btn.button_type.class_name();
    let classes: Vec<IdOrClass> = vec![
        Class(AzString::from("__azul-native-button")),
        Class(AzString::from(type_class)),
        Class(AzString::from("__azul-theme-flora")),
    ];

    let mut button = Dom::create_node(NodeType::Button);

    let has_icon = !btn.icon.as_str().is_empty() || btn.icon_dom.is_some();
    let has_image = btn.image.is_some();
    let has_trailing_icon = !btn.trailing_icon.as_str().is_empty();
    // A command with words takes flora's box and capitals; an icon-only one
    // (a transport key, a toolbar glyph) keeps the widget's own metrics.
    let has_label = !btn.label.as_str().is_empty();

    // Resolved before `btn`'s fields are moved into the tree below.
    let btn_container_style = btn.resolved_container_style();
    // A caller who injected a container style (`Some`) chose every property in
    // it — resting face, dark colours and states included. The chrome widgets
    // hand in part styles complete with their own hover/pressed pairs, and
    // anything this theme appended after them would win the cascade (inline
    // resolution is last-match) and paint the theme's greys over the ribbon's
    // blue. So the theme adds to its OWN default only.
    let btn_owns_style = btn.container_style.as_ref().is_none();
    // The same for the label: the widget's default face and size give way to
    // flora's capitals; a caller's label style is taken as it is.
    let btn_label_style = if btn.label_style.as_ref().is_none() {
        CssPropertyWithConditionsVec::from_vec(in_caps(
            btn.resolved_label_style().as_slice(),
            CAPS_COMMAND,
        ))
    } else {
        btn.resolved_label_style()
    };
    let btn_image_style = btn.resolved_image_style();
    let btn_icon_style = btn.resolved_icon_style();
    let btn_trailing_icon_style = btn.resolved_trailing_icon_style();

    let a11y_name_src: String = if btn.label.as_str().is_empty() {
        if has_icon {
            btn.icon.as_str().to_string()
        } else {
            String::new()
        }
    } else {
        btn.label.as_str().to_string()
    };

    if has_icon {
        button = button.with_child(match btn.icon_dom.into_option() {
            Some(dom) => dom,
            None => Dom::create_icon(btn.icon).with_css_props(btn_icon_style),
        });
    }

    if let Some(image) = btn.image.into_option() {
        button = button.with_child(Dom::create_image(image).with_css_props(btn_image_style));
    }

    let skip_label = btn.label.as_str().is_empty() && (has_icon || has_image || has_trailing_icon);
    if !skip_label {
        button = button.with_child(
            crate::widgets::widget_p_chrome()
                .with_css_props(btn_label_style)
                .with_children(azul_core::dom::DomVec::from_vec(vec![
                    Dom::create_text_do_not_use_without_block_level_wrapper(btn.label),
                ])),
        );
    }

    if has_trailing_icon {
        button = button.with_child(
            Dom::create_icon(btn.trailing_icon).with_css_props(btn_trailing_icon_style),
        );
    }

    let a11y_name = a11y_name_src;
    let mut a11y = AccessibilityInfo {
        role: AccessibilityRole::PushButton,
        ..AccessibilityInfo::default()
    };
    if !a11y_name.is_empty() {
        a11y.accessibility_name = Some(AzString::from(a11y_name)).into();
    }

    let mut container_style: Vec<CssPropertyWithConditions> =
        btn_container_style.as_slice().to_vec();

    if btn_owns_style {
        // The house radius (`--fl-r`), and for a command with words the
        // specimen's box: 4px over and under the capitals, 12px either side.
        container_style.extend(super::decl::radius(3));
        if has_label {
            container_style.extend(super::decl::padding(4, 12, 4, 12));
        }
        // The resting face - after the widget's flat fill, which it wins
        // over, and before the states, which win over it. Each light value
        // with its night twin right after it.
        container_style.extend(flora_button_face(kind, has_label));
        // A toggled-on command rests on its pressed face.
        if toggled_on && !disabled {
            container_style.extend(button_toggled_face(btn_type));
        }
        if disabled {
            // The disabled paper, and no hover or pressed paint at all.
            container_style.extend(flora_disabled_face(kind, has_label));
        } else {
            // The interactive states go LAST. Inline declarations resolve
            // last-match wins and a `dark_mode(..)` rule matches in every
            // pseudo-state, so a dark resting value pushed after a
            // `dark_on_hover` twin would shadow it.
            container_style.extend(flora_button_states(kind, has_label));
            // Light moves slowly across a stone and across the field of a
            // metal-edged command, its edge and its glow coming up on their
            // own curve (`LIT_FACE`); paper and the quiet note change state
            // at the house pace. A press is quick either way.
            container_style.extend(match kind {
                FloraButtonKind::Stone(_) | FloraButtonKind::Illuminated => {
                    flora_fade_on_curves(LIT_FACE, FL_DUR_SLOW_MS)
                }
                FloraButtonKind::Standard | FloraButtonKind::Quiet => {
                    flora_fade(FLORA_FACE, FL_DUR_MS)
                }
            });
        }
        // The keyboard ring, last: it wins the Left and Bottom slots over a
        // hovered rim and a pressed well. A disabled command keeps its stop.
        container_style.extend(double_ring(2));
    } else if disabled {
        // A caller's style (a ribbon button's): no hover / pressed paint,
        // dimmed - the shared rule.
        container_style = crate::widgets::button::disabled_style(&container_style);
    }

    button
        .with_css_props(CssPropertyWithConditionsVec::from_vec(container_style))
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_callbacks(callbacks.into())
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(a11y)
}

// ==== fields and marks (FLORA11) ====
//
// The design system's INPUT: field paper (`--fl-fld`) in a `--fl-bd2`
// hairline at the house radius, sunk by `--fl-well`, written in Garamond. The
// rule darkens to `--fl-bd3` under the pointer. Focused, the edge takes the
// accent, the paper lifts (`--fl-hT`) and the double ring stands one pixel
// off it (`outline: 2px solid var(--acc); outline-offset: 1px`). A check
// box, a radio well and a drop-down's closed field are cut from the same
// paper; a mark set into one (a tick, a dot, a filled track) is the accent
// stone.

/// `--fl-well`: `inset 0 1px 2px rgba(48,45,38,.10)`, at night
/// `rgba(0,0,0,.45)`, in the lip's slot (Top).
#[must_use]
pub(crate) fn well() -> [CssPropertyWithConditions; 2] {
    themed_shadow_in(
        ShadowSlot::Top,
        (1, 2, 0),
        ColorU::new(48, 45, 38, 26),
        ColorU::new(0, 0, 0, 115),
        true,
    )
}

/// `--font-serif`: running text in flora is Garamond too - the same stack as
/// the capitals, set upright.
pub(crate) const SERIF_FAMILY: StyleFontFamilyVec = FONT_CAPS;

/// A flora field at rest: a solid 1px `--fl-bd2` rule at the house radius
/// around field paper, sunk by the well, in the house ink - each light value
/// with its night twin.
#[must_use]
pub(crate) fn field_skin() -> Vec<CssPropertyWithConditions> {
    use super::decl;
    let mut v = Vec::with_capacity(28);
    v.extend(decl::border(1));
    v.extend(decl::radius(3));
    v.extend(decl::themed_fill(LIGHT_FLD, DARK_FLD));
    v.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD2));
    v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    v.extend(well());
    v
}

/// A flora field's states: the rule darkening under the pointer; focused,
/// the accent edge, the lifted paper and the double ring a pixel off it.
/// Push after [`field_skin`] (and after any dark resting value).
#[must_use]
pub(crate) fn field_states() -> Vec<CssPropertyWithConditions> {
    use super::decl;
    let mut v = Vec::with_capacity(20);
    v.extend(decl::hover_border_color(LIGHT_BD3, DARK_BD3));
    v.extend(decl::focus_ring(LIGHT_ACC, DARK_GLOW));
    v.extend(CssPropertyWithConditions::themed_on_focus(
        decl::fill(LIGHT_HT),
        decl::fill(DARK_FLD2),
    ));
    v.extend(double_ring(1));
    v.extend(flora_fade(FLORA_FACE, FL_DUR_MS));
    v
}

/// The checked mark of a flora check box: the stone itself - the accent
/// falling to its deep tone (`linear-gradient(acc, deep)`), edged in the deep
/// tone - laid over the whole box, edge included, so the click that only shows
/// or hides it (`check_box::input`, by opacity) turns the empty paper box into
/// the filled one. Its tick is [`check_tick`].
#[must_use]
fn check_mark_skin() -> Vec<CssPropertyWithConditions> {
    use super::decl;
    let mut v = Vec::with_capacity(24);
    v.push(decl::position(LayoutPosition::Absolute));
    v.push(decl::px_top(-1.0));
    v.push(decl::px_left(-1.0));
    v.push(CssPropertyWithConditions::simple(CssProperty::const_box_sizing(
        LayoutBoxSizing::BorderBox,
    )));
    v.push(decl::px_width(15.0));
    v.push(decl::px_height(15.0));
    v.push(decl::display_flex());
    v.push(CssPropertyWithConditions::simple(CssProperty::const_justify_content(
        LayoutJustifyContent::Center,
    )));
    v.push(CssPropertyWithConditions::simple(CssProperty::const_align_items(
        LayoutAlignItems::Center,
    )));
    v.extend(decl::border(1));
    v.extend(decl::radius(3));
    v.extend(decl::border_colors(LIGHT_DEEP).map(CssPropertyWithConditions::simple));
    v.push(CssPropertyWithConditions::simple(layers(alloc::vec![super::decl::face(
        LIGHT_ACC, LIGHT_DEEP
    )])));
    v
}

const CHECK_TICK_ROTATION: &[StyleTransform] =
    &[StyleTransform::Rotate(AngleValue::const_deg(45))];

/// The tick in a checked box: two strokes of the paper ink (`--fl-on-acc`), an
/// L turned 45 degrees - drawn, so it needs no glyph from the font.
#[must_use]
fn check_tick() -> Dom {
    use super::decl;
    let mut v = Vec::with_capacity(10);
    v.push(decl::px_width(4.0));
    v.push(decl::px_height(8.0));
    v.push(CssPropertyWithConditions::simple(CssProperty::const_margin_top(
        LayoutMarginTop::const_px(-2),
    )));
    v.extend(decl::border_right(2));
    v.extend(decl::border_bottom(2));
    v.push(CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: LIGHT_ON_ACC,
        },
    )));
    v.push(CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: LIGHT_ON_ACC,
        },
    )));
    v.push(CssPropertyWithConditions::simple(CssProperty::const_transform(
        StyleTransformVec::from_const_slice(CHECK_TICK_ROTATION),
    )));
    Dom::create_div().with_css_props(CssPropertyWithConditionsVec::from_vec(v))
}

/// The knob of a flora switch and the thumb of a flora slider: a bead of
/// paper, `radial-gradient(circle at 35% 30%, #FDFAF1, #CFC4AD)`.
#[must_use]
fn paper_bead() -> StyleBackgroundContent {
    StyleBackgroundContent::RadialGradient(RadialGradient {
        shape: Shape::Circle,
        size: RadialGradientSize::FarthestCorner,
        position: StyleBackgroundPosition {
            horizontal: BackgroundPositionHorizontal::Exact(PixelValue::const_percent(35)),
            vertical: BackgroundPositionVertical::Exact(PixelValue::const_percent(30)),
        },
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_vec(alloc::vec![
            stop(0, ColorU::rgb(0xFD, 0xFA, 0xF1)),
            stop(100, ColorU::rgb(0xCF, 0xC4, 0xAD)),
        ]),
    })
}

/// A flora switch's track face, on (the stone: the accent falling to its
/// deep tone, in the theme's - a spin's - accent) or off (the trough,
/// `--fl-track` falling to `--fl-fld2`, by day or at night). Read by the
/// switch's click handler, which writes the face it toggles to.
#[must_use]
pub(crate) fn switch_track_face(checked: bool, dark: bool) -> StyleBackgroundContentVec {
    let face = if checked {
        let ramp = super::spin::FloraSpin::current().ramp();
        super::decl::face(ramp.acc, ramp.deep)
    } else if dark {
        super::decl::face(DARK_TRACK, DARK_FLD2)
    } else {
        super::decl::face(LIGHT_TRACK, LIGHT_FLD2)
    };
    StyleBackgroundContentVec::from_vec(alloc::vec![face])
}

use crate::widgets::check_box::CheckBox;

/// The flora check box (the design system's selection card): a 15px box of
/// field paper in a `--fl-bd2` rule at the house radius, sunk by the well;
/// checked, the stone fills it, edged in the deep tone, a paper-ink tick cut
/// into it. The rule darkens under the pointer; focused, the double ring.
/// A caller's container or mark style is taken as it is.
#[must_use]
pub fn check_box(cb: CheckBox) -> Dom {
    use super::decl;
    let cb_name = cb.accessibility_name.clone();
    crate::widgets::warn_widget_needs_a_name("check_box", cb_name.is_some());

    let checked_now = cb.check_box_state.inner.checked;

    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{EventFilter, HoverEventFilter},
    };

    let owns_container = cb.container_style.as_ref().is_none();
    let owns_mark = cb.content_style.as_ref().is_none();

    let mut container_style: Vec<CssPropertyWithConditions> =
        cb.resolved_container_style().as_slice().to_vec();
    if owns_container {
        container_style.push(decl::position(LayoutPosition::Relative));
        container_style.push(decl::px_width(13.0));
        container_style.push(decl::px_height(13.0));
        container_style.extend(decl::padding(0, 0, 0, 0));
        container_style.extend(decl::border(1));
        container_style.extend(decl::radius(3));
        container_style.extend(decl::themed_layers(
            vec![decl::face(LIGHT_FLD, LIGHT_FLD2)],
            vec![decl::face(DARK_FLD, DARK_FLD2)],
        ));
        container_style.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD2));
        container_style.extend(well());
        // States last.
        container_style.extend(decl::hover_border_color(LIGHT_BD3, DARK_BD3));
        container_style.extend(double_ring(2));
    } else {
        container_style.push(CssPropertyWithConditions::dark_mode(
            CssProperty::BackgroundContent(
                StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(
                    DARK_SUR,
                )])
                .into(),
            ),
        ));
    }
    let mut content_style: Vec<CssPropertyWithConditions> =
        cb.resolved_content_style().as_slice().to_vec();
    if owns_mark {
        // The widget's default (its size, its opacity - what the click
        // toggles), then the stone over it.
        content_style.extend(check_mark_skin());
    } else if checked_now {
        content_style.push(CssPropertyWithConditions::dark_mode(
            CssProperty::BackgroundContent(
                StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_INK)])
                    .into(),
            ),
        ));
    }

    let mut mark = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from(
            crate::widgets::check_box::CHECKBOX_CONTENT_CLASS,
        ))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(content_style));
    if owns_mark {
        mark = mark.with_child(check_tick());
    }

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from(
            crate::widgets::check_box::CHECKBOX_CONTAINER_CLASS,
        ))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(container_style))
        .with_callbacks(
            vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::Click),
                callback: CoreCallback {
                    cb: crate::widgets::check_box::input::default_on_checkbox_clicked as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
                refany: RefAny::new(cb.check_box_state),
            }]
            .into(),
        )
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::CheckButton,
            accessibility_name: cb_name,
            states: azul_core::a11y::AccessibilityStateVec::from_const_slice(if checked_now {
                &[azul_core::a11y::AccessibilityState::CheckedTrue]
            } else {
                &[azul_core::a11y::AccessibilityState::CheckedFalse]
            }),
            ..Default::default()
        })
        .with_children(vec![mark].into())
}

use crate::widgets::text_input::{
    default_on_focus_lost, default_on_focus_received, default_on_mouse_hover,
    default_on_text_input, default_on_virtual_key_down, TextInput, TEXT_INPUT_CONTAINER_CLASS,
    TEXT_INPUT_LABEL_CLASS,
};

/// A field's PROMPT in flora's hint ink (`--fl-soft2`, its night twin the
/// dark hint) - unless the caller's label style declares a prompt ink of its
/// own (`owns_label` false). Flat's #9B9B9B, which the field's default label
/// style carries, read at 1.93:1 on a flora strip.
fn push_prompt_ink(label_style: &mut Vec<CssPropertyWithConditions>, owns_label: bool) {
    use azul_css::dynamic_selector::{DynamicSelector, ModeCondition, PseudoStateType};
    const PROMPT: [DynamicSelector; 1] = [DynamicSelector::PseudoState(PseudoStateType::Placeholder)];
    let callers_own = !owns_label
        && label_style.iter().any(|p| {
            p.property.get_type() == CssPropertyType::TextColor
                && p.apply_if.as_ref() == PROMPT.as_slice()
        });
    if callers_own {
        return;
    }
    label_style.push(CssPropertyWithConditions::on_placeholder(CssProperty::TextColor(
        StyleTextColor { inner: LIGHT_SOFT2 }.into(),
    )));
    label_style.push(CssPropertyWithConditions::with_single_condition(
        CssProperty::const_text_color(StyleTextColor { inner: DARK_SOFT2 }),
        &[
            DynamicSelector::Mode(ModeCondition::Dark),
            DynamicSelector::PseudoState(PseudoStateType::Placeholder),
        ],
    ));
}

#[must_use]
pub fn text_input(mut ti: TextInput) -> Dom {
    let a11y_name: Option<AzString> = ti.text_input_state.inner.placeholder.as_ref().cloned();
    let a11y_value: String = ti
        .text_input_state
        .inner
        .text
        .as_ref()
        .iter()
        .filter_map(|c| core::char::from_u32(*c))
        .collect();

    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{
            AttributeType, DomVec, EventFilter, FocusEventFilter, HoverEventFilter,
            IdOrClass::Class, TabIndex,
        },
    };

    ti.text_input_state.inner.cursor_pos = ti.text_input_state.inner.text.len();

    // What the line SHOWS - the value, or a password's mask. The engine's
    // buffer is seeded from it, so this is also what every caret offset the
    // engine reports indexes into.
    let label_text: String = crate::widgets::text_input::display_text(&ti.text_input_state.inner);

    let placeholder = ti
        .text_input_state
        .inner
        .placeholder
        .as_ref()
        .map(|s| s.as_str().to_string())
        .unwrap_or_default();

    // Resolved before `ti.text_input_state` is moved out below, and through the
    // widget's own resolver rather than a second copy of its default — the point
    // of the resolver is that flat and flora cannot drift on this answer.
    let resolved_container_style = ti.resolved_container_style();
    let resolved_label_style = ti.resolved_label_style();
    let owns_container = ti.container_style.as_ref().is_none();
    let owns_label = ti.label_style.as_ref().is_none();

    let state_ref = RefAny::new(ti.text_input_state);

    let mut container_style: Vec<CssPropertyWithConditions> =
        resolved_container_style.as_slice().to_vec();
    let mut label_style: Vec<CssPropertyWithConditions> = resolved_label_style.as_slice().to_vec();
    if owns_container {
        // The design system's field (see `field_skin`), the states last.
        container_style.extend(field_skin());
        container_style.extend(field_states());
    } else {
        // A caller's field (a PDF form's, on paper): only the night twins it
        // left open, and the shared ring.
        for twin in [
            CssProperty::BackgroundContent(
                StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_SUR)])
                    .into(),
            ),
            CssProperty::TextColor(StyleTextColor { inner: DARK_INK }.into()),
            CssProperty::BorderTopColor(StyleBorderTopColor { inner: DARK_BD }.into()),
            CssProperty::BorderBottomColor(StyleBorderBottomColor { inner: DARK_BD }.into()),
            CssProperty::BorderLeftColor(StyleBorderLeftColor { inner: DARK_BD }.into()),
            CssProperty::BorderRightColor(StyleBorderRightColor { inner: DARK_BD }.into()),
        ] {
            super::decl::push_dark_twin(
                &mut container_style,
                CssPropertyWithConditions::dark_mode(twin),
            );
        }
        // Appended LAST: the last matching inline declaration wins, so a
        // `dark_mode` border pushed after these would beat the dark ring.
        container_style.extend_from_slice(&FIELD_BORDER_STATES);
    }
    if owns_label {
        // The value is written in Garamond, in the house ink.
        label_style.retain(|p| p.property.get_type() != CssPropertyType::FontFamily);
        label_style.push(CssPropertyWithConditions::simple(CssProperty::const_font_family(
            SERIF_FAMILY,
        )));
        label_style.extend(super::decl::themed_ink(LIGHT_INK, DARK_INK));
    } else {
        super::decl::push_dark_twin(
            &mut label_style,
            CssPropertyWithConditions::dark_mode(CssProperty::TextColor(
                StyleTextColor { inner: DARK_INK }.into(),
            )),
        );
    }
    push_prompt_ink(&mut label_style, owns_label);

    Dom::create_div()
        .with_ids_and_classes(vec![Class(TEXT_INPUT_CONTAINER_CLASS.into())].into())
        .with_css_props(CssPropertyWithConditionsVec::from_vec(container_style))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Text,
            accessibility_name: a11y_name.into(),
            accessibility_value: Some(AzString::from(a11y_value)).into(),
            ..Default::default()
        })
        .with_contenteditable(true)
        .with_dataset(Some(state_ref.clone()).into())
        .with_callbacks(
            vec![
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::FocusReceived),
                    refany: state_ref.clone(),
                    callback: CoreCallback {
                        cb: default_on_focus_received as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::FocusLost),
                    refany: state_ref.clone(),
                    callback: CoreCallback {
                        cb: default_on_focus_lost as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::TextInput),
                    refany: state_ref.clone(),
                    callback: CoreCallback {
                        cb: default_on_text_input as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                    refany: state_ref.clone(),
                    callback: CoreCallback {
                        cb: default_on_virtual_key_down as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseOver),
                    refany: state_ref,
                    callback: CoreCallback {
                        cb: default_on_mouse_hover as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
            ]
            .into(),
        )
        .with_children(
            vec![crate::widgets::widget_p()
                .with_ids_and_classes(vec![Class(TEXT_INPUT_LABEL_CLASS.into())].into())
                .with_css_props(CssPropertyWithConditionsVec::from_vec(label_style))
                .with_attribute(AttributeType::Placeholder(placeholder.into()))
                .with_children(DomVec::from_vec(vec![
                    Dom::create_text_do_not_use_without_block_level_wrapper(label_text),
                ]))]
            .into(),
        )
}

/// The flora label: the platform's label geometry, written in flora.css's
/// quiet ink (`--color-text-light: var(--fl-intro)`), with the night value
/// of the same token as its dark twin. A caller's `label_style` is taken as
/// it is.
#[must_use]
pub fn label(l: crate::widgets::label::Label) -> Dom {
    use azul_core::dom::{IdOrClass::Class, IdOrClassVec};
    use AzString;

    static LABEL_CLASS: &[IdOrClass] = &[
        Class(AzString::from_const_str("__azul-native-label")),
        Class(AzString::from_const_str("__azul-theme-flora")),
    ];

    // Resolved before `l.string` is moved out below.
    let owns_style = l.label_style.as_ref().is_none();
    let mut label_style: Vec<CssPropertyWithConditions> =
        l.resolved_label_style().as_slice().to_vec();
    if owns_style {
        // The platform table's ink (and its system dark twin) make way for
        // flora's: one ink per mode, not a flat grey overridden later.
        label_style.retain(|p| p.property.get_type() != CssPropertyType::TextColor);
        label_style.extend(super::decl::themed_ink(LIGHT_INTRO, DARK_INTRO));
    }

    crate::widgets::widget_p_with_text(l.string)
        .with_ids_and_classes(IdOrClassVec::from_const_slice(LABEL_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(label_style))
}

/// The flora switch (the design system's toggle): a pill-shaped trough of
/// field paper (`--fl-track` falling to `--fl-fld2`) in a `--fl-bd2` hairline,
/// sunk by the well; on, the trough fills with the stone (the accent falling
/// to its deep tone). The knob is a bead of paper in a `--fl-bd5` hairline,
/// casting a small shadow. The widget's geometry (and its slide) is kept: the
/// hairlines are inset shadows, so nothing moves. Focused, the double ring.
/// A caller's track or knob style is taken as it is.
#[must_use]
pub fn switch(s: crate::widgets::switch::Switch) -> Dom {
    use super::decl;
    let is_checked = s.switch_state.inner.checked;
    // Resolved up front: the knob's Dom is built after `s.switch_state` has
    // been moved into the callback's RefAny, and the resolver needs the whole
    // widget.
    let resolved_track_style = s.resolved_track_style();
    let resolved_knob_style = s.resolved_knob_style();
    let owns_track = s.track_style.as_ref().is_none();
    let owns_knob = s.knob_style.as_ref().is_none();
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{Dom, EventFilter, HoverEventFilter, IdOrClassVec, TabIndex},
    };

    let sw_name = s.accessibility_name.clone();
    crate::widgets::warn_widget_needs_a_name("switch", sw_name.is_some());

    let mut track_style = resolved_track_style.as_slice().to_vec();
    if owns_track {
        track_style.extend(CssPropertyWithConditions::themed(
            CssProperty::const_background_content(switch_track_face(is_checked, false)),
            CssProperty::const_background_content(switch_track_face(is_checked, true)),
        ));
        // The trough's hairline, drawn inside the box so the knob's travel
        // stays the widget's, and the well.
        track_style.extend(themed_shadow_in(
            ShadowSlot::Left,
            (0, 0, 1),
            if is_checked { LIGHT_DEEP } else { LIGHT_BD2 },
            if is_checked { LIGHT_DEEP } else { DARK_BD2 },
            true,
        ));
        track_style.extend(themed_shadow_in(
            ShadowSlot::Top,
            (1, 2, 0),
            ColorU::new(48, 45, 38, 31),
            ColorU::new(0, 0, 0, 115),
            true,
        ));
        track_style.extend(double_ring(2));
    }
    let mut knob_style = resolved_knob_style.as_slice().to_vec();
    if owns_knob {
        knob_style.push(CssPropertyWithConditions::simple(layers(alloc::vec![paper_bead()])));
        knob_style.push(CssPropertyWithConditions::simple(shadow_in(
            ShadowSlot::Left,
            0,
            0,
            1,
            LIGHT_BD5,
            true,
        )));
        knob_style.push(CssPropertyWithConditions::simple(shadow_in(
            ShadowSlot::Bottom,
            1,
            2,
            0,
            ColorU::new(48, 45, 38, 77),
            false,
        )));
    }

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from(
            crate::widgets::switch::SWITCH_TRACK_CLASS,
        ))
        .with_css_props(track_style.into())
        .with_callbacks(
            alloc::vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::Click),
                callback: CoreCallback {
                    cb: crate::widgets::switch::input::default_on_switch_clicked as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
                refany: RefAny::new(s.switch_state),
            }]
            .into(),
        )
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::CheckButton,
            accessibility_name: sw_name,
            states: azul_core::a11y::AccessibilityStateVec::from_vec(alloc::vec![
                if is_checked {
                    azul_core::a11y::AccessibilityState::CheckedTrue
                } else {
                    azul_core::a11y::AccessibilityState::CheckedFalse
                },
            ]),
            ..Default::default()
        })
        .with_children(
            alloc::vec![Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from(
                    crate::widgets::switch::SWITCH_KNOB_CLASS
                ))
                .with_css_props(knob_style.into())]
            .into(),
        )
}

// -----------------------------------------------------------------------------
// PROGRESSBAR
// -----------------------------------------------------------------------------

/// The flora bar, mounted in the widget's `VirtualView` wrapper (the same box
/// in every theme: `progressbar::mount`).
#[must_use]
pub fn progressbar(bar: crate::widgets::progressbar::ProgressBar) -> Dom {
    crate::widgets::progressbar::mount(bar, progressbar_render_virtual_view)
}

/// The render core behind [`ProgressBar::render_bar`] (percentage widths,
/// `bounds_px: None`) and the `VirtualView` callback (absolute pixel sizes
/// computed from the node's known bounds, `Some((width, height))`).
///
/// The split exists because the two contexts size differently. Percentages
/// inside a `VirtualView` DO resolve correctly against the view's bounds
/// (the child DOM lays out against its own viewport - it briefly resolved
/// against the WINDOW, fixed 2026-08-29, pinned by
/// `a_virtual_view_child_lays_out_against_the_view_bounds_not_the_window`),
/// but the bounds mode stays PIXEL-based for what percentages cannot
/// express: the track is sized to the bounds minus its 1px border ring so the
/// ring lands INSIDE the box - with the normal-flow sizing (content height +
/// borders) the ring overflowed the VV node and was clipped away at the right
/// and bottom ("oddly cut off", user report 2026-08-29) - and the fill is an
/// exact device-pixel split of the known content width.
///
/// Flora's bar (the design system's progress): a thin SUNKEN trough - a 5px
/// band of `--fl-track` in a `--fl-bd5` rule, its near lip shadowing it -
/// centred in the widget's box, filled with the stone sunk into it
/// (`linear-gradient(175deg, deep, acc 92%)`) and led by a 3px gold edge: the
/// gold only as the leading edge. A bar the caller painted
/// (`with_bar_background`, `with_container_background`) keeps its paint.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn progressbar_render_bar_impl(
    bar: crate::widgets::progressbar::ProgressBar,
    bounds_px: Option<(f32, f32)>,
) -> Dom {
    use azul_core::dom::DomVec;
    use super::decl;

    let this = bar;
    let defaults = crate::widgets::progressbar::ProgressBar::create(0.0);
    let own_bar = this.bar_background == defaults.bar_background;
    let own_container = this.container_background == defaults.container_background;
    let percent_done = this.progressbar_state.percent_done.clamp(0.0, 100.0);
    // The trough: 5px between its rules, centred in the box the widget gives.
    let (track_outer, margin_top) = match bounds_px {
        Some((_, h)) => {
            let outer = h.clamp(2.0, 7.0);
            (outer, ((h - outer) / 2.0).floor().max(0.0))
        }
        None => (7.0, 0.0),
    };
    let inner_w = bounds_px.map(|(w, _)| (w - 2.0).max(0.0));
    let (bar_width, remaining_width, filled_px) = match inner_w {
        Some(inner) => {
            let filled = inner * percent_done / 100.0;
            (PixelValue::px(filled), PixelValue::px(inner - filled), filled)
        }
        None => (
            PixelValue::percent(percent_done),
            PixelValue::percent(100.0 - percent_done),
            f32::INFINITY,
        ),
    };

    // .__azul-native-progress-bar-container: the widget's base (its
    // structure, the same in every theme), then flora's trough.
    let mut container_props = crate::widgets::progressbar::BAR_CONTAINER_BASE.to_vec();
    container_props.push(CssPropertyWithConditions::simple(CssProperty::Height(
        LayoutHeightValue::Exact(LayoutHeight::Px(PixelValue::px((track_outer - 2.0).max(0.0)))),
    )));
    container_props.push(CssPropertyWithConditions::simple(CssProperty::const_margin_top(
        LayoutMarginTop {
            inner: PixelValue::px(margin_top),
        },
    )));
    if let Some(inner) = inner_w {
        container_props.push(CssPropertyWithConditions::simple(CssProperty::Width(
            LayoutWidthValue::Exact(LayoutWidth::Px(PixelValue::px(inner))),
        )));
    }
    container_props.extend(decl::border(1));
    container_props.extend(decl::radius(2));
    container_props.extend(decl::themed_border_color(LIGHT_BD5, DARK_BD5));
    if own_container {
        container_props.extend(decl::themed_fill(LIGHT_TRACK, DARK_TRACK));
    } else {
        container_props.push(CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
            StyleBackgroundContentVecValue::Exact(this.container_background.clone()),
        )));
    }
    // The near lip's shadow falling into the trough.
    container_props.extend(themed_shadow_in(
        ShadowSlot::Top,
        (1, 2, 0),
        ColorU::new(48, 45, 38, 89),
        ColorU::new(0, 0, 0, 140),
        true,
    ));

    // .__azul-native-progress-bar-bar: the stone sunk into the trough, the
    // gold at its leading edge (once there is room for it).
    let mut bar_props = alloc::vec![CssPropertyWithConditions::simple(CssProperty::Width(
        LayoutWidthValue::Exact(LayoutWidth::Px(bar_width)),
    ))];
    if own_bar {
        bar_props.push(CssPropertyWithConditions::simple(layers(alloc::vec![
            StyleBackgroundContent::LinearGradient(LinearGradient {
                direction: deg(175),
                extend_mode: ExtendMode::Clamp,
                stops: NormalizedLinearColorStopVec::from_vec(alloc::vec![
                    stop(0, LIGHT_DEEP),
                    stop(92, LIGHT_ACC),
                ]),
            })
        ])));
        if filled_px >= 4.0 {
            bar_props.push(leading_edge());
        }
    } else {
        bar_props.push(CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
            StyleBackgroundContentVecValue::Exact(this.bar_background.clone()),
        )));
    }

    Dom::create_div()
        .with_css_props(CssPropertyWithConditionsVec::from_vec(container_props))
        .with_ids_and_classes({
            const CONTAINER_CLASSES: &[IdOrClass] = &[Class(AzString::from_const_str(
                "__azul-native-progress-bar-container",
            ))];
            IdOrClassVec::from_const_slice(CONTAINER_CLASSES)
        })
        // For a progress bar the VALUE is the content: two coloured divs
        // say nothing to a screen reader, "75%" says everything. Published
        // on every build so it tracks the bar; a callback that moves the
        // bar live without a rebuild keeps it current with
        // `CallbackInfo::set_accessibility_value` on this node.
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::ProgressBar,
            // What the bar measures - only the caller knows; see
            // `ProgressBar::with_accessibility_name`.
            accessibility_name: this.accessibility_name.clone(),
            accessibility_value: Some(AzString::from(alloc::format!(
                "{:.0}%",
                // NaN clamps to NaN and would read "NaN%"; an unknown
                // value announces as empty, like the bar it draws.
                if percent_done.is_finite() { percent_done } else { 0.0 }
            )))
            .into(),
            ..Default::default()
        })
        .with_children(DomVec::from_vec(vec![
            Dom::create_div()
                .with_css_props(CssPropertyWithConditionsVec::from_vec(bar_props))
                .with_ids_and_classes({
                    const BAR_CLASSES: &[IdOrClass] =
                        &[Class(AzString::from_const_str("__azul-native-progress-bar-bar"))];
                    IdOrClassVec::from_const_slice(BAR_CLASSES)
                }),
            Dom::create_div()
                .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
                    CssPropertyWithConditions::simple(CssProperty::Width(
                        LayoutWidthValue::Exact(LayoutWidth::Px(remaining_width)),
                    )),
                ]))
                .with_ids_and_classes({
                    const REMAINING_CLASSES: &[IdOrClass] = &[Class(AzString::from_const_str(
                        "__azul-native-progress-bar-remaining",
                    ))];
                    IdOrClassVec::from_const_slice(REMAINING_CLASSES)
                }),
        ]))
}

#[must_use]
/// The widget's `VirtualView` callback: render the CURRENT state of the bar
/// into the node's bounds. Invoked on mount and again every time
/// [`ProgressBar::update_progress`] queues a re-render.
///
/// The bar is not scrollable content, so all three rects collapse to one:
/// `materialized` == `virtual_rect` == the container's box at origin zero.
pub extern "C" fn progressbar_render_virtual_view(
    mut data: RefAny,
    info: VirtualViewCallbackInfo,
) -> VirtualViewReturn {
    let Some(state) = data.downcast_ref::<crate::widgets::progressbar::ProgressBarLocalDataset>()
    else {
        // Foreign payload: render nothing rather than lying about bounds.
        return VirtualViewReturn::default();
    };
    let size = info.bounds.get_logical_size();
    let rect = LogicalRect::new(LogicalPosition::zero(), size);
    // Clone-per-render is two enum copies + an `AzString`-less state copy; the
    // backgrounds are either `&'static` (shared, no alloc) or a caller-owned
    // heap vec that must be preserved for the NEXT render anyway. Pixel
    // widths, not percentages: the callback knows its bounds (see
    // `render_bar_impl`).
    VirtualViewReturn::with_dom(
        progressbar_render_bar_impl(state.bar.clone(), Some((size.width, size.height))),
        rect,
        rect,
    )
}

/// The slider's sunken track, as one layer over the 20px widget box: clear
/// above and below, a 1px `edge` on top, the `trough` (6px band, 8px-12px), a
/// 1px `foot` under it - hard stops, one gradient.
#[must_use]
fn slider_band(edge: ColorU, trough: ColorU, foot: ColorU) -> StyleBackgroundContent {
    let clear = ColorU::TRANSPARENT;
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: TO_BOTTOM,
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_vec(alloc::vec![
            stop(0, clear),
            stop(35, clear),
            stop(35, edge),
            stop(40, edge),
            stop(40, trough),
            stop(60, trough),
            stop(60, foot),
            stop(65, foot),
            stop(65, clear),
            stop(100, clear),
        ]),
    })
}

/// The gold that leads a filled track (the design system's leading edge,
/// `linear-gradient(#fedb37, #9f7928)`, here the leaf's own gold).
const LEADING_GOLD: ColorU = ColorU::rgb(0xD2, 0xB0, 0x52);

/// `inset -3px 0 0 <gold>`: a 3px gold band along a fill's right edge.
fn leading_edge() -> CssPropertyWithConditions {
    CssPropertyWithConditions::simple(CssProperty::box_shadow_right(StyleBoxShadow {
        offset_x: PixelValueNoPercent {
            inner: PixelValue::const_px(-3),
        },
        offset_y: PixelValueNoPercent {
            inner: PixelValue::const_px(0),
        },
        blur_radius: PixelValueNoPercent {
            inner: PixelValue::const_px(0),
        },
        spread_radius: PixelValueNoPercent {
            inner: PixelValue::const_px(0),
        },
        clip_mode: BoxShadowClipMode::Inset,
        color: LEADING_GOLD,
    }))
}

/// The filled part of a flora slider: the stone sunk into the trough
/// (`linear-gradient(175deg, deep 0%, acc 92%)`), its gold leading edge at the
/// thumb's centre, reaching left past the track's start (the track clips it)
/// - so it follows the thumb wherever the drag puts it.
#[must_use]
fn slider_fill() -> Dom {
    use super::decl;
    let mut v = Vec::with_capacity(8);
    v.push(decl::position(LayoutPosition::Absolute));
    v.push(CssPropertyWithConditions::simple(CssProperty::const_right(LayoutRight::const_px(
        8,
    ))));
    v.push(decl::px_top(6.0));
    v.push(decl::px_width(400.0));
    v.push(decl::px_height(4.0));
    v.push(CssPropertyWithConditions::simple(layers(alloc::vec![
        StyleBackgroundContent::LinearGradient(LinearGradient {
            direction: deg(175),
            extend_mode: ExtendMode::Clamp,
            stops: NormalizedLinearColorStopVec::from_vec(alloc::vec![
                stop(0, LIGHT_DEEP),
                stop(92, LIGHT_ACC),
            ]),
        })
    ])));
    v.push(leading_edge());
    Dom::create_div().with_css_props(CssPropertyWithConditionsVec::from_vec(v))
}

const DIAMOND_ROTATION: &[StyleTransform] = &[StyleTransform::Rotate(AngleValue::const_deg(45))];

/// The flora slider's thumb: a diamond of paper - a 14px square turned 45
/// degrees, `linear-gradient(135deg, #FDFAF1, #D8CDB6)` in a `--fl-soft2`
/// hairline, lit along its top edge and casting a small shadow.
#[must_use]
fn slider_diamond() -> Dom {
    use super::decl;
    let mut v = Vec::with_capacity(20);
    v.push(decl::position(LayoutPosition::Absolute));
    v.push(decl::px_top(1.0));
    v.push(decl::px_left(1.0));
    v.push(CssPropertyWithConditions::simple(CssProperty::const_box_sizing(
        LayoutBoxSizing::BorderBox,
    )));
    v.push(decl::px_width(14.0));
    v.push(decl::px_height(14.0));
    v.extend(decl::border(1));
    v.extend(decl::themed_border_color(LIGHT_SOFT2, DARK_SOFT2));
    v.push(CssPropertyWithConditions::simple(layers(alloc::vec![
        StyleBackgroundContent::LinearGradient(LinearGradient {
            direction: deg(135),
            extend_mode: ExtendMode::Clamp,
            stops: NormalizedLinearColorStopVec::from_vec(alloc::vec![
                stop(0, ColorU::rgb(0xFD, 0xFA, 0xF1)),
                stop(100, ColorU::rgb(0xD8, 0xCD, 0xB6)),
            ]),
        })
    ])));
    v.push(CssPropertyWithConditions::simple(shadow_in(
        ShadowSlot::Top,
        1,
        0,
        0,
        white(153),
        true,
    )));
    v.push(CssPropertyWithConditions::simple(shadow_in(
        ShadowSlot::Bottom,
        1,
        3,
        0,
        ColorU::new(48, 45, 38, 89),
        false,
    )));
    v.push(CssPropertyWithConditions::simple(CssProperty::const_transform(
        StyleTransformVec::from_const_slice(DIAMOND_ROTATION),
    )));
    Dom::create_div().with_css_props(CssPropertyWithConditionsVec::from_vec(v))
}

pub fn slider(slider: crate::widgets::slider::Slider) -> Dom {
    let value_now = slider.slider_state.inner.value;
    let a11y_name = slider.accessibility_name.clone();
    crate::widgets::warn_widget_needs_a_name("Slider", a11y_name.is_some());

    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{EventFilter, HoverEventFilter, TabIndex},
        refany::{OptionRefAny, RefAny},
    };

    // Resolved before `slider.slider_state` is moved out below; the resolvers
    // borrow `&slider`, and the thumb's margin is derived from the state.
    let resolved_track_style = slider.resolved_track_style();
    let resolved_thumb_style = slider.resolved_thumb_style();
    // A part the caller styled (`Some`) is the caller's: no theme paint on it.
    let track_is_callers = slider.track_style.is_some();
    let thumb_is_callers = slider.thumb_style.is_some();

    let state = RefAny::new(slider.slider_state);
    let mk = |event: EventFilter, cb: usize| CoreCallbackData {
        event,
        callback: CoreCallback {
            cb,
            ctx: OptionRefAny::None,
        },
        refany: state.clone(),
    };
    let callbacks = vec![
        mk(
            EventFilter::Hover(HoverEventFilter::MouseDown),
            crate::widgets::slider::on_slider_pointer_down as usize,
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::MouseMove),
            crate::widgets::slider::on_slider_pointer_move as usize,
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            crate::widgets::slider::on_slider_pointer_up as usize,
        ),
        mk(
            EventFilter::Focus(azul_core::events::FocusEventFilter::VirtualKeyDown),
            crate::widgets::slider::on_slider_key as usize,
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::MouseLeave),
            crate::widgets::slider::on_slider_pointer_leave as usize,
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::TouchStart),
            crate::widgets::slider::on_slider_pointer_down as usize,
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::TouchMove),
            crate::widgets::slider::on_slider_pointer_move as usize,
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::TouchEnd),
            crate::widgets::slider::on_slider_pointer_up as usize,
        ),
    ];

    let mut track_style = resolved_track_style.as_slice().to_vec();
    let mut thumb_style = resolved_thumb_style.as_slice().to_vec();

    // Flora (the design system's slider): a thin sunken track - a 6px band of
    // `--fl-track` between a `--fl-bd5` top edge and a `--fl-bd4` bottom one,
    // drawn as the widget box's background so the box stays the whole hit
    // area - filled left of the thumb with the stone, a gold leading edge at
    // its end, under a DIAMOND of paper (a square turned 45 degrees). The
    // fill hangs off the thumb (which the drag moves by its margin) and is
    // clipped by the track box. Only on the widget's own parts: a style the
    // caller set is the caller's (the status bar's zoom slider draws its own
    // rail and thumb).
    let mut thumb_children = Vec::new();
    if !track_is_callers {
        track_style.push(super::decl::px_height(20.0));
        track_style.extend(super::decl::radius(2));
        track_style.push(CssPropertyWithConditions::simple(CssProperty::const_overflow_x(
            LayoutOverflow::Hidden,
        )));
        track_style.push(CssPropertyWithConditions::simple(CssProperty::const_overflow_y(
            LayoutOverflow::Hidden,
        )));
        track_style.extend(CssPropertyWithConditions::themed(
            layers(alloc::vec![slider_band(LIGHT_BD5, LIGHT_TRACK, LIGHT_BD4)]),
            layers(alloc::vec![slider_band(DARK_BD5, DARK_TRACK, DARK_BD4)]),
        ));
        track_style.extend(double_ring(2));
    }
    if !thumb_is_callers {
        thumb_style.push(super::decl::position(LayoutPosition::Relative));
        thumb_style.push(CssPropertyWithConditions::simple(super::decl::fill(
            ColorU::TRANSPARENT,
        )));
        thumb_children.push(slider_fill());
        thumb_children.push(slider_diamond());
    }

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(
            AzString::from_const_str("__azul-native-slider"),
        )]))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(track_style))
        .with_callbacks(callbacks.into())
        .with_dataset(OptionRefAny::Some(state))
        .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(
            crate::widgets::slider::merge_slider_state,
        ))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Slider,
            accessibility_name: a11y_name,
            accessibility_value: Some(AzString::from(alloc::format!("{value_now}"))).into(),
            ..Default::default()
        })
        .with_children(
            vec![Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(
                    AzString::from_const_str("__azul-native-slider-thumb"),
                )]))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(thumb_style))
                .with_children(thumb_children.into())]
            .into(),
        )
}

#[must_use]
pub fn text_area(mut ta: crate::widgets::text_area::TextArea) -> Dom {
    let ta_name: Option<AzString> = ta.text_area_state.inner.placeholder.as_ref().cloned();

    use azul_core::dom::{
        AttributeType, DomVec, EventFilter, FocusEventFilter, IdOrClass::Class, TabIndex,
    };

    ta.text_area_state.inner.cursor_pos = ta.text_area_state.inner.text.len();

    // Resolved before `ta.text_area_state` is moved out below, and through the
    // widget's resolver rather than a second copy of its default.
    let resolved_container_style = ta.resolved_container_style();

    let label_text: String = ta
        .text_area_state
        .inner
        .text
        .iter()
        .filter_map(|s| core::char::from_u32(*s))
        .collect();

    let placeholder = ta
        .text_area_state
        .inner
        .placeholder
        .as_ref()
        .map(|s| s.as_str().to_string())
        .unwrap_or_default();

    let state_ref = RefAny::new(ta.text_area_state);

    let owns_container = ta.container_style.as_ref().is_none();
    let mut container_style: Vec<CssPropertyWithConditions> =
        resolved_container_style.as_slice().to_vec();
    let mut label_style: Vec<CssPropertyWithConditions> = match &ta.label_style {
        azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::Some(s) => {
            s.as_slice().to_vec()
        }
        azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::None => {
            let mut v = crate::widgets::text_area::TEXT_AREA_LABEL_PROPS.to_vec();
            // The text is written in Garamond, in the house ink.
            v.retain(|p| p.property.get_type() != CssPropertyType::FontFamily);
            v.push(CssPropertyWithConditions::simple(CssProperty::const_font_family(
                SERIF_FAMILY,
            )));
            v.extend(super::decl::themed_ink(LIGHT_INK, DARK_INK));
            v
        }
    };
    if owns_container {
        // The design system's field (see `field_skin`), the states last.
        container_style.extend(field_skin());
        container_style.extend(field_states());
    } else {
        for twin in [
            CssProperty::BackgroundContent(
                StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_SUR)])
                    .into(),
            ),
            CssProperty::TextColor(StyleTextColor { inner: DARK_INK }.into()),
            CssProperty::BorderTopColor(StyleBorderTopColor { inner: DARK_BD }.into()),
            CssProperty::BorderBottomColor(StyleBorderBottomColor { inner: DARK_BD }.into()),
            CssProperty::BorderLeftColor(StyleBorderLeftColor { inner: DARK_BD }.into()),
            CssProperty::BorderRightColor(StyleBorderRightColor { inner: DARK_BD }.into()),
        ] {
            super::decl::push_dark_twin(
                &mut container_style,
                CssPropertyWithConditions::dark_mode(twin),
            );
        }
        super::decl::push_dark_twin(
            &mut label_style,
            CssPropertyWithConditions::dark_mode(CssProperty::TextColor(
                StyleTextColor { inner: DARK_INK }.into(),
            )),
        );
        // The interactive states go LAST (last match wins).
        container_style.extend_from_slice(&FIELD_BORDER_STATES);
    }

    Dom::create_div()
        .with_ids_and_classes(vec![Class("__azul-native-text-area-container".into())].into())
        .with_css_props(CssPropertyWithConditionsVec::from_vec(container_style))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Text,
            accessibility_name: ta_name.into(),
            ..Default::default()
        })
        .with_contenteditable(true)
        .with_dataset(Some(state_ref.clone()).into())
        .with_callbacks(
            vec![
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::FocusReceived),
                    refany: state_ref.clone(),
                    callback: azul_core::callbacks::CoreCallback {
                        cb: crate::widgets::text_area::default_on_focus_received as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::FocusLost),
                    refany: state_ref.clone(),
                    callback: azul_core::callbacks::CoreCallback {
                        cb: crate::widgets::text_area::default_on_focus_lost as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::TextInput),
                    refany: state_ref.clone(),
                    callback: azul_core::callbacks::CoreCallback {
                        cb: crate::widgets::text_area::default_on_text_input as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                    refany: state_ref,
                    callback: azul_core::callbacks::CoreCallback {
                        cb: crate::widgets::text_area::default_on_virtual_key_down as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
            ]
            .into(),
        )
        .with_children(
            vec![crate::widgets::widget_p()
                .with_ids_and_classes(vec![Class("__azul-native-text-area-label".into())].into())
                .with_css_props(CssPropertyWithConditionsVec::from_vec(label_style))
                .with_attribute(AttributeType::Placeholder(placeholder.into()))
                .with_children(DomVec::from_vec(vec![
                    Dom::create_text_do_not_use_without_block_level_wrapper(label_text),
                ]))]
            .into(),
        )
}
const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

/// Flora's closed drop-down, after `drop_down::DROPDOWN_WRAPPER_BASE` (R5):
/// the design system's field - field paper in a `--fl-bd2` rule at the house
/// radius, sunk by the well - holding the choice in Garamond, with the field
/// states (the rule darkening under the pointer, the accent edge and the
/// double ring when focused). The list it opens is the menu
/// (`drop_down::on_dropdown_click`).
#[must_use]
pub(crate) fn flora_dropdown_wrapper_style() -> Vec<CssPropertyWithConditions> {
    let mut v = Vec::with_capacity(48);
    v.push(CssPropertyWithConditions::simple(CssProperty::const_font_size(
        StyleFontSize::const_px(14),
    )));
    v.push(CssPropertyWithConditions::simple(CssProperty::const_font_family(SERIF_FAMILY)));
    v.extend(super::decl::padding(3, 6, 3, 8));
    v.extend(field_skin());
    v.extend(field_states());
    v
}

/// Flora's label skin, after `drop_down::DROPDOWN_LABEL_BASE` (R5).
const FLORA_DROPDOWN_LABEL_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(10),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: LIGHT_INK,
    })),
    CssPropertyWithConditions::dark_mode(CssProperty::const_text_color(StyleTextColor {
        inner: DARK_INK,
    })),
];

/// Flora's arrow skin, after `drop_down::DROPDOWN_ARROW_BASE` (R5): the small
/// drop mark in the quiet ink (`--fl-soft2`).
const FLORA_DROPDOWN_ARROW_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(16))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: LIGHT_SOFT2,
    })),
    CssPropertyWithConditions::dark_mode(CssProperty::const_text_color(StyleTextColor {
        inner: DARK_SOFT2,
    })),
];

#[must_use]
pub fn drop_down(dd: crate::widgets::drop_down::DropDown) -> Dom {
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{
            Dom, DomVec, EventFilter, FocusEventFilter, IdOrClass::Class, IdOrClassVec, TabIndex,
        },
        refany::RefAny,
    };
    use azul_css::AzString;

    let selected_label: Option<AzString> = dd
        .choices
        .as_ref()
        .get(dd.selected)
        .map(|o| AzString::from(o.as_str().to_string()));

    const DROPDOWN_CLASS: &[IdOrClass] =
        &[Class(AzString::from_const_str("__azul-native-dropdown"))];

    let selected_text = dd
        .choices
        .as_slice()
        .get(dd.selected)
        .cloned()
        .unwrap_or_else(|| AzString::from_const_str(""));

    let refany = RefAny::new(dd);

    // Every part: the widget's structure (R5), then flora's skin.
    use crate::widgets::drop_down::{
        DROPDOWN_ARROW_BASE, DROPDOWN_LABEL_BASE, DROPDOWN_WRAPPER_BASE,
    };

    Dom::create_div()
        .with_css_props(CssPropertyWithConditionsVec::from_vec(
            [DROPDOWN_WRAPPER_BASE, flora_dropdown_wrapper_style().as_slice()].concat(),
        ))
        .with_ids_and_classes(IdOrClassVec::from_const_slice(DROPDOWN_CLASS))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::ComboBox,
            accessibility_value: selected_label.into(),
            ..Default::default()
        })
        .with_callbacks(
            vec![CoreCallbackData {
                event: EventFilter::Focus(FocusEventFilter::FocusReceived),
                refany,
                callback: CoreCallback {
                    cb: crate::widgets::drop_down::on_dropdown_click as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
            }]
            .into(),
        )
        .with_children(DomVec::from_vec(vec![
            crate::widgets::widget_p_chrome()
                .with_css_props(CssPropertyWithConditionsVec::from_vec(
                    [DROPDOWN_LABEL_BASE, FLORA_DROPDOWN_LABEL_STYLE].concat(),
                ))
                .with_children(DomVec::from_vec(vec![
                    Dom::create_text_do_not_use_without_block_level_wrapper(selected_text),
                ])),
            Dom::create_icon(AzString::from_const_str("arrow_drop_down")).with_css_props(
                CssPropertyWithConditionsVec::from_vec(
                    [DROPDOWN_ARROW_BASE, FLORA_DROPDOWN_ARROW_STYLE].concat(),
                ),
            ),
        ]))
}

#[must_use]
pub fn avatar(a: crate::widgets::avatar::Avatar) -> Dom {
    use azul_core::dom::{Dom, IdOrClassVec};
    let size = a.size;
    let child = match a.image.into_option() {
        Some(image) => Dom::create_image(image)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(
                crate::widgets::avatar::AVATAR_IMAGE_CLASS,
            ))
            .with_css_props(crate::widgets::avatar::build_image_style(size)),
        None => crate::widgets::widget_p_with_text(a.initials).with_ids_and_classes(
            IdOrClassVec::from_const_slice(crate::widgets::avatar::AVATAR_INITIALS_CLASS),
        ),
    };

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(
            crate::widgets::avatar::AVATAR_CLASS,
        ))
        .with_css_props(
            match a.avatar_style {
                azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::Some(style) => {
                    style.as_slice().to_vec()
                }
                azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::None => {
                    crate::widgets::avatar::build_avatar_style(size)
                        .as_slice()
                        .to_vec()
                }
            }
            .into(),
        )
        .with_children(alloc::vec![child].into())
}

// ---------------------------------------------------------------------------
// INTERACTIVE STATES
// ---------------------------------------------------------------------------
//
// The same vocabulary flat.rs declares, in flora's palette. Both themes expose
// these names so a widget's theme function reads identically in either.

/// The focus ring: the focused control's border takes the accent colour.
///
/// One const per edge — a border colour is four properties, and a ring that sets
/// only some of them leaves the rest at their resting colour. Each has a dark
/// twin in [`DARK_GLOW`], flora.css's night `--focus-color`: the accent stone
/// itself (#2F4A85) stands only 1.8:1 off the night leaf, so at night the ring
/// lifts to the stone's highlight, which clears 3:1 on every night surface
/// (`night_focus_ring_tests`).
pub const FOCUS_BORDER_TOP: CssPropertyWithConditions =
    CssPropertyWithConditions::on_focus(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: LIGHT_ACC,
    }));

/// See [`FOCUS_BORDER_TOP`].
pub const FOCUS_BORDER_BOTTOM: CssPropertyWithConditions = CssPropertyWithConditions::on_focus(
    CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: LIGHT_ACC }),
);

/// See [`FOCUS_BORDER_TOP`].
pub const FOCUS_BORDER_LEFT: CssPropertyWithConditions = CssPropertyWithConditions::on_focus(
    CssProperty::const_border_left_color(StyleBorderLeftColor { inner: LIGHT_ACC }),
);

/// See [`FOCUS_BORDER_TOP`].
pub const FOCUS_BORDER_RIGHT: CssPropertyWithConditions = CssPropertyWithConditions::on_focus(
    CssProperty::const_border_right_color(StyleBorderRightColor { inner: LIGHT_ACC }),
);

/// The dark twin of [`FOCUS_BORDER_TOP`].
pub const FOCUS_BORDER_TOP_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_top_color(
        StyleBorderTopColor { inner: DARK_GLOW },
    ));

/// The dark twin of [`FOCUS_BORDER_BOTTOM`].
pub const FOCUS_BORDER_BOTTOM_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor { inner: DARK_GLOW },
    ));

/// The dark twin of [`FOCUS_BORDER_LEFT`].
pub const FOCUS_BORDER_LEFT_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_left_color(
        StyleBorderLeftColor { inner: DARK_GLOW },
    ));

/// The dark twin of [`FOCUS_BORDER_RIGHT`].
pub const FOCUS_BORDER_RIGHT_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_right_color(
        StyleBorderRightColor { inner: DARK_GLOW },
    ));

// ---------------------------------------------------------------------------
// INTERACTIVE STATES — text fields
// ---------------------------------------------------------------------------

/// Border colour on hover, one const per edge, light mode.
///
/// A border colour is four properties, so a state that sets only some edges
/// leaves the rest at their resting colour — which is why these travel as a set
/// rather than individually.
pub const HOVER_BORDER_TOP: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: LIGHT_ACC,
    }));

/// See [`HOVER_BORDER_TOP`].
pub const HOVER_BORDER_BOTTOM: CssPropertyWithConditions = CssPropertyWithConditions::on_hover(
    CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: LIGHT_ACC }),
);

/// See [`HOVER_BORDER_TOP`].
pub const HOVER_BORDER_LEFT: CssPropertyWithConditions = CssPropertyWithConditions::on_hover(
    CssProperty::const_border_left_color(StyleBorderLeftColor { inner: LIGHT_ACC }),
);

/// See [`HOVER_BORDER_TOP`].
pub const HOVER_BORDER_RIGHT: CssPropertyWithConditions = CssPropertyWithConditions::on_hover(
    CssProperty::const_border_right_color(StyleBorderRightColor { inner: LIGHT_ACC }),
);

/// The dark twin of [`HOVER_BORDER_TOP`].
pub const HOVER_BORDER_TOP_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_top_color(
        StyleBorderTopColor { inner: DARK_ACC },
    ));

/// The dark twin of [`HOVER_BORDER_BOTTOM`].
pub const HOVER_BORDER_BOTTOM_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor { inner: DARK_ACC },
    ));

/// The dark twin of [`HOVER_BORDER_LEFT`].
pub const HOVER_BORDER_LEFT_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_left_color(
        StyleBorderLeftColor { inner: DARK_ACC },
    ));

/// The dark twin of [`HOVER_BORDER_RIGHT`].
pub const HOVER_BORDER_RIGHT_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_right_color(
        StyleBorderRightColor { inner: DARK_ACC },
    ));

/// Every border state a text field takes: the accent on hover and on focus, each
/// edge, each with its dark twin.
///
/// One array so a theme function appends the whole set in a line and cannot ship
/// half of it. This is what a widget file used to declare itself, in light mode
/// only — the reason a hovered field kept its light-blue ring on a dark surface.
pub const FIELD_BORDER_STATES: [CssPropertyWithConditions; 16] = [
    HOVER_BORDER_TOP,
    HOVER_BORDER_BOTTOM,
    HOVER_BORDER_LEFT,
    HOVER_BORDER_RIGHT,
    HOVER_BORDER_TOP_DARK,
    HOVER_BORDER_BOTTOM_DARK,
    HOVER_BORDER_LEFT_DARK,
    HOVER_BORDER_RIGHT_DARK,
    FOCUS_BORDER_TOP,
    FOCUS_BORDER_BOTTOM,
    FOCUS_BORDER_LEFT,
    FOCUS_BORDER_RIGHT,
    FOCUS_BORDER_TOP_DARK,
    FOCUS_BORDER_BOTTOM_DARK,
    FOCUS_BORDER_LEFT_DARK,
    FOCUS_BORDER_RIGHT_DARK,
];

/// Every state a flora command of one type takes - hover, pressed and the
/// keyboard ring - exactly as [`button`] appends them after its resting
/// face ([`FloraButtonKind`]: raised paper lifts and sinks, a stone brightens
/// with the metal coming up and sinks into its well, a quiet note darkens its
/// ink). For a labelled command; an icon-only quiet one has no wash.
#[must_use]
pub fn button_states(
    button_type: crate::widgets::button::ButtonType,
) -> Vec<CssPropertyWithConditions> {
    let mut out = flora_button_states(FloraButtonKind::of(button_type), true);
    out.extend(double_ring(2));
    out
}

/// A hover fill with BOTH halves chosen by the caller.
///
/// For widgets that carry their own palette (`RibbonTheme`, `StatusBarTheme`,
/// ...). The rule for picking `dark`: a surface that is its own colour — a blue
/// nav, a coloured button — keeps its light hover colour, because the surface
/// does not change in dark mode either; a page-neutral surface takes this
/// theme's [`DARK_HT`]. See `button_states` for the same rule applied.
#[must_use]
pub fn hover_bg_both(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 2] {
    let bg = |c: ColorU| {
        CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(alloc::vec![
            StyleBackgroundContent::Color(c),
        ]))
    };
    [
        CssPropertyWithConditions::on_hover(bg(light)),
        CssPropertyWithConditions::dark_on_hover(bg(dark)),
    ]
}

/// A pressed fill with BOTH halves chosen by the caller — see [`hover_bg_both`].
#[must_use]
pub fn active_bg_both(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 2] {
    let bg = |c: ColorU| {
        CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(alloc::vec![
            StyleBackgroundContent::Color(c),
        ]))
    };
    [
        CssPropertyWithConditions::on_active(bg(light)),
        CssPropertyWithConditions::dark_on_active(bg(dark)),
    ]
}

// The PHASE 2 anchor block below stays LAST (the plan's parallel-work
// contract), and the consts the other migrations place in it are items after
// this module — which is exactly what `items_after_test_module` objects to.
#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod gradient_tests {
    //! Phase 3 of `doc/WIDGET_THEME_MIGRATION.md`: the raised / hover / pressed
    //! faces are gradients built from the stop tokens, and the controls this
    //! module renders carry them — a gradient defined and never declared on a
    //! node would be the same smell as a token referenced only by itself.

    use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType, ThemeCondition};

    use super::*;
    use crate::widgets::{
        button::{Button, ButtonType},
        slider::Slider,
        themes::{OptionUiTheme, UiTheme},
    };

    /// `(offset %, colour)` per stop, in order. Panics on anything but a linear
    /// gradient of concrete colours, which every gradient here is.
    fn stops_of(bg: &StyleBackgroundContent) -> Vec<(PercentageValue, ColorU)> {
        let StyleBackgroundContent::LinearGradient(g) = bg else {
            panic!("not a linear gradient: {bg:?}");
        };
        g.stops
            .as_ref()
            .iter()
            .map(|s| match s.color {
                ColorOrSystem::Color(c) => (s.offset, c),
                ColorOrSystem::System(_) => panic!("a system colour in a transcribed stop"),
            })
            .collect()
    }

    /// The layer lists of every `background` declared on the root whose
    /// conditions satisfy `pred`, in declaration order — the last one wins.
    fn backgrounds_where(
        dom: &Dom,
        pred: impl Fn(&[DynamicSelector]) -> bool,
    ) -> Vec<Vec<StyleBackgroundContent>> {
        dom.root
            .style
            .iter_inline_properties()
            .filter(|(_, c)| pred(c.as_ref()))
            .filter_map(|(p, _)| match p {
                CssProperty::BackgroundContent(b) => b.get_property().map(|b| b.as_ref().to_vec()),
                _ => None,
            })
            .collect()
    }

    /// The resting background: the last unconditional declaration.
    fn resting_background(dom: &Dom) -> Option<Vec<StyleBackgroundContent>> {
        backgrounds_where(dom, <[DynamicSelector]>::is_empty).pop()
    }

    /// The dark-mode resting background: gated on the dark theme and nothing
    /// else — a `dark_on_hover` rule is not it.
    fn dark_resting_background(dom: &Dom) -> Option<Vec<StyleBackgroundContent>> {
        backgrounds_where(dom, |c| {
            matches!(c, [DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)])
        })
        .pop()
    }

    /// The background for `state`, in light mode (`dark == false`: no theme
    /// condition) or dark mode (`dark == true`: the dark condition as well).
    fn state_background(
        dom: &Dom,
        state: PseudoStateType,
        dark: bool,
    ) -> Option<Vec<StyleBackgroundContent>> {
        backgrounds_where(dom, |c| {
            c.iter()
                .any(|s| matches!(s, DynamicSelector::PseudoState(st) if *st == state))
                && c.iter()
                    .any(|s| matches!(s, DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)))
                    == dark
        })
        .pop()
    }

    fn flora_button(label: &'static str, ty: ButtonType) -> Button {
        let mut b = Button::with_type(AzString::from_const_str(label), ty);
        b.theme = OptionUiTheme::Some(UiTheme::Flora);
        b
    }

    #[test]
    fn each_face_is_a_two_stop_vertical_gradient_from_its_top_token_to_its_bottom_token() {
        let faces = [
            ("RAISED_FACE_LIGHT", RAISED_FACE_LIGHT, LIGHT_RT, LIGHT_RB),
            ("RAISED_FACE_DARK", RAISED_FACE_DARK, DARK_RT, DARK_RB),
            ("HOVER_FACE_LIGHT", HOVER_FACE_LIGHT, LIGHT_HT, LIGHT_HB),
            ("HOVER_FACE_DARK", HOVER_FACE_DARK, DARK_HT, DARK_HB),
            ("PRESSED_FACE_LIGHT", PRESSED_FACE_LIGHT, LIGHT_PT, LIGHT_PB),
            ("PRESSED_FACE_DARK", PRESSED_FACE_DARK, DARK_PT, DARK_PB),
        ];
        for (name, face, top, bottom) in faces {
            let StyleBackgroundContent::LinearGradient(g) = &face else {
                panic!("{name} is not a linear gradient: {face:?}");
            };
            assert_eq!(
                g.direction, TO_BOTTOM,
                "{name}: `linear-gradient(a, b)` has no angle, so it runs top to bottom"
            );
            assert_eq!(g.extend_mode, ExtendMode::Clamp, "{name}");
            assert_eq!(
                stops_of(&face),
                vec![
                    (PercentageValue::const_new(0), top),
                    (PercentageValue::const_new(100), bottom),
                ],
                "{name}: two stops, the top token then the bottom token"
            );
            // flat.rs's pairs are equal values; flora's are not, or the
            // "gradient" would be a flat fill with extra steps.
            assert_ne!(top, bottom, "{name}: a flora face has two different stops");
        }
    }

    #[test]
    fn the_overlays_never_hide_the_colour_beneath_them() {
        // The rig and the gloss are laid OVER a command's own colour, so every
        // stop must be translucent and each one must fade to nothing somewhere.
        for (name, overlay) in [
            ("STONE_RIG_TOP", STONE_RIG_TOP),
            ("STONE_RIG_LEFT", STONE_RIG_LEFT),
            ("STONE_STREAK", STONE_STREAK),
            ("STONE_STREAK_HOVER", STONE_STREAK_HOVER),
            ("SUNKEN_RIG_TOP", SUNKEN_RIG_TOP),
            ("SUNKEN_RIG_LEFT", SUNKEN_RIG_LEFT),
            ("SUNKEN_RIG_BOTTOM", SUNKEN_RIG_BOTTOM),
            ("ORB_GLOSS", ORB_GLOSS),
        ] {
            let stops = stops_of(&overlay);
            assert!(stops.len() >= 2, "{name}: fewer than two stops");
            assert!(
                stops.iter().all(|(_, c)| c.a < 255),
                "{name}: an opaque stop would hide the colour beneath: {stops:?}"
            );
            assert!(
                stops.iter().any(|(_, c)| c.a < 32),
                "{name}: an overlay fades out somewhere: {stops:?}"
            );
        }
    }

    #[test]
    fn a_flora_default_button_rests_on_the_raised_paper_face_in_both_modes() {
        let dom = button(Button::with_type(
            AzString::from_const_str("OK"),
            ButtonType::Default,
        ));
        let light = resting_background(&dom).expect("the button declares a resting background");
        assert!(
            matches!(
                light.first(),
                Some(StyleBackgroundContent::LinearGradient(_))
            ),
            "the resting face is a flat Color, not flora's raised paper: {light:?}"
        );
        assert_eq!(light, vec![RAISED_FACE_LIGHT]);
        assert_eq!(dark_resting_background(&dom), Some(vec![RAISED_FACE_DARK]));
    }

    #[test]
    fn the_rendered_flora_default_button_hovers_and_presses_to_the_face_gradients() {
        // `Button::dom` is the path that renders in the product: it builds its
        // own tree and appends `button_states` for the theme the button carries.
        let dom = flora_button("OK", ButtonType::Default).dom();
        assert_eq!(
            state_background(&dom, PseudoStateType::Hover, false),
            Some(vec![HOVER_FACE_LIGHT])
        );
        assert_eq!(
            state_background(&dom, PseudoStateType::Hover, true),
            Some(vec![HOVER_FACE_DARK])
        );
        assert_eq!(
            state_background(&dom, PseudoStateType::Active, false),
            Some(vec![PRESSED_FACE_LIGHT])
        );
        assert_eq!(
            state_background(&dom, PseudoStateType::Active, true),
            Some(vec![PRESSED_FACE_DARK])
        );
    }

    #[test]
    fn a_primary_button_is_the_accent_gem_and_sinks_into_its_well_when_pressed() {
        let dom = button(Button::with_type(
            AzString::from_const_str("Go"),
            ButtonType::Primary,
        ));
        assert_eq!(
            resting_background(&dom),
            Some(raised_stone(STONE_ACCENT, false)),
            "the accent stone, its rig over it, in the theme's accent - never the button's own colour"
        );
        assert!(
            matches!(
                resting_background(&dom).as_deref(),
                Some([StyleBackgroundContent::RadialGradient(_), ..])
            ),
            "--fl-gem is a radial cut lit at the upper left"
        );
        assert_eq!(
            dark_resting_background(&dom),
            None,
            "a stone keeps its colour in dark mode, so it needs no dark override"
        );

        let dom = flora_button("Go", ButtonType::Primary).dom();
        assert_eq!(
            state_background(&dom, PseudoStateType::Hover, false),
            Some(raised_stone(STONE_ACCENT, true)),
            "hovered, the same layers with the brighter streak - so the fade tweens"
        );
        assert_eq!(
            state_background(&dom, PseudoStateType::Active, false),
            Some(sunken_stone(STONE_ACCENT))
        );
    }

    #[test]
    fn the_semantic_types_are_stones_of_their_own_and_illuminated_is_paper_in_metal() {
        for (ty, stone) in [
            (ButtonType::Success, STONE_LEAF),
            (ButtonType::Danger, STONE_CLAY),
            (ButtonType::Warning, STONE_AMBER),
            (ButtonType::Info, STONE_SLATE),
        ] {
            let dom = button(Button::with_type(AzString::from_const_str("Go"), ty));
            assert_eq!(resting_background(&dom), Some(raised_stone(stone, false)), "{ty:?}");
        }
        let dom = button(Button::with_type(
            AzString::from_const_str("Illuminate"),
            ButtonType::Illuminated,
        ));
        assert_eq!(resting_background(&dom), Some(vec![RAISED_FACE_LIGHT]));
        let top = dom
            .root
            .style
            .iter_inline_properties()
            .filter(|(_, c)| c.as_ref().is_empty())
            .filter_map(|(p, _)| match p {
                CssProperty::BorderTopColor(c) => c.get_property().map(|c| c.inner),
                _ => None,
            })
            .last();
        assert_eq!(top, Some(LEAF_EDGE_LIGHT[0]), "the gold stays in the border");
    }

    #[test]
    fn the_quiet_button_is_brass_ink_on_the_faintest_paper_and_takes_no_stone() {
        let dom = button(Button::with_type(
            AzString::from_const_str("more"),
            ButtonType::Link,
        ));
        assert_eq!(
            resting_background(&dom),
            Some(vec![super::super::decl::face(LIGHT_RT, LIGHT_FLD2)]),
            ".btn-quiet: --fl-rT falling to --fl-fld2"
        );
        let dom = flora_button("more", ButtonType::Link).dom();
        assert_eq!(
            state_background(&dom, PseudoStateType::Active, false),
            None,
            "a quiet note does not press in"
        );
    }

    #[test]
    fn the_closed_dropdown_is_field_paper_in_both_modes() {
        let backgrounds: Vec<(bool, Vec<StyleBackgroundContent>)> = flora_dropdown_wrapper_style()
            .iter()
            .filter(|p| p.pseudo_state_conditions().is_empty())
            .filter_map(|p| match &p.property {
                CssProperty::BackgroundContent(b) => Some((
                    matches!(
                        p.apply_if.as_ref(),
                        [DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)]
                    ),
                    b.get_property()?.as_ref().to_vec(),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            backgrounds,
            vec![
                (false, vec![StyleBackgroundContent::Color(LIGHT_FLD)]),
                (true, vec![StyleBackgroundContent::Color(DARK_FLD)]),
            ]
        );
    }

    #[test]
    fn the_slider_thumb_is_a_paper_diamond_leading_the_filled_track() {
        let dom = slider(Slider::create(50.0, 0.0, 100.0));
        let thumb = &dom.children.as_ref()[0];
        assert_eq!(
            resting_background(thumb),
            Some(vec![StyleBackgroundContent::Color(ColorU::TRANSPARENT)]),
            "the thumb box only carries the fill and the diamond"
        );
        let parts = thumb.children.as_ref();
        assert_eq!(parts.len(), 2, "the fill, then the diamond over it");
        let turned = parts[1].root.style.iter_inline_properties().any(|(p, _)| {
            matches!(p, CssProperty::Transform(t)
                if t.get_property().is_some_and(|t| t.as_ref() == DIAMOND_ROTATION))
        });
        assert!(turned, "the diamond is a square turned 45 degrees");
        let fill = resting_background(&parts[0]).expect("the fill is painted");
        assert!(
            stops_of(&fill[0]).iter().any(|(_, c)| *c == LIGHT_ACC),
            "the fill is the accent stone: {fill:?}"
        );
    }
}

// ===========================================================================
// PHASE 2 — per-widget state sections. Each widget's interactive-state consts
// live between its own pair of markers and nowhere else, so that the
// migrations can proceed in parallel without touching one another's lines.
// ===========================================================================

// == STATES: list_view ==
// == /STATES: list_view ==

//
//
//

// == STATES: tabs ==
// == /STATES: tabs ==

//
//
//

// == STATES: text_input ==
//
// text_input declares no states of its own: it takes `FIELD_BORDER_STATES`
// (the text-fields section above), because its eight rules were the same
// accent ring on hover and on focus that text_area had, and `text_input()`
// appends that array rather than a second copy of it. The one difference the
// widget file used to make — a grey hover ring on Windows only — was a
// platform distinction neither theme draws, so it went with the move.
//
// == /STATES: text_input ==

//
//
//

// == STATES: chrome ==
// == /STATES: chrome ==

//
//
//

// ==== scrollbars ====
//
// "The scrollbar thumb is undyed wool on a parchment track. It widens
// nowhere, glows never" (the design system's scrollbar card): the wool is
// flora.css's `--fl-sbA` falling to `--fl-sbB` and the track `--fl-track`.
// azul draws a scrollbar part in one colour (`getters::get_scrollbar_style`
// reduces a part to its colour), so the wool is the middle of its two stops.
// The platform keeps its own widths and overlay behaviour; only the colours
// are flora's. Its hover darkening needs the engine's thumb hover colour,
// which nothing reads yet.

/// The wool by day (between `--fl-sbA` #D4D1C9 and `--fl-sbB` #ADAAA1) and
/// at night (between #3F3F3F and #333333).
pub(crate) const SCROLLBAR_WOOL: (ColorU, ColorU) =
    (ColorU::rgb(0xC0, 0xBD, 0xB5), ColorU::rgb(0x39, 0x39, 0x39));

/// The parchment track: `--fl-track` by day and at night.
pub(crate) const SCROLLBAR_PARCHMENT: (ColorU, ColorU) = (LIGHT_TRACK, DARK_TRACK);

/// The sheet that gives every scroll box under it flora's scrollbars, inert
/// in every other theme: `@theme(flora) { * { scrollbar-color: wool
/// parchment } }`, its night twin under the dark mode. `scrollbar-color` is a
/// scroll box's own property (azul does not inherit it), hence `*`; an app's
/// own `scrollbar-color` on a box still wins (author order). `pinned`: for a
/// subtree pinned to flora, which carries no `@theme` block - the same rules
/// without the theme's condition.
#[must_use]
pub(crate) fn scrollbar_sheet(pinned: bool) -> azul_css::css::Css {
    use azul_css::{
        css::{rule_priority, Css, CssDeclaration, CssPath, CssPathSelector, CssRuleBlock},
        dynamic_selector::{DynamicSelector, ModeCondition, ThemeCondition},
    };
    let flora = || {
        (!pinned).then(|| {
            DynamicSelector::Theme(ThemeCondition::Custom(AzString::from_const_str("flora")))
        })
    };
    let rule = |thumb: ColorU, track: ColorU, conditions: Vec<DynamicSelector>| CssRuleBlock {
        path: CssPath {
            selectors: alloc::vec![CssPathSelector::Global].into(),
        },
        declarations: alloc::vec![CssDeclaration::Static(CssProperty::ScrollbarColor(
            StyleScrollbarColorValue::Exact(StyleScrollbarColor::Custom(ScrollbarColorCustom {
                thumb,
                track,
            })),
        ))]
        .into(),
        conditions: conditions.into(),
        priority: rule_priority::AUTHOR,
    };
    Css {
        rules: alloc::vec![
            rule(SCROLLBAR_WOOL.0, SCROLLBAR_PARCHMENT.0, flora().into_iter().collect()),
            rule(
                SCROLLBAR_WOOL.1,
                SCROLLBAR_PARCHMENT.1,
                flora()
                    .into_iter()
                    .chain(core::iter::once(DynamicSelector::Mode(ModeCondition::Dark)))
                    .collect()
            ),
        ]
        .into(),
        ..Css::default()
    }
}

// ==== dialog ====
//
// Dialog, Modal and Popover in flora's terms (`doc/templates/flora.css`, and
// the design system's "Vespers approaches" card). The dialog is a LEAF laid
// on the page - `--fl-sur` in a `--fl-bd5` rule at the house radius, casting
// the floating leaf's shadow - with a HEADER BAND across its top: the window
// chrome's metal-free face (`--fl-ct` falling to `--fl-cb`, closed by a dark
// rule), on which the title is set in flora's capitals in the paper ink and
// the close glyph sits in the same ink. The band is the title row's own
// background, reaching past the panel's 14px inset to both edges; a dialog
// without a title but with a close draws it on the row that stands in for
// the title, so the close never lands on paper. The body is written in
// Garamond, inset 14px; the buttons a dialog carries line up at its foot. A modal dims
// its window with the drop panel's warm overlay (`.nav-overlay`,
// rgba(20, 19, 16, 0.45)), the same by day and by night. A popover is a
// small leaf without a band: its title in capitals on the paper, its close
// a quiet action in brass ink ([`popover_skin`]).

/// `.nav-overlay`: the warm dim behind a modal dialog, in both modes.
pub const DIALOG_BACKDROP: ColorU = ColorU::new(20, 19, 16, 115);
/// The leaf's cast shadow by day (`0 8px 22px`): rgba(48, 45, 38, 0.20).
const DIALOG_SHADOW_LIGHT: ColorU = ColorU::new(48, 45, 38, 51);
/// The same at night: rgba(0, 0, 0, 0.55).
const DIALOG_SHADOW_DARK: ColorU = ColorU::new(0, 0, 0, 140);
/// `--fl-shadow-2`'s first layer by day: rgba(48, 45, 38, 0.16).
const POPOVER_SHADOW_LIGHT: ColorU = ColorU::new(48, 45, 38, 41);
/// `--fl-shadow-2`'s first layer by night: rgba(0, 0, 0, 0.5).
const POPOVER_SHADOW_DARK: ColorU = ColorU::new(0, 0, 0, 128);
/// `.btn-quiet:hover`'s wash by day: rgba(180, 135, 44, 0.08).
const DIALOG_QUIET_WASH_LIGHT: ColorU = ColorU::new(180, 135, 44, 20);
/// The same wash by night, in the night brass: rgba(196, 181, 142, 0.10).
const DIALOG_QUIET_WASH_DARK: ColorU = ColorU::new(196, 181, 142, 26);
/// The light wash on the band under the pointer: rgba(255, 252, 240, 0.15).
const DIALOG_BAND_WASH: ColorU = ColorU::new(255, 252, 240, 38);
/// The rule that closes the band, by day (the chrome's foot, darker).
const DIALOG_BAND_RULE_LIGHT: ColorU = ColorU::rgb(0x55, 0x52, 0x4A);

/// The header band's height, the rule included.
const DIALOG_BAND_PX: isize = 30;

/// The header band as a background layer over its row: `top` falling to
/// `bottom` down to its last pixel, that pixel the `rule`, nothing under it.
#[must_use]
fn dialog_band(top: ColorU, bottom: ColorU, rule: ColorU) -> StyleBackgroundContent {
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: TO_BOTTOM,
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_vec(alloc::vec![
            px_stop(0, top),
            px_stop(DIALOG_BAND_PX - 1, bottom),
            px_stop(DIALOG_BAND_PX - 1, rule),
            px_stop(DIALOG_BAND_PX, rule),
            px_stop(DIALOG_BAND_PX, ColorU { a: 0, ..rule }),
        ]),
    })
}

/// The band behind a row of a dialog's head (its title, or the row that
/// stands in for it): the band by day and at night, reaching past the
/// panel's 14px inset to both edges, its top corners following the panel's.
#[must_use]
fn dialog_band_row() -> Vec<CssPropertyWithConditions> {
    use super::decl;
    let mut v = Vec::with_capacity(12);
    v.extend(decl::margin(0, -14, 12, -14));
    v.extend(decl::radius_corners(2, 2, 0, 0));
    v.extend(decl::themed_layers(
        alloc::vec![dialog_band(LIGHT_CT, LIGHT_CB, DIALOG_BAND_RULE_LIGHT)],
        alloc::vec![dialog_band(DARK_CT, DARK_CB, DARK_BD5)],
    ));
    v
}

/// A dialog's title in flora's capitals, tracked .1em, 8px over and under
/// a 14px line: on the band (`on_band`, the paper ink, the band's height
/// exactly) or on the paper (ruled off, in the quiet ink).
#[must_use]
fn dialog_title(on_band: bool) -> Vec<CssPropertyWithConditions> {
    use super::decl;
    type P = CssPropertyWithConditions;
    let mut title = crate::widgets::dialog::DIALOG_TITLE_BASE.to_vec();
    title.extend(caps((11, 0.1)));
    title.push(P::simple(CssProperty::const_line_height(StyleLineHeight::Length(
        PixelValue::const_px(14),
    ))));
    title.push(P::simple(CssProperty::const_text_align(StyleTextAlign::Left)));
    // The right inset keeps the heading clear of the absolutely-placed close.
    if on_band {
        title.extend(dialog_band_row());
        title.extend(decl::padding(8, 42, 8, 14));
        title.push(P::simple(decl::ink(LIGHT_ON_ACC)));
    } else {
        title.extend(decl::padding(0, 28, 8, 0));
        title.push(P::simple(CssProperty::const_margin_bottom(LayoutMarginBottom::const_px(8))));
        title.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
        title.extend(decl::themed_border(decl::Edges::BOTTOM, 1, LIGHT_SEP, DARK_SEP));
    }
    title
}

/// Flora's dialog skin (also the modal's; the popover has [`popover_skin`]).
#[must_use]
pub(crate) fn dialog_skin() -> crate::widgets::dialog::DialogSkin {
    use super::decl;
    use crate::widgets::dialog as d;
    type P = CssPropertyWithConditions;

    // Every part: the dialog's structure (R5), then flora's skin.
    //
    // The leaf and its band. Same width as flat's panel (280..520 px).
    let mut panel = d::DIALOG_PANEL_BASE.to_vec();
    panel.extend([
        P::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(280))),
        P::simple(CssProperty::const_max_width(LayoutMaxWidth::const_px(520))),
        P::simple(CssProperty::const_font_size(StyleFontSize::const_px(14))),
        P::simple(CssProperty::const_font_family(SERIF_FAMILY)),
    ]);
    panel.extend(decl::padding(0, 14, 12, 14));
    panel.extend(decl::themed_border(decl::Edges::ALL, 1, LIGHT_BD5, DARK_BD5));
    panel.extend(decl::radius(3));
    panel.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
    panel.extend(decl::themed_ink(LIGHT_INK2, DARK_INK2));
    panel.extend(decl::themed_shadow(8, 22, DIALOG_SHADOW_LIGHT, DIALOG_SHADOW_DARK));

    // Without a title, the row that stands in for it carries the band, so
    // the close still sits on it.
    let mut close_row = d::DIALOG_CLOSE_ROW_STYLE.to_vec();
    close_row.push(P::simple(CssProperty::const_min_height(LayoutMinHeight::const_px(
        DIALOG_BAND_PX,
    ))));
    close_row.extend(dialog_band_row());

    // The close, on the band, in the paper ink; the light wash under the
    // pointer, the glow ring on focus (it stands off the band by day and by
    // night).
    let mut close = d::DIALOG_CLOSE_BASE.to_vec();
    close.extend([
        P::simple(CssProperty::const_top(LayoutTop::const_px(3))),
        P::simple(CssProperty::const_right(LayoutRight::const_px(8))),
        decl::font_size(18),
    ]);
    close.extend(decl::padding(0, 5, 0, 5));
    close.extend(decl::radius(3));
    close.push(P::simple(decl::ink(LIGHT_ON_ACC)));
    close.extend(decl::ring_slot());
    // States last.
    close.push(P::on_hover(decl::fill(DIALOG_BAND_WASH)));
    close.extend(decl::focus_ring(LIGHT_GLOW, DARK_GLOW));

    d::DialogSkin {
        theme: super::UiTheme::Flora,
        panel: CssPropertyWithConditionsVec::from_vec(panel),
        title: CssPropertyWithConditionsVec::from_vec(dialog_title(true)),
        close_row: CssPropertyWithConditionsVec::from_vec(close_row),
        close: CssPropertyWithConditionsVec::from_vec(close),
        content: CssPropertyWithConditionsVec::from_const_slice(d::DIALOG_CONTENT_STYLE),
        backdrop: d::backdrop_style(DIALOG_BACKDROP),
    }
}

/// Flora's popover panel: a small leaf - `--fl-sur`, `--fl-bd2`, the house
/// radius (`--fl-r`, 3px) and the nearer shadow of `--fl-shadow-2`.
#[must_use]
pub fn popover_panel_style() -> CssPropertyWithConditionsVec {
    use super::decl;
    type P = CssPropertyWithConditions;

    // The widget's base (its structure, the same in every theme), then the
    // leaf.
    let mut v = crate::widgets::popover::POPOVER_PANEL_BASE.to_vec();
    v.push(P::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(160))));
    v.extend(decl::padding(8, 8, 8, 8));
    v.extend(decl::themed_border(decl::Edges::ALL, 1, LIGHT_BD2, DARK_BD2));
    v.extend(decl::radius(3));
    v.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
    v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    v.extend(decl::themed_shadow(2, 5, POPOVER_SHADOW_LIGHT, POPOVER_SHADOW_DARK));
    CssPropertyWithConditionsVec::from_vec(v)
}

/// Flora's popover skin: the dialog's parts on a small leaf without the
/// band - the title in capitals on the paper, ruled off; the close a quiet
/// action in brass ink (`.btn-quiet`: `--fl-qt`, darkening to `--fl-qt2`
/// over the quiet wash), ringed in the accent by day and the glow by night.
#[must_use]
pub(crate) fn popover_skin() -> crate::widgets::dialog::DialogSkin {
    use super::decl;
    use crate::widgets::dialog as d;
    type P = CssPropertyWithConditions;

    let mut close = d::DIALOG_CLOSE_BASE.to_vec();
    close.extend([
        P::simple(CssProperty::const_top(LayoutTop::const_px(6))),
        P::simple(CssProperty::const_right(LayoutRight::const_px(8))),
        decl::font_size(18),
    ]);
    close.extend(decl::padding(0, 5, 0, 5));
    close.extend(decl::radius(3));
    close.extend(decl::themed_ink(LIGHT_QT, DARK_QT));
    close.extend(decl::ring_slot());
    // States last: a resting dark twin matches in every state.
    close.extend(decl::hover_ink(LIGHT_QT2, DARK_QT2));
    close.extend(decl::hover_fill(DIALOG_QUIET_WASH_LIGHT, DIALOG_QUIET_WASH_DARK));
    close.extend(decl::focus_ring(LIGHT_ACC, DARK_GLOW));

    let mut skin = dialog_skin();
    skin.panel = popover_panel_style();
    skin.title = CssPropertyWithConditionsVec::from_vec(dialog_title(false));
    skin.close_row = CssPropertyWithConditionsVec::from_const_slice(d::DIALOG_CLOSE_ROW_STYLE);
    skin.close = CssPropertyWithConditionsVec::from_vec(close);
    skin
}

/// Renders a [`crate::widgets::dialog::Dialog`] in the flora theme.
#[must_use]
pub fn dialog(d: crate::widgets::dialog::Dialog) -> Dom {
    d.build(dialog_skin())
}

/// Renders a [`crate::widgets::modal::Modal`] in the flora theme.
#[must_use]
pub fn modal(m: crate::widgets::modal::Modal) -> Dom {
    m.build(dialog_skin())
}

/// Renders a [`crate::widgets::popover::Popover`] in the flora theme.
#[must_use]
pub fn popover(p: crate::widgets::popover::Popover) -> Dom {
    p.build(popover_skin())
}

// ==== number_input ====
//
// A NumberInput draws nothing of its own: the TextInput it wraps is the field,
// rendered by `text_input` above in flora. What flora adds for a number field
// the app did not style: the field's own paper by day (`--fl-fld`) under flora
// ink, a `--fl-bd2` hairline, the house radius and the well a field is sunk in
// (`--fl-well`); the night field, border and ink are `text_input`'s. Its focus
// ring lifts to the stone's glow by night (`--focus-color`) - appended after
// `text_input`'s states, so it is the one that wins.

/// `--fl-well` by day: inset 0 1px 2px rgba(48, 45, 38, 0.10).
const NUMBER_INPUT_WELL_LIGHT: ColorU = ColorU::new(48, 45, 38, 26);
/// `--fl-well` by night: inset 0 1px 2px rgba(0, 0, 0, 0.45).
const NUMBER_INPUT_WELL_DARK: ColorU = ColorU::new(0, 0, 0, 115);

/// Renders a [`crate::widgets::number_input::NumberInput`] in the flora theme.
#[must_use]
pub fn number_input(mut n: crate::widgets::number_input::NumberInput) -> Dom {
    use azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec;

    use super::{decl, style_kit as kit};
    type P = CssPropertyWithConditions;

    n.text_input.set_theme(super::UiTheme::Flora);
    // A caller who styled the field chose every property of it; the theme
    // adds to its OWN default only (as `button` does).
    let owns_field = n.text_input.container_style.is_none();
    if owns_field {
        let mut field = crate::widgets::text_input::TEXT_INPUT_CONTAINER_PROPS.to_vec();
        field.push(P::simple(decl::fill(LIGHT_FLD)));
        field.push(P::simple(decl::ink(LIGHT_INK)));
        field.extend(decl::themed_border(decl::Edges::ALL, 1, LIGHT_BD2, DARK_BD));
        field.extend(decl::radius(3));
        field.extend(decl::themed_inset_shadow(
            1,
            2,
            NUMBER_INPUT_WELL_LIGHT,
            NUMBER_INPUT_WELL_DARK,
        ));
        n.text_input.container_style =
            OptionCssPropertyWithConditionsVec::Some(CssPropertyWithConditionsVec::from_vec(field));
    }
    if n.text_input.label_style.is_none() {
        let mut label = crate::widgets::text_input::TEXT_INPUT_LABEL_PROPS.to_vec();
        label.push(P::simple(decl::ink(LIGHT_INK)));
        n.text_input.label_style =
            OptionCssPropertyWithConditionsVec::Some(CssPropertyWithConditionsVec::from_vec(label));
    }
    let mut dom = n.build();
    if owns_field {
        for p in decl::focus_ring(LIGHT_ACC, DARK_GLOW) {
            dom.add_css_property(p);
        }
    }
    dom.add_class(AzString::from_const_str(kit::FLORA_CLASS));
    dom
}

// ==== pagination ====
//
// A flora pager is a row of raised paper (`.btn-secondary`: `--fl-rT` over
// `--fl-rB`, a `--fl-bd2` hairline, flora ink) joined into one bar with the
// house radius (`--fl-r`, 3px) on its outer corners. The current page is the
// SUNKEN accent stone flora cuts a selected item as (`.nav-links a.active`,
// `.lang-grid button.active`: `--fl-gem-sunken` under the sunken rig, written
// in `--fl-on-acc`) - its own colour by day and by night. The end the pager
// cannot go past is disabled paper (`--fl-disBg` / `--fl-disTx`). A page hovers
// and presses on flora's faces, and every button is ringed on focus: an inset
// ring (the inner pages share their side borders) in the accent by day, the
// stone's glow by night and on the stone itself.
//
// The selected stone is shared by the W3b widgets below (segmented, radio,
// stepper), which is why it is not named after the pager.

const SELECTED_STONE_STOPS: &[NormalizedLinearColorStop] =
    &[stop(0, LIGHT_DEEP), stop(96, LIGHT_ACC)];

/// `--fl-gem-sunken`: `linear-gradient(175deg, var(--fl-deep) 0%,
/// var(--fl-acc) 96%)` - a stone pressed into its well.
pub const SELECTED_STONE_GEM: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: deg(175),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(SELECTED_STONE_STOPS),
    });

/// The selected item of a flora group: the sunken accent stone under the
/// sunken rig (lit from below the near edge). The same in both modes - a
/// stone is its own colour.
#[must_use]
pub fn selected_stone() -> Vec<StyleBackgroundContent> {
    vec![
        SELECTED_STONE_GEM,
        SUNKEN_RIG_TOP,
        SUNKEN_RIG_LEFT,
        SUNKEN_RIG_BOTTOM,
    ]
}

/// Flora's pagination skin.
#[must_use]
pub(crate) fn pagination_skin() -> crate::widgets::pagination::PaginationSkin {
    crate::widgets::pagination::PaginationSkin {
        theme: super::UiTheme::Flora,
        button: pagination_button,
    }
}

/// One flora pagination button: the widget's base (its structure, the same in
/// every theme), then the skin - box, joined hairline, face, then states.
fn pagination_button(
    face: crate::widgets::pagination::PageFace,
    is_first: bool,
    is_last: bool,
) -> CssPropertyWithConditionsVec {
    use super::decl;
    use crate::widgets::pagination::{PageFace, PAGINATION_BUTTON_BASE};
    type P = CssPropertyWithConditions;

    let mut v = PAGINATION_BUTTON_BASE.to_vec();
    v.extend([
        P::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(36))),
        decl::font_size(13),
        P::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    ]);
    v.extend(decl::padding(6, 12, 6, 12));
    // Joined: every button draws top, bottom and right; only the first draws
    // a left edge, so neighbours share one hairline.
    let edges = decl::Edges {
        top: true,
        right: true,
        bottom: true,
        left: is_first,
    };
    v.extend(decl::themed_border(edges, 1, LIGHT_BD2, DARK_BD2));
    if is_first {
        v.extend(decl::radius_corners(3, 0, 0, 3));
    }
    if is_last {
        v.extend(decl::radius_corners(0, 3, 3, 0));
    }
    match face {
        PageFace::Neutral => {
            v.extend(decl::themed_layers(
                vec![RAISED_FACE_LIGHT],
                vec![RAISED_FACE_DARK],
            ));
            v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
        }
        PageFace::Disabled => {
            v.extend(decl::themed_fill(LIGHT_DISBG, DARK_DISBG));
            v.extend(decl::themed_ink(LIGHT_DISTX, DARK_DISTX));
        }
        PageFace::Current => {
            v.push(P::simple(decl::layers(selected_stone())));
            v.push(P::simple(decl::ink(LIGHT_ON_ACC)));
        }
    }
    // States last: a resting dark twin matches in every state.
    if face == PageFace::Neutral {
        v.extend(decl::hover_layers(
            vec![HOVER_FACE_LIGHT],
            vec![HOVER_FACE_DARK],
        ));
        v.extend(decl::active_layers(
            vec![PRESSED_FACE_LIGHT],
            vec![PRESSED_FACE_DARK],
        ));
    }
    let ring = if face == PageFace::Current {
        LIGHT_GLOW
    } else {
        LIGHT_ACC
    };
    v.extend(decl::focus_halo_inset_stacked(ring, DARK_GLOW));
    CssPropertyWithConditionsVec::from_vec(v)
}

/// Renders a [`crate::widgets::pagination::Pagination`] in the flora theme.
#[must_use]
pub fn pagination(p: crate::widgets::pagination::Pagination) -> Dom {
    p.build(pagination_skin())
}

// ==== radio_group ====
//
// A flora radio is a WELL cut into the leaf: field paper (`--fl-fld`, by night
// the night field) under a `--fl-bd3` hairline - the heavier rule a control
// takes - sunk by `--fl-well`. The checked radio holds a small accent stone
// (`--fl-acc` under the orb's specular cap, `.fl-orb-gloss`), its own colour by
// day and by night. Labels are flora ink. A row washes to `--fl-hov` under the
// pointer and is ringed on focus, in the accent by day and the glow by night.
// The indicator keeps the widget's fixed geometry (16px, never shrinks).

/// `--fl-hov` by day: rgba(253, 252, 248, 0.6).
const RADIO_GROUP_HOVER_LIGHT: ColorU = ColorU::new(253, 252, 248, 153);
/// `--fl-hov` by night: rgba(58, 58, 58, 0.7).
const RADIO_GROUP_HOVER_DARK: ColorU = ColorU::new(58, 58, 58, 179);

/// Flora's radio-group skin for a group laid out `horizontal`ly or not.
#[must_use]
pub(crate) fn radio_group_skin(horizontal: bool) -> crate::widgets::radio_group::RadioGroupSkin {
    use super::decl;
    use crate::widgets::radio_group as r;
    type P = CssPropertyWithConditions;

    let mut row = r::build_row_style(horizontal).into_library_owned_vec();
    row.extend(decl::padding(1, 4, 1, 2));
    row.extend(decl::radius(3));
    row.extend(decl::ring_slot());
    // States last.
    row.extend(decl::hover_fill(RADIO_GROUP_HOVER_LIGHT, RADIO_GROUP_HOVER_DARK));
    row.extend(decl::focus_ring(LIGHT_ACC, DARK_GLOW));

    // The well: the widget's base (its structure, the same in every theme),
    // the widget's geometry, flora's paper.
    let mut circle = r::RADIO_GROUP_CIRCLE_BASE.to_vec();
    circle.extend([
        P::simple(CssProperty::const_width(LayoutWidth::const_px(r::CIRCLE_SIZE))),
        P::simple(CssProperty::const_height(LayoutHeight::const_px(r::CIRCLE_SIZE))),
    ]);
    circle.extend(decl::themed_border(decl::Edges::ALL, r::CIRCLE_BORDER, LIGHT_BD3, DARK_BD3));
    circle.extend(decl::radius(r::CIRCLE_RADIUS));
    circle.extend(decl::themed_fill(LIGHT_FLD, DARK_FLD));
    // `--fl-well`, the same inset the flora number field is sunk by.
    circle.extend(decl::themed_inset_shadow(
        1,
        2,
        NUMBER_INPUT_WELL_LIGHT,
        NUMBER_INPUT_WELL_DARK,
    ));

    // The stone, shown (100) or laid out and invisible (0), on the widget's
    // base.
    let dot = |opacity: isize| {
        let mut v = r::RADIO_GROUP_DOT_BASE.to_vec();
        v.extend([
            P::simple(CssProperty::const_width(LayoutWidth::const_px(r::DOT_SIZE))),
            P::simple(CssProperty::const_height(LayoutHeight::const_px(r::DOT_SIZE))),
            // The design system's radio dot: the stone lit at 35% 30%,
            // `radial-gradient(circle at 35% 30%, acc, deep)`.
            P::simple(decl::layers(vec![StyleBackgroundContent::RadialGradient(
                RadialGradient {
                    shape: Shape::Circle,
                    size: RadialGradientSize::FarthestCorner,
                    position: StyleBackgroundPosition {
                        horizontal: BackgroundPositionHorizontal::Exact(
                            PixelValue::const_percent(35),
                        ),
                        vertical: BackgroundPositionVertical::Exact(PixelValue::const_percent(
                            30,
                        )),
                    },
                    extend_mode: ExtendMode::Clamp,
                    stops: NormalizedLinearColorStopVec::from_vec(vec![
                        stop(0, LIGHT_ACC),
                        stop(100, LIGHT_DEEP),
                    ]),
                },
            )])),
        ]);
        v.extend(decl::radius(r::DOT_RADIUS));
        v.push(P::simple(CssProperty::const_opacity(StyleOpacity::const_new(
            opacity,
        ))));
        CssPropertyWithConditionsVec::from_vec(v)
    };

    let mut label = r::RADIO_GROUP_LABEL_STYLE.to_vec();
    label.extend(decl::themed_ink(LIGHT_INK, DARK_INK));

    r::RadioGroupSkin {
        theme: super::UiTheme::Flora,
        row: CssPropertyWithConditionsVec::from_vec(row),
        circle: CssPropertyWithConditionsVec::from_vec(circle),
        dot_selected: dot(100),
        dot_unselected: dot(0),
        label: CssPropertyWithConditionsVec::from_vec(label),
    }
}

/// Renders a [`crate::widgets::radio_group::RadioGroup`] in the flora theme.
#[must_use]
pub fn radio_group(rg: crate::widgets::radio_group::RadioGroup) -> Dom {
    let skin = radio_group_skin(rg.radio_group_state.horizontal);
    rg.build(skin)
}

// ==== segmented ====
//
// A flora segmented control is the pager's language applied to a choice: raised
// paper segments (`--fl-rT` over `--fl-rB`, flora ink) joined under one
// `--fl-bd2` hairline with the house radius on the outer corners, and the chosen
// segment cut as the sunken accent stone (`flora::selected_stone`, in
// `--fl-on-acc`) - its own colour by day and by night. A segment hovers and
// presses on flora's faces and is ringed on focus with an inset ring, in the
// accent by day, the glow by night and on the stone.

/// Flora's segmented skin.
#[must_use]
pub(crate) fn segmented_skin() -> crate::widgets::segmented::SegmentedSkin {
    crate::widgets::segmented::SegmentedSkin {
        theme: super::UiTheme::Flora,
        segment: segmented_segment,
    }
}

/// One flora segment: the widget's base (its structure, the same in every
/// theme), then the skin - box, joined hairline, face, then states.
fn segmented_segment(selected: bool, is_first: bool, is_last: bool) -> CssPropertyWithConditionsVec {
    use super::decl;
    type P = CssPropertyWithConditions;

    let mut v = crate::widgets::segmented::SEGMENT_BASE.to_vec();
    v.extend([
        decl::font_size(13),
        P::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    ]);
    v.extend(decl::padding(6, 12, 6, 12));
    // Joined: only the first segment draws a left edge.
    let edges = decl::Edges {
        top: true,
        right: true,
        bottom: true,
        left: is_first,
    };
    v.extend(decl::themed_border(edges, 1, LIGHT_BD2, DARK_BD2));
    v.extend(decl::radius_corners(
        if is_first { 3 } else { 0 },
        if is_last { 3 } else { 0 },
        if is_last { 3 } else { 0 },
        if is_first { 3 } else { 0 },
    ));
    if selected {
        v.push(P::simple(decl::layers(selected_stone())));
        v.push(P::simple(decl::ink(LIGHT_ON_ACC)));
    } else {
        v.extend(decl::themed_layers(
            vec![RAISED_FACE_LIGHT],
            vec![RAISED_FACE_DARK],
        ));
        v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
        // States last: a resting dark twin matches in every state.
        v.extend(decl::hover_layers(
            vec![HOVER_FACE_LIGHT],
            vec![HOVER_FACE_DARK],
        ));
        v.extend(decl::active_layers(
            vec![PRESSED_FACE_LIGHT],
            vec![PRESSED_FACE_DARK],
        ));
    }
    let ring = if selected { LIGHT_GLOW } else { LIGHT_ACC };
    v.extend(decl::focus_halo_inset_stacked(ring, DARK_GLOW));
    CssPropertyWithConditionsVec::from_vec(v)
}

/// Renders a [`crate::widgets::segmented::Segmented`] in the flora theme.
#[must_use]
pub fn segmented(s: crate::widgets::segmented::Segmented) -> Dom {
    s.build(segmented_skin())
}

// ==== split_pane ====
//
// A flora divider is a CHANNEL between two leaves: the toolbar strip
// (`--fl-strip`, by night the night strip) between two `--fl-bd` hairlines on
// its long sides, drawn inside the bar (border-box) so the thickness the drag
// arithmetic subtracts is still the rendered one. It lifts to `--fl-hB` under
// the pointer, sinks to `--fl-pT` while dragged, and is ringed on focus with an
// inset ring that fills the channel - the accent by day, the glow by night.

/// Flora's split-pane skin for a pane split in `direction`.
#[must_use]
pub(crate) fn split_pane_skin(
    direction: crate::widgets::split_pane::SplitDirection,
) -> crate::widgets::split_pane::SplitPaneSkin {
    use super::decl;
    use crate::widgets::split_pane::{self as s, SplitDirection};
    type P = CssPropertyWithConditions;

    // The hairlines run along the bar's long sides.
    let edges = match direction {
        SplitDirection::Horizontal => decl::Edges {
            top: false,
            right: true,
            bottom: false,
            left: true,
        },
        SplitDirection::Vertical => decl::Edges {
            top: true,
            right: false,
            bottom: true,
            left: false,
        },
    };
    // The widget's base (its structure, the same in every theme), then the
    // skin: the thickness, the channel, the states.
    let mut divider = s::divider_base(direction);
    divider.push(P::simple(s::divider_thickness(direction)));
    divider.extend(decl::themed_border(edges, 1, LIGHT_BD, DARK_BD));
    divider.extend(decl::themed_fill(LIGHT_STRIP, DARK_STRIP));
    // States last: a resting dark twin matches in every state.
    divider.extend(decl::hover_fill(LIGHT_HB, DARK_HB));
    divider.extend(decl::active_fill(LIGHT_PT, DARK_PT));
    divider.extend(decl::focus_halo_inset_stacked(LIGHT_ACC, DARK_GLOW));

    s::SplitPaneSkin {
        theme: super::UiTheme::Flora,
        divider: CssPropertyWithConditionsVec::from_vec(divider),
    }
}

/// Renders a [`crate::widgets::split_pane::SplitPane`] in the flora theme.
#[must_use]
pub fn split_pane(sp: crate::widgets::split_pane::SplitPane) -> Dom {
    let skin = split_pane_skin(sp.split_pane_state.inner.direction);
    sp.build(skin)
}

// ==== stepper ====
//
// A flora stepper marks the way walked in accent STONES - each reached step a
// raised accent stone under the rig and the specular streak flora lays on every
// stone (`.btn-primary`), numbered in `--fl-on-acc`, its own colour by day and by
// night - joined by an accent line. The way ahead is raised paper (`--fl-rT` over
// `--fl-rB`) numbered in soft ink (`--fl-soft1`), joined by a `--fl-bd` line.
// Every circle wears a `--fl-bd2` hairline inside its box. Labels are ink for
// the way walked and soft ink ahead. A cell washes to `--fl-hov` under the
// pointer and is ringed on focus: the accent by day, the glow by night.

/// Flora's stepper skin.
#[must_use]
pub(crate) fn stepper_skin() -> crate::widgets::stepper::StepperSkin {
    crate::widgets::stepper::StepperSkin {
        theme: super::UiTheme::Flora,
        cell: stepper_cell,
        circle: stepper_circle,
        connector: stepper_connector,
        label: stepper_label,
    }
}

fn stepper_cell() -> CssPropertyWithConditionsVec {
    use super::decl;
    let mut v = crate::widgets::stepper::STEPPER_STEP_STYLE.to_vec();
    v.extend(decl::padding(2, 2, 4, 2));
    v.extend(decl::radius(3));
    v.extend(decl::ring_slot());
    // States last.
    v.extend(decl::hover_fill(RADIO_GROUP_HOVER_LIGHT, RADIO_GROUP_HOVER_DARK));
    v.extend(decl::focus_ring(LIGHT_ACC, DARK_GLOW));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn stepper_circle(reached: bool) -> CssPropertyWithConditionsVec {
    use super::decl;
    use crate::widgets::stepper as s;
    type P = CssPropertyWithConditions;

    // The circle's structure is the widget's (`stepper::CIRCLE_BASE`: the
    // hairline below sits inside the box, the same 28px as every theme's);
    // what follows is flora's skin on it.
    let mut v = s::CIRCLE_BASE.to_vec();
    v.extend([
        P::simple(CssProperty::const_width(LayoutWidth::const_px(s::CIRCLE_SIZE))),
        P::simple(CssProperty::const_height(LayoutHeight::const_px(s::CIRCLE_SIZE))),
        P::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(s::CIRCLE_SIZE))),
        decl::font_size(13),
        P::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    ]);
    v.extend(decl::radius(s::CIRCLE_RADIUS));
    v.extend(decl::themed_border(decl::Edges::ALL, 1, LIGHT_BD2, DARK_BD2));
    if reached {
        v.push(P::simple(decl::layers(stone_face(LIGHT_ACC, STONE_STREAK))));
        v.push(P::simple(decl::ink(LIGHT_ON_ACC)));
    } else {
        v.extend(decl::themed_layers(
            vec![RAISED_FACE_LIGHT],
            vec![RAISED_FACE_DARK],
        ));
        v.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    }
    CssPropertyWithConditionsVec::from_vec(v)
}

fn stepper_connector(fill: crate::widgets::stepper::ConnFill) -> CssPropertyWithConditionsVec {
    use super::decl;
    use crate::widgets::stepper::{self as s, ConnFill};
    type P = CssPropertyWithConditions;

    let mut v = s::CONNECTOR_BASE.to_vec();
    v.push(P::simple(CssProperty::const_height(LayoutHeight::const_px(
        s::CONNECTOR_HEIGHT,
    ))));
    match fill {
        ConnFill::Accent => v.push(P::simple(decl::fill(LIGHT_ACC))),
        ConnFill::Muted => v.extend(decl::themed_fill(LIGHT_BD, DARK_BD)),
        ConnFill::Hidden => v.push(P::simple(decl::fill(ColorU::TRANSPARENT))),
    }
    CssPropertyWithConditionsVec::from_vec(v)
}

fn stepper_label(reached: bool) -> CssPropertyWithConditionsVec {
    use super::decl;
    type P = CssPropertyWithConditions;

    let mut v = crate::widgets::stepper::LABEL_BASE.to_vec();
    v.extend([
        decl::font_size(12),
        P::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        P::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(6))),
    ]);
    if reached {
        v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    } else {
        v.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    }
    CssPropertyWithConditionsVec::from_vec(v)
}

/// Renders a [`crate::widgets::stepper::Stepper`] in the flora theme.
#[must_use]
pub fn stepper(s: crate::widgets::stepper::Stepper) -> Dom {
    s.build(stepper_skin())
}

// ==== time_picker ====
//
// A flora time picker is a WELL of field paper (`--fl-fld`, by night the night
// field) under a `--fl-bd2` hairline, sunk by `--fl-well`, with the house radius.
// The readouts are flora ink, the `:` soft ink, and the arrows icon ink
// (`--fl-icon`) that turns to ink on a raised hover face and sinks on a press -
// quiet controls, no chrome at rest. The AM/PM toggle is raised paper
// (`.btn-secondary`) in ink. Every column (the spin button, the Tab stop - the
// arrows are click targets only) and the toggle are ringed on focus: the accent
// by day, the glow by night. The arrows keep the widget's 40x16 hit box.

/// Flora's time picker skin.
#[must_use]
pub(crate) fn time_picker_skin() -> crate::widgets::time_picker::TimePickerSkin {
    use super::decl;
    use crate::widgets::time_picker as t;
    type P = CssPropertyWithConditions;

    // Every part is the widget's base (`time_picker::CONTAINER_BASE`,
    // `CLICKABLE_BASE`, `READOUT_BASE`: its structure), then flora's skin.
    let mut container = t::CONTAINER_BASE.to_vec();
    container.extend(decl::padding(4, 6, 4, 6));
    container.extend(decl::themed_border(decl::Edges::ALL, 1, LIGHT_BD2, DARK_BD2));
    container.extend(decl::radius(3));
    container.extend(decl::themed_fill(LIGHT_FLD, DARK_FLD));
    container.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    container.extend(decl::themed_inset_shadow(
        1,
        2,
        NUMBER_INPUT_WELL_LIGHT,
        NUMBER_INPUT_WELL_DARK,
    ));

    let mut arrow = t::CLICKABLE_BASE.to_vec();
    arrow.extend([
        P::simple(CssProperty::const_width(LayoutWidth::const_px(40))),
        P::simple(CssProperty::const_height(LayoutHeight::const_px(16))),
        decl::font_size(11),
        P::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        P::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(2))),
        P::simple(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(2))),
    ]);
    arrow.extend(decl::radius(3));
    arrow.extend(decl::themed_ink(LIGHT_ICON, DARK_ICON));
    // States last.
    arrow.extend(decl::hover_layers(
        vec![HOVER_FACE_LIGHT],
        vec![HOVER_FACE_DARK],
    ));
    arrow.extend(decl::hover_ink(LIGHT_INK, DARK_INK));
    arrow.extend(decl::active_layers(
        vec![PRESSED_FACE_LIGHT],
        vec![PRESSED_FACE_DARK],
    ));

    // The column is the spin button: its base, then its focus ring.
    let mut spinner = t::SPINNER_STYLE.to_vec();
    spinner.extend(decl::radius(3));
    spinner.extend(decl::focus_halo_inset_stacked(LIGHT_ACC, DARK_GLOW));

    let mut display = t::READOUT_BASE.to_vec();
    display.extend([
        decl::font_size(18),
        P::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        P::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(2))),
        P::simple(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(2))),
    ]);
    display.extend(decl::themed_ink(LIGHT_INK, DARK_INK));

    let mut separator = t::READOUT_BASE.to_vec();
    separator.extend([
        decl::font_size(18),
        P::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(2))),
        P::simple(CssProperty::const_padding_right(LayoutPaddingRight::const_px(2))),
    ]);
    separator.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    let mut ampm = t::CLICKABLE_BASE.to_vec();
    ampm.extend([
        decl::font_size(13),
        P::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        P::simple(CssProperty::const_margin_left(LayoutMarginLeft::const_px(8))),
    ]);
    ampm.extend(decl::padding(4, 8, 4, 8));
    ampm.extend(decl::radius(3));
    ampm.extend(decl::themed_border(decl::Edges::ALL, 1, LIGHT_BD2, DARK_BD2));
    ampm.extend(decl::themed_layers(
        vec![RAISED_FACE_LIGHT],
        vec![RAISED_FACE_DARK],
    ));
    ampm.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    // States last.
    ampm.extend(decl::hover_layers(
        vec![HOVER_FACE_LIGHT],
        vec![HOVER_FACE_DARK],
    ));
    ampm.extend(decl::active_layers(
        vec![PRESSED_FACE_LIGHT],
        vec![PRESSED_FACE_DARK],
    ));
    ampm.extend(decl::focus_ring(LIGHT_ACC, DARK_GLOW));

    t::TimePickerSkin {
        theme: super::UiTheme::Flora,
        container: CssPropertyWithConditionsVec::from_vec(container),
        spinner: CssPropertyWithConditionsVec::from_vec(spinner),
        arrow: CssPropertyWithConditionsVec::from_vec(arrow),
        display: CssPropertyWithConditionsVec::from_vec(display),
        separator: CssPropertyWithConditionsVec::from_vec(separator),
        ampm: CssPropertyWithConditionsVec::from_vec(ampm),
    }
}

/// Renders a [`crate::widgets::time_picker::TimePicker`] in the flora theme.
#[must_use]
pub fn time_picker(p: crate::widgets::time_picker::TimePicker) -> Dom {
    p.build(time_picker_skin())
}

// ==== toast ====
//
// Flora has no pastel alert fills: a notice is a LEAF laid over the page
// (`--fl-sur` under a `--fl-bd2` hairline, the house radius, the nearer shadow of
// `--fl-shadow-2`), in flora ink, and what KIND of notice it is runs as a thread
// in its left margin - the way flora sets a quotation against a metal thread
// (`blockquote`). The threads are the house hues flora.css lists as alternates to
// its accent: the accent itself for information (by night its glow), leaf for
// success, brass (`--color-gold`) for a warning, clay for danger - each with its
// night value. The "x" is a quiet action in brass ink, ringed on focus in the
// accent by day and the glow by night. The card keeps the widget's placement.

/// The kind's thread in the margin: `(by day, by night)`.
const fn toast_thread(kind: crate::widgets::toast::ToastKind) -> (ColorU, ColorU) {
    use crate::widgets::toast::ToastKind;
    match kind {
        ToastKind::Info => (LIGHT_ACC, DARK_GLOW),
        // leaf: #44684F / #7FA98C
        ToastKind::Success => (ColorU::new(68, 104, 79, 255), ColorU::new(127, 169, 140, 255)),
        // brass: --color-gold #9A8B5F / #C4B58E
        ToastKind::Warning => (ColorU::new(154, 139, 95, 255), ColorU::new(196, 181, 142, 255)),
        // clay: #7E4A42 / #B3837A
        ToastKind::Danger => (ColorU::new(126, 74, 66, 255), ColorU::new(179, 131, 122, 255)),
    }
}

/// Flora's toast skin.
#[must_use]
pub(crate) fn toast_skin() -> crate::widgets::toast::ToastSkin {
    use super::decl;
    use crate::widgets::toast as t;
    type P = CssPropertyWithConditions;

    // The widget's close base (`toast::TOAST_CLOSE_BASE`), then flora's skin.
    let mut close = t::TOAST_CLOSE_BASE.to_vec();
    close.extend([
        decl::font_size(18),
        P::simple(CssProperty::const_margin_left(LayoutMarginLeft::const_px(12))),
    ]);
    close.extend(decl::padding(0, 5, 0, 5));
    close.extend(decl::radius(3));
    close.extend(decl::themed_ink(LIGHT_QT, DARK_QT));
    close.extend(decl::ring_slot());
    // States last.
    close.extend(decl::hover_ink(LIGHT_QT2, DARK_QT2));
    close.extend(decl::hover_fill(DIALOG_QUIET_WASH_LIGHT, DIALOG_QUIET_WASH_DARK));
    close.extend(decl::focus_ring(LIGHT_ACC, DARK_GLOW));

    t::ToastSkin {
        theme: super::UiTheme::Flora,
        container: toast_container,
        message: CssPropertyWithConditionsVec::from_const_slice(t::TOAST_MESSAGE_STYLE),
        close: CssPropertyWithConditionsVec::from_vec(close),
    }
}

/// The flora card for a kind: a leaf with the kind's thread in its margin.
fn toast_container(kind: crate::widgets::toast::ToastKind) -> CssPropertyWithConditionsVec {
    use super::decl;
    use crate::widgets::toast as t;
    type P = CssPropertyWithConditions;

    let (thread, thread_dark) = toast_thread(kind);
    // The card's structure and placement - where every theme's toast floats,
    // the positioned parent's corner - is the widget's (`TOAST_CARD_BASE`).
    let mut v = t::TOAST_CARD_BASE.to_vec();
    v.extend([
        P::simple(CssProperty::const_max_width(LayoutMaxWidth::const_px(t::TOAST_MAX_WIDTH))),
        decl::font_size(14),
        P::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ]);
    v.extend(decl::padding(12, 12, 12, 14));
    let hairline = decl::Edges {
        top: true,
        right: true,
        bottom: true,
        left: false,
    };
    v.extend(decl::themed_border(hairline, 1, LIGHT_BD2, DARK_BD2));
    let margin = decl::Edges {
        top: false,
        right: false,
        bottom: false,
        left: true,
    };
    v.extend(decl::themed_border(margin, 3, thread, thread_dark));
    v.extend(decl::radius(3));
    v.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
    v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    v.extend(decl::themed_shadow(
        2,
        5,
        POPOVER_SHADOW_LIGHT,
        POPOVER_SHADOW_DARK,
    ));
    CssPropertyWithConditionsVec::from_vec(v)
}

/// Renders a [`crate::widgets::toast::Toast`] in the flora theme.
#[must_use]
pub fn toast(t: crate::widgets::toast::Toast) -> Dom {
    t.build(toast_skin())
}

// ==== tooltip ====
//
// A flora tip is marginalia on DARK OAK - "never black" (the design system's
// tooltip: `#3B3327` under `#EFE7D7`, a `--fl-soft1` rule) - at the house
// radius, casting a small warm shadow (`0 3px 8px rgba(60,48,30,.3)`). The
// same wood by night, its rule a step darker and its shadow the night's. It
// keeps the widget's placement and starts hidden, so the enter / leave
// handlers work unchanged.

/// Dark oak, by day / by night.
pub(crate) const TOOLTIP_OAK: (ColorU, ColorU) =
    (ColorU::rgb(0x3B, 0x33, 0x27), ColorU::rgb(0x3B, 0x33, 0x27));
/// The pale ink on the oak, by day / by night.
pub(crate) const TOOLTIP_OAK_INK: (ColorU, ColorU) =
    (ColorU::rgb(0xEF, 0xE7, 0xD7), ColorU::rgb(0xEF, 0xE7, 0xD7));
/// The oak's rule: `--fl-soft1` by day, the oak's own shade by night.
pub(crate) const TOOLTIP_OAK_RULE: (ColorU, ColorU) =
    (ColorU::rgb(0x66, 0x64, 0x5C), ColorU::rgb(0x55, 0x4A, 0x3A));
/// The oak's shadow: rgba(60, 48, 30, 0.3) by day, rgba(0, 0, 0, 0.55) by night.
const TOOLTIP_SHADOW: (ColorU, ColorU) = (ColorU::new(60, 48, 30, 77), ColorU::new(0, 0, 0, 140));

/// Flora's tooltip skin.
#[must_use]
pub(crate) fn tooltip_skin() -> crate::widgets::tooltip::TooltipSkin {
    use super::decl;
    use crate::widgets::tooltip as t;
    // The widget's tip base (`tooltip::TIP_BASE`: placed below the wrapper,
    // on one line, hidden until hovered - the value the leave handler writes
    // back), then flora's oak.
    let mut tip = t::TIP_BASE.to_vec();
    tip.push(decl::font_size(13));
    tip.push(CssPropertyWithConditions::simple(CssProperty::const_font_family(SERIF_FAMILY)));
    tip.extend(decl::padding(4, 9, 4, 9));
    tip.extend(decl::radius(3));
    tip.extend(decl::themed_border(
        decl::Edges::ALL,
        1,
        TOOLTIP_OAK_RULE.0,
        TOOLTIP_OAK_RULE.1,
    ));
    tip.extend(decl::themed_fill(TOOLTIP_OAK.0, TOOLTIP_OAK.1));
    tip.extend(decl::themed_ink(TOOLTIP_OAK_INK.0, TOOLTIP_OAK_INK.1));
    tip.extend(decl::themed_shadow(3, 8, TOOLTIP_SHADOW.0, TOOLTIP_SHADOW.1));

    t::TooltipSkin {
        theme: super::UiTheme::Flora,
        wrapper: CssPropertyWithConditionsVec::from_const_slice(t::TOOLTIP_WRAPPER_STYLE),
        tip: CssPropertyWithConditionsVec::from_vec(tip),
    }
}

/// Renders a [`crate::widgets::tooltip::Tooltip`] in the flora theme.
#[must_use]
pub fn tooltip(t: crate::widgets::tooltip::Tooltip) -> Dom {
    t.build(tooltip_skin())
}

// ==== video ====
//
// The picture is the source's own; the widget's only chrome is its "no signal"
// poster. Flora draws it as the ink panel it sets code in
// (`--fl-code-bg` under a `--fl-code-bd` hairline, by day and by night) - a
// screen reads as ink on the page in both modes.

/// Flora's "no signal" poster.
#[must_use]
pub(crate) fn video_poster_style() -> CssPropertyWithConditionsVec {
    use super::decl;

    let mut v = decl::fill_box().to_vec();
    v.extend(decl::themed_fill(CODE_VIEW_BG.0, CODE_VIEW_BG.1));
    v.extend(decl::themed_border(
        decl::Edges::ALL,
        1,
        CODE_VIEW_BD.0,
        CODE_VIEW_BD.1,
    ));
    CssPropertyWithConditionsVec::from_vec(v)
}

/// Renders a [`crate::widgets::video::VideoWidget`] in the flora theme.
#[must_use]
pub fn video(w: crate::widgets::video::VideoWidget) -> Dom {
    w.build(super::UiTheme::Flora)
}

// ==== text input kinds (type=search) ====
//
// The search field's row and its clear button. `text_input.rs` builds the
// field itself (the same `text_input()` above) and wires the button's click;
// the look is the theme's. Flora's button is the recessed round badge of a
// native search field: a grey disc with a light cross.

/// The dark-theme disc of the search clear badge under the pointer: a step
/// lighter than [`DARK_BD`], still well inside the dark range.
const SEARCH_CLEAR_DARK_HOVER: ColorU = ColorU {
    r: 96,
    g: 96,
    b: 96,
    a: 255,
};

/// The clear button (a cross) of a `type=search` field, shown only while the field
/// holds text (`visible`). The widget flips `display` live on the
/// empty/non-empty transition; this is the state it is BUILT in.
#[must_use]
pub fn search_clear_button(visible: bool) -> Dom {
    // The badge's structure is the widget's (`text_input::search_clear_base`:
    // a flex box that centres the cross); flora's skin is the 14px disc.
    let mut style: Vec<CssPropertyWithConditions> =
        crate::widgets::text_input::search_clear_base(visible).to_vec();
    style.extend([
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(14))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(14))),
        CssPropertyWithConditions::simple(CssProperty::const_margin_left(
            LayoutMarginLeft::const_px(4),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            11,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(7),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(7),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(7),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(7),
        )),
    ]);
    // The disc: the icon grey in the light theme, the border grey in the dark
    // one (a light disc there would be a light island on the dark field), a
    // step towards the ink under the pointer; the cross is the page colour in
    // the light theme and the ink in the dark one.
    style.extend(CssPropertyWithConditions::themed(
        CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(&[
            StyleBackgroundContent::Color(LIGHT_ICON),
        ])),
        CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(&[
            StyleBackgroundContent::Color(DARK_BD),
        ])),
    ));
    style.extend(CssPropertyWithConditions::themed(
        CssProperty::const_text_color(StyleTextColor { inner: LIGHT_PG }),
        CssProperty::const_text_color(StyleTextColor { inner: DARK_INK }),
    ));
    style.extend(CssPropertyWithConditions::themed_on_hover(
        CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(&[
            StyleBackgroundContent::Color(LIGHT_INK),
        ])),
        CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(&[
            StyleBackgroundContent::Color(SEARCH_CLEAR_DARK_HOVER),
        ])),
    ));

    crate::widgets::widget_p_with_text(AzString::from_const_str("\u{00D7}"))
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(AzString::from_const_str(
            crate::widgets::text_input::SEARCH_CLEAR_CLASS,
        ))]))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(style))
}

/// The row of a `type=search` field: the field (which grows) and its clear
/// badge after it - the widget's row (`text_input::SEARCH_FIELD_BASE`);
/// flora paints nothing on it.
#[must_use]
pub fn search_field(field: Dom, clear: Dom) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(AzString::from_const_str(
            crate::widgets::text_input::SEARCH_FIELD_CLASS,
        ))]))
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
            crate::widgets::text_input::SEARCH_FIELD_BASE,
        ))
        .with_children(vec![field, clear].into())
}

// ==== text input kinds (invalid look) ====

/// The border of a text field whose value the user edited into an INVALID
/// state (`type=email` / `type=url` syntax, `pattern`): flora's warm brick
/// red, which sits with its paper-and-ink palette where the flat theme's
/// signal red would glare. Light mode.
pub const INVALID_RING: ColorU = ColorU {
    r: 192,
    g: 57,
    b: 43,
    a: 255,
};

/// [`INVALID_RING`] in the dark theme: a lighter coral that keeps the warmth
/// and still reads on a dark field.
pub const DARK_INVALID_RING: ColorU = ColorU {
    r: 232,
    g: 132,
    b: 122,
    a: 255,
};

/// The four border colours of the invalid look, for the light (`dark ==
/// false`) or the dark theme. `text_input.rs` writes them as an OVERRIDE on
/// the field host while the value is invalid (see its `paint_invalid_ring`);
/// an override carries no theme condition, so the mode is chosen when it is
/// written.
#[must_use]
pub fn text_input_invalid_ring(dark: bool) -> Vec<CssProperty> {
    let inner = if dark { DARK_INVALID_RING } else { INVALID_RING };
    vec![
        CssProperty::const_border_top_color(StyleBorderTopColor { inner }),
        CssProperty::const_border_right_color(StyleBorderRightColor { inner }),
        CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner }),
        CssProperty::const_border_left_color(StyleBorderLeftColor { inner }),
    ]
}

// ==== datetime-local ====

/// `<input type=datetime-local>`: the date part and the time part in one row,
/// on flora's field paper inside one rounded outline so the pair reads as ONE
/// control.
#[must_use]
pub fn datetime_local(date: Dom, time: Dom) -> Dom {
    // The widget's structure (R5), then flora's skin.
    let mut style: Vec<CssPropertyWithConditions> = crate::widgets::datetime_local::base_row();
    style.extend([
        CssPropertyWithConditions::simple(CssProperty::ColumnGap(LayoutColumnGapValue::Exact(
            LayoutColumnGap {
                inner: PixelValue::const_px(6),
            },
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_padding_left(
            LayoutPaddingLeft::const_px(6),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_right(
            LayoutPaddingRight::const_px(6),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
            3,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
            LayoutPaddingBottom::const_px(3),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(6),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(6),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(6),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(6),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
            LayoutBorderTopWidth::const_px(1),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
            LayoutBorderRightWidth::const_px(1),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
            LayoutBorderBottomWidth::const_px(1),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
            LayoutBorderLeftWidth::const_px(1),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
            StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
            StyleBorderBottomStyle {
                inner: BorderStyle::Solid,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_style(
            StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            },
        )),
    ]);
    style.extend(CssPropertyWithConditions::themed(
        CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(&[
            StyleBackgroundContent::Color(LIGHT_FLD),
        ])),
        CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(&[
            StyleBackgroundContent::Color(DARK_FLD),
        ])),
    ));
    style.extend(CssPropertyWithConditions::themed(
        CssProperty::const_border_top_color(StyleBorderTopColor { inner: LIGHT_BD }),
        CssProperty::const_border_top_color(StyleBorderTopColor { inner: DARK_BD }),
    ));
    style.extend(CssPropertyWithConditions::themed(
        CssProperty::const_border_right_color(StyleBorderRightColor { inner: LIGHT_BD }),
        CssProperty::const_border_right_color(StyleBorderRightColor { inner: DARK_BD }),
    ));
    style.extend(CssPropertyWithConditions::themed(
        CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: LIGHT_BD }),
        CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: DARK_BD }),
    ));
    style.extend(CssPropertyWithConditions::themed(
        CssProperty::const_border_left_color(StyleBorderLeftColor { inner: LIGHT_BD }),
        CssProperty::const_border_left_color(StyleBorderLeftColor { inner: DARK_BD }),
    ));

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(AzString::from_const_str(
            crate::widgets::datetime_local::DATETIME_LOCAL_CLASS,
        ))]))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(style))
        .with_children(vec![date, time].into())
}

// ==== form ====

/// `<form>`: a `NodeType::Form` node stacking its content in a column, a
/// little airier than flat. The form paints nothing of its own in either mode
/// - it is structure, and its controls carry their own light and dark faces.
#[must_use]
pub fn form(children: azul_core::dom::DomVec) -> Dom {
    Dom::create_node(NodeType::Form)
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(AzString::from_const_str(
            crate::widgets::form::FORM_CLASS,
        ))]))
        .with_css_props(CssPropertyWithConditionsVec::from_vec({
            // The widget's structure (R5), then flora's airier gap.
            let mut style = crate::widgets::form::base_form();
            style.push(CssPropertyWithConditions::simple(CssProperty::RowGap(
                LayoutRowGapValue::Exact(LayoutRowGap {
                    inner: PixelValue::const_px(10),
                }),
            )));
            style
        }))
        .with_children(children)
}

// ==== badge ====
//
// flora.css `.pill`: "a small-caps label inside a hairline border. Outlined,
// not filled" - raised paper (--fl-rT -> --fl-rB) behind a 1px --fl-bd2 edge,
// --fl-soft1 ink, bold and tracked out, the 3px house radius. A coloured
// badge is `.pill-live`, "a small stone": its own colour under the depth rig
// the accent stone carries, a deep edge and --fl-on-acc ink. A stone is its
// own colour in both modes ("the accent keeps its stone"); the paper pill
// takes the night face, edge and ink.
//
// The semantic stones below are shared by every flora widget with a kind
// (badge, chip, alert): the accent for Primary and the alternates flora.css
// lists as holding up against its ground - leaf, clay, slate - plus an amber
// cut for warnings, since a brass (metal) fill would break the house rule
// "metal on borders, never a field".

/// One of flora's semantic stones: the face, the deep edge it is set in, the
/// soft tint it washes a panel with, and the glow that reads on a dark ground.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct FloraStone {
    /// The stone's face.
    pub stone: ColorU,
    /// The edge it is set in (and its pressed face).
    pub deep: ColorU,
    /// The soft tint of the same colour, for a light panel.
    pub soft: ColorU,
    /// The stone's highlight: its colour where a dark ground needs it.
    pub glow: ColorU,
}

/// The accent stone (`--fl-acc` / `--fl-deep` / `--fl-soft` / `--fl-glow`).
pub const STONE_ACCENT: FloraStone = FloraStone {
    stone: LIGHT_ACC,
    deep: LIGHT_DEEP,
    soft: LIGHT_SOFT,
    glow: LIGHT_GLOW,
};

/// Leaf: flora.css's green alternate (#44684F / #2F4C39 / #E1E6E1 / #7FA98C).
pub const STONE_LEAF: FloraStone = FloraStone {
    stone: ColorU::rgb(0x44, 0x68, 0x4F),
    deep: ColorU::rgb(0x2F, 0x4C, 0x39),
    soft: ColorU::rgb(0xE1, 0xE6, 0xE1),
    glow: ColorU::rgb(0x7F, 0xA9, 0x8C),
};

/// Clay: flora.css's red alternate (#7E4A42 / #5E332D / #EAE0DD / #B3837A).
pub const STONE_CLAY: FloraStone = FloraStone {
    stone: ColorU::rgb(0x7E, 0x4A, 0x42),
    deep: ColorU::rgb(0x5E, 0x33, 0x2D),
    soft: ColorU::rgb(0xEA, 0xE0, 0xDD),
    glow: ColorU::rgb(0xB3, 0x83, 0x7A),
};

/// Slate: flora.css's blue-grey alternate (#4A5C6B / #354551 / #DEE3E7 /
/// #8AA0B0).
pub const STONE_SLATE: FloraStone = FloraStone {
    stone: ColorU::rgb(0x4A, 0x5C, 0x6B),
    deep: ColorU::rgb(0x35, 0x45, 0x51),
    soft: ColorU::rgb(0xDE, 0xE3, 0xE7),
    glow: ColorU::rgb(0x8A, 0xA0, 0xB0),
};

/// Amber: the warning stone, cut to the same depth as the others so
/// --fl-on-acc reads on it at better than 5:1.
pub const STONE_AMBER: FloraStone = FloraStone {
    stone: ColorU::rgb(0x8A, 0x5A, 0x1E),
    deep: ColorU::rgb(0x6B, 0x44, 0x15),
    soft: ColorU::rgb(0xF1, 0xE6, 0xD6),
    glow: ColorU::rgb(0xC4, 0x93, 0x5A),
};

/// The badge kind's stone, or `None` for the neutral paper pill.
const fn badge_stone(kind: crate::widgets::badge::BadgeKind) -> Option<FloraStone> {
    use crate::widgets::badge::BadgeKind;
    match kind {
        BadgeKind::Default => None,
        BadgeKind::Primary => Some(STONE_ACCENT),
        BadgeKind::Success => Some(STONE_LEAF),
        BadgeKind::Danger => Some(STONE_CLAY),
        BadgeKind::Warning => Some(STONE_AMBER),
        BadgeKind::Info => Some(STONE_SLATE),
    }
}

/// The flora pill for one badge kind.
fn flora_badge_style(kind: crate::widgets::badge::BadgeKind) -> Vec<CssPropertyWithConditions> {
    use super::decl;

    // The pill's base (its centred, hugging row), then flora's skin.
    let mut style = crate::widgets::badge::BADGE_BASE.to_vec();
    style.extend([
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            11,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        // `font-weight: 700; letter-spacing: 0.1em` - the small-caps label,
        // tracked a little tighter because the face here is the UI sans.
        decl::bold(),
        decl::letter_spacing_em(0.08),
    ]);
    style.extend(decl::padding(1, 8, 1, 8));
    style.extend(decl::radius(3));
    style.extend(decl::border(1));
    match badge_stone(kind) {
        None => {
            style.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD2));
            style.extend(decl::themed_layers(
                vec![RAISED_FACE_LIGHT],
                vec![RAISED_FACE_DARK],
            ));
            style.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
        }
        Some(stone) => {
            style.extend(decl::border_colors(stone.deep).map(CssPropertyWithConditions::simple));
            style.push(CssPropertyWithConditions::simple(decl::layers(stone_face(
                stone.stone,
                STONE_STREAK,
            ))));
            style.push(CssPropertyWithConditions::simple(decl::ink(LIGHT_ON_ACC)));
        }
    }
    style
}

/// The flora badge: flora.css's `.pill` (neutral) or `.pill-live` (a stone,
/// for every coloured kind). A caller's `badge_style` is taken as it is.
#[must_use]
pub fn badge(b: crate::widgets::badge::Badge) -> Dom {
    static FLORA_BADGE_CLASSES: &[IdOrClass] = &[
        Class(AzString::from_const_str("__azul-native-badge")),
        Class(AzString::from_const_str("__azul-theme-flora")),
    ];

    let crate::widgets::badge::Badge {
        string,
        kind,
        badge_style,
        ..
    } = b;
    let style = badge_style
        .into_option()
        .unwrap_or_else(|| CssPropertyWithConditionsVec::from_vec(flora_badge_style(kind)));

    crate::widgets::widget_p_with_text(string)
        .with_ids_and_classes(IdOrClassVec::from_const_slice(FLORA_BADGE_CLASSES))
        .with_css_props(style)
}

// ==== divider ====
//
// flora.css `hr`: "A hairline" - 1px of --fl-sep, which is #D8D5CE by day and
// #383838 at night. The page gives an `hr` 40px of air; inside a widget tree
// that would push everything apart, so the flora rule takes 8px, twice the
// flat rule's 4px, which is as much of the house's slowness as a separator
// can carry.

/// The flora rule for one orientation: the widget's structure (R5), then the
/// geometry, then the colour with its night twin.
fn flora_divider_style(
    orientation: crate::widgets::divider::DividerOrientation,
) -> Vec<CssPropertyWithConditions> {
    use crate::widgets::divider::DividerOrientation;

    // Span the parent's cross axis, never grow along the main one.
    let mut style = crate::widgets::divider::DIVIDER_BASE.to_vec();
    match orientation {
        DividerOrientation::Horizontal => {
            style.push(CssPropertyWithConditions::simple(CssProperty::const_height(
                LayoutHeight::const_px(1),
            )));
            style.extend(super::decl::margin(8, 0, 8, 0));
        }
        DividerOrientation::Vertical => {
            style.push(CssPropertyWithConditions::simple(CssProperty::const_width(
                LayoutWidth::const_px(1),
            )));
            style.extend(super::decl::margin(0, 8, 0, 8));
        }
    }
    style.extend(super::decl::themed_fill(LIGHT_SEP, DARK_SEP));
    style
}

/// The flora divider: a 1px --fl-sep hairline. A caller's `divider_style` is
/// taken as it is.
#[must_use]
pub fn divider(d: crate::widgets::divider::Divider) -> Dom {
    static FLORA_DIVIDER_CLASSES: &[IdOrClass] = &[
        Class(AzString::from_const_str("__azul-native-divider")),
        Class(AzString::from_const_str("__azul-theme-flora")),
    ];
    let orientation = d.orientation;
    let style = d.divider_style.into_option().unwrap_or_else(|| {
        CssPropertyWithConditionsVec::from_vec(flora_divider_style(orientation))
    });
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(FLORA_DIVIDER_CLASSES))
        .with_css_props(style)
}

// ==== spinner ====
//
// Flora's own indicator is the spoke wheel - "motion is slow and continuous;
// light moves across a stone, it does not snap" is what the travelling
// opacity wave is - drawn in the house ink (--fl-ink, #262521 by day and
// #E7E7E7 at night) rather than pure black. Asked for the ring, flora draws
// it in the accent stone (--fl-acc), lifted at night to the stone's own
// highlight (--fl-glow) the way flora.css lifts its focus colour, because the
// deep blue disappears on the night ground. The fade takes --fl-dur (0.42 s),
// flora's duration for a state change.

/// The flora spinner: the spoke wheel in the house ink, or the ring in the
/// accent stone when asked.
#[must_use]
pub fn spinner(s: crate::widgets::spinner::Spinner) -> Dom {
    use crate::widgets::spinner::{SpinnerLook, SpinnerStyle};
    crate::widgets::spinner::build(
        s,
        &SpinnerLook {
            auto: SpinnerStyle::Spokes,
            spoke_ink: (
                StyleBackgroundContent::Color(LIGHT_INK),
                Some(StyleBackgroundContent::Color(DARK_INK)),
            ),
            arc_ink: (
                StyleBackgroundContent::Color(LIGHT_ACC),
                Some(StyleBackgroundContent::Color(DARK_GLOW)),
            ),
            fade_ms: 420,
            marker: Some("__azul-theme-flora"),
        },
    )
}

// ==== chip ====
//
// A flora tag is the badge's `.pill` cut for content: the same raised paper in
// a --fl-bd2 hairline and the 3px house radius, but set in the content ink
// (--fl-ink2) at normal weight, because a tag is a word the user wrote, not a
// label the application stamped. A coloured tag is one of the shared stones.
// The remove "x" has no colour of its own - it inherits the pill's ink, so it
// reads on paper and on every stone - and lights up under the pointer with a
// translucent wash of that ink. Focus is flora.css's `--focus-color`: the
// accent, lifted to its glow at night, drawn as a halo.

/// The tag kind's stone, or `None` for the neutral paper tag.
const fn chip_stone(kind: crate::widgets::chip::ChipKind) -> Option<FloraStone> {
    use crate::widgets::chip::ChipKind;
    match kind {
        ChipKind::Default => None,
        ChipKind::Primary => Some(STONE_ACCENT),
        ChipKind::Success => Some(STONE_LEAF),
        ChipKind::Danger => Some(STONE_CLAY),
        ChipKind::Warning => Some(STONE_AMBER),
        ChipKind::Info => Some(STONE_SLATE),
    }
}

/// The flora pill for one tag kind.
fn flora_chip_container(kind: crate::widgets::chip::ChipKind) -> Vec<CssPropertyWithConditions> {
    use super::decl;

    // The pill's base (its hugging row), then flora's skin.
    let mut style = crate::widgets::chip::CHIP_CONTAINER_BASE.to_vec();
    style.extend([
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            12,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ]);
    style.extend(decl::padding(3, 7, 3, 9));
    style.extend(decl::radius(3));
    style.extend(decl::border(1));
    match chip_stone(kind) {
        None => {
            style.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD2));
            style.extend(decl::themed_layers(
                vec![RAISED_FACE_LIGHT],
                vec![RAISED_FACE_DARK],
            ));
            style.extend(decl::themed_ink(LIGHT_INK2, DARK_INK2));
        }
        Some(stone) => {
            style.extend(decl::border_colors(stone.deep).map(CssPropertyWithConditions::simple));
            style.push(CssPropertyWithConditions::simple(decl::layers(stone_face(
                stone.stone,
                STONE_STREAK,
            ))));
            style.push(CssPropertyWithConditions::simple(decl::ink(LIGHT_ON_ACC)));
        }
    }
    style
}

/// The flora chip: flora's paper tag or a stone, with a quiet remove button
/// and the accent's focus halo.
#[must_use]
pub fn chip(c: crate::widgets::chip::Chip) -> Dom {
    use super::decl;
    use crate::widgets::chip::ChipLook;

    // The skins: `chip::build` lays the label's and the "x"'s over their
    // bases (the label's hug; the "x"'s hug, pointer and unselectable glyph).
    let mut label_focus = decl::radius(3).to_vec();
    label_focus.extend(decl::focus_halo(LIGHT_ACC, DARK_GLOW));

    let mut remove = vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            13,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_margin_left(
            LayoutMarginLeft::const_px(5),
        )),
    ];
    remove.extend(decl::padding(0, 3, 0, 3));
    remove.extend(decl::radius(3));
    // A wash of the pill's own ink: dark on paper by day, light at night.
    remove.extend(decl::hover_fill(
        ColorU::new(38, 37, 33, 26),
        ColorU::new(255, 255, 255, 36),
    ));
    remove.extend(decl::active_fill(
        ColorU::new(38, 37, 33, 46),
        ColorU::new(255, 255, 255, 56),
    ));
    remove.extend(decl::focus_halo(LIGHT_ACC, DARK_GLOW));

    crate::widgets::chip::build(
        c,
        &ChipLook {
            container: flora_chip_container,
            label: Vec::new(),
            label_focus,
            remove,
            marker: Some("__azul-theme-flora"),
        },
    )
}

// ==== alert ====
//
// A flora alert is a leaf laid on the page: "a pale, near-neutral ground with
// only the faintest warmth in it" - here the faintest wash of the kind's stone
// (its soft tint) - in a --fl-bd hairline, with a 3px thread of the stone
// down the left edge the way flora.css threads a quotation with metal, and
// the leaf's --fl-shadow-1 under it. At night the leaf is the night surface,
// neutral, and only the thread keeps the kind, lifted to the stone's glow so
// it still reads on the dark ground. The close button is quiet ink
// (--fl-soft1) that comes up to --fl-ink under the pointer, and rings in
// flora's focus colour.

/// The alert kind's stone.
const fn alert_stone(kind: crate::widgets::alert::AlertKind) -> FloraStone {
    use crate::widgets::alert::AlertKind;
    match kind {
        AlertKind::Info => STONE_ACCENT,
        AlertKind::Success => STONE_LEAF,
        AlertKind::Warning => STONE_AMBER,
        AlertKind::Danger => STONE_CLAY,
    }
}

/// `--fl-shadow-1`: `0 1px 2px rgba(48, 45, 38, 0.14)`, and its night value
/// `rgba(0, 0, 0, 0.55)`.
const LEAF_SHADOW_LIGHT: ColorU = ColorU::new(48, 45, 38, 36);
const LEAF_SHADOW_DARK: ColorU = ColorU::new(0, 0, 0, 140);

/// The flora banner for one alert kind.
fn flora_alert_container(
    kind: crate::widgets::alert::AlertKind,
) -> Vec<CssPropertyWithConditions> {
    use super::decl;

    let stone = alert_stone(kind);
    // The banner's base (its row), then flora's skin.
    let mut style = crate::widgets::alert::ALERT_CONTAINER_BASE.to_vec();
    style.extend([
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            14,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ]);
    style.extend(decl::padding(12, 14, 12, 12));
    style.extend(decl::radius(3));
    style.extend(decl::border(1));
    // The thread: heavier than the hairline, in the stone.
    style.extend(decl::border_left(3));
    style.extend(decl::themed_border_color(LIGHT_BD, DARK_BD));
    style.extend(decl::themed_border_left_color(stone.stone, stone.glow));
    style.extend(decl::themed_fill(stone.soft, DARK_SUR));
    style.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    style.extend(decl::themed_shadow(1, 2, LEAF_SHADOW_LIGHT, LEAF_SHADOW_DARK));
    style
}

/// The flora alert: the leaf banner with its stone thread, and a quiet close
/// button.
#[must_use]
pub fn alert(a: crate::widgets::alert::Alert) -> Dom {
    use super::decl;
    use crate::widgets::alert::AlertLook;

    // The skins: `alert::build` lays the message's and the close button's
    // over their bases (the message's growth; the button's hug, pointer and
    // unselectable glyph).
    let mut close = vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            16,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_margin_left(
            LayoutMarginLeft::const_px(12),
        )),
    ];
    close.extend(decl::padding(0, 4, 0, 4));
    close.extend(decl::radius(3));
    close.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    close.extend(decl::hover_ink(LIGHT_INK, DARK_INK));
    close.extend(decl::hover_fill(
        ColorU::new(38, 37, 33, 20),
        ColorU::new(255, 255, 255, 28),
    ));
    close.extend(decl::focus_halo(LIGHT_ACC, DARK_GLOW));

    crate::widgets::alert::build(
        a,
        &AlertLook {
            container: flora_alert_container,
            message: Vec::new(),
            close,
            marker: Some("__azul-theme-flora"),
        },
    )
}

// ==== card ====
//
// flora.css names the card's surface itself: --fl-sur is "a leaf laid on the
// page: cards, panels". A flora card is that leaf in a --fl-bd hairline, at
// the house's largest radius (--fl-r2, 5px - "nothing is rounder than 5"),
// lifted off the page by a warm shadow a little deeper than an alert's
// (--fl-shadow-2's near half), and it writes the content it holds in --fl-ink.
// At night every one of those takes its night value. A card takes no focus.

/// The near half of `--fl-shadow-2`: `0 2px 5px rgba(48, 45, 38, 0.16)`; at
/// night `rgba(0, 0, 0, 0.5)`.
const CARD_LEAF_SHADOW_LIGHT: ColorU = ColorU::new(48, 45, 38, 41);
const CARD_LEAF_SHADOW_DARK: ColorU = ColorU::new(0, 0, 0, 128);

/// The flora card's box, after the card's own flex-grow and its base (the
/// column, `card::CARD_BASE`).
fn flora_card_style() -> Vec<CssPropertyWithConditions> {
    use super::decl;

    let mut style = decl::padding(14, 14, 14, 14).to_vec();
    style.extend(decl::radius(5));
    style.extend(decl::border(1));
    style.extend(decl::themed_border_color(LIGHT_BD, DARK_BD));
    style.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
    style.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    style.extend(decl::themed_shadow(
        2,
        5,
        CARD_LEAF_SHADOW_LIGHT,
        CARD_LEAF_SHADOW_DARK,
    ));
    style
}

/// The flora card: a leaf on the page.
#[must_use]
pub fn card(c: crate::widgets::card::Card) -> Dom {
    static FLORA_CARD_CLASSES: &[IdOrClass] = &[
        Class(AzString::from_const_str("__azul-native-card")),
        Class(AzString::from_const_str("__azul-theme-flora")),
    ];
    crate::widgets::card::build(
        c,
        &flora_card_style(),
        IdOrClassVec::from_const_slice(FLORA_CARD_CLASSES),
    )
}

// ==== frame ====
//
// A flora group box keeps the frame's shape - a rule, the title, a rule, then
// the bordered content - and speaks flora in the two places a group box has a
// voice. The title is flora.css's `.fl-label`, "the small-caps label that
// sits over every group": bold, tracked out, in --fl-soft1. The rules are
// --fl-bd, the house's hairline, instead of a neutral grey. At night the
// label and the rules take their night values. A frame takes no focus.

/// The flora frame's look: the frame's own geometry, flora's label and
/// rules.
#[must_use]
pub(crate) fn frame_look() -> crate::widgets::frame::FrameLook {
    use super::decl;
    use crate::widgets::frame::{
        FrameLook, FRAME_AFTER_STYLE, FRAME_BEFORE_STYLE, FRAME_CONTENT_STYLE,
        FRAME_HEADER_STYLE, FRAME_ROOT_STYLE, FRAME_TITLE_STYLE,
    };

    // Each rule keeps the frame's geometry; its colour is appended after the
    // flat one (and its system twin), so flora's pair is the one that wins.
    let mut before = FRAME_BEFORE_STYLE.to_vec();
    before.extend(decl::themed_border_top_color(LIGHT_BD, DARK_BD));
    before.extend(decl::themed_border_left_color(LIGHT_BD, DARK_BD));

    let mut after = FRAME_AFTER_STYLE.to_vec();
    after.extend(decl::themed_border_top_color(LIGHT_BD, DARK_BD));
    after.extend(CssPropertyWithConditions::themed(
        CssProperty::const_border_right_color(StyleBorderRightColor { inner: LIGHT_BD }),
        CssProperty::const_border_right_color(StyleBorderRightColor { inner: DARK_BD }),
    ));

    let mut content = FRAME_CONTENT_STYLE.to_vec();
    content.extend(decl::themed_border_color(LIGHT_BD, DARK_BD));

    // `.fl-label`: the capitals in Garamond (`--font-caps`), bold, tracked
    // 0.12em, in --fl-soft1 - the specimen's section title.
    let mut title = FRAME_TITLE_STYLE.to_vec();
    title.extend(caps(CAPS_TITLE));
    title.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    FrameLook {
        root: FRAME_ROOT_STYLE.to_vec(),
        header: FRAME_HEADER_STYLE.to_vec(),
        before,
        title,
        after,
        content,
        marker: Some("__azul-theme-flora"),
    }
}

/// The flora frame: the frame's own geometry, flora's label and rules.
#[must_use]
pub fn frame(f: crate::widgets::frame::Frame) -> Dom {
    crate::widgets::frame::build(f, &frame_look())
}

// ==== breadcrumb ====
//
// flora.css writes links "in brass ink": --fl-qt, "muted almost to gray - it
// should read as a different ink, not as a highlight", deepening to --fl-qt2
// under the pointer and underlined. A flora trail is those links, ending on
// the current page in the house ink, divided by a quiet chevron in --fl-soft2
// rather than a slash - a trail, not a path. Focus is flora's accent halo.
// Every ink has its night value (at night the brass warms up: "it is the only
// thing in the room still catching a light").

/// The flora breadcrumb: brass-ink links and a quiet chevron.
#[must_use]
pub fn breadcrumb(b: crate::widgets::breadcrumb::Breadcrumb) -> Dom {
    use super::decl;
    use crate::widgets::breadcrumb::BreadcrumbLook;

    // The skins: `breadcrumb::build` lays each over the crumb's base (its
    // hug, its unselectable text, the link's pointer).
    let mut item = decl::themed_ink(LIGHT_QT, DARK_QT).to_vec();
    item.extend(decl::radius(3));
    item.extend(decl::hover_ink(LIGHT_QT2, DARK_QT2));
    item.extend(decl::hover_underline());
    item.extend(decl::focus_halo(LIGHT_ACC, DARK_GLOW));

    let mut current = decl::themed_ink(LIGHT_INK, DARK_INK).to_vec();
    current.push(decl::semibold());

    let mut separator = decl::themed_ink(LIGHT_SOFT2, DARK_SOFT2).to_vec();
    separator.extend(decl::margin(0, 7, 0, 7));

    crate::widgets::breadcrumb::build(
        b,
        &BreadcrumbLook {
            item,
            current,
            separator,
            separator_glyph: AzString::from_const_str("\u{203A}"),
            marker: Some("__azul-theme-flora"),
        },
    )
}

// ==== accordion ====
//
// flora.css's own collapsible list is the FAQ: items ruled apart by a
// --color-border hairline, questions set semibold that take the brass accent
// under the pointer. A flora accordion is that list on a leaf: the panel is
// --fl-sur in a --fl-bd hairline at the house radius, each header is raised
// paper (the standard command's face) that lifts to the hover face and turns
// brass under the pointer and presses in, the title is semibold. Focus is
// flora's accent halo, inside the header (the panel clips). At night every
// face, rule and ink takes its night value.

/// The flora accordion: a FAQ list on a leaf.
#[must_use]
pub fn accordion(a: crate::widgets::accordion::Accordion) -> Dom {
    use super::decl;
    use crate::widgets::accordion::AccordionLook;

    // The skins: `accordion::build` lays each over the part's base (the
    // panel's clipped column, the header's row and pointer, ...).
    let mut container = vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            14,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SERIF_FAMILY)),
    ];
    container.extend(decl::border(1));
    container.extend(decl::themed_border_color(LIGHT_BD, DARK_BD));
    container.extend(decl::radius(3));
    container.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
    container.extend(decl::themed_ink(LIGHT_INK, DARK_INK));

    let mut section = decl::border_bottom(1).to_vec();
    section.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));

    let mut header = decl::padding(10, 12, 10, 12).to_vec();
    header.extend(decl::themed_layers(
        vec![RAISED_FACE_LIGHT],
        vec![RAISED_FACE_DARK],
    ));
    header.extend(decl::hover_layers(
        vec![HOVER_FACE_LIGHT],
        vec![HOVER_FACE_DARK],
    ));
    header.extend(decl::active_layers(
        vec![PRESSED_FACE_LIGHT],
        vec![PRESSED_FACE_DARK],
    ));
    header.extend(decl::hover_ink(LIGHT_QT, DARK_QT));
    header.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));

    let title = vec![decl::semibold()];

    crate::widgets::accordion::build(
        a,
        &AccordionLook {
            container,
            section,
            header,
            title,
            // flora.css's FAQ `+`, a cross when open (rotate 45deg); it takes
            // the header's ink, so it turns brass under the pointer too.
            chevron: crate::widgets::accordion::chevron_box(18),
            chevron_icon: "add",
            chevron_turn_deg: 45,
            marker: Some("__azul-theme-flora"),
        },
    )
}

// ==== menubar ====
//
// flora.css gives toolbars their own surface, --fl-strip, and closes a strip
// with a rule. A flora menu bar is that strip in the house ink, closed along
// its foot by a --fl-bd hairline; its items behave like flora's nav links:
// the hover face (--fl-hT -> --fl-hB) under the pointer, the pressed face
// while held - light entering a raised face, then the face pushed in. Every
// surface, rule and ink has its night value. The items take no focus, as in
// the flat bar: the keyboard reaches a menu through the platform.

/// The flora menu bar: flora's toolbar strip.
#[must_use]
pub fn menubar(m: crate::widgets::menubar::Menubar) -> Dom {
    use super::decl;

    // The widget's structure first (R5), then flora's strip.
    let mut bar = crate::widgets::menubar::base_bar();
    bar.extend([
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::Px(
            PixelValue::const_percent(100),
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(28))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            14,
        ))),
    ]);
    bar.extend(decl::padding(0, 0, 0, 4));
    bar.extend(decl::themed_fill(LIGHT_STRIP, DARK_STRIP));
    bar.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    bar.extend(decl::border_bottom(1));
    bar.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));

    let mut item = crate::widgets::menubar::base_item();
    item.extend(decl::padding(0, 11, 0, 11));
    item.extend(decl::radius(3));
    item.extend(decl::hover_layers(
        vec![HOVER_FACE_LIGHT],
        vec![HOVER_FACE_DARK],
    ));
    item.extend(decl::active_layers(
        vec![PRESSED_FACE_LIGHT],
        vec![PRESSED_FACE_DARK],
    ));

    let bar = CssPropertyWithConditionsVec::from_vec(bar);
    let item = CssPropertyWithConditionsVec::from_vec(item);
    crate::widgets::menubar::build(
        &m.menu,
        Some("__azul-theme-flora"),
        |dom| dom.with_css_props(bar.clone()),
        |dom| dom.with_css_props(item.clone()),
    )
}

// ==== color_input ====
//
// A flora colour swatch is a sample laid in a frame: its colour inside a
// --fl-bd2 hairline at the house radius, the frame darkening to --fl-bd3
// under the pointer and taking flora's focus colour on focus. The picker is a
// leaf lifted off the page (--fl-sur in a --fl-bd rule, 5px radius, a warm
// shadow); its preview is framed like the swatch, its eyedropper is raised
// paper in --fl-icon ink, its grip handle a --fl-bd bar. The plane, hue and
// alpha bars ring in the accent on focus. At night every surface, rule and
// ink takes its night value.
//
// The picker's strings are CSS because the picker is (its live updates restyle
// its parts through the same channel); `@media (prefers-color-scheme: dark)`
// carries each night value. Its shadow is written at a quarter of the alpha it
// should show: the `box-shadow` shorthand lays the same shadow on all four of
// azul's per-side slots, and the painter draws every one.

/// The flora picker panel: a leaf lifted off the page (its skin; the column
/// is the widget's base, `PICKER_PANEL_BASE_CSS`).
const FLORA_PICKER_PANEL_CSS: &str =
    "gap: 8px; padding: 10px; background: #F2F1ED; border: 1px solid #C6C3BB; border-radius: \
     5px; box-shadow: 0px 6px 14px rgba(48, 45, 38, 0.06); font-size: 12px; color: #262521; \
     @media (prefers-color-scheme: dark) { background: #232323; border-color: #3F3F3F; color: \
     #E7E7E7; box-shadow: 0px 6px 14px rgba(0, 0, 0, 0.18); }";

/// The flora preview: framed like the swatch (its positioned clip is the
/// widget's base, `PICKER_PREVIEW_BASE_CSS`).
const FLORA_PICKER_PREVIEW_CSS: &str =
    "width: 28px; height: 28px; border-radius: 3px; border: 1px solid #B4B1A9; @media \
     (prefers-color-scheme: dark) { border-color: #4A4A4A; }";

/// The flora eyedropper: raised paper, --fl-icon ink (its centred box and
/// pointer are the widget's base, `PICKER_EYEDROPPER_BASE_CSS`).
const FLORA_PICKER_EYEDROPPER_CSS: &str =
    "width: 28px; height: 28px; border: 1px solid #B4B1A9; border-radius: 3px; background: \
     linear-gradient(#FAF9F5, #ECEAE4); color: #56544C; font-size: 18px; @media \
     (prefers-color-scheme: dark) { background: linear-gradient(#333333, #292929); color: \
     #BEBEBE; border-color: #4A4A4A; }";

/// The flora grip handle: a --fl-bd bar.
const FLORA_PICKER_GRIP_HANDLE_CSS: &str =
    "width: 36px; height: 4px; border-radius: 2px; background: #C6C3BB; @media \
     (prefers-color-scheme: dark) { background: #3F3F3F; }";

/// The flora colour input: a sample in a hairline frame, and a leaf picker.
#[must_use]
pub fn color_input(c: crate::widgets::color_input::ColorInput) -> Dom {
    use super::decl;
    use crate::widgets::color_input::ColorInputLook;

    let mut swatch = decl::border(1).to_vec();
    swatch.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD2));
    swatch.extend(decl::radius(3));
    swatch.extend(decl::hover_border_color(LIGHT_BD3, DARK_BD3));
    swatch.extend(decl::focus_ring(LIGHT_ACC, DARK_GLOW));

    crate::widgets::color_input::build(
        c,
        &ColorInputLook {
            swatch,
            panel_css: FLORA_PICKER_PANEL_CSS,
            preview_css: FLORA_PICKER_PREVIEW_CSS,
            eyedropper_css: FLORA_PICKER_EYEDROPPER_CSS,
            grip_handle_css: FLORA_PICKER_GRIP_HANDLE_CSS,
            slider_focus: decl::focus_halo(LIGHT_ACC, DARK_GLOW).to_vec(),
            marker: Some("__azul-theme-flora"),
        },
    )
}

// ==== date_picker ====
//
// A flora date picker is a paper field over a paper calendar. The field is
// flora's field face (--fl-fld, "input fields") in a --fl-bd2 hairline at the
// house radius; it darkens its rule under the pointer and rings in the accent
// on focus. The calendar is a leaf (--fl-sur in --fl-bd, 5px, the card's
// shadow): the month (or, picking a month, the year) in semibold ink between
// brass buttons (--fl-qt, flora's quiet-action ink), the weekday names in
// --fl-soft2, the cells in --fl-ink lifting to the hover face under the
// pointer, and the pick - the day, every day of the week, the month - cut as
// the accent stone, which, being a stone, is its own colour at night. Every
// stop rings in the accent; every other surface and ink has its night value,
// and a click repaints the grid with the same stone and inks.

/// One grid cell's geometry - the widget's own box, without its colours.
fn flora_day_geometry(selected: bool) -> Vec<CssPropertyWithConditions> {
    crate::widgets::date_picker::build_day_cell_style(selected)
        .into_library_owned_vec()
        .into_iter()
        .filter(|p| {
            !matches!(
                p.property,
                CssProperty::BackgroundContent(_) | CssProperty::TextColor(_)
            )
        })
        .collect()
}

/// The flora date picker: a paper field over a paper calendar, in every
/// mode.
#[must_use]
pub fn date_picker(d: crate::widgets::date_picker::DatePicker) -> Dom {
    crate::widgets::date_picker::build(d, &date_picker_look())
}

/// The flora date picker's look, part by part - what [`date_picker`] builds
/// with, and what the date range picker draws its two calendars in.
#[must_use]
pub(crate) fn date_picker_look() -> crate::widgets::date_picker::DatePickerLook {
    use super::decl;
    use crate::widgets::date_picker::DatePickerLook;

    let mut look = DatePickerLook::established();

    // The field: flora's field face, appended after the established one so
    // its pair is the one that wins in both modes.
    look.field.extend(decl::themed_fill(LIGHT_FLD, DARK_FLD));
    look.field.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD2));
    look.field.extend(decl::radius(3));
    look.field.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    look.field.extend(decl::hover_border_color(LIGHT_BD3, DARK_BD3));
    look.field.extend(decl::focus_ring(LIGHT_ACC, DARK_GLOW));

    // The calendar: a leaf.
    look.panel.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
    look.panel.extend(decl::themed_border_color(LIGHT_BD, DARK_BD));
    look.panel.extend(decl::radius(5));
    look.panel.extend(decl::themed_shadow(
        2,
        5,
        CARD_LEAF_SHADOW_LIGHT,
        CARD_LEAF_SHADOW_DARK,
    ));

    look.header_label.push(decl::semibold());
    look.header_label.extend(decl::themed_ink(LIGHT_INK, DARK_INK));

    look.nav.extend(decl::themed_ink(LIGHT_QT, DARK_QT));
    look.nav.extend(decl::hover_ink(LIGHT_QT2, DARK_QT2));
    look.nav.extend(decl::radius(3));
    look.nav.extend(decl::focus_halo(LIGHT_ACC, DARK_GLOW));

    look.weekday.extend(decl::themed_ink(LIGHT_SOFT2, DARK_SOFT2));

    // The cells are built from the widget's geometry alone: the picked one
    // is a stone in both modes, so it must carry no dark twin at all. The
    // month grid takes these faces too (three cells wide).
    let stone = || stone_face(LIGHT_ACC, STONE_STREAK);
    let mut chosen = flora_day_geometry(true);
    chosen.extend(decl::radius(3));
    chosen.push(CssPropertyWithConditions::simple(decl::layers(stone())));
    chosen.push(CssPropertyWithConditions::simple(decl::ink(LIGHT_ON_ACC)));
    chosen.extend(decl::focus_halo(LIGHT_ACC, DARK_GLOW));
    look.day_selected = chosen;

    let mut other = flora_day_geometry(false);
    other.extend(decl::radius(3));
    other.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    other.extend(decl::hover_layers(
        vec![HOVER_FACE_LIGHT],
        vec![HOVER_FACE_DARK],
    ));
    other.extend(decl::focus_halo(LIGHT_ACC, DARK_GLOW));
    look.day_other = other;
    // Today: a 1px ring inside the cell in the accent, the glow at night.
    look.day_today = CssPropertyWithConditions::themed(
        decl::shadow(0, 0, 1, LIGHT_ACC, true),
        decl::shadow(0, 0, 1, DARK_GLOW, true),
    )
    .to_vec();
    look.marker = Some("__azul-theme-flora");
    look
}

// ==== combobox ====
//
// A flora combobox is a flora FIELD with a LEAF of options under it
// (`doc/templates/flora.css`). The field is the number field's: field paper
// (`--fl-fld`; the night field `text_input` uses) under flora ink, a
// `--fl-bd2` hairline, the house radius and the well a field is sunk in
// (`--fl-well`), ringed on focus in the accent by day and the stone's glow by
// night (`--focus-color`). The arrow is written in icon ink (`--fl-icon`).
// The list is the popover's small leaf - `--fl-sur`, `--fl-bd2`, the nearer
// shadow of `--fl-shadow-2`, square where it meets the field. Its rows are
// flora ink, wash to `--fl-hov` under the pointer, and - Tab stops - take the
// inset focus ring. Geometry is the widget's own (paddings, min-width).

/// Flora's combobox skin.
#[must_use]
pub(crate) fn combobox_skin() -> crate::widgets::combobox::ComboBoxSkin {
    use super::decl;
    use crate::widgets::combobox as c;
    type P = CssPropertyWithConditions;

    // The field: the widget's structure (R5), flora's field paper sunk in
    // its well.
    let mut field = c::COMBOBOX_FIELD_BASE.to_vec();
    field.extend(decl::padding(3, 4, 3, 4));
    field.extend(decl::themed_border(decl::Edges::ALL, 1, LIGHT_BD2, DARK_BD));
    field.extend(decl::radius(3));
    field.extend(decl::themed_fill(LIGHT_FLD, DARK_SUR));
    field.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    field.extend(decl::themed_inset_shadow(
        1,
        2,
        NUMBER_INPUT_WELL_LIGHT,
        NUMBER_INPUT_WELL_DARK,
    ));
    // States last: a resting dark twin matches in every state.
    field.extend(decl::focus_ring(LIGHT_ACC, DARK_GLOW));

    let mut arrow = c::COMBOBOX_ARROW_STYLE.to_vec();
    arrow.extend(decl::themed_ink(LIGHT_ICON, DARK_ICON));

    // The list: the widget's structure, then a small leaf, square along the
    // field.
    let mut list = c::COMBOBOX_LIST_BASE.to_vec();
    list.push(P::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(c::MIN_WIDTH))));
    list.extend(decl::themed_border(decl::Edges::ALL, 1, LIGHT_BD2, DARK_BD2));
    list.extend(decl::radius_corners(0, 0, 3, 3));
    list.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
    list.extend(decl::themed_shadow(2, 5, POPOVER_SHADOW_LIGHT, POPOVER_SHADOW_DARK));

    let mut option = c::COMBOBOX_OPTION_BASE.to_vec();
    option.extend(decl::padding(6, 10, 6, 10));
    option.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    // States last.
    option.extend(decl::hover_fill(RADIO_GROUP_HOVER_LIGHT, RADIO_GROUP_HOVER_DARK));
    option.extend(decl::focus_halo_inset_stacked(LIGHT_ACC, DARK_GLOW));

    c::ComboBoxSkin {
        theme: super::UiTheme::Flora,
        wrapper: CssPropertyWithConditionsVec::from_const_slice(c::COMBOBOX_WRAPPER_STYLE),
        field: CssPropertyWithConditionsVec::from_vec(field),
        text: CssPropertyWithConditionsVec::from_const_slice(c::COMBOBOX_TEXT_STYLE),
        arrow: CssPropertyWithConditionsVec::from_vec(arrow),
        option: CssPropertyWithConditionsVec::from_vec(option),
        list: CssPropertyWithConditionsVec::from_vec(list),
    }
}

/// Renders a [`crate::widgets::combobox::ComboBox`] in the flora theme.
#[must_use]
pub fn combobox(c: crate::widgets::combobox::ComboBox) -> Dom {
    c.build(combobox_skin())
}

// ==== chrome (ribbon, quick_access, statusbar) ====
//
// The three Office-style chrome widgets - the ribbon, the quick-access title
// band and the status bar - have one look each that their palette structs
// (`RibbonTheme`, `QuickAccessTheme`, `StatusBarTheme`) describe: the flat
// look, the Office one. Their flora look REPAINTS that chrome and never
// re-measures it: every part keeps the flat part's geometry (the ribbon's
// 68px item row, the 26px tab strip, the 23px status bar were measured for
// those numbers) and takes flora's paint - flora's surfaces, hairlines, ink,
// paper faces and stones, each with its night value.

/// Every property that places or sizes a chrome box: what a look of the
/// ribbon, the quick-access band or the status bar must leave where the flat
/// look measured it.
#[cfg(test)]
pub(crate) const CHROME_METRICS: &[CssPropertyType] = &[
    CssPropertyType::Display,
    CssPropertyType::FlexDirection,
    CssPropertyType::FlexGrow,
    CssPropertyType::FlexShrink,
    CssPropertyType::FlexWrap,
    CssPropertyType::AlignItems,
    CssPropertyType::JustifyContent,
    CssPropertyType::BoxSizing,
    CssPropertyType::Width,
    CssPropertyType::Height,
    CssPropertyType::MinWidth,
    CssPropertyType::PaddingTop,
    CssPropertyType::PaddingRight,
    CssPropertyType::PaddingBottom,
    CssPropertyType::PaddingLeft,
    CssPropertyType::MarginTop,
    CssPropertyType::MarginRight,
    CssPropertyType::MarginBottom,
    CssPropertyType::MarginLeft,
    CssPropertyType::BorderTopWidth,
    CssPropertyType::BorderRightWidth,
    CssPropertyType::BorderBottomWidth,
    CssPropertyType::BorderLeftWidth,
    CssPropertyType::FontSize,
    CssPropertyType::FontFamily,
    CssPropertyType::TextAlign,
    CssPropertyType::Position,
    CssPropertyType::Top,
    CssPropertyType::Left,
    CssPropertyType::ZIndex,
    CssPropertyType::OverflowX,
    CssPropertyType::OverflowY,
    CssPropertyType::WhiteSpace,
];

/// Every [`CHROME_METRICS`] property the `flora` build of a chrome widget
/// resolves differently from its `flat` build (at rest, in the light mode),
/// node by node - empty when flora repainted the chrome without moving it.
#[cfg(test)]
pub(crate) fn chrome_metric_findings(flat: &Dom, flora: &Dom) -> Vec<String> {
    use super::theme_checks as tc;
    let (a, b) = (tc::nodes(flat), tc::nodes(flora));
    if a.len() != b.len() {
        return alloc::vec![alloc::format!(
            "{} nodes flat, {} flora: the two looks must build the same tree",
            a.len(),
            b.len()
        )];
    }
    let mut out = Vec::new();
    for ((path, x), (_, y)) in a.iter().zip(b.iter()) {
        for ty in CHROME_METRICS {
            let (was, is) = (
                tc::resolve(x, *ty, false, None),
                tc::resolve(y, *ty, false, None),
            );
            if was != is {
                out.push(alloc::format!("{path}: {ty:?} is {was:?} flat, {is:?} flora"));
            }
        }
    }
    out
}

/// Whether a declaration of `ty` is PAINT - a fill, an ink, a border colour
/// or a shadow: what a look decides, as opposed to where a box sits and how
/// big it is.
const fn is_chrome_paint(ty: CssPropertyType) -> bool {
    matches!(
        ty,
        CssPropertyType::BackgroundContent
            | CssPropertyType::TextColor
            | CssPropertyType::BorderTopColor
            | CssPropertyType::BorderRightColor
            | CssPropertyType::BorderBottomColor
            | CssPropertyType::BorderLeftColor
            | CssPropertyType::BoxShadowTop
            | CssPropertyType::BoxShadowRight
            | CssPropertyType::BoxShadowBottom
            | CssPropertyType::BoxShadowLeft
    )
}

/// An established (flat) chrome part's GEOMETRY: its declarations without
/// their paint ([`is_chrome_paint`]), without the dark twins and without the
/// state rules - the unconditional (and viewport-conditioned) metrics the
/// widget's layout was measured with, in the part's own order.
fn chrome_geometry(part: &CssPropertyWithConditionsVec) -> Vec<CssPropertyWithConditions> {
    use azul_css::dynamic_selector::DynamicSelector;
    part.as_ref()
        .iter()
        .filter(|p| {
            !is_chrome_paint(p.property.get_type())
                && p.apply_if.as_ref().iter().all(|c| {
                    !matches!(
                        c,
                        DynamicSelector::Theme(_)
                            | DynamicSelector::Mode(_)
                            | DynamicSelector::PseudoState(_)
                    )
                })
        })
        .cloned()
        .collect()
}

/// One chrome part in the flora look. A part the caller set (`Some`) is the
/// caller's and stays as it is, in this look as in the flat one; a part left
/// `None` becomes `established`'s geometry with `paint` laid on it.
fn chrome_part(
    slot: &mut azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec,
    established: &CssPropertyWithConditionsVec,
    paint: impl FnOnce(&mut Vec<CssPropertyWithConditions>),
) {
    if slot.is_some() {
        return;
    }
    let mut part = chrome_geometry(established);
    paint(&mut part);
    *slot = azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::Some(
        CssPropertyWithConditionsVec::from_vec(part),
    );
}

/// A chrome control's lift under the pointer: flora's hover face
/// (`--fl-hT` -> `--fl-hB`) in a `--fl-bd` hairline.
fn chrome_lift(v: &mut Vec<CssPropertyWithConditions>) {
    use super::decl;
    v.extend(decl::hover_layers(
        vec![HOVER_FACE_LIGHT],
        vec![HOVER_FACE_DARK],
    ));
    v.extend(decl::hover_border_color(LIGHT_BD, DARK_BD));
}

/// A toolbar key's states: [`chrome_lift`] under the pointer, the pressed
/// face (`--fl-pT` -> `--fl-pB`) while held, and the focus ring - the accent
/// by day, the stone's glow by night (flora.css `--focus-color`). Appended
/// again after any resting face a part lays over the key (a toggled button,
/// the active view), so that face never shadows them.
fn chrome_key_states(v: &mut Vec<CssPropertyWithConditions>) {
    use super::decl;
    chrome_lift(v);
    v.extend(decl::active_layers(
        vec![PRESSED_FACE_LIGHT],
        vec![PRESSED_FACE_DARK],
    ));
    v.extend(decl::focus_ring(LIGHT_ACC, DARK_GLOW));
}

/// A flora toolbar key (`.nav-links a`, cut for the chrome): bare at rest - a
/// transparent face in a transparent hairline, so the strip shows through -
/// with the house radius, then [`chrome_key_states`]. The key keeps the 1px
/// border its geometry has; the hover and the ring colour it.
fn chrome_key(v: &mut Vec<CssPropertyWithConditions>) {
    use super::decl;
    v.extend(decl::radius(3));
    v.push(CssPropertyWithConditions::simple(decl::fill(ColorU::TRANSPARENT)));
    v.extend(
        super::decl::border_colors(ColorU::TRANSPARENT).map(CssPropertyWithConditions::simple),
    );
    chrome_key_states(v);
}

/// A floating chrome leaf (the gallery's expansion panel, the touch tab
/// picker): the popover's small leaf - `--fl-sur` in a `--fl-bd2` hairline,
/// the house radius, the nearer shadow of `--fl-shadow-2`.
fn chrome_leaf(v: &mut Vec<CssPropertyWithConditions>) {
    use super::decl;
    v.extend(decl::radius(3));
    v.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
    v.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD2));
    v.extend(decl::themed_shadow(
        2,
        5,
        POPOVER_SHADOW_LIGHT,
        POPOVER_SHADOW_DARK,
    ));
}

// ==== ribbon ====
//
// A flora ribbon is flora's toolbar strip (`--fl-strip`, closed along its foot
// by the 2px rule of metal) over a leaf (`--fl-sur`) that holds the groups,
// each ruled off from the next by a `--fl-sep` hairline and captioned in soft
// ink (`--fl-soft1`). Its tab row is Firefox's (Australis) in flora's metal
// ("the Australis tab" below): the unselected tabs are flora's nav tabs
// (`.nav-links a`), soft ink on the strip lifting to the hover face and the
// house ink under the pointer; the selected tab is the sunken accent stone
// (`.nav-links a.active`: `--fl-gem-sunken` under the sunken rig) in a
// surround of the rule's metal that climbs its S-curved sides, written in
// `--fl-on-acc` - its own colour by day and by night. The application button
// is the raised accent stone of a primary command (`.btn-primary`: the
// accent under the depth rig and its streak), cut the same way.
// Every command is a toolbar key: bare paper at rest, the hover face in a
// hairline under the pointer, the pressed face while held, ringed on focus; a
// toggled one stays pushed in, in a `--fl-bd3` hairline. The gallery is a well
// of field paper (`--fl-fld` in `--fl-bd2`, sunk by `--fl-well`) whose picked
// cell is washed in the accent's soft tint (`--fl-soft`; by night the lifted
// face `--fl-hT`, a light tint being a light island there) and rimmed in the
// accent; its expansion panel, a collapsed group's popup and the touch
// chrome's tab picker are popover leaves. Labels are `--fl-ink`, glyphs
// `--fl-icon`, chevrons and captions
// `--fl-soft1` / `--fl-soft2`; the accent as text is `--fl-acc` by day and
// `--fl-glow` by night. The touch chrome's picked group is the sunken stone,
// its own colour in either mode.

/// Flora's ribbon: every part the caller left `None` in `s` filled with
/// flora's paint on the flat part's geometry (see the chrome section above).
#[must_use]
pub(crate) fn ribbon_style(
    mut s: crate::widgets::ribbon::RibbonStyle,
) -> crate::widgets::ribbon::RibbonStyle {
    use super::decl;
    type P = CssPropertyWithConditions;

    let e = s.resolved_container_style();
    chrome_part(&mut s.container_style, &e, |v| {
        v.extend(decl::themed_fill(LIGHT_STRIP, DARK_STRIP));
        v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
        v.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));
    });
    // The tab row is the one part flora RE-MEASURES: Firefox's tab row cut
    // in flora's metal (`australis`, see "the Australis tab"). Its parts
    // still start from the flat part's structure and geometry, so both looks
    // declare the same structure; the selected tab and the application
    // button get their curves in `Ribbon::build_chrome`.
    let e = s.resolved_tab_bar_style();
    chrome_part(&mut s.tab_bar_style, &e, australis::strip);
    let e = s.resolved_app_button_style();
    chrome_part(&mut s.app_button_style, &e, australis::app_button);
    let e = s.resolved_tab_style();
    chrome_part(&mut s.tab_style, &e, |v| australis::tab(v, true));
    let e = s.resolved_tab_active_style();
    chrome_part(&mut s.tab_active_style, &e, |v| australis::selected(v, true));
    // The rule is the strip's own (an inset line under every tab): the
    // filler draws no foot of its own.
    let e = s.resolved_tab_filler_style();
    chrome_part(&mut s.tab_filler_style, &e, |v| {
        v.push(P::simple(CssProperty::const_border_bottom_width(
            LayoutBorderBottomWidth::const_px(0),
        )));
    });
    let e = s.resolved_content_style();
    chrome_part(&mut s.content_style, &e, |v| {
        v.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
    });
    let e = s.resolved_group_style();
    chrome_part(&mut s.group_style, &e, |v| {
        v.extend(decl::themed_border_right_color(LIGHT_SEP, DARK_SEP));
    });
    let e = s.resolved_group_label_style();
    chrome_part(&mut s.group_label_style, &e, |v| {
        v.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    });
    let e = s.resolved_launcher_button_style();
    chrome_part(&mut s.launcher_button_style, &e, chrome_key);
    let e = s.resolved_launcher_icon_style();
    chrome_part(&mut s.launcher_icon_style, &e, |v| {
        v.extend(decl::themed_ink(LIGHT_SOFT2, DARK_SOFT2));
    });
    let e = s.resolved_separator_style();
    chrome_part(&mut s.separator_style, &e, |v| {
        v.extend(decl::themed_fill(LIGHT_SEP, DARK_SEP));
    });
    let e = s.resolved_large_button_style();
    chrome_part(&mut s.large_button_style, &e, chrome_key);
    let e = s.resolved_small_button_style();
    chrome_part(&mut s.small_button_style, &e, chrome_key);
    let (large, small) = (s.resolved_large_icon_style(), s.resolved_small_icon_style());
    for (slot, e) in [
        (&mut s.large_icon_style, large),
        (&mut s.small_icon_style, small),
    ] {
        chrome_part(slot, &e, |v| v.extend(decl::themed_ink(LIGHT_ICON, DARK_ICON)));
    }
    let e = s.resolved_large_label_style();
    chrome_part(&mut s.large_label_style, &e, |v| {
        v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    });
    let e = s.resolved_small_label_style();
    chrome_part(&mut s.small_label_style, &e, |v| {
        v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    });
    let e = s.resolved_arrow_icon_style();
    chrome_part(&mut s.arrow_icon_style, &e, |v| {
        v.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    });
    // APPENDED to a toggled button's key: pushed-in paper, the key's states
    // after it again.
    let e = s.resolved_checked_style();
    chrome_part(&mut s.checked_style, &e, |v| {
        v.extend(decl::themed_layers(
            vec![PRESSED_FACE_LIGHT],
            vec![PRESSED_FACE_DARK],
        ));
        v.extend(decl::themed_border_color(LIGHT_BD3, DARK_BD3));
        chrome_key_states(v);
    });
    let e = s.resolved_gallery_frame_style();
    chrome_part(&mut s.gallery_frame_style, &e, |v| {
        v.extend(decl::radius(3));
        v.extend(decl::themed_fill(LIGHT_FLD, DARK_FLD));
        v.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD2));
        v.extend(decl::themed_inset_shadow(
            1,
            2,
            NUMBER_INPUT_WELL_LIGHT,
            NUMBER_INPUT_WELL_DARK,
        ));
    });
    let e = s.resolved_gallery_cell_style();
    chrome_part(&mut s.gallery_cell_style, &e, |v| {
        v.push(P::simple(decl::fill(ColorU::TRANSPARENT)));
        v.extend(decl::border_colors(ColorU::TRANSPARENT).map(P::simple));
        // Cells are divided by a hairline on their right edge.
        v.extend(decl::themed_border_right_color(LIGHT_SEP, DARK_SEP));
        chrome_lift(v);
    });
    // APPENDED to the picked cell: the accent's soft wash, rimmed in the
    // accent, the lift after it again.
    let e = s.resolved_gallery_cell_selected_style();
    chrome_part(&mut s.gallery_cell_selected_style, &e, |v| {
        v.extend(decl::themed_fill(LIGHT_SOFT, DARK_HT));
        v.extend(decl::themed_border_color(LIGHT_ACC, DARK_GLOW));
        chrome_lift(v);
    });
    let e = s.resolved_gallery_cell_label_style();
    chrome_part(&mut s.gallery_cell_label_style, &e, |v| {
        v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    });
    let e = s.resolved_gallery_spinner_style();
    chrome_part(&mut s.gallery_spinner_style, &e, |v| {
        v.extend(decl::themed_border_left_color(LIGHT_SEP, DARK_SEP));
    });
    let e = s.resolved_gallery_panel_style();
    chrome_part(&mut s.gallery_panel_style, &e, chrome_leaf);
    // A collapsed group's popup is a floating leaf too, the group laid on it
    // as on the band's leaf.
    let e = s.resolved_group_popup_style();
    chrome_part(&mut s.group_popup_style, &e, chrome_leaf);
    // The spinner's buttons have no border to colour: they ring with an
    // inset halo (the frame clips an outer one).
    let e = s.resolved_gallery_spinner_button_style();
    chrome_part(&mut s.gallery_spinner_button_style, &e, |v| {
        v.push(P::simple(decl::fill(ColorU::TRANSPARENT)));
        v.extend(decl::hover_layers(
            vec![HOVER_FACE_LIGHT],
            vec![HOVER_FACE_DARK],
        ));
        v.extend(decl::active_layers(
            vec![PRESSED_FACE_LIGHT],
            vec![PRESSED_FACE_DARK],
        ));
        v.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));
    });
    let e = s.resolved_gallery_spinner_icon_style();
    chrome_part(&mut s.gallery_spinner_icon_style, &e, |v| {
        v.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    });
    let e = s.resolved_mobile_tab_button_style();
    chrome_part(&mut s.mobile_tab_button_style, &e, |v| {
        v.extend(decl::themed_ink(LIGHT_ACC, DARK_GLOW));
        v.extend(decl::themed_fill(LIGHT_STRIP, DARK_STRIP));
        v.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));
    });
    let e = s.resolved_mobile_tab_arrow_style();
    chrome_part(&mut s.mobile_tab_arrow_style, &e, |v| {
        v.extend(decl::themed_ink(LIGHT_ACC, DARK_GLOW));
    });
    let e = s.resolved_mobile_tab_overlay_style();
    chrome_part(&mut s.mobile_tab_overlay_style, &e, chrome_leaf);
    let (overlay_item, group_item) = (
        s.resolved_mobile_tab_overlay_item_style(),
        s.resolved_mobile_group_list_item_style(),
    );
    for (slot, e) in [
        (&mut s.mobile_tab_overlay_item_style, overlay_item),
        (&mut s.mobile_group_list_item_style, group_item),
    ] {
        chrome_part(slot, &e, |v| {
            v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
            v.extend(decl::themed_border_bottom_color(LIGHT_SEP, DARK_SEP));
            v.extend(decl::hover_layers(
                vec![HOVER_FACE_LIGHT],
                vec![HOVER_FACE_DARK],
            ));
        });
    }
    let e = s.resolved_mobile_group_list_style();
    chrome_part(&mut s.mobile_group_list_style, &e, |v| {
        v.extend(decl::themed_fill(LIGHT_STRIP, DARK_STRIP));
        // The divider sits on whichever side faces the content (the
        // handedness decides which edge has a width); both are coloured.
        v.extend(decl::themed_border_left_color(LIGHT_SEP, DARK_SEP));
        v.extend(decl::themed_border_right_color(LIGHT_SEP, DARK_SEP));
    });
    // APPENDED to the picked group: the sunken stone, which stays the stone
    // under the pointer (the lift would un-pick it).
    let e = s.resolved_mobile_group_list_item_selected_style();
    chrome_part(&mut s.mobile_group_list_item_selected_style, &e, |v| {
        v.push(P::simple(decl::layers(selected_stone())));
        v.push(P::simple(decl::ink(LIGHT_ON_ACC)));
        v.extend(decl::hover_layers(selected_stone(), selected_stone()));
    });
    s
}

// ==== statusbar ====
//
// A flora status bar is flora's toolbar strip (`--fl-strip`) closed along its
// TOP by a `--fl-bd` hairline - the menubar's strip, turned over for the foot
// of the window - with the status written in soft ink (`--fl-soft1`) and the
// glyphs in `--fl-icon`. Clickable segments, the view switcher and the zoom
// buttons are toolbar keys; the active view stays pushed in (the pressed face
// in a `--fl-bd3` hairline). The zoom slider runs on a `--fl-bd3` hairline
// rail with a tick at 100% under a thumb of raised paper (`--fl-rT` ->
// `--fl-rB` in `--fl-bd2`); the slider rings with an inset halo on focus.
// Where the Office bar is an accent strip, flora keeps its accent for stones
// and rings: a bar of paper at the foot of the page.

/// Flora's status bar: every part the caller left `None` in `s` filled with
/// flora's paint on the flat part's geometry (see the chrome section above).
#[must_use]
pub(crate) fn statusbar_style(
    mut s: crate::widgets::statusbar::StatusBarStyle,
) -> crate::widgets::statusbar::StatusBarStyle {
    use super::decl;
    type P = CssPropertyWithConditions;

    let e = s.resolved_bar_style();
    chrome_part(&mut s.bar_style, &e, |v| {
        v.extend(decl::themed_fill(LIGHT_STRIP, DARK_STRIP));
        v.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
        // The hairline along the top: an inset line, so it costs no height.
        v.extend(decl::themed_inset_shadow(1, 0, LIGHT_BD, DARK_BD));
    });
    let keys = (
        s.resolved_segment_style(),
        s.resolved_view_button_style(),
        s.resolved_zoom_button_style(),
    );
    for (slot, e) in [
        (&mut s.segment_style, keys.0),
        (&mut s.view_button_style, keys.1),
        (&mut s.zoom_button_style, keys.2),
    ] {
        chrome_part(slot, &e, chrome_key);
    }
    let glyphs = (
        s.resolved_segment_icon_style(),
        s.resolved_view_icon_style(),
        s.resolved_zoom_icon_style(),
    );
    for (slot, e) in [
        (&mut s.segment_icon_style, glyphs.0),
        (&mut s.view_icon_style, glyphs.1),
        (&mut s.zoom_icon_style, glyphs.2),
    ] {
        chrome_part(slot, &e, |v| v.extend(decl::themed_ink(LIGHT_ICON, DARK_ICON)));
    }
    let e = s.resolved_segment_label_style();
    chrome_part(&mut s.segment_label_style, &e, |v| {
        v.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    });
    // APPENDED to the active view's key: pushed-in paper, the key's states
    // after it again.
    let e = s.resolved_view_button_active_style();
    chrome_part(&mut s.view_button_active_style, &e, |v| {
        v.extend(decl::themed_layers(
            vec![PRESSED_FACE_LIGHT],
            vec![PRESSED_FACE_DARK],
        ));
        v.extend(decl::themed_border_color(LIGHT_BD3, DARK_BD3));
        chrome_key_states(v);
    });
    let (rail, tick) = (s.resolved_zoom_rail_style(), s.resolved_zoom_tick_style());
    for (slot, e) in [(&mut s.zoom_rail_style, rail), (&mut s.zoom_tick_style, tick)] {
        chrome_part(slot, &e, |v| v.extend(decl::themed_fill(LIGHT_BD3, DARK_BD3)));
    }
    // The slider's hit area stays transparent (the rail is drawn by its host)
    // and rings with an inset halo: it has no border to colour.
    let e = s.resolved_slider_track_style();
    chrome_part(&mut s.slider_track_style, &e, |v| {
        v.push(P::simple(decl::fill(ColorU::TRANSPARENT)));
        v.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));
    });
    let e = s.resolved_slider_thumb_style();
    chrome_part(&mut s.slider_thumb_style, &e, |v| {
        v.extend(decl::radius(2));
        v.extend(decl::themed_layers(
            vec![RAISED_FACE_LIGHT],
            vec![RAISED_FACE_DARK],
        ));
        v.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD2));
    });
    let e = s.resolved_zoom_label_style();
    chrome_part(&mut s.zoom_label_style, &e, |v| {
        v.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
        chrome_key(v);
    });
    // The sync indicator's glyph when the sync failed: the clay stone, lifted
    // to its glow at night (the alert's danger thread).
    let e = s.resolved_sync_icon_error_style();
    chrome_part(&mut s.sync_icon_error_style, &e, |v| {
        v.extend(decl::themed_ink(STONE_CLAY.stone, STONE_CLAY.glow));
    });
    s
}

// ==== quick_access ====
//
// A flora title band is the recessed band behind the leaves (`--fl-desk`):
// window chrome, one step deeper than the ribbon's toolbar strip under it,
// with the window's title in `--fl-intro` and the glyphs in `--fl-icon` (the
// customize chevron in `--fl-soft2`). Every action and window control is a
// toolbar key. The close key warms to clay under the pointer - flora's red
// alternate, the stone its danger commands are cut from ([`STONE_CLAY`]) - as
// a soft wash by day and its deep by night, rimmed in its glow, and deepens
// while held (the glow by day, the stone by night): the caption red of a
// desktop titlebar, said in flora's palette. The glyph keeps its ink on both
// washes.

/// Flora's title band: every part the caller left `None` in `s` filled with
/// flora's paint on the flat part's geometry (see the chrome section above).
#[must_use]
pub(crate) fn quick_access_style(
    mut s: crate::widgets::quick_access::QuickAccessStyle,
) -> crate::widgets::quick_access::QuickAccessStyle {
    use super::decl;

    let e = s.resolved_bar_style();
    chrome_part(&mut s.bar_style, &e, |v| {
        v.extend(decl::themed_fill(LIGHT_DESK, DARK_DESK));
        v.extend(decl::themed_ink(LIGHT_INTRO, DARK_INTRO));
    });
    let (action, window) = (
        s.resolved_action_button_style(),
        s.resolved_window_button_style(),
    );
    for (slot, e) in [
        (&mut s.action_button_style, action),
        (&mut s.window_button_style, window),
    ] {
        chrome_part(slot, &e, chrome_key);
    }
    let (action, window) = (s.resolved_action_icon_style(), s.resolved_window_icon_style());
    for (slot, e) in [(&mut s.action_icon_style, action), (&mut s.window_icon_style, window)] {
        chrome_part(slot, &e, |v| v.extend(decl::themed_ink(LIGHT_ICON, DARK_ICON)));
    }
    let e = s.resolved_menu_arrow_style();
    chrome_part(&mut s.menu_arrow_style, &e, |v| {
        v.extend(decl::themed_ink(LIGHT_SOFT2, DARK_SOFT2));
    });
    let e = s.resolved_title_style();
    chrome_part(&mut s.title_style, &e, |v| {
        v.extend(decl::themed_ink(LIGHT_INTRO, DARK_INTRO));
    });
    // APPENDED to the close key's window key: clay under the pointer and while
    // held, then the ring again so the clay rim never hides it.
    let e = s.resolved_close_button_style();
    chrome_part(&mut s.close_button_style, &e, |v| {
        v.extend(decl::hover_fill(STONE_CLAY.soft, STONE_CLAY.deep));
        v.extend(decl::hover_border_color(STONE_CLAY.glow, STONE_CLAY.glow));
        v.extend(decl::active_fill(STONE_CLAY.glow, STONE_CLAY.stone));
        v.extend(decl::focus_ring(LIGHT_ACC, DARK_GLOW));
    });
    s
}

// ==== tree_view ====
//
// A flora tree is a sheet of FIELD PAPER laid in the leaf: `--fl-fld` (by
// night the night field) inside a `--fl-bd2` hairline with the house radius,
// written in flora ink, its disclosure chevrons in `--fl-icon`. A row washes
// to `--fl-hov` under the pointer (the radio row's wash) and sinks to the
// pressed face while held. The selected row is the sunken accent stone
// (`selected_stone`, flora's `.nav-links a.active`), its label and chevron in
// `--fl-on-acc` - its own colour by day and by night. An open parent's
// children hang from a `--fl-sep` guide rule under its chevron (the indent
// stays 16px a level). The rows have no border to colour, so each is ringed
// on focus with an inset ring: the accent by day, the glow by night and on
// the stone. The 16px icon column is the flat tree's, so leaves line up with
// their parents' labels in both looks.

/// Flora's tree-view look.
#[must_use]
pub(crate) fn tree_view_look() -> crate::widgets::tree_view::TreeViewLook {
    use super::decl;
    use crate::widgets::tree_view as t;
    type P = CssPropertyWithConditions;
    let part = CssPropertyWithConditionsVec::from_vec;

    // Every part is the widget's base (`tree_view::TREE_CONTAINER_BASE`,
    // `ROW_BASE`, `CHILDREN_BASE`, `ICON_BASE`, `LABEL_BASE`: its
    // structure), then flora's skin.
    let mut container = t::TREE_CONTAINER_BASE.to_vec();
    container.extend([
        decl::font_size(13),
        P::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ]);
    container.extend(decl::padding(3, 3, 3, 3));
    container.extend(decl::themed_border(decl::Edges::ALL, 1, LIGHT_BD2, DARK_BD2));
    container.extend(decl::radius(3));
    container.extend(decl::themed_fill(LIGHT_FLD, DARK_FLD));
    container.extend(decl::themed_ink(LIGHT_INK, DARK_INK));

    // A row's box, selected or not.
    let row_box = || {
        let mut v = t::ROW_BASE.to_vec();
        v.extend(decl::padding(3, 6, 3, 6));
        v.extend(decl::radius(3));
        v
    };
    let mut row = row_box();
    // States last: a resting dark twin matches in every state.
    row.extend(decl::hover_fill(RADIO_GROUP_HOVER_LIGHT, RADIO_GROUP_HOVER_DARK));
    row.extend(decl::active_layers(
        vec![PRESSED_FACE_LIGHT],
        vec![PRESSED_FACE_DARK],
    ));
    row.extend(decl::focus_halo_inset_stacked(LIGHT_ACC, DARK_GLOW));

    let mut row_selected = row_box();
    row_selected.push(P::simple(decl::layers(selected_stone())));
    row_selected.push(P::simple(decl::ink(LIGHT_ON_ACC)));
    row_selected.extend(decl::focus_halo_inset_stacked(LIGHT_GLOW, DARK_GLOW));

    // The guide rule sits under the parent's chevron (6px row padding + half
    // the 16px icon column); margin + rule + padding keep the 16px indent.
    let mut children = t::CHILDREN_BASE.to_vec();
    children.extend([
        P::simple(CssProperty::const_margin_left(LayoutMarginLeft::const_px(13))),
        P::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(2))),
    ]);
    let guide = decl::Edges {
        top: false,
        right: false,
        bottom: false,
        left: true,
    };
    children.extend(decl::themed_border(guide, 1, LIGHT_SEP, DARK_SEP));

    // The chevron: the flat tree's 16px column, flora's icon ink - or, on the
    // stone, the stone's ink.
    let icon = |ink: Vec<P>| {
        let mut v = t::ICON_BASE.to_vec();
        v.push(decl::font_size(16));
        v.extend(ink);
        v
    };

    let label = |ink: Vec<P>| {
        let mut v = t::LABEL_BASE.to_vec();
        v.push(P::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(4))));
        v.extend(ink);
        v
    };

    t::TreeViewLook {
        container: part(container),
        row: part(row),
        row_selected: part(row_selected),
        children: part(children),
        icon: part(icon(decl::themed_ink(LIGHT_ICON, DARK_ICON).to_vec())),
        icon_selected: part(icon(vec![P::simple(decl::ink(LIGHT_ON_ACC))])),
        leaf_spacer: CssPropertyWithConditionsVec::from_const_slice(t::LEAF_SPACER_STYLE),
        label: part(label(decl::themed_ink(LIGHT_INK, DARK_INK).to_vec())),
        label_selected: part(label(vec![P::simple(decl::ink(LIGHT_ON_ACC))])),
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

// ==== tabs ====
//
// A flora tab bar is flora's navigation strip (`.navbar`, `.nav-links a`):
// raised CHROME (`--fl-rT` over `--fl-rB` - chrome is a smooth face, no
// grain) closed along its foot by a 2px rule of metal, its tabs the Australis
// tab row ("the Australis tab" below) the ribbon's tab row is too. The
// unselected tabs stand ON the rule, written in `--fl-soft1`; under the
// pointer they lift to the hover face in a `--fl-bd` hairline and the label
// darkens to `--fl-ink`; pressed, they sink to the pressed face in a
// `--fl-bd3` hairline. The SELECTED tab is the sunken accent stone stood
// upright, in `--fl-on-acc`, set in the rule's metal that climbs its S-curved
// sides and runs along its head: it stands over the rule and breaks it, which
// is what "selected" means in a tab bar. Every label is the UI hand's
// capitals, on one line whichever tab is selected. The tabs start a curve's
// width in and the rule runs on to both ends of the strip. Every tab rings on
// focus with an inset ring: the accent by day, the glow by night and on the
// stone.
//
// The panel the selected tab opens onto is a LEAF (`--fl-sur`) in a `--fl-bd`
// hairline on three sides with the house radius at its foot, open at the top
// where the strip's rule closes it; padded, it breathes 10px. The tab is open
// at its foot onto it - no line between them. flora.css also bleeds the
// stone down into the page (`.fl-tab-foot`, a blurred accent): that belongs
// with the band, the deep page of the stone's own colour the website's tab
// opens onto; on paper it would be a blue smear, so the leaf takes none.

// -- the metal: flora.css's ribbon ---------------------------------------
//
// "The rule that closes the strip and the three borders of the selected tab
// are ONE piece of metal." The rule is a specular that travels along the
// strip (`--fl-rule-metal-bg`), dim brass at both ends; the tab's metal is a
// rolled bead lit at its head (`--fl-rolled-tab`) that arrives at the turn
// colour, `--fl-metal-turn`, where it meets the rule; and beside each foot a
// run-out eases the rule into that same colour (`.fl-tab-runout-l/-r`), so
// rule, S and head meet as one value wherever the tab stands on the strip.
// The metal shows through transparent borders, under the faces that cover
// the padding box (`background: <face> padding-box, <metal> border-box`,
// [`over_metal`]): the strip's 2px foot, the selected tab's 2px head. Metal
// is its own colour by day and by night (flora.css does not redefine it for
// the dark theme).

/// `--fl-metal-turn` (#C6B279): the value flora's ribbon has where the rule
/// that closes a tab strip turns and climbs the selected tab - the one metal
/// the rule and the tab's surround are cut from.
pub const TAB_METAL: ColorU = ColorU::rgb(0xC6, 0xB2, 0x79);
/// The rolled metal at its lit head (`--fl-rolled-tab`'s #FFFDF3).
pub const METAL_LIT: ColorU = ColorU::rgb(0xFF, 0xFD, 0xF3);
/// The rolled metal a fifth of the way down (`--fl-rolled-tab`'s #E4DCB8).
pub const METAL_ROLL: ColorU = ColorU::rgb(0xE4, 0xDC, 0xB8);
/// The rule's glint, at a third and two thirds of the strip
/// (`--fl-rule-metal-bg`'s `rgba(239, 235, 211, 1)`).
pub const METAL_GLINT: ColorU = ColorU::rgb(239, 235, 211);
/// The rule's darker roll between its glints (`rgba(162, 146, 95, .9)`).
pub const METAL_DIM: ColorU = ColorU::rgb(162, 146, 95);
/// The rule's brass at both ends of the strip (`rgba(122, 112, 82, .5)`).
pub const METAL_SHADE: ColorU = ColorU::rgb(122, 112, 82);

/// `--fl-cove-o`: `--fl-cove` (12px) + `--fl-metal` (2px), how far above the
/// foot the rolled bead has arrived at the turn colour.
const TAB_COVE_O: isize = 14;
/// `--fl-runout`: how far along the rule each run-out eases it into the turn.
pub const TAB_RUNOUT: isize = 34;

/// `c` at alpha `a` (an `rgba(..)` alpha as the CSS parser rounds it).
const fn at_alpha(c: ColorU, a: u8) -> ColorU {
    ColorU::new(c.r, c.g, c.b, a)
}

const RULE_METAL_STOPS: &[NormalizedLinearColorStop] = &[
    stop(0, at_alpha(METAL_SHADE, 128)),
    stop(16, at_alpha(TAB_METAL, 242)),
    stop(32, METAL_GLINT),
    stop(50, at_alpha(METAL_DIM, 230)),
    stop(68, METAL_GLINT),
    stop(84, at_alpha(TAB_METAL, 242)),
    stop(100, at_alpha(METAL_SHADE, 128)),
];

/// `--fl-rule-metal-bg`, the rule that closes a tab strip: brass at half
/// alpha at both ends, the glint at a third and at two thirds, a darker roll
/// between - laid along the strip's whole width.
///
/// `linear-gradient(90deg, rgba(122, 112, 82, 0.5) 0%, rgba(198, 178, 121,
/// 0.95) 16%, rgba(239, 235, 211, 1) 32%, rgba(162, 146, 95, 0.9) 50%,
/// rgba(239, 235, 211, 1) 68%, rgba(198, 178, 121, 0.95) 84%, rgba(122, 112,
/// 82, 0.5) 100%)`.
pub const RULE_METAL: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: deg(90),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(RULE_METAL_STOPS),
    });

const ROLLED_TAB_STOPS: &[NormalizedLinearColorStop] = &[
    stop(0, METAL_LIT),
    stop(22, METAL_ROLL),
    // `calc(100% - var(--fl-cove-o))`: the tangent where the bead meets the
    // turn colour, measured up from the foot.
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(TAB_METAL),
        offset_px: FloatValue::const_new(-TAB_COVE_O),
    },
    stop(100, TAB_METAL),
];

/// `--fl-rolled-tab`, the bead the selected tab's metal is cut from: lit at
/// its head, falling away down its sides, and arriving at `--fl-metal-turn`
/// the cove's height above the foot, which it holds - so the S of each curve
/// and the rule it joins are the same value where they touch.
///
/// `linear-gradient(180deg, #FFFDF3 0%, #E4DCB8 22%, var(--fl-metal-turn)
/// calc(100% - var(--fl-cove-o)), var(--fl-metal-turn) 100%)`.
pub const ROLLED_TAB: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: deg(180),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(ROLLED_TAB_STOPS),
    });

/// `--fl-metal-turn-0` to `--fl-metal-turn`: spelled out rather than from
/// `transparent`, so a renderer that interpolates straight RGBA cannot fringe
/// the run-out.
const RUNOUT_STOPS: &[NormalizedLinearColorStop] =
    &[stop(0, at_alpha(TAB_METAL, 0)), stop(100, TAB_METAL)];

/// `.fl-tab-runout-l`: the rule easing into the turn colour beside the left
/// foot. `linear-gradient(90deg, var(--fl-metal-turn-0) 0%,
/// var(--fl-metal-turn) 100%)`.
pub const RUNOUT_LEFT: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: deg(90),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(RUNOUT_STOPS),
    });

/// `.fl-tab-runout-r`, the same beside the right foot (`270deg`).
pub const RUNOUT_RIGHT: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: deg(270),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(RUNOUT_STOPS),
    });

/// `background: <face> padding-box, <metal> border-box`: `face` (painted
/// first to last) over `metal`, the layers in paint order - the metal first.
/// Pair it with [`over_metal_clips`]: the metal then shows only through a
/// transparent border.
fn over_metal(
    metal: StyleBackgroundContent,
    face: Vec<StyleBackgroundContent>,
) -> Vec<StyleBackgroundContent> {
    let mut layers = Vec::with_capacity(face.len() + 1);
    layers.push(metal);
    layers.extend(face);
    layers
}

/// The clip boxes of an [`over_metal`] background whose face has
/// `face_layers` layers: the metal's border box, then a padding box for each
/// layer of the face.
fn over_metal_clips(face_layers: usize) -> CssPropertyWithConditions {
    let mut boxes = alloc::vec![StyleBackgroundClip::BorderBox];
    boxes.resize(face_layers + 1, StyleBackgroundClip::PaddingBox);
    CssPropertyWithConditions::simple(CssProperty::background_clip(
        StyleBackgroundClipVec::from_vec(boxes),
    ))
}

/// A strip's foot: `--fl-metal` of transparent border along its bottom, the
/// rule seen through it (the strip's background is [`over_metal`] on
/// [`RULE_METAL`]). It costs no height the strip did not have: the tabs stand
/// on it, the selected one reaches down over it.
fn tab_rule_foot(v: &mut Vec<CssPropertyWithConditions>) {
    v.extend(super::decl::border_bottom(TAB_GAUGE));
    v.push(CssPropertyWithConditions::simple(
        CssProperty::const_border_bottom_color(StyleBorderBottomColor {
            inner: ColorU::TRANSPARENT,
        }),
    ));
}

/// `.nav-links a:active`'s well: `inset 0 1px 3px rgba(48, 45, 38, 0.2)`.
const TAB_PRESS_SHADOW: ColorU = ColorU::new(48, 45, 38, 51);

/// `--fl-dur-ray`: light falling into a sunken stone - the shafts brighten
/// and drift across it over 1.8s, ease-in-out (flora.css's MOTION: "a shaft
/// moving in a quarter second reads as a flicker, not as light").
pub const FL_DUR_RAY_MS: u32 = 1800;

/// The selected stone's light: its background (where the streak lives)
/// follows the pointer at the shafts' pace.
fn stone_light_fade() -> CssPropertyWithConditions {
    use azul_css::props::{
        basic::{
            animation::{
                AnimationIterationCount, AnimationTiming, StyleAnimation, StyleAnimationVec,
            },
            time::CssDuration,
        },
        property::StyleAnimationVecValue,
    };
    CssPropertyWithConditions::simple(CssProperty::Animation(StyleAnimationVecValue::Exact(
        StyleAnimationVec::from_vec(alloc::vec![StyleAnimation {
            name: AzString::from_const_str("background"),
            duration: CssDuration::from_millis(FL_DUR_RAY_MS),
            delay: CssDuration::from_millis(0),
            iterations: AnimationIterationCount::Count(1),
            timing: AnimationTiming::EaseInOut,
            clip: true,
        }]),
    )))
}

// -- the Australis tab ---------------------------------------------------
//
// Flora's tab row is Firefox's (Australis, 2014-2017) cut in flora's metal.
// The strip has air above the tabs and is closed along its foot by the 2px
// rule (`--fl-metal`): its transparent bottom border with `--fl-rule-metal-bg`
// under the strip's face, so the rule runs under every tab, from one end of
// the strip to the other, and the selected tab can cover it. The selected tab
// is the sunken accent stone stood upright - `--fl-gem-sunken` from the deep
// at its top to the accent at its foot, under the sunken rig - set in the
// rolled metal: the rule leaves the strip's foot, eased into the turn colour
// by the run-out, climbs the tab's left side as Firefox's S-curve
// (`tabs::australis_curves`: the band of metal the S is, filled through the
// engine's own path clip), runs along its head - the stone on the padding box
// over `--fl-rolled-tab` on the border box, through a transparent 2px border
// - and comes down the right S into the rule again: one ribbon of metal with
// the stone set in it, lit along the head and turning to the rule's colour at
// the feet, open at the foot onto the page below. Under the pointer the light
// moves across the stone (`.nav-links a.active:hover::before`): the streak
// brightens at the shafts' pace, `--fl-dur-ray`. The unselected tabs sit
// ABOVE the rule on the strip's own paper, soft ink, lifting to the hover
// face in a hairline and the house ink under the pointer over `--fl-dur` on
// `--fl-ease` - the declaration that wins in flora.css (`.nav-links a`'s
// RAISED <-> SUNKEN rule, after the `0.2s ease` it overrides) - a press in
// `--fl-dur-fast`. Every label is set in the UI hand's capitals
// (`--font-caps`, EB Garamond), tracked out.

/// A flora tab row's selected tab, in px (its border box, foot to top edge).
pub const TAB_HEIGHT: isize = 28;
/// `--fl-metal`: the gauge of the strip's rule and of the selected tab's
/// surround, in px.
pub const TAB_GAUGE: isize = 2;
/// The air above the tabs in a flora tab row, in px.
pub const TAB_AIR: isize = 4;
/// A tab's padding beside its label: the selected tab's curves hang into
/// it, so it is the curve's width - a foot never reaches a neighbour's label.
pub const TAB_SIDE: isize = crate::widgets::tabs::CURVE_WIDTH as isize;

/// A tab's label in flora: the UI hand's capitals, tracked out - flora.css's
/// `.nav-links a` (17px all-small-caps at 700, 0.07em) set as true capitals of
/// the same height.
fn tab_caps(v: &mut Vec<CssPropertyWithConditions>) {
    use super::decl;
    v.push(CssPropertyWithConditions::simple(CssProperty::const_font_family(FONT_CAPS)));
    v.push(decl::font_size(12));
    v.push(decl::semibold());
    v.push(decl::letter_spacing_em(0.08));
    v.push(CssPropertyWithConditions::simple(CssProperty::TextTransform(
        StyleTextTransformValue::Exact(StyleTextTransform::Uppercase),
    )));
}

/// A tab's box in a flora tab row: `TAB_SIDE` beside the label and no
/// margins but the selected tab's foot. The SELECTED tab is the full
/// `TAB_HEIGHT`, a transparent `--fl-metal` head the rolled metal shows
/// through and no other border (`position: relative` carries its curves,
/// `z-index` keeps a neighbour's hover face off them); it reaches a gauge
/// below the strip's content, over the rule along the strip's foot, so it
/// breaks the rule and is open at its foot. An unselected tab is one gauge
/// shorter and stands on the rule, in a 1px hairline on three sides
/// (`.nav-links a { border: 1px solid transparent; border-bottom: none }`,
/// coloured under the pointer); its label sits on the selected one's line (a
/// gauge of border and padding over it, where the selected tab has its head,
/// and a gauge under the selected label). The label's line is the content
/// box's height, so it is centred in either box model; `border_box` says
/// which one the tab's base declares (the ribbon's tabs size their border
/// box, the tab bar's their content).
fn australis_tab_box(v: &mut Vec<CssPropertyWithConditions>, selected: bool, border_box: bool) {
    use super::decl;
    type P = CssPropertyWithConditions;
    let g = TAB_GAUGE;
    let line = TAB_HEIGHT - 2 * g;
    // (border-box height, foot margin, padding top / bottom, head, sides)
    let (border_height, foot, pad_top, pad_bottom, head, side) = if selected {
        (TAB_HEIGHT, -g, 0, g, g, 0)
    } else {
        (TAB_HEIGHT - g, 0, g - 1, 0, 1, 1)
    };
    let height = if border_box { border_height } else { line };
    v.push(P::simple(CssProperty::const_height(LayoutHeight::const_px(height))));
    v.push(P::simple(CssProperty::const_line_height(StyleLineHeight::Length(
        PixelValue::const_px(line),
    ))));
    v.extend(decl::margin(0, 0, foot, 0));
    v.extend(decl::padding(pad_top, TAB_SIDE - side, pad_bottom, TAB_SIDE - side));
    v.extend([
        P::simple(CssProperty::const_border_top_width(LayoutBorderTopWidth::const_px(head))),
        P::simple(CssProperty::const_border_right_width(LayoutBorderRightWidth::const_px(side))),
        P::simple(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(0))),
        P::simple(CssProperty::const_border_left_width(LayoutBorderLeftWidth::const_px(side))),
        P::simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        })),
    ]);
    if selected {
        v.push(decl::position(LayoutPosition::Relative));
        v.push(P::simple(CssProperty::const_z_index(LayoutZIndex::Integer(1))));
    } else {
        v.extend([
            P::simple(CssProperty::const_border_right_style(StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            })),
            P::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            })),
        ]);
    }
}

const TAB_FACE_STOPS: &[NormalizedLinearColorStop] = &[stop(0, LIGHT_DEEP), stop(96, LIGHT_ACC)];

/// The selected tab's stone: `--fl-gem-sunken` (`linear-gradient(175deg,
/// var(--fl-deep) 0%, var(--fl-acc) 96%)`) stood upright. The tab is three
/// boxes of three widths - the left curve, the middle, the right curve - and
/// only a vertical gradient is the same value at the same height in all of
/// them.
pub const TAB_FACE: StyleBackgroundContent =
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: deg(180),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(TAB_FACE_STOPS),
    });

/// The left curve's share of the sunken rig's left-edge shadow: the darkest
/// value `SUNKEN_RIG_LEFT` starts the middle on, held across the whole curve,
/// so the face has no seam where the curve meets the middle.
const TAB_LEFT_SHADE: StyleBackgroundContent =
    StyleBackgroundContent::Color(ColorU::new(8, 6, 2, 77));

/// [`TAB_LEFT_SHADE`] for a RAISED stone: the darkest value
/// `STONE_RIG_LEFT` starts its face on.
const STONE_LEFT_SHADE: StyleBackgroundContent =
    StyleBackgroundContent::Color(ColorU::new(12, 10, 4, 56));

/// The selected tab's face (its middle): the upright stone under the sunken
/// rig, with the light's `streak` raked across it.
#[must_use]
pub fn australis_face(streak: StyleBackgroundContent) -> Vec<StyleBackgroundContent> {
    vec![
        TAB_FACE,
        SUNKEN_RIG_TOP,
        SUNKEN_RIG_LEFT,
        SUNKEN_RIG_BOTTOM,
        streak,
    ]
}

/// The face inside one of a cut stone's curves: the stone and its rig's
/// vertical layers (the same at every height as the middle's), the left
/// curve in the shadow of the left wall; the streak stays on the middle. The
/// selected tab is the sunken stone ([`australis_face`]); the application
/// button is `raised` ([`stone_face`] in the accent).
fn australis_curve_face(raised: bool, left: bool) -> Vec<StyleBackgroundContent> {
    let mut v = if raised {
        vec![StyleBackgroundContent::Color(LIGHT_ACC), STONE_RIG_TOP]
    } else {
        vec![TAB_FACE, SUNKEN_RIG_TOP, SUNKEN_RIG_BOTTOM]
    };
    if left {
        v.push(if raised { STONE_LEFT_SHADE } else { TAB_LEFT_SHADE });
    }
    v
}

/// The curves of a stone flora cuts as an Australis tab - the selected tab,
/// or the `raised` application button: the stone inside each S, the S cut
/// from the rolled metal its head is cut from (the band of metal at the
/// gauge, [`ROLLED_TAB`] laid over the curve's whole height - the head's
/// own - and seen through the band), and the run-outs that ease the strip's
/// rule into the turn colour beside each foot. The stone inside the S stands
/// where the middle's stone stands, below the 2px head (the middle's padding
/// box), so the three boxes meet without a seam. A stone and metal are their
/// own colours in both modes.
#[must_use]
pub(crate) fn tab_curves(raised: bool) -> crate::widgets::tabs::TabCurveLook {
    use super::decl;
    type P = CssPropertyWithConditions;
    let part = CssPropertyWithConditionsVec::from_vec;
    let fill = |left: bool| {
        part(vec![
            P::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(TAB_GAUGE))),
            P::simple(CssProperty::background_clip(StyleBackgroundClipVec::from_vec(
                alloc::vec![StyleBackgroundClip::ContentBox],
            ))),
            P::simple(decl::layers(australis_curve_face(raised, left))),
        ])
    };
    let paint = |layer: StyleBackgroundContent| part(vec![P::simple(decl::layers(vec![layer]))]);
    crate::widgets::tabs::TabCurveLook {
        height: TAB_HEIGHT as f32,
        gauge: TAB_GAUGE as f32,
        left_fill: fill(true),
        right_fill: fill(false),
        metal: paint(ROLLED_TAB),
        runout: TAB_RUNOUT as f32,
        runout_left: paint(RUNOUT_LEFT),
        runout_right: paint(RUNOUT_RIGHT),
    }
}

/// The parts of a flora tab row - the strip, an unselected tab, the selected
/// tab - each on `established` (the part's structure and the geometry it
/// was laid out with, whose own paint is dropped; see `chrome_part`) with
/// the Australis metrics and paint laid on it. Shared by the ribbon's tab row
/// and the tab bar.
pub(crate) mod australis {
    use super::*;

    /// The strip: air above the tabs and room before the first one for its
    /// curve, the strip's paper on its padding box, and the rule - the
    /// website's travelling metal, along the whole strip - through its
    /// transparent foot.
    pub(crate) fn strip(v: &mut Vec<CssPropertyWithConditions>) {
        use super::super::decl;
        v.extend(decl::padding(TAB_AIR, 0, 0, TAB_SIDE));
        v.push(CssPropertyWithConditions::simple(CssProperty::const_height(
            LayoutHeight::const_px(TAB_AIR + TAB_HEIGHT),
        )));
        tab_rule_foot(v);
        v.extend(decl::themed_layers(
            over_metal(RULE_METAL, vec![StyleBackgroundContent::Color(LIGHT_STRIP)]),
            over_metal(RULE_METAL, vec![StyleBackgroundContent::Color(DARK_STRIP)]),
        ));
        v.push(over_metal_clips(1));
    }

    /// An unselected tab: bare at rest on the strip, soft ink, in a
    /// transparent hairline; under the pointer the hover face in a `--fl-bd`
    /// hairline and the house ink, held the pressed face in a `--fl-bd3`
    /// hairline over its well (`.nav-links a:hover` / `:active`) - every
    /// change over `--fl-dur` on `--fl-ease`, a press in `--fl-dur-fast`.
    pub(crate) fn tab(v: &mut Vec<CssPropertyWithConditions>, border_box: bool) {
        use super::super::decl;
        type P = CssPropertyWithConditions;
        australis_tab_box(v, false, border_box);
        tab_caps(v);
        v.extend(decl::radius_corners(4, 4, 0, 0));
        v.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
        v.push(P::simple(decl::fill(ColorU::TRANSPARENT)));
        v.extend(decl::border_colors(ColorU::TRANSPARENT).map(P::simple));
        // States last: a resting dark twin matches in every state.
        v.extend(decl::hover_layers(
            vec![HOVER_FACE_LIGHT],
            vec![HOVER_FACE_DARK],
        ));
        v.extend(decl::hover_ink(LIGHT_INK, DARK_INK));
        v.extend(decl::hover_border_color(LIGHT_BD, DARK_BD));
        v.extend(decl::active_layers(
            vec![PRESSED_FACE_LIGHT],
            vec![PRESSED_FACE_DARK],
        ));
        for (light, dark) in decl::border_colors(LIGHT_BD3)
            .into_iter()
            .zip(decl::border_colors(DARK_BD3))
        {
            v.extend(P::themed_on_active(light, dark));
        }
        let well = || shadow_in(ShadowSlot::Top, 1, 3, 0, TAB_PRESS_SHADOW, true);
        v.extend(P::themed_on_active(well(), well()));
        v.extend(flora_fade(FLORA_FACE, FL_DUR_MS));
    }

    /// The selected tab: the upright stone in `--fl-on-acc` on its padding
    /// box, over the rolled metal on its border box - seen through the
    /// transparent head (its sides are its curves, `tab_curves`) - and the
    /// light moving across the stone under the pointer at the shafts' pace.
    /// Its own colour in both modes.
    pub(crate) fn selected(v: &mut Vec<CssPropertyWithConditions>, border_box: bool) {
        use super::super::decl;
        type P = CssPropertyWithConditions;
        australis_tab_box(v, true, border_box);
        tab_caps(v);
        v.extend(decl::radius(0));
        let face = australis_face(STONE_STREAK);
        let face_layers = face.len();
        v.push(P::simple(decl::layers(over_metal(ROLLED_TAB, face))));
        v.push(over_metal_clips(face_layers));
        v.push(P::simple(decl::ink(LIGHT_ON_ACC)));
        v.push(P::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
            inner: ColorU::TRANSPARENT,
        })));
        let lit = over_metal(ROLLED_TAB, australis_face(STONE_STREAK_HOVER));
        v.extend(decl::hover_layers(lit.clone(), lit));
        v.push(stone_light_fade());
    }

    /// The application button: the RAISED accent stone of a primary command
    /// (`.btn-primary`, its depth rig and streak) cut as the selected tab is,
    /// the Azlin design system's gem in its gold setting with the rolled
    /// metal under it, written in `--fl-on-acc`. The streak brightens under
    /// the pointer over `--fl-dur-slow` (`.btn-primary::before`); held, the
    /// stone sinks to its pressed face in `--fl-dur-fast`.
    pub(crate) fn app_button(v: &mut Vec<CssPropertyWithConditions>) {
        use super::super::decl;
        type P = CssPropertyWithConditions;
        australis_tab_box(v, true, true);
        tab_caps(v);
        v.extend(decl::radius(0));
        let face = stone_face(LIGHT_ACC, STONE_STREAK);
        let face_layers = face.len();
        v.push(P::simple(decl::layers(over_metal(ROLLED_TAB, face))));
        v.push(over_metal_clips(face_layers));
        v.push(P::simple(decl::ink(LIGHT_ON_ACC)));
        v.push(P::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
            inner: ColorU::TRANSPARENT,
        })));
        // A stone is its own colour in both modes, so its states repeat for
        // the night: every state rule keeps its twin. Each state keeps the
        // face's layer count, so the one clip list fits them all.
        let lit = over_metal(ROLLED_TAB, stone_face(LIGHT_ACC, STONE_STREAK_HOVER));
        v.extend(decl::hover_layers(lit.clone(), lit));
        let held = over_metal(ROLLED_TAB, sunken_stone_face(LIGHT_DEEP));
        v.extend(decl::active_layers(held.clone(), held));
        v.extend(flora_fade(&["background"], FL_DUR_SLOW_MS));
    }
}

/// Flora's tab-bar look: the Australis tab row ("the Australis tab") on the
/// navigation strip's raised chrome, closed by the rule. The selected tab's
/// curves and run-outs are hung on it by `TabHeader::dom` (`tab_curves`).
#[must_use]
pub(crate) fn tab_header_look() -> crate::widgets::tabs::TabHeaderLook {
    use super::decl;
    type P = CssPropertyWithConditions;
    let part = CssPropertyWithConditionsVec::from_vec;

    // Every part is the widget's base (`tabs::HEADER_BASE`, `AFTER_BASE`,
    // `TAB_BASE`: its structure), then flora's skin.
    let mut header = crate::widgets::tabs::HEADER_BASE.to_vec();
    header.extend([
        // Flora's own layout: the tabs stand on the strip's foot. Flat's
        // native tabs hang from the top of the bar (its default).
        P::simple(CssProperty::const_align_items(LayoutAlignItems::End)),
        P::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
        decl::font_size(13),
        P::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(TAB_AIR))),
    ]);
    // `.navbar` and `.navbar::after`: the raised chrome on the padding box,
    // the rule through the transparent foot under it.
    tab_rule_foot(&mut header);
    header.extend(decl::themed_layers(
        over_metal(RULE_METAL, vec![RAISED_FACE_LIGHT]),
        over_metal(RULE_METAL, vec![RAISED_FACE_DARK]),
    ));
    header.push(over_metal_clips(1));

    // The tabs start a curve's width in, where the first tab's foot lands
    // when it is selected: a fixed spacer (flat's grows).
    let before = vec![
        P::simple(CssProperty::const_width(LayoutWidth::const_px(TAB_SIDE))),
        P::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    ];
    let after = crate::widgets::tabs::AFTER_BASE.to_vec();

    // A tab: the widget's tab base (its content box sized, the pointer
    // among it), the centred label, then the Australis tab, ringed on focus
    // with an inset ring - the accent by day, the glow by night and on the
    // stone.
    let tab_box = || {
        let mut v = crate::widgets::tabs::TAB_BASE.to_vec();
        v.push(P::simple(CssProperty::const_text_align(StyleTextAlign::Center)));
        v
    };
    let mut tab = tab_box();
    australis::tab(&mut tab, false);
    tab.extend(decl::focus_halo_inset_stacked(LIGHT_ACC, DARK_GLOW));

    let mut active = tab_box();
    australis::selected(&mut active, false);
    active.extend(decl::focus_halo_inset_stacked(LIGHT_GLOW, DARK_GLOW));

    // No seams: a tab next to the selected one is a tab like any other.
    let tab = part(tab);
    crate::widgets::tabs::TabHeaderLook {
        header: part(header),
        before: part(before),
        after: part(after),
        active: part(active),
        before_active: tab.clone(),
        after_active: tab.clone(),
        inactive: tab,
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// Flora's tab-panel look.
#[must_use]
pub(crate) fn tab_content_look() -> crate::widgets::tabs::TabContentLook {
    use super::decl;
    // The widget's panel base (`tabs::PANEL_BASE`), then the leaf.
    let leaf = || {
        let mut v = crate::widgets::tabs::PANEL_BASE.to_vec();
        v.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
        v
    };
    let mut padded = leaf();
    padded.extend(decl::padding(10, 10, 10, 10));
    let open_top = decl::Edges {
        top: false,
        right: true,
        bottom: true,
        left: true,
    };
    padded.extend(decl::themed_border(open_top, 1, LIGHT_BD, DARK_BD));
    padded.extend(decl::radius_corners(0, 0, 3, 3));

    crate::widgets::tabs::TabContentLook {
        padded: CssPropertyWithConditionsVec::from_vec(padded),
        unpadded: CssPropertyWithConditionsVec::from_vec(leaf()),
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

// ==== titlebar ====
//
// A flora titlebar is flora's WINDOW CHROME (`.azul-titlebar`, the frame
// the docs draw around every screenshot): a band of `--fl-ct` over `--fl-cb`
// - warm slate by day, near-black by night; dark in both, so its ink is the
// chrome's light ink (#F2F2F2) in both - closed by a `--fl-bd5` line where
// the bar has one. The title steps back to a dimmer ink when the window
// loses focus (`:backdrop`), as every desktop's does. The window controls
// inherit the chrome's ink; minimize and maximize wash in it under the
// pointer and sink to the band's foot when pressed, and close turns to the
// clay stone (flora's red alternate, the mock's close light) with its glyph
// in `--fl-on-acc`. The traffic-light gems the mock draws are the OS's to
// draw on macOS, so the controls stay the platform's glyphs.
//
// Flora changes the PAINT only: the band's height, the centred
// `system:title:bold` title at the platform's size, the padding that clears
// the OS's controls, whether there is a line and how wide, the drag region -
// all of it is the platform's, built by `Titlebar::container_style_painted`
// / `title_style_painted` exactly as for the native look. The desktop's
// colour fields (`title_color`, `background_color`, the hover colours) are
// the native look's; flora draws its own chrome.

/// `--fl-ct`: the top of flora's window chrome by day (#837F74).
pub const LIGHT_CT: ColorU = ColorU::rgb(0x83, 0x7F, 0x74);
/// `--fl-cb`: the foot of flora's window chrome by day (#67635A).
pub const LIGHT_CB: ColorU = ColorU::rgb(0x67, 0x63, 0x5A);
/// `--fl-ct` by night (#383838).
pub const DARK_CT: ColorU = ColorU::rgb(0x38, 0x38, 0x38);
/// `--fl-cb` by night (#262626).
pub const DARK_CB: ColorU = ColorU::rgb(0x26, 0x26, 0x26);
/// The ink written on flora's window chrome, by day and by night
/// (`.azul-titlebar`'s #F2F2F2): the band is dark in both modes.
pub const CHROME_INK: ColorU = ColorU::rgb(0xF2, 0xF2, 0xF2);
/// The chrome's ink while the window is unfocused (`:backdrop`): the title
/// steps back, as every desktop's does.
pub const CHROME_INK_DIM: ColorU = ColorU::rgb(0xB9, 0xB5, 0xAB);
/// A window control under the pointer: the chrome's ink as a 15% wash.
pub const CHROME_HOVER: ColorU = ColorU::new(0xF2, 0xF2, 0xF2, 38);

/// Flora's titlebar look: flora's window chrome on the bar's own metrics.
#[must_use]
pub(crate) fn titlebar_look(
    bar: &crate::widgets::titlebar::Titlebar,
    show_buttons: bool,
) -> crate::widgets::titlebar::TitlebarLook {
    use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};

    use super::decl;
    type P = CssPropertyWithConditions;

    let band = decl::themed_layers(
        vec![decl::face(LIGHT_CT, LIGHT_CB)],
        vec![decl::face(DARK_CT, DARK_CB)],
    )
    .to_vec();
    let line = P::themed(
        CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: LIGHT_BD5 }),
        CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: DARK_BD5 }),
    )
    .to_vec();
    // The bar's own ink: the window controls' glyphs inherit it.
    let ink = vec![P::simple(decl::ink(CHROME_INK))];
    // Resting first, `:backdrop` after it: last match wins.
    let title_ink = vec![
        P::simple(decl::ink(CHROME_INK)),
        P::with_single_condition(
            decl::ink(CHROME_INK_DIM),
            &[DynamicSelector::PseudoState(PseudoStateType::Backdrop)],
        ),
    ];

    let mut button = Vec::new();
    button.extend(decl::hover_fill(CHROME_HOVER, CHROME_HOVER));
    button.extend(decl::active_fill(LIGHT_CB, DARK_CB));

    let mut close = Vec::new();
    close.extend(decl::hover_fill(STONE_CLAY.stone, STONE_CLAY.stone));
    close.extend(decl::hover_ink(LIGHT_ON_ACC, LIGHT_ON_ACC));
    close.extend(decl::active_fill(STONE_CLAY.deep, STONE_CLAY.deep));

    crate::widgets::titlebar::TitlebarLook {
        container: bar.container_style_painted(show_buttons, band, line, ink),
        title: bar.title_style_painted(show_buttons, title_ink),
        button: CssPropertyWithConditionsVec::from_vec(button),
        close: CssPropertyWithConditionsVec::from_vec(close),
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

// ==== combobox (active option) ====
//
// The option the arrow keys made ACTIVE (the field keeps focus; WAI-ARIA
// combobox) wears the `--fl-hov` wash its rows take under the pointer, so the
// keyboard's "you are here" reads like the mouse's. Light, then dark.

/// The fill of a flora combobox's active option, `[light, dark]`.
pub(crate) const COMBOBOX_ACTIVE_OPTION: [ColorU; 2] =
    [RADIO_GROUP_HOVER_LIGHT, RADIO_GROUP_HOVER_DARK];

// ==== night focus ring (V1) ====
//
// flora.css lifts the focus ring to the stone's own highlight at night
// (`--focus-color: var(--fl-glow)`, "so it survives on a dark ground"): the
// accent stone #2F4A85 stands only 1.8:1 off `--fl-sur` #232323, and WCAG 2.2
// (1.4.11, non-text contrast) asks 3:1 of a focus indicator against what it
// sits on. Every flora ring drawn at night - the shared `FOCUS_BORDER_*_DARK`
// of buttons and text fields included - must clear that on every night
// surface flora paints.

#[cfg(test)]
mod night_focus_ring_tests {
    use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType, ThemeCondition};

    use super::*;
    use crate::widgets::themes::{theme_checks as tc, UiTheme};

    /// Every surface a flora control and its ring sit on at night: the page,
    /// the leaf, the desk, strips and tracks, a control's raised, hover and
    /// pressed faces, the field papers and the disabled face.
    const NIGHT_SURFACES: [(&str, ColorU); 14] = [
        ("--fl-pg", DARK_PG),
        ("--fl-sur", DARK_SUR),
        ("--fl-desk", DARK_DESK),
        ("--fl-strip", DARK_STRIP),
        ("--fl-track", DARK_TRACK),
        ("--fl-rT", DARK_RT),
        ("--fl-rB", DARK_RB),
        ("--fl-hT", DARK_HT),
        ("--fl-hB", DARK_HB),
        ("--fl-pT", DARK_PT),
        ("--fl-pB", DARK_PB),
        ("--fl-fld", DARK_FLD),
        ("--fl-fld2", DARK_FLD2),
        ("--fl-disbg", DARK_DISBG),
    ];

    fn border_colour(p: &CssPropertyWithConditions) -> ColorU {
        tc::border_color(&p.property).expect("a focus ring edge is a border colour")
    }

    fn assert_stands_off_the_night(what: &str, ring: ColorU) {
        for (name, surface) in NIGHT_SURFACES {
            let ratio = ring.contrast_ratio(&surface);
            assert!(
                ratio >= 3.0,
                "{what}: the night focus ring {ring:?} stands {ratio:.2}:1 off {name} \
                 {surface:?}; a focus indicator needs 3:1"
            );
        }
    }

    #[test]
    fn the_shared_night_focus_ring_stands_three_to_one_off_every_night_surface() {
        for (edge, ring) in [
            ("TOP", FOCUS_BORDER_TOP_DARK),
            ("RIGHT", FOCUS_BORDER_RIGHT_DARK),
            ("BOTTOM", FOCUS_BORDER_BOTTOM_DARK),
            ("LEFT", FOCUS_BORDER_LEFT_DARK),
        ] {
            let conds = ring.apply_if.as_ref();
            assert!(
                conds.contains(&DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark))
                    && conds.contains(&DynamicSelector::PseudoState(PseudoStateType::Focus)),
                "premise: the {edge} edge is the dark :focus twin, {conds:?}"
            );
            assert_stands_off_the_night(
                &alloc::format!("FOCUS_BORDER_{edge}_DARK"),
                border_colour(&ring),
            );
        }
    }

    /// The token the ring takes is flora.css's night `--focus-color`, the
    /// one every newer flora widget already rings in.
    #[test]
    fn the_night_focus_ring_is_flora_s_glow_token() {
        assert_stands_off_the_night("--fl-glow", DARK_GLOW);
        for ring in [
            FOCUS_BORDER_TOP_DARK,
            FOCUS_BORDER_RIGHT_DARK,
            FOCUS_BORDER_BOTTOM_DARK,
            FOCUS_BORDER_LEFT_DARK,
        ] {
            assert_eq!(border_colour(&ring), DARK_GLOW);
        }
    }

    /// Through a real widget: a focused flora button at night.
    #[test]
    fn a_focused_flora_button_rings_in_the_glow_at_night() {
        let dom = crate::widgets::button::Button::create(AzString::from_const_str("OK"))
            .with_theme(UiTheme::Flora)
            .dom();
        assert_eq!(
            tc::focus_ring_color(&dom, false),
            Some(LIGHT_ACC),
            "by day: the stone"
        );
        let night = tc::focus_ring_color(&dom, true).expect("a flora button rings at night");
        assert_stands_off_the_night("a flora button", night);
    }
}

// ==== backstage ====
//
// A flora backstage is the flyout navigation drawer of flora.css
// (`.mobile-menu`, the panel the socket opens) laid on the Office column.
// The column is the drawer: a leaf of paper (`--fl-sur`) with a `--fl-bd2`
// hairline along the edge that faces the page. The pane beside it is the page
// the drawer lies on (`--fl-pg`), written in the house ink (`--color-text`,
// `--fl-ink`), so whatever the application puts there reads in either mode.
//
// The nav items are the drawer's links (`.mobile-menu a`): keys inset from
// the drawer's edges (`padding: 14px 12px`, `gap: 2px`, a 1px ring each), bare
// at rest in the house ink, lifting to the hover face in a `--fl-bd` hairline
// under the pointer and pressing to the pressed face - the toolbar key of the
// chrome section ([`chrome_key`]). The selected item (`.mobile-menu
// a.active`) is the sunken stone (`--fl-gem-sunken` under the sunken rig) in
// `--fl-on-acc`, edged in brass - the leaf border, drawn in the one colour
// the ribbon's rule is cut from (`--fl-metal-turn`, [`TAB_METAL`]) where the
// CSS rolls two radial passes - and it stays the stone under the pointer and
// while held.
//
// The back button closes the drawer the ribbon's FILE stone opened: the same
// raised accent stone, seated in a brass collar (the socket's
// `.fl-orb-collar`), its arrow in `--fl-on-acc`. Its streak brightens under
// the pointer, it sinks while held, and it rings on focus like the socket
// (`.fl-orb:focus-visible`: the focus colour, following the circle). A stone
// is its own colour in both modes, so its states repeat for the night.
//
// Metrics: the column, the back button's circle and the fonts are the flat
// look's. Only the keys are re-measured, to the drawer's inset: 12px in from
// either side, 2px apart. The label stays where the flat look writes it (the
// 24px indent) and the items keep the flat pitch (38px), so a theme switch
// moves no word. The drawer's own shadow (`-12px 0 42px`) is not drawn: the
// page beside the column paints over an outset shadow of its sibling.

/// How far a drawer key sits in from the drawer's edges
/// (`.mobile-menu { padding: 14px 12px }`).
const BACKSTAGE_KEY_INSET: isize = 12;
/// Half the drawer's gap between two keys (`.mobile-menu { gap: 2px }`),
/// above and below each key.
const BACKSTAGE_KEY_GAP_HALF: isize = 1;
/// A drawer key's height: the flat item's 38px pitch less the gap, so the
/// items keep their pitch.
const BACKSTAGE_KEY_H: isize = 36;
/// A drawer key's side padding: the flat item's 24px label indent less the
/// inset and the key's 1px ring, so the label keeps its place.
const BACKSTAGE_KEY_PAD: isize = 11;

/// Flora's backstage: every part the caller left `None` in `s` filled with
/// flora's paint on the flat part's geometry (see the chrome section above);
/// the nav items re-measured to the drawer's keys.
#[must_use]
pub(crate) fn backstage_style(
    mut s: crate::widgets::backstage::BackstageStyle,
) -> crate::widgets::backstage::BackstageStyle {
    use super::decl;
    type P = CssPropertyWithConditions;

    // The page: the root and the two boxes that hold the caller's strip and
    // pane.
    let e = s.resolved_root_style();
    chrome_part(&mut s.root_style, &e, |v| {
        v.extend(decl::themed_fill(LIGHT_PG, DARK_PG));
        v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    });
    let (right, content) = (s.resolved_right_style(), s.resolved_content_style());
    for (slot, e) in [(&mut s.right_style, right), (&mut s.content_style, content)] {
        chrome_part(slot, &e, |v| v.extend(decl::themed_fill(LIGHT_PG, DARK_PG)));
    }

    // The drawer: a leaf, its hairline on the edge that faces the page.
    let e = s.resolved_nav_style();
    chrome_part(&mut s.nav_style, &e, |v| {
        v.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
        v.extend(decl::themed_border(
            decl::Edges {
                top: false,
                right: true,
                bottom: false,
                left: false,
            },
            1,
            LIGHT_BD2,
            DARK_BD2,
        ));
    });

    // The back button: the accent stone in a brass collar.
    let e = s.resolved_back_button_style();
    chrome_part(&mut s.back_button_style, &e, |v| {
        v.push(P::simple(decl::layers(stone_face(LIGHT_ACC, STONE_STREAK))));
        v.extend(decl::border_colors(TAB_METAL).map(P::simple));
        let lit = stone_face(LIGHT_ACC, STONE_STREAK_HOVER);
        v.extend(decl::hover_layers(lit.clone(), lit));
        let held = sunken_stone_face(LIGHT_DEEP);
        v.extend(decl::active_layers(held.clone(), held));
        v.extend(decl::focus_halo_stacked(LIGHT_ACC, DARK_GLOW));
    });
    let e = s.resolved_back_icon_style();
    chrome_part(&mut s.back_icon_style, &e, |v| {
        v.push(P::simple(decl::ink(LIGHT_ON_ACC)));
    });

    // A nav item: the drawer's key. The flat row's height and indent give
    // way to the key's box (see the consts above); everything else is the
    // flat row's.
    let e = s.resolved_nav_item_style();
    chrome_part(&mut s.nav_item_style, &e, |v| {
        v.retain(|p| {
            !matches!(
                p.property.get_type(),
                CssPropertyType::Height | CssPropertyType::PaddingLeft
            )
        });
        v.push(P::simple(CssProperty::const_height(LayoutHeight::const_px(
            BACKSTAGE_KEY_H,
        ))));
        v.extend(decl::margin(
            BACKSTAGE_KEY_GAP_HALF,
            BACKSTAGE_KEY_INSET,
            BACKSTAGE_KEY_GAP_HALF,
            BACKSTAGE_KEY_INSET,
        ));
        v.extend(decl::padding(0, BACKSTAGE_KEY_PAD, 0, BACKSTAGE_KEY_PAD));
        v.extend(decl::border(1));
        v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
        chrome_key(v);
    });
    // APPENDED to the selected item: the sunken stone in a brass edge, which
    // stays the stone under the pointer and while held (the lift would
    // un-pick it). Its states come after its resting face, so none of the
    // key's is shadowed; the ring is the stone's glow, by day and by night.
    let e = s.resolved_nav_item_active_style();
    chrome_part(&mut s.nav_item_active_style, &e, |v| {
        v.push(P::simple(decl::layers(selected_stone())));
        v.push(P::simple(decl::ink(LIGHT_ON_ACC)));
        v.extend(decl::border_colors(TAB_METAL).map(P::simple));
        v.extend(decl::hover_layers(selected_stone(), selected_stone()));
        v.extend(decl::hover_border_color(TAB_METAL, TAB_METAL));
        v.extend(decl::active_layers(selected_stone(), selected_stone()));
        v.extend(decl::focus_ring(LIGHT_GLOW, DARK_GLOW));
    });
    // APPENDED to the item after a gap: the flat gap, nothing to paint.
    let e = s.resolved_nav_item_gap_style();
    chrome_part(&mut s.nav_item_gap_style, &e, |_| {});
    s
}

// ==== accordion (groups) ====
//
// A flora group is flora.css's `.fl-label` over a hairline: the title and its
// count set bold, tracked out, in --fl-soft1 - "the small-caps label that
// sits over every group" - then a --fl-bd rule to the end of the row and the
// chevron, which points right while the group is closed and turns down when
// it opens. No leaf around the groups: they sit on the page. A header takes
// the radio row's wash under the pointer and flora's accent halo, inside, on
// focus. At night every ink and rule takes its night value.

/// The flora groups accordion: labelled groups on the page.
#[must_use]
pub fn accordion_groups(a: crate::widgets::accordion::Accordion) -> Dom {
    use super::decl;
    use crate::widgets::accordion::AccordionLook;

    let mut container = vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            13,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ];
    container.extend(decl::themed_ink(LIGHT_INK, DARK_INK));

    let section = decl::margin(0, 0, 8, 0).to_vec();

    let mut header = decl::padding(4, 6, 4, 6).to_vec();
    header.extend(decl::radius(3));
    header.extend(decl::hover_fill(
        RADIO_GROUP_HOVER_LIGHT,
        RADIO_GROUP_HOVER_DARK,
    ));
    header.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));

    // `.fl-label`: bold, tracked out, --fl-soft1; it hugs its text so the
    // rule takes the rest of the row.
    let mut title = vec![
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
        decl::bold(),
        decl::letter_spacing_em(0.06),
    ];
    title.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    let mut rule = decl::margin(0, 0, 0, 10).to_vec();
    rule.extend(decl::border_bottom(1));
    rule.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));

    crate::widgets::accordion::build_groups(
        a,
        &AccordionLook {
            container,
            section,
            header,
            title,
            chevron: crate::widgets::accordion::chevron_box(16),
            chevron_icon: "chevron_right",
            chevron_turn_deg: 90,
            marker: Some(super::style_kit::FLORA_CLASS),
        },
        &rule,
    )
}

// ==== tile ====
//
// A flora tile: the icon in brass ink beside the title, the capacity bar in
// the accent on the house track (an ember red past a tenth free), the "x
// free of y" line in --fl-soft1. A tile washes to the radio row's hover
// under the pointer; the selected one sits on the track colour; focus is
// flora's accent halo, inside. At night every ink and wash takes its night
// value.

/// The ember the bar turns when the volume is nearly full.
const TILE_ALARM: ColorU = ColorU::new(180, 60, 44, 255);

/// One solid layer, as a bar's fill or track.
fn solid_layer(color: ColorU) -> StyleBackgroundContentVec {
    StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(color)])
}

/// Flora's tile look.
#[must_use]
pub(crate) fn tile_look() -> crate::widgets::tile::TileLook {
    use super::decl;
    let mut tile = vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            13,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ];
    tile.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    tile.extend(decl::padding(8, 8, 8, 8));
    tile.extend(decl::radius(3));
    tile.extend(decl::hover_fill(
        RADIO_GROUP_HOVER_LIGHT,
        RADIO_GROUP_HOVER_DARK,
    ));
    tile.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));

    let tile_selected = decl::themed_fill(LIGHT_TRACK, DARK_TRACK).to_vec();

    let mut icon = vec![CssPropertyWithConditions::simple(CssProperty::const_font_size(
        StyleFontSize::const_px(44),
    ))];
    icon.extend(decl::margin(0, 10, 0, 0));
    icon.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    let mut detail = vec![CssPropertyWithConditions::simple(CssProperty::const_font_size(
        StyleFontSize::const_px(12),
    ))];
    detail.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    crate::widgets::tile::TileLook {
        tile,
        tile_selected,
        icon,
        title: vec![decl::semibold()],
        detail,
        bar: decl::margin(4, 0, 4, 0).to_vec(),
        bar_height: 10,
        bar_track: solid_layer(LIGHT_TRACK),
        bar_fill: solid_layer(LIGHT_ACC),
        bar_fill_alarm: solid_layer(TILE_ALARM),
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora tile.
#[must_use]
pub fn tile(t: crate::widgets::tile::Tile) -> Dom {
    crate::widgets::tile::build(t, &tile_look())
}

// ==== details_pane ====
//
// A flora details pane is a strip of the leaf under a --fl-bd hairline: the
// big icon in brass, the item's name semibold, its kind as `.fl-label`
// (bold, tracked, --fl-soft1), the keys in --fl-soft1 set right. At night
// the night leaf and inks.

/// Flora's details-pane look.
#[must_use]
pub(crate) fn details_pane_look() -> crate::widgets::details_pane::DetailsPaneLook {
    use super::decl;
    let mut pane = vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            13,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ];
    pane.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    pane.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
    pane.extend(decl::padding(10, 16, 10, 16));
    pane.extend([
        CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
            LayoutBorderTopWidth::const_px(1),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_style(
            StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            },
        )),
    ]);
    pane.extend(decl::themed_border_top_color(LIGHT_BD, DARK_BD));

    let mut icon = vec![CssPropertyWithConditions::simple(CssProperty::const_font_size(
        StyleFontSize::const_px(56),
    ))];
    icon.extend(decl::margin(0, 14, 0, 0));
    icon.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    let heading = decl::margin(0, 28, 0, 0).to_vec();
    let title = vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            14,
        ))),
        decl::semibold(),
    ];
    let mut subtitle = vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            11,
        ))),
        decl::bold(),
        decl::letter_spacing_em(0.08),
    ];
    subtitle.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    let mut key = vec![CssPropertyWithConditions::simple(CssProperty::const_width(
        LayoutWidth::const_px(110),
    ))];
    key.extend(decl::margin(0, 8, 0, 0));
    key.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    crate::widgets::details_pane::DetailsPaneLook {
        pane,
        icon,
        heading,
        title,
        subtitle,
        properties: Vec::new(),
        row: decl::margin(1, 0, 1, 0).to_vec(),
        key,
        value: Vec::new(),
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora details pane.
#[must_use]
pub fn details_pane(p: crate::widgets::details_pane::DetailsPane) -> Dom {
    crate::widgets::details_pane::build(p, &details_pane_look())
}

// ==== address_bar ====
//
// A flora address bar is a toolbar strip (--fl-strip) over a --fl-bd
// hairline, set in the house serif (flora's chrome is Garamond; the path
// keeps its own case, so no capitals). Back and Forward are round stones of
// raised paper (--fl-rT / --fl-rB) in a --fl-bd2 ring that lift to the hover
// face and press in; Recent and Up are quiet arrows on the strip. The
// breadcrumb box is field paper (--fl-fld) inside a --fl-bd2 rule at the
// house radius, ringed by the accent under the pointer, the location's icon
// in brass ink; its segments are the house ink on the bare paper until the
// pointer lifts them to the hover face, each folder with a quiet --fl-soft2
// chevron; Refresh closes the box past a hairline. Focus is flora's accent
// halo. At night the night strip, field, faces and glow.

/// A button of the flora address bar at rest: bare paper in a transparent
/// 1 px rim (so the hover's rim moves nothing), `width` (or its content's)
/// by `height`; the hover face in a --fl-bd2 rim under the pointer, the
/// pressed face in --fl-bd3 pressed, the accent halo on focus.
fn address_bar_control(width: Option<isize>, height: isize) -> Vec<CssPropertyWithConditions> {
    use super::decl;
    let mut v = Vec::new();
    if let Some(width) = width {
        v.push(CssPropertyWithConditions::simple(CssProperty::const_width(
            LayoutWidth::const_px(width),
        )));
    }
    v.push(CssPropertyWithConditions::simple(CssProperty::const_height(
        LayoutHeight::const_px(height),
    )));
    v.extend(decl::border(1));
    v.extend(decl::border_colors(ColorU::TRANSPARENT).map(CssPropertyWithConditions::simple));
    v.extend(decl::radius(3));
    v.push(CssPropertyWithConditions::simple(decl::fill(ColorU::TRANSPARENT)));
    v.extend(decl::hover_layers(vec![HOVER_FACE_LIGHT], vec![HOVER_FACE_DARK]));
    v.extend(decl::hover_border_color(LIGHT_BD2, DARK_BD2));
    v.extend(decl::active_layers(vec![PRESSED_FACE_LIGHT], vec![PRESSED_FACE_DARK]));
    v.extend(decl::active_border_color(LIGHT_BD3, DARK_BD3));
    v.extend(decl::focus_halo(LIGHT_ACC, DARK_GLOW));
    v
}

/// Flora's address-bar look.
#[must_use]
pub(crate) fn address_bar_look() -> crate::widgets::address_bar::AddressBarLook {
    use super::decl;
    type C = CssPropertyWithConditions;
    let mut bar = vec![
        C::simple(CssProperty::const_font_size(StyleFontSize::const_px(14))),
        C::simple(CssProperty::const_font_family(SERIF_FAMILY)),
    ];
    bar.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    bar.extend(decl::themed_fill(LIGHT_STRIP, DARK_STRIP));
    bar.extend(decl::padding(5, 8, 5, 8));
    bar.extend(decl::border_bottom(1));
    bar.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));

    // Back and Forward: round stones of raised paper in the --fl-bd2 ring.
    let mut round = vec![
        C::simple(CssProperty::const_width(LayoutWidth::const_px(26))),
        C::simple(CssProperty::const_height(LayoutHeight::const_px(26))),
    ];
    round.extend(decl::border(1));
    round.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD2));
    round.extend(decl::radius(13));
    round.extend(decl::themed_layers(vec![RAISED_FACE_LIGHT], vec![RAISED_FACE_DARK]));
    round.extend(decl::hover_layers(vec![HOVER_FACE_LIGHT], vec![HOVER_FACE_DARK]));
    round.extend(decl::hover_border_color(LIGHT_BD3, DARK_BD3));
    round.extend(decl::active_layers(vec![PRESSED_FACE_LIGHT], vec![PRESSED_FACE_DARK]));
    round.extend(decl::focus_halo(LIGHT_ACC, DARK_GLOW));

    let mut arrow_icon = vec![C::simple(CssProperty::const_font_size(StyleFontSize::const_px(16)))];
    arrow_icon.extend(decl::themed_ink(LIGHT_ICON, DARK_ICON));

    // The breadcrumb box: field paper in the --fl-bd2 rule, the accent ring
    // under the pointer.
    let mut field_box = vec![C::simple(CssProperty::const_height(LayoutHeight::const_px(26)))];
    field_box.extend(decl::padding(0, 0, 0, 2));
    field_box.extend(decl::margin(0, 8, 0, 6));
    field_box.extend(decl::border(1));
    field_box.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD2));
    field_box.extend(decl::radius(3));
    field_box.extend(decl::themed_fill(LIGHT_FLD, DARK_FLD));
    field_box.extend(decl::hover_border_color(LIGHT_ACC, DARK_GLOW));

    // The location's icon in brass ink, flora's quiet accent.
    let mut icon = vec![C::simple(CssProperty::const_font_size(StyleFontSize::const_px(16)))];
    icon.extend(decl::padding(0, 4, 0, 4));
    icon.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    let mut crumb = address_bar_control(None, 22);
    crumb.extend(decl::padding(0, 6, 0, 6));

    let mut label = vec![
        C::simple(CssProperty::const_font_size(StyleFontSize::const_px(14))),
        C::simple(CssProperty::const_font_family(SERIF_FAMILY)),
    ];
    label.extend(decl::themed_ink(LIGHT_INK, DARK_INK));

    let mut chevron_icon =
        vec![C::simple(CssProperty::const_font_size(StyleFontSize::const_px(14)))];
    chevron_icon.extend(decl::themed_ink(LIGHT_SOFT2, DARK_SOFT2));

    // Refresh closes the box past a --fl-bd hairline; it lifts like the
    // segments and keeps its rule.
    let mut refresh = vec![
        C::simple(CssProperty::const_width(LayoutWidth::const_px(26))),
        C::simple(CssProperty::const_height(LayoutHeight::const_px(24))),
    ];
    refresh.extend(decl::border_left(1));
    refresh.extend(decl::themed_border_left_color(LIGHT_BD, DARK_BD));
    refresh.push(C::simple(decl::fill(ColorU::TRANSPARENT)));
    refresh.extend(decl::hover_layers(vec![HOVER_FACE_LIGHT], vec![HOVER_FACE_DARK]));
    refresh.extend(decl::active_layers(vec![PRESSED_FACE_LIGHT], vec![PRESSED_FACE_DARK]));
    refresh.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));

    let mut refresh_icon =
        vec![C::simple(CssProperty::const_font_size(StyleFontSize::const_px(15)))];
    refresh_icon.extend(decl::themed_ink(LIGHT_ICON, DARK_ICON));

    crate::widgets::address_bar::AddressBarLook {
        theme: super::UiTheme::Flora,
        bar,
        nav: decl::margin(0, 2, 0, 0).to_vec(),
        round,
        arrow: address_bar_control(Some(24), 26),
        recent: address_bar_control(Some(16), 26),
        arrow_icon,
        field_box,
        // The path field draws its own frame: the box keeps its place.
        field_box_editing: decl::margin(0, 8, 0, 6).to_vec(),
        icon,
        field: decl::padding(0, 1, 0, 1).to_vec(),
        edit: Vec::new(),
        crumb,
        current: Vec::new(),
        label,
        chevron: address_bar_control(Some(16), 22),
        chevron_icon,
        overflow: address_bar_control(Some(22), 22),
        refresh,
        refresh_icon,
        search: vec![C::simple(CssProperty::const_width(LayoutWidth::const_px(220)))],
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora address bar.
#[must_use]
pub fn address_bar(b: crate::widgets::address_bar::AddressBar) -> Dom {
    crate::widgets::address_bar::build(b, &address_bar_look())
}

// ==== ribbon_file_menu ====
//
// A flora File menu is a leaf of paper (--fl-sur) laid over the ribbon in a
// --fl-bd2 hairline at the house radius with flora's warm cast shadow. Its
// left column is the strip's paper (--fl-strip): commands set in flora's
// capitals beside their brass-ink icons, lifting to the hover face in a
// --fl-bd2 rim under the pointer and pressing in; quiet --fl-soft2 chevrons
// on the commands with sub-commands, --fl-sep rules between the groups. The
// side column's title is a section title in capitals over a rule; the places
// are Garamond, their numbers and pins in brass; a sub-command's label sits
// over its description in the softer ink. Focus is flora's accent halo. At
// night every paper, rule and ink takes its night value.

/// Flora's File-menu look.
#[must_use]
pub(crate) fn ribbon_file_menu_look() -> crate::widgets::ribbon_file_menu::RibbonFileMenuLook {
    use super::decl;
    type C = CssPropertyWithConditions;
    let px = |n: isize| C::simple(CssProperty::const_font_size(StyleFontSize::const_px(n)));
    // A row of the menu at rest: bare paper in a transparent rim (so the
    // hover's rim moves nothing); the hover face in a --fl-bd2 rim under the
    // pointer, the pressed face in --fl-bd3, the accent halo inside it on
    // focus (the columns clip at their edges).
    let row = |height: isize| {
        let mut v = vec![C::simple(CssProperty::const_height(LayoutHeight::const_px(height)))];
        v.extend(decl::border(1));
        v.extend(decl::border_colors(ColorU::TRANSPARENT).map(C::simple));
        v.extend(decl::radius(3));
        v.push(C::simple(decl::fill(ColorU::TRANSPARENT)));
        v.extend(decl::hover_layers(vec![HOVER_FACE_LIGHT], vec![HOVER_FACE_DARK]));
        v.extend(decl::hover_border_color(LIGHT_BD2, DARK_BD2));
        v.extend(decl::active_layers(vec![PRESSED_FACE_LIGHT], vec![PRESSED_FACE_DARK]));
        v.extend(decl::active_border_color(LIGHT_BD3, DARK_BD3));
        v.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));
        v
    };

    let mut menu = vec![px(14), C::simple(CssProperty::const_font_family(SERIF_FAMILY))];
    menu.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    menu.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
    menu.extend(decl::border(1));
    menu.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD2));
    menu.extend(decl::radius(4));
    menu.extend(decl::themed_shadow(
        4,
        14,
        ColorU::new(48, 45, 38, 64),
        ColorU::new(0, 0, 0, 160),
    ));

    let mut commands = vec![C::simple(CssProperty::const_width(LayoutWidth::const_px(250)))];
    commands.extend(decl::padding(6, 4, 6, 4));
    commands.extend(decl::themed_fill(LIGHT_STRIP, DARK_STRIP));

    let mut command = row(46);
    command.extend(decl::padding(0, 8, 0, 6));

    let mut command_icon = vec![px(30)];
    command_icon.extend(decl::margin(0, 10, 0, 0));
    command_icon.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    // A command is a command: flora's capitals.
    let mut command_label = caps(CAPS_COMMAND);
    command_label.extend(decl::themed_ink(LIGHT_INK, DARK_INK));

    let mut arrow = vec![px(16)];
    arrow.extend(decl::margin(0, 0, 0, 6));
    arrow.extend(decl::themed_ink(LIGHT_SOFT2, DARK_SOFT2));

    let mut rule = vec![C::simple(CssProperty::const_height(LayoutHeight::const_px(1)))];
    rule.extend(decl::margin(4, 8, 4, 50));
    rule.extend(decl::themed_fill(LIGHT_SEP, DARK_SEP));

    let mut side = vec![C::simple(CssProperty::const_width(LayoutWidth::const_px(320)))];
    side.extend(decl::padding(8, 8, 8, 8));
    side.extend(decl::border_left(1));
    side.extend(decl::themed_border_left_color(LIGHT_BD, DARK_BD));

    // The side column's title: a section title in capitals over a rule.
    let mut title = caps(CAPS_TITLE);
    title.extend(decl::padding(2, 4, 6, 4));
    title.extend(decl::margin(0, 0, 4, 0));
    title.extend(decl::border_bottom(1));
    title.extend(decl::themed_border_bottom_color(LIGHT_SEP, DARK_SEP));
    title.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    let mut open = row(26);
    open.extend(decl::padding(0, 4, 0, 4));

    let mut number = vec![
        px(12),
        C::simple(CssProperty::const_width(LayoutWidth::const_px(16))),
        C::simple(CssProperty::text_decoration(StyleTextDecoration::Underline)),
    ];
    number.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    let mut place_label = vec![px(14), C::simple(CssProperty::const_font_family(SERIF_FAMILY))];
    place_label.extend(decl::themed_ink(LIGHT_INK, DARK_INK));

    let mut pin = row(26);
    pin.push(C::simple(CssProperty::const_width(LayoutWidth::const_px(26))));
    pin.extend(decl::margin(0, 0, 0, 2));

    let mut pin_icon = vec![px(14)];
    pin_icon.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    let mut sub = row(52);
    sub.extend(decl::padding(0, 8, 0, 6));

    let mut sub_icon = vec![px(24)];
    sub_icon.extend(decl::margin(0, 10, 0, 0));
    sub_icon.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    let mut sub_label = vec![
        px(14),
        C::simple(CssProperty::const_font_family(SERIF_FAMILY)),
        decl::semibold(),
    ];
    sub_label.extend(decl::themed_ink(LIGHT_INK, DARK_INK));

    let mut sub_description = vec![px(12)];
    sub_description.extend(decl::margin(1, 0, 0, 0));
    sub_description.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    crate::widgets::ribbon_file_menu::RibbonFileMenuLook {
        theme: super::UiTheme::Flora,
        menu,
        commands,
        command,
        command_icon,
        command_label,
        arrow,
        rule,
        side,
        title,
        place: Vec::new(),
        open,
        number,
        place_label,
        pin,
        pin_icon,
        sub,
        sub_icon,
        sub_label,
        sub_description,
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora File menu.
#[must_use]
pub fn ribbon_file_menu(m: crate::widgets::ribbon_file_menu::RibbonFileMenu) -> Dom {
    crate::widgets::ribbon_file_menu::build(m, &ribbon_file_menu_look())
}

// ==== shells ====
//
// The flora shells keep every measure of the flat ones (the same window
// frame, in the same places) and take flora's paint: the parchment page
// under paper panes, the house hairlines between them, the toolbar strip
// for the module switcher and the bars, the brass ink on the module icons,
// the selection as a wash of --fl-soft with --fl-deep ink by day and the
// accent stone with paper ink by night, the radio row's hover wash under
// the pointer, and flora's accent halo inside every pane F6 lands on - the
// glow at night. ONE look for every shell (`ShellLook`), paint and metrics
// only: the structure is the shells' own (`shells::*_BASE`).

/// A font declaration pair: the chrome size and flora's hand - Garamond
/// (`--font-serif` / `--font-caps`, the bundled EB Garamond first), the face
/// every flora surface writes in; what a shell's content inherits.
fn shell_font(px: isize) -> [CssPropertyWithConditions; 2] {
    [
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            px,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(FONT_CAPS)),
    ]
}

/// A stop at `px` along the gradient line (a length stop, for a raster).
const fn px_stop(px: isize, color: ColorU) -> NormalizedLinearColorStop {
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(color),
        offset_px: FloatValue::const_new(px),
    }
}

/// One hairline raster of `--fl-grain`: `repeating-linear-gradient(<angle>,
/// <ink> 0 1px, transparent 1px 3px)` - a 1px line every 3px. Its clear
/// half is the ink at no alpha, so no renderer fringes the hard stop.
#[must_use]
fn grain_raster(angle: isize, ink: ColorU) -> StyleBackgroundContent {
    let clear = ColorU { a: 0, ..ink };
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: deg(angle),
        extend_mode: ExtendMode::Repeat,
        stops: NormalizedLinearColorStopVec::from_vec(alloc::vec![
            px_stop(0, ink),
            px_stop(1, ink),
            px_stop(1, clear),
            px_stop(3, clear),
        ]),
    })
}

/// The flora GROUND: the page (`--fl-pg`) and the linen it rests on -
/// flora.css's `--fl-grain`, "two hairline rasters the whole ground rests
/// on": `rgba(90,86,74,.030)` across and `.022` down by day,
/// `rgba(0,0,0,.20)` / `.14` at night. Two repeating gradients, each one
/// display item with a native repeat (no tiles), painted over the page
/// colour: what `body` wears on the website, here the shells' and the theme
/// scope's root.
#[must_use]
pub(crate) fn linen_ground(dark: bool) -> Vec<StyleBackgroundContent> {
    if dark {
        alloc::vec![
            StyleBackgroundContent::Color(DARK_PG),
            grain_raster(0, ColorU::new(0, 0, 0, 51)),
            grain_raster(90, ColorU::new(0, 0, 0, 36)),
        ]
    } else {
        alloc::vec![
            StyleBackgroundContent::Color(LIGHT_PG),
            grain_raster(0, ColorU::new(90, 86, 74, 8)),
            grain_raster(90, ColorU::new(90, 86, 74, 6)),
        ]
    }
}

/// `border-right: 1px solid` without a colour.
const fn shell_border_right() -> [CssPropertyWithConditions; 2] {
    [
        CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
            LayoutBorderRightWidth::const_px(1),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
            StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            },
        )),
    ]
}

/// `border-top: 1px solid` without a colour.
const fn shell_border_top() -> [CssPropertyWithConditions; 2] {
    [
        CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
            LayoutBorderTopWidth::const_px(1),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_style(
            StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            },
        )),
    ]
}

/// The backdrop tint under the command palette: a warm dusk by day, night
/// at night.
const SHELL_BACKDROP_LIGHT: ColorU = ColorU::new(40, 34, 24, 100);
const SHELL_BACKDROP_DARK: ColorU = ColorU::new(0, 0, 0, 160);
/// The palette panel's shadow: warm by day, near-black at night.
const SHELL_SHADOW_LIGHT: ColorU = ColorU::new(60, 50, 30, 70);
const SHELL_SHADOW_DARK: ColorU = ColorU::new(0, 0, 0, 170);

/// Flora's shell look.
#[must_use]
#[allow(clippy::too_many_lines)]
pub(crate) fn shell_look() -> crate::widgets::shells::ShellLook {
    use super::decl;
    use crate::widgets::shells::ShellLook;

    let px = |p: CssProperty| CssPropertyWithConditions::simple(p);

    // ---- a clickable item of the chrome: a module button, a row, a tab ----
    let item = || {
        let mut v = decl::padding(6, 10, 6, 10).to_vec();
        v.extend(decl::radius(3));
        v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
        v.extend(decl::hover_fill(RADIO_GROUP_HOVER_LIGHT, RADIO_GROUP_HOVER_DARK));
        v.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));
        v
    };
    // ---- flora's selection on the active item ----
    let selected = || {
        let mut v = decl::themed_fill(LIGHT_SOFT, DARK_ACC).to_vec();
        v.extend(decl::themed_ink(LIGHT_DEEP, DARK_ON_ACC));
        v.push(decl::semibold());
        v
    };
    // ---- a pane: a paper surface and the halo F6 shows ----
    let pane = || {
        let mut v = decl::themed_fill(LIGHT_SUR, DARK_SUR).to_vec();
        v.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));
        v
    };
    // ---- a strip of the chrome (toolbar, bars) ----
    let strip = || decl::themed_fill(LIGHT_STRIP, DARK_STRIP).to_vec();
    let hairline_right = || {
        let mut v = shell_border_right().to_vec();
        v.extend(decl::themed_border_right_color(LIGHT_BD, DARK_BD));
        v
    };
    let hairline_top = || {
        let mut v = shell_border_top().to_vec();
        v.extend(decl::themed_border_top_color(LIGHT_BD, DARK_BD));
        v
    };
    let hairline_bottom = || {
        let mut v = decl::border_bottom(1).to_vec();
        v.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));
        v
    };
    let hairline_left = || {
        let mut v = decl::border_left(1).to_vec();
        v.extend(decl::themed_border_left_color(LIGHT_BD, DARK_BD));
        v
    };

    // ---- OfficeShell ----
    let mut shell_root = shell_font(13).to_vec();
    shell_root.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    shell_root.extend(decl::themed_layers(linen_ground(false), linen_ground(true)));

    let mut shell_rail = strip();
    shell_rail.extend(hairline_right());
    shell_rail.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));

    let mut shell_right_bar = vec![px(CssProperty::const_width(LayoutWidth::const_px(240)))];
    shell_right_bar.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
    shell_right_bar.extend(hairline_left());
    shell_right_bar.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));

    // ---- ShellNavigationPane ----
    let mut nav_root = vec![px(CssProperty::const_width(LayoutWidth::const_px(230)))];
    nav_root.extend(shell_font(13));
    nav_root.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    nav_root.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
    nav_root.extend(hairline_right());

    let mut nav_modules = strip();
    nav_modules.extend(hairline_top());

    let mut nav_module_icon = vec![px(CssProperty::const_font_size(StyleFontSize::const_px(20)))];
    nav_module_icon.extend(decl::margin(0, 10, 0, 0));
    nav_module_icon.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    let mut nav_footer = decl::padding(4, 4, 4, 4).to_vec();
    nav_footer.push(px(CssProperty::const_justify_content(LayoutJustifyContent::End)));

    let mut nav_strip = vec![px(CssProperty::const_width(LayoutWidth::const_px(40)))];
    nav_strip.extend(strip());
    nav_strip.extend(hairline_right());

    let mut nav_strip_item = decl::padding(8, 0, 8, 0).to_vec();
    nav_strip_item.push(px(CssProperty::const_justify_content(LayoutJustifyContent::Center)));
    nav_strip_item.extend(decl::themed_ink(LIGHT_QT, DARK_QT));
    nav_strip_item.extend(decl::hover_fill(RADIO_GROUP_HOVER_LIGHT, RADIO_GROUP_HOVER_DARK));
    nav_strip_item.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));

    // ---- ShellCommandPalette ----
    let mut palette_backdrop = vec![
        px(CssProperty::const_top(LayoutTop::const_px(0))),
        px(CssProperty::const_left(LayoutLeft::const_px(0))),
        px(CssProperty::const_right(LayoutRight::const_px(0))),
        px(CssProperty::const_bottom(LayoutInsetBottom::const_px(0))),
        px(CssProperty::const_z_index(LayoutZIndex::Integer(100))),
        px(CssProperty::const_padding_top(LayoutPaddingTop::const_px(80))),
    ];
    palette_backdrop.extend(decl::themed_fill(SHELL_BACKDROP_LIGHT, SHELL_BACKDROP_DARK));

    let mut palette_panel = vec![
        px(CssProperty::const_width(LayoutWidth::const_px(560))),
        px(CssProperty::const_max_width(LayoutMaxWidth {
            inner: PixelValue::const_percent(90),
        })),
    ];
    palette_panel.extend(shell_font(13));
    palette_panel.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    palette_panel.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
    palette_panel.extend(decl::border(1));
    palette_panel.extend(decl::themed_border_color(LIGHT_BD, DARK_BD));
    palette_panel.extend(decl::radius(5));
    palette_panel.extend(decl::themed_shadow(10, 28, SHELL_SHADOW_LIGHT, SHELL_SHADOW_DARK));

    let mut palette_input = decl::padding(8, 8, 8, 8).to_vec();
    palette_input.extend(decl::border_bottom(1));
    palette_input.extend(decl::themed_border_bottom_color(LIGHT_SEP, DARK_SEP));

    let mut palette_list = vec![px(CssProperty::const_max_height(LayoutMaxHeight::const_px(
        360,
    )))];
    palette_list.extend(decl::padding(4, 4, 4, 4));

    let mut palette_row_icon = vec![px(CssProperty::const_font_size(StyleFontSize::const_px(18)))];
    palette_row_icon.extend(decl::margin(0, 10, 0, 0));
    palette_row_icon.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    let mut palette_row_shortcut = vec![px(CssProperty::const_font_size(StyleFontSize::const_px(
        12,
    )))];
    palette_row_shortcut.extend(decl::margin(0, 0, 0, 12));
    palette_row_shortcut.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    let mut palette_empty = decl::padding(12, 12, 12, 12).to_vec();
    palette_empty.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    // ---- ShellSettingsLayout ----
    let mut settings_root = shell_font(13).to_vec();
    settings_root.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    settings_root.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));

    let mut settings_search = decl::padding(8, 12, 8, 12).to_vec();
    settings_search.extend(strip());
    settings_search.extend(hairline_bottom());

    let mut settings_categories = vec![px(CssProperty::const_width(LayoutWidth::const_px(200)))];
    settings_categories.extend(decl::padding(8, 4, 8, 4));
    settings_categories.extend(strip());
    settings_categories.extend(hairline_right());

    let mut settings_section_title = vec![px(CssProperty::const_font_size(
        StyleFontSize::const_px(12),
    ))];
    settings_section_title.push(decl::semibold());
    settings_section_title.push(decl::letter_spacing_em(0.06));
    settings_section_title.extend(decl::margin(0, 0, 8, 0));
    settings_section_title.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    // ---- ShellEmptyState ----
    let mut empty_root = decl::padding(32, 32, 32, 32).to_vec();
    empty_root.extend(shell_font(13));
    empty_root.extend(decl::themed_ink(LIGHT_INK, DARK_INK));

    let mut empty_icon = vec![px(CssProperty::const_font_size(StyleFontSize::const_px(48)))];
    empty_icon.extend(decl::margin(0, 0, 12, 0));
    empty_icon.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    let mut empty_title = vec![px(CssProperty::const_font_size(StyleFontSize::const_px(15)))];
    empty_title.push(decl::semibold());
    empty_title.extend(decl::margin(0, 0, 4, 0));

    let mut empty_detail = vec![px(CssProperty::const_font_size(StyleFontSize::const_px(13)))];
    empty_detail.extend(decl::margin(0, 0, 12, 0));
    empty_detail.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    // ---- ShellThemeScope ----
    let mut scope_root = shell_font(13).to_vec();
    scope_root.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    scope_root.extend(decl::themed_layers(linen_ground(false), linen_ground(true)));

    // ---- the bars ----
    let mut toolbar_row = decl::padding(4, 8, 4, 8).to_vec();
    toolbar_row.extend(strip());
    toolbar_row.extend(hairline_bottom());

    let mut drawer = decl::themed_fill(LIGHT_SUR, DARK_SUR).to_vec();
    drawer.extend(hairline_top());

    let mut tiles_grid = decl::padding(8, 8, 8, 8).to_vec();
    tiles_grid.extend(decl::themed_fill(LIGHT_DESK, DARK_DESK));

    let mut app_bar = vec![px(CssProperty::const_height(LayoutHeight::const_px(48)))];
    app_bar.extend(decl::padding(0, 8, 0, 8));
    app_bar.extend(shell_font(13));
    app_bar.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    app_bar.extend(strip());
    app_bar.extend(hairline_bottom());

    let mut app_bar_title = vec![px(CssProperty::const_font_size(StyleFontSize::const_px(16)))];
    app_bar_title.push(decl::semibold());
    app_bar_title.extend(decl::margin(0, 8, 0, 8));

    let fab = vec![
        px(CssProperty::const_bottom(LayoutInsetBottom::const_px(16))),
        px(CssProperty::const_right(LayoutRight::const_px(16))),
    ];

    let mut bottom_tabs = vec![px(CssProperty::const_height(LayoutHeight::const_px(56)))];
    bottom_tabs.extend(strip());
    bottom_tabs.extend(hairline_top());

    let mut bottom_tab = decl::padding(6, 4, 6, 4).to_vec();
    bottom_tab.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    bottom_tab.extend(decl::hover_fill(RADIO_GROUP_HOVER_LIGHT, RADIO_GROUP_HOVER_DARK));
    bottom_tab.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));

    let mut bottom_tab_label = vec![px(CssProperty::const_font_size(StyleFontSize::const_px(11)))];
    bottom_tab_label.extend(decl::margin(2, 0, 0, 0));

    ShellLook {
        shell_root,
        shell_title: Vec::new(),
        shell_ribbon: Vec::new(),
        shell_body: decl::themed_fill(LIGHT_SUR, DARK_SUR).to_vec(),
        shell_pane: pane(),
        shell_rail,
        shell_right_bar,
        shell_status: Vec::new(),
        shell_backstage: decl::themed_fill(LIGHT_SUR, DARK_SUR).to_vec(),
        nav_root,
        nav_header: decl::padding(8, 8, 4, 8).to_vec(),
        nav_groups: decl::padding(4, 4, 4, 4).to_vec(),
        nav_modules,
        nav_module: item(),
        nav_module_active: selected(),
        nav_module_icon,
        nav_module_label: vec![px(CssProperty::const_font_size(StyleFontSize::const_px(13)))],
        nav_footer,
        nav_strip,
        nav_strip_item,
        palette_backdrop,
        palette_panel,
        palette_input,
        palette_list,
        palette_row: item(),
        palette_row_selected: selected(),
        palette_row_icon,
        palette_row_label: vec![px(CssProperty::const_font_size(StyleFontSize::const_px(13)))],
        palette_row_shortcut,
        palette_empty,
        settings_root,
        settings_search,
        settings_categories,
        settings_category: item(),
        settings_category_active: selected(),
        settings_sections: decl::padding(16, 24, 16, 24).to_vec(),
        settings_section: decl::margin(0, 0, 20, 0).to_vec(),
        settings_section_title,
        empty_root,
        empty_icon,
        empty_title,
        empty_detail,
        empty_action: decl::margin(8, 0, 0, 0).to_vec(),
        scope_root,
        toolbar_row,
        drawer,
        tiles_grid,
        tile_cell: decl::padding(4, 4, 4, 4).to_vec(),
        app_bar,
        app_bar_title,
        page: decl::themed_fill(LIGHT_SUR, DARK_SUR).to_vec(),
        fab,
        bottom_tabs,
        bottom_tab,
        bottom_tab_active: decl::themed_ink(LIGHT_ACC, DARK_GLOW).to_vec(),
        bottom_tab_icon: vec![px(CssProperty::const_font_size(StyleFontSize::const_px(22)))],
        bottom_tab_label,
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}
// ==== info_bar ====
//
// A flora info bar is the alert's leaf turned into a strip: the faintest
// wash of the kind's stone across the width, a --fl-bd hairline under it
// and the stone's 3px thread down its left edge; the glyph in brass, the
// text in --fl-ink, the action a link key. At night the night surface and
// inks, the thread lifted to the stone's glow.

/// The flora strip for one kind.
fn flora_info_bar_strip(kind: crate::widgets::alert::AlertKind) -> Vec<CssPropertyWithConditions> {
    use super::decl;
    let stone = alert_stone(kind);
    let mut v = vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            13,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ];
    v.extend(decl::padding(6, 12, 6, 10));
    v.extend(decl::border_bottom(1));
    v.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));
    v.extend(decl::border_left(3));
    v.extend(decl::themed_border_left_color(stone.stone, stone.glow));
    v.extend(decl::themed_fill(stone.soft, DARK_SUR));
    v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    v
}

/// Flora's info-bar look.
#[must_use]
pub(crate) fn info_bar_look() -> crate::widgets::info_bar::InfoBarLook {
    use super::decl;
    let mut icon = vec![CssPropertyWithConditions::simple(CssProperty::const_font_size(
        StyleFontSize::const_px(18),
    ))];
    icon.extend(decl::margin(0, 8, 0, 0));
    icon.extend(decl::themed_ink(LIGHT_QT, DARK_QT));
    crate::widgets::info_bar::InfoBarLook {
        strip: flora_info_bar_strip,
        icon,
        text: Vec::new(),
        action: decl::margin(0, 0, 0, 12).to_vec(),
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora info bar.
#[must_use]
pub fn info_bar(b: crate::widgets::info_bar::InfoBar) -> Dom {
    crate::widgets::info_bar::build(b, &info_bar_look())
}

// ==== mail widgets: shared strokes ====
//
// What the mail panes share in flora: the toolbar strip (--fl-strip) closed
// by a --fl-bd hairline below or above (a search row, a sort band, a
// footer, a button row) and the leaf (--fl-sur) the list and the message
// sit on, in --fl-ink.

/// A toolbar strip closed by a hairline below.
fn flora_strip_below() -> Vec<CssPropertyWithConditions> {
    use super::decl;
    let mut v = decl::themed_fill(LIGHT_STRIP, DARK_STRIP).to_vec();
    v.extend(decl::border_bottom(1));
    v.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));
    v
}

/// A toolbar strip opened by a hairline above.
fn flora_strip_above() -> Vec<CssPropertyWithConditions> {
    use super::decl;
    let mut v = decl::themed_fill(LIGHT_STRIP, DARK_STRIP).to_vec();
    v.extend(decl::border_top(1));
    v.extend(decl::themed_border_top_color(LIGHT_BD, DARK_BD));
    v
}

/// The leaf: the UI face in the ink on the surface.
fn flora_leaf() -> Vec<CssPropertyWithConditions> {
    use super::decl;
    let mut v = vec![
        super::decl::font_size(13),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ];
    v.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    v.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
    v
}

/// Flora's small label: bold, tracked out, in --fl-soft1 (`.fl-label`).
fn flora_label() -> Vec<CssPropertyWithConditions> {
    use super::decl;
    let mut v = vec![
        super::decl::font_size(11),
        decl::bold(),
        decl::letter_spacing_em(0.08),
    ];
    v.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    v
}

// ==== summary_list ====
//
// A flora message list is a leaf under a toolbar strip and a sort band: the
// rows a --fl-sep hairline apart, the sender in --fl-ink (bold when unread),
// the subject and the date in --fl-soft1, the preview in --fl-soft2, the
// glyphs in brass; a row washes to the radio row's hover under the pointer,
// the selected one sits on the track colour, focus is the accent halo
// inside; a group header is `.fl-label` on the strip. At night every ink
// and wash takes its night value.

/// Flora's summary-list look.
#[must_use]
pub(crate) fn summary_list_look() -> crate::widgets::summary_list::SummaryListLook {
    use super::decl;

    let mut toolbar = decl::padding(6, 8, 6, 8).to_vec();
    toolbar.extend(flora_strip_below());

    let mut sort = vec![decl::font_size(12)];
    sort.extend(decl::padding(4, 8, 4, 8));
    sort.extend(flora_strip_below());
    sort.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    let mut row = decl::padding(6, 8, 6, 8).to_vec();
    row.extend(decl::border_bottom(1));
    row.extend(decl::themed_border_bottom_color(LIGHT_SEP, DARK_SEP));
    row.extend(decl::hover_fill(
        RADIO_GROUP_HOVER_LIGHT,
        RADIO_GROUP_HOVER_DARK,
    ));
    row.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));

    let mut group = flora_label();
    group.extend(decl::padding(4, 8, 3, 8));
    group.extend(flora_strip_below());

    let mut icon = vec![decl::font_size(18)];
    icon.extend(decl::margin(0, 10, 0, 0));
    icon.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    let mut subject = vec![decl::font_size(12)];
    subject.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    let mut preview = vec![decl::font_size(12)];
    preview.extend(decl::themed_ink(LIGHT_SOFT2, DARK_SOFT2));
    let mut date = vec![decl::font_size(12)];
    date.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    date.extend(decl::margin(0, 0, 2, 10));
    let mut attachment = vec![decl::font_size(14)];
    attachment.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    crate::widgets::summary_list::SummaryListLook {
        list: flora_leaf(),
        toolbar,
        search: decl::margin(0, 6, 0, 0).to_vec(),
        scopes: Vec::new(),
        sort,
        rows: Vec::new(),
        row,
        row_unread: Vec::new(),
        row_selected: decl::themed_fill(LIGHT_TRACK, DARK_TRACK).to_vec(),
        group,
        icon,
        from: Vec::new(),
        from_unread: vec![decl::bold()],
        subject,
        preview,
        date,
        attachment,
        flag: decl::margin(0, 0, 0, 4).to_vec(),
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora message list.
#[must_use]
pub fn summary_list(l: crate::widgets::summary_list::SummaryList) -> Dom {
    crate::widgets::summary_list::build(l, &summary_list_look())
}

// ==== reading_pane ====
//
// A flora reading pane is the message on a leaf: the subject large and
// semibold over the sender line, the header fields in a block under a
// --fl-bd hairline with the keys as `.fl-label`, the body on the same leaf,
// and the people footer a toolbar strip over a hairline. At night the night
// leaf and inks.

/// Flora's reading-pane look.
#[must_use]
pub(crate) fn reading_pane_look() -> crate::widgets::reading_pane::ReadingPaneLook {
    use super::decl;

    let mut header = decl::padding(14, 16, 10, 16).to_vec();
    header.extend(decl::border_bottom(1));
    header.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));

    let mut subject = vec![decl::font_size(20), decl::semibold()];
    subject.extend(decl::margin(0, 0, 4, 0));

    let mut date = vec![decl::font_size(12)];
    date.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    date.extend(decl::margin(0, 0, 0, 12));

    let mut fields = vec![decl::font_size(12)];
    fields.extend(decl::padding(8, 16, 8, 16));
    fields.extend(decl::border_bottom(1));
    fields.extend(decl::themed_border_bottom_color(LIGHT_SEP, DARK_SEP));

    let mut field_key = flora_label();
    field_key.push(CssPropertyWithConditions::simple(CssProperty::const_width(
        LayoutWidth::const_px(56),
    )));
    field_key.extend(decl::margin(0, 8, 0, 0));

    let mut attachments = decl::padding(6, 16, 6, 16).to_vec();
    attachments.extend(decl::border_bottom(1));
    attachments.extend(decl::themed_border_bottom_color(LIGHT_SEP, DARK_SEP));

    let mut footer = vec![decl::font_size(12)];
    footer.extend(decl::padding(8, 16, 8, 16));
    footer.extend(flora_strip_above());

    crate::widgets::reading_pane::ReadingPaneLook {
        pane: flora_leaf(),
        header,
        subject,
        sender_line: vec![decl::font_size(12)],
        date,
        notice: Vec::new(),
        fields,
        field_key,
        field_value: Vec::new(),
        attachments,
        body: decl::padding(16, 16, 16, 16).to_vec(),
        footer,
        footer_line: decl::margin(0, 0, 0, 8).to_vec(),
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora reading pane.
#[must_use]
pub fn reading_pane(p: crate::widgets::reading_pane::ReadingPane) -> Dom {
    crate::widgets::reading_pane::build(p, &reading_pane_look())
}

// ==== todo_bar ====
//
// A flora To-Do bar is a toolbar strip in a column: the calendar at the
// top, the appointments between two --fl-bd hairlines (`.fl-label` tone
// when there are none), the task line, and the tasks a --fl-sep hairline
// apart, a done task's date in --fl-soft2. At night the night strip and
// inks.

/// Flora's To-Do bar look.
#[must_use]
pub(crate) fn todo_bar_look() -> crate::widgets::todo_bar::ToDoBarLook {
    use super::decl;

    let mut bar = vec![
        decl::font_size(13),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ];
    bar.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    bar.extend(decl::themed_fill(LIGHT_STRIP, DARK_STRIP));
    bar.extend(decl::padding(10, 10, 10, 10));

    let mut appointments = decl::padding(6, 4, 6, 4).to_vec();
    appointments.extend(decl::border_top(1));
    appointments.extend(decl::themed_border_top_color(LIGHT_BD, DARK_BD));
    appointments.extend(decl::border_bottom(1));
    appointments.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));

    let mut empty = vec![decl::font_size(12)];
    empty.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    empty.extend(decl::padding(4, 0, 4, 0));

    let mut task = decl::padding(4, 0, 4, 0).to_vec();
    task.extend(decl::border_bottom(1));
    task.extend(decl::themed_border_bottom_color(LIGHT_SEP, DARK_SEP));

    let mut task_due = vec![decl::font_size(11)];
    task_due.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    task_due.extend(decl::margin(0, 0, 0, 6));

    crate::widgets::todo_bar::ToDoBarLook {
        bar,
        calendar: decl::margin(0, 0, 10, 0).to_vec(),
        appointments,
        appointment: decl::padding(2, 0, 2, 0).to_vec(),
        empty,
        task_input: decl::margin(8, 0, 8, 0).to_vec(),
        tasks: Vec::new(),
        task,
        task_done: decl::themed_ink(LIGHT_SOFT2, DARK_SOFT2).to_vec(),
        task_title: decl::margin(0, 0, 0, 6).to_vec(),
        task_due,
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora To-Do bar.
#[must_use]
pub fn todo_bar(b: crate::widgets::todo_bar::ToDoBar) -> Dom {
    crate::widgets::todo_bar::build(b, &todo_bar_look())
}

// ==== wizard_layout ====
//
// A flora wizard is a dialog of paper: the rail on a toolbar strip over a
// hairline, the page on the leaf with its title semibold, the buttons on a
// strip under a hairline. At night the night strip, leaf and inks.

/// Flora's wizard-layout look.
#[must_use]
pub(crate) fn wizard_layout_look() -> crate::widgets::wizard_layout::WizardLayoutLook {
    use super::decl;

    let mut rail = decl::padding(12, 16, 12, 16).to_vec();
    rail.extend(flora_strip_below());

    let mut title = vec![decl::font_size(18), decl::semibold()];
    title.extend(decl::margin(0, 0, 12, 0));

    let mut buttons = decl::padding(10, 16, 10, 16).to_vec();
    buttons.extend(flora_strip_above());

    // The frames: the banner a band of paper over a hairline, the side
    // panel a toolbar strip; the glyphs in brass.
    let mut subtitle = vec![decl::font_size(12)];
    subtitle.extend(decl::themed_ink(LIGHT_INK2, DARK_INK2));
    subtitle.extend(decl::margin(2, 0, 8, 0));

    let mut banner = decl::padding(12, 16, 4, 16).to_vec();
    banner.extend(decl::themed_fill(LIGHT_PG, DARK_PG));
    banner.extend(decl::border_bottom(1));
    banner.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));

    let mut banner_icon = vec![decl::font_size(32)];
    banner_icon.extend(decl::margin(0, 0, 8, 12));
    banner_icon.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    let mut side_panel = vec![CssPropertyWithConditions::simple(CssProperty::const_width(
        LayoutWidth::const_px(180),
    ))];
    side_panel.extend(decl::padding(16, 12, 16, 16));
    side_panel.extend(decl::themed_fill(LIGHT_STRIP, DARK_STRIP));
    side_panel.extend(shell_border_right());
    side_panel.extend(decl::themed_border_right_color(LIGHT_BD, DARK_BD));

    let mut side_icon = vec![decl::font_size(40)];
    side_icon.extend(decl::margin(0, 0, 16, 0));
    side_icon.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    let mut side_step = vec![decl::font_size(13)];
    side_step.extend(decl::padding(4, 0, 4, 0));
    side_step.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    let mut side_step_current = vec![decl::semibold()];
    side_step_current.extend(decl::themed_ink(LIGHT_INK, DARK_INK));

    let warning = crate::widgets::alert::AlertKind::Warning;
    let mut reason = vec![decl::font_size(12)];
    reason.extend(decl::margin(0, 12, 0, 12));
    reason.extend(decl::themed_ink(warning.colors().2, warning.dark_colors().2));

    crate::widgets::wizard_layout::WizardLayoutLook {
        layout: flora_leaf(),
        rail,
        page: decl::padding(16, 16, 16, 16).to_vec(),
        title,
        buttons,
        button: decl::margin(0, 0, 0, 8).to_vec(),
        subtitle,
        banner,
        banner_title: vec![decl::font_size(14), decl::semibold()],
        banner_icon,
        side_panel,
        side_icon,
        side_step,
        side_step_current,
        reason,
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora wizard layout.
#[must_use]
pub fn wizard_layout(w: crate::widgets::wizard_layout::WizardLayout) -> Dom {
    crate::widgets::wizard_layout::build(w, &wizard_layout_look())
}

// ==== dialog kit (wizard pages, path input, shortcut recorder, settings rows, standard dialogs) ====
//
// A flora dialog is paper: the leaf's ink, a heading semibold, sizes / help
// / descriptions in --fl-soft1, a field box of --fl-fld under a --fl-bd3
// hairline that takes the accent ring on focus (the glow at night), rows a
// --fl-sep apart, a setting row's label column 240 wide, a search match on
// a warm wash, the recorder a field that washes to the radio row's hover
// while it listens, the message glyphs in the alert palette's inks (the
// question in the accent), the button row a toolbar strip over a hairline.

/// The search match's wash, light and dark.
const KIT_MARK_LIGHT: ColorU = ColorU::new(246, 222, 150, 255);
const KIT_MARK_DARK: ColorU = ColorU::new(96, 78, 30, 255);
const KIT_MARK_INK_DARK: ColorU = ColorU::new(255, 236, 179, 255);

/// Flora's dialog-kit look.
#[must_use]
pub(crate) fn dialog_kit_look() -> crate::widgets::dialog_kit::DialogKitLook {
    use super::decl;
    use crate::widgets::alert::AlertKind;

    let px = |p: CssProperty| CssPropertyWithConditions::simple(p);
    let soft = |size: isize| {
        let mut v = vec![decl::font_size(size)];
        v.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
        v
    };
    let glyph_of = |kind: AlertKind| {
        let mut v = vec![decl::font_size(32)];
        v.extend(decl::margin(0, 16, 0, 0));
        v.extend(decl::themed_ink(kind.colors().2, kind.dark_colors().2));
        v
    };

    let mut page = vec![
        decl::font_size(13),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ];
    page.extend(decl::themed_ink(LIGHT_INK, DARK_INK));

    let mut logo = vec![decl::font_size(48)];
    logo.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    let mut label = vec![decl::semibold()];
    label.extend(decl::margin(0, 0, 4, 0));

    let mut scroll_box = decl::border(1).to_vec();
    scroll_box.extend(decl::themed_border_color(LIGHT_BD3, DARK_BD3));
    scroll_box.extend(decl::themed_fill(LIGHT_FLD, DARK_FLD));
    scroll_box.extend(decl::padding(6, 8, 6, 8));
    scroll_box.extend(decl::radius(3));
    scroll_box.extend(decl::focus_ring(LIGHT_ACC, DARK_GLOW));

    let mut list_row = decl::padding(4, 4, 4, 4).to_vec();
    list_row.extend(decl::border_bottom(1));
    list_row.extend(decl::themed_border_bottom_color(LIGHT_SEP, DARK_SEP));

    let mut size = soft(12);
    size.extend(decl::margin(0, 0, 0, 12));

    let mut total = vec![decl::semibold()];
    total.extend(decl::padding(8, 0, 0, 0));

    let mut description = soft(12);
    description.extend(decl::margin(0, 0, 6, 28));

    let mut summary_key = flora_label();
    summary_key.extend(decl::margin(4, 0, 0, 0));

    let mut field_row = decl::padding(8, 0, 8, 0).to_vec();
    field_row.extend(decl::border_bottom(1));
    field_row.extend(decl::themed_border_bottom_color(LIGHT_SEP, DARK_SEP));

    let mut field_label = vec![px(CssProperty::const_width(LayoutWidth::const_px(240)))];
    field_label.extend(decl::margin(0, 16, 0, 0));

    let mut help = soft(12);
    help.extend(decl::margin(2, 0, 0, 0));

    let mut mark = decl::themed_fill(KIT_MARK_LIGHT, KIT_MARK_DARK).to_vec();
    mark.extend(decl::themed_ink(LIGHT_INK, KIT_MARK_INK_DARK));

    let mut modified = vec![decl::font_size(12)];
    modified.extend(decl::margin(0, 6, 0, 0));
    modified.extend(decl::themed_ink(LIGHT_ACC, DARK_GLOW));

    let mut unit = soft(12);
    unit.extend(decl::margin(0, 0, 0, 6));

    let mut recorder = vec![
        px(CssProperty::const_min_width(LayoutMinWidth::const_px(160))),
        decl::font_size(13),
    ];
    recorder.extend(decl::border(1));
    recorder.extend(decl::themed_border_color(LIGHT_BD3, DARK_BD3));
    recorder.extend(decl::themed_fill(LIGHT_FLD, DARK_FLD));
    recorder.extend(decl::padding(4, 8, 4, 8));
    recorder.extend(decl::radius(3));
    recorder.extend(decl::hover_border_color(LIGHT_ACC, DARK_GLOW));
    recorder.extend(decl::focus_ring(LIGHT_ACC, DARK_GLOW));

    let mut recorder_recording = decl::themed_border_color(LIGHT_ACC, DARK_GLOW);
    recorder_recording.extend(decl::themed_fill(RADIO_GROUP_HOVER_LIGHT, RADIO_GROUP_HOVER_DARK));

    let mut dialog = vec![px(CssProperty::const_min_width(LayoutMinWidth::const_px(360)))];
    dialog.extend(decl::padding(16, 20, 16, 20));

    let mut icon_question = vec![decl::font_size(32)];
    icon_question.extend(decl::margin(0, 16, 0, 0));
    icon_question.extend(decl::themed_ink(LIGHT_ACC, DARK_GLOW));

    let warning = AlertKind::Warning;
    let mut notice = vec![decl::font_size(12)];
    notice.extend(decl::margin(0, 12, 0, 12));
    notice.extend(decl::themed_ink(warning.colors().2, warning.dark_colors().2));

    let mut category_icon = vec![decl::font_size(16)];
    category_icon.extend(decl::margin(0, 8, 0, 0));
    category_icon.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    let mut buttons = decl::padding(10, 16, 10, 16).to_vec();
    buttons.extend(flora_strip_above());

    crate::widgets::dialog_kit::DialogKitLook {
        page,
        heading: vec![decl::font_size(18), decl::semibold()],
        text: vec![decl::font_size(13)],
        hint: soft(12),
        logo,
        block: decl::margin(0, 0, 12, 0).to_vec(),
        label,
        scroll_box,
        list_row,
        size,
        total,
        check_row: decl::padding(3, 0, 3, 0).to_vec(),
        check_label: decl::margin(0, 0, 0, 8).to_vec(),
        description,
        summary_key,
        summary_row: decl::padding(2, 0, 0, 16).to_vec(),
        field_row,
        field_label,
        help,
        mark,
        modified,
        unit,
        recorder,
        recorder_recording,
        dialog,
        icon_info: glyph_of(AlertKind::Info),
        icon_warning: glyph_of(AlertKind::Warning),
        icon_error: glyph_of(AlertKind::Danger),
        icon_question,
        buttons,
        button: decl::margin(0, 0, 0, 8).to_vec(),
        notice,
        category_icon,
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}
// ==== timeline ====
//
// A flora timeline is the editor's bench: the ruler on the toolbar strip
// (--fl-strip) under a --fl-bd hairline, the track headers on the leaf with
// their names in `.fl-label`, the lanes on the desk a --fl-sep hairline
// apart, the clips as rounded blocks in earthy tints (slate video, moss
// audio, plum titles, the accent, a stone grey) with warm white names, the
// selected clip ringed in brass, the playhead a clay line; focus is the
// accent halo inside the lanes (the glow at night).

const TIMELINE_VIDEO_LIGHT: ColorU = ColorU::rgb(0x5A, 0x72, 0xA3);
const TIMELINE_VIDEO_DARK: ColorU = ColorU::rgb(0x45, 0x5B, 0x85);
const TIMELINE_AUDIO_LIGHT: ColorU = ColorU::rgb(0x5E, 0x85, 0x50);
const TIMELINE_AUDIO_DARK: ColorU = ColorU::rgb(0x4A, 0x6A, 0x3F);
const TIMELINE_TITLE_LIGHT: ColorU = ColorU::rgb(0x96, 0x68, 0x8A);
const TIMELINE_TITLE_DARK: ColorU = ColorU::rgb(0x77, 0x50, 0x6D);
const TIMELINE_MUTED_LIGHT: ColorU = ColorU::rgb(0x84, 0x7F, 0x74);
const TIMELINE_MUTED_DARK: ColorU = ColorU::rgb(0x5A, 0x56, 0x4F);
const TIMELINE_CLIP_INK: ColorU = ColorU::rgb(0xFB, 0xF8, 0xF0);
const TIMELINE_PLAYHEAD_LIGHT: ColorU = ColorU::rgb(0xB4, 0x3C, 0x2C);
const TIMELINE_PLAYHEAD_DARK: ColorU = ColorU::rgb(0xE0, 0x6A, 0x55);
const TIMELINE_SELECTED_RING: ColorU = ColorU::rgb(0xE0, 0xB3, 0x41);

/// A line `px` wide (a tick, the playhead).
fn flora_timeline_line(px: isize, light: ColorU, dark: ColorU) -> Vec<CssPropertyWithConditions> {
    use super::decl;
    let mut v = vec![CssPropertyWithConditions::simple(CssProperty::const_width(
        LayoutWidth::const_px(px),
    ))];
    v.extend(decl::themed_fill(light, dark));
    v
}

/// A flora clip of `tint`: its block, its warm white name.
fn flora_timeline_clip(
    tint: crate::widgets::timeline::TimelineClipTint,
) -> Vec<CssPropertyWithConditions> {
    use super::decl;
    use crate::widgets::timeline::TimelineClipTint as T;
    let (light, dark) = match tint {
        T::Video => (TIMELINE_VIDEO_LIGHT, TIMELINE_VIDEO_DARK),
        T::Audio => (TIMELINE_AUDIO_LIGHT, TIMELINE_AUDIO_DARK),
        T::Title => (TIMELINE_TITLE_LIGHT, TIMELINE_TITLE_DARK),
        T::Accent => (LIGHT_ACC, DARK_ACC),
        T::Muted => (TIMELINE_MUTED_LIGHT, TIMELINE_MUTED_DARK),
    };
    let mut v = decl::themed_fill(light, dark).to_vec();
    v.extend(decl::themed_ink(TIMELINE_CLIP_INK, TIMELINE_CLIP_INK));
    v.extend(decl::radius(4));
    v.extend(decl::padding(0, 6, 0, 4));
    v
}

/// Flora's timeline look.
#[must_use]
pub(crate) fn timeline_look() -> crate::widgets::timeline::TimelineLook {
    use super::decl;

    let mut corner = vec![decl::font_size(12), decl::semibold()];
    corner.extend(decl::padding(0, 10, 0, 10));
    corner.extend(decl::themed_ink(LIGHT_INK2, DARK_INK2));

    let mut ruler = decl::border_left(1).to_vec();
    ruler.extend(decl::themed_border_left_color(LIGHT_BD, DARK_BD));

    let mut tick_label = vec![decl::font_size(10)];
    tick_label.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    let mut header = decl::padding(0, 2, 0, 10).to_vec();
    header.extend(decl::border_bottom(1));
    header.extend(decl::themed_border_bottom_color(LIGHT_SEP, DARK_SEP));

    let mut lanes = decl::themed_fill(LIGHT_DESK, DARK_DESK).to_vec();
    lanes.extend(decl::border_left(1));
    lanes.extend(decl::themed_border_left_color(LIGHT_BD, DARK_BD));
    lanes.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));

    let mut lane = decl::border_bottom(1).to_vec();
    lane.extend(decl::themed_border_bottom_color(LIGHT_SEP, DARK_SEP));

    let selected = CssPropertyWithConditions::themed(
        decl::shadow(0, 0, 2, TIMELINE_SELECTED_RING, true),
        decl::shadow(0, 0, 2, TIMELINE_SELECTED_RING, true),
    )
    .to_vec();

    let mut clip_thumb = decl::margin(0, 5, 0, 0).to_vec();
    clip_thumb.extend(decl::radius(3));

    let clip_label = vec![decl::font_size(11), decl::semibold()];
    let mut clip_detail = vec![decl::font_size(10)];
    clip_detail.extend(decl::margin(0, 0, 0, 6));

    let mut scroll_track = decl::themed_fill(LIGHT_TRACK, DARK_TRACK).to_vec();
    scroll_track.extend(decl::radius(4));
    scroll_track.extend(decl::margin(2, 8, 2, 4));

    let mut thumb = decl::themed_fill(LIGHT_BD3, DARK_BD3).to_vec();
    thumb.extend(decl::radius(3));

    crate::widgets::timeline::TimelineLook {
        root: flora_leaf(),
        head: flora_strip_below(),
        corner,
        ruler,
        tick: flora_timeline_line(1, LIGHT_BD3, DARK_BD3),
        tick_minor: flora_timeline_line(1, LIGHT_BD, DARK_BD),
        tick_label,
        ruler_head: flora_timeline_line(2, TIMELINE_PLAYHEAD_LIGHT, TIMELINE_PLAYHEAD_DARK),
        headers: decl::themed_fill(LIGHT_SUR, DARK_SUR).to_vec(),
        header,
        track_name: flora_label(),
        lanes,
        lane,
        clip: flora_timeline_clip,
        clip_selected: selected,
        clip_thumb,
        clip_label,
        clip_detail,
        playhead: flora_timeline_line(2, TIMELINE_PLAYHEAD_LIGHT, TIMELINE_PLAYHEAD_DARK),
        scroll: flora_strip_above(),
        scroll_track,
        thumb,
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora timeline.
#[must_use]
pub fn timeline(t: crate::widgets::timeline::Timeline) -> Dom {
    crate::widgets::timeline::build(t, &timeline_look())
}
// ==== selection_adorner ====
//
// A flora selection adorner draws in the accent stone: the frame a hairline
// of --fl-acc, the handles round paper buttons ringed in it, the rotate
// handle a filled accent dot on an accent stem, the smart guides in clay,
// the marquee a soft accent wash in an accent hairline; focus is the accent
// halo inside the canvas. At night the accent lifts to its glow and the
// handles take the night page.

const ADORNER_GUIDE_LIGHT: ColorU = ColorU::new(0xB4, 0x5A, 0x3C, 255);
const ADORNER_GUIDE_DARK: ColorU = ColorU::new(0xE0, 0x9A, 0x7A, 255);
const ADORNER_MARQUEE_LIGHT: ColorU = ColorU::new(0x2F, 0x4A, 0x85, 28);
const ADORNER_MARQUEE_DARK: ColorU = ColorU::new(0x7A, 0x93, 0xC6, 40);

/// Flora's selection-adorner look.
#[must_use]
pub(crate) fn selection_adorner_look() -> crate::widgets::selection_adorner::SelectionAdornerLook {
    use super::decl;

    let mut frame = decl::border(1).to_vec();
    frame.extend(decl::themed_border_color(LIGHT_ACC, DARK_GLOW));

    let mut handle = decl::border(1).to_vec();
    handle.extend(decl::themed_border_color(LIGHT_ACC, DARK_GLOW));
    handle.extend(decl::themed_fill(LIGHT_PG, DARK_PG));
    handle.extend(decl::radius(4));

    let mut marquee = decl::themed_fill(ADORNER_MARQUEE_LIGHT, ADORNER_MARQUEE_DARK).to_vec();
    marquee.extend(decl::border(1));
    marquee.extend(decl::themed_border_color(LIGHT_ACC, DARK_GLOW));

    crate::widgets::selection_adorner::SelectionAdornerLook {
        root: decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW).to_vec(),
        frame: frame.clone(),
        group: frame,
        editing: decl::themed_fill(LIGHT_GLOW, DARK_GLOW).to_vec(),
        handle,
        rotate: decl::themed_fill(LIGHT_ACC, DARK_GLOW).to_vec(),
        stem: decl::themed_fill(LIGHT_ACC, DARK_GLOW).to_vec(),
        guide: decl::themed_fill(ADORNER_GUIDE_LIGHT, ADORNER_GUIDE_DARK).to_vec(),
        marquee,
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

// ==== thumbnail_strip ====
//
// A flora thumbnail strip lies on the toolbar strip: the numbers in the
// quiet ink, each preview on paper in a --fl-bd frame that takes the accent
// stone when the slide is selected, a hidden slide at half strength, the
// section headers semibold in the ink; focus is the accent halo inside the
// item. At night the night strip, the accent's glow.

/// Flora's thumbnail-strip look.
#[must_use]
pub(crate) fn thumbnail_strip_look() -> crate::widgets::thumbnail_strip::ThumbnailStripLook {
    use super::decl;

    let mut strip = vec![
        decl::font_size(12),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ];
    strip.extend(decl::themed_fill(LIGHT_STRIP, DARK_STRIP));
    strip.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    strip.extend(decl::padding(6, 6, 6, 6));

    let mut section = vec![decl::font_size(12), decl::semibold()];
    section.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    section.extend(decl::padding(6, 4, 4, 2));

    let mut section_icon = vec![decl::font_size(16)];
    section_icon.extend(decl::margin(0, 4, 0, 0));
    section_icon.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    let mut item = decl::padding(4, 6, 4, 2).to_vec();
    item.extend(decl::radius(4));
    item.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));

    let mut number = vec![decl::font_size(12)];
    number.extend(decl::themed_ink(LIGHT_QT, DARK_QT));
    number.extend(decl::margin(2, 6, 0, 0));

    let mut badge = vec![decl::font_size(14)];
    badge.extend(decl::themed_ink(LIGHT_QT, DARK_QT));

    let mut thumb = decl::border(2).to_vec();
    thumb.extend(decl::themed_border_color(LIGHT_BD, DARK_BD));
    thumb.extend(decl::themed_fill(LIGHT_PG, DARK_PG));
    thumb.extend(decl::radius(2));

    crate::widgets::thumbnail_strip::ThumbnailStripLook {
        strip,
        section,
        section_icon,
        item,
        item_selected: decl::themed_fill(LIGHT_TRACK, DARK_TRACK).to_vec(),
        number,
        badge,
        thumb,
        thumb_selected: decl::themed_border_color(LIGHT_ACC, DARK_GLOW),
        thumb_hidden: vec![CssPropertyWithConditions::simple(CssProperty::const_opacity(
            azul_css::props::style::StyleOpacity::const_new(50),
        ))],
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

// ==== cell_grid ====
//
// A flora grid is a page of cells a --fl-sep hairline apart under a strip
// of column letters and beside a strip of row numbers (--fl-strip, the
// letters in --fl-soft1); the headers of the selection sit on the track
// colour, a wholly selected column or row in the accent; the selected cells
// take the track colour, the current range the accent outline with the fill
// handle at its corner, the editor a page-coloured box in the accent. At
// night every surface, ink and wash takes its night value.

/// Flora's cell-grid look.
#[must_use]
pub(crate) fn cell_grid_look() -> crate::widgets::cell_grid::CellGridLook {
    use super::decl;

    let mut grid = alloc::vec![CssPropertyWithConditions::simple(
        CssProperty::const_font_family(SYSTEM_UI_FAMILY)
    )];
    grid.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    grid.extend(decl::themed_fill(LIGHT_PG, DARK_PG));

    let mut header = alloc::vec![decl::font_size(11)];
    header.extend(decl::themed_fill(LIGHT_STRIP, DARK_STRIP));
    header.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    header.extend(decl::border_right(1));
    header.extend(decl::themed_border_right_color(LIGHT_BD, DARK_BD));
    header.extend(decl::border_bottom(1));
    header.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));

    let mut header_active = decl::themed_fill(LIGHT_TRACK, DARK_TRACK).to_vec();
    header_active.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    let mut header_selected = decl::themed_fill(LIGHT_ACC, DARK_ACC).to_vec();
    header_selected.extend(decl::themed_ink(LIGHT_ON_ACC, DARK_ON_ACC));

    let mut grid_line_right = decl::border_right(1).to_vec();
    grid_line_right.extend(decl::themed_border_right_color(LIGHT_SEP, DARK_SEP));
    let mut grid_line_bottom = decl::border_bottom(1).to_vec();
    grid_line_bottom.extend(decl::themed_border_bottom_color(LIGHT_SEP, DARK_SEP));
    let mut no_line_right = decl::border_right(1).to_vec();
    no_line_right.extend(decl::themed_border_right_color(LIGHT_PG, DARK_PG));
    let mut no_line_bottom = decl::border_bottom(1).to_vec();
    no_line_bottom.extend(decl::themed_border_bottom_color(LIGHT_PG, DARK_PG));

    let mut outline = decl::border(2).to_vec();
    outline.extend(decl::themed_border_color(LIGHT_ACC, DARK_ACC));
    let mut fill_handle = decl::themed_fill(LIGHT_ACC, DARK_ACC).to_vec();
    fill_handle.extend(decl::border(1));
    fill_handle.extend(decl::themed_border_color(LIGHT_PG, DARK_PG));
    let mut fill_preview = decl::border(1).to_vec();
    fill_preview.extend(decl::themed_border_color(LIGHT_SOFT1, DARK_SOFT1));
    let mut editor = decl::themed_fill(LIGHT_PG, DARK_PG).to_vec();
    editor.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    editor.extend(decl::border(2));
    editor.extend(decl::themed_border_color(LIGHT_ACC, DARK_ACC));
    editor.extend(decl::padding(0, 3, 0, 3));

    crate::widgets::cell_grid::CellGridLook {
        grid,
        corner: header.clone(),
        header,
        header_active,
        header_selected,
        grid_line_right,
        grid_line_bottom,
        no_line_right,
        no_line_bottom,
        selected: decl::themed_fill(LIGHT_TRACK, DARK_TRACK).to_vec(),
        freeze_line: decl::themed_fill(LIGHT_BD3, DARK_BD3).to_vec(),
        outline,
        fill_handle,
        fill_preview,
        editor,
        caret: decl::themed_fill(LIGHT_INK, DARK_INK).to_vec(),
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora cell grid.
#[must_use]
pub(crate) fn cell_grid(g: crate::widgets::cell_grid::CellGridResolved) -> Dom {
    crate::widgets::cell_grid::build(g, &cell_grid_look())
}

// ==== tree_view badge ====
//
// The count after a tree node's label: semibold in flora's accent on the
// field paper (its glow at night, the accent itself being too deep for the
// dark paper), and on the selected stone in the stone's own ink, like the
// label beside it.

/// Flora's look for a tree node's badge.
#[must_use]
pub(crate) fn tree_view_badge_look() -> crate::widgets::tree_view::TreeViewBadgeLook {
    use super::decl;
    use crate::widgets::tree_view as t;
    type P = CssPropertyWithConditions;

    let badge = |ink: Vec<P>| {
        let mut v = t::BADGE_BASE.to_vec();
        v.extend(decl::padding(0, 2, 0, 6));
        v.push(decl::semibold());
        v.extend(ink);
        CssPropertyWithConditionsVec::from_vec(v)
    };
    t::TreeViewBadgeLook {
        badge: badge(decl::themed_ink(LIGHT_ACC, DARK_GLOW).to_vec()),
        badge_selected: badge(vec![P::simple(decl::ink(LIGHT_ON_ACC))]),
    }
}

// ==== button: toggled ====

/// The face a toggled-on button rests on (`Button::with_toggled(true)`):
/// the face its `:active` state shows, at rest, in both modes - paper
/// pushed in for the standard and the illuminated command, a stone sunk into
/// its well, a quiet note in its darker ink.
#[must_use]
pub fn button_toggled_face(
    button_type: crate::widgets::button::ButtonType,
) -> Vec<CssPropertyWithConditions> {
    use super::decl;
    match FloraButtonKind::of(button_type) {
        FloraButtonKind::Quiet => {
            let mut v = CssPropertyWithConditions::themed(
                CssProperty::TextDecoration(StyleTextDecoration::Underline.into()),
                CssProperty::TextDecoration(StyleTextDecoration::Underline.into()),
            )
            .to_vec();
            v.extend(decl::themed_ink(LIGHT_QT2, DARK_QT2));
            v
        }
        FloraButtonKind::Stone(stone) => alloc::vec![
            CssPropertyWithConditions::simple(layers(sunken_stone(stone))),
            CssPropertyWithConditions::simple(shadow_in(
                ShadowSlot::Top,
                2,
                5,
                0,
                ColorU::new(0, 0, 0, 115),
                true,
            )),
            CssPropertyWithConditions::simple(no_shadow_in(ShadowSlot::Right)),
            CssPropertyWithConditions::simple(no_shadow_in(ShadowSlot::Bottom)),
        ],
        FloraButtonKind::Standard | FloraButtonKind::Illuminated => {
            let mut v = decl::themed_layers(
                alloc::vec![PRESSED_FACE_LIGHT],
                alloc::vec![PRESSED_FACE_DARK],
            )
            .to_vec();
            v.extend(themed_shadow_in(
                ShadowSlot::Top,
                (1, 3, 0),
                ColorU::new(48, 45, 38, 46),
                ColorU::new(0, 0, 0, 115),
                true,
            ));
            v.push(CssPropertyWithConditions::simple(no_shadow_in(ShadowSlot::Right)));
            v.push(CssPropertyWithConditions::simple(no_shadow_in(ShadowSlot::Bottom)));
            v
        }
    }
}

// ==== rich_text_editor ====
//
// A flora rich-text editor is the leaf (--fl-sur, --fl-ink) in a --fl-bd
// hairline frame, under a toolbar strip (--fl-strip) closed by a hairline.
// The document on the leaf is the user's content: the mode's system
// colours, the same in every theme.

/// Flora's rich-text editor look.
#[must_use]
pub(crate) fn rich_text_editor_look() -> crate::widgets::rich_text_editor::RichTextEditorLook {
    use super::decl;
    let mut frame = decl::border(1).to_vec();
    frame.extend(decl::themed_border_color(LIGHT_BD, DARK_BD));
    let mut toolbar = decl::padding(2, 4, 2, 4).to_vec();
    toolbar.extend(flora_strip_below());
    crate::widgets::rich_text_editor::RichTextEditorLook {
        frame,
        toolbar,
        page: flora_leaf(),
    }
}

/// The flora rich-text editor's chrome (frame, toolbar strip, page).
#[must_use]
pub fn rich_text_editor(chrome: crate::widgets::rich_text_editor::RichTextEditorChrome) -> Dom {
    crate::widgets::rich_text_editor::build_chrome(chrome, &rich_text_editor_look())
}

// ==== date_repeat_picker ====
//
// A flora date repeat picker is the same form on flora's paper: rows a
// little further apart, the label column in the `.fl-label` tone, the
// units in it too, the controls flora's own. At night the night inks.

/// Flora's date-repeat-picker look.
#[must_use]
pub(crate) fn date_repeat_picker_look() -> crate::widgets::date_repeat_picker::DateRepeatPickerLook {
    use super::decl;

    let gap = |px: isize| {
        CssPropertyWithConditions::simple(CssProperty::ColumnGap(LayoutColumnGapValue::Exact(
            LayoutColumnGap {
                inner: PixelValue::const_px(px),
            },
        )))
    };
    let mut editor = vec![
        decl::font_size(13),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ];
    editor.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    let mut row = decl::margin(5, 0, 5, 0).to_vec();
    row.push(gap(8));
    let mut label = vec![decl::px_width(68.0)];
    label.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    let unit = decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1).to_vec();

    crate::widgets::date_repeat_picker::DateRepeatPickerLook {
        editor,
        row,
        label,
        unit,
        number: vec![decl::px_width(64.0)],
        weekdays: vec![gap(4)],
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora date repeat picker.
#[must_use]
pub fn date_repeat_picker(e: crate::widgets::date_repeat_picker::DateRepeatPicker) -> Dom {
    crate::widgets::date_repeat_picker::build(e, &date_repeat_picker_look())
}

// ==== data_table ====
//
// A flora data table is a ledger on the field paper: rows a --fl-sep
// hairline apart with the surface tone on every other one, the column
// titles semibold on the strip (a sorted column's title in the accent, its
// glow at night), the filter row in the field tone with the label ink for
// its placeholder, selected rows on the accent's soft wash (the accent
// itself at night), the cursor's cell in the accent outline, scroll bars of
// the quiet ink on the surface. At night every surface and ink takes its
// night value.

/// Flora's data-table look.
#[must_use]
pub(crate) fn data_table_look() -> crate::widgets::data_table::DataTableLook {
    use super::decl;

    let mut table = alloc::vec![CssPropertyWithConditions::simple(
        CssProperty::const_font_family(SYSTEM_UI_FAMILY)
    )];
    table.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    table.extend(decl::themed_fill(LIGHT_FLD, DARK_FLD));

    let mut header = alloc::vec![decl::semibold()];
    header.extend(decl::themed_fill(LIGHT_STRIP, DARK_STRIP));
    header.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    header.extend(decl::border_right(1));
    header.extend(decl::themed_border_right_color(LIGHT_BD, DARK_BD));
    header.extend(decl::border_bottom(1));
    header.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));

    let mut filter = decl::themed_fill(LIGHT_SUR, DARK_SUR).to_vec();
    filter.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    filter.extend(decl::border_right(1));
    filter.extend(decl::themed_border_right_color(LIGHT_SEP, DARK_SEP));
    filter.extend(decl::border_bottom(1));
    filter.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));

    let mut cell = decl::border_right(1).to_vec();
    cell.extend(decl::themed_border_right_color(LIGHT_SEP, DARK_SEP));
    let mut row = decl::border_bottom(1).to_vec();
    row.extend(decl::themed_border_bottom_color(LIGHT_SEP, DARK_SEP));

    let mut cursor = decl::border(2).to_vec();
    cursor.extend(decl::themed_border_color(LIGHT_ACC, DARK_GLOW));
    let mut editor = decl::themed_fill(LIGHT_FLD, DARK_FLD).to_vec();
    editor.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    editor.extend(decl::border(2));
    editor.extend(decl::themed_border_color(LIGHT_ACC, DARK_GLOW));
    editor.extend(decl::padding(0, 5, 0, 5));

    let mut thumb = decl::themed_fill(LIGHT_SOFT2, DARK_SOFT2).to_vec();
    thumb.extend(decl::radius(4));
    let mut notice = decl::themed_fill(LIGHT_FLD, DARK_FLD).to_vec();
    notice.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    notice.extend(decl::padding(2, 6, 2, 6));
    notice.extend(decl::border(1));
    notice.extend(decl::themed_border_color(LIGHT_BD, DARK_BD));
    notice.extend(decl::radius(3));

    crate::widgets::data_table::DataTableLook {
        table,
        header,
        header_sorted: decl::themed_ink(LIGHT_ACC, DARK_GLOW).to_vec(),
        filter,
        filter_empty: decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1).to_vec(),
        cell,
        row,
        row_alternate: decl::themed_fill(LIGHT_SUR, DARK_SUR).to_vec(),
        row_selected: decl::themed_fill(LIGHT_SOFT, DARK_ACC).to_vec(),
        cursor,
        editor,
        caret: decl::themed_fill(LIGHT_INK, DARK_INK).to_vec(),
        freeze_line: decl::themed_fill(LIGHT_BD3, DARK_BD3).to_vec(),
        track: decl::themed_fill(LIGHT_SUR, DARK_SUR).to_vec(),
        thumb,
        notice,
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora data table.
#[must_use]
pub(crate) fn data_table(t: crate::widgets::data_table::DataTableResolved) -> Dom {
    crate::widgets::data_table::build(t, &data_table_look())
}

// ==== list_view ====
//
// A flora list is a ledger on the field paper (`--fl-fld`, the night field
// by night): the rows in the ink, every other one on the surface tone (the
// data table's band), the cells a faint `--fl-sep2` rule apart. Its header
// is the raised paper face (`--fl-rT` -> `--fl-rB`) closed by a `--fl-bd`
// hairline; the column titles are flora's chrome - EB Garamond capitals in
// `--fl-intro`, a `--fl-sep` separator between two - the sorted one's title
// and arrow in the accent (its glow by night). A title lifts to the hover
// face with a brass rule under it (the metal edge comes up; the night gold
// after dark) and sinks to the pressed face. A row washes to the accent's
// soft tint under the pointer; the selected row is flora.css's `::selection`
// - the accent's soft wash written in its deep tone by day, the accent stone
// written in the paper ink by night - and the row the keyboard is on is
// ringed in the accent (its glow by night). Every accent colour is the base
// ramp's, so a spin (`flora:green`, ...) recuts the list with the rest of
// flora.

/// A row under the pointer: the accent's soft wash at 60 % by day (flora's
/// `--fl-hov` strength), the accent at 35 % by night - translucent over the
/// paper or a stripe, and recut by a spin, which keeps the alpha.
const LIST_ROW_HOVER_LIGHT: ColorU = ColorU {
    a: 153,
    ..LIGHT_SOFT
};
/// See [`LIST_ROW_HOVER_LIGHT`].
const LIST_ROW_HOVER_DARK: ColorU = ColorU { a: 90, ..DARK_ACC };

/// Flora's list-view look (`list_view::ListViewLook`): the skin of every
/// part, laid over the list's base.
#[must_use]
pub(crate) fn list_view_look() -> crate::widgets::list_view::ListViewLook {
    use super::decl;
    type P = CssPropertyWithConditions;
    let ui = || P::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY));

    // The list: field paper, in the ink (the caller's cells inherit it).
    let mut list = decl::themed_fill(LIGHT_FLD, DARK_FLD).to_vec();
    list.extend(decl::themed_ink(LIGHT_INK, DARK_INK));

    // The header: the raised paper face, closed by a hairline.
    let mut header = vec![decl::px_height(24.0)];
    header.extend(decl::themed_layers(
        vec![RAISED_FACE_LIGHT],
        vec![RAISED_FACE_DARK],
    ));
    header.extend(decl::border_bottom(1));
    header.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));

    // A column title's box: a separator on its right, a transparent foot the
    // brass rule colours under the pointer. States last: a resting dark twin
    // matches in every state.
    let mut column = decl::padding(0, 0, 0, 7).to_vec();
    column.extend(decl::border_right(1));
    column.extend(decl::themed_border_right_color(LIGHT_SEP, DARK_SEP));
    column.extend(decl::border_bottom(1));
    column.extend(decl::themed_border_bottom_color(
        ColorU::TRANSPARENT,
        ColorU::TRANSPARENT,
    ));
    column.extend(decl::hover_layers(vec![HOVER_FACE_LIGHT], vec![HOVER_FACE_DARK]));
    column.extend(P::themed_on_hover(
        CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: TAB_METAL }),
        CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: DARK_QT }),
    ));
    column.extend(decl::active_layers(
        vec![PRESSED_FACE_LIGHT],
        vec![PRESSED_FACE_DARK],
    ));

    // A title: flora's chrome capitals.
    let mut title = caps(CAPS_LABEL);
    title.extend(decl::themed_ink(LIGHT_INTRO, DARK_INTRO));
    let mut sort_arrow = vec![decl::font_size(14)];
    sort_arrow.extend(decl::themed_ink(LIGHT_ACC, DARK_GLOW));

    // A row: its ring slot (a transparent hairline the focus ring colours);
    // the accent's wash under the pointer; the accent's ring on focus.
    let mut row = decl::padding(3, 0, 3, 0).to_vec();
    row.extend(decl::ring_slot());
    row.extend(decl::hover_fill(LIST_ROW_HOVER_LIGHT, LIST_ROW_HOVER_DARK));
    row.extend(decl::focus_ring(LIGHT_ACC, DARK_GLOW));

    // A stripe: the surface tone - and the hover wash again after it, or the
    // resting band would beat the hover.
    let mut row_alternate = decl::themed_fill(LIGHT_SUR, DARK_SUR).to_vec();
    row_alternate.extend(decl::hover_fill(LIST_ROW_HOVER_LIGHT, LIST_ROW_HOVER_DARK));

    // The selection (flora.css's `::selection`): the soft wash in the deep
    // tone by day, the stone in the paper ink by night - the same under the
    // pointer.
    let mut row_selected = decl::themed_fill(LIGHT_SOFT, DARK_ACC).to_vec();
    row_selected.extend(decl::themed_ink(LIGHT_DEEP, DARK_ON_ACC));
    row_selected.extend(decl::hover_fill(LIGHT_SOFT, DARK_ACC));

    // A cell: the title's padding and a faint ledger rule on its right (as
    // wide as its column's title box, which has its separator).
    let mut cell = decl::padding(0, 0, 0, 7).to_vec();
    cell.extend([decl::font_size(12), ui()]);
    cell.extend(decl::border_right(1));
    cell.extend(decl::themed_border_right_color(LIGHT_SEP2, DARK_SEP2));

    crate::widgets::list_view::ListViewLook {
        list,
        header,
        column,
        title,
        title_sorted: decl::themed_ink(LIGHT_ACC, DARK_GLOW).to_vec(),
        sort_arrow,
        row,
        row_alternate,
        row_selected,
        cell,
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

// ==== chart ====
//
// The flora chart is a leaf laid on the page (flora's surface, its night
// surface in the dark) with flora's small corner, in the UI face at 12 px:
// the title semibold in the ink, tick labels in the soft ink, legend names
// and axis titles in the intro ink. Gridlines are the faint separator, the
// baseline and the crosshair the strong border. The series wear the chart's
// categorical palette (`chart::CHART_PALETTE`: flora's accent stones are
// too dark and grey to tell series apart - they fail the colour-blind
// checks); the selection and the focus ring wear the accent stone by day
// and its glow at night. The tooltip is the tooltip widget's own.

/// Flora's chart skin.
#[must_use]
pub(crate) fn chart_skin() -> crate::widgets::chart::ChartSkin {
    use super::decl;
    use crate::widgets::chart::{ChartColor, ChartSkin, CHART_PALETTE};

    let mut root = decl::themed_fill(LIGHT_SUR, DARK_SUR).to_vec();
    root.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    root.push(decl::font_size(12));
    root.push(CssPropertyWithConditions::simple(CssProperty::const_font_family(
        SYSTEM_UI_FAMILY,
    )));
    root.extend(decl::radius(5));

    let mut title = vec![decl::font_size(14), decl::semibold()];
    title.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    let mut tick = vec![decl::font_size(11)];
    tick.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    let mut caption = vec![decl::font_size(12)];
    caption.extend(decl::themed_ink(LIGHT_INTRO, DARK_INTRO));

    let mut table_head = decl::padding(4, 8, 4, 8).to_vec();
    table_head.push(decl::semibold());
    table_head.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    table_head.extend(decl::border_bottom(1));
    table_head.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));
    let mut table_cell = decl::padding(3, 8, 3, 8).to_vec();
    table_cell.extend(decl::themed_ink(LIGHT_INTRO, DARK_INTRO));

    ChartSkin {
        root,
        title,
        tick,
        caption,
        tip: tooltip_skin().tip.as_slice().to_vec(),
        table_head,
        table_cell,
        surface: ChartColor::create(LIGHT_SUR, DARK_SUR),
        grid: ChartColor::create(LIGHT_SEP2, DARK_SEP2),
        axis: ChartColor::create(LIGHT_BD3, DARK_BD3),
        crosshair: ChartColor::create(LIGHT_BD3, DARK_BD3),
        accent: ChartColor::create(LIGHT_ACC, DARK_GLOW),
        palette: CHART_PALETTE,
        marker: Some("__azul-theme-flora"),
    }
}

// ==== toolbar ====
//
// A flora toolbar is flora's toolbar strip (`--fl-strip`, closed along its
// foot by a `--fl-bd` rule) whose tools are flora's toolbar keys
// (`chrome_key`, the ribbon's commands): bare paper at rest, the hover face
// in a hairline under the pointer, the pressed face while held, ringed on
// focus; a toggle that is on stays pushed in, in a `--fl-bd3` hairline (the
// ribbon's checked key). Icons in the house icon ink, a menu button's arrow
// in soft ink, separators the `--fl-sep` hairline. At night every surface
// and ink takes its night value.

/// Flora's toolbar look.
#[must_use]
pub(crate) fn toolbar_look() -> crate::widgets::toolbar::ToolbarLook {
    use super::decl;

    let font = CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY));
    let mut bar = vec![font.clone()];
    bar.extend(decl::themed_fill(LIGHT_STRIP, DARK_STRIP));
    bar.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    bar.extend(decl::padding(2, 4, 2, 4));
    bar.extend(decl::border_bottom(1));
    bar.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));

    let mut item = decl::themed_ink(LIGHT_INK, DARK_INK).to_vec();
    chrome_key(&mut item);

    let mut item_pressed =
        decl::themed_layers(vec![PRESSED_FACE_LIGHT], vec![PRESSED_FACE_DARK]).to_vec();
    item_pressed.extend(decl::themed_border_color(LIGHT_BD3, DARK_BD3));
    chrome_key_states(&mut item_pressed);

    crate::widgets::toolbar::ToolbarLook {
        bar,
        item,
        item_pressed,
        label: vec![font],
        icon: decl::themed_ink(LIGHT_ICON, DARK_ICON).to_vec(),
        arrow: decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1).to_vec(),
        separator: decl::themed_fill(LIGHT_SEP, DARK_SEP).to_vec(),
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

// ==== token_input ====
//
// A flora token input is a flora field holding the chips: field paper
// (`--fl-fld`) in a `--fl-bd2` hairline with the house radius, the
// surface at night (the entry inside it is the text input, which takes the
// surface at night too), the hairline deepening to `--fl-bd3` under the
// pointer. The entry rings itself on focus in the accent (its glow at
// night): the field cannot until the engine raises `:focus-within`. The
// suggestions are a floating leaf (`chrome_leaf`); a suggestion lifts to
// the hover face under the pointer, the highlighted one rests on the
// accent's soft wash (the accent at night), as a data table's selected row.

/// Flora's token-input look.
#[must_use]
pub(crate) fn token_input_look() -> crate::widgets::token_input::TokenInputLook {
    use super::decl;

    let mut root = vec![CssPropertyWithConditions::simple(CssProperty::const_font_family(
        SYSTEM_UI_FAMILY,
    ))];
    root.extend(decl::themed_ink(LIGHT_INK, DARK_INK));

    let mut field = decl::themed_fill(LIGHT_FLD, DARK_SUR).to_vec();
    field.extend(decl::border(1));
    field.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD));
    field.extend(decl::radius(3));
    field.extend(decl::padding(2, 4, 2, 4));
    field.extend(decl::hover_border_color(LIGHT_BD3, DARK_BD3));

    let mut list = decl::border(1).to_vec();
    chrome_leaf(&mut list);
    list.extend(decl::padding(2, 0, 2, 0));

    let mut option = decl::padding(4, 8, 4, 8).to_vec();
    option.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    option.extend(decl::hover_fill(LIGHT_HT, DARK_HT));

    let mut option_active = decl::themed_fill(LIGHT_SOFT, DARK_ACC).to_vec();
    option_active.extend(decl::hover_fill(LIGHT_SOFT, DARK_ACC));

    crate::widgets::token_input::TokenInputLook {
        root,
        field,
        entry: decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW).to_vec(),
        list,
        option,
        option_active,
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

// ==== icon_grid ====
//
// A flora icon grid is laid on field paper (`--fl-fld`): glyphs in the
// house icon ink, labels in the ink under them; an item lifts to the hover
// face under the pointer, a selected one rests on the accent's soft wash in
// an accent hairline (the accent and its glow at night, as a data table's
// selected row), the focused one is outlined in the accent; the rubber band
// is the selection adorner's marquee; the scroll bar is the data table's.
// The grid rings itself inside in the accent when it has the focus.

/// Flora's icon-grid look.
#[must_use]
pub(crate) fn icon_grid_look() -> crate::widgets::icon_grid::IconGridLook {
    use super::decl;

    let mut grid = vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
        decl::font_size(12),
    ];
    grid.extend(decl::themed_fill(LIGHT_FLD, DARK_FLD));
    grid.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    grid.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));

    let mut item = decl::padding(4, 2, 2, 2).to_vec();
    item.extend(decl::radius(3));
    item.push(CssPropertyWithConditions::simple(decl::fill(ColorU::TRANSPARENT)));
    item.extend(decl::border_colors(ColorU::TRANSPARENT).map(CssPropertyWithConditions::simple));
    item.extend(decl::hover_fill(LIGHT_HT, DARK_HT));

    let mut item_selected = decl::themed_fill(LIGHT_SOFT, DARK_ACC).to_vec();
    item_selected.extend(decl::themed_border_color(LIGHT_ACC, DARK_GLOW));
    // The hover face again after the resting one, so it is not shadowed.
    item_selected.extend(decl::hover_fill(LIGHT_SOFT, DARK_ACC));

    let mut label = decl::margin(2, 0, 0, 0).to_vec();
    label.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    let mut badge = vec![decl::font_size(16)];
    badge.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    let mut thumb = decl::themed_fill(LIGHT_SOFT2, DARK_SOFT2).to_vec();
    thumb.extend(decl::radius(4));

    crate::widgets::icon_grid::IconGridLook {
        grid,
        item,
        item_selected,
        item_focused: decl::themed_border_color(LIGHT_ACC, DARK_GLOW),
        icon: decl::themed_ink(LIGHT_ICON, DARK_ICON).to_vec(),
        label,
        badge,
        marquee: selection_adorner_look().marquee,
        track: decl::themed_fill(LIGHT_SUR, DARK_SUR).to_vec(),
        thumb,
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

// ==== money_input ====
//
// A flora money input is flora's field with the currency code on a raised
// paper tab beside it: the raised face (--fl-rT -> --fl-rB) in the --fl-bd2
// hairline at the house radius, the code semibold and tracked out in
// --fl-soft1 like a pill's label; every surface, edge and ink has its night
// value. The field itself is the TextInput's own flora look.

/// Flora's money-input skin.
#[must_use]
pub(crate) fn money_input_skin() -> crate::widgets::money_input::MoneyInputSkin {
    use super::decl;

    let root = vec![CssPropertyWithConditions::simple(
        CssProperty::const_font_family(SYSTEM_UI_FAMILY),
    )];
    let mut addon = decl::padding(0, 8, 0, 8).to_vec();
    addon.push(decl::font_size(12));
    addon.push(decl::semibold());
    addon.push(decl::letter_spacing_em(0.06));
    addon.extend(decl::themed_layers(
        vec![decl::face(LIGHT_RT, LIGHT_RB)],
        vec![decl::face(DARK_RT, DARK_RB)],
    ));
    addon.extend(decl::themed_border(decl::Edges::ALL, 1, LIGHT_BD2, DARK_BD2));
    addon.extend(decl::radius(3));
    addon.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    crate::widgets::money_input::MoneyInputSkin {
        root,
        addon,
        marker: Some("__azul-theme-flora"),
    }
}

// ==== gauge ====
//
// A flora gauge cuts its value from the semantic stones the badges and chips
// wear - leaf for ok, amber for a warning, clay for critical, slate for a
// plain range - each by day as the stone and at night as its glow, on the
// groove of flora's track; outside every band the value is the accent stone
// (its glow at night). The value is semibold in --fl-ink, the label in
// --fl-intro.

/// Flora's gauge skin.
#[must_use]
pub(crate) fn gauge_skin() -> crate::widgets::gauge::GaugeSkin {
    use super::decl;
    use crate::widgets::chart::ChartColor;

    let stone = |s: FloraStone| ChartColor::create(s.stone, s.glow);
    let mut root = vec![CssPropertyWithConditions::simple(
        CssProperty::const_font_family(SYSTEM_UI_FAMILY),
    )];
    root.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    let mut value_text = vec![decl::semibold()];
    value_text.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    let label = decl::themed_ink(LIGHT_INTRO, DARK_INTRO).to_vec();
    crate::widgets::gauge::GaugeSkin {
        root,
        value_text,
        label,
        track: ChartColor::create(LIGHT_TRACK, DARK_TRACK),
        ok: stone(STONE_LEAF),
        warn: stone(STONE_AMBER),
        bad: stone(STONE_CLAY),
        neutral: stone(STONE_SLATE),
        accent: ChartColor::create(LIGHT_ACC, DARK_GLOW),
        marker: Some("__azul-theme-flora"),
    }
}

// ==== date_range_picker ====
//
// A flora date range picker's two calendars are the flora date picker's
// leaves (`date_picker_look`); beside them the presets stand as a column of
// quiet rows behind a --fl-bd2 hairline - --fl-ink, lifting to the hover
// face under the pointer, ringed in the accent on focus - and the summary
// line is written in --fl-intro.

/// Flora's date-range-picker skin.
#[must_use]
pub(crate) fn date_range_picker_skin(
) -> crate::widgets::date_range_picker::DateRangePickerSkin {
    use super::decl;

    let root = vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
        decl::font_size(13),
    ];
    let right = decl::Edges {
        top: false,
        right: true,
        bottom: false,
        left: false,
    };
    let mut presets = decl::padding(0, 12, 0, 0).to_vec();
    presets.extend(decl::themed_border(right, 1, LIGHT_BD2, DARK_BD2));
    presets.push(decl::px_min_width(120.0));
    let mut preset = decl::padding(4, 8, 4, 8).to_vec();
    preset.extend(decl::radius(3));
    preset.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    preset.extend(decl::hover_layers(
        vec![HOVER_FACE_LIGHT],
        vec![HOVER_FACE_DARK],
    ));
    preset.extend(decl::focus_halo(LIGHT_ACC, DARK_GLOW));
    let mut summary = vec![decl::font_size(12)];
    summary.extend(decl::themed_ink(LIGHT_INTRO, DARK_INTRO));
    crate::widgets::date_range_picker::DateRangePickerSkin {
        root,
        presets,
        preset,
        summary,
    }
}

// ==== terminal_view ====
//
// Flora's terminal is "Flora ink": the code panel's warm ink ground in BOTH
// modes (a terminal is a dark room in a light window too) - `#211F1B` under
// `#E4E1D6` by day, the neutral dark room `#141414` under `#E2E2E2` at
// night, the code panel's dim `#928D80` / `#858585` as bright black. The
// ANSI colours come from flora's accent families, lifted to read on the ink:
// clay for red, leaf for green, slate for blue, plum for magenta, an ochre
// and a sea-green between them. The cursor is the paper ink, the selection a
// translucent wash of it.

/// Flora's terminal palette (`TerminalPalette::flora_ink`).
#[must_use]
pub(crate) const fn terminal_palette() -> crate::widgets::terminal_view::TerminalPalette {
    use azul_css::props::basic::color::ColorU as U;
    use crate::widgets::chart::ChartColor as C;
    const fn pair(light: u32, dark: u32) -> C {
        C::create(
            U::rgb((light >> 16) as u8, (light >> 8) as u8, light as u8),
            U::rgb((dark >> 16) as u8, (dark >> 8) as u8, dark as u8),
        )
    }
    crate::widgets::terminal_view::TerminalPalette {
        black: pair(0x3A_362F, 0x2A_2A2A),
        red: pair(0xC4_7B6E, 0xC8_7E72),
        green: pair(0x8F_B08C, 0x8C_B08F),
        yellow: pair(0xD2_B06A, 0xD0_B26E),
        blue: pair(0x7F_9CB8, 0x82_9FBA),
        magenta: pair(0xA8_93BD, 0xAA_96BE),
        cyan: pair(0x7F_AFA8, 0x80_B0AA),
        white: pair(0xCF_CABC, 0xCF_CFCF),
        bright_black: pair(0x92_8D80, 0x85_8585),
        bright_red: pair(0xDB_9488, 0xDD_978B),
        bright_green: pair(0xA9_C9A5, 0xA8_C9AB),
        bright_yellow: pair(0xE6_C985, 0xE4_CA88),
        bright_blue: pair(0x9D_B6CF, 0x9F_B8D0),
        bright_magenta: pair(0xC0_AED3, 0xC2_B0D4),
        bright_cyan: pair(0x9C_C8C1, 0x9D_C9C3),
        bright_white: pair(0xF1_EEE6, 0xF2_F2F2),
        foreground: pair(0xE4_E1D6, 0xE2_E2E2),
        background: pair(0x21_1F1B, 0x14_1414),
        cursor: pair(0xE4_E1D6, 0xE2_E2E2),
        selection: C::create(U::rgba(0xE4, 0xE1, 0xD6, 0x48), U::rgba(0xE2, 0xE2, 0xE2, 0x40)),
    }
}

// ==== level_meter ====
//
// The flora level meter sits in flora's stone: the trough is flora's track
// with its 4 px corners, the level a sage green, ochre past -18 dB, a brick
// red past -6 dB - flora's earth tones, saturated enough to read at a
// glance. At night the trough is the dark track and the colours lift.

const LEVEL_OK_LIGHT: ColorU = ColorU::new(0x4F, 0x8A, 0x45, 255);
const LEVEL_OK_DARK: ColorU = ColorU::new(0x7F, 0xB8, 0x6F, 255);
const LEVEL_WARM_LIGHT: ColorU = ColorU::new(0xC2, 0x8A, 0x1E, 255);
const LEVEL_WARM_DARK: ColorU = ColorU::new(0xE0, 0xB0, 0x50, 255);
const LEVEL_HOT_LIGHT: ColorU = ColorU::new(0xB0, 0x40, 0x32, 255);
const LEVEL_HOT_DARK: ColorU = ColorU::new(0xD9, 0x70, 0x5F, 255);

/// Flora's level-meter look.
#[must_use]
pub(crate) fn level_meter_look() -> crate::widgets::level_meter::LevelMeterLook {
    use super::decl;
    let mut track = decl::themed_fill(LIGHT_TRACK, DARK_TRACK).to_vec();
    track.extend(decl::radius(4));
    crate::widgets::level_meter::LevelMeterLook {
        track,
        ok: decl::themed_fill(LEVEL_OK_LIGHT, LEVEL_OK_DARK).to_vec(),
        warm: decl::themed_fill(LEVEL_WARM_LIGHT, LEVEL_WARM_DARK).to_vec(),
        hot: decl::themed_fill(LEVEL_HOT_LIGHT, LEVEL_HOT_DARK).to_vec(),
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora level meter.
#[must_use]
pub fn level_meter(m: crate::widgets::level_meter::LevelMeter) -> Dom {
    crate::widgets::level_meter::build(m, &level_meter_look())
}

// ==== seek_bar ====
//
// The flora seek bar: the times in the UI face at 12 px in flora's soft ink,
// flora's track with its 4 px corners, the loaded part in the border stone,
// the played part in the accent stone (its glow at night), chapter ticks cut
// in the surface, a round thumb in the accent; the trough wears flora's focus
// halo.

/// Flora's seek-bar look.
#[must_use]
pub(crate) fn seek_bar_look() -> crate::widgets::seek_bar::SeekBarLook {
    use super::decl;
    let mut time = vec![
        decl::font_size(12),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ];
    time.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    let mut track = decl::themed_fill(LIGHT_TRACK, DARK_TRACK).to_vec();
    track.extend(decl::radius(4));
    track.extend(decl::margin(0, 8, 0, 8));
    track.extend(decl::focus_halo(LIGHT_GLOW, DARK_GLOW));
    let mut buffered = decl::themed_fill(LIGHT_BD, DARK_BD).to_vec();
    buffered.extend(decl::radius(4));
    let mut played = decl::themed_fill(LIGHT_ACC, DARK_GLOW).to_vec();
    played.extend(decl::radius(4));
    let mut thumb = decl::themed_fill(LIGHT_ACC, DARK_GLOW).to_vec();
    thumb.extend(decl::radius(6));
    crate::widgets::seek_bar::SeekBarLook {
        time,
        track,
        buffered,
        played,
        tick: decl::themed_fill(LIGHT_SUR, DARK_SUR).to_vec(),
        thumb,
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora seek bar.
#[must_use]
pub fn seek_bar(b: crate::widgets::seek_bar::SeekBar) -> Dom {
    crate::widgets::seek_bar::build(b, &seek_bar_look())
}

// ==== media_controls ====
//
// The flora transport: flora's link-style icon buttons and its primary
// play button, a little more air between them than flat's, the volume slider
// set off by a gap. The buttons and the slider carry the theme.

/// Flora's media-controls look.
#[must_use]
pub(crate) fn media_controls_look() -> crate::widgets::media_controls::MediaControlsLook {
    use super::decl;
    crate::widgets::media_controls::MediaControlsLook {
        row: decl::padding(4, 6, 4, 6).to_vec(),
        volume: decl::margin(0, 0, 0, 16).to_vec(),
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora media controls.
#[must_use]
pub fn media_controls(c: crate::widgets::media_controls::MediaControls) -> Dom {
    crate::widgets::media_controls::build(c, &media_controls_look())
}

// ==== waveform ====
//
// The flora waveform: stone bars (the strong border) on flora's surface with
// its 4 px corners, the played part in the accent stone (its glow at night),
// the playhead in the ink; the surface wears flora's focus halo.

/// Flora's waveform look.
#[must_use]
pub(crate) fn waveform_look() -> crate::widgets::waveform::WaveformLook {
    use super::decl;
    let mut root = decl::themed_fill(LIGHT_SUR, DARK_SUR).to_vec();
    root.extend(decl::radius(4));
    root.extend(decl::padding(4, 4, 4, 4));
    root.extend(decl::focus_halo(LIGHT_GLOW, DARK_GLOW));
    let mut bar = decl::themed_fill(LIGHT_BD3, DARK_BD3).to_vec();
    bar.extend(decl::margin(0, 1, 0, 0));
    let mut played = decl::themed_fill(LIGHT_ACC, DARK_GLOW).to_vec();
    played.extend(decl::margin(0, 1, 0, 0));
    crate::widgets::waveform::WaveformLook {
        root,
        bar,
        played,
        head: decl::themed_fill(LIGHT_INK, DARK_INK).to_vec(),
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora waveform.
#[must_use]
pub fn waveform(w: crate::widgets::waveform::Waveform) -> Dom {
    crate::widgets::waveform::build(w, &waveform_look())
}

// ==== code_view ====
//
// Flora sets code as an INK PANEL in both modes (planning: the flora
// code-panel tokens `--fl-code-bg / -fg / -bd` are the default editor
// theme): `--fl-code-bg` under `--fl-code-fg` by day and by night, the line
// numbers in the panel's dim ink behind a `--fl-code-bd` hairline, the
// caret's line a step lighter than the panel, selections a warm wash, the
// token inks warm stones and moss that each read at least 4.5:1 on the
// panel (keywords rust, strings olive, comments stone, types moss).

/// `--fl-code-bg` by day / by night.
const CODE_VIEW_BG: (ColorU, ColorU) = (ColorU::new(33, 31, 27, 255), ColorU::new(20, 20, 20, 255));
/// `--fl-code-fg` by day / by night.
const CODE_VIEW_FG: (ColorU, ColorU) =
    (ColorU::new(228, 225, 214, 255), ColorU::new(226, 226, 226, 255));
/// `--fl-code-bd` by day / by night.
const CODE_VIEW_BD: (ColorU, ColorU) = (ColorU::new(68, 63, 53, 255), ColorU::new(54, 54, 54, 255));
/// The panel's dim ink (line numbers, comments) by day / by night.
const CODE_VIEW_DIM: (ColorU, ColorU) =
    (ColorU::new(146, 139, 124, 255), ColorU::new(128, 128, 128, 255));

/// The (day, night) ink of every `CodeTokenKind`, in declaration order.
const CODE_VIEW_INKS: [(ColorU, ColorU); crate::widgets::code_view::CODE_TOKEN_KINDS] = [
    // Plain
    CODE_VIEW_FG,
    // Keyword
    (ColorU::new(224, 149, 106, 255), ColorU::new(230, 155, 112, 255)),
    // Type
    (ColorU::new(143, 193, 169, 255), ColorU::new(143, 193, 169, 255)),
    // Function
    (ColorU::new(230, 200, 138, 255), ColorU::new(230, 200, 138, 255)),
    // StringLiteral
    (ColorU::new(185, 204, 122, 255), ColorU::new(185, 204, 122, 255)),
    // Number
    (ColorU::new(211, 155, 196, 255), ColorU::new(211, 155, 196, 255)),
    // Comment
    CODE_VIEW_DIM,
    // Constant
    (ColorU::new(143, 188, 212, 255), ColorU::new(143, 188, 212, 255)),
    // Macro
    (ColorU::new(211, 155, 196, 255), ColorU::new(211, 155, 196, 255)),
    // Attribute
    (ColorU::new(169, 184, 198, 255), ColorU::new(169, 184, 198, 255)),
    // Operator
    (ColorU::new(216, 212, 200, 255), ColorU::new(216, 216, 216, 255)),
    // Punctuation
    (ColorU::new(181, 175, 162, 255), ColorU::new(180, 180, 180, 255)),
    // Variable
    (ColorU::new(226, 213, 190, 255), ColorU::new(226, 213, 190, 255)),
    // Tag
    (ColorU::new(224, 149, 106, 255), ColorU::new(230, 155, 112, 255)),
    // Heading
    (ColorU::new(224, 149, 106, 255), ColorU::new(230, 155, 112, 255)),
    // Link
    (ColorU::new(143, 188, 212, 255), ColorU::new(143, 188, 212, 255)),
    // Invalid
    (ColorU::new(242, 139, 130, 255), ColorU::new(242, 139, 130, 255)),
];

/// Flora's code-view look.
#[must_use]
pub(crate) fn code_view_look() -> crate::widgets::code_view::CodeViewLook {
    use super::decl;

    let mut view = decl::themed_fill(CODE_VIEW_BG.0, CODE_VIEW_BG.1).to_vec();
    view.extend(decl::themed_ink(CODE_VIEW_FG.0, CODE_VIEW_FG.1));

    let mut gutter = decl::themed_ink(CODE_VIEW_DIM.0, CODE_VIEW_DIM.1).to_vec();
    gutter.extend(decl::border_right(1));
    gutter.extend(decl::themed_border_right_color(CODE_VIEW_BD.0, CODE_VIEW_BD.1));

    let mut thumb = decl::themed_fill(ColorU::new(96, 89, 76, 255), ColorU::new(84, 84, 84, 255)).to_vec();
    thumb.extend(decl::radius(4));

    crate::widgets::code_view::CodeViewLook {
        view,
        gutter,
        gutter_current: decl::themed_ink(CODE_VIEW_FG.0, CODE_VIEW_FG.1).to_vec(),
        current_line: decl::themed_fill(ColorU::new(45, 42, 37, 255), ColorU::new(32, 32, 32, 255)).to_vec(),
        selection: decl::themed_fill(ColorU::new(86, 76, 58, 255), ColorU::new(66, 66, 76, 255)).to_vec(),
        caret: decl::themed_fill(CODE_VIEW_FG.0, CODE_VIEW_FG.1).to_vec(),
        track: decl::themed_fill(ColorU::new(40, 38, 33, 255), ColorU::new(28, 28, 28, 255)).to_vec(),
        thumb,
        tokens: CODE_VIEW_INKS
            .iter()
            .map(|(day, night)| decl::themed_ink(*day, *night).to_vec())
            .collect(),
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora code view.
#[must_use]
pub(crate) fn code_view(v: crate::widgets::code_view::CodeViewResolved) -> Dom {
    crate::widgets::code_view::build(v, &code_view_look())
}

// ==== icon_grid (item extras) ====
//
// The OPTIONAL extras of a flora icon-grid item (user decision D3,
// 2026-10-05): its extra lines under the label in the soft ink one size
// down (the dialogs' hint ink), and the placeholder tile's corners rounded
// like a thumbnail's - the item gives the tile its colour.

/// Flora's skin for an icon-grid item's extras.
#[must_use]
pub(crate) fn icon_grid_extras_look() -> crate::widgets::icon_grid::IconGridExtrasLook {
    use super::decl;

    let mut line = vec![decl::font_size(11)];
    line.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    crate::widgets::icon_grid::IconGridExtrasLook {
        line,
        placeholder: decl::radius(4).to_vec(),
    }
}

#[cfg(test)]
mod flora16_look_tests {
    //! The flora looks the apps showed wrong (FLORA16's audit): labels and
    //! commands not in flora's capitals, a fill without a night value, a
    //! wash borrowed from flat, token inks that are not flora.css's.

    use azul_css::{
        dynamic_selector::{DynamicSelector, ModeCondition},
        props::{
            basic::color::{ColorOrSystem, ColorU},
            property::{CssProperty, CssPropertyType},
            style::{text::StyleTextTransform, StyleBackgroundContent},
        },
    };

    use super::*;

    /// The last declaration of `ty` in `props` that holds AT REST by day
    /// (`dark` false) or at night: no pseudo-state, no other condition.
    fn at_rest(
        props: &[CssPropertyWithConditions],
        ty: CssPropertyType,
        dark: bool,
    ) -> Option<CssProperty> {
        props
            .iter()
            .filter(|p| {
                p.property.get_type() == ty
                    && p.apply_if.as_ref().iter().all(|c| match c {
                        DynamicSelector::Mode(ModeCondition::Dark) => dark,
                        DynamicSelector::Mode(ModeCondition::Light) => !dark,
                        _ => false,
                    })
            })
            .map(|p| p.property.clone())
            .last()
    }

    fn in_capitals(props: &[CssPropertyWithConditions]) -> bool {
        matches!(
            at_rest(props, CssPropertyType::TextTransform, false),
            Some(CssProperty::TextTransform(v))
                if v.get_property() == Some(&StyleTextTransform::Uppercase)
        )
    }

    fn fill(props: &[CssPropertyWithConditions], dark: bool) -> Option<ColorU> {
        at_rest(props, CssPropertyType::BackgroundContent, dark)
            .and_then(|p| super::super::theme_checks::bg_color(&p))
    }

    fn ink(props: &[CssPropertyWithConditions], dark: bool) -> Option<ColorU> {
        match at_rest(props, CssPropertyType::TextColor, dark)? {
            CssProperty::TextColor(v) => v.get_property().map(|c| c.inner),
            _ => None,
        }
    }

    /// A segment of a flora segmented control is a command: Garamond
    /// capitals, as every flora button and tab (it was 13px mixed case next
    /// to OK / CANCEL).
    #[test]
    fn a_flora_segment_is_set_in_capitals() {
        let segment = segmented_skin().segment;
        for (selected, first, last) in [(false, true, false), (true, false, true)] {
            let v = segment(selected, first, last);
            assert!(in_capitals(v.as_ref()), "selected {selected}");
            assert!(
                matches!(
                    at_rest(v.as_ref(), CssPropertyType::FontFamily, false),
                    Some(CssProperty::FontFamily(f)) if f.get_property() == Some(&FONT_CAPS)
                ),
                "in flora's capitals hand"
            );
        }
    }

    /// The filled part of a flora slider lifts to the stone's glow at night
    /// (flora.css: the accent INK lifts to `--fl-glow` on a dark ground): the
    /// day stone on the night trough read 1.5 - 2:1.
    #[test]
    fn a_flora_slider_fill_lifts_to_the_glow_at_night() {
        let dom = slider_fill();
        let last_stop = |dark: bool| -> Option<ColorU> {
            let p = super::super::theme_checks::background(&dom, dark)?;
            match super::super::theme_checks::bg_layers(&p).first()? {
                StyleBackgroundContent::LinearGradient(g) => {
                    match g.stops.as_ref().last()?.color {
                        ColorOrSystem::Color(c) => Some(c),
                        _ => None,
                    }
                }
                _ => None,
            }
        };
        assert_eq!(last_stop(false), Some(LIGHT_ACC), "by day the stone");
        assert_eq!(last_stop(true), Some(LIGHT_GLOW), "at night the stone's glow");
    }

    /// The lit range of a flora date picker is flora's selection wash - the
    /// stone's soft tint by day, its deep tone at night - not flat's blue.
    #[test]
    fn a_flora_date_range_is_washed_in_floras_selection() {
        let look = date_picker_look();
        assert_eq!(fill(&look.day_in_range, false), Some(LIGHT_SOFT));
        assert_eq!(fill(&look.day_in_range, true), Some(DARK_DEEP));
    }

    /// A settings section's title is `.fl-label`: capitals in the label ink
    /// (`--fl-soft1`), not semibold brass.
    #[test]
    fn a_flora_settings_section_title_is_floras_label() {
        let title = shell_look().settings_section_title;
        assert!(in_capitals(&title));
        assert_eq!(ink(&title, false), Some(LIGHT_SOFT1));
        assert_eq!(ink(&title, true), Some(DARK_SOFT1));
    }

    /// A small flora label (`.fl-label`: a list's group header, a field's
    /// key) is set in capitals.
    #[test]
    fn floras_small_label_is_set_in_capitals() {
        assert!(in_capitals(&flora_label()));
    }

    /// The code view's token inks are flora.css's own (its Prism tokens
    /// "recut in the house palette"), the comments in the panel's dim ink.
    #[test]
    fn the_code_views_tokens_are_flora_css_inks() {
        use crate::widgets::code_view::CodeTokenKind as K;
        let look = code_view_look();
        let token = |k: K, dark: bool| ink(&look.tokens[k as usize], dark);
        for dark in [false, true] {
            for (k, want) in [
                (K::Keyword, ColorU::rgb(0xD2, 0xC7, 0x9E)),
                (K::StringLiteral, ColorU::rgb(0xA3, 0xC0, 0xAB)),
                (K::Function, ColorU::rgb(0xD3, 0xB7, 0x9C)),
                (K::Type, ColorU::rgb(0xD3, 0xB7, 0x9C)),
                (K::Number, ColorU::rgb(0xA9, 0xB6, 0xD8)),
                (K::Constant, ColorU::rgb(0xA9, 0xB6, 0xD8)),
                (K::Operator, ColorU::rgb(0xCB, 0xC7, 0xB4)),
                (K::Punctuation, ColorU::rgb(0xA9, 0xA5, 0x97)),
                (K::Tag, ColorU::rgb(0xC9, 0xA4, 0x9C)),
                (K::Attribute, ColorU::rgb(0xC9, 0xA4, 0x9C)),
            ] {
                assert_eq!(token(k, dark), Some(want), "{k:?} (dark {dark})");
            }
        }
        assert_eq!(token(K::Comment, false), Some(ColorU::rgb(0x92, 0x8D, 0x80)));
        assert_eq!(token(K::Comment, true), Some(ColorU::rgb(0x85, 0x85, 0x85)));
    }
}
