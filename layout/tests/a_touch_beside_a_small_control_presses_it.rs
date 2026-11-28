//! A touch beside a small control presses it (the touch hit-slop, `azul_layout::hit_slop`): on a phone a
//! finger that lands next to a 20 px box that takes a click is for that box, as long as it lands inside the
//! 48 dp the box is grown to, on no interactive box's own pixels, and not under something painted over it.
//!
//! The tester is built from a real layout, as the shell builds it, and asked the way `perform_hit_test` asks.

use std::sync::Arc;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData},
    dom::{Dom, DomId, EventFilter, HoverEventFilter, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::{OptionRefAny, RefAny},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::system::defaults;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, headless::CpuHitTester, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A box that takes a click (the callback is never called: only that the node has one).
fn control(id: &str, left: f32, top: f32) -> Dom {
    Dom::create_div()
        .with_id(id.into())
        .with_css(&format!(
            "position: absolute; left: {left}px; top: {top}px; width: 20px; height: 20px; background: #3366cc;"
        ))
        .with_callbacks(
            vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::Click),
                callback: CoreCallback { cb: 0, ctx: OptionRefAny::None },
                refany: RefAny::new(0_u8),
            }]
            .into(),
        )
}

/// A box with no callback at all.
fn plain(id: &str, left: f32, top: f32, w: f32, h: f32, paint: &str) -> Dom {
    Dom::create_div().with_id(id.into()).with_css(&format!(
        "position: absolute; left: {left}px; top: {top}px; width: {w}px; height: {h}px; {paint}"
    ))
}

struct Scene {
    window: LayoutWindow,
    tester: CpuHitTester,
}

impl Scene {
    fn new(children: Vec<Dom>) -> Self {
        let mut window = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        window.set_system_style(Arc::new(defaults::android_material_light()));
        let mut state = FullWindowState::default();
        state.size.dimensions = LogicalSize::new(411.0, 914.0);
        window.current_window_state = state.clone();
        let mut body = Dom::create_body().with_css("margin: 0; position: relative; width: 411px; height: 914px;");
        for child in children {
            body.add_child(child);
        }
        window
            .layout_and_generate_display_list(
                StyledDom::create_from_dom(body),
                &state,
                &RendererResources::default(),
                &ExternalSystemCallbacks::rust_internal(),
                &mut None,
            )
            .expect("layout");
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout(&window.layout_results);
        Self { window, tester }
    }

    fn node(&self, id: &str) -> NodeId {
        let layout = &self.window.layout_results[&DomId::ROOT_ID];
        (0..layout.styled_dom.node_data.as_ref().len())
            .map(NodeId::new)
            .find(|n| {
                layout.styled_dom.node_data.as_container()[*n]
                    .get_ids_and_classes()
                    .iter()
                    .any(|c| matches!(c.as_id(), Some(s) if s == id))
            })
            .unwrap_or_else(|| panic!("a node with the id {id}"))
    }

    /// What the touch at (x, y) is for, asked the way `perform_hit_test` asks (the floor of Android).
    fn touch(&self, x: f32, y: f32, occluders: &[LogicalRect]) -> Option<(NodeId, LogicalPosition)> {
        self.tester
            .touch_slop_hit(LogicalPosition::new(x, y), 48.0, occluders, &|_, _| None, &|_, _| None, None)
            .map(|(dom, node, local)| {
                assert_eq!(dom, DomId::ROOT_ID);
                (node, local)
            })
    }
}

fn scene() -> Scene {
    Scene::new(vec![
        // Two controls 4 px apart, a plain box, a control with a toast painted over its left side.
        control("a", 100.0, 100.0),
        control("b", 124.0, 100.0),
        plain("plain", 100.0, 300.0, 20.0, 20.0, "background: #888888;"),
        control("c", 100.0, 500.0),
        plain("toast", 70.0, 480.0, 60.0, 60.0, "background: #cc3333;"),
    ])
}

#[test]
fn a_touch_beside_a_small_control_is_for_it() {
    let s = scene();
    // 6 px left of a 20 px control: inside the 14 px the 48 dp floor adds on each side.
    let (node, _) = s.touch(94.0, 110.0, &[]).expect("a is meant");
    assert_eq!(node, s.node("a"));
    // 12 px above it.
    let (node, _) = s.touch(110.0, 88.0, &[]).expect("a is meant");
    assert_eq!(node, s.node("a"));
}

#[test]
fn a_touch_beyond_the_floors_reach_is_for_nobody() {
    let s = scene();
    assert!(s.touch(60.0, 110.0, &[]).is_none(), "40 px away");
    assert!(s.touch(110.0, 70.0, &[]).is_none(), "30 px above");
}

#[test]
fn a_touch_on_a_controls_own_pixels_is_the_normal_hit_tests() {
    let s = scene();
    assert!(s.touch(110.0, 110.0, &[]).is_none(), "on a");
    // On b's pixels, which lie in a's reach too: b's, so not the slop's.
    assert!(s.touch(126.0, 110.0, &[]).is_none(), "on b");
}

#[test]
fn between_two_controls_the_nearer_centre_wins() {
    let s = scene();
    // The gap is x 120 to 124; a's centre is at 110, b's at 134.
    assert_eq!(s.touch(121.0, 110.0, &[]).map(|(n, _)| n), Some(s.node("a")));
    assert_eq!(s.touch(123.0, 110.0, &[]).map(|(n, _)| n), Some(s.node("b")));
}

#[test]
fn a_box_without_a_callback_gets_no_slop() {
    let s = scene();
    assert!(s.touch(94.0, 310.0, &[]).is_none());
}

#[test]
fn something_painted_over_a_control_covers_its_slop() {
    let s = scene();
    // The toast lies over x 70 to 130 and y 480 to 540, painted after c.
    assert!(s.touch(94.0, 510.0, &[]).is_none(), "under the toast");
    // Right of the toast, inside c's reach (c ends at 120, its grown rect at 134).
    assert_eq!(s.touch(132.0, 510.0, &[]).map(|(n, _)| n), Some(s.node("c")));
}

#[test]
fn the_answer_is_the_control_and_the_touch_clamped_into_it() {
    let s = scene();
    let (node, local) = s.touch(94.0, 88.0, &[]).expect("a is meant");
    assert_eq!(node, s.node("a"));
    // The nearest point of a's own pixels (100..120, 100..120): what a tap on it would have been.
    assert_eq!((local.x, local.y), (100.0, 100.0));
}

#[test]
fn an_open_overlay_over_the_touch_blocks_the_slop() {
    let s = scene();
    let popup = LogicalRect::new(LogicalPosition::new(80.0, 90.0), LogicalSize::new(40.0, 40.0));
    assert!(s.touch(94.0, 110.0, &[popup]).is_none());
    // The same touch with the overlay elsewhere is the control's.
    let elsewhere = LogicalRect::new(LogicalPosition::new(300.0, 300.0), LogicalSize::new(40.0, 40.0));
    assert!(s.touch(94.0, 110.0, &[elsewhere]).is_some());
}

#[test]
fn a_window_without_an_interactive_box_has_no_slop() {
    let s = Scene::new(vec![plain("p", 100.0, 100.0, 20.0, 20.0, "background: #888888;")]);
    assert!(s.touch(94.0, 110.0, &[]).is_none());
}
