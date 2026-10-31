//! While a selection drag is in progress, the drag - and the autoscroll it
//! drives - is the ONE writer of its field's scroll offset.
//!
//! A pointer point is resolved to a glyph against a scroll offset; if a
//! reveal moves that offset later in the same pass, the painted selection end
//! is no longer under the pointer, and near an edge the next event resolves a
//! glyph further along and the reveal scrolls again: a "micro-autoscroll"
//! whose speed depends on the mouse event rate, stacked on the real
//! autoscroll. See `scripts/TEXT_SCROLL_VS_CARET_REVEAL_ARCHITECTURE_2026_09_26.md`
//! §6 and §8 step 5.
//!
//! The harness drives the layout window without a shell pass tail, so a
//! reveal the press asked for is still pending when the drag begins - the
//! state any reveal request is in between the input that issued it and the
//! layout that performs it.

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

/// `body(0) > host div(1) > value p(2) > text(3)`: the widget's shape.
const HOST: NodeId = NodeId::new(1);
const VALUE_P: NodeId = NodeId::new(2);

struct Harness {
    lw: LayoutWindow,
    hit_tester: CpuHitTester,
}

impl Harness {
    fn new(value: &str, width: f32, height: f32) -> Self {
        let widget = azul_layout::widgets::text_input::TextInput::create()
            .with_text(value.into())
            .dom();
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
        h.finalize();
        h
    }

    fn finalize(&mut self) {
        let now = Instant::from(std::time::Instant::now());
        azul_layout::managers::scroll_registration::register_scroll_nodes(&mut self.lw, &now);
        self.hit_tester.rebuild_from_layout_with_gpu(
            &self.lw.layout_results,
            Some(&self.lw.gpu_state_manager),
        );
    }

    /// A layout pass over the SAME StyledDom (a hover restyle, an animation
    /// frame, the incremental relayout a keystroke asks for), then the
    /// finalize tail.
    fn relayout(&mut self) {
        let window_state = self.lw.current_window_state.clone();
        let styled_dom = self
            .lw
            .layout_results
            .remove(&DomId::ROOT_ID)
            .expect("root layout result")
            .styled_dom;
        let mut dbg = Some(Vec::new());
        self.lw
            .layout_and_generate_display_list(
                styled_dom,
                &window_state,
                &RendererResources::default(),
                &ExternalSystemCallbacks::rust_internal(),
                &mut dbg,
            )
            .unwrap();
        self.finalize();
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

    fn press(&mut self, at: LogicalPosition) {
        self.update_hit_test_at(at);
        self.lw.process_mouse_click_for_selection(at, 0);
        self.lw.focus_manager.set_focused_node(Some(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(HOST)),
        }));
        self.lw.text_selection_drag_anchor = Some(at);
    }

    fn drag(&mut self, from: LogicalPosition, to: LogicalPosition) {
        self.update_hit_test_at(to);
        self.lw.process_mouse_drag_for_selection(from, to);
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

    fn value_offset_x(&self) -> f32 {
        self.lw
            .scroll_manager
            .get_current_offset(DomId::ROOT_ID, VALUE_P)
            .map_or(0.0, |o| o.x)
    }
}

/// Press 1px inside the right edge of a field scrolled to the middle, drag
/// on, and let a layout pass run: the text must not move under the pointer.
#[test]
fn a_layout_during_a_drag_selection_does_not_scroll_the_text_under_the_pointer() {
    let mut h = Harness::new(VALUE, 200.0, 120.0);
    let field = h.border_box(VALUE_P);
    h.lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        VALUE_P,
        LogicalPosition::new(40.0, 0.0),
        Instant::from(std::time::Instant::now()),
    );
    assert_eq!(
        h.value_offset_x(),
        40.0,
        "harness: the overflowing value is scrolled to the middle"
    );

    let y = field.origin.y + field.size.height / 2.0;
    let press = LogicalPosition::new(field.origin.x + field.size.width - 1.0, y);
    h.press(press);
    let offset_at_press = h.value_offset_x();

    let mut at = press;
    for step in 0..3 {
        let next = LogicalPosition::new(at.x + 1.0, y);
        h.drag(at, next);
        at = next;
        assert_eq!(
            h.lw.scroll_manager.pending_reveal(),
            None,
            "step {step}: the drag owns the view - a reveal still pending from before it (here \
             the press's) must not be performed against the drag"
        );
        h.relayout();
        assert_eq!(
            h.value_offset_x(),
            offset_at_press,
            "THE BUG, step {step}: a layout during the drag moved the field from x={offset_at_press} \
             to x={} - a caret reveal, not the drag, scrolled the text under the pointer",
            h.value_offset_x()
        );
    }
}
