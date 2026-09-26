//! A `system:` colour keyword paints the palette of the theme it is rendered
//! in.
//!
//! `system:<name>` is how a stylesheet says "the desktop's window background"
//! or "the desktop's label colour" instead of hard-coding one hex value that
//! is right in one theme and wrong in the other. The keyword only earns that
//! if it reaches the paint, resolved against the SAME theme the cascade used:
//! a light field on a dark card is exactly the inconsistency the keywords are
//! meant to rule out.
//!
//! Each check runs under the macOS light AND dark presets, so a keyword that
//! "works" by resolving to one fixed colour cannot pass: the two presets
//! disagree on every slot asked for here.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
    window::WindowTheme,
};
use azul_css::{
    dynamic_selector::DynamicSelectorContext,
    props::basic::{color::ColorU, PhysicalSize},
    system::{defaults, SystemStyle, Theme},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const THEMES: [Theme; 2] = [Theme::Light, Theme::Dark];

fn preset(theme: Theme) -> SystemStyle {
    match theme {
        Theme::Light => defaults::macos_modern_light(),
        Theme::Dark => defaults::macos_modern_dark(),
    }
}

fn window_theme(theme: Theme) -> WindowTheme {
    match theme {
        Theme::Light => WindowTheme::LightMode,
        Theme::Dark => WindowTheme::DarkMode,
    }
}

/// The preset's own value for a slot, so the expectation is read from the
/// same palette the window is handed, never restated here.
fn slot(theme: Theme, pick: fn(&SystemStyle) -> Option<ColorU>) -> ColorU {
    pick(&preset(theme)).expect("the macOS preset fills this slot")
}

/// Lay `body > div(css)` out in a real window under `theme` and return the
/// size and fill of every rect the display list paints.
fn painted_rects(css: &str, theme: Theme) -> Vec<(f32, f32, ColorU)> {
    let dom = Dom::create_body().with_child(Dom::create_div().with_css(css));
    let styled = StyledDom::create_from_dom(dom);

    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    // Before the first layout: a system style handed over afterwards is
    // invisible to the cascade that already ran.
    lw.set_system_style(Arc::new(preset(theme)));
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(200.0, 100.0);
    ws.theme = window_theme(theme);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .unwrap();

    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("a layout result for the root DOM")
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Rect { bounds, color, .. } => {
                Some((bounds.0.size.width, bounds.0.size.height, *color))
            }
            _ => None,
        })
        .collect()
}

/// `body(0) > div(1, css) > text(2)`, cascaded under `theme`'s preset the
/// way a layout pass installs it.
fn styled_under(css: &str, theme: Theme) -> (StyledDom, Arc<SystemStyle>) {
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_css(css)
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("ink")),
    );
    let mut sd = StyledDom::create_from_dom(dom);
    let style = Arc::new(preset(theme));
    let ctx = DynamicSelectorContext::from_system_style(&style).with_viewport(800.0, 600.0);
    sd.set_dynamic_selector_context(ctx);
    (sd, style)
}

fn text_color(css: &str, theme: Theme, node: usize) -> ColorU {
    let (sd, style) = styled_under(css, theme);
    azul_layout::solver3::getters::get_style_properties(
        &sd,
        NodeId::new(node),
        Some(&style),
        PhysicalSize::new(800.0, 600.0),
    )
    .color
}

fn border_top_color(css: &str, theme: Theme) -> Option<ColorU> {
    let (sd, _style) = styled_under(css, theme);
    let div = NodeId::new(1);
    let state = sd.styled_nodes.as_container()[div].styled_node_state;
    azul_layout::solver3::getters::get_border_info(&sd, div, &state)
        .colors
        .top
        .and_then(|v| v.get_property().copied())
        .map(|c| c.inner)
}

#[test]
fn a_system_background_paints_the_window_background_of_the_theme() {
    for theme in THEMES {
        let want = slot(theme, |s| s.colors.window_background.into_option());
        for css in [
            "width: 40px; height: 20px; background-color: system:window-background;",
            "width: 40px; height: 20px; background: system:window-background;",
        ] {
            let rects = painted_rects(css, theme);
            let fill = rects
                .iter()
                .find(|(w, h, _)| (*w - 40.0).abs() < 0.01 && (*h - 20.0).abs() < 0.01)
                .map(|(_, _, c)| *c);
            assert_eq!(
                fill,
                Some(want),
                "{theme:?}: `{css}` must paint its 40x20 box in the preset's window background; \
                 rects painted: {rects:?}"
            );
        }
    }
}

#[test]
fn a_system_text_colour_resolves_to_the_label_colour_of_the_theme() {
    for theme in THEMES {
        let want = slot(theme, |s| s.colors.text.into_option());
        assert_eq!(
            text_color("color: system:text;", theme, 1),
            want,
            "{theme:?}: `color: system:text` on the div"
        );
        assert_eq!(
            text_color("color: system:text;", theme, 2),
            want,
            "{theme:?}: the text node inherits the keyword and resolves it the same way"
        );
    }
}

#[test]
fn a_system_border_colour_resolves_to_the_accent_of_the_theme() {
    for theme in THEMES {
        let want = slot(theme, |s| s.colors.accent.into_option());
        assert_eq!(
            border_top_color("border: 2px solid system:accent;", theme),
            Some(want),
            "{theme:?}: the `border` shorthand with a system colour"
        );
        assert_eq!(
            border_top_color(
                "border-width: 2px; border-style: solid; border-color: system:accent;",
                theme
            ),
            Some(want),
            "{theme:?}: the `border-color` longhand path"
        );
    }
}
