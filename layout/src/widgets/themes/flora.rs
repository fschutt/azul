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
    // A caller who injected a container style (`Some`) chose every property in
    // it — resting face, dark colours and states included. The chrome widgets
    // hand in part styles complete with their own hover/pressed pairs, and
    // anything this theme appended after them would win the cascade (inline
    // resolution is last-match) and paint the theme's greys over the ribbon's
    // blue. So the theme adds to its OWN default only.
    let btn_owns_style = btn.container_style.as_ref().is_none();
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

    // Add dark mode colors to container style
    let mut container_style: Vec<CssPropertyWithConditions> =
        btn_container_style.as_slice().to_vec();

    if btn_owns_style {
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
                    container_style.push(CssPropertyWithConditions::dark_mode(layers(vec![
                        RAISED_FACE_DARK,
                    ])));
                }
                ButtonType::Link => {
                    container_style.push(CssPropertyWithConditions::dark_mode(layers(vec![
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

        // Dark ink and dark borders belong to the NEUTRAL surface (raised
        // paper); a coloured stone keeps its own text and edge colours in
        // both modes, and the link has no face to border. Same rule as the
        // resting face above, from the one place it lives:
        // `ButtonType::surface`.
        if btn_type.surface() == crate::widgets::button::ButtonSurface::Neutral {
            container_style.push(CssPropertyWithConditions::dark_mode(
                CssProperty::TextColor(StyleTextColor { inner: DARK_INK }.into()),
            ));
            container_style.push(CssPropertyWithConditions::dark_mode(
                CssProperty::BorderTopColor(StyleBorderTopColor { inner: DARK_BD }.into()),
            ));
            container_style.push(CssPropertyWithConditions::dark_mode(
                CssProperty::BorderBottomColor(StyleBorderBottomColor { inner: DARK_BD }.into()),
            ));
            container_style.push(CssPropertyWithConditions::dark_mode(
                CssProperty::BorderLeftColor(StyleBorderLeftColor { inner: DARK_BD }.into()),
            ));
            container_style.push(CssPropertyWithConditions::dark_mode(
                CssProperty::BorderRightColor(StyleBorderRightColor { inner: DARK_BD }.into()),
            ));
        }

        // Here we could wrap the button in decorative DOM nodes for the skeumorphic flora look.
        // For now, we apply basic properties to test the theming engine.

        // The interactive states go LAST. Inline declarations resolve last-match
        // wins and a `dark_theme(..)` rule matches in every pseudo-state, so any
        // dark resting colour pushed after a `dark_on_hover` / `dark_on_focus` twin
        // would shadow it — no ring, no hover face, in dark mode.
        container_style.extend(button_states(btn_type));
    }

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
    container_style.push(CssPropertyWithConditions::dark_mode(
        CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_SUR)])
                .into(),
        ),
    ));
    let is_checked = cb.check_box_state.inner.checked;
    let mut content_style: Vec<CssPropertyWithConditions> =
        cb.resolved_content_style().as_slice().to_vec();
    if checked_now {
        content_style.push(CssPropertyWithConditions::dark_mode(
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

    let state_ref = RefAny::new(ti.text_input_state);

    let mut container_style: Vec<CssPropertyWithConditions> =
        resolved_container_style.as_slice().to_vec();
    container_style.push(CssPropertyWithConditions::dark_mode(
        CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_SUR)])
                .into(),
        ),
    ));
    container_style.push(CssPropertyWithConditions::dark_mode(
        CssProperty::TextColor(StyleTextColor { inner: DARK_INK }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_mode(
        CssProperty::BorderTopColor(StyleBorderTopColor { inner: DARK_BD }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_mode(
        CssProperty::BorderBottomColor(StyleBorderBottomColor { inner: DARK_BD }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_mode(
        CssProperty::BorderLeftColor(StyleBorderLeftColor { inner: DARK_BD }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_mode(
        CssProperty::BorderRightColor(StyleBorderRightColor { inner: DARK_BD }.into()),
    ));

    // The interactive states the widget no longer declares. Appended LAST —
    // after the base style and after the theme's own dark resting colours —
    // because the last matching inline declaration wins: a `dark_theme` border
    // pushed after these would beat the dark hover/focus ring. One array so
    // half of them cannot ship.
    container_style.extend_from_slice(&FIELD_BORDER_STATES);

    let mut label_style: Vec<CssPropertyWithConditions> = resolved_label_style.as_slice().to_vec();
    label_style.push(CssPropertyWithConditions::dark_mode(
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

        // .__azul-native-progress-bar-container: the widget's base (its
        // structure, the same in every theme), then flora's skin.
        let mut container_props = crate::widgets::progressbar::BAR_CONTAINER_BASE.to_vec();
        container_props.extend(vec![
            CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
                LayoutHeight::Px(container_height),
            ))),
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
        ]);
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
    track_style.push(CssPropertyWithConditions::dark_mode(
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
    thumb_style.push(CssPropertyWithConditions::dark_mode(layers(vec![
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
    container_style.push(CssPropertyWithConditions::dark_mode(
        CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_SUR)])
                .into(),
        ),
    ));
    container_style.push(CssPropertyWithConditions::dark_mode(
        CssProperty::TextColor(StyleTextColor { inner: DARK_INK }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_mode(
        CssProperty::BorderTopColor(StyleBorderTopColor { inner: DARK_BD }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_mode(
        CssProperty::BorderBottomColor(StyleBorderBottomColor { inner: DARK_BD }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_mode(
        CssProperty::BorderLeftColor(StyleBorderLeftColor { inner: DARK_BD }.into()),
    ));
    container_style.push(CssPropertyWithConditions::dark_mode(
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
    label_style.push(CssPropertyWithConditions::dark_mode(
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

/// Flora's trigger skin, after `drop_down::DROPDOWN_WRAPPER_BASE` (R5).
const FLORA_DROPDOWN_WRAPPER_STYLE: &[CssPropertyWithConditions] = &[
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
    CssPropertyWithConditions::dark_mode(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(RAISED_FACE_DARK_LAYER),
    )),
    CssPropertyWithConditions::dark_mode(CssProperty::const_text_color(StyleTextColor {
        inner: DARK_INK,
    })),
    CssPropertyWithConditions::dark_mode(CssProperty::const_border_top_color(
        StyleBorderTopColor { inner: DARK_BD },
    )),
    CssPropertyWithConditions::dark_mode(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor { inner: DARK_BD },
    )),
    CssPropertyWithConditions::dark_mode(CssProperty::const_border_left_color(
        StyleBorderLeftColor { inner: DARK_BD },
    )),
    CssPropertyWithConditions::dark_mode(CssProperty::const_border_right_color(
        StyleBorderRightColor { inner: DARK_BD },
    )),
];

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

/// Flora's arrow skin, after `drop_down::DROPDOWN_ARROW_BASE` (R5).
const FLORA_DROPDOWN_ARROW_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(18))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: LIGHT_INK,
    })),
    CssPropertyWithConditions::dark_mode(CssProperty::const_text_color(StyleTextColor {
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

    // Every part: the widget's structure (R5), then flora's skin.
    use crate::widgets::drop_down::{
        DROPDOWN_ARROW_BASE, DROPDOWN_LABEL_BASE, DROPDOWN_WRAPPER_BASE,
    };

    Dom::create_div()
        .with_css_props(CssPropertyWithConditionsVec::from_vec(
            [DROPDOWN_WRAPPER_BASE, FLORA_DROPDOWN_WRAPPER_STYLE].concat(),
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
    let neutral = button_type.surface() == crate::widgets::button::ButtonSurface::Neutral;
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

// ==== dialog ====
//
// Dialog, Modal and Popover in flora's terms (`doc/templates/flora.css`). The
// panel is a LEAF laid on the page: `--fl-sur` with a `--fl-bd2` hairline, the
// house's larger radius (`--fl-r2`: nothing is rounder than 5) and the shadow a
// floating leaf casts (`--fl-shadow-3`, its first layer). The title is ruled off
// from the content with a `--fl-sep` hairline, the way flora rules a heading.
// The close glyph is a quiet action, so it is written in brass ink
// (`.btn-quiet`: `--fl-qt`, darkening to `--fl-qt2` over the quiet wash on
// hover). A modal dims its window with the drop panel's warm overlay
// (`.nav-overlay`, rgba(20, 19, 16, 0.45)), the same by day and by night. Every
// colour pairs with its night value, and the focus ring is the accent by day and
// lifts to the stone's glow by night (`--focus-color`).

/// `.nav-overlay`: the warm dim behind a modal dialog, in both modes.
pub const DIALOG_BACKDROP: ColorU = ColorU::new(20, 19, 16, 115);
/// `--fl-shadow-3`'s first layer by day: rgba(48, 45, 38, 0.18).
const DIALOG_SHADOW_LIGHT: ColorU = ColorU::new(48, 45, 38, 46);
/// `--fl-shadow-3`'s first layer by night: rgba(0, 0, 0, 0.55).
const DIALOG_SHADOW_DARK: ColorU = ColorU::new(0, 0, 0, 140);
/// `--fl-shadow-2`'s first layer by day: rgba(48, 45, 38, 0.16).
const POPOVER_SHADOW_LIGHT: ColorU = ColorU::new(48, 45, 38, 41);
/// `--fl-shadow-2`'s first layer by night: rgba(0, 0, 0, 0.5).
const POPOVER_SHADOW_DARK: ColorU = ColorU::new(0, 0, 0, 128);
/// `.btn-quiet:hover`'s wash by day: rgba(180, 135, 44, 0.08).
const DIALOG_QUIET_WASH_LIGHT: ColorU = ColorU::new(180, 135, 44, 20);
/// The same wash by night, in the night brass: rgba(196, 181, 142, 0.10).
const DIALOG_QUIET_WASH_DARK: ColorU = ColorU::new(196, 181, 142, 26);

/// Flora's dialog skin (also the modal's; the popover swaps in its panel).
#[must_use]
pub(crate) fn dialog_skin() -> crate::widgets::dialog::DialogSkin {
    use super::style_kit as kit;
    use crate::widgets::dialog as d;
    type P = CssPropertyWithConditions;

    // Every part: the dialog's structure (R5), then flora's skin.
    //
    // The leaf. Same box as flat's panel (280..520 px wide, 20px inset).
    let mut panel = d::DIALOG_PANEL_BASE.to_vec();
    panel.extend([
        P::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(280))),
        P::simple(CssProperty::const_max_width(LayoutMaxWidth::const_px(520))),
        P::simple(CssProperty::const_font_size(StyleFontSize::const_px(14))),
        P::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ]);
    panel.extend(kit::padding(20, 20, 20, 20));
    panel.extend(kit::border(kit::Edges::ALL, 1, LIGHT_BD2, DARK_BD2));
    panel.extend(kit::radius(5));
    panel.extend(kit::themed_bg(LIGHT_SUR, DARK_SUR));
    panel.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
    panel.extend(kit::drop_shadow(6, 14, DIALOG_SHADOW_LIGHT, DIALOG_SHADOW_DARK));

    // The heading, ruled off.
    let mut title = d::DIALOG_TITLE_BASE.to_vec();
    title.extend([
        kit::font_size(17),
        kit::weight(StyleFontWeight::W600),
        P::simple(CssProperty::const_text_align(StyleTextAlign::Left)),
        P::simple(CssProperty::const_margin_bottom(LayoutMarginBottom::const_px(12))),
    ]);
    // The right inset keeps the heading clear of the absolutely-placed close.
    title.extend(kit::padding(0, 28, 10, 0));
    title.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
    title.extend(kit::border(kit::Edges::BOTTOM, 1, LIGHT_SEP, DARK_SEP));

    // The quiet close.
    let mut close = d::DIALOG_CLOSE_BASE.to_vec();
    close.extend([
        P::simple(CssProperty::const_top(LayoutTop::const_px(8))),
        P::simple(CssProperty::const_right(LayoutRight::const_px(10))),
        kit::font_size(20),
    ]);
    close.extend(kit::padding(0, 5, 0, 5));
    close.extend(kit::radius(3));
    close.extend(kit::themed_ink(LIGHT_QT, DARK_QT));
    close.extend(kit::ring_slot());
    // States last: a resting dark twin matches in every state.
    close.extend(kit::hover_ink(LIGHT_QT2, DARK_QT2));
    close.extend(kit::hover_bg(DIALOG_QUIET_WASH_LIGHT, DIALOG_QUIET_WASH_DARK));
    close.extend(kit::focus_ring(LIGHT_ACC, DARK_GLOW));

    d::DialogSkin {
        theme: super::UiTheme::Flora,
        panel: CssPropertyWithConditionsVec::from_vec(panel),
        title: CssPropertyWithConditionsVec::from_vec(title),
        close_row: CssPropertyWithConditionsVec::from_const_slice(d::DIALOG_CLOSE_ROW_STYLE),
        close: CssPropertyWithConditionsVec::from_vec(close),
        content: CssPropertyWithConditionsVec::from_const_slice(d::DIALOG_CONTENT_STYLE),
        backdrop: d::backdrop_style(DIALOG_BACKDROP),
    }
}

/// Flora's popover panel: a small leaf - `--fl-sur`, `--fl-bd2`, the house
/// radius (`--fl-r`, 3px) and the nearer shadow of `--fl-shadow-2`.
#[must_use]
pub fn popover_panel_style() -> CssPropertyWithConditionsVec {
    use super::style_kit as kit;
    type P = CssPropertyWithConditions;

    // The widget's base (its structure, the same in every theme), then the
    // leaf.
    let mut v = crate::widgets::popover::POPOVER_PANEL_BASE.to_vec();
    v.push(P::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(160))));
    v.extend(kit::padding(8, 8, 8, 8));
    v.extend(kit::border(kit::Edges::ALL, 1, LIGHT_BD2, DARK_BD2));
    v.extend(kit::radius(3));
    v.extend(kit::themed_bg(LIGHT_SUR, DARK_SUR));
    v.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
    v.extend(kit::drop_shadow(2, 5, POPOVER_SHADOW_LIGHT, POPOVER_SHADOW_DARK));
    CssPropertyWithConditionsVec::from_vec(v)
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
    let mut skin = dialog_skin();
    skin.panel = popover_panel_style();
    p.build(skin)
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

    use super::style_kit as kit;
    type P = CssPropertyWithConditions;

    n.text_input.set_theme(super::UiTheme::Flora);
    // A caller who styled the field chose every property of it; the theme
    // adds to its OWN default only (as `button` does).
    let owns_field = n.text_input.container_style.is_none();
    if owns_field {
        let mut field = crate::widgets::text_input::TEXT_INPUT_CONTAINER_PROPS.to_vec();
        field.push(P::simple(kit::bg(LIGHT_FLD)));
        field.push(P::simple(kit::ink(LIGHT_INK)));
        field.extend(kit::border(kit::Edges::ALL, 1, LIGHT_BD2, DARK_BD));
        field.extend(kit::radius(3));
        field.extend(kit::inset_shadow(
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
        label.push(P::simple(kit::ink(LIGHT_INK)));
        n.text_input.label_style =
            OptionCssPropertyWithConditionsVec::Some(CssPropertyWithConditionsVec::from_vec(label));
    }
    let mut dom = n.build();
    if owns_field {
        for p in kit::focus_ring(LIGHT_ACC, DARK_GLOW) {
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
    use super::style_kit as kit;
    use crate::widgets::pagination::{PageFace, PAGINATION_BUTTON_BASE};
    type P = CssPropertyWithConditions;

    let mut v = PAGINATION_BUTTON_BASE.to_vec();
    v.extend([
        P::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(36))),
        kit::font_size(13),
        P::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    ]);
    v.extend(kit::padding(6, 12, 6, 12));
    // Joined: every button draws top, bottom and right; only the first draws
    // a left edge, so neighbours share one hairline.
    let edges = kit::Edges {
        top: true,
        right: true,
        bottom: true,
        left: is_first,
    };
    v.extend(kit::border(edges, 1, LIGHT_BD2, DARK_BD2));
    if is_first {
        v.extend(kit::radius_corners(3, 0, 0, 3));
    }
    if is_last {
        v.extend(kit::radius_corners(0, 3, 3, 0));
    }
    match face {
        PageFace::Neutral => {
            v.extend(kit::themed_layers(
                vec![RAISED_FACE_LIGHT],
                vec![RAISED_FACE_DARK],
            ));
            v.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
        }
        PageFace::Disabled => {
            v.extend(kit::themed_bg(LIGHT_DISBG, DARK_DISBG));
            v.extend(kit::themed_ink(LIGHT_DISTX, DARK_DISTX));
        }
        PageFace::Current => {
            v.push(P::simple(kit::layers(selected_stone())));
            v.push(P::simple(kit::ink(LIGHT_ON_ACC)));
        }
    }
    // States last: a resting dark twin matches in every state.
    if face == PageFace::Neutral {
        v.extend(kit::hover_layers(
            vec![HOVER_FACE_LIGHT],
            vec![HOVER_FACE_DARK],
        ));
        v.extend(kit::active_layers(
            vec![PRESSED_FACE_LIGHT],
            vec![PRESSED_FACE_DARK],
        ));
    }
    let ring = if face == PageFace::Current {
        LIGHT_GLOW
    } else {
        LIGHT_ACC
    };
    v.extend(kit::focus_shadow_ring(ring, DARK_GLOW));
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
    use super::style_kit as kit;
    use crate::widgets::radio_group as r;
    type P = CssPropertyWithConditions;

    let mut row = r::build_row_style(horizontal).into_library_owned_vec();
    row.extend(kit::padding(1, 4, 1, 2));
    row.extend(kit::radius(3));
    row.extend(kit::ring_slot());
    // States last.
    row.extend(kit::hover_bg(RADIO_GROUP_HOVER_LIGHT, RADIO_GROUP_HOVER_DARK));
    row.extend(kit::focus_ring(LIGHT_ACC, DARK_GLOW));

    // The well: the widget's base (its structure, the same in every theme),
    // the widget's geometry, flora's paper.
    let mut circle = r::RADIO_GROUP_CIRCLE_BASE.to_vec();
    circle.extend([
        P::simple(CssProperty::const_width(LayoutWidth::const_px(r::CIRCLE_SIZE))),
        P::simple(CssProperty::const_height(LayoutHeight::const_px(r::CIRCLE_SIZE))),
    ]);
    circle.extend(kit::border(kit::Edges::ALL, r::CIRCLE_BORDER, LIGHT_BD3, DARK_BD3));
    circle.extend(kit::radius(r::CIRCLE_RADIUS));
    circle.extend(kit::themed_bg(LIGHT_FLD, DARK_FLD));
    // `--fl-well`, the same inset the flora number field is sunk by.
    circle.extend(kit::inset_shadow(
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
            P::simple(kit::layers(vec![
                StyleBackgroundContent::Color(LIGHT_ACC),
                ORB_GLOSS,
            ])),
        ]);
        v.extend(kit::radius(r::DOT_RADIUS));
        v.push(P::simple(CssProperty::const_opacity(StyleOpacity::const_new(
            opacity,
        ))));
        CssPropertyWithConditionsVec::from_vec(v)
    };

    let mut label = r::RADIO_GROUP_LABEL_STYLE.to_vec();
    label.extend(kit::themed_ink(LIGHT_INK, DARK_INK));

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
    use super::style_kit as kit;
    type P = CssPropertyWithConditions;

    let mut v = crate::widgets::segmented::SEGMENT_BASE.to_vec();
    v.extend([
        kit::font_size(13),
        P::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    ]);
    v.extend(kit::padding(6, 12, 6, 12));
    // Joined: only the first segment draws a left edge.
    let edges = kit::Edges {
        top: true,
        right: true,
        bottom: true,
        left: is_first,
    };
    v.extend(kit::border(edges, 1, LIGHT_BD2, DARK_BD2));
    v.extend(kit::radius_corners(
        if is_first { 3 } else { 0 },
        if is_last { 3 } else { 0 },
        if is_last { 3 } else { 0 },
        if is_first { 3 } else { 0 },
    ));
    if selected {
        v.push(P::simple(kit::layers(selected_stone())));
        v.push(P::simple(kit::ink(LIGHT_ON_ACC)));
    } else {
        v.extend(kit::themed_layers(
            vec![RAISED_FACE_LIGHT],
            vec![RAISED_FACE_DARK],
        ));
        v.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
        // States last: a resting dark twin matches in every state.
        v.extend(kit::hover_layers(
            vec![HOVER_FACE_LIGHT],
            vec![HOVER_FACE_DARK],
        ));
        v.extend(kit::active_layers(
            vec![PRESSED_FACE_LIGHT],
            vec![PRESSED_FACE_DARK],
        ));
    }
    let ring = if selected { LIGHT_GLOW } else { LIGHT_ACC };
    v.extend(kit::focus_shadow_ring(ring, DARK_GLOW));
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
    use super::style_kit as kit;
    use crate::widgets::split_pane::{self as s, SplitDirection};
    type P = CssPropertyWithConditions;

    // The hairlines run along the bar's long sides.
    let edges = match direction {
        SplitDirection::Horizontal => kit::Edges {
            top: false,
            right: true,
            bottom: false,
            left: true,
        },
        SplitDirection::Vertical => kit::Edges {
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
    divider.extend(kit::border(edges, 1, LIGHT_BD, DARK_BD));
    divider.extend(kit::themed_bg(LIGHT_STRIP, DARK_STRIP));
    // States last: a resting dark twin matches in every state.
    divider.extend(kit::hover_bg(LIGHT_HB, DARK_HB));
    divider.extend(kit::active_bg(LIGHT_PT, DARK_PT));
    divider.extend(kit::focus_shadow_ring(LIGHT_ACC, DARK_GLOW));

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
    use super::style_kit as kit;
    let mut v = crate::widgets::stepper::STEPPER_STEP_STYLE.to_vec();
    v.extend(kit::padding(2, 2, 4, 2));
    v.extend(kit::radius(3));
    v.extend(kit::ring_slot());
    // States last.
    v.extend(kit::hover_bg(RADIO_GROUP_HOVER_LIGHT, RADIO_GROUP_HOVER_DARK));
    v.extend(kit::focus_ring(LIGHT_ACC, DARK_GLOW));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn stepper_circle(reached: bool) -> CssPropertyWithConditionsVec {
    use super::style_kit as kit;
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
        kit::font_size(13),
        P::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    ]);
    v.extend(kit::radius(s::CIRCLE_RADIUS));
    v.extend(kit::border(kit::Edges::ALL, 1, LIGHT_BD2, DARK_BD2));
    if reached {
        v.push(P::simple(kit::layers(stone_face(LIGHT_ACC, STONE_STREAK))));
        v.push(P::simple(kit::ink(LIGHT_ON_ACC)));
    } else {
        v.extend(kit::themed_layers(
            vec![RAISED_FACE_LIGHT],
            vec![RAISED_FACE_DARK],
        ));
        v.extend(kit::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    }
    CssPropertyWithConditionsVec::from_vec(v)
}

fn stepper_connector(fill: crate::widgets::stepper::ConnFill) -> CssPropertyWithConditionsVec {
    use super::style_kit as kit;
    use crate::widgets::stepper::{self as s, ConnFill};
    type P = CssPropertyWithConditions;

    let mut v = s::CONNECTOR_BASE.to_vec();
    v.push(P::simple(CssProperty::const_height(LayoutHeight::const_px(
        s::CONNECTOR_HEIGHT,
    ))));
    match fill {
        ConnFill::Accent => v.push(P::simple(kit::bg(LIGHT_ACC))),
        ConnFill::Muted => v.extend(kit::themed_bg(LIGHT_BD, DARK_BD)),
        ConnFill::Hidden => v.push(P::simple(kit::bg(ColorU::TRANSPARENT))),
    }
    CssPropertyWithConditionsVec::from_vec(v)
}

fn stepper_label(reached: bool) -> CssPropertyWithConditionsVec {
    use super::style_kit as kit;
    type P = CssPropertyWithConditions;

    let mut v = crate::widgets::stepper::LABEL_BASE.to_vec();
    v.extend([
        kit::font_size(12),
        P::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        P::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(6))),
    ]);
    if reached {
        v.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
    } else {
        v.extend(kit::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
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
    use super::style_kit as kit;
    use crate::widgets::time_picker as t;
    type P = CssPropertyWithConditions;

    // Every part is the widget's base (`time_picker::CONTAINER_BASE`,
    // `CLICKABLE_BASE`, `READOUT_BASE`: its structure), then flora's skin.
    let mut container = t::CONTAINER_BASE.to_vec();
    container.extend(kit::padding(4, 6, 4, 6));
    container.extend(kit::border(kit::Edges::ALL, 1, LIGHT_BD2, DARK_BD2));
    container.extend(kit::radius(3));
    container.extend(kit::themed_bg(LIGHT_FLD, DARK_FLD));
    container.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
    container.extend(kit::inset_shadow(
        1,
        2,
        NUMBER_INPUT_WELL_LIGHT,
        NUMBER_INPUT_WELL_DARK,
    ));

    let mut arrow = t::CLICKABLE_BASE.to_vec();
    arrow.extend([
        P::simple(CssProperty::const_width(LayoutWidth::const_px(40))),
        P::simple(CssProperty::const_height(LayoutHeight::const_px(16))),
        kit::font_size(11),
        P::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        P::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(2))),
        P::simple(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(2))),
    ]);
    arrow.extend(kit::radius(3));
    arrow.extend(kit::themed_ink(LIGHT_ICON, DARK_ICON));
    // States last.
    arrow.extend(kit::hover_layers(
        vec![HOVER_FACE_LIGHT],
        vec![HOVER_FACE_DARK],
    ));
    arrow.extend(kit::hover_ink(LIGHT_INK, DARK_INK));
    arrow.extend(kit::active_layers(
        vec![PRESSED_FACE_LIGHT],
        vec![PRESSED_FACE_DARK],
    ));

    // The column is the spin button: its base, then its focus ring.
    let mut spinner = t::SPINNER_STYLE.to_vec();
    spinner.extend(kit::radius(3));
    spinner.extend(kit::focus_shadow_ring(LIGHT_ACC, DARK_GLOW));

    let mut display = t::READOUT_BASE.to_vec();
    display.extend([
        kit::font_size(18),
        P::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        P::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(2))),
        P::simple(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(2))),
    ]);
    display.extend(kit::themed_ink(LIGHT_INK, DARK_INK));

    let mut separator = t::READOUT_BASE.to_vec();
    separator.extend([
        kit::font_size(18),
        P::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(2))),
        P::simple(CssProperty::const_padding_right(LayoutPaddingRight::const_px(2))),
    ]);
    separator.extend(kit::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    let mut ampm = t::CLICKABLE_BASE.to_vec();
    ampm.extend([
        kit::font_size(13),
        P::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        P::simple(CssProperty::const_margin_left(LayoutMarginLeft::const_px(8))),
    ]);
    ampm.extend(kit::padding(4, 8, 4, 8));
    ampm.extend(kit::radius(3));
    ampm.extend(kit::border(kit::Edges::ALL, 1, LIGHT_BD2, DARK_BD2));
    ampm.extend(kit::themed_layers(
        vec![RAISED_FACE_LIGHT],
        vec![RAISED_FACE_DARK],
    ));
    ampm.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
    // States last.
    ampm.extend(kit::hover_layers(
        vec![HOVER_FACE_LIGHT],
        vec![HOVER_FACE_DARK],
    ));
    ampm.extend(kit::active_layers(
        vec![PRESSED_FACE_LIGHT],
        vec![PRESSED_FACE_DARK],
    ));
    ampm.extend(kit::focus_ring(LIGHT_ACC, DARK_GLOW));

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
    use super::style_kit as kit;
    use crate::widgets::toast as t;
    type P = CssPropertyWithConditions;

    // The widget's close base (`toast::TOAST_CLOSE_BASE`), then flora's skin.
    let mut close = t::TOAST_CLOSE_BASE.to_vec();
    close.extend([
        kit::font_size(18),
        P::simple(CssProperty::const_margin_left(LayoutMarginLeft::const_px(12))),
    ]);
    close.extend(kit::padding(0, 5, 0, 5));
    close.extend(kit::radius(3));
    close.extend(kit::themed_ink(LIGHT_QT, DARK_QT));
    close.extend(kit::ring_slot());
    // States last.
    close.extend(kit::hover_ink(LIGHT_QT2, DARK_QT2));
    close.extend(kit::hover_bg(DIALOG_QUIET_WASH_LIGHT, DIALOG_QUIET_WASH_DARK));
    close.extend(kit::focus_ring(LIGHT_ACC, DARK_GLOW));

    t::ToastSkin {
        theme: super::UiTheme::Flora,
        container: toast_container,
        message: CssPropertyWithConditionsVec::from_const_slice(t::TOAST_MESSAGE_STYLE),
        close: CssPropertyWithConditionsVec::from_vec(close),
    }
}

/// The flora card for a kind: a leaf with the kind's thread in its margin.
fn toast_container(kind: crate::widgets::toast::ToastKind) -> CssPropertyWithConditionsVec {
    use super::style_kit as kit;
    use crate::widgets::toast as t;
    type P = CssPropertyWithConditions;

    let (thread, thread_dark) = toast_thread(kind);
    // The card's structure and placement - where every theme's toast floats,
    // the positioned parent's corner - is the widget's (`TOAST_CARD_BASE`).
    let mut v = t::TOAST_CARD_BASE.to_vec();
    v.extend([
        P::simple(CssProperty::const_max_width(LayoutMaxWidth::const_px(t::TOAST_MAX_WIDTH))),
        kit::font_size(14),
        P::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ]);
    v.extend(kit::padding(12, 12, 12, 14));
    let hairline = kit::Edges {
        top: true,
        right: true,
        bottom: true,
        left: false,
    };
    v.extend(kit::border(hairline, 1, LIGHT_BD2, DARK_BD2));
    let margin = kit::Edges {
        top: false,
        right: false,
        bottom: false,
        left: true,
    };
    v.extend(kit::border(margin, 3, thread, thread_dark));
    v.extend(kit::radius(3));
    v.extend(kit::themed_bg(LIGHT_SUR, DARK_SUR));
    v.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
    v.extend(kit::drop_shadow(
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
// A flora tip is marginalia set as flora sets code: an INK PANEL on the page
// (`--fl-code-bg` under `--fl-code-fg`, a `--fl-code-bd` hairline), the house
// radius and the nearest shadow (`--fl-shadow-1`) - an ink panel in both modes,
// each with its night value. It keeps the widget's placement and starts hidden,
// so the enter / leave handlers work unchanged.

/// `--fl-code-bg` by day / by night.
const TOOLTIP_INK_BG: (ColorU, ColorU) = (ColorU::new(33, 31, 27, 255), ColorU::new(20, 20, 20, 255));
/// `--fl-code-fg` by day / by night.
const TOOLTIP_INK_FG: (ColorU, ColorU) =
    (ColorU::new(228, 225, 214, 255), ColorU::new(226, 226, 226, 255));
/// `--fl-code-bd` by day / by night.
const TOOLTIP_INK_BD: (ColorU, ColorU) = (ColorU::new(68, 63, 53, 255), ColorU::new(54, 54, 54, 255));
/// `--fl-shadow-1` by day (rgba(48, 45, 38, 0.14)) / by night (rgba(0, 0, 0, 0.55)).
const TOOLTIP_SHADOW: (ColorU, ColorU) = (ColorU::new(48, 45, 38, 36), ColorU::new(0, 0, 0, 140));

/// Flora's tooltip skin.
#[must_use]
pub(crate) fn tooltip_skin() -> crate::widgets::tooltip::TooltipSkin {
    use super::style_kit as kit;
    use crate::widgets::tooltip as t;
    // The widget's tip base (`tooltip::TIP_BASE`: placed below the wrapper,
    // on one line, hidden until hovered - the value the leave handler writes
    // back), then flora's ink panel.
    let mut tip = t::TIP_BASE.to_vec();
    tip.push(kit::font_size(12));
    tip.extend(kit::padding(4, 8, 4, 8));
    tip.extend(kit::radius(3));
    tip.extend(kit::border(
        kit::Edges::ALL,
        1,
        TOOLTIP_INK_BD.0,
        TOOLTIP_INK_BD.1,
    ));
    tip.extend(kit::themed_bg(TOOLTIP_INK_BG.0, TOOLTIP_INK_BG.1));
    tip.extend(kit::themed_ink(TOOLTIP_INK_FG.0, TOOLTIP_INK_FG.1));
    tip.extend(kit::drop_shadow(1, 2, TOOLTIP_SHADOW.0, TOOLTIP_SHADOW.1));

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
// poster. Flora draws it as the ink panel it sets code and tooltips in
// (`--fl-code-bg` under a `--fl-code-bd` hairline, by day and by night) - a
// screen reads as ink on the page in both modes.

/// Flora's "no signal" poster.
#[must_use]
pub(crate) fn video_poster_style() -> CssPropertyWithConditionsVec {
    use super::style_kit as kit;

    let mut v = kit::fill().to_vec();
    v.extend(kit::themed_bg(TOOLTIP_INK_BG.0, TOOLTIP_INK_BG.1));
    v.extend(kit::border(
        kit::Edges::ALL,
        1,
        TOOLTIP_INK_BD.0,
        TOOLTIP_INK_BD.1,
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

    // `.fl-label`: font-weight 700, letter-spacing 0.12em, --fl-soft1.
    let mut title = FRAME_TITLE_STYLE.to_vec();
    title.push(decl::bold());
    title.push(decl::letter_spacing_em(0.12));
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
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
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

    crate::widgets::date_picker::build(d, &look)
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
    use super::style_kit as kit;
    use crate::widgets::combobox as c;
    type P = CssPropertyWithConditions;

    // The field: the widget's structure (R5), flora's field paper sunk in
    // its well.
    let mut field = c::COMBOBOX_FIELD_BASE.to_vec();
    field.extend(kit::padding(3, 4, 3, 4));
    field.extend(kit::border(kit::Edges::ALL, 1, LIGHT_BD2, DARK_BD));
    field.extend(kit::radius(3));
    field.extend(kit::themed_bg(LIGHT_FLD, DARK_SUR));
    field.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
    field.extend(kit::inset_shadow(
        1,
        2,
        NUMBER_INPUT_WELL_LIGHT,
        NUMBER_INPUT_WELL_DARK,
    ));
    // States last: a resting dark twin matches in every state.
    field.extend(kit::focus_ring(LIGHT_ACC, DARK_GLOW));

    let mut arrow = c::COMBOBOX_ARROW_STYLE.to_vec();
    arrow.extend(kit::themed_ink(LIGHT_ICON, DARK_ICON));

    // The list: the widget's structure, then a small leaf, square along the
    // field.
    let mut list = c::COMBOBOX_LIST_BASE.to_vec();
    list.push(P::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(c::MIN_WIDTH))));
    list.extend(kit::border(kit::Edges::ALL, 1, LIGHT_BD2, DARK_BD2));
    list.extend(kit::radius_corners(0, 0, 3, 3));
    list.extend(kit::themed_bg(LIGHT_SUR, DARK_SUR));
    list.extend(kit::drop_shadow(2, 5, POPOVER_SHADOW_LIGHT, POPOVER_SHADOW_DARK));

    let mut option = c::COMBOBOX_OPTION_BASE.to_vec();
    option.extend(kit::padding(6, 10, 6, 10));
    option.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
    // States last.
    option.extend(kit::hover_bg(RADIO_GROUP_HOVER_LIGHT, RADIO_GROUP_HOVER_DARK));
    option.extend(kit::focus_shadow_ring(LIGHT_ACC, DARK_GLOW));

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
    use super::style_kit as kit;
    v.extend(kit::hover_layers(
        vec![HOVER_FACE_LIGHT],
        vec![HOVER_FACE_DARK],
    ));
    v.extend(kit::hover_border(LIGHT_BD, DARK_BD));
}

/// A toolbar key's states: [`chrome_lift`] under the pointer, the pressed
/// face (`--fl-pT` -> `--fl-pB`) while held, and the focus ring - the accent
/// by day, the stone's glow by night (flora.css `--focus-color`). Appended
/// again after any resting face a part lays over the key (a toggled button,
/// the active view), so that face never shadows them.
fn chrome_key_states(v: &mut Vec<CssPropertyWithConditions>) {
    use super::style_kit as kit;
    chrome_lift(v);
    v.extend(kit::active_layers(
        vec![PRESSED_FACE_LIGHT],
        vec![PRESSED_FACE_DARK],
    ));
    v.extend(kit::focus_ring(LIGHT_ACC, DARK_GLOW));
}

/// A flora toolbar key (`.nav-links a`, cut for the chrome): bare at rest - a
/// transparent face in a transparent hairline, so the strip shows through -
/// with the house radius, then [`chrome_key_states`]. The key keeps the 1px
/// border its geometry has; the hover and the ring colour it.
fn chrome_key(v: &mut Vec<CssPropertyWithConditions>) {
    use super::style_kit as kit;
    v.extend(kit::radius(3));
    v.push(CssPropertyWithConditions::simple(kit::bg(ColorU::TRANSPARENT)));
    v.extend(
        super::decl::border_colors(ColorU::TRANSPARENT).map(CssPropertyWithConditions::simple),
    );
    chrome_key_states(v);
}

/// A floating chrome leaf (the gallery's expansion panel, the touch tab
/// picker): the popover's small leaf - `--fl-sur` in a `--fl-bd2` hairline,
/// the house radius, the nearer shadow of `--fl-shadow-2`.
fn chrome_leaf(v: &mut Vec<CssPropertyWithConditions>) {
    use super::{decl, style_kit as kit};
    v.extend(kit::radius(3));
    v.extend(kit::themed_bg(LIGHT_SUR, DARK_SUR));
    v.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD2));
    v.extend(kit::drop_shadow(
        2,
        5,
        POPOVER_SHADOW_LIGHT,
        POPOVER_SHADOW_DARK,
    ));
}

// ==== ribbon ====
//
// A flora ribbon is flora's toolbar strip (`--fl-strip`, closed along its foot
// by a `--fl-bd` rule) over a leaf (`--fl-sur`) that holds the groups, each
// ruled off from the next by a `--fl-sep` hairline and captioned in soft ink
// (`--fl-soft1`). Its tabs are flora's nav tabs (`.nav-links a`): soft ink on
// the strip, square-shouldered at the foot, lifting to the hover face and the
// house ink under the pointer; the selected tab is the sunken accent stone
// (`.nav-links a.active`: `--fl-gem-sunken` under the sunken rig, set in its
// `--fl-deep` edge) written in `--fl-on-acc` - its own colour by day and by
// night. The application button is the raised accent stone of a primary
// command (`.btn-primary`: the accent under the depth rig and its streak).
// Every command is a toolbar key: bare paper at rest, the hover face in a
// hairline under the pointer, the pressed face while held, ringed on focus; a
// toggled one stays pushed in, in a `--fl-bd3` hairline. The gallery is a well
// of field paper (`--fl-fld` in `--fl-bd2`, sunk by `--fl-well`) whose picked
// cell is washed in the accent's soft tint (`--fl-soft`; by night the lifted
// face `--fl-hT`, a light tint being a light island there) and rimmed in the
// accent; its expansion panel and the touch chrome's tab picker are popover
// leaves. Labels are `--fl-ink`, glyphs `--fl-icon`, chevrons and captions
// `--fl-soft1` / `--fl-soft2`; the accent as text is `--fl-acc` by day and
// `--fl-glow` by night. The touch chrome's picked group is the sunken stone,
// its own colour in either mode.

/// Flora's ribbon: every part the caller left `None` in `s` filled with
/// flora's paint on the flat part's geometry (see the chrome section above).
#[must_use]
pub(crate) fn ribbon_style(
    mut s: crate::widgets::ribbon::RibbonStyle,
) -> crate::widgets::ribbon::RibbonStyle {
    use super::{decl, style_kit as kit};
    type P = CssPropertyWithConditions;

    let e = s.resolved_container_style();
    chrome_part(&mut s.container_style, &e, |v| {
        v.extend(kit::themed_bg(LIGHT_STRIP, DARK_STRIP));
        v.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
        v.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));
    });
    let e = s.resolved_tab_bar_style();
    chrome_part(&mut s.tab_bar_style, &e, |v| {
        v.extend(kit::themed_bg(LIGHT_STRIP, DARK_STRIP));
    });
    let e = s.resolved_app_button_style();
    chrome_part(&mut s.app_button_style, &e, |v| {
        v.extend(kit::radius_corners(3, 3, 0, 0));
        v.push(P::simple(kit::layers(stone_face(LIGHT_ACC, STONE_STREAK))));
        v.push(P::simple(kit::ink(LIGHT_ON_ACC)));
        // A stone is its own colour in both modes, so its states repeat for
        // the night: every state rule keeps its twin.
        let lit = stone_face(LIGHT_ACC, STONE_STREAK_HOVER);
        v.extend(kit::hover_layers(lit.clone(), lit));
        let held = sunken_stone_face(LIGHT_DEEP);
        v.extend(kit::active_layers(held.clone(), held));
    });
    let e = s.resolved_tab_style();
    chrome_part(&mut s.tab_style, &e, |v| {
        v.extend(kit::radius_corners(4, 4, 0, 0));
        v.extend(kit::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
        v.push(P::simple(kit::bg(ColorU::TRANSPARENT)));
        // The strip's rule runs across an unselected tab's foot.
        v.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));
        v.extend(kit::hover_layers(
            vec![HOVER_FACE_LIGHT],
            vec![HOVER_FACE_DARK],
        ));
        v.extend(kit::hover_ink(LIGHT_INK, DARK_INK));
        v.extend(kit::active_layers(
            vec![PRESSED_FACE_LIGHT],
            vec![PRESSED_FACE_DARK],
        ));
    });
    let e = s.resolved_tab_active_style();
    chrome_part(&mut s.tab_active_style, &e, |v| {
        v.extend(kit::radius_corners(4, 4, 0, 0));
        v.push(P::simple(kit::layers(selected_stone())));
        v.push(P::simple(kit::ink(LIGHT_ON_ACC)));
        v.extend(decl::border_colors(LIGHT_DEEP).map(P::simple));
    });
    let e = s.resolved_tab_filler_style();
    chrome_part(&mut s.tab_filler_style, &e, |v| {
        v.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));
    });
    let e = s.resolved_content_style();
    chrome_part(&mut s.content_style, &e, |v| {
        v.extend(kit::themed_bg(LIGHT_SUR, DARK_SUR));
    });
    let e = s.resolved_group_style();
    chrome_part(&mut s.group_style, &e, |v| {
        v.extend(decl::themed_border_right_color(LIGHT_SEP, DARK_SEP));
    });
    let e = s.resolved_group_label_style();
    chrome_part(&mut s.group_label_style, &e, |v| {
        v.extend(kit::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    });
    let e = s.resolved_launcher_button_style();
    chrome_part(&mut s.launcher_button_style, &e, chrome_key);
    let e = s.resolved_launcher_icon_style();
    chrome_part(&mut s.launcher_icon_style, &e, |v| {
        v.extend(kit::themed_ink(LIGHT_SOFT2, DARK_SOFT2));
    });
    let e = s.resolved_separator_style();
    chrome_part(&mut s.separator_style, &e, |v| {
        v.extend(kit::themed_bg(LIGHT_SEP, DARK_SEP));
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
        chrome_part(slot, &e, |v| v.extend(kit::themed_ink(LIGHT_ICON, DARK_ICON)));
    }
    let e = s.resolved_large_label_style();
    chrome_part(&mut s.large_label_style, &e, |v| {
        v.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
    });
    let e = s.resolved_small_label_style();
    chrome_part(&mut s.small_label_style, &e, |v| {
        v.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
    });
    let e = s.resolved_arrow_icon_style();
    chrome_part(&mut s.arrow_icon_style, &e, |v| {
        v.extend(kit::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    });
    // APPENDED to a toggled button's key: pushed-in paper, the key's states
    // after it again.
    let e = s.resolved_checked_style();
    chrome_part(&mut s.checked_style, &e, |v| {
        v.extend(kit::themed_layers(
            vec![PRESSED_FACE_LIGHT],
            vec![PRESSED_FACE_DARK],
        ));
        v.extend(decl::themed_border_color(LIGHT_BD3, DARK_BD3));
        chrome_key_states(v);
    });
    let e = s.resolved_gallery_frame_style();
    chrome_part(&mut s.gallery_frame_style, &e, |v| {
        v.extend(kit::radius(3));
        v.extend(kit::themed_bg(LIGHT_FLD, DARK_FLD));
        v.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD2));
        v.extend(kit::inset_shadow(
            1,
            2,
            NUMBER_INPUT_WELL_LIGHT,
            NUMBER_INPUT_WELL_DARK,
        ));
    });
    let e = s.resolved_gallery_cell_style();
    chrome_part(&mut s.gallery_cell_style, &e, |v| {
        v.push(P::simple(kit::bg(ColorU::TRANSPARENT)));
        v.extend(decl::border_colors(ColorU::TRANSPARENT).map(P::simple));
        // Cells are divided by a hairline on their right edge.
        v.extend(decl::themed_border_right_color(LIGHT_SEP, DARK_SEP));
        chrome_lift(v);
    });
    // APPENDED to the picked cell: the accent's soft wash, rimmed in the
    // accent, the lift after it again.
    let e = s.resolved_gallery_cell_selected_style();
    chrome_part(&mut s.gallery_cell_selected_style, &e, |v| {
        v.extend(kit::themed_bg(LIGHT_SOFT, DARK_HT));
        v.extend(decl::themed_border_color(LIGHT_ACC, DARK_GLOW));
        chrome_lift(v);
    });
    let e = s.resolved_gallery_cell_label_style();
    chrome_part(&mut s.gallery_cell_label_style, &e, |v| {
        v.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
    });
    let e = s.resolved_gallery_spinner_style();
    chrome_part(&mut s.gallery_spinner_style, &e, |v| {
        v.extend(decl::themed_border_left_color(LIGHT_SEP, DARK_SEP));
    });
    let e = s.resolved_gallery_panel_style();
    chrome_part(&mut s.gallery_panel_style, &e, chrome_leaf);
    // The spinner's buttons have no border to colour: they ring with an
    // inset halo (the frame clips an outer one).
    let e = s.resolved_gallery_spinner_button_style();
    chrome_part(&mut s.gallery_spinner_button_style, &e, |v| {
        v.push(P::simple(kit::bg(ColorU::TRANSPARENT)));
        v.extend(kit::hover_layers(
            vec![HOVER_FACE_LIGHT],
            vec![HOVER_FACE_DARK],
        ));
        v.extend(kit::active_layers(
            vec![PRESSED_FACE_LIGHT],
            vec![PRESSED_FACE_DARK],
        ));
        v.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));
    });
    let e = s.resolved_gallery_spinner_icon_style();
    chrome_part(&mut s.gallery_spinner_icon_style, &e, |v| {
        v.extend(kit::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    });
    let e = s.resolved_mobile_tab_button_style();
    chrome_part(&mut s.mobile_tab_button_style, &e, |v| {
        v.extend(kit::themed_ink(LIGHT_ACC, DARK_GLOW));
        v.extend(kit::themed_bg(LIGHT_STRIP, DARK_STRIP));
        v.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));
    });
    let e = s.resolved_mobile_tab_arrow_style();
    chrome_part(&mut s.mobile_tab_arrow_style, &e, |v| {
        v.extend(kit::themed_ink(LIGHT_ACC, DARK_GLOW));
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
            v.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
            v.extend(decl::themed_border_bottom_color(LIGHT_SEP, DARK_SEP));
            v.extend(kit::hover_layers(
                vec![HOVER_FACE_LIGHT],
                vec![HOVER_FACE_DARK],
            ));
        });
    }
    let e = s.resolved_mobile_group_list_style();
    chrome_part(&mut s.mobile_group_list_style, &e, |v| {
        v.extend(kit::themed_bg(LIGHT_STRIP, DARK_STRIP));
        // The divider sits on whichever side faces the content (the
        // handedness decides which edge has a width); both are coloured.
        v.extend(decl::themed_border_left_color(LIGHT_SEP, DARK_SEP));
        v.extend(decl::themed_border_right_color(LIGHT_SEP, DARK_SEP));
    });
    // APPENDED to the picked group: the sunken stone, which stays the stone
    // under the pointer (the lift would un-pick it).
    let e = s.resolved_mobile_group_list_item_selected_style();
    chrome_part(&mut s.mobile_group_list_item_selected_style, &e, |v| {
        v.push(P::simple(kit::layers(selected_stone())));
        v.push(P::simple(kit::ink(LIGHT_ON_ACC)));
        v.extend(kit::hover_layers(selected_stone(), selected_stone()));
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
    use super::{decl, style_kit as kit};
    type P = CssPropertyWithConditions;

    let e = s.resolved_bar_style();
    chrome_part(&mut s.bar_style, &e, |v| {
        v.extend(kit::themed_bg(LIGHT_STRIP, DARK_STRIP));
        v.extend(kit::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
        // The hairline along the top: an inset line, so it costs no height.
        v.extend(kit::inset_shadow(1, 0, LIGHT_BD, DARK_BD));
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
        chrome_part(slot, &e, |v| v.extend(kit::themed_ink(LIGHT_ICON, DARK_ICON)));
    }
    let e = s.resolved_segment_label_style();
    chrome_part(&mut s.segment_label_style, &e, |v| {
        v.extend(kit::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    });
    // APPENDED to the active view's key: pushed-in paper, the key's states
    // after it again.
    let e = s.resolved_view_button_active_style();
    chrome_part(&mut s.view_button_active_style, &e, |v| {
        v.extend(kit::themed_layers(
            vec![PRESSED_FACE_LIGHT],
            vec![PRESSED_FACE_DARK],
        ));
        v.extend(decl::themed_border_color(LIGHT_BD3, DARK_BD3));
        chrome_key_states(v);
    });
    let (rail, tick) = (s.resolved_zoom_rail_style(), s.resolved_zoom_tick_style());
    for (slot, e) in [(&mut s.zoom_rail_style, rail), (&mut s.zoom_tick_style, tick)] {
        chrome_part(slot, &e, |v| v.extend(kit::themed_bg(LIGHT_BD3, DARK_BD3)));
    }
    // The slider's hit area stays transparent (the rail is drawn by its host)
    // and rings with an inset halo: it has no border to colour.
    let e = s.resolved_slider_track_style();
    chrome_part(&mut s.slider_track_style, &e, |v| {
        v.push(P::simple(kit::bg(ColorU::TRANSPARENT)));
        v.extend(decl::focus_halo_inset(LIGHT_ACC, DARK_GLOW));
    });
    let e = s.resolved_slider_thumb_style();
    chrome_part(&mut s.slider_thumb_style, &e, |v| {
        v.extend(kit::radius(2));
        v.extend(kit::themed_layers(
            vec![RAISED_FACE_LIGHT],
            vec![RAISED_FACE_DARK],
        ));
        v.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD2));
    });
    let e = s.resolved_zoom_label_style();
    chrome_part(&mut s.zoom_label_style, &e, |v| {
        v.extend(kit::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
        chrome_key(v);
    });
    // The sync indicator's glyph when the sync failed: the clay stone, lifted
    // to its glow at night (the alert's danger thread).
    let e = s.resolved_sync_icon_error_style();
    chrome_part(&mut s.sync_icon_error_style, &e, |v| {
        v.extend(kit::themed_ink(STONE_CLAY.stone, STONE_CLAY.glow));
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
    use super::style_kit as kit;

    let e = s.resolved_bar_style();
    chrome_part(&mut s.bar_style, &e, |v| {
        v.extend(kit::themed_bg(LIGHT_DESK, DARK_DESK));
        v.extend(kit::themed_ink(LIGHT_INTRO, DARK_INTRO));
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
        chrome_part(slot, &e, |v| v.extend(kit::themed_ink(LIGHT_ICON, DARK_ICON)));
    }
    let e = s.resolved_menu_arrow_style();
    chrome_part(&mut s.menu_arrow_style, &e, |v| {
        v.extend(kit::themed_ink(LIGHT_SOFT2, DARK_SOFT2));
    });
    let e = s.resolved_title_style();
    chrome_part(&mut s.title_style, &e, |v| {
        v.extend(kit::themed_ink(LIGHT_INTRO, DARK_INTRO));
    });
    // APPENDED to the close key's window key: clay under the pointer and while
    // held, then the ring again so the clay rim never hides it.
    let e = s.resolved_close_button_style();
    chrome_part(&mut s.close_button_style, &e, |v| {
        v.extend(kit::hover_bg(STONE_CLAY.soft, STONE_CLAY.deep));
        v.extend(kit::hover_border(STONE_CLAY.glow, STONE_CLAY.glow));
        v.extend(kit::active_bg(STONE_CLAY.glow, STONE_CLAY.stone));
        v.extend(kit::focus_ring(LIGHT_ACC, DARK_GLOW));
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
    use super::style_kit as kit;
    use crate::widgets::tree_view as t;
    type P = CssPropertyWithConditions;
    let part = CssPropertyWithConditionsVec::from_vec;

    // Every part is the widget's base (`tree_view::TREE_CONTAINER_BASE`,
    // `ROW_BASE`, `CHILDREN_BASE`, `ICON_BASE`, `LABEL_BASE`: its
    // structure), then flora's skin.
    let mut container = t::TREE_CONTAINER_BASE.to_vec();
    container.extend([
        kit::font_size(13),
        P::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ]);
    container.extend(kit::padding(3, 3, 3, 3));
    container.extend(kit::border(kit::Edges::ALL, 1, LIGHT_BD2, DARK_BD2));
    container.extend(kit::radius(3));
    container.extend(kit::themed_bg(LIGHT_FLD, DARK_FLD));
    container.extend(kit::themed_ink(LIGHT_INK, DARK_INK));

    // A row's box, selected or not.
    let row_box = || {
        let mut v = t::ROW_BASE.to_vec();
        v.extend(kit::padding(3, 6, 3, 6));
        v.extend(kit::radius(3));
        v
    };
    let mut row = row_box();
    // States last: a resting dark twin matches in every state.
    row.extend(kit::hover_bg(RADIO_GROUP_HOVER_LIGHT, RADIO_GROUP_HOVER_DARK));
    row.extend(kit::active_layers(
        vec![PRESSED_FACE_LIGHT],
        vec![PRESSED_FACE_DARK],
    ));
    row.extend(kit::focus_shadow_ring(LIGHT_ACC, DARK_GLOW));

    let mut row_selected = row_box();
    row_selected.push(P::simple(kit::layers(selected_stone())));
    row_selected.push(P::simple(kit::ink(LIGHT_ON_ACC)));
    row_selected.extend(kit::focus_shadow_ring(LIGHT_GLOW, DARK_GLOW));

    // The guide rule sits under the parent's chevron (6px row padding + half
    // the 16px icon column); margin + rule + padding keep the 16px indent.
    let mut children = t::CHILDREN_BASE.to_vec();
    children.extend([
        P::simple(CssProperty::const_margin_left(LayoutMarginLeft::const_px(13))),
        P::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(2))),
    ]);
    let guide = kit::Edges {
        top: false,
        right: false,
        bottom: false,
        left: true,
    };
    children.extend(kit::border(guide, 1, LIGHT_SEP, DARK_SEP));

    // The chevron: the flat tree's 16px column, flora's icon ink - or, on the
    // stone, the stone's ink.
    let icon = |ink: Vec<P>| {
        let mut v = t::ICON_BASE.to_vec();
        v.push(kit::font_size(16));
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
        icon: part(icon(kit::themed_ink(LIGHT_ICON, DARK_ICON).to_vec())),
        icon_selected: part(icon(vec![P::simple(kit::ink(LIGHT_ON_ACC))])),
        leaf_spacer: CssPropertyWithConditionsVec::from_const_slice(t::LEAF_SPACER_STYLE),
        label: part(label(kit::themed_ink(LIGHT_INK, DARK_INK).to_vec())),
        label_selected: part(label(vec![P::simple(kit::ink(LIGHT_ON_ACC))])),
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

// ==== tabs ====
//
// A flora tab bar is flora's navigation strip (`.navbar`, `.nav-links a`):
// raised CHROME (`--fl-rT` over `--fl-rB` - chrome is a smooth face, no
// grain) closed along its foot by a 2px rule of metal. The unselected tabs
// sit BEHIND the rule - it runs across their feet - written in `--fl-soft1`,
// with 4px shoulders and an invisible 1px edge on their other three sides;
// under the pointer they lift to the hover face, the edge turns `--fl-bd` and
// the label darkens to `--fl-ink`; pressed, they sink to the pressed face in
// a `--fl-bd3` edge. The SELECTED tab is the sunken accent stone
// (`selected_stone`) in `--fl-on-acc`, cut from the rule's metal on three
// sides at the rule's gauge, with 6px shoulders and no foot: it sits above
// the rule and breaks it, which is what "selected" means in a tab bar. Its
// label sits on the same line as the others' (4px + the 2px rule below an
// unselected label, 6px below the selected one). flora.css's corner assembly
// (flare, cove and run-out - radial masks around the selected tab's foot) is
// not drawn; its metal is `--fl-metal-turn`, the value the ribbon has where
// that corner would be. The tabs start 8px in and the rule runs on to the end
// of the strip. The tab widths and the strip's height follow the label, as
// flora's navigation does, not the flat bar's fixed 21 / 23px. Every tab
// rings on focus with an inset ring: the accent by day, the glow by night and
// on the stone.
//
// The panel the selected tab opens onto is a LEAF (`--fl-sur`) in a `--fl-bd`
// hairline on three sides with the house radius at its foot, open at the top
// where the strip's rule closes it; padded, it breathes 10px.

/// `--fl-metal-turn` (#C6B279): the value flora's ribbon has where the rule
/// that closes a tab strip turns and climbs the selected tab - the one metal
/// the rule and the tab's surround are cut from. Metal is its own colour by
/// day and by night (flora.css does not redefine it for the dark theme).
pub const TAB_METAL: ColorU = ColorU::rgb(0xC6, 0xB2, 0x79);

/// Flora's tab-bar look.
#[must_use]
pub(crate) fn tab_header_look() -> crate::widgets::tabs::TabHeaderLook {
    use super::style_kit as kit;
    type P = CssPropertyWithConditions;
    let part = CssPropertyWithConditionsVec::from_vec;
    // The strip's rule: 2px of metal along a foot.
    let rule = || kit::border(kit::Edges::BOTTOM, 2, TAB_METAL, TAB_METAL);
    let three_sides = kit::Edges {
        top: true,
        right: true,
        bottom: false,
        left: true,
    };

    // Every part is the widget's base (`tabs::HEADER_BASE`, `AFTER_BASE`,
    // `TAB_BASE`: its structure), then flora's skin.
    let mut header = crate::widgets::tabs::HEADER_BASE.to_vec();
    header.extend([
        // Flora's own layout: the tabs stand ON the strip's rule. Flat's
        // native tabs hang from the top of the bar (its default).
        P::simple(CssProperty::const_align_items(LayoutAlignItems::End)),
        P::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
        kit::font_size(13),
        P::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(6))),
    ]);
    header.extend(kit::themed_layers(
        vec![RAISED_FACE_LIGHT],
        vec![RAISED_FACE_DARK],
    ));

    // The tabs start 8px in: a fixed spacer (flat's grows).
    let mut before = vec![
        P::simple(CssProperty::const_width(LayoutWidth::const_px(8))),
        P::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    ];
    before.extend(rule());
    let mut after = crate::widgets::tabs::AFTER_BASE.to_vec();
    after.extend(rule());

    // A tab's box: the widget's tab base (the pointer among it), then the
    // centred label and the gap to the next tab.
    let tab_box = |top: isize, bottom: isize| {
        let mut v = crate::widgets::tabs::TAB_BASE.to_vec();
        v.extend([
            P::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
            P::simple(CssProperty::const_margin_right(LayoutMarginRight::const_px(2))),
        ]);
        v.extend(kit::padding(top, 12, bottom, 12));
        v
    };

    // An unselected tab, behind the rule.
    let mut tab = tab_box(3, 4);
    tab.extend(kit::border(
        three_sides,
        1,
        ColorU::TRANSPARENT,
        ColorU::TRANSPARENT,
    ));
    tab.extend(rule());
    tab.extend(kit::radius_corners(4, 4, 0, 0));
    tab.extend(kit::themed_ink(LIGHT_SOFT1, DARK_SOFT1));
    // States last: a resting dark twin matches in every state. The edge
    // states colour the three edges the tab owns - never the foot, which is
    // the strip's rule.
    let edges = |light: ColorU, dark: ColorU| {
        [
            (
                CssProperty::const_border_top_color(StyleBorderTopColor { inner: light }),
                CssProperty::const_border_top_color(StyleBorderTopColor { inner: dark }),
            ),
            (
                CssProperty::const_border_right_color(StyleBorderRightColor { inner: light }),
                CssProperty::const_border_right_color(StyleBorderRightColor { inner: dark }),
            ),
            (
                CssProperty::const_border_left_color(StyleBorderLeftColor { inner: light }),
                CssProperty::const_border_left_color(StyleBorderLeftColor { inner: dark }),
            ),
        ]
    };
    tab.extend(kit::hover_layers(
        vec![HOVER_FACE_LIGHT],
        vec![HOVER_FACE_DARK],
    ));
    tab.extend(kit::hover_ink(LIGHT_INK, DARK_INK));
    for (light, dark) in edges(LIGHT_BD, DARK_BD) {
        tab.extend(P::themed_on_hover(light, dark));
    }
    tab.extend(kit::active_layers(
        vec![PRESSED_FACE_LIGHT],
        vec![PRESSED_FACE_DARK],
    ));
    for (light, dark) in edges(LIGHT_BD3, DARK_BD3) {
        tab.extend(P::themed_on_active(light, dark));
    }
    tab.extend(kit::focus_shadow_ring(LIGHT_ACC, DARK_GLOW));

    // The selected tab: the stone in the rule's metal, open at its foot.
    let mut active = tab_box(3, 6);
    active.extend(kit::border(three_sides, 2, TAB_METAL, TAB_METAL));
    active.extend(kit::radius_corners(6, 6, 0, 0));
    active.push(P::simple(kit::layers(selected_stone())));
    active.push(P::simple(kit::ink(LIGHT_ON_ACC)));
    active.extend(kit::focus_shadow_ring(LIGHT_GLOW, DARK_GLOW));

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
    use super::style_kit as kit;
    // The widget's panel base (`tabs::PANEL_BASE`), then the leaf.
    let leaf = || {
        let mut v = crate::widgets::tabs::PANEL_BASE.to_vec();
        v.extend(kit::themed_bg(LIGHT_SUR, DARK_SUR));
        v
    };
    let mut padded = leaf();
    padded.extend(kit::padding(10, 10, 10, 10));
    let open_top = kit::Edges {
        top: false,
        right: true,
        bottom: true,
        left: true,
    };
    padded.extend(kit::border(open_top, 1, LIGHT_BD, DARK_BD));
    padded.extend(kit::radius_corners(0, 0, 3, 3));

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

    use super::style_kit as kit;
    type P = CssPropertyWithConditions;

    let band = kit::themed_layers(
        vec![kit::face(LIGHT_CT, LIGHT_CB)],
        vec![kit::face(DARK_CT, DARK_CB)],
    )
    .to_vec();
    let line = P::themed(
        CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: LIGHT_BD5 }),
        CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: DARK_BD5 }),
    )
    .to_vec();
    // The bar's own ink: the window controls' glyphs inherit it.
    let ink = vec![P::simple(kit::ink(CHROME_INK))];
    // Resting first, `:backdrop` after it: last match wins.
    let title_ink = vec![
        P::simple(kit::ink(CHROME_INK)),
        P::with_single_condition(
            kit::ink(CHROME_INK_DIM),
            &[DynamicSelector::PseudoState(PseudoStateType::Backdrop)],
        ),
    ];

    let mut button = Vec::new();
    button.extend(kit::hover_bg(CHROME_HOVER, CHROME_HOVER));
    button.extend(kit::active_bg(LIGHT_CB, DARK_CB));

    let mut close = Vec::new();
    close.extend(kit::hover_bg(STONE_CLAY.stone, STONE_CLAY.stone));
    close.extend(kit::hover_ink(LIGHT_ON_ACC, LIGHT_ON_ACC));
    close.extend(kit::active_bg(STONE_CLAY.deep, STONE_CLAY.deep));

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
    use super::{decl, style_kit as kit};
    type P = CssPropertyWithConditions;

    // The page: the root and the two boxes that hold the caller's strip and
    // pane.
    let e = s.resolved_root_style();
    chrome_part(&mut s.root_style, &e, |v| {
        v.extend(kit::themed_bg(LIGHT_PG, DARK_PG));
        v.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
    });
    let (right, content) = (s.resolved_right_style(), s.resolved_content_style());
    for (slot, e) in [(&mut s.right_style, right), (&mut s.content_style, content)] {
        chrome_part(slot, &e, |v| v.extend(kit::themed_bg(LIGHT_PG, DARK_PG)));
    }

    // The drawer: a leaf, its hairline on the edge that faces the page.
    let e = s.resolved_nav_style();
    chrome_part(&mut s.nav_style, &e, |v| {
        v.extend(kit::themed_bg(LIGHT_SUR, DARK_SUR));
        v.extend(kit::border(
            kit::Edges {
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
        v.push(P::simple(kit::layers(stone_face(LIGHT_ACC, STONE_STREAK))));
        v.extend(decl::border_colors(TAB_METAL).map(P::simple));
        let lit = stone_face(LIGHT_ACC, STONE_STREAK_HOVER);
        v.extend(kit::hover_layers(lit.clone(), lit));
        let held = sunken_stone_face(LIGHT_DEEP);
        v.extend(kit::active_layers(held.clone(), held));
        v.extend(kit::focus_halo(LIGHT_ACC, DARK_GLOW));
    });
    let e = s.resolved_back_icon_style();
    chrome_part(&mut s.back_icon_style, &e, |v| {
        v.push(P::simple(kit::ink(LIGHT_ON_ACC)));
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
        v.extend(kit::padding(0, BACKSTAGE_KEY_PAD, 0, BACKSTAGE_KEY_PAD));
        v.extend(decl::border(1));
        v.extend(kit::themed_ink(LIGHT_INK, DARK_INK));
        chrome_key(v);
    });
    // APPENDED to the selected item: the sunken stone in a brass edge, which
    // stays the stone under the pointer and while held (the lift would
    // un-pick it). Its states come after its resting face, so none of the
    // key's is shadowed; the ring is the stone's glow, by day and by night.
    let e = s.resolved_nav_item_active_style();
    chrome_part(&mut s.nav_item_active_style, &e, |v| {
        v.push(P::simple(kit::layers(selected_stone())));
        v.push(P::simple(kit::ink(LIGHT_ON_ACC)));
        v.extend(decl::border_colors(TAB_METAL).map(P::simple));
        v.extend(kit::hover_layers(selected_stone(), selected_stone()));
        v.extend(kit::hover_border(TAB_METAL, TAB_METAL));
        v.extend(kit::active_layers(selected_stone(), selected_stone()));
        v.extend(kit::focus_ring(LIGHT_GLOW, DARK_GLOW));
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
// hairline; the path sits in field paper (--fl-fld) inside a --fl-bd2 rule
// at the house radius, ringed by the accent under the pointer; the search
// box keeps a fixed width. At night the night strip, field and glow.

/// Flora's address-bar look.
#[must_use]
pub(crate) fn address_bar_look() -> crate::widgets::address_bar::AddressBarLook {
    use super::decl;
    let mut bar = vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            13,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ];
    bar.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    bar.extend(decl::themed_fill(LIGHT_STRIP, DARK_STRIP));
    bar.extend(decl::padding(5, 8, 5, 8));
    bar.extend(decl::border_bottom(1));
    bar.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));

    let mut field = vec![CssPropertyWithConditions::simple(CssProperty::const_height(
        LayoutHeight::const_px(26),
    ))];
    field.extend(decl::padding(0, 8, 0, 8));
    field.extend(decl::margin(0, 8, 0, 4));
    field.extend(decl::border(1));
    field.extend(decl::themed_border_color(LIGHT_BD2, DARK_BD2));
    field.extend(decl::radius(3));
    field.extend(decl::themed_fill(LIGHT_FLD, DARK_FLD));
    field.extend(decl::hover_border_color(LIGHT_ACC, DARK_GLOW));

    crate::widgets::address_bar::AddressBarLook {
        bar,
        nav: decl::margin(0, 2, 0, 0).to_vec(),
        field,
        field_editing: decl::margin(0, 8, 0, 4).to_vec(),
        search: vec![CssPropertyWithConditions::simple(CssProperty::const_width(
            LayoutWidth::const_px(220),
        ))],
        marker: Some(super::style_kit::FLORA_CLASS),
    }
}

/// The flora address bar.
#[must_use]
pub fn address_bar(b: crate::widgets::address_bar::AddressBar) -> Dom {
    crate::widgets::address_bar::build(b, &address_bar_look())
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

/// A font declaration pair: the chrome size and the system family.
fn shell_font(px: isize) -> [CssPropertyWithConditions; 2] {
    [
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            px,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ]
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
    shell_root.extend(decl::themed_fill(LIGHT_PG, DARK_PG));

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
    scope_root.extend(decl::themed_fill(LIGHT_PG, DARK_PG));

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
