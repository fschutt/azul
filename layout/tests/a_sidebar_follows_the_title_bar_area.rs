//! A sidebar that keeps clear of the window's own controls through CSS: its
//! padding reads the window's title bar area, `env(titlebar-area-*)` (the
//! Window Controls Overlay names), or azul's `system:titlebar-*` spelling of
//! the same values.
//!
//! The area belongs to a window whose content runs under its title bar (a
//! `NoTitle` window out of full screen) on a platform that draws its controls
//! over the content (macOS's traffic lights): the strip along the top edge,
//! less the controls and the padding beside them. Everywhere else there is no
//! area, and `env()` takes its fallback, or zero without one.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
    window::{WindowDecorations, WindowFrame},
};
use azul_css::{
    props::basic::pixel::{OptionPixelValue, PixelValue},
    system::{Platform, SystemStyle, TitlebarButtonSide, TitlebarMetrics},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const SIDEBAR: usize = 1;
const CONTENT: usize = 2;

/// A desktop on `platform` whose title bar is `height` px tall, with 78px of
/// controls on the left and 8px of padding beside them (macOS's geometry).
fn style(platform: Platform, height: f32) -> SystemStyle {
    // `SystemStyle` implements `Drop`, so it is built field by field.
    let mut style = SystemStyle::default();
    style.platform = platform;
    style.metrics.titlebar = TitlebarMetrics {
        button_side: TitlebarButtonSide::Left,
        height: OptionPixelValue::Some(PixelValue::px(height)),
        button_area_width: OptionPixelValue::Some(PixelValue::px(78.0)),
        padding_horizontal: OptionPixelValue::Some(PixelValue::px(8.0)),
        ..TitlebarMetrics::default()
    };
    style
}

/// A row of a sidebar padded by the title bar area and a content pane that
/// reads nothing of it.
fn page(sidebar_css: &str) -> Dom {
    Dom::create_body()
        .with_css("margin: 0px; display: flex; flex-direction: row; align-items: flex-start;")
        .with_child(
            Dom::create_div()
                .with_css(&format!("flex-shrink: 0; width: 200px; height: 10px; {sidebar_css}")),
        )
        .with_child(Dom::create_div().with_css("flex-shrink: 0; width: 50px; height: 10px;"))
}

const SIDEBAR_CSS: &str =
    "padding-top: env(titlebar-area-height); padding-left: env(titlebar-area-x, 12px);";

fn window_state(decorations: WindowDecorations) -> FullWindowState {
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 300.0);
    ws.flags.decorations = decorations;
    ws
}

fn lay_out(lw: &mut LayoutWindow, styled: StyledDom, ws: &FullWindowState) {
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
}

fn window(style: SystemStyle, sidebar_css: &str, ws: &FullWindowState) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.system_style = Some(Arc::new(style));
    let mut dom = page(sidebar_css);
    lay_out(
        &mut lw,
        StyledDom::create(&mut dom, azul_css::css::Css::empty()),
        ws,
    );
    lw
}

/// Lays the window's own styled DOM out again (no rebuild of the app's DOM).
fn relayout(lw: &mut LayoutWindow, ws: &FullWindowState) {
    let result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
    lay_out(lw, result.styled_dom, ws);
}

fn rect(lw: &LayoutWindow, node: usize) -> LogicalRect {
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: Some(NodeId::new(node)).into(),
    })
    .expect("the node has a rect")
}

#[test]
fn a_sidebar_padded_by_the_title_bar_area_follows_a_change_of_the_metrics_alone() {
    let mut ws = window_state(WindowDecorations::NoTitle);
    let mut lw = window(style(Platform::MacOs, 28.0), SIDEBAR_CSS, &ws);
    let sidebar = rect(&lw, SIDEBAR);
    assert_eq!(
        sidebar.size.height, 38.0,
        "10px of content under the 28px title bar area: {sidebar:?}"
    );
    assert_eq!(
        sidebar.size.width, 286.0,
        "200px of content after the 78px of traffic lights and the 8px beside them: {sidebar:?}"
    );
    let content = rect(&lw, CONTENT);

    // The desktop's title bar grows: the same DOM, laid out again.
    lw.system_style = Some(Arc::new(style(Platform::MacOs, 38.0)));
    relayout(&mut lw, &ws);
    assert_eq!(
        rect(&lw, SIDEBAR).size.height,
        48.0,
        "the sidebar follows the new title bar height"
    );
    assert_eq!(rect(&lw, CONTENT), content, "the content pane reads none of it");
    assert_eq!(
        lw.layout_cache.last_reconcile_fresh, 1,
        "only the box that reads the title bar area is laid out anew, not the page (reused {})",
        lw.layout_cache.last_reconcile_reused
    );

    // Full screen hides the title bar: no area, so the fallback or zero.
    ws.flags.frame = WindowFrame::Fullscreen;
    relayout(&mut lw, &ws);
    let sidebar = rect(&lw, SIDEBAR);
    assert_eq!(sidebar.size.height, 10.0, "no area: zero without a fallback");
    assert_eq!(sidebar.size.width, 212.0, "no area: the 12px fallback");
}

#[test]
fn a_window_without_a_title_bar_area_takes_the_fallback_or_zero() {
    // Windows keeps its caption above the content, and a window with its
    // full title bar has nothing of its content under it.
    for (platform, decorations) in [
        (Platform::Windows, WindowDecorations::NoTitle),
        (Platform::MacOs, WindowDecorations::Normal),
    ] {
        let ws = window_state(decorations);
        let lw = window(style(platform.clone(), 28.0), SIDEBAR_CSS, &ws);
        let sidebar = rect(&lw, SIDEBAR);
        assert_eq!(
            (sidebar.size.width, sidebar.size.height),
            (212.0, 10.0),
            "{platform:?} {decorations:?}: the 12px fallback on the left, zero on top"
        );
    }
}

#[test]
fn the_system_titlebar_lengths_are_the_title_bar_areas_values() {
    let ws = window_state(WindowDecorations::NoTitle);
    let lw = window(
        style(Platform::MacOs, 28.0),
        "padding-top: system:titlebar-height; padding-left: system:titlebar-button-width;",
        &ws,
    );
    let sidebar = rect(&lw, SIDEBAR);
    assert_eq!(
        (sidebar.size.width, sidebar.size.height),
        (278.0, 38.0),
        "the 78px of controls on the left, the 28px of the title bar on top"
    );
}
