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
// * `--fl-grain` / `--fl-fibre`: `repeating-linear-gradient` with PIXEL stops (`0 1px, transparent
//   1px 3px`); a stop here is a percentage.
// * `--fl-rolled-tab`: a `calc()` stop.
// * The `mask-image` gradients (not backgrounds) and the `.docs-card::after` sheen (a keyframe
//   animation).
// * `--fl-band`, `--fl-rolled`, `--fl-rule-metal-bg`, `--fl-gem-sunken`, the `.fl-tab-*` pieces and
//   the scrollbar thumb: tabs, ribbon rules and scrollbars have no theme function in this module
//   yet. They belong with the widget that gets one, not as consts nothing declares.

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

/// [`RAISED_FACE_LIGHT`] as a one-layer background, for `const` style slices.
const RAISED_FACE_LIGHT_LAYER: &[StyleBackgroundContent] = &[RAISED_FACE_LIGHT];

/// [`RAISED_FACE_DARK`] as a one-layer background, for `const` style slices.
const RAISED_FACE_DARK_LAYER: &[StyleBackgroundContent] = &[RAISED_FACE_DARK];

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

    // Resolved before `btn`'s fields are moved into the tree below.
    let btn_container_style = btn.resolved_container_style();
    let btn_label_style = btn.resolved_label_style();
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
            crate::widgets::widget_p()
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

    // Add dark mode colors to container style
    let mut container_style: Vec<CssPropertyWithConditions> =
        btn_container_style.as_slice().to_vec();
    // The resting face, in flora.css's terms. The standard command is raised
    // paper (`.btn-secondary`: `linear-gradient(var(--fl-rT), var(--fl-rB))`),
    // with its dark twin. A coloured command is a stone: its own colour in
    // both modes, under the depth rig and the streak the CSS lays on its
    // accent stone. The Link button has no surface and keeps what it had, the
    // dark surface included. It is part of the BASE: after the widget's flat
    // fill, which it wins over, and before the states, which win over it.
    {
        use crate::widgets::button::ButtonType;
        match btn_type {
            ButtonType::Default => {
                container_style.push(CssPropertyWithConditions::simple(layers(vec![
                    RAISED_FACE_LIGHT,
                ])));
                container_style.push(CssPropertyWithConditions::dark_theme(layers(vec![
                    RAISED_FACE_DARK,
                ])));
            }
            ButtonType::Link => {
                container_style.push(CssPropertyWithConditions::dark_theme(layers(vec![
                    StyleBackgroundContent::Color(DARK_SUR),
                ])));
            }
            _ => {
                let (bg, _, _) = crate::widgets::button::get_button_colors(btn_type);
                container_style.push(CssPropertyWithConditions::simple(layers(stone_face(
                    bg,
                    STONE_STREAK,
                ))));
            }
        }
    }

    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::TextColor(StyleTextColor { inner: DARK_INK }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BorderTopColor(StyleBorderTopColor { inner: DARK_BD }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BorderBottomColor(StyleBorderBottomColor { inner: DARK_BD }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BorderLeftColor(StyleBorderLeftColor { inner: DARK_BD }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BorderRightColor(StyleBorderRightColor { inner: DARK_BD }.into()),
    ));

    // Here we could wrap the button in decorative DOM nodes for the skeumorphic flora look.
    // For now, we apply basic properties to test the theming engine.

    // The interactive states go LAST. Inline declarations resolve last-match
    // wins and a `dark_theme(..)` rule matches in every pseudo-state, so any
    // dark resting colour pushed after a `dark_on_hover` / `dark_on_focus` twin
    // would shadow it — no ring, no hover face, in dark mode.
    container_style.extend(button_states(btn_type));

    button
        .with_css_props(CssPropertyWithConditionsVec::from_vec(container_style))
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_callbacks(callbacks.into())
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(a11y)
}

use crate::widgets::check_box::CheckBox;

#[must_use]
pub fn check_box(cb: CheckBox) -> Dom {
    let cb_name = cb.accessibility_name.clone();
    crate::widgets::warn_widget_needs_a_name("check_box", cb_name.is_some());

    let checked_now = cb.check_box_state.inner.checked;

    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{EventFilter, HoverEventFilter},
    };

    let mut container_style: Vec<CssPropertyWithConditions> =
        cb.resolved_container_style().as_slice().to_vec();
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_SUR)])
                .into(),
        ),
    ));
    let is_checked = cb.check_box_state.inner.checked;
    let mut content_style: Vec<CssPropertyWithConditions> =
        cb.resolved_content_style().as_slice().to_vec();
    if checked_now {
        content_style.push(CssPropertyWithConditions::dark_theme(
            CssProperty::BackgroundContent(
                StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_INK)])
                    .into(),
            ),
        ));
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
        .with_children(
            vec![Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from(
                    crate::widgets::check_box::CHECKBOX_CONTENT_CLASS,
                ))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(content_style))]
            .into(),
        )
}

use crate::widgets::text_input::{
    default_on_focus_lost, default_on_focus_received, default_on_mouse_hover,
    default_on_text_input, default_on_virtual_key_down, TextInput, TEXT_INPUT_CONTAINER_CLASS,
    TEXT_INPUT_LABEL_CLASS,
};

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

    let label_text: String = ti
        .text_input_state
        .inner
        .text
        .iter()
        .filter_map(|s| core::char::from_u32(*s))
        .collect();

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

    let state_ref = RefAny::new(ti.text_input_state);

    let mut container_style: Vec<CssPropertyWithConditions> =
        resolved_container_style.as_slice().to_vec();
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_SUR)])
                .into(),
        ),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::TextColor(StyleTextColor { inner: DARK_INK }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BorderTopColor(StyleBorderTopColor { inner: DARK_BD }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BorderBottomColor(StyleBorderBottomColor { inner: DARK_BD }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BorderLeftColor(StyleBorderLeftColor { inner: DARK_BD }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BorderRightColor(StyleBorderRightColor { inner: DARK_BD }.into()),
    ));

    // The interactive states the widget no longer declares. Appended LAST —
    // after the base style and after the theme's own dark resting colours —
    // because the last matching inline declaration wins: a `dark_theme` border
    // pushed after these would beat the dark hover/focus ring. One array so
    // half of them cannot ship.
    container_style.extend_from_slice(&FIELD_BORDER_STATES);

    let mut label_style: Vec<CssPropertyWithConditions> = resolved_label_style.as_slice().to_vec();
    label_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::TextColor(StyleTextColor { inner: DARK_INK }.into()),
    ));

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

#[must_use]
pub fn label(l: crate::widgets::label::Label) -> Dom {
    use azul_core::dom::{IdOrClass::Class, IdOrClassVec};
    use AzString;

    static LABEL_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-label"))];

    // Resolved before `l.string` is moved out below.
    let label_style = l.resolved_label_style();

    crate::widgets::widget_p_with_text(l.string)
        .with_ids_and_classes(IdOrClassVec::from_const_slice(LABEL_CLASS))
        .with_css_props(label_style)
}

#[must_use]
pub fn switch(s: crate::widgets::switch::Switch) -> Dom {
    let is_checked = s.switch_state.inner.checked;
    // Resolved up front: the knob's Dom is built after `s.switch_state` has
    // been moved into the callback's RefAny, and the resolver needs the whole
    // widget.
    let resolved_track_style = s.resolved_track_style();
    let resolved_knob_style = s.resolved_knob_style();
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{Dom, EventFilter, HoverEventFilter, IdOrClassVec, TabIndex},
    };

    let sw_name = s.accessibility_name.clone();
    crate::widgets::warn_widget_needs_a_name("switch", sw_name.is_some());

    let switch_checked = s.switch_state.inner.checked;

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from(
            crate::widgets::switch::SWITCH_TRACK_CLASS,
        ))
        .with_css_props(resolved_track_style.as_slice().to_vec().into())
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
                if switch_checked {
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
                .with_css_props(resolved_knob_style.as_slice().to_vec().into())]
            .into(),
        )
}

// -----------------------------------------------------------------------------
// PROGRESSBAR
// -----------------------------------------------------------------------------

#[must_use]
pub fn progressbar(bar: crate::widgets::progressbar::ProgressBar) -> Dom {
    let height = bar.height;
    let dataset = RefAny::new(crate::widgets::progressbar::ProgressBarLocalDataset { bar });
    Dom::create_virtual_view(
        dataset.clone(),
        azul_core::callbacks::VirtualViewCallback::create(progressbar_render_virtual_view),
    )
    .with_dataset(Some(dataset).into())
    .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
        CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
            LayoutHeight::Px(height),
        ))),
        CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
            LayoutWidth::Px(PixelValue::percent(100.0)),
        ))),
        CssPropertyWithConditions::simple(CssProperty::OverflowX(LayoutOverflowValue::Exact(
            LayoutOverflow::Hidden,
        ))),
        CssPropertyWithConditions::simple(CssProperty::OverflowY(LayoutOverflowValue::Exact(
            LayoutOverflow::Hidden,
        ))),
    ]))
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
/// express: the container is sized to `bounds - 2px borders` so its 1px
/// border ring lands INSIDE the box - with the normal-flow sizing (content
/// height + borders) the ring overflowed the VV node and was clipped away
/// at the right and bottom ("oddly cut off", user report 2026-08-29) - and
/// the fill is an exact device-pixel split of the known content width.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn progressbar_render_bar_impl(
    bar: crate::widgets::progressbar::ProgressBar,
    bounds_px: Option<(f32, f32)>,
) -> Dom {
    {
        use azul_core::dom::DomVec;

        let this = bar;
        let percent_done = this.progressbar_state.percent_done.clamp(0.0, 100.0);
        // Sizes resolved per context (see fn docs). The bounds branch
        // subtracts the container's 1px border ring so children + borders
        // exactly fill the VV box.
        let (bar_width, remaining_width) = match bounds_px {
            Some((w, _)) => {
                let inner = (w - 2.0).max(0.0);
                let filled = inner * percent_done / 100.0;
                (PixelValue::px(filled), PixelValue::px(inner - filled))
            }
            None => (
                PixelValue::percent(percent_done),
                PixelValue::percent(100.0 - percent_done),
            ),
        };
        let container_height = match bounds_px {
            Some((_, h)) => PixelValue::px((h - 2.0).max(0.0)),
            None => this.height,
        };

        let mut container_props = vec![
            // .__azul-native-progress-bar-container
            CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
                LayoutHeight::Px(container_height),
            ))),
            // `display: flex` is LOAD-BEARING: azul's default display is
            // BLOCK, so `flex-direction: row` alone stacks the two
            // children as full-width, zero-height block boxes - the fill
            // never painted anywhere the widget was used (found 2026-08-29
            // via the azpaint pressure meter; also the real culprit behind
            // the "inline-width meter never repaints" ledger entry).
            CssPropertyWithConditions::simple(CssProperty::Display(LayoutDisplayValue::Exact(
                LayoutDisplay::Flex,
            ))),
            CssPropertyWithConditions::simple(CssProperty::FlexDirection(
                LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row),
            )),
            CssPropertyWithConditions::simple(CssProperty::BoxShadowBottom(
                StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                    offset_x: PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    offset_y: PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    color: ColorU {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 9,
                    },
                    blur_radius: PixelValueNoPercent {
                        inner: PixelValue::const_px(15),
                    },
                    spread_radius: PixelValueNoPercent {
                        inner: PixelValue::const_px(2),
                    },
                    clip_mode: BoxShadowClipMode::Inset,
                })),
            )),
            CssPropertyWithConditions::simple(CssProperty::BoxShadowTop(
                StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                    offset_x: PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    offset_y: PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    color: ColorU {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 9,
                    },
                    blur_radius: PixelValueNoPercent {
                        inner: PixelValue::const_px(15),
                    },
                    spread_radius: PixelValueNoPercent {
                        inner: PixelValue::const_px(2),
                    },
                    clip_mode: BoxShadowClipMode::Inset,
                })),
            )),
            CssPropertyWithConditions::simple(CssProperty::BoxShadowRight(
                StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                    offset_x: PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    offset_y: PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    color: ColorU {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 9,
                    },
                    blur_radius: PixelValueNoPercent {
                        inner: PixelValue::const_px(15),
                    },
                    spread_radius: PixelValueNoPercent {
                        inner: PixelValue::const_px(2),
                    },
                    clip_mode: BoxShadowClipMode::Inset,
                })),
            )),
            CssPropertyWithConditions::simple(CssProperty::BoxShadowLeft(
                StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                    offset_x: PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    offset_y: PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    color: ColorU {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 9,
                    },
                    blur_radius: PixelValueNoPercent {
                        inner: PixelValue::const_px(15),
                    },
                    spread_radius: PixelValueNoPercent {
                        inner: PixelValue::const_px(2),
                    },
                    clip_mode: BoxShadowClipMode::Inset,
                })),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderBottomRightRadius(
                StyleBorderBottomRightRadiusValue::Exact(StyleBorderBottomRightRadius {
                    inner: PixelValue::const_px(3),
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderBottomLeftRadius(
                StyleBorderBottomLeftRadiusValue::Exact(StyleBorderBottomLeftRadius {
                    inner: PixelValue::const_px(3),
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderTopRightRadius(
                StyleBorderTopRightRadiusValue::Exact(StyleBorderTopRightRadius {
                    inner: PixelValue::const_px(3),
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderTopLeftRadius(
                StyleBorderTopLeftRadiusValue::Exact(StyleBorderTopLeftRadius {
                    inner: PixelValue::const_px(3),
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
                LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
                    inner: PixelValue::const_px(1),
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
                LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
                    inner: PixelValue::const_px(1),
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
                LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
                    inner: PixelValue::const_px(1),
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
                LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
                    inner: PixelValue::const_px(1),
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
                StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
                    inner: BorderStyle::Solid,
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
                StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
                    inner: BorderStyle::Solid,
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
                StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
                    inner: BorderStyle::Solid,
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
                StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
                    inner: BorderStyle::Solid,
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
                StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
                    inner: ColorU {
                        r: 178,
                        g: 178,
                        b: 178,
                        a: 255,
                    },
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
                StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
                    inner: ColorU {
                        r: 178,
                        g: 178,
                        b: 178,
                        a: 255,
                    },
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
                StyleBorderRightColorValue::Exact(StyleBorderRightColor {
                    inner: ColorU {
                        r: 178,
                        g: 178,
                        b: 178,
                        a: 255,
                    },
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
                StyleBorderTopColorValue::Exact(StyleBorderTopColor {
                    inner: ColorU {
                        r: 178,
                        g: 178,
                        b: 178,
                        a: 255,
                    },
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
                StyleBackgroundContentVecValue::Exact(this.container_background.clone()),
            )),
        ];
        if let Some((w, _)) = bounds_px {
            container_props.push(CssPropertyWithConditions::simple(CssProperty::Width(
                LayoutWidthValue::Exact(LayoutWidth::Px(PixelValue::px((w - 2.0).max(0.0)))),
            )));
        }

        Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_vec(container_props))
            .with_ids_and_classes({
                const IDS_AND_CLASSES_10874511710181900075: &[IdOrClass] = &[Class(
                    AzString::from_const_str("__azul-native-progress-bar-container"),
                )];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_10874511710181900075)
            })
            // For a progress bar the VALUE is the content: two coloured divs
            // say nothing to a screen reader, "75%" says everything. Published
            // on every build so it tracks the bar; a callback that moves the
            // bar live without a rebuild keeps it current with
            // `CallbackInfo::set_accessibility_value` on this node.
            .with_accessibility_info(AccessibilityInfo {
                role: AccessibilityRole::ProgressBar,
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
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
                        // .__azul-native-progress-bar-bar
                        // Use percentage width instead of flex-grow hack
                        CssPropertyWithConditions::simple(CssProperty::Width(
                            LayoutWidthValue::Exact(LayoutWidth::Px(bar_width)),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BoxShadowBottom(
                            StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                                offset_x: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                offset_y: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                color: ColorU {
                                    r: 0,
                                    g: 51,
                                    b: 0,
                                    a: 51,
                                },
                                blur_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(15),
                                },
                                spread_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(12),
                                },
                                clip_mode: BoxShadowClipMode::Inset,
                            })),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BoxShadowTop(
                            StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                                offset_x: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                offset_y: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                color: ColorU {
                                    r: 0,
                                    g: 51,
                                    b: 0,
                                    a: 51,
                                },
                                blur_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(15),
                                },
                                spread_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(12),
                                },
                                clip_mode: BoxShadowClipMode::Inset,
                            })),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BoxShadowRight(
                            StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                                offset_x: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                offset_y: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                color: ColorU {
                                    r: 0,
                                    g: 51,
                                    b: 0,
                                    a: 51,
                                },
                                blur_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(15),
                                },
                                spread_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(12),
                                },
                                clip_mode: BoxShadowClipMode::Inset,
                            })),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BoxShadowLeft(
                            StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                                offset_x: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                offset_y: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                color: ColorU {
                                    r: 0,
                                    g: 51,
                                    b: 0,
                                    a: 51,
                                },
                                blur_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(15),
                                },
                                spread_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(12),
                                },
                                clip_mode: BoxShadowClipMode::Inset,
                            })),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderBottomRightRadius(
                            StyleBorderBottomRightRadiusValue::Exact(
                                StyleBorderBottomRightRadius {
                                    inner: PixelValue::const_px(1),
                                },
                            ),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderBottomLeftRadius(
                            StyleBorderBottomLeftRadiusValue::Exact(StyleBorderBottomLeftRadius {
                                inner: PixelValue::const_px(1),
                            }),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderTopRightRadius(
                            StyleBorderTopRightRadiusValue::Exact(StyleBorderTopRightRadius {
                                inner: PixelValue::const_px(1),
                            }),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderTopLeftRadius(
                            StyleBorderTopLeftRadiusValue::Exact(StyleBorderTopLeftRadius {
                                inner: PixelValue::const_px(1),
                            }),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
                            StyleBackgroundContentVecValue::Exact(this.bar_background),
                        )),
                    ]))
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_16512648314570682783: &[IdOrClass] = &[Class(
                            AzString::from_const_str("__azul-native-progress-bar-bar"),
                        )];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_16512648314570682783)
                    }),
                Dom::create_div()
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
                        // .__azul-native-progress-bar-remaining
                        // Use percentage width for the remaining space
                        CssPropertyWithConditions::simple(CssProperty::Width(
                            LayoutWidthValue::Exact(LayoutWidth::Px(remaining_width)),
                        )),
                    ]))
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_2492405364126620395: &[IdOrClass] = &[Class(
                            AzString::from_const_str("__azul-native-progress-bar-remaining"),
                        )];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_2492405364126620395)
                    }),
            ]))
    }
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

    // Flora specific. The rail stays a flat track: flora.css's `--fl-track` is
    // a flat token, and the rail has no border to hold a paler well against the
    // page. The thumb is the one domed control here, and the CSS caps its dome
    // with `.fl-orb-gloss`: laid OVER whatever colour the widget resolved for
    // the thumb — read back rather than restated, so it cannot drift from
    // slider.rs — and over the theme's accent in dark mode.
    track_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_TRACK)])
                .into(),
        ),
    ));
    let mut thumb_layers: Vec<StyleBackgroundContent> = thumb_style
        .iter()
        .rev()
        .find_map(|p| match &p.property {
            CssProperty::BackgroundContent(b) if p.apply_if.as_ref().is_empty() => {
                b.get_property().map(|b| b.as_ref().to_vec())
            }
            _ => None,
        })
        .unwrap_or_default();
    thumb_layers.push(ORB_GLOSS);
    thumb_style.push(CssPropertyWithConditions::simple(layers(thumb_layers)));
    thumb_style.push(CssPropertyWithConditions::dark_theme(layers(vec![
        StyleBackgroundContent::Color(DARK_ACC),
        ORB_GLOSS,
    ])));

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
                .with_css_props(CssPropertyWithConditionsVec::from_vec(thumb_style))]
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

    let mut container_style: Vec<CssPropertyWithConditions> =
        resolved_container_style.as_slice().to_vec();
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_SUR)])
                .into(),
        ),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::TextColor(StyleTextColor { inner: DARK_INK }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BorderTopColor(StyleBorderTopColor { inner: DARK_BD }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BorderBottomColor(StyleBorderBottomColor { inner: DARK_BD }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BorderLeftColor(StyleBorderLeftColor { inner: DARK_BD }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BorderRightColor(StyleBorderRightColor { inner: DARK_BD }.into()),
    ));

    let mut label_style: Vec<CssPropertyWithConditions> = match &ta.label_style {
        azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::Some(s) => {
            s.as_slice().to_vec()
        }
        azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::None => {
            crate::widgets::text_area::TEXT_AREA_LABEL_PROPS.to_vec()
        }
    };
    label_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::TextColor(StyleTextColor { inner: DARK_INK }.into()),
    ));

    // The interactive states go LAST. Inline declarations resolve last-match
    // wins and a `dark_theme(..)` rule matches in every pseudo-state, so any
    // dark resting colour pushed after a `dark_on_hover` / `dark_on_focus` twin
    // would shadow it — no ring, no hover face, in dark mode.
    container_style.extend_from_slice(&FIELD_BORDER_STATES);

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

const FLORA_DROPDOWN_WRAPPER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineFlex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(6),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(6),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        4,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
        LayoutBorderTopWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
        LayoutBorderRightWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(RAISED_FACE_LIGHT_LAYER),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: LIGHT_INK,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: LIGHT_BD,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor { inner: LIGHT_BD },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: LIGHT_BD,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor { inner: LIGHT_BD },
    )),
    CssPropertyWithConditions::dark_theme(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(RAISED_FACE_DARK_LAYER),
    )),
    CssPropertyWithConditions::dark_theme(CssProperty::const_text_color(StyleTextColor {
        inner: DARK_INK,
    })),
    CssPropertyWithConditions::dark_theme(CssProperty::const_border_top_color(
        StyleBorderTopColor { inner: DARK_BD },
    )),
    CssPropertyWithConditions::dark_theme(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor { inner: DARK_BD },
    )),
    CssPropertyWithConditions::dark_theme(CssProperty::const_border_left_color(
        StyleBorderLeftColor { inner: DARK_BD },
    )),
    CssPropertyWithConditions::dark_theme(CssProperty::const_border_right_color(
        StyleBorderRightColor { inner: DARK_BD },
    )),
];

const FLORA_DROPDOWN_LABEL_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(10),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: LIGHT_INK,
    })),
    CssPropertyWithConditions::dark_theme(CssProperty::const_text_color(StyleTextColor {
        inner: DARK_INK,
    })),
];

const FLORA_DROPDOWN_ARROW_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(18))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: LIGHT_INK,
    })),
    CssPropertyWithConditions::dark_theme(CssProperty::const_text_color(StyleTextColor {
        inner: DARK_INK,
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

    Dom::create_div()
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
            FLORA_DROPDOWN_WRAPPER_STYLE,
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
            crate::widgets::widget_p()
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                    FLORA_DROPDOWN_LABEL_STYLE,
                ))
                .with_children(DomVec::from_vec(vec![
                    Dom::create_text_do_not_use_without_block_level_wrapper(selected_text),
                ])),
            Dom::create_icon(AzString::from_const_str("arrow_drop_down")).with_css_props(
                CssPropertyWithConditionsVec::from_const_slice(FLORA_DROPDOWN_ARROW_STYLE),
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
/// twin using [`DARK_ACC`]: the accent is the one state colour with a genuine
/// per-mode value in both palettes, which is why the plan names it.
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
        StyleBorderTopColor { inner: DARK_ACC },
    ));

/// The dark twin of [`FOCUS_BORDER_BOTTOM`].
pub const FOCUS_BORDER_BOTTOM_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor { inner: DARK_ACC },
    ));

/// The dark twin of [`FOCUS_BORDER_LEFT`].
pub const FOCUS_BORDER_LEFT_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_left_color(
        StyleBorderLeftColor { inner: DARK_ACC },
    ));

/// The dark twin of [`FOCUS_BORDER_RIGHT`].
pub const FOCUS_BORDER_RIGHT_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_right_color(
        StyleBorderRightColor { inner: DARK_ACC },
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

/// Every state a button of one semantic type takes: hover fill, pressed fill and
/// focus ring, each with its dark twin.
///
/// The coloured types' values come from [`crate::widgets::button::get_button_colors`],
/// so there is still one source of truth for them; the neutral type's faces and
/// every DARK half are chosen here, because this is the only place the palette
/// is in scope.
///
/// The rule differs by type on purpose:
///
/// * `Default` is the neutral paper button, so its surface belongs to the PAGE: it hovers and
///   presses to the theme's faces — [`HOVER_FACE_LIGHT`] / [`HOVER_FACE_DARK`] and
///   [`PRESSED_FACE_LIGHT`] / [`PRESSED_FACE_DARK`], the gradients flora.css draws for
///   `.btn-secondary:hover` and `:active`.
/// * Every other type carries its own semantic colour — a Primary button is blue whichever mode the
///   app is in — so the same hover and pressed colours apply in dark mode. A neutral grey hover on
///   a blue button would be wrong, and inventing a second blue would be a design decision this
///   refactor has no business making. That colour is the base layer; over it goes the depth rig
///   flora.css lays on its accent stone (`.btn-primary::after` and `::before`), sunken while
///   pressed.
/// * `Link` has no surface at all: it underlines instead, in both modes.
#[must_use]
pub fn button_states(
    button_type: crate::widgets::button::ButtonType,
) -> Vec<CssPropertyWithConditions> {
    use crate::widgets::button::ButtonType;

    if button_type == ButtonType::Link {
        return alloc::vec![
            CssPropertyWithConditions::on_hover(CssProperty::TextDecoration(
                StyleTextDecoration::Underline.into(),
            )),
            CssPropertyWithConditions::dark_on_hover(CssProperty::TextDecoration(
                StyleTextDecoration::Underline.into(),
            )),
        ];
    }

    let (_, bg_hover, bg_active) = crate::widgets::button::get_button_colors(button_type);
    let neutral = button_type == ButtonType::Default;
    // The neutral button is paper: it hovers and presses to the theme's faces,
    // each with its dark twin. A coloured button is a stone: the same colour in
    // both modes (see above), under the rig flora.css lays on a stone.
    let (hover, dark_hover, active, dark_active) = if neutral {
        (
            vec![HOVER_FACE_LIGHT],
            vec![HOVER_FACE_DARK],
            vec![PRESSED_FACE_LIGHT],
            vec![PRESSED_FACE_DARK],
        )
    } else {
        (
            stone_face(bg_hover, STONE_STREAK_HOVER),
            stone_face(bg_hover, STONE_STREAK_HOVER),
            sunken_stone_face(bg_active),
            sunken_stone_face(bg_active),
        )
    };

    let mut out = alloc::vec![
        CssPropertyWithConditions::on_hover(layers(hover)),
        CssPropertyWithConditions::dark_on_hover(layers(dark_hover)),
        CssPropertyWithConditions::on_active(layers(active)),
        CssPropertyWithConditions::dark_on_active(layers(dark_active)),
    ];

    // The neutral button is the only one with a visible resting border, so it is
    // the only one whose border reacts to hover.
    if neutral {
        let light = ColorU::rgb(173, 181, 189);
        for (l, d) in [
            (
                CssProperty::const_border_top_color(StyleBorderTopColor { inner: light }),
                CssProperty::const_border_top_color(StyleBorderTopColor { inner: DARK_BD }),
            ),
            (
                CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: light }),
                CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: DARK_BD }),
            ),
            (
                CssProperty::const_border_left_color(StyleBorderLeftColor { inner: light }),
                CssProperty::const_border_left_color(StyleBorderLeftColor { inner: DARK_BD }),
            ),
            (
                CssProperty::const_border_right_color(StyleBorderRightColor { inner: light }),
                CssProperty::const_border_right_color(StyleBorderRightColor { inner: DARK_BD }),
            ),
        ] {
            out.push(CssPropertyWithConditions::on_hover(l));
            out.push(CssPropertyWithConditions::dark_on_hover(d));
        }
    }

    // The focus ring is the accent in both modes, and the consts already pair it.
    out.push(FOCUS_BORDER_TOP);
    out.push(FOCUS_BORDER_BOTTOM);
    out.push(FOCUS_BORDER_LEFT);
    out.push(FOCUS_BORDER_RIGHT);
    out.push(FOCUS_BORDER_TOP_DARK);
    out.push(FOCUS_BORDER_BOTTOM_DARK);
    out.push(FOCUS_BORDER_LEFT_DARK);
    out.push(FOCUS_BORDER_RIGHT_DARK);
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
        button::{get_button_colors, Button, ButtonType},
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
            matches!(c, [DynamicSelector::Theme(ThemeCondition::Dark)])
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
                    .any(|s| matches!(s, DynamicSelector::Theme(ThemeCondition::Dark)))
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
    fn a_coloured_button_keeps_its_own_colour_as_the_base_layer_under_the_rig() {
        let (bg, bg_hover, bg_active) = get_button_colors(ButtonType::Primary);

        let dom = button(Button::with_type(
            AzString::from_const_str("Go"),
            ButtonType::Primary,
        ));
        assert_eq!(
            resting_background(&dom),
            Some(vec![
                StyleBackgroundContent::Color(bg),
                STONE_RIG_TOP,
                STONE_RIG_LEFT,
                STONE_STREAK,
            ]),
            "the stone's colour paints first; the rig and streak go over it"
        );
        assert_eq!(
            dark_resting_background(&dom),
            None,
            "a stone keeps its colour in dark mode, so it needs no dark override"
        );

        let dom = flora_button("Go", ButtonType::Primary).dom();
        let hovered = vec![
            StyleBackgroundContent::Color(bg_hover),
            STONE_RIG_TOP,
            STONE_RIG_LEFT,
            STONE_STREAK_HOVER,
        ];
        assert_eq!(
            state_background(&dom, PseudoStateType::Hover, false),
            Some(hovered.clone())
        );
        assert_eq!(
            state_background(&dom, PseudoStateType::Hover, true),
            Some(hovered)
        );
        let pressed = vec![
            StyleBackgroundContent::Color(bg_active),
            SUNKEN_RIG_TOP,
            SUNKEN_RIG_LEFT,
            SUNKEN_RIG_BOTTOM,
        ];
        assert_eq!(
            state_background(&dom, PseudoStateType::Active, false),
            Some(pressed.clone())
        );
        assert_eq!(
            state_background(&dom, PseudoStateType::Active, true),
            Some(pressed)
        );
    }

    #[test]
    fn the_link_button_has_no_surface_and_grows_no_face() {
        let dom = button(Button::with_type(
            AzString::from_const_str("more"),
            ButtonType::Link,
        ));
        assert_eq!(
            resting_background(&dom),
            Some(vec![StyleBackgroundContent::Color(ColorU::TRANSPARENT)]),
            "the widget's transparent fill is untouched"
        );
        let dom = flora_button("more", ButtonType::Link).dom();
        assert_eq!(
            state_background(&dom, PseudoStateType::Hover, false),
            None,
            "a link underlines on hover; it does not take a face"
        );
    }

    #[test]
    fn the_dropdown_wrapper_rests_on_the_raised_paper_face_in_both_modes() {
        let backgrounds: Vec<(bool, Vec<StyleBackgroundContent>)> = FLORA_DROPDOWN_WRAPPER_STYLE
            .iter()
            .filter_map(|p| match &p.property {
                CssProperty::BackgroundContent(b) => Some((
                    matches!(
                        p.apply_if.as_ref(),
                        [DynamicSelector::Theme(ThemeCondition::Dark)]
                    ),
                    b.get_property()?.as_ref().to_vec(),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            backgrounds,
            vec![
                (false, vec![RAISED_FACE_LIGHT]),
                (true, vec![RAISED_FACE_DARK]),
            ]
        );
    }

    #[test]
    fn the_slider_thumb_wears_the_orb_gloss_over_its_own_colour() {
        let dom = slider(Slider::create(50.0, 0.0, 100.0));
        let thumb = &dom.children.as_ref()[0];

        let light = resting_background(thumb).expect("the thumb declares a background");
        assert!(
            matches!(light.first(), Some(StyleBackgroundContent::Color(_))),
            "the widget's own thumb colour is the base layer, read back rather than restated: \
             {light:?}"
        );
        assert_eq!(light.last(), Some(&ORB_GLOSS), "the gloss is the top layer");
        assert_eq!(light.len(), 2);

        assert_eq!(
            dark_resting_background(thumb),
            Some(vec![StyleBackgroundContent::Color(DARK_ACC), ORB_GLOSS]),
            "dark mode: the same cap over the theme's accent"
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
