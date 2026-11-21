//! The macOS titlebar lines up with the bar AppKit draws.
//!
//! Measured through AppKit on macOS 15.5
//! (`scripts/NATIVE_WIDGET_LOOK_REFERENCE_2026_09_28.md`, section 4.1): a
//! window without a toolbar has a 28pt titlebar, its traffic lights are 12pt
//! circles centred on the bar's midline (y = 14), and its title is centred on
//! the WINDOW's width. A `NoTitle` / `NoTitleAutoInject` window keeps that
//! band and those lights, so a title the app draws has to land on the same
//! line, or it reads as a second bar pasted under the real one.
//!
//! These tests lay the bar out and read the rects: the bar's height, where
//! the title's text run is centred horizontally, and where its line box is
//! centred vertically.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::{css::Css, system::defaults};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, widgets::titlebar::Titlebar, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The width of the window the reference measured in.
pub(crate) const WINDOW_WIDTH: f32 = 480.0;
/// A macOS titlebar without a toolbar. 38 is the height WITH a unified
/// compact toolbar, 52 with a unified one.
pub(crate) const NATIVE_BAR_HEIGHT: f32 = 28.0;
/// The traffic lights' centre line: `standardWindowButton` frames are 14x16
/// at y = 6 in the 28pt band, with or without `FullSizeContentView`.
pub(crate) const TRAFFIC_LIGHT_CENTRE_Y: f32 = 14.0;
/// How far from a line a centre may be and still read as ON it.
pub(crate) const TOLERANCE: f32 = 0.5;

/// The system fonts, scanned once for every test in this file.
fn fonts() -> FcFontCache {
    static FONTS: std::sync::OnceLock<FcFontCache> = std::sync::OnceLock::new();
    FONTS.get_or_init(FcFontCache::build).clone()
}

/// `root` laid out in a `WINDOW_WIDTH` x 300 window, styled with `css`.
pub(crate) fn laid_out(mut root: Dom, css: Css) -> LayoutWindow {
    let mut lw = LayoutWindow::new(fonts()).expect("a layout window");
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(WINDOW_WIDTH, 300.0);
    lw.current_window_state = window_state.clone();
    let styled = StyledDom::create(&mut root, css);
    let mut debug_messages = None;
    lw.layout_and_generate_display_list(
        styled,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug_messages,
    )
    .expect("layout");
    lw
}

/// `bar` as the first thing in a margin-less body, which is where the shell
/// puts the bar it injects.
fn in_a_window(bar: Dom) -> Dom {
    Dom::create_body().with_css("margin: 0px;").with_child(bar)
}

/// Every node of the root DOM carrying the class `class`, in document order.
pub(crate) fn nodes_with_class(lw: &LayoutWindow, class: &str) -> Vec<NodeId> {
    let lr = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("a layout result");
    let container = lr.styled_dom.node_data.as_container();
    (0..container.len())
        .map(NodeId::new)
        .filter(|nid| {
            container[*nid].attributes().as_ref().iter().any(|a| {
                a.as_class().is_some_and(|c| {
                    let s: &str = c;
                    s == class
                })
            })
        })
        .collect()
}

/// The border box of `node` as laid out.
pub(crate) fn border_box(lw: &LayoutWindow, node: NodeId) -> LogicalRect {
    let lr = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("a layout result");
    let idx = *lr
        .layout_tree
        .dom_to_layout
        .get(&node)
        .and_then(|indices| indices.first())
        .expect("the node has a layout box");
    let origin = lr
        .calculated_positions
        .get(idx.index())
        .copied()
        .expect("the node has a position");
    let size = lr
        .layout_tree
        .get(idx)
        .and_then(|n| n.used_size)
        .expect("the node has a size");
    LogicalRect::new(origin, size)
}

/// The vertical centre of a box.
pub(crate) fn centre_y(r: LogicalRect) -> f32 {
    r.origin.y + r.size.height / 2.0
}

/// The horizontal centre of the text run laid out in the inline formatting
/// context `ifc_root` establishes. The root has no padding or border.
pub(crate) fn text_run_centre_x(lw: &LayoutWindow, ifc_root: NodeId) -> f32 {
    let lr = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("a layout result");
    let idx = *lr
        .layout_tree
        .dom_to_layout
        .get(&ifc_root)
        .and_then(|indices| indices.first())
        .expect("the text's block has a layout box");
    let run = lr
        .layout_tree
        .materialized_inline_layout_for_node(idx.index())
        .expect("the block lays out its text")
        .bounds();
    assert!(run.width > 0.0, "premise: the title's text was shaped");
    border_box(lw, ifc_root).origin.x + run.x + run.width / 2.0
}

/// The title's line box: `.csd-title > p > text`, and the `<p>` (the title's
/// first child) is the block that holds the line.
fn title_line(lw: &LayoutWindow) -> NodeId {
    let title = *nodes_with_class(lw, "csd-title")
        .first()
        .expect("the bar renders a .csd-title");
    NodeId::new(title.index() + 1)
}

/// The bar `NoTitleAutoInject` injects on macOS: `Titlebar::from_system_style`
/// in title-only mode, AppKit drawing the traffic lights over it.
fn injected_macos_bar() -> LayoutWindow {
    let style = defaults::macos_modern_light();
    laid_out(
        in_a_window(Titlebar::from_system_style("Window Title".into(), &style).dom()),
        Css::empty(),
    )
}

#[test]
fn the_injected_macos_titlebar_is_28px_tall() {
    let lw = injected_macos_bar();
    let bar = border_box(&lw, nodes_with_class(&lw, "csd-titlebar")[0]);
    assert!(
        (bar.size.height - NATIVE_BAR_HEIGHT).abs() < 0.01,
        "the bar is {}px tall, AppKit's titlebar is {NATIVE_BAR_HEIGHT}",
        bar.size.height
    );
}

#[test]
fn the_injected_macos_title_is_centred_on_the_window() {
    let lw = injected_macos_bar();
    let x = text_run_centre_x(&lw, title_line(&lw));
    assert!(
        (x - WINDOW_WIDTH / 2.0).abs() <= TOLERANCE,
        "the title is centred at x = {x}, the window at {}",
        WINDOW_WIDTH / 2.0
    );
}

/// The title's line box is centred on the traffic lights' line.
///
/// The widget used to centre it with `padding-top: (height - font_size) / 2`,
/// as if the line were exactly `font_size` tall. A line box is taller than
/// its font size (about 1.2x), so the title sat most of a pixel low, and the
/// truncation to whole pixels moved it by another half.
#[test]
fn the_injected_macos_title_sits_on_the_traffic_lights_line() {
    let lw = injected_macos_bar();
    let line = border_box(&lw, title_line(&lw));
    let y = centre_y(line);
    assert!(
        (y - TRAFFIC_LIGHT_CENTRE_Y).abs() <= TOLERANCE,
        "the title's line box is centred at y = {y}, the traffic lights at \
         {TRAFFIC_LIGHT_CENTRE_Y} (line box {line:?})"
    );
}

/// A full client-side bar (`WindowDecorations::None`): its own controls and
/// its title on ONE line, and the title on the window's middle.
///
/// The title kept its `padding-top` inside a bar that already centres its
/// children (`align-items: center`), so the padding pushed it down by half
/// again: about 3.5px below the controls.
#[test]
fn a_csd_titlebars_title_sits_on_the_line_of_its_controls() {
    let style = defaults::macos_modern_light();
    let tm = &style.metrics.titlebar;
    let bar = Titlebar::from_system_style_csd("Window Title".into(), &style)
        .dom_with_buttons(&tm.buttons, tm.button_side);
    let lw = laid_out(in_a_window(bar), style.create_csd_stylesheet());

    let line = title_line(&lw);
    let title_y = centre_y(border_box(&lw, line));
    let controls = nodes_with_class(&lw, "csd-button");
    assert!(!controls.is_empty(), "premise: the bar renders its controls");
    for control in controls {
        let control_y = centre_y(border_box(&lw, control));
        assert!(
            (title_y - control_y).abs() <= TOLERANCE,
            "the title is centred at y = {title_y}, the control {control:?} at {control_y}"
        );
    }

    let x = text_run_centre_x(&lw, line);
    assert!(
        (x - WINDOW_WIDTH / 2.0).abs() <= TOLERANCE,
        "the title is centred at x = {x}, the window at {}",
        WINDOW_WIDTH / 2.0
    );
}

/// A macOS titlebar has no fill of its own, and the system separator under it.
///
/// With a transparent titlebar (`NoTitle`, `NoTitleAutoInject`) the window's
/// own background shows through the bar, so the widget paints none. The line
/// under a standard titlebar is one device pixel (0.5pt): #D0D0D0 in light
/// mode, #000000 in dark mode (measured through AppKit, reference section
/// 4.1). The widget had no separator at all, so the bar ran into the content
/// with nothing between them.
///
/// The injected bar follows the app theme (W5b): it carries the flat look
/// and the flora look, each in its `@theme(<name>)` block. This is the
/// NATIVE look, so it reads the bar at rest under the default app theme,
/// flat: the unconditional declarations and flat's block.
#[test]
fn the_macos_titlebar_has_no_fill_and_the_system_separator() {
    use azul_css::{
        dynamic_selector::{DynamicSelector, ThemeCondition},
        props::{basic::color::ColorU, property::CssProperty, style::BorderStyle},
    };
    let at_rest_under_flat = |conditions: &[DynamicSelector]| {
        conditions.iter().all(|c| {
            matches!(c, DynamicSelector::Theme(ThemeCondition::Custom(name)) if name.as_str() == "flat")
        })
    };

    for (style, line) in [
        (
            defaults::macos_modern_light(),
            ColorU::new_rgb(0xD0, 0xD0, 0xD0),
        ),
        (defaults::macos_modern_dark(), ColorU::new_rgb(0, 0, 0)),
    ] {
        let theme = style.mode;
        let bar = Titlebar::from_system_style("Window Title".into(), &style).dom();
        let resting: Vec<CssProperty> = bar
            .root
            .style
            .iter_inline_properties()
            .filter(|(_, conditions)| at_rest_under_flat(conditions.as_ref()))
            .map(|(p, _)| p.clone())
            .collect();

        assert!(
            !resting
                .iter()
                .any(|p| matches!(p, CssProperty::BackgroundContent(_))),
            "{theme:?}: the bar paints a fill of its own"
        );
        let colour = resting.iter().find_map(|p| match p {
            CssProperty::BorderBottomColor(v) => v.get_property().map(|c| c.inner),
            _ => None,
        });
        assert_eq!(colour, Some(line), "{theme:?}: the separator's colour");
        let width = resting.iter().find_map(|p| match p {
            CssProperty::BorderBottomWidth(v) => v.get_property().map(|w| w.inner.number.get()),
            _ => None,
        });
        assert!(
            width.is_some_and(|w| w > 0.0),
            "{theme:?}: the separator has no width ({width:?})"
        );
        assert!(
            resting.iter().any(|p| matches!(
                p,
                CssProperty::BorderBottomStyle(v)
                    if v.get_property().is_some_and(|s| s.inner == BorderStyle::Solid)
            )),
            "{theme:?}: the separator is not a solid line"
        );
    }
}

// ---------------------------------------------------------------------------
// The AzWidgets demo draws its own bar under `WindowDecorations::NoTitle`:
// AppKit keeps the 28pt band and the traffic lights, the demo draws the rest
// with azul's `Titlebar` widget (`Titlebar::create`, whose metrics are the
// platform's: 28px and the traffic lights' room on macOS). Off macOS there
// are no traffic lights to line up with and the widget takes that platform's
// height, so these three run on macOS only.
// ---------------------------------------------------------------------------

/// The demo's titlebar as the demo builds it, laid out in the demo's body:
/// `body > bar(1) > title(2) > p(3) > text(4)`, the body carrying the inline
/// style the demo's source gives it (read from the source, as the demo's
/// theme tests read it).
#[cfg(target_os = "macos")]
fn demo_bar() -> LayoutWindow {
    let f = crate::azul_widgets_demo_follows_the_theme::page_frame();
    let root = Dom::create_body()
        .with_css(&f.body)
        .with_child(f.titlebar());
    laid_out(root, Css::empty())
}

#[cfg(target_os = "macos")]
const DEMO_BAR: NodeId = NodeId::new(1);

/// 38 is the unified-compact TOOLBAR height. The demo has no toolbar, so its
/// traffic lights stay on the 28pt band's midline, 5px above the middle of a
/// 38px bar.
#[cfg(target_os = "macos")]
#[test]
fn the_demo_titlebar_is_28px_tall() {
    let lw = demo_bar();
    let bar = border_box(&lw, DEMO_BAR);
    assert!(
        (bar.size.height - NATIVE_BAR_HEIGHT).abs() < 0.01,
        "the demo's bar is {}px tall, AppKit's titlebar is {NATIVE_BAR_HEIGHT}",
        bar.size.height
    );
}

/// The demo's title is centred on the window, like AppKit's; it was left
/// aligned after an 82px padding.
#[cfg(target_os = "macos")]
#[test]
fn the_demo_title_is_centred_on_the_window() {
    let lw = demo_bar();
    let x = text_run_centre_x(&lw, title_line(&lw));
    assert!(
        (x - WINDOW_WIDTH / 2.0).abs() <= TOLERANCE,
        "the demo's title is centred at x = {x}, the window at {}",
        WINDOW_WIDTH / 2.0
    );
}

#[cfg(target_os = "macos")]
#[test]
fn the_demo_title_sits_on_the_traffic_lights_line() {
    let lw = demo_bar();
    let line = border_box(&lw, title_line(&lw));
    let y = centre_y(line);
    assert!(
        (y - TRAFFIC_LIGHT_CENTRE_Y).abs() <= TOLERANCE,
        "the demo's title is centred at y = {y}, the traffic lights at \
         {TRAFFIC_LIGHT_CENTRE_Y} (title box {line:?})"
    );
}
