//! The app's colour-scheme choice: follow the desktop, or pin light / dark.
//!
//! `AppConfig::color_scheme` (startup) and `CallbackInfo::set_color_scheme`
//! (runtime) hold ONE app-wide choice, an `OptionWindowTheme`: `None` follows
//! the desktop, `Some(theme)` pins it. What a window shows is decided in ONE
//! place, `azul_layout::window::resolve_window_theme` (theme-chain invariant
//! I1), with the precedence
//!
//! ```text
//! AZ_THEME env pin  >  the app's choice  >  the window's own theme  >  the desktop
//! ```
//!
//! (the window's own theme is what the shells keep in `FullWindowState::theme`:
//! the desktop's, or the `WindowCreateOptions::theme` seed it was created with).
//!
//! Each check pins BOTH directions where it can: a pin that "works" by
//! resolving to one fixed scheme would pass a one-sided assertion.

use std::sync::Arc;

use azul_core::{
    callbacks::{SystemStyleDependencies, SystemStyleDependency},
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
    window::{OptionWindowTheme, WindowTheme},
};
use azul_css::{
    dynamic_selector::ThemeCondition,
    props::basic::color::{ColorU, SystemColorRef},
    system::{defaults, SystemStyle},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::DisplayListItem,
    window::{resolve_window_theme_with, LayoutWindow},
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const PIN_LIGHT: OptionWindowTheme = OptionWindowTheme::Some(WindowTheme::LightMode);
const PIN_DARK: OptionWindowTheme = OptionWindowTheme::Some(WindowTheme::DarkMode);
const FOLLOW: OptionWindowTheme = OptionWindowTheme::None;

/// `AZ_THEME` outranks everything these tests vary; under it they have
/// nothing to compare (the env test below covers that case).
fn env_pinned() -> bool {
    azul_css::dynamic_selector::theme_pinned_by_env().is_some()
}

/// A window on `desktop`, with the app's choice `app` mirrored into it.
fn window(desktop: SystemStyle, app: OptionWindowTheme) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.set_system_style(Arc::new(desktop));
    lw.color_scheme = app;
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

/// `body > p > "5"`, no colour declared anywhere: the text takes the UA's
/// default, which is the theme's (black on light, near-white on dark).
fn unstyled_text() -> StyledDom {
    let dom = Dom::create_body().with_child(
        Dom::create_p().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("5")),
    );
    StyledDom::create_from_dom(dom)
}

/// `body > div(40x20, system:window-background)`.
fn window_background_box() -> StyledDom {
    let dom = Dom::create_body().with_child(Dom::create_div().with_css(
        "width: 40px; height: 20px; background-color: system:window-background;",
    ));
    StyledDom::create_from_dom(dom)
}

fn painted_text(lw: &LayoutWindow) -> Vec<ColorU> {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { color, .. } => Some(*color),
            _ => None,
        })
        .collect()
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

fn is_light(c: &ColorU) -> bool {
    c.r >= 0xd0 && c.g >= 0xd0 && c.b >= 0xd0
}

fn is_dark(c: &ColorU) -> bool {
    c.r < 0x30 && c.g < 0x30 && c.b < 0x30
}

fn declared(deps: &[SystemStyleDependency]) -> SystemStyleDependencies {
    let mut set = SystemStyleDependencies::empty();
    for d in deps {
        set.insert(*d);
    }
    set
}

#[test]
fn an_app_pinned_dark_renders_dark_on_a_light_desktop() {
    if env_pinned() {
        return;
    }
    let light_desktop = defaults::macos_modern_light();
    let light_bg = light_desktop
        .colors
        .window_background
        .into_option()
        .expect("the macOS light preset fills the window background");
    let mut lw = window(light_desktop, PIN_DARK);

    // The window follows its light desktop; only the app's pin says dark.
    assert_eq!(
        lw.window_theme_for(WindowTheme::LightMode),
        WindowTheme::DarkMode,
        "the app's pin outranks the desktop"
    );
    // The ONE decision function applies the pin even when handed the
    // desktop-following theme, so no caller can bypass it (I1).
    let ws = window_state(WindowTheme::LightMode);
    let ctx = lw.dynamic_selector_context(&ws);
    assert_eq!(
        ctx.theme,
        ThemeCondition::Dark,
        "an app pinned dark must get `@theme dark` / prefers-color-scheme: dark rules"
    );

    // The `system:` palette is the DARK one, not the light desktop's: the
    // light preset has no dark palette, so every slot takes the keyword's
    // dark default.
    let dark_bg = SystemColorRef::WindowBackground.fallback(true);
    assert_ne!(dark_bg, light_bg, "premise: the two palettes differ");
    assert_eq!(
        ctx.system_color(SystemColorRef::WindowBackground),
        dark_bg,
        "system:window-background under a dark pin on a light desktop"
    );

    // ... and it is what gets painted.
    let ws = window_state(lw.window_theme_for(WindowTheme::LightMode));
    lay_out(&mut lw, window_background_box(), &ws);
    assert_eq!(
        box_fill(&lw, 40.0, 20.0),
        Some(dark_bg),
        "the 40x20 box must paint the dark window background"
    );
}

#[test]
fn an_app_pinned_light_renders_light_on_a_dark_desktop() {
    if env_pinned() {
        return;
    }
    let mut lw = window(defaults::macos_modern_dark(), PIN_LIGHT);
    assert_eq!(
        lw.window_theme_for(WindowTheme::DarkMode),
        WindowTheme::LightMode
    );
    let ws = window_state(lw.window_theme_for(WindowTheme::DarkMode));
    assert_eq!(
        lw.dynamic_selector_context(&ws).theme,
        ThemeCondition::Light
    );
    lay_out(&mut lw, unstyled_text(), &ws);
    let text = painted_text(&lw);
    assert!(!text.is_empty(), "the text run must be painted");
    assert!(
        text.iter().all(is_dark),
        "an app pinned light on a dark desktop paints black text, got {text:?}"
    );
}

#[test]
fn switching_the_app_back_to_system_follows_the_desktop_again() {
    if env_pinned() {
        return;
    }
    for (desktop, desktop_theme, want, pinned_before) in [
        (
            defaults::macos_modern_light(),
            WindowTheme::LightMode,
            ThemeCondition::Light,
            PIN_DARK,
        ),
        (
            defaults::macos_modern_dark(),
            WindowTheme::DarkMode,
            ThemeCondition::Dark,
            PIN_LIGHT,
        ),
    ] {
        let desktop_bg = desktop
            .colors
            .window_background
            .into_option()
            .expect("the macOS presets fill the window background");
        let mut lw = window(desktop, pinned_before);
        assert_ne!(
            lw.window_theme_for(desktop_theme),
            desktop_theme,
            "premise: pinned to the other scheme"
        );

        lw.color_scheme = FOLLOW;
        assert_eq!(
            lw.window_theme_for(desktop_theme),
            desktop_theme,
            "System follows the desktop"
        );
        let ws = window_state(lw.window_theme_for(desktop_theme));
        let ctx = lw.dynamic_selector_context(&ws);
        assert_eq!(ctx.theme, want, "System on a {desktop_theme:?} desktop");
        assert_eq!(
            ctx.system_color(SystemColorRef::WindowBackground),
            desktop_bg,
            "back on System the desktop's OWN palette applies again, not a fallback"
        );
    }
}

#[test]
fn a_desktop_flip_while_pinned_light_keeps_the_app_light() {
    if env_pinned() {
        return;
    }
    let mut lw = window(defaults::macos_modern_light(), PIN_LIGHT);
    let ws = window_state(lw.window_theme_for(WindowTheme::LightMode));
    lay_out(&mut lw, unstyled_text(), &ws);
    // The app's layout() read the scheme (`get_theme`) and nothing else.
    lw.recorded_style_dependencies = declared(&[SystemStyleDependency::Theme]);

    // The desktop goes dark.
    let old_style = lw.system_style.clone().expect("the light desktop style");
    let new_style = Arc::new(defaults::macos_modern_dark());
    let window_theme = lw.window_theme_for(WindowTheme::DarkMode);
    assert_eq!(
        window_theme,
        WindowTheme::LightMode,
        "a desktop flip does not move a pinned app"
    );
    assert!(
        !lw.system_style_change_needs_full_regeneration(&old_style, &new_style),
        "layout() read the scheme, and the scheme it reads (the pin) did not move: the flip needs \
         no new DOM"
    );

    lw.set_system_style(new_style);
    let ws = window_state(window_theme);
    assert_eq!(
        lw.dynamic_selector_context(&ws).theme,
        ThemeCondition::Light,
        "the cascade stays light"
    );
    let retained = lw
        .layout_results
        .remove(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom;
    lay_out(&mut lw, retained, &ws);
    let text = painted_text(&lw);
    assert!(!text.is_empty(), "the text run must be painted");
    assert!(
        text.iter().all(is_dark),
        "after the desktop went dark the pinned-light app still paints black text, got {text:?}"
    );
}

/// The contrast to the test above: an app that FOLLOWS the desktop and read
/// the scheme is rebuilt when the desktop flips - its DOM saw the old scheme.
#[test]
fn a_desktop_flip_rebuilds_an_app_that_follows_it_and_read_the_scheme() {
    if env_pinned() {
        return;
    }
    let mut lw = window(defaults::macos_modern_light(), FOLLOW);
    let ws = window_state(WindowTheme::LightMode);
    lay_out(&mut lw, unstyled_text(), &ws);
    lw.recorded_style_dependencies = declared(&[SystemStyleDependency::Theme]);
    let old_style = lw.system_style.clone().expect("the light desktop style");
    let new_style = Arc::new(defaults::macos_modern_dark());
    assert!(lw.system_style_change_needs_full_regeneration(&old_style, &new_style));
}

#[test]
fn a_scheme_switch_recolours_the_retained_dom_without_a_new_one() {
    if env_pinned() {
        return;
    }
    let mut lw = window(defaults::macos_modern_light(), FOLLOW);
    let ws = window_state(WindowTheme::LightMode);
    lay_out(&mut lw, unstyled_text(), &ws);
    let before = painted_text(&lw);
    assert!(!before.is_empty() && before.iter().all(is_dark), "light first: {before:?}");

    // layout() declared nothing about the OS style: the scheme is paint-only
    // for it (I6), so the switch re-styles the DOM it already has.
    lw.recorded_style_dependencies = SystemStyleDependencies::empty();
    lw.color_scheme = PIN_DARK;
    assert!(
        !lw.color_scheme_change_needs_new_dom(),
        "a layout() that never read the scheme must not be re-run for a scheme switch"
    );

    let ws = window_state(lw.window_theme_for(WindowTheme::LightMode));
    let retained = lw
        .layout_results
        .remove(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom;
    lay_out(&mut lw, retained, &ws);
    let after = painted_text(&lw);
    assert!(!after.is_empty(), "the text run must be painted");
    assert!(
        after.iter().all(is_light),
        "the retained DOM re-styled under the dark pin paints near-white text, got {after:?}"
    );

    // And back to System: light again, same DOM.
    lw.color_scheme = FOLLOW;
    let ws = window_state(lw.window_theme_for(WindowTheme::LightMode));
    let retained = lw
        .layout_results
        .remove(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom;
    lay_out(&mut lw, retained, &ws);
    let back = painted_text(&lw);
    assert!(
        !back.is_empty() && back.iter().all(is_dark),
        "back on System (light desktop) the text is black again, got {back:?}"
    );
}

/// The `depends_on_locale` rule for the scheme: `layout()` is re-run for a
/// scheme switch exactly when it READ the scheme - `get_theme` records
/// `Theme`, `get_system_style` records `Everything`.
#[test]
fn a_layout_that_read_the_scheme_needs_a_new_dom_on_a_switch() {
    let mut lw = window(defaults::macos_modern_light(), FOLLOW);
    assert!(
        lw.color_scheme_change_needs_new_dom(),
        "nothing laid out yet: there is no DOM to re-style"
    );
    lay_out(
        &mut lw,
        unstyled_text(),
        &window_state(WindowTheme::LightMode),
    );

    for (deps, rebuild) in [
        (declared(&[]), false),
        (declared(&[SystemStyleDependency::Theme]), true),
        (declared(&[SystemStyleDependency::Everything]), true),
        (declared(&[SystemStyleDependency::Colors]), false),
        (
            declared(&[SystemStyleDependency::Fonts, SystemStyleDependency::Metrics]),
            false,
        ),
    ] {
        lw.recorded_style_dependencies = deps;
        assert_eq!(
            lw.color_scheme_change_needs_new_dom(),
            rebuild,
            "declared {:#b}",
            deps.facets
        );
    }
}

/// The precedence itself, on the pure decision function (so it is checked
/// whatever `AZ_THEME` the test process runs under).
#[test]
fn the_env_pin_outranks_the_app_which_outranks_the_window() {
    use WindowTheme::{DarkMode, LightMode};

    let env_light = Some(ThemeCondition::Light);
    let env_dark = Some(ThemeCondition::Dark);
    for app in [FOLLOW, PIN_LIGHT, PIN_DARK] {
        for own in [LightMode, DarkMode] {
            assert_eq!(
                resolve_window_theme_with(env_light.clone(), app, own),
                LightMode,
                "AZ_THEME=light wins over app {app:?} and window {own:?}"
            );
            assert_eq!(
                resolve_window_theme_with(env_dark.clone(), app, own),
                DarkMode,
                "AZ_THEME=dark wins over app {app:?} and window {own:?}"
            );
        }
    }
    for own in [LightMode, DarkMode] {
        assert_eq!(resolve_window_theme_with(None, PIN_LIGHT, own), LightMode);
        assert_eq!(resolve_window_theme_with(None, PIN_DARK, own), DarkMode);
        assert_eq!(
            resolve_window_theme_with(None, FOLLOW, own),
            own,
            "System: the window's own (desktop-following) theme"
        );
    }
}

/// The live half of the env rule, meaningful only in a process that runs
/// under `AZ_THEME` (screenshot / CI runs): there the pin beats the app's
/// choice in the context every cascade evaluates against.
#[test]
fn under_az_theme_the_env_pin_wins_over_the_apps_choice() {
    let Some(pinned) = azul_css::dynamic_selector::theme_pinned_by_env() else {
        return;
    };
    for app in [FOLLOW, PIN_LIGHT, PIN_DARK] {
        let lw = window(defaults::macos_modern_light(), app);
        for own in [WindowTheme::LightMode, WindowTheme::DarkMode] {
            assert_eq!(
                lw.dynamic_selector_context(&window_state(own)).theme,
                pinned,
                "AZ_THEME outranks the app's {app:?}"
            );
        }
    }
}
