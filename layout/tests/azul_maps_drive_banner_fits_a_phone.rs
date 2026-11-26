//! The AzMaps navigation banner and bar fit a phone.
//!
//! While a route is driven the map shows a banner (the next manoeuvre) over the top and a bar (arrival, the
//! switches, End) over the bottom. Both were centred boxes of a fixed width - 460 and 560 px, `left: 50%` and a
//! negative margin - so on a 411 dp phone the banner ran 25 dp out of the screen on each side and the bar 75 dp:
//! the End key and the first switch were off the screen. On a phone (a window narrower than
//! `COMPACT_WIDTH`) they are as wide as the screen less 8 px each side; a desktop window keeps the centred
//! boxes.
//!
//! The app is an example crate this one cannot link, so this reads its SOURCE, the way
//! `azul_maps_sidebar_is_a_sheet_on_a_phone.rs` does: the styles are the constants of
//! `examples/azul-maps/src/drive_ui.rs` and `lib.rs`, the keys are the engine's own buttons in the Android
//! style, the root is the map's own root inside the bars of the emulator.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::{
    props::basic::pixel::{OptionPixelValue, PixelValue},
    system::{defaults, SafeAreaInsets},
    AzString,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, widgets::button::{Button, ButtonType}, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `examples/azul-maps/src/drive_ui.rs` and `lib.rs`, verbatim, at compile time.
const DRIVE: &str = include_str!("../../examples/azul-maps/src/drive_ui.rs");
const MAPS: &str = include_str!("../../examples/azul-maps/src/lib.rs");

/// The value of `const <name>: &str = "...";` in `source` as the compiler reads it: a `\` at the end of a line
/// continues the literal on the next one without its indentation.
fn const_css(source: &str, name: &str) -> String {
    let head = format!("const {name}: &str =");
    let start = source.find(&head).unwrap_or_else(|| panic!("the source has `{head}`"));
    let rest = &source[start + head.len()..];
    let open = rest.find('"').expect("the constant is a string literal");
    let mut out = String::new();
    let mut chars = rest[open + 1..].chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => return out,
            '\\' => match chars.next() {
                Some('\n') => {
                    while chars.peek().is_some_and(|n| n.is_whitespace()) {
                        chars.next();
                    }
                }
                Some(other) => out.push(other),
                None => break,
            },
            _ => out.push(c),
        }
    }
    panic!("the literal of {name} does not end");
}

const COMPACT_WIDTH: f32 = 560.0;

/// The words of a manoeuvre that is long in German.
const INSTRUCTION: &str = "In 300 m rechts abbiegen auf die Bismarckstrasse Richtung Hechingen und Reutlingen";
/// The bar's summary.
const SUMMARY: &str = "Ankunft 14:32 \u{b7} 18 min \u{b7} 8,4 km";

/// body > root > [banner > [icon, words > [distance, instruction]], bar > [summary, three switches, End]] in a
/// window `width` x `height` with the Android style and the emulator's bars.
fn lay_out(width: f32, height: f32) -> LayoutWindow {
    let phone = width < COMPACT_WIDTH;
    let banner_spot = const_css(DRIVE, if phone { "BANNER_SPOT_PHONE" } else { "BANNER_SPOT" });
    let bar_spot = const_css(DRIVE, if phone { "BAR_SPOT_PHONE" } else { "BAR_SPOT" });

    let banner = Dom::create_div()
        .with_css(&format!("{banner_spot} {}", const_css(DRIVE, "BANNER")))
        .with_id("banner".into())
        .with_child(Dom::create_div().with_css("width: 44px; height: 44px; flex-shrink: 0;"))
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; flex-grow: 1; min-width: 0px;")
                .with_child(
                    Dom::create_div()
                        .with_css(&const_css(DRIVE, "BANNER_DISTANCE"))
                        .with_child(Dom::create_span_with_text("300 m")),
                )
                .with_child(
                    Dom::create_div()
                        .with_css(&const_css(DRIVE, "BANNER_TEXT"))
                        .with_child(Dom::create_span_with_text(INSTRUCTION)),
                ),
        );

    let mut bar = Dom::create_div()
        .with_css(&format!("{bar_spot} {}", const_css(DRIVE, "BAR")))
        .with_id("bar".into())
        .with_child(
            Dom::create_div()
                .with_css(&const_css(DRIVE, "BAR_TEXT"))
                .with_child(Dom::create_span_with_text(SUMMARY)),
        );
    for icon in ["navigation", "volume_up", "directions_car"] {
        bar.add_child(Button::create(AzString::from("")).with_icon(AzString::from(icon)).dom());
    }
    bar.add_child(Button::with_type(AzString::from("Beenden"), ButtonType::Primary).dom().with_id("end".into()));

    let root = Dom::create_div()
        .with_css(&const_css(MAPS, "ROOT"))
        .with_child(banner)
        .with_child(bar);
    let dom = Dom::create_body()
        .with_css("margin: 0px; width: 100%; height: 100%; display: flex; flex-direction: column;")
        .with_child(root);

    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    lw.set_system_style(Arc::new(defaults::android_material_light()));
    let px = |v: f32| OptionPixelValue::Some(PixelValue::px(v));
    lw.safe_area_insets = SafeAreaInsets { top: px(48.8), bottom: px(48.0), ..SafeAreaInsets::default() };
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(width, height);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("layout");
    lw
}

/// Every laid-out node of the root document, in window coordinates.
fn every_rect(w: &LayoutWindow) -> Vec<(usize, LogicalRect)> {
    let lr = w.layout_results.get(&DomId::ROOT_ID).expect("root layout");
    let mut out = Vec::new();
    for (dom_node, indices) in lr.layout_tree.dom_to_layout.iter() {
        for idx in indices {
            let hot = lr.layout_tree.get(*idx).expect("layout node");
            if let Some(size) = hot.used_size {
                out.push((
                    dom_node.index(),
                    LogicalRect { origin: lr.calculated_positions[idx.index()], size },
                ));
            }
        }
    }
    out
}

/// The box of the node whose id is `id`.
fn rect_of_id(w: &LayoutWindow, id: &str) -> LogicalRect {
    let lr = w.layout_results.get(&DomId::ROOT_ID).expect("root layout");
    let node = (0..lr.styled_dom.node_data.as_ref().len())
        .map(NodeId::new)
        .find(|n| {
            lr.styled_dom.node_data.as_container()[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_id(), Some(s) if s == id))
        })
        .unwrap_or_else(|| panic!("a node with the id {id}"));
    let idx = *lr.layout_tree.dom_to_layout[&node].first().expect("laid out");
    let size = lr.layout_tree.get(idx).expect("layout node").used_size.expect("sized");
    LogicalRect { origin: lr.calculated_positions[idx.index()], size }
}

#[test]
fn the_compact_width_is_the_one_the_app_uses() {
    assert!(
        MAPS.contains(&format!("const COMPACT_WIDTH: f32 = {COMPACT_WIDTH:.1};")),
        "the app's width under which the screen is a phone's changed: update this test"
    );
}

#[test]
fn on_a_phone_the_banner_and_the_bar_are_the_screen_less_8_px_each_side_and_no_node_crosses_an_edge() {
    for (width, height) in [(320.0, 640.0), (360.0, 800.0), (393.0, 852.0), (411.0, 914.0)] {
        let w = lay_out(width, height);
        for id in ["banner", "bar"] {
            let r = rect_of_id(&w, id);
            assert!(
                (r.origin.x - 8.0).abs() <= 1.0 && (r.size.width - (width - 16.0)).abs() <= 1.0,
                "{width} dp: the {id} runs from {} to {}",
                r.origin.x,
                r.origin.x + r.size.width
            );
        }
        for (node, r) in every_rect(&w) {
            assert!(
                r.origin.x >= -0.5 && r.origin.x + r.size.width <= width + 0.5,
                "{width} dp: node {node} runs from {} to {}, past the screen's edge",
                r.origin.x,
                r.origin.x + r.size.width
            );
        }
    }
}

#[test]
fn the_end_key_of_the_bar_is_on_the_screen_and_a_finger_wide() {
    for (width, height) in [(320.0, 640.0), (360.0, 800.0), (411.0, 914.0)] {
        let w = lay_out(width, height);
        let end = rect_of_id(&w, "end");
        let bar = rect_of_id(&w, "bar");
        assert!(
            end.origin.x >= bar.origin.x - 0.5 && end.origin.x + end.size.width <= bar.origin.x + bar.size.width + 0.5,
            "{width} dp: End runs from {} to {}, the bar from {} to {}",
            end.origin.x,
            end.origin.x + end.size.width,
            bar.origin.x,
            bar.origin.x + bar.size.width
        );
        assert!(end.size.height >= 47.9, "{width} dp: End is {} dp tall", end.size.height);
    }
}

#[test]
fn a_phone_in_landscape_and_a_desktop_window_keep_the_centred_boxes() {
    for (width, height) in [(852.0, 393.0), (1024.0, 720.0)] {
        let w = lay_out(width, height);
        let banner = rect_of_id(&w, "banner");
        let bar = rect_of_id(&w, "bar");
        assert!((banner.size.width - 460.0).abs() <= 1.0, "{width}: the banner is {}", banner.size.width);
        assert!((bar.size.width - 560.0).abs() <= 1.0, "{width}: the bar is {}", bar.size.width);
        for r in [banner, bar] {
            assert!(
                (r.origin.x + r.size.width / 2.0 - width / 2.0).abs() <= 1.0,
                "{width}: centred, the box is at {}",
                r.origin.x
            );
        }
    }
}
