//! A selection drag held past the edge of a text field autoscrolls THE
//! FIELD - the box the dragged text scrolls in - and not the page.
//!
//! `auto_scroll_timer_callback` anchors on the focused node, which in a
//! TextInput is the contenteditable HOST, and walked the host and its DOM
//! ANCESTORS for a scroll box. The value `<p>` that actually scrolls is a
//! CHILD of the host, so it was never found: the drag scrolled the page, or
//! nothing. The caret reveal anchors on the editing session's block (the
//! `<p>`), so the two answered "which box does a text gesture scroll" with
//! two different rules. See
//! `scripts/SELECTION_WHEN_CLIPPED_ARCHITECTURE_2026_09_26.md` M3.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::{RendererResources, SystemAnimations},
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, headless::CpuHitTester, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const VALUE: &str = "The quick brown fox jumps over the lazy dog and keeps on running";

/// Both widgets: `body(0) > host(1) > value p(2) > text(3)`.
const HOST: NodeId = NodeId::new(1);
const VALUE_P: NodeId = NodeId::new(2);

fn dom_node(node: NodeId) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    }
}

struct Harness {
    lw: LayoutWindow,
    hit_tester: CpuHitTester,
}

impl Harness {
    fn new(widget: Dom, width: f32, height: f32) -> Self {
        let styled_dom = StyledDom::create_from_dom(Dom::create_body().with_child(widget));
        let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
        lw.system_animations_override = Some(SystemAnimations::disabled());
        let mut window_state = FullWindowState::default();
        window_state.size.dimensions = LogicalSize::new(width, height);
        lw.current_window_state = window_state.clone();
        let mut dbg = Some(Vec::new());
        lw.layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut dbg,
        )
        .unwrap();
        lw.current_window_state = window_state;

        let mut h = Self {
            lw,
            hit_tester: CpuHitTester::new(),
        };
        let now = Instant::from(std::time::Instant::now());
        azul_layout::managers::scroll_registration::register_scroll_nodes(&mut h.lw, &now);
        h.hit_tester
            .rebuild_from_layout_with_gpu(&h.lw.layout_results, Some(&h.lw.gpu_state_manager));
        h
    }

    fn update_hit_test_at(&mut self, position: LogicalPosition) {
        use azul_layout::managers::hover::InputPointId;
        let focused_node = self.lw.focus_manager.get_focused_node().copied();
        let hit_test = {
            let scroll_manager = &self.lw.scroll_manager;
            let gpu = &self.lw.gpu_state_manager;
            let resolve = |d: DomId, n: NodeId| -> Option<LogicalPosition> {
                scroll_manager.get_current_offset(d, n)
            };
            let resolve_tf = |d: DomId, n: NodeId| {
                gpu.caches
                    .get(&d)
                    .and_then(|c| c.css_current_transform_values.get(&n))
                    .copied()
            };
            let hits = self
                .hit_tester
                .hit_test_scrolled(position, &resolve, &resolve_tf);
            azul_layout::headless::convert_cpu_hit_test_to_full(
                &self.hit_tester,
                &hits,
                focused_node,
                &self.lw.layout_results,
                position,
                &resolve,
                &resolve_tf,
            )
        };
        self.lw
            .hover_manager
            .push_hit_test(InputPointId::Mouse, hit_test);
    }

    /// The press that starts a selection drag: the text-selection click,
    /// click-to-focus on the host, and the drag anchor the shell latches.
    fn press(&mut self, at: LogicalPosition) {
        self.update_hit_test_at(at);
        self.lw.process_mouse_click_for_selection(at, 0);
        self.lw.focus_manager.set_focused_node(Some(dom_node(HOST)));
        self.lw.text_selection_drag_anchor = Some(at);
    }

    fn border_box(&self, node: NodeId) -> LogicalRect {
        let lr = self
            .lw
            .layout_results
            .get(&DomId::ROOT_ID)
            .expect("root layout result");
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
}

/// THE BUG (M3): a selection drag in an overflowing TextInput autoscrolls
/// its value `<p>`, anchored as the timer anchors - on the focused HOST.
#[test]
fn a_selection_drag_in_a_text_input_autoscrolls_its_value_line() {
    let widget = azul_layout::widgets::text_input::TextInput::create()
        .with_text(VALUE.into())
        .dom();
    let mut h = Harness::new(widget, 200.0, 120.0);
    let field = h.border_box(VALUE_P);
    assert!(
        h.lw.scroll_manager
            .get_scroll_state(DomId::ROOT_ID, VALUE_P)
            .is_some(),
        "harness: the overflowing value <p> is a registered scroll box"
    );

    h.press(LogicalPosition::new(
        field.origin.x + 20.0,
        field.origin.y + field.size.height / 2.0,
    ));

    let anchor = h
        .lw
        .focus_manager
        .get_focused_node()
        .copied()
        .expect("harness: the host is focused");
    assert_eq!(
        h.lw.drag_autoscroll_box(anchor),
        Some(dom_node(VALUE_P)),
        "THE BUG: a selection drag anchored on the focused host found the host's DOM ancestors' \
         scroll box (or none) - never the value <p> the dragged text scrolls in"
    );
}

/// The control: a TextArea's host IS its scroll box (its container carries
/// `overflow-y: auto`), which both the old rule and the new one find.
#[test]
fn a_selection_drag_in_a_text_area_autoscrolls_the_text_area() {
    let widget = azul_layout::widgets::text_area::TextArea::create()
        .with_text("line\n".repeat(20).as_str().into())
        .dom();
    let mut h = Harness::new(widget, 400.0, 300.0);
    assert!(
        h.lw.scroll_manager
            .get_scroll_state(DomId::ROOT_ID, HOST)
            .is_some(),
        "harness: twenty lines overflow the 64px TextArea, whose container scrolls"
    );
    let area = h.border_box(HOST);
    h.press(LogicalPosition::new(
        area.origin.x + 20.0,
        area.origin.y + 10.0,
    ));

    assert_eq!(
        h.lw.drag_autoscroll_box(dom_node(HOST)),
        Some(dom_node(HOST)),
        "the TextArea's container is the box its text scrolls in"
    );
}
