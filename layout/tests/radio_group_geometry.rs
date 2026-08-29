//! The device symptom (AzWidgets, macOS, intermittent): a RadioGroup's
//! indicator renders as a vertically stretched OVAL with the selected dot
//! sitting at the TOP of it — i.e. the circle's fixed 16px height gave way to
//! the row's stretch, and the wrapper's `align-items: center` stopped
//! centering the dot. Both are one defect class: a flex cross-axis property
//! lost on SOME passes (reconcile / incremental relayout), which is why it
//! only happens "sometimes" on device.
//!
//! The test lays the SAME widget out over several passes, flipping the
//! selected index between passes (the widget restyles the dots and the
//! reconcile path runs), and asserts the geometry every time: the circle
//! stays 16x16 and the dot is centered in it on BOTH axes.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, widgets::radio_group::RadioGroup, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

// Border-box: 16px content + 1px border each side.
const CIRCLE_SIZE: f32 = 18.0;
const DOT_SIZE: f32 = 8.0;

fn node_rect(lw: &LayoutWindow, node: NodeId) -> Option<(f32, f32, f32, f32)> {
    let lr = lw.get_layout_result(&DomId::ROOT_ID)?;
    let idx = *lr.layout_tree.dom_to_layout.get(&node)?.first()?;
    let pos = lr.calculated_positions.get(idx.index())?;
    let size = lr.layout_tree.nodes.get(idx.index())?.used_size?;
    Some((pos.x, pos.y, size.width, size.height))
}

fn nodes_with_class(lw: &LayoutWindow, class: &str) -> Vec<NodeId> {
    let Some(lr) = lw.get_layout_result(&DomId::ROOT_ID) else {
        return Vec::new();
    };
    let container = lr.styled_dom.node_data.as_container();
    (0..container.len())
        .map(NodeId::new)
        .filter(|nid| {
            container[*nid].attributes().as_ref().iter().any(|a| {
                a.as_class().is_some_and(|c| {
                    let s: &str = c.as_ref();
                    s == class
                })
            })
        })
        .collect()
}

#[test]
fn the_radio_circle_stays_round_and_its_dot_stays_centered_across_passes() {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = window_state;
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();

    // Pass 1..=4: flip the selected index each pass — the widget swaps the
    // dots' opacity style sets and the reconcile/incremental path runs.
    for pass in 0..4usize {
        let selected = pass % 3;
        let options: Vec<azul_css::AzString> = vec![
            azul_css::AzString::from("Option A"),
            azul_css::AzString::from("Option B"),
            azul_css::AzString::from("Option C"),
        ];
        let mut rg = RadioGroup::create(options.into());
        rg.radio_group_state.inner.selected_index = selected;
        let mut dom = Dom::create_body().with_child(rg.dom());
        let (css, _) = azul_css::parser2::new_from_str("* { margin: 0; padding: 0; }");
        let styled = StyledDom::create(&mut dom, css);
        let ws = lw.current_window_state.clone();
        let mut dbg = None;
        lw.layout_and_generate_display_list(
            styled,
            &ws,
            &renderer_resources,
            &system_callbacks,
            &mut dbg,
        )
        .unwrap();

        let circles = nodes_with_class(&lw, "__azul-native-radio-group-circle");
        let dots = nodes_with_class(&lw, "__azul-native-radio-group-dot");
        assert_eq!(circles.len(), 3, "pass {pass}: three circles");
        assert_eq!(dots.len(), 3, "pass {pass}: three dots");

        for (circle, dot) in circles.iter().zip(dots.iter()) {
            let (cx, cy, cw, ch) = node_rect(&lw, *circle).expect("circle rect");
            let (dx, dy, dw, dh) = node_rect(&lw, *dot).expect("dot rect");
            assert!(
                (cw - CIRCLE_SIZE).abs() < 0.6 && (ch - CIRCLE_SIZE).abs() < 0.6,
                "pass {pass}: the circle must stay {CIRCLE_SIZE}px round, got \
                 {cw}x{ch} — its fixed height lost to the row's cross-axis \
                 stretch (the device OVAL)"
            );
            assert!(
                (dw - DOT_SIZE).abs() < 0.6 && (dh - DOT_SIZE).abs() < 0.6,
                "pass {pass}: dot must stay {DOT_SIZE}px, got {dw}x{dh}"
            );
            let expect_dx = cx + (cw - dw) / 2.0;
            let expect_dy = cy + (ch - dh) / 2.0;
            assert!(
                (dx - expect_dx).abs() < 1.0 && (dy - expect_dy).abs() < 1.0,
                "pass {pass}: dot at ({dx},{dy}) but the circle centre wants \
                 ({expect_dx},{expect_dy}) — align-items:center was lost (the \
                 device dot-at-the-top)"
            );
        }
    }
}
