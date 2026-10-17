//! A flora SPIN - `flora:green`, `flora:red`, ... - is flora cut in another
//! accent stone (flora.css: "One block; swap it to retint the whole site").
//! Under the app theme `flora:green` every colour flora's widgets write in the
//! base (blue) accent ramp is the green ramp's, at the same alpha, and every
//! other colour - ground, paper, ink, brass, borders - is exactly flora's.
//!
//! The window's app theme decides. These tests build widgets under the theme
//! they are for (the engine enters the window's theme around `layout()`) and
//! style them the way the engine does, through `LayoutWindow::style_user_dom`
//! - the path every app DOM takes to the cascade.

use azul_core::{app_theme::ThemeScope, dom::Dom, styled_dom::StyledDom};
use azul_css::{
    css::CssDeclaration,
    props::{
        basic::color::{ColorOrSystem, ColorU},
        property::CssProperty,
        style::StyleBackgroundContent,
    },
    AzString,
};
use azul_layout::{
    widgets::{
        button::{Button, ButtonType},
        check_box::CheckBox,
        text_input::TextInput,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// flora.css's accent block: `--fl-acc`, `--fl-deep`, `--fl-soft`, `--fl-glow`.
const BLUE_RAMP: [ColorU; 4] = [
    ColorU::rgb(0x2F, 0x4A, 0x85),
    ColorU::rgb(0x1E, 0x32, 0x60),
    ColorU::rgb(0xE0, 0xE4, 0xEE),
    ColorU::rgb(0x7A, 0x93, 0xC6),
];

/// The design system's Ordinary-time stone, the stone of `flora:green`, in
/// the same order.
const GREEN_RAMP: [ColorU; 4] = [
    ColorU::rgb(0x3E, 0x6B, 0x4A),
    ColorU::rgb(0x2C, 0x4E, 0x36),
    ColorU::rgb(0xE4, 0xEB, 0xE2),
    ColorU::rgb(0x7B, 0xA9, 0x89),
];

fn same_rgb(a: ColorU, b: ColorU) -> bool {
    (a.r, a.g, a.b) == (b.r, b.g, b.b)
}

/// The widgets a form is made of, built for the app theme `theme` and styled
/// by a window running in it.
fn styled_under(theme: &str) -> StyledDom {
    let dom = {
        let _scope = ThemeScope::enter(AzString::from(theme));
        Dom::create_body()
            .with_child(Button::with_type(AzString::from("Send"), ButtonType::Primary).dom())
            .with_child(Button::create(AzString::from("Cancel")).dom())
            .with_child(TextInput::create().dom())
            .with_child(CheckBox::create(true).dom())
    };
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new");
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = azul_core::geom::LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state;
    lw.app_theme = AzString::from(theme);
    lw.style_user_dom(dom)
}

/// Every colour one property declares, in declaration order.
fn colours_of(p: &CssProperty, out: &mut Vec<ColorU>) {
    let stop = |c: &ColorOrSystem, out: &mut Vec<ColorU>| {
        if let ColorOrSystem::Color(c) = c {
            out.push(*c);
        }
    };
    let layer = |l: &StyleBackgroundContent, out: &mut Vec<ColorU>| match l {
        StyleBackgroundContent::Color(c) => out.push(*c),
        StyleBackgroundContent::LinearGradient(g) => {
            g.stops.as_ref().iter().for_each(|s| stop(&s.color, out));
        }
        StyleBackgroundContent::RadialGradient(g) => {
            g.stops.as_ref().iter().for_each(|s| stop(&s.color, out));
        }
        StyleBackgroundContent::ConicGradient(g) => {
            g.stops.as_ref().iter().for_each(|s| stop(&s.color, out));
        }
        _ => {}
    };
    match p {
        CssProperty::TextColor(v) => out.extend(v.get_property().map(|c| c.inner)),
        CssProperty::BorderTopColor(v) => out.extend(v.get_property().map(|c| c.inner)),
        CssProperty::BorderRightColor(v) => out.extend(v.get_property().map(|c| c.inner)),
        CssProperty::BorderBottomColor(v) => out.extend(v.get_property().map(|c| c.inner)),
        CssProperty::BorderLeftColor(v) => out.extend(v.get_property().map(|c| c.inner)),
        CssProperty::BackgroundContent(v) => {
            if let Some(layers) = v.get_property() {
                layers.as_ref().iter().for_each(|l| layer(l, out));
            }
        }
        CssProperty::BoxShadowLeft(v)
        | CssProperty::BoxShadowRight(v)
        | CssProperty::BoxShadowTop(v)
        | CssProperty::BoxShadowBottom(v) => {
            out.extend(v.get_property().map(|s| s.as_ref().color));
        }
        _ => {}
    }
}

/// Every colour the styled DOM's nodes declare in their own style, node by
/// node, in declaration order (every state and both modes).
fn declared_colours(styled: &StyledDom) -> Vec<ColorU> {
    let mut out = Vec::new();
    for node in styled.node_data.as_ref() {
        for rule in node.style.rules.as_ref() {
            for d in rule.declarations.as_ref() {
                if let CssDeclaration::Static(p) = d {
                    colours_of(p, &mut out);
                }
            }
        }
    }
    out
}

#[test]
fn a_flora_green_window_paints_the_accent_in_the_green_stone() {
    let colours = declared_colours(&styled_under("flora:green"));
    let blue: Vec<_> = colours
        .iter()
        .filter(|c| BLUE_RAMP.iter().any(|b| same_rgb(**c, *b)))
        .collect();
    assert!(blue.is_empty(), "flora:green still declares the blue stone: {blue:?}");
    assert!(
        colours.iter().any(|c| same_rgb(*c, GREEN_RAMP[0])),
        "flora:green declares no green stone anywhere"
    );
}

#[test]
fn a_flora_window_keeps_the_blue_stone() {
    let colours = declared_colours(&styled_under("flora"));
    assert!(
        colours.iter().any(|c| same_rgb(*c, BLUE_RAMP[0])),
        "flora declares no blue stone"
    );
    assert!(
        !colours.iter().any(|c| GREEN_RAMP.iter().any(|g| same_rgb(*c, *g))),
        "flora declares a green stone"
    );
}

#[test]
fn a_spin_changes_nothing_but_the_accent() {
    let base = declared_colours(&styled_under("flora"));
    let green = declared_colours(&styled_under("flora:green"));
    assert_eq!(base.len(), green.len(), "a spin never adds or drops a declaration");
    for (i, (b, g)) in base.iter().zip(&green).enumerate() {
        match BLUE_RAMP.iter().position(|r| same_rgb(*b, *r)) {
            Some(token) => assert_eq!(
                *g,
                ColorU { a: b.a, ..GREEN_RAMP[token] },
                "colour {i}: the blue token {b:?} becomes the green one at its own alpha"
            ),
            None => assert_eq!(b, g, "colour {i}: not an accent, so not the spin's business"),
        }
    }
}
