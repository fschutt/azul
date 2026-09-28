//! A press and drag on the text of an overflowing TextInput selects that text.
//!
//! Device bug (AzWidgets, macOS): once a single-line field's value was wider
//! than the field, a press + drag on the text selected nothing and the text
//! "moved a bit". The press never reached the text. Every shell asks the
//! scroll manager's scrollbars BEFORE the event pass
//! (`macos/events.rs` `handle_mouse_down` → `perform_scrollbar_hit_test`, and
//! the same on Windows, X11, Wayland and the headless backend), and the
//! manager had built a 16px horizontal bar for the value `<p>` - a bar its own
//! style (`scrollbar-width: none`) says does not exist, and which covered the
//! whole 13px line. The press became a thumb drag, the drag scrolled the `<p>`
//! instead of selecting, and that was the "moves a bit".
//!
//! `textinput_resize_selection.rs` drives the same widget and is green: its
//! `click` goes straight to the text and never meets the gate. The harness
//! here takes the gate first, the way the shells do ([`scrollbar_under_press`]),
//! so the suite sees what the device sees. See
//! `scripts/SELECTION_WHEN_CLIPPED_ARCHITECTURE_2026_09_26.md` (M1, M1b, M2).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId, ScrollbarOrientation},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    selection::Selection,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    headless::CpuHitTester,
    managers::scroll_state::{ScrollbarComponent, ScrollbarHit},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// Far wider than a 200px window, far narrower than a 900px one.
const VALUE: &str = "The quick brown fox jumps over the lazy dog and keeps on running";

/// `body(0) > host div(1) > value p(2) > text(3)`: the widget's shape
/// (`themes::flat::text_input`). The `<p>` is the IFC root and the horizontal
/// scroll box (`overflow-x: auto; overflow-y: hidden; scrollbar-width: none`).
const HOST: NodeId = NodeId::new(1);
const VALUE_P: NodeId = NodeId::new(2);

/// THE SHELLS' PRESS GATE, and nothing else. Every backend asks the scroll
/// manager's bars before the event pass, and a press that lands on one never
/// reaches the text. Kept to this one function so it can become
/// `LayoutWindow::route_press` once there is one.
fn scrollbar_under_press(lw: &LayoutWindow, at: LogicalPosition) -> Option<ScrollbarHit> {
    lw.scroll_manager.hit_test_scrollbars(at)
}

/// A thumb the press grabbed: what `ScrollbarDragState` holds in the shells.
#[derive(Debug, Clone, Copy)]
struct ThumbDrag {
    hit: ScrollbarHit,
    initial_offset: LogicalPosition,
}

struct Harness {
    lw: LayoutWindow,
    hit_tester: CpuHitTester,
    renderer_resources: RendererResources,
    system_callbacks: ExternalSystemCallbacks,
    /// Where the last press went, for the failure messages.
    last_press: Option<ScrollbarHit>,
    /// Set while a press on a thumb is being dragged.
    thumb_drag: Option<ThumbDrag>,
}

impl Harness {
    /// The REAL TextInput widget - inline `with_css_props` styling, the shape
    /// AzWidgets ships - alone in a `width` x `height` window.
    fn new(value: &str, width: f32, height: f32) -> Self {
        let widget = azul_layout::widgets::text_input::TextInput::create()
            .with_text(value.into())
            .dom();
        let styled_dom = StyledDom::create_from_dom(Dom::create_body().with_child(widget));

        let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
        let mut window_state = FullWindowState::default();
        window_state.size.dimensions = LogicalSize::new(width, height);
        lw.current_window_state = window_state.clone();
        let renderer_resources = RendererResources::default();
        let system_callbacks = ExternalSystemCallbacks::rust_internal();
        let mut dbg = Some(Vec::new());
        lw.layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut dbg,
        )
        .unwrap();
        lw.current_window_state = window_state;

        let mut h = Self {
            lw,
            hit_tester: CpuHitTester::new(),
            renderer_resources,
            system_callbacks,
            last_press: None,
            thumb_drag: None,
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

    /// The resize fast path as the shells drive it: `resize_only_hint` +
    /// relayout of the SAME StyledDom at the new size, then the finalize tail.
    fn resize(&mut self, width: f32, height: f32) {
        let mut window_state = self.lw.current_window_state.clone();
        window_state.size.dimensions = LogicalSize::new(width, height);
        self.lw.layout_cache.resize_only_hint = true;
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
                &self.renderer_resources,
                &self.system_callbacks,
                &mut dbg,
            )
            .unwrap();
        self.lw.current_window_state = window_state;
        self.finalize();
    }

    /// Port of `PlatformWindow::update_hit_test_at` (the same port as
    /// `textinput_resize_selection.rs` and `layout/src/e2e/runner.rs`).
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

    /// A PHYSICAL press, routed the way every shell routes it: the scrollbar
    /// gate first. A press on a bar is the bar's - a thumb starts a drag, the
    /// track pages (not ported: the press is consumed all the same) - and the
    /// text never hears of it. Only a press past the gate becomes the
    /// text-selection click and the click-to-focus of the event pass.
    fn press(&mut self, at: LogicalPosition, time_ms: u64) {
        self.last_press = scrollbar_under_press(&self.lw, at);
        if let Some(hit) = self.last_press {
            if hit.component == ScrollbarComponent::Thumb {
                let initial_offset = self
                    .lw
                    .scroll_manager
                    .get_current_offset(hit.dom_id, hit.node_id)
                    .unwrap_or_default();
                self.thumb_drag = Some(ThumbDrag {
                    hit,
                    initial_offset,
                });
            }
            return;
        }
        self.update_hit_test_at(at);
        self.lw.process_mouse_click_for_selection(at, time_ms);
        self.lw.focus_manager.set_focused_node(Some(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(HOST)),
        }));
    }

    /// The pointer moves with the button held: a grabbed thumb follows it
    /// (port of `handle_scrollbar_drag`), anything else extends the text
    /// selection from the press.
    fn drag(&mut self, from: LogicalPosition, to: LogicalPosition) {
        let Some(drag) = self.thumb_drag else {
            self.update_hit_test_at(to);
            self.lw.process_mouse_drag_for_selection(from, to);
            return;
        };
        let (dom, node, orientation) = (drag.hit.dom_id, drag.hit.node_id, drag.hit.orientation);
        let Some(bar) = self
            .lw
            .scroll_manager
            .get_scrollbar_state(dom, node, orientation)
            .copied()
        else {
            return;
        };
        let Some(info) = self.lw.scroll_manager.get_scroll_node_info(dom, node) else {
            return;
        };
        let (pixel_delta, track, max_scroll, initial) = match orientation {
            ScrollbarOrientation::Horizontal => (
                to.x - drag.hit.global_position.x,
                bar.track_rect.size.width,
                info.max_scroll_x,
                drag.initial_offset.x,
            ),
            ScrollbarOrientation::Vertical => (
                to.y - drag.hit.global_position.y,
                bar.track_rect.size.height,
                info.max_scroll_y,
                drag.initial_offset.y,
            ),
        };
        let usable = (track - bar.thumb_size_ratio * track).max(1.0);
        let target = (initial + pixel_delta / usable * max_scroll).clamp(0.0, max_scroll);
        let mut offset = info.current_offset;
        match orientation {
            ScrollbarOrientation::Horizontal => offset.x = target,
            ScrollbarOrientation::Vertical => offset.y = target,
        }
        let now = Instant::from(std::time::Instant::now());
        self.lw
            .scroll_manager
            .set_scroll_position(dom, node, offset, now);
        self.lw.scroll_manager.calculate_scrollbar_states();
    }

    /// The border box of `node` as laid out (static space; nothing above the
    /// field scrolls).
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

    /// The value text's laid-out advance width. Reads the MATERIALIZED layout:
    /// under `AZ_DENSE_TEXT=1` the stored one is the retirement sentinel.
    fn text_extent(&self) -> f32 {
        let lr = self.lw.layout_results.get(&DomId::ROOT_ID).unwrap();
        let tree = &lr.layout_tree;
        (0..tree.nodes.len())
            .filter_map(|idx| tree.materialized_inline_layout_for_node(idx))
            .fold(0.0_f32, |max, layout| max.max(layout.bounds().width))
    }

    /// The primary selection as `(anchor byte, focus byte)` when it is a
    /// range, `Err` with what it is otherwise.
    fn selected_bytes(&self) -> Result<(u32, u32), String> {
        let Some(mc) = self.lw.text_edit_manager.multi_cursor.as_ref() else {
            return Err("no editing session at all".to_string());
        };
        match mc.get_primary().map(|c| &c.selection) {
            Some(Selection::Range(r)) => Ok((
                r.start.cluster_id.start_byte_in_run,
                r.end.cluster_id.start_byte_in_run,
            )),
            other => Err(format!("{other:?}")),
        }
    }

    /// The painted selection bands.
    fn selection_rects(&self) -> usize {
        use azul_layout::solver3::display_list::DisplayListItem;
        self.lw
            .get_layout_result(&DomId::ROOT_ID)
            .expect("layout result")
            .display_list
            .items
            .iter()
            .filter(|item| matches!(item, DisplayListItem::SelectionRect { .. }))
            .count()
    }

    fn value_offset(&self) -> Option<LogicalPosition> {
        self.lw
            .scroll_manager
            .get_current_offset(DomId::ROOT_ID, VALUE_P)
    }
}

/// Press on the text 20px in, drag 40px to the right, all inside the field:
/// the six-odd glyphs under the drag are selected and painted, and the text
/// does not move.
fn press_and_drag_selects_the_dragged_glyphs(h: &mut Harness, label: &str) {
    let field = h.border_box(VALUE_P);
    let press = LogicalPosition::new(
        field.origin.x + 20.0,
        field.origin.y + field.size.height / 2.0,
    );
    let release = LogicalPosition::new(press.x + 40.0, press.y);
    assert!(
        release.x + 10.0 < field.origin.x + field.size.width,
        "harness ({label}): the drag must end inside the field, {release:?} vs {field:?}"
    );
    let offset_before = h.value_offset();

    h.press(press, 0);
    h.drag(press, release);

    let (anchor, focus) = h.selected_bytes().unwrap_or_else(|what| {
        panic!(
            "THE BUG ({label}): a press on the text at {press:?} and a drag to {release:?} \
             selected nothing - the selection is {what}. The press went to {:?} (a scrollbar hit \
             takes the press before the text sees it).",
            h.last_press
        )
    });
    assert!(
        anchor < focus && focus - anchor >= 3,
        "{label}: 40px of 11px text is several glyphs, but the range is bytes {anchor}..{focus}"
    );
    assert!(
        anchor < 10,
        "{label}: the range starts under the press, 20px into the field, not at byte {anchor}"
    );
    assert!(
        h.selection_rects() > 0,
        "{label}: the selected range {anchor}..{focus} paints no SelectionRect"
    );
    assert_eq!(
        h.value_offset(),
        offset_before,
        "{label}: a drag that stays inside the field must not scroll its text"
    );
}

/// THE DEVICE BUG. The value overflows a 200px window; a press on the text
/// and a 40px drag must select, exactly as they do when the value fits.
#[test]
fn a_press_and_drag_on_an_overflowing_field_selects_the_dragged_text() {
    let mut h = Harness::new(VALUE, 200.0, 120.0);

    let field = h.border_box(VALUE_P);
    let extent = h.text_extent();
    assert!(
        extent > field.size.width + 20.0,
        "harness: the value ({extent}px) must overflow its field ({field:?})"
    );
    assert!(
        h.value_offset().is_some(),
        "harness: the overflowing value <p> is a registered scroll container"
    );

    press_and_drag_selects_the_dragged_glyphs(&mut h, "overflowing");
}

/// The user's repro from `textinput_resize_selection.rs`, M1b: shrink the
/// window so the value overflows, scroll it (a caret reveal or the wheel
/// does), grow the window back so the value fits - the text is back at the
/// start of its field and a press + drag still selects.
///
/// Registration skipped every box that did not overflow, so the value `<p>`
/// kept the state it had while it did: its old, narrow rects and its
/// offset. The text that now fits stayed scrolled out of its own field.
#[test]
fn a_field_that_fits_again_is_back_at_its_start_and_still_selects() {
    let mut h = Harness::new(VALUE, 200.0, 120.0);
    let max_scroll = h
        .lw
        .scroll_manager
        .get_scroll_node_info(DomId::ROOT_ID, VALUE_P)
        .expect("harness: the overflowing value <p> is a registered scroll container")
        .max_scroll_x;
    assert!(
        max_scroll > 40.0,
        "harness: the value must overflow by more than 40px, got {max_scroll}"
    );
    h.lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        VALUE_P,
        LogicalPosition::new(40.0, 0.0),
        Instant::from(std::time::Instant::now()),
    );
    assert_eq!(
        h.value_offset(),
        Some(LogicalPosition::new(40.0, 0.0)),
        "harness: the value is scrolled 40px"
    );

    h.resize(900.0, 120.0);
    let field = h.border_box(VALUE_P);
    let extent = h.text_extent();
    assert!(
        extent + 10.0 < field.size.width,
        "harness: at 900px the value ({extent}px) fits its field ({field:?})"
    );

    assert_eq!(
        h.value_offset(),
        Some(LogicalPosition::zero()),
        "THE BUG (M1b): the value fits its field again but is still scrolled out of it"
    );
    press_and_drag_selects_the_dragged_glyphs(&mut h, "fits again");
}
