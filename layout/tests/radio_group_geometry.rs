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
    dom::{Dom, DomId, NodeId, TabIndex},
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
                    let s: &str = c;
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
                "pass {pass}: the circle must stay {CIRCLE_SIZE}px round, got {cw}x{ch} — its \
                 fixed height lost to the row's cross-axis stretch (the device OVAL)"
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
                 ({expect_dx},{expect_dy}) — align-items:center was lost (the device \
                 dot-at-the-top)"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The two ways the device got its pills (2026-09-28: indicators TALLER than
// wide, the dot off-centre)
// ---------------------------------------------------------------------------

/// Lays `dom` out in `lw` - reconciled against whatever `lw` laid out
/// before, exactly like a `RefreshDom` pass.
fn lay_out(lw: &mut LayoutWindow, mut dom: Dom) {
    let (css, _) = azul_css::parser2::new_from_str("");
    let styled = StyledDom::create(&mut dom, css);
    let ws = lw.current_window_state.clone();
    let mut dbg = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .unwrap();
}

/// Re-runs layout over the window's OWN styled DOM - the incremental
/// relayout every animation frame and every restyle runs.
fn relayout_in_place(lw: &mut LayoutWindow) {
    let result = lw
        .layout_results
        .remove(&DomId::ROOT_ID)
        .expect("harness: laid out");
    let ws = lw.current_window_state.clone();
    let mut dbg = None;
    lw.layout_and_generate_display_list(
        result.styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .unwrap();
}

fn window(width: f32, height: f32) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(width, height);
    lw.current_window_state = window_state;
    lw
}

fn group(selected: usize) -> Dom {
    let options: Vec<azul_css::AzString> = vec![
        azul_css::AzString::from("Option A"),
        azul_css::AzString::from("Option B"),
        azul_css::AzString::from("Option C"),
    ];
    let mut rg = RadioGroup::create(options.into());
    rg.radio_group_state.inner.selected_index = selected;
    rg.dom()
}

/// The circles' rects, in document order.
fn circle_rects(lw: &LayoutWindow) -> Vec<(f32, f32, f32, f32)> {
    nodes_with_class(lw, "__azul-native-radio-group-circle")
        .into_iter()
        .map(|c| node_rect(lw, c).expect("circle rect"))
        .collect()
}

/// Every circle is `CIRCLE_SIZE` round with its dot in the middle.
fn assert_round(lw: &LayoutWindow, when: &str) {
    let circles = nodes_with_class(lw, "__azul-native-radio-group-circle");
    let dots = nodes_with_class(lw, "__azul-native-radio-group-dot");
    assert_eq!(circles.len(), 3, "{when}: three circles");
    assert_eq!(dots.len(), 3, "{when}: three dots");
    for (circle, dot) in circles.iter().zip(dots.iter()) {
        let (cx, cy, cw, ch) = node_rect(lw, *circle).expect("circle rect");
        let (dx, dy, dw, dh) = node_rect(lw, *dot).expect("dot rect");
        assert!(
            (cw - CIRCLE_SIZE).abs() < 0.6 && (ch - CIRCLE_SIZE).abs() < 0.6,
            "{when}: the circle must stay {CIRCLE_SIZE}px round, got {cw}x{ch} (the device pill)"
        );
        assert!(
            (dw - DOT_SIZE).abs() < 0.6 && (dh - DOT_SIZE).abs() < 0.6,
            "{when}: the dot must stay {DOT_SIZE}px, got {dw}x{dh}"
        );
        assert!(
            (dx - (cx + (cw - dw) / 2.0)).abs() < 1.0 && (dy - (cy + (ch - dh) / 2.0)).abs() < 1.0,
            "{when}: the dot at ({dx},{dy}) is off the centre of the circle at ({cx},{cy}) \
             {cw}x{ch}"
        );
    }
}

/// A row narrower than its label: the label wraps or overflows, the
/// indicator keeps its size. A flex item shrinks by default, down to its
/// content's minimum - for the circle that is its 8px dot plus the 1px
/// borders - so the 18x18 ring became a 10x18 pill. A native radio button's
/// indicator never gives way to its text.
#[test]
fn the_radio_circle_stays_round_in_a_row_too_narrow_for_its_label() {
    let mut lw = window(400.0, 300.0);
    lay_out(
        &mut lw,
        Dom::create_body()
            .with_css("margin: 0;")
            .with_child(Dom::create_div().with_css("width: 40px;").with_child(group(0))),
    );
    assert_round(&lw, "a 40px wide group");
}

/// The widgets demo's shape: the group sits, `align-self: start`, in a
/// labelled flex column inside a padded card inside the scrolling page
/// column of a full-height flex body.
fn demo_page(selected: usize) -> Dom {
    let labelled = Dom::create_div()
        .with_css("display: flex; flex-direction: column; margin-bottom: 16px;")
        .with_child(
            Dom::create_span_with_text("RadioGroup")
                .with_css("font-size: 12px; font-weight: bold; margin-bottom: 6px;"),
        )
        .with_child(group(selected));
    let card = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; border-radius: 10px; padding: 18px; \
             margin-bottom: 20px;",
        )
        .with_child(labelled);
    let page = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; overflow-y: auto; flex-grow: 1; \
             min-height: 0; padding: 24px;",
        )
        .with_child(card);
    Dom::create_body()
        .with_css("margin: 0; display: flex; flex-direction: column; height: 100%;")
        .with_child(page)
}

/// What the widget's click and arrow handlers write into the LIVE DOM on
/// top of the rebuild: the group's one Tab stop moves to row `stop`
/// (`roving::set_stop` through `CallbackInfo::set_tab_index`, which the
/// shell applies in place).
fn move_the_tab_stop_in_place(lw: &mut LayoutWindow, stop: usize) {
    let rows = nodes_with_class(lw, "__azul-native-radio-group-row");
    let result = lw
        .layout_results
        .get_mut(&DomId::ROOT_ID)
        .expect("harness: laid out");
    let mut node_data = result.styled_dom.node_data.as_container_mut();
    for (i, row) in rows.iter().enumerate() {
        node_data[*row].set_tab_index(if i == stop {
            TabIndex::Auto
        } else {
            TabIndex::NoKeyboardFocus
        });
    }
}

/// Clicking (or arrowing) through the group in the demo's page: every click
/// rebuilds the DOM with the check - and the roving Tab stop - on another
/// row, the handler moves the stop in the live DOM as well, and the next
/// frame relays that DOM out in place. The indicators keep their size and
/// their place through all of it ("lay out twice in one window and diff the
/// node rects").
#[test]
fn the_radio_circles_keep_their_rects_while_clicks_move_the_check_in_the_demo_page() {
    let mut lw = window(420.0, 640.0);
    lay_out(&mut lw, demo_page(0));
    assert_round(&lw, "the first layout");
    let first = circle_rects(&lw);

    for (pass, selected) in [1usize, 2, 0, 1, 2].into_iter().enumerate() {
        lay_out(&mut lw, demo_page(selected));
        assert_round(&lw, &format!("pass {pass}: the rebuild checking {selected}"));
        move_the_tab_stop_in_place(&mut lw, selected);
        relayout_in_place(&mut lw);
        assert_round(&lw, &format!("pass {pass}: the in-place relayout after it"));
        let now = circle_rects(&lw);
        assert_eq!(now.len(), first.len(), "pass {pass}: three circles");
        for (i, (a, b)) in first.iter().zip(now.iter()).enumerate() {
            assert!(
                (a.0 - b.0).abs() < 0.6
                    && (a.1 - b.1).abs() < 0.6
                    && (a.2 - b.2).abs() < 0.6
                    && (a.3 - b.3).abs() < 0.6,
                "pass {pass}: circle {i} moved or resized from {a:?} to {b:?}"
            );
        }
    }
}
