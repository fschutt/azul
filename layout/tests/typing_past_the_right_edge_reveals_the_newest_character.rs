//! Typing past the right edge of a single-line field shows the character
//! just typed, in the same pass that typed it.
//!
//! Device bug (AzWidgets, macOS, 2026-09-28): "the reveal is ONE KEYSTROKE
//! LATE". Typing at the end of an overflowing TextInput clipped the newest
//! character at the right edge; it came into view only with the next key.
//!
//! The shells reveal the caret right after the edit lands
//! (`ApplyTextChangeset` → `ScrollCursorIntoViewAfterTextInput`, and the
//! `CreateTextInput` arm the macOS IME path takes), BEFORE any relayout. The
//! edit's re-shape (`reshape_text_node`) publishes the new content width to
//! the layout tree, but the `ScrollManager` - whose `content_rect` the reveal
//! is clamped against - was only refreshed when the box's scrollbar necessity
//! FLIPPED. A field that already overflowed kept the extent of the previous
//! keystroke, so the reveal stopped exactly one keystroke short.
//!
//! The harness drives the shell's order: the edit, then the reveal, with no
//! layout in between.

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

/// Far wider than a 200px window.
const VALUE: &str = "The quick brown fox jumps over the lazy dog and keeps on running";

/// `body(0) > host div(1) > value p(2) > text(3)`: the widget's shape
/// (`themes::flat::text_input`). The `<p>` is the IFC root and the horizontal
/// scroll box.
const HOST: NodeId = NodeId::new(1);
const VALUE_P: NodeId = NodeId::new(2);

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
        // The instant reveal, so the offset moves synchronously: with
        // animations on a long reveal queues a glide for the physics timer,
        // which only the shells run.
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

    /// The shells' finalize tail after a layout: scroll registration, then
    /// the CPU hit-tester rebuild.
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

    /// A press on the text: the text-selection click and the click-to-focus
    /// of the event pass - an editing session on the value, focus on the
    /// host.
    fn press(&mut self, at: LogicalPosition) {
        self.update_hit_test_at(at);
        self.lw.process_mouse_click_for_selection(at, 0);
        self.lw.focus_manager.set_focused_node(Some(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(HOST)),
        }));
    }

    /// The border box of `node` as laid out (static space; nothing above the
    /// field scrolls, so this is also where it is on screen).
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

    /// Where the caret is ON SCREEN (its own box's scroll taken off).
    fn caret_on_screen(&self) -> LogicalRect {
        self.lw
            .get_focused_cursor_rect_viewport()
            .expect("the editing session has a caret")
    }

    /// What the shells do for a keystroke: record the text, land it, then
    /// reveal the caret - with no layout in between.
    fn type_like_the_shell(&mut self, text: &str) {
        let affected = self.lw.record_text_input(text);
        assert!(
            !affected.is_empty(),
            "harness: '{text}' must be recorded against the focused host"
        );
        let landed = self.lw.apply_text_changeset();
        assert!(
            !landed.dirty_nodes.is_empty(),
            "harness: '{text}' must land in the value"
        );
        let _ = self
            .lw
            .scroll_selection_into_view(SelectionScrollType::Cursor, ScrollMode::Instant);
    }
}

/// THE DEVICE BUG. The caret sits at the end of an overflowing value that is
/// already scrolled to its end; four wide characters typed there must all be
/// on screen after the keystroke's own reveal.
#[test]
fn a_character_typed_past_the_right_edge_is_on_screen_after_its_own_reveal() {
    let mut h = Harness::new(VALUE, 200.0, 120.0);
    let field = h.border_box(VALUE_P);

    h.press(LogicalPosition::new(
        field.origin.x + 20.0,
        field.origin.y + field.size.height / 2.0,
    ));
    assert!(
        h.lw.set_focused_selection_from_byte_range(VALUE.len(), VALUE.len()),
        "harness: the press opened an editing session to put the caret in"
    );
    // The frame after the caret moved: registration knows the caret host now.
    h.finalize();
    let _ = h
        .lw
        .scroll_selection_into_view(SelectionScrollType::Cursor, ScrollMode::Instant);
    let before = h.caret_on_screen();
    assert!(
        h.value_offset_x() > 20.0,
        "harness: the value must be scrolled to its end, offset {}",
        h.value_offset_x()
    );
    assert!(
        before.origin.x + before.size.width <= field.origin.x + field.size.width + 0.5,
        "harness: the caret at the end is on screen before typing: caret {before:?}, field \
         {field:?}"
    );

    h.type_like_the_shell("WWWW");

    let after = h.caret_on_screen();
    let field_right = field.origin.x + field.size.width;
    assert!(
        after.origin.x + after.size.width <= field_right + 0.5,
        "THE BUG: the caret behind the four characters just typed is at x {}..{}, past the \
         field's right edge {field_right} - the reveal stopped at the extent of the PREVIOUS \
         keystroke (value offset {})",
        after.origin.x,
        after.origin.x + after.size.width,
        h.value_offset_x()
    );
    assert!(
        after.origin.x >= field.origin.x,
        "the reveal must not overshoot the caret off the left edge: caret {after:?}, field \
         {field:?}"
    );
}

/// The control: a keystroke whose caret is already on screen does not move
/// the view. Without it the test above would pass for a reveal that simply
/// scrolls to the end of the content every time.
#[test]
fn a_character_typed_in_the_middle_of_a_visible_value_leaves_the_view_alone() {
    let mut h = Harness::new(VALUE, 200.0, 120.0);
    let field = h.border_box(VALUE_P);

    h.press(LogicalPosition::new(
        field.origin.x + 20.0,
        field.origin.y + field.size.height / 2.0,
    ));
    h.finalize();
    let offset_before = h.value_offset_x();

    h.type_like_the_shell("i");

    assert_eq!(
        h.value_offset_x(),
        offset_before,
        "a caret 20px inside the field needs no reveal"
    );
}
