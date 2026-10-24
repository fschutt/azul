//! A selection reveal shows the end the user is extending - its FOCUS - not
//! the start of a range that is wider than the field.
//!
//! `scroll_selection_into_view(Selection, ..)` revealed the selection's
//! bounding rect, and `calculate_instant_scroll_delta` tests the left edge
//! before the right (the top before the bottom). A range wider than its
//! scrollport therefore always scrolled to its START: Shift+Right past the
//! right edge of a TextInput, or Shift+Down past the bottom of a TextArea,
//! took the view away from the end being extended. Browsers reveal the
//! selection's focus. See
//! `scripts/TEXT_SCROLL_VS_CARET_REVEAL_ARCHITECTURE_2026_09_26.md` (M3, §8
//! step 3).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::{RendererResources, SystemAnimations},
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    headless::CpuHitTester,
    window::{LayoutWindow, ScrollMode, SelectionScrollType},
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `body(0) > host div(1) > value p(2) > text(3)`: the widget's shape.
const HOST: NodeId = NodeId::new(1);
const VALUE_P: NodeId = NodeId::new(2);

/// 200 characters: far wider than a 200px field.
fn value() -> String {
    "the quick brown fox ".repeat(10)
}

struct Harness {
    lw: LayoutWindow,
    hit_tester: CpuHitTester,
}

impl Harness {
    /// The REAL TextInput widget alone in a `width` x `height` window.
    fn new(value: &str, width: f32, height: f32) -> Self {
        let widget = azul_layout::widgets::text_input::TextInput::create()
            .with_text(value.into())
            .dom();
        let styled_dom = StyledDom::create_from_dom(Dom::create_body().with_child(widget));

        let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
        // The instant reveal: a long one would otherwise queue a glide for
        // the physics timer, which only the shells run.
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

    /// Scroll registration, then the CPU hit-tester rebuild.
    fn finalize(&mut self) {
        let now = Instant::from(std::time::Instant::now());
        azul_layout::managers::scroll_registration::register_scroll_nodes(&mut self.lw, &now);
        self.hit_tester.rebuild_from_layout_with_gpu(
            &self.lw.layout_results,
            Some(&self.lw.gpu_state_manager),
        );
    }

    /// Port of `PlatformWindow::update_hit_test_at`.
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

    /// A press on the text: an editing session on the value, focus on the
    /// host.
    fn press(&mut self, at: LogicalPosition) {
        self.update_hit_test_at(at);
        self.lw.process_mouse_click_for_selection(at, 0);
        self.lw.focus_manager.set_focused_node(Some(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(HOST)),
        }));
    }

    /// The border box of `node` (static space; nothing above the field
    /// scrolls, so this is also where it is on screen).
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

/// A range from byte 0 to byte 150 of a 200-character value, extended
/// forwards (the focus is at 150): the reveal must put byte 150 on screen.
#[test]
fn a_range_wider_than_the_field_reveals_its_focus_end() {
    let text = value();
    let mut h = Harness::new(&text, 200.0, 120.0);
    let field = h.border_box(VALUE_P);

    h.press(LogicalPosition::new(
        field.origin.x + 20.0,
        field.origin.y + field.size.height / 2.0,
    ));
    assert!(
        h.lw.set_focused_selection_from_byte_range(0, 150),
        "harness: the press opened an editing session to select in"
    );
    h.finalize();

    let _ = h
        .lw
        .scroll_selection_into_view(SelectionScrollType::Selection, ScrollMode::Instant);

    // The primary cursor of a range is its END - here the focus at byte 150.
    let focus = h
        .lw
        .get_focused_cursor_rect_viewport()
        .expect("the selection has a focus caret");
    let field_right = field.origin.x + field.size.width;
    assert!(
        focus.origin.x >= field.origin.x && focus.origin.x + focus.size.width <= field_right + 0.5,
        "THE BUG: after revealing the selection 0..150 the focus end is at x {}..{}, outside \
         the field {}..{field_right} - the reveal went to the range's START (value offset {})",
        focus.origin.x,
        focus.origin.x + focus.size.width,
        field.origin.x,
        h.value_offset_x()
    );
}

/// The control: a range that FITS the field is revealed whole - its start
/// stays on screen too - so the fix cannot simply reveal the focus caret
/// and drop the rest.
#[test]
fn a_range_that_fits_the_field_is_revealed_whole() {
    let text = value();
    let mut h = Harness::new(&text, 200.0, 120.0);
    let field = h.border_box(VALUE_P);

    h.press(LogicalPosition::new(
        field.origin.x + 20.0,
        field.origin.y + field.size.height / 2.0,
    ));
    // Bytes 160..170: ten characters, far past the right edge, narrower
    // than the field.
    assert!(h.lw.set_focused_selection_from_byte_range(160, 170));
    h.finalize();

    let _ = h
        .lw
        .scroll_selection_into_view(SelectionScrollType::Selection, ScrollMode::Instant);

    let focus = h
        .lw
        .get_focused_cursor_rect_viewport()
        .expect("the selection has a focus caret");
    // Where byte 160 is on screen: collapse onto it (moving a caret scrolls
    // nothing by itself) and read the caret back.
    assert!(h.lw.set_focused_selection_from_byte_range(160, 160));
    let start_on_screen = h
        .lw
        .get_focused_cursor_rect_viewport()
        .expect("byte 160 has a caret")
        .origin
        .x;
    let field_right = field.origin.x + field.size.width;
    assert!(
        start_on_screen >= field.origin.x - 0.5 && focus.origin.x <= field_right + 0.5,
        "a range narrower than the field is shown whole: start at x {start_on_screen}, focus at \
         x {}, field {}..{field_right}",
        focus.origin.x,
        field.origin.x
    );
}
