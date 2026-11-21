//! The input method's geometry questions are answered where the text IS on
//! screen - with the field's own scroll taken into account.
//!
//! The IME protocols ask two things in window coordinates: "where is the
//! text at byte k" (macOS `firstRectForCharacterRange:`, iOS
//! `caretRectForPosition:` / `firstRectForRange:`) and "which byte is under
//! this point" (`characterIndexForPoint:`, `closestPositionToPoint:`, the
//! selection-handle drag). Both answered in STATIC layout space: in a
//! TextInput scrolled by S the candidate window opened S px to the right of
//! the caret, and a point resolved to the character S px to its left. The
//! caret's own viewport rect (`get_focused_cursor_rect_viewport`, the IME's
//! fallback) always applied the scroll - it is the oracle here. See
//! `scripts/SELECTION_WHEN_CLIPPED_ARCHITECTURE_2026_09_26.md` M5.

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

/// How far the value is scrolled.
const SCROLL_X: f32 = 40.0;

/// The byte the questions are about: before the 'm' of "jumps" - on screen
/// at a 40px scroll, and the start of a wide glyph.
const BYTE: usize = 22;

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

    fn press(&mut self, at: LogicalPosition) {
        self.update_hit_test_at(at);
        self.lw.process_mouse_click_for_selection(at, 0);
        self.lw.focus_manager.set_focused_node(Some(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(HOST)),
        }));
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

    /// An editing session in the overflowing value, the value scrolled by
    /// [`SCROLL_X`], the caret on [`BYTE`]; returns where that caret is on
    /// screen (the oracle).
    fn scrolled_with_the_caret_on_the_byte() -> (Self, LogicalRect) {
        let mut h = Self::new(VALUE, 200.0, 120.0);
        let field = h.border_box(VALUE_P);
        h.press(LogicalPosition::new(
            field.origin.x + 20.0,
            field.origin.y + field.size.height / 2.0,
        ));
        h.lw.scroll_manager.set_scroll_position(
            DomId::ROOT_ID,
            VALUE_P,
            LogicalPosition::new(SCROLL_X, 0.0),
            Instant::from(std::time::Instant::now()),
        );
        assert_eq!(
            h.lw.scroll_manager
                .get_current_offset(DomId::ROOT_ID, VALUE_P)
                .map(|o| o.x),
            Some(SCROLL_X),
            "harness: the value is scrolled"
        );
        assert!(
            h.lw.set_focused_selection_from_byte_range(BYTE, BYTE),
            "harness: the press opened an editing session"
        );
        let caret = h
            .lw
            .get_focused_cursor_rect_viewport()
            .expect("harness: the caret has an on-screen rect");
        assert!(
            caret.origin.x > field.origin.x && caret.origin.x < field.origin.x + field.size.width,
            "harness: byte {BYTE} is on screen at a {SCROLL_X}px scroll: caret {caret:?}, field \
             {field:?}"
        );
        (h, caret)
    }
}

/// "Where is byte k?" - the candidate window's anchor.
#[test]
fn the_ime_rect_of_a_byte_is_where_the_caret_on_it_is_painted() {
    let (h, caret) = Harness::scrolled_with_the_caret_on_the_byte();
    let rect = h
        .lw
        .focused_rect_for_byte_offset(BYTE)
        .expect("byte 22 has a rect");
    assert!(
        (rect.origin.x - caret.origin.x).abs() < 0.5 && (rect.origin.y - caret.origin.y).abs() < 0.5,
        "THE BUG: the IME rect for byte {BYTE} is at {:?}, the caret on it is painted at {:?} - \
         off by the field's own scroll ({SCROLL_X}px)",
        rect.origin,
        caret.origin
    );
    let range = h
        .lw
        .focused_rect_for_byte_range(BYTE, BYTE + 3)
        .expect("bytes 22..25 have a rect");
    assert!(
        (range.origin.x - caret.origin.x).abs() < 0.5,
        "the range rect starts where its first byte's caret is painted: {:?} vs {:?}",
        range.origin,
        caret.origin
    );
}

/// "Which byte is under this point?" - a click into a composition, the
/// reconversion popup, the selection-handle drag.
#[test]
fn the_byte_under_a_point_is_the_one_painted_under_it() {
    let (h, caret) = Harness::scrolled_with_the_caret_on_the_byte();
    let point = LogicalPosition::new(caret.origin.x + 1.0, caret.origin.y + caret.size.height / 2.0);
    let byte = h
        .lw
        .focused_byte_offset_for_point(point)
        .expect("a point on the text resolves to a byte");
    assert!(
        byte.abs_diff(BYTE) <= 1,
        "THE BUG: the point 1px right of the caret on byte {BYTE} resolved to byte {byte} - the \
         character about {SCROLL_X}px to its left"
    );
}

/// The selection handles hang under the carets where those are PAINTED -
/// and are grabbed there.
///
/// `selection_handle_geometry` (what a shell that draws its own handles
/// asks, and what `selection_handle_at` hit-tests a window point against)
/// measured the two ends with a STATIC caret rect, documented as window
/// coordinates: in a field scrolled by S the handles were S px right of the
/// selection they mark, and a press on the painted handle missed it (the M5
/// class, left open by the text-scroll-reveal work).
#[test]
fn the_selection_handles_hang_under_the_carets_where_they_are_painted() {
    let (mut h, _) = Harness::scrolled_with_the_caret_on_the_byte();
    // The engine's own handles (Android's), so `selection_handle_at` answers.
    h.lw.text_edit_manager.selection_handles = true;
    assert!(
        h.lw.set_focused_selection_from_byte_range(BYTE, BYTE + 3),
        "harness: a range in the session"
    );
    let start = h
        .lw
        .focused_rect_for_byte_offset(BYTE)
        .expect("the range's start has an on-screen rect");
    let [first, _] = h
        .lw
        .selection_handle_geometry()
        .expect("a range has two handles");
    assert!(
        (first.center.x - start.origin.x).abs() < 0.5,
        "THE BUG: the start handle hangs at x={}, the caret it marks is painted at x={} - off by \
         the field's own scroll ({SCROLL_X}px)",
        first.center.x,
        start.origin.x
    );
    assert!(
        h.lw.selection_handle_at(first.center).is_some(),
        "a press on the handle where it hangs grabs it"
    );
}
