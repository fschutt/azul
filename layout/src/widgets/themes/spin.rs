//! Flora's SPINS - `flora:green`, `flora:red`, `flora:purple`, `flora:gold`,
//! `flora:rose`: the app theme `flora` cut in another accent stone.
//!
//! `flora` itself is the website's deep blue (flora.css's accent block,
//! `--fl-acc` / `--fl-deep` / `--fl-soft` / `--fl-glow`). The CSS says what a
//! retint is: "One block; swap it to retint the whole site." A spin is
//! exactly that swap - the four tokens of the accent ramp and the band cut
//! from them change; the ground, the metal, the ink, the paper faces, every
//! radius, metric and motion stay. The names are the liturgical colours the
//! specimen's gem rows are cut in:
//!
//! | spin            | season     | acc       | deep      | soft      | glow      |
//! |-----------------|------------|-----------|-----------|-----------|-----------|
//! | `flora`         | (the site) | `#2F4A85` | `#1E3260` | `#E0E4EE` | `#7A93C6` |
//! | `flora:green`   | Ordinary   | `#3E6B4A` | `#2C4E36` | `#E4EBE2` | `#7BA989` |
//! | `flora:red`     | Pentecost  | `#8E3B33` | `#6E2A24` | `#F0E2DE` | `#C07A6E` |
//! | `flora:purple`  | Advent     | `#5B4470` | `#433154` | `#E9E4EF` | `#937FAC` |
//! | `flora:gold`    | Easter     | `#876A1E` | `#66501A` | `#F7F0DC` | `#E9CF7E` |
//! | `flora:rose`    | Gaudete    | `#9A5763` | `#74404A` | `#F5E6E7` | `#DCA6AC` |
//!
//! The seasons are the Azlin design system's liturgical set (the "Interface
//! Specimen", `season` tweak: ordinary / advent / easter / pentecost / rose),
//! on flora.css's neutral ground - where the two disagree flora.css wins, and
//! they only meet in the accent block. Green, purple and red are the
//! specimen's stones as they are. Gold and rose are cut one step deeper than
//! the specimen's `#B08D2E` / `#B76E79` (which carry the paper ink at 2.8:1
//! and 3.4:1): flora writes `--fl-on-acc` (#F4F2EA) on every stone, and every
//! stone here carries it at 4.5:1 or better. `flora:blue` names the base.
//!
//! # How a spin reaches the paint
//!
//! The theme chain makes a spin live (`azul_css::dynamic_selector::app_theme_chain`):
//! `flora:green` is `[flora:green, flora, flat]`, its STRUCTURAL theme is
//! `flora`, so every widget builds flora's DOM and flora's `@theme(flora)`
//! blocks are the live ones - a spin never changes a DOM's shape
//! (RICING_LAYERS §7.1, "spins only change paint").
//!
//! Flora's widgets write the accent as the base ramp's colours (the
//! `flora::LIGHT_ACC` .. consts, a few hundred declarations over many widget
//! files, a good part of them in `const` style slices). Rather than thread a
//! ramp through every one of them, the base ramp IS the variable set:
//! [`respin_dom`] rewrites every colour of a built DOM whose RGB is one of the
//! base ramp's to the spin's colour at the SAME alpha (a `--fl-gla` halo keeps
//! its 55 %, a selection wash its tint) - one pass over the app's DOM after
//! `layout()` returned, in `LayoutWindow::style_user_dom_in_scope`, the path
//! every app DOM (layout callback, VirtualView, form controls) takes to the
//! cascade. A theme switch rebuilds every DOM, so the pass sees each build
//! under the spin it is for; a mode switch needs nothing, the ramp being the
//! same by day and by night (flora.css: "the accent keeps its stone").
//!
//! What it rewrites: the typed colours of node styles and component sheets
//! (inks, fills, gradient stops, borders, shadows, carets, selections,
//! scrollbar parts, `var()` fallbacks, keyframes) and a custom property whose
//! value IS one hex colour (`ShellThemeScope`'s `--az-accent` of the default
//! blue family). Only the base ramp's RGB values are keys: the other stones
//! (a leaf or clay badge), the brass, the ground and every colour of the app's
//! own are left as they are. No spin colour is itself a key, so a DOM spun
//! twice is spun once ([`tests::no_spin_colour_is_a_base_key`]).
//!
//! What it never rewrites: a subtree the app marks as its DOCUMENT's content
//! with the class [`DOCUMENT_CONTENT_CLASS`] (`__azul-document-content`) - a
//! slide in its deck theme, a page, a theme preview. Its colours are the
//! document's, which may well be flora's blue (AzShow's "Stone" deck theme
//! is): the spin is the chrome's.

use alloc::vec::Vec;

use azul_core::dom::{Dom, DomVec};
use azul_css::{
    css::{
        BoxOrStatic, Css, CssDeclaration, CssDeclarationVec, CssPropertyValue, CssRuleBlockVec,
        CssVec, KeyframeStopVec, KeyframesVec,
    },
    props::{
        basic::color::{ColorOrSystem, ColorU},
        property::{CssProperty, CssPropertyVec},
        style::{
            NormalizedLinearColorStopVec, NormalizedRadialColorStopVec, StyleBackgroundContent,
            StyleBackgroundContentVec,
        },
    },
    AzString,
};

/// One accent ramp: flora.css's four accent tokens.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FloraAccentRamp {
    /// `--fl-acc`: the stone - primary commands, a checked box, a filled
    /// track, the focus ring by day.
    pub acc: ColorU,
    /// `--fl-deep`: the stone's shadow side - its border, the sunken
    /// selected item, selected text by day.
    pub deep: ColorU,
    /// `--fl-soft`: the wash - a selected row, a hovered option.
    pub soft: ColorU,
    /// `--fl-glow`: the stone's highlight - the gem's lit spot, the focus
    /// ring at night.
    pub glow: ColorU,
}

const fn rgb(r: u8, g: u8, b: u8) -> ColorU {
    ColorU { r, g, b, a: 255 }
}

/// The base ramp: flora.css's accent block, the colours flora's widgets are
/// written in.
const BASE: FloraAccentRamp = FloraAccentRamp {
    acc: super::flora::LIGHT_ACC,
    deep: super::flora::LIGHT_DEEP,
    soft: super::flora::LIGHT_SOFT,
    glow: super::flora::LIGHT_GLOW,
};

/// `--fl-band`'s stops, by day (`#2B4477 / #1B2C4C / #16233D`) then at night
/// (`#24395F / #141F35 / #101827`): the deep page a selected tab opens onto,
/// cut from the same stone.
const BASE_BAND: [ColorU; 6] = [
    rgb(0x2B, 0x44, 0x77),
    rgb(0x1B, 0x2C, 0x4C),
    rgb(0x16, 0x23, 0x3D),
    rgb(0x24, 0x39, 0x5F),
    rgb(0x14, 0x1F, 0x35),
    rgb(0x10, 0x18, 0x27),
];

/// The accent stone of the app theme `flora`: blue (the base), or one of its
/// spins. See the module docs for the table.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum FloraSpin {
    /// `flora` (also `flora:blue`): the website's deep blue.
    #[default]
    Blue,
    /// `flora:green` - Ordinary time.
    Green,
    /// `flora:red` - Pentecost.
    Red,
    /// `flora:purple` - Advent (and Lent).
    Purple,
    /// `flora:gold` - Easter (and Christmas).
    Gold,
    /// `flora:rose` - Gaudete (and Laetare).
    Rose,
}

impl FloraSpin {
    /// Every spin, the base first: the order a theme picker lists them in.
    pub const ALL: [Self; 6] = [
        Self::Blue,
        Self::Green,
        Self::Red,
        Self::Purple,
        Self::Gold,
        Self::Rose,
    ];

    /// The spin's own name, the part after `flora:`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Blue => "blue",
            Self::Green => "green",
            Self::Red => "red",
            Self::Purple => "purple",
            Self::Gold => "gold",
            Self::Rose => "rose",
        }
    }

    /// The app-theme name that selects it (`CallbackInfo::set_theme`,
    /// `AppConfig::with_theme`, `AZ_THEME`): `flora` for the base,
    /// `flora:<name>` for a spin.
    #[must_use]
    pub const fn theme_name(self) -> &'static str {
        match self {
            Self::Blue => "flora",
            Self::Green => "flora:green",
            Self::Red => "flora:red",
            Self::Purple => "flora:purple",
            Self::Gold => "flora:gold",
            Self::Rose => "flora:rose",
        }
    }

    /// The spin ONE app-theme name selects: `flora` and `flora:blue` the
    /// base, `flora:<name>` that spin; `None` for any other name (not a
    /// chain - see [`Self::of_theme`]).
    #[must_use]
    pub fn from_theme_name(name: &str) -> Option<Self> {
        if name == "flora" {
            return Some(Self::Blue);
        }
        let spin = name.strip_prefix("flora:")?;
        Self::ALL.into_iter().find(|s| s.name() == spin)
    }

    /// The spin a theme CHAIN paints in: its first entry that names one
    /// (`flora:green` in `[flora:green, flora, flat]`, also behind a user
    /// theme whose `fallback:` is a spin), and only when `flora` is the
    /// chain's structural theme - under `flat` no flora paint is live.
    /// `None`: not a flora chain.
    #[must_use]
    pub fn of_chain<S: AsRef<str>>(chain: &[S]) -> Option<Self> {
        if azul_css::dynamic_selector::structural_app_theme(chain) != Some("flora") {
            return None;
        }
        chain
            .iter()
            .find_map(|entry| Self::from_theme_name(entry.as_ref()))
            .or(Some(Self::Blue))
    }

    /// The spin the app theme `name` paints in: [`Self::of_chain`] of the
    /// chain it expands to.
    #[must_use]
    pub fn of_theme(name: &str) -> Option<Self> {
        let chain = azul_css::dynamic_selector::app_theme_chain(name);
        Self::of_chain(chain.as_slice())
    }

    /// The spin of the DOM being built ([`azul_core::app_theme::current_theme`]);
    /// the base when that is no flora chain.
    #[must_use]
    pub fn current() -> Self {
        Self::of_theme(azul_core::app_theme::current_theme().as_str()).unwrap_or_default()
    }

    /// The spin's accent ramp (the module docs' table).
    #[must_use]
    pub const fn ramp(self) -> FloraAccentRamp {
        let (acc, deep, soft, glow) = match self {
            Self::Blue => return BASE,
            Self::Green => (
                rgb(0x3E, 0x6B, 0x4A),
                rgb(0x2C, 0x4E, 0x36),
                rgb(0xE4, 0xEB, 0xE2),
                rgb(0x7B, 0xA9, 0x89),
            ),
            Self::Red => (
                rgb(0x8E, 0x3B, 0x33),
                rgb(0x6E, 0x2A, 0x24),
                rgb(0xF0, 0xE2, 0xDE),
                rgb(0xC0, 0x7A, 0x6E),
            ),
            Self::Purple => (
                rgb(0x5B, 0x44, 0x70),
                rgb(0x43, 0x31, 0x54),
                rgb(0xE9, 0xE4, 0xEF),
                rgb(0x93, 0x7F, 0xAC),
            ),
            Self::Gold => (
                rgb(0x87, 0x6A, 0x1E),
                rgb(0x66, 0x50, 0x1A),
                rgb(0xF7, 0xF0, 0xDC),
                rgb(0xE9, 0xCF, 0x7E),
            ),
            Self::Rose => (
                rgb(0x9A, 0x57, 0x63),
                rgb(0x74, 0x40, 0x4A),
                rgb(0xF5, 0xE6, 0xE7),
                rgb(0xDC, 0xA6, 0xAC),
            ),
        };
        FloraAccentRamp {
            acc,
            deep,
            soft,
            glow,
        }
    }

    /// The spin's band (`--fl-band`, day then night, as [`BASE_BAND`]): the
    /// base's own, or cut from the spin's stone the way flora.css cuts the
    /// blue one - the top a quarter of the way from the accent to the deep
    /// tone, darkening away from it.
    fn band(self) -> [ColorU; 6] {
        if self == Self::Blue {
            return BASE_BAND;
        }
        let FloraAccentRamp { acc, deep, .. } = self.ramp();
        [
            scale(lerp(deep, acc, 0.72), 0.97),
            scale(deep, 0.88),
            scale(deep, 0.68),
            scale(lerp(deep, acc, 0.35), 0.9),
            scale(deep, 0.62),
            scale(deep, 0.47),
        ]
    }

    /// The rewrite the spin makes: `(base colour, spin colour)` pairs, RGB
    /// only (a match keeps its own alpha). Empty for the base.
    #[must_use]
    fn pairs(self) -> Vec<(ColorU, ColorU)> {
        if self == Self::Blue {
            return Vec::new();
        }
        let to = self.ramp();
        let mut out = alloc::vec![
            (BASE.acc, to.acc),
            (BASE.deep, to.deep),
            (BASE.soft, to.soft),
            (BASE.glow, to.glow),
        ];
        out.extend(BASE_BAND.into_iter().zip(self.band()));
        out
    }
}

/// `a + (b - a) * t`, per channel, opaque.
fn lerp(a: ColorU, b: ColorU, t: f32) -> ColorU {
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    rgb(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b))
}

/// `c * k`, per channel, opaque (a darker shade of the same stone).
fn scale(c: ColorU, k: f32) -> ColorU {
    let s = |x: u8| (f32::from(x) * k).round().clamp(0.0, 255.0) as u8;
    rgb(s(c.r), s(c.g), s(c.b))
}

// ==== the rewrite ====

/// The `(from, to)` pairs of one spin.
type SpinMap = [(ColorU, ColorU)];

/// `c` in the spin: the spin's colour at `c`'s alpha when `c`'s RGB is a
/// base colour, `None` (unchanged) otherwise.
fn swap(c: ColorU, map: &SpinMap) -> Option<ColorU> {
    map.iter()
        .find(|(from, _)| from.r == c.r && from.g == c.g && from.b == c.b)
        .map(|(_, to)| ColorU {
            r: to.r,
            g: to.g,
            b: to.b,
            a: c.a,
        })
}

/// A gradient stop's colour in the spin (a `system:` colour is the
/// desktop's, never a ramp's).
fn swap_stop(c: ColorOrSystem, map: &SpinMap) -> Option<ColorOrSystem> {
    match c {
        ColorOrSystem::Color(c) => swap(c, map).map(ColorOrSystem::Color),
        ColorOrSystem::System(_) => None,
    }
}

/// One background layer in the spin; `None` when it carries no base colour.
fn respin_layer(layer: &StyleBackgroundContent, map: &SpinMap) -> Option<StyleBackgroundContent> {
    match layer {
        StyleBackgroundContent::Color(c) => swap(*c, map).map(StyleBackgroundContent::Color),
        StyleBackgroundContent::LinearGradient(g) => {
            let stops = respin_slice(g.stops.as_ref(), |s| {
                swap_stop(s.color, map).map(|color| {
                    let mut s = *s;
                    s.color = color;
                    s
                })
            })?;
            let mut g = g.clone();
            g.stops = NormalizedLinearColorStopVec::from_vec(stops);
            Some(StyleBackgroundContent::LinearGradient(g))
        }
        StyleBackgroundContent::RadialGradient(g) => {
            let stops = respin_slice(g.stops.as_ref(), |s| {
                swap_stop(s.color, map).map(|color| {
                    let mut s = *s;
                    s.color = color;
                    s
                })
            })?;
            let mut g = g.clone();
            g.stops = NormalizedLinearColorStopVec::from_vec(stops);
            Some(StyleBackgroundContent::RadialGradient(g))
        }
        StyleBackgroundContent::ConicGradient(g) => {
            let stops = respin_slice(g.stops.as_ref(), |s| {
                swap_stop(s.color, map).map(|color| {
                    let mut s = *s;
                    s.color = color;
                    s
                })
            })?;
            let mut g = g.clone();
            g.stops = NormalizedRadialColorStopVec::from_vec(stops);
            Some(StyleBackgroundContent::ConicGradient(g))
        }
        StyleBackgroundContent::Image(_) | StyleBackgroundContent::SystemColor(_) => None,
    }
}

/// `items` with every item `f` rewrites rewritten; `None` when `f` rewrote
/// none (the caller keeps what it has - no allocation for the common case).
fn respin_slice<T: Clone>(items: &[T], mut f: impl FnMut(&T) -> Option<T>) -> Option<Vec<T>> {
    let mut out: Option<Vec<T>> = None;
    for (i, item) in items.iter().enumerate() {
        if let Some(new) = f(item) {
            out.get_or_insert_with(|| items.to_vec())[i] = new;
        }
    }
    out
}

/// An `Exact` value rewritten by `f`; `None` for a keyword or no change.
fn exact<T>(v: &CssPropertyValue<T>, f: impl FnOnce(&T) -> Option<T>) -> Option<CssPropertyValue<T>> {
    v.get_property().and_then(f).map(CssPropertyValue::Exact)
}

/// A shadow in the spin.
fn respin_shadow(
    v: &CssPropertyValue<BoxOrStatic<azul_css::props::style::StyleBoxShadow>>,
    map: &SpinMap,
) -> Option<CssPropertyValue<BoxOrStatic<azul_css::props::style::StyleBoxShadow>>> {
    exact(v, |s| {
        let s = *s.as_ref();
        swap(s.color, map).map(|color| BoxOrStatic::heap(azul_css::props::style::StyleBoxShadow { color, ..s }))
    })
}

/// One property in the spin; `None` when it carries no base colour.
#[must_use]
pub fn respin_property(p: &CssProperty, map: &[(ColorU, ColorU)]) -> Option<CssProperty> {
    macro_rules! inner {
        ($variant:ident, $v:expr) => {
            exact($v, |c| {
                swap(c.inner, map).map(|inner| {
                    let mut c = c.clone();
                    c.inner = inner;
                    c
                })
            })
            .map(CssProperty::$variant)
        };
    }
    match p {
        CssProperty::TextColor(v) => inner!(TextColor, v),
        CssProperty::CaretColor(v) => inner!(CaretColor, v),
        CssProperty::SelectionBackgroundColor(v) => inner!(SelectionBackgroundColor, v),
        CssProperty::SelectionColor(v) => inner!(SelectionColor, v),
        CssProperty::BorderTopColor(v) => inner!(BorderTopColor, v),
        CssProperty::BorderRightColor(v) => inner!(BorderRightColor, v),
        CssProperty::BorderBottomColor(v) => inner!(BorderBottomColor, v),
        CssProperty::BorderLeftColor(v) => inner!(BorderLeftColor, v),
        CssProperty::BackgroundContent(v) => exact(v, |layers| {
            respin_slice(layers.as_ref(), |l| respin_layer(l, map))
                .map(StyleBackgroundContentVec::from_vec)
        })
        .map(CssProperty::BackgroundContent),
        CssProperty::ScrollbarTrack(v) => {
            exact(v, |l| respin_layer(l, map)).map(CssProperty::ScrollbarTrack)
        }
        CssProperty::ScrollbarThumb(v) => {
            exact(v, |l| respin_layer(l, map)).map(CssProperty::ScrollbarThumb)
        }
        CssProperty::BoxShadowLeft(v) => respin_shadow(v, map).map(CssProperty::BoxShadowLeft),
        CssProperty::BoxShadowRight(v) => respin_shadow(v, map).map(CssProperty::BoxShadowRight),
        CssProperty::BoxShadowTop(v) => respin_shadow(v, map).map(CssProperty::BoxShadowTop),
        CssProperty::BoxShadowBottom(v) => {
            respin_shadow(v, map).map(CssProperty::BoxShadowBottom)
        }
        CssProperty::TextShadow(v) => respin_shadow(v, map).map(CssProperty::TextShadow),
        _ => None,
    }
}

/// One declaration in the spin: a typed property, a `var()`'s fallback, or
/// a custom property whose value is one hex colour.
fn respin_declaration(d: &CssDeclaration, map: &SpinMap) -> Option<CssDeclaration> {
    match d {
        CssDeclaration::Static(p) => respin_property(p, map).map(CssDeclaration::Static),
        CssDeclaration::Dynamic(dynamic) => {
            respin_property(&dynamic.default_value, map).map(|default_value| {
                let mut dynamic = dynamic.clone();
                dynamic.default_value = default_value;
                CssDeclaration::Dynamic(dynamic)
            })
        }
        CssDeclaration::CustomProperty(c) => {
            let color = ColorU::parse_hex(c.value.as_str())?;
            swap(color, map).map(|to| {
                let mut c = c.clone();
                c.value = AzString::from(to.to_hex());
                CssDeclaration::CustomProperty(c)
            })
        }
    }
}

/// A stylesheet in the spin, in place: its rules' declarations and its
/// keyframes. A vec is only rebuilt when something in it changes, and never
/// written through (a `const` style's storage is static).
fn respin_css(css: &mut Css, map: &SpinMap) {
    let rules = respin_slice(css.rules.as_ref(), |rule| {
        respin_slice(rule.declarations.as_ref(), |d| respin_declaration(d, map)).map(|declarations| {
            let mut rule = rule.clone();
            rule.declarations = CssDeclarationVec::from_vec(declarations);
            rule
        })
    });
    if let Some(rules) = rules {
        css.rules = CssRuleBlockVec::from_vec(rules);
    }
    let keyframes = respin_slice(css.keyframes.as_ref(), |track| {
        respin_slice(track.stops.as_ref(), |stop| {
            respin_slice(stop.props.as_ref(), |p| respin_property(p, map)).map(|props| {
                let mut stop = stop.clone();
                stop.props = CssPropertyVec::from_vec(props);
                stop
            })
        })
        .map(|stops| {
            let mut track = track.clone();
            track.stops = KeyframeStopVec::from_vec(stops);
            track
        })
    });
    if let Some(keyframes) = keyframes {
        css.keyframes = KeyframesVec::from_vec(keyframes);
    }
}

/// The class that marks a subtree as the app's DOCUMENT content (a slide, a
/// page, a theme preview): a spin leaves its colours - the document's - as
/// they are, all the way down (module docs).
pub const DOCUMENT_CONTENT_CLASS: &str = "__azul-document-content";

fn respin_dom_with(dom: &mut Dom, map: &SpinMap) {
    if dom.root.has_class(DOCUMENT_CONTENT_CLASS) {
        return;
    }
    respin_css(&mut dom.root.style, map);
    if !dom.css.as_ref().is_empty() {
        let mut sheets = core::mem::replace(&mut dom.css, CssVec::from_vec(Vec::new()))
            .into_library_owned_vec();
        for sheet in &mut sheets {
            respin_css(sheet, map);
        }
        dom.css = CssVec::from_vec(sheets);
    }
    if !dom.children.as_ref().is_empty() {
        let mut children = core::mem::replace(&mut dom.children, DomVec::from_vec(Vec::new()))
            .into_library_owned_vec();
        for child in &mut children {
            respin_dom_with(child, map);
        }
        // Same nodes, same count: `estimated_total_children` still holds.
        dom.children = DomVec::from_vec(children);
    }
}

/// Paints a built DOM in `spin`: every base-ramp colour becomes the spin's
/// (module docs). The base spin changes nothing.
pub fn respin_dom(dom: &mut Dom, spin: FloraSpin) {
    let map = spin.pairs();
    if map.is_empty() {
        return;
    }
    respin_dom_with(dom, &map);
}

/// [`respin_dom`] for the spin the app theme `theme` paints in - nothing
/// unless it is a flora chain with a spin. What `LayoutWindow` runs on every
/// app DOM before the cascade.
pub fn respin_dom_for_theme(dom: &mut Dom, theme: &str) {
    if let Some(spin) = FloraSpin::of_theme(theme) {
        respin_dom(dom, spin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ink flora lays on every stone (`--fl-on-acc`).
    const ON_ACC: ColorU = rgb(0xF4, 0xF2, 0xEA);

    fn luminance(c: ColorU) -> f32 {
        let lin = |x: u8| {
            let s = f32::from(x) / 255.0;
            if s <= 0.039_28 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
    }

    fn contrast(a: ColorU, b: ColorU) -> f32 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    #[test]
    fn every_spin_round_trips_through_its_theme_name() {
        for spin in FloraSpin::ALL {
            assert_eq!(FloraSpin::from_theme_name(spin.theme_name()), Some(spin));
        }
        assert_eq!(FloraSpin::from_theme_name("flora:blue"), Some(FloraSpin::Blue));
        assert_eq!(FloraSpin::from_theme_name("flora:teal"), None);
        assert_eq!(FloraSpin::from_theme_name("flat"), None);
    }

    #[test]
    fn a_spin_is_found_in_its_chain_and_only_under_flora() {
        assert_eq!(FloraSpin::of_theme("flora:green"), Some(FloraSpin::Green));
        assert_eq!(FloraSpin::of_theme("flora"), Some(FloraSpin::Blue));
        assert_eq!(FloraSpin::of_theme("flat"), None);
        assert_eq!(
            FloraSpin::of_chain(&["mine", "flora:rose", "flora", "flat"]),
            Some(FloraSpin::Rose),
            "a user theme falling back to a spin paints in it"
        );
    }

    #[test]
    fn no_spin_colour_is_a_base_key() {
        for spin in FloraSpin::ALL {
            for (_, to) in spin.pairs() {
                assert!(
                    swap(to, &FloraSpin::Red.pairs()).is_none()
                        && swap(to, &FloraSpin::Green.pairs()).is_none(),
                    "{spin:?}: {to:?} is a base colour, a second pass would move it again"
                );
            }
        }
    }

    #[test]
    fn every_stone_carries_the_paper_ink() {
        for spin in FloraSpin::ALL {
            let r = spin.ramp();
            assert!(
                contrast(ON_ACC, r.acc) >= 4.5,
                "{spin:?}: on-acc on the stone reads {:.2}:1",
                contrast(ON_ACC, r.acc)
            );
            assert!(contrast(ON_ACC, r.deep) >= 6.0, "{spin:?}: the deep tone is the stone's shadow");
            assert!(luminance(r.soft) > 0.7, "{spin:?}: the wash sits near the ground");
        }
    }

    #[test]
    fn a_spin_keeps_alpha_and_leaves_every_other_colour() {
        let map = FloraSpin::Green.pairs();
        let halo = ColorU { a: 140, ..BASE.glow };
        assert_eq!(swap(halo, &map), Some(ColorU { a: 140, ..FloraSpin::Green.ramp().glow }));
        assert_eq!(swap(ON_ACC, &map), None);
        assert_eq!(swap(super::super::flora::LIGHT_PG, &map), None);
    }

    #[test]
    fn a_spun_dom_carries_the_spins_stone_where_the_base_had_its_own() {
        use azul_css::{dynamic_selector::CssPropertyWithConditions, props::style::StyleTextColor};
        let mut dom = Dom::create_div()
            .with_css_props(
                alloc::vec![
                    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
                        inner: BASE.acc,
                    })),
                    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
                        inner: ON_ACC,
                    })),
                ]
                .into(),
            )
            .with_child(Dom::create_div().with_css("color: #2f4a85;"));
        respin_dom(&mut dom, FloraSpin::Red);
        let colours = |css: &Css| -> Vec<ColorU> {
            css.rules
                .as_ref()
                .iter()
                .flat_map(|r| r.declarations.as_ref().iter())
                .filter_map(|d| match d {
                    CssDeclaration::Static(CssProperty::TextColor(v)) => {
                        v.get_property().map(|c| c.inner)
                    }
                    _ => None,
                })
                .collect()
        };
        assert_eq!(colours(&dom.root.style), [FloraSpin::Red.ramp().acc, ON_ACC]);
        let child = &dom.children.as_ref()[0];
        assert_eq!(colours(&child.css.as_ref()[0]), [FloraSpin::Red.ramp().acc]);
    }

    /// AzShow's "Stone" deck theme is flora's base blue too: under
    /// `flora:red` the slides came out red. A spin repaints the CHROME; a
    /// subtree the app marks as its document's content (a slide, a page, a
    /// theme preview) keeps the colours the document carries, all the way
    /// down.
    #[test]
    fn a_spin_leaves_a_subtree_marked_as_document_content_alone() {
        let ink = |dom: &Dom| -> Vec<ColorU> {
            dom.css
                .as_ref()
                .iter()
                .flat_map(|css| css.rules.as_ref().iter())
                .flat_map(|r| r.declarations.as_ref().iter())
                .filter_map(|d| match d {
                    CssDeclaration::Static(CssProperty::TextColor(v)) => {
                        v.get_property().map(|c| c.inner)
                    }
                    _ => None,
                })
                .collect()
        };
        let mut dom = Dom::create_div().with_child(Dom::create_div().with_css("color: #2f4a85;")).with_child(
            Dom::create_div()
                .with_class(AzString::from_const_str("__azul-document-content"))
                .with_css("color: #2f4a85;")
                .with_child(Dom::create_div().with_css("color: #2f4a85;")),
        );
        respin_dom(&mut dom, FloraSpin::Red);
        let [chrome, content] = dom.children.as_ref() else {
            panic!("two children");
        };
        assert_eq!(ink(chrome), [FloraSpin::Red.ramp().acc], "the chrome takes the spin");
        assert_eq!(ink(content), [BASE.acc], "the content keeps its colour");
        assert_eq!(
            ink(&content.children.as_ref()[0]),
            [BASE.acc],
            "and so does everything in it"
        );
    }
}
