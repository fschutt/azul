//! The APP THEME (`AppConfig::with_theme`, `CallbackInfo::set_theme`): one name - `flat`,
//! `flora`, later `native` and user themes - that `@theme(<name>)` blocks select by, separate
//! from the light / dark colour scheme.
//!
//! What a window evaluates is its `LayoutWindow::app_theme` - the theme its DOM was built
//! under, which the shells keep equal to the app's choice (`regenerate_layout` brings a window
//! up to it). These tests set it per window and never publish the app-global choice: the
//! layout tests run side by side in one process.
//!
//! Every check pins BOTH directions: a block that "works" by applying unconditionally, or a
//! theme that "works" by never leaving flat, would pass a one-sided assertion.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
    window::WindowTheme,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, DynamicSelector, ThemeCondition},
    props::{
        basic::color::ColorU,
        layout::{LayoutHeight, LayoutWidth},
        property::CssProperty,
        style::{StyleBackgroundContent, StyleBackgroundContentVec},
    },
    system::{defaults, SystemStyle},
    AzString,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const BASE: ColorU = ColorU::rgb(0x80, 0x80, 0x80);
const FLAT: ColorU = ColorU::rgb(0x00, 0x00, 0xff);
const FLORA: ColorU = ColorU::rgb(0xff, 0x00, 0x00);
const FLORA_NIGHT: ColorU = ColorU::rgb(0x40, 0x00, 0x00);

/// `AZ_MODE` pins the COLOUR SCHEME; the tests below that vary it have nothing to compare
/// under a pin.
fn scheme_pinned() -> bool {
    azul_css::dynamic_selector::mode_pinned_by_env().is_some()
}

fn window(desktop: SystemStyle, app_theme: &str) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.set_system_style(Arc::new(desktop));
    lw.app_theme = AzString::from(app_theme.to_string());
    lw
}

fn window_state(theme: WindowTheme) -> FullWindowState {
    let mut ws = FullWindowState::default();
    ws.theme = theme;
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    ws
}

fn lay_out(lw: &mut LayoutWindow, styled: StyledDom, ws: &FullWindowState) {
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = None;
    lw.layout_and_generate_display_list(styled, ws, &rr, &sc, &mut dbg)
        .expect("layout");
}

fn box_fill(lw: &LayoutWindow, width: f32, height: f32) -> Option<ColorU> {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::Rect { bounds, color, .. }
                if (bounds.0.size.width - width).abs() < 0.01
                    && (bounds.0.size.height - height).abs() < 0.01 =>
            {
                Some(*color)
            }
            _ => None,
        })
}

/// An APP stylesheet with a base rule and one block per theme: `body > div(40x20)`.
fn stylesheet_document() -> StyledDom {
    let component_map = azul_core::xml::ComponentMap::default();
    azul_layout::xml::domxml_from_str(
        "<html><head><style>
            div { width: 40px; height: 20px; background-color: #808080; }
            @theme(flat) { div { background-color: #0000ff; } }
            @theme(flora) { div { background-color: #ff0000; } }
        </style></head><body><div></div></body></html>",
        &component_map,
    )
    .parsed_dom
}

const BG_BASE: CssProperty = CssProperty::const_background_content(
    StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(BASE)]),
);
const BG_FLAT: CssProperty = CssProperty::const_background_content(
    StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(FLAT)]),
);
const BG_FLORA: CssProperty = CssProperty::const_background_content(
    StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(FLORA)]),
);
const BG_FLORA_NIGHT: CssProperty = CssProperty::const_background_content(
    StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(FLORA_NIGHT)]),
);

/// What a migrated widget emits: its structure's base declarations, then EVERY theme's block
/// as const statics, conditioned on the theme (the dark twin on the colour scheme too).
static WIDGET_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(40))),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(20))),
    CssPropertyWithConditions::simple(BG_BASE),
    CssPropertyWithConditions::with_single_condition(BG_FLAT, azul_css::theme_conditions!("flat")),
    CssPropertyWithConditions::with_single_condition(
        BG_FLORA,
        azul_css::theme_conditions!("flora"),
    ),
    CssPropertyWithConditions::with_single_condition(
        BG_FLORA_NIGHT,
        azul_css::theme_conditions!("flora", DynamicSelector::Theme(ThemeCondition::Dark)),
    ),
];

fn widget_document() -> StyledDom {
    let dom = Dom::create_body().with_child(Dom::create_div().with_css_props(
        azul_css::dynamic_selector::CssPropertyWithConditionsVec::from_const_slice(WIDGET_STYLE),
    ));
    StyledDom::create_from_dom(dom)
}

#[test]
fn a_new_window_starts_in_the_default_theme_flat() {
    // Nothing in this test binary publishes an app theme, so a new window takes the default.
    let lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    assert_eq!(lw.app_theme.as_str(), "flat");
    let ctx = lw.dynamic_selector_context(&window_state(WindowTheme::LightMode));
    assert_eq!(ctx.app_theme(), "flat");
}

#[test]
fn the_window_context_carries_the_app_theme_beside_the_colour_scheme() {
    if scheme_pinned() {
        return;
    }
    let lw = window(defaults::macos_modern_light(), "flora");
    let ctx = lw.dynamic_selector_context(&window_state(WindowTheme::LightMode));
    assert_eq!(ctx.app_theme(), "flora");
    assert_eq!(
        ctx.theme,
        ThemeCondition::Light,
        "the colour scheme is still the window's light"
    );
    let ctx = lw.dynamic_selector_context(&window_state(WindowTheme::DarkMode));
    assert_eq!(ctx.app_theme(), "flora", "a dark window keeps its app theme");
    assert_eq!(ctx.theme, ThemeCondition::Dark);
}

#[test]
fn a_stylesheets_theme_blocks_paint_only_under_their_app_theme() {
    // `monokai` has no block: every theme chain ends in the default theme (the floor, design
    // §7.1), so an unknown theme looks like flat until someone writes its block.
    for (app_theme, want) in [("flat", FLAT), ("flora", FLORA), ("monokai", FLAT)] {
        let mut lw = window(defaults::macos_modern_light(), app_theme);
        let ws = window_state(WindowTheme::LightMode);
        lay_out(&mut lw, stylesheet_document(), &ws);
        assert_eq!(
            box_fill(&lw, 40.0, 20.0),
            Some(want),
            "app theme {app_theme}: its own block, or the default theme's when it has none"
        );
    }
}

/// A spin-off app theme (`AppConfig::with_theme("xyz:pink")`, or `AZ_THEME=xyz:pink`, which
/// `azul_core::app_theme::app_theme` resolves before a window is built) gives the window's
/// cascade context the prefix chain over the default theme.
#[test]
fn a_spin_off_app_theme_gives_the_window_its_prefix_chain_over_the_default() {
    let lw = window(defaults::macos_modern_light(), "xyz:pink");
    let ctx = lw.dynamic_selector_context(&window_state(WindowTheme::LightMode));
    let chain: Vec<&str> = ctx.theme_chain.as_ref().iter().map(AzString::as_str).collect();
    assert_eq!(chain, ["xyz:pink", "xyz", "flat"]);
    assert_eq!(ctx.app_theme(), "xyz:pink");
}

/// A theme switch rebuilds the DOM in the shells, but the cascade must not depend on that: the
/// SAME retained DOM, offered a context with another app theme, re-resolves its blocks.
#[test]
fn a_retained_dom_follows_the_windows_app_theme() {
    let mut lw = window(defaults::macos_modern_light(), "flat");
    let ws = window_state(WindowTheme::LightMode);
    lay_out(&mut lw, stylesheet_document(), &ws);
    assert_eq!(box_fill(&lw, 40.0, 20.0), Some(FLAT));

    for (app_theme, want) in [("flora", FLORA), ("flat", FLAT)] {
        lw.app_theme = AzString::from(app_theme.to_string());
        let retained = lw
            .layout_results
            .remove(&DomId::ROOT_ID)
            .expect("laid out")
            .styled_dom;
        lay_out(&mut lw, retained, &ws);
        assert_eq!(box_fill(&lw, 40.0, 20.0), Some(want), "back and forth: {app_theme}");
    }
}

/// The widget shape: inline declarations carrying every theme's block.
#[test]
fn a_widgets_inline_theme_blocks_paint_the_active_theme_in_both_colour_schemes() {
    if scheme_pinned() {
        return;
    }
    for (app_theme, scheme, want) in [
        ("flat", WindowTheme::LightMode, FLAT),
        ("flat", WindowTheme::DarkMode, FLAT),
        ("flora", WindowTheme::LightMode, FLORA),
        ("flora", WindowTheme::DarkMode, FLORA_NIGHT),
        // No block of its own: the default theme's, the floor of every chain.
        ("monokai", WindowTheme::LightMode, FLAT),
    ] {
        let desktop = match scheme {
            WindowTheme::LightMode => defaults::macos_modern_light(),
            WindowTheme::DarkMode => defaults::macos_modern_dark(),
        };
        let mut lw = window(desktop, app_theme);
        lay_out(&mut lw, widget_document(), &window_state(scheme));
        assert_eq!(
            box_fill(&lw, 40.0, 20.0),
            Some(want),
            "app theme {app_theme}, {scheme:?}"
        );
    }
}
