//! Clicking a page inside a `VirtualView` must place the caret under the
//! pointer — AzWriter's document canvas, reduced to its load-bearing shape.
//!
//! AzWriter (`examples/azul-writer`) paints its document as a stack of A4
//! sheets inside a `VirtualView`: only a window of pages is materialised, into
//! a NESTED dom the engine composites at
//! `host.bounds.origin + (materialized_origin - scroll_offset)`. Each sheet
//! holds one `contenteditable` content root with the page's blocks under it.
//!
//! Three things have to hold for a click to become a caret, and each one was
//! broken:
//!
//! 1. the hit test must find the nested dom's nodes — it clipped the child to
//!    a viewport that MOVED with the scrolled content, so past the first
//!    screenful nothing in the page was hittable at all;
//! 2. an EMPTY editable line must accept a caret — `hittest_cursor` answers
//!    `None` for a layout with no clusters, which is exactly the editing-host
//!    strut a blank document is made of, so a new document could not be
//!    clicked into;
//! 3. focusing an empty editing host must anchor the caret on a line that can
//!    be painted, not on the host block itself.
//!
//! (3) is also the STARTUP caret, which never involves a click at all: AzWriter
//! calls `set_focus_to_path(ROOT, ".mw-doc")` once the first layout exists, and
//! that resolves to the editing host, so the engine alone decides which line
//! the caret lands on.
//!
//! The DOM here is built with `StyledDom::create_from_dom`, the same entry the
//! shell uses — `create(dom, Css::empty())` skips the per-subtree inline-CSS
//! scoping and the page canvas never resolves its flex size.

use azul_core::callbacks::{
    EdgeType, VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewCallbackReason,
    VirtualViewReturn,
};
use azul_core::dom::{Dom, DomId, DomNodeId, NodeId, NodeType, OptionDom};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::refany::RefAny;
use azul_core::resources::RendererResources;
use azul_core::styled_dom::{NodeHierarchyItemId, StyledDom};
use azul_layout::headless::CpuHitTester;
use azul_layout::managers::hover::InputPointId;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;
use std::sync::{Arc, Mutex};

/// A4 at 96 dpi with 1 inch margins — `document::a4_page_setup()`.
const PAGE_W: f32 = 794.0;
const PAGE_H: f32 = 1123.0;
const PAGE_PAD: f32 = 96.0;
/// One sheet plus its bottom margin — `editor_ui::page_stride`.
const STRIDE: f32 = PAGE_H + 16.0;
const TOTAL_PAGES: usize = 12;

const WIN_W: f32 = 1400.0;
const WIN_H: f32 = 900.0;
/// Height of the fake title band + ribbon above the canvas.
const CHROME_H: f32 = 150.0;

/// What the document holds: `None` = the blank "Document1" (one implicit empty
/// paragraph, exactly what `markdown_to_content_dom("")` produces).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Doc {
    Blank,
    Paragraphs,
}

struct Probe {
    doc: Doc,
    /// The window the callback last materialised, and the offset it saw.
    first: usize,
    count: usize,
    scroll_y: f32,
    /// How many times the callback ran, and the reason each time.
    invocations: Vec<VirtualViewCallbackReason>,
    /// When set, the next materialisation prepends a banner subtree before
    /// the pages — every page/paragraph NodeId shifts, the way inserting a
    /// block above the viewport shifts a real document's arena.
    prepend_banner: bool,
}

type Shared = Arc<Mutex<Probe>>;

/// One page sheet: the white A4 box, with the page's `contenteditable`
/// content root inside its margin box.
fn page_dom(doc: Doc, page_idx: usize) -> Dom {
    let sheet_css = format!(
        "width: {}px; height: {}px; background: white; flex-grow: 0; flex-shrink: 0; \
         border: 1px solid #a6a6a6; margin-bottom: 16px; box-sizing: border-box; \
         padding: {}px; overflow: hidden;",
        PAGE_W as isize, PAGE_H as isize, PAGE_PAD as isize,
    );
    // The `contenteditable` flag lives on the content ROOT, and the blocks are
    // its children — `document::unwrap_html_shell` builds exactly this shape.
    let mut content = Dom::create_div().with_css("display: block;");
    content.set_contenteditable(true);
    let content = match doc {
        // The implicit empty paragraph a blank document still gets.
        Doc::Blank => content.with_child(Dom::create_p()),
        Doc::Paragraphs => content.with_child(Dom::create_p_with_text(format!(
            "Paragraph {page_idx} of the document, long enough to click into."
        ))),
    };
    Dom::create_div().with_css(&sheet_css).with_child(content)
}

/// `editor_ui::pages_virtual_view`, structurally verbatim.
extern "C" fn pages_view(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let probe = data.downcast_ref::<Shared>().expect("probe").clone();
    let mut probe = probe.lock().unwrap();

    let viewport_h = info.bounds.get_logical_size().height;
    let first_visible = (info.scroll_offset.y.max(0.0) / STRIDE) as usize;
    let first = first_visible.saturating_sub(1);
    let visible = (viewport_h / STRIDE).ceil() as usize + 2;
    let count = visible.max(3).min(TOTAL_PAGES.saturating_sub(first));
    probe.first = first;
    probe.count = count;
    probe.scroll_y = info.scroll_offset.y;
    probe.invocations.push(info.reason);

    let mut col =
        Dom::create_div().with_css("display: flex; flex-direction: column; align-items: center;");
    if probe.prepend_banner {
        col.add_child(
            Dom::create_div()
                .with_css("height: 24px;")
                .with_child(Dom::create_p_with_text("banner")),
        );
    }
    for page_idx in first..(first + count) {
        col.add_child(page_dom(probe.doc, page_idx));
    }

    VirtualViewReturn {
        dom: OptionDom::Some(col),
        materialized: LogicalRect::new(
            LogicalPosition::new(0.0, first as f32 * STRIDE),
            LogicalSize::new(PAGE_W + 2.0, count as f32 * STRIDE),
        ),
        virtual_rect: LogicalRect::new(
            LogicalPosition::zero(),
            LogicalSize::new(PAGE_W + 2.0, TOTAL_PAGES as f32 * STRIDE),
        ),
    }
}

struct Harness {
    lw: LayoutWindow,
    probe: Shared,
}

impl Harness {
    fn new(doc: Doc) -> Self {
        let probe: Shared = Arc::new(Mutex::new(Probe {
            doc,
            first: 0,
            count: 0,
            scroll_y: 0.0,
            invocations: Vec::new(),
            prepend_banner: false,
        }));
        let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
        let mut window_state = FullWindowState::default();
        window_state.size.dimensions = LogicalSize::new(WIN_W, WIN_H);
        lw.current_window_state = window_state;
        let mut h = Self { lw, probe };
        h.relayout();
        h
    }

    fn relayout(&mut self) {
        let canvas = Dom::create_div()
            .with_css(
                "flex-grow: 1; min-height: 0px; background: #e3e3e3; display: flex; \
                 flex-direction: column; align-items: center; padding-top: 18px; \
                 overflow: hidden;",
            )
            .with_child(
                Dom::create_virtual_view(
                    RefAny::new(self.probe.clone()),
                    VirtualViewCallback::create(pages_view),
                )
                .with_css("flex-grow: 1; min-height: 0px; width: 100%;"),
            );
        let dom = Dom::create_body()
            .with_css(
                "display: flex; flex-direction: column; margin: 0; padding: 0; height: 100%; \
                 background: white; font-size: 15px;",
            )
            .with_child(
                Dom::create_div()
                    .with_css(
                        "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;",
                    )
                    .with_child(
                        Dom::create_div()
                            .with_css(&format!("height: {}px; flex-grow: 0;", CHROME_H as isize)),
                    )
                    .with_child(canvas)
                    .with_child(Dom::create_div().with_css("height: 24px; flex-grow: 0;")),
            );
        let styled_dom = StyledDom::create_from_dom(dom);
        let window_state = self.lw.current_window_state.clone();
        let renderer_resources = RendererResources::default();
        let system_callbacks = ExternalSystemCallbacks::rust_internal();
        let mut debug_messages = Some(Vec::new());
        self.lw
            .layout_and_generate_display_list(
                styled_dom,
                &window_state,
                &renderer_resources,
                &system_callbacks,
                &mut debug_messages,
            )
            .unwrap();
    }

    /// The pages `VirtualView` host node and the nested dom it mounted.
    fn virtual_view(&self) -> (DomId, NodeId) {
        let lr = self
            .lw
            .get_layout_result(&DomId::ROOT_ID)
            .expect("root layout");
        let n = lr.styled_dom.node_data.as_container().len();
        (0..n)
            .map(NodeId::new)
            .find_map(|node| {
                self.lw
                    .virtual_view_manager
                    .get_nested_dom_id(DomId::ROOT_ID, node)
                    .map(|child| (child, node))
            })
            .expect("the canvas mounted a VirtualView")
    }

    /// The shells' `perform_hit_test` + `update_hit_test_at`, CPU arm, then
    /// the click-to-caret entry point they call on mouse-down.
    fn click_at(&mut self, position: LogicalPosition) {
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout_with_gpu(
            &self.lw.layout_results,
            Some(&self.lw.gpu_state_manager),
        );
        let focused = self.lw.focus_manager.get_focused_node().copied();
        let hit = {
            let scroll_manager = &self.lw.scroll_manager;
            let gpu = &self.lw.gpu_state_manager;
            let resolve = |d: DomId, n: NodeId| scroll_manager.get_current_offset(d, n);
            let resolve_tf = |d: DomId, n: NodeId| {
                gpu.caches
                    .get(&d)
                    .and_then(|c| c.css_current_transform_values.get(&n))
                    .copied()
            };
            let hits = tester.hit_test_scrolled(position, &resolve, &resolve_tf);
            azul_layout::headless::convert_cpu_hit_test_to_full(
                &tester,
                &hits,
                focused,
                &self.lw.layout_results,
                position,
                &resolve,
                &resolve_tf,
            )
        };
        self.lw
            .hover_manager
            .push_hit_test(InputPointId::Mouse, hit);
        self.lw.process_mouse_click_for_selection(position, 0);
    }

    /// One committed wheel/physics offset for the pages host, exactly as the
    /// shells and the headless runner handle a `ScrollTo` (`common/event.rs`,
    /// `e2e/runner.rs`): set the offset, let the view decide whether the new
    /// offset demands a re-materialization, re-point the host item, then run
    /// the pre-frame drain. Returns whether the callback was re-invoked.
    fn scroll_tick(&mut self, y: f32) -> bool {
        let (_nested, host) = self.virtual_view();
        let system_callbacks = ExternalSystemCallbacks::rust_internal();
        let now = (system_callbacks.get_system_time_fn.cb)();
        self.lw.scroll_manager.set_scroll_position_unclamped(
            DomId::ROOT_ID,
            host,
            LogicalPosition::new(0.0, y),
            now,
        );
        self.lw.scroll_manager.calculate_scrollbar_states();
        self.lw
            .check_and_queue_virtual_view_reinvoke(DomId::ROOT_ID, host);
        self.lw
            .patch_virtual_view_content_offset(DomId::ROOT_ID, host);

        let window_state = self.lw.current_window_state.clone();
        let renderer_resources = RendererResources::default();
        let updated = self.lw.process_pending_virtual_view_updates(
            &window_state,
            &renderer_resources,
            &system_callbacks,
        );
        !updated.is_empty()
    }

    /// The host display list's `VirtualView` item for the pages: where the
    /// renderer composites the nested dom (`bounds`, `content_offset`).
    fn host_item(&self) -> (LogicalRect, LogicalPosition) {
        let (nested, _host) = self.virtual_view();
        self.lw
            .get_layout_result(&DomId::ROOT_ID)
            .expect("root layout")
            .display_list
            .items
            .iter()
            .find_map(|item| match item {
                DisplayListItem::VirtualView {
                    child_dom_id,
                    bounds,
                    content_offset,
                    ..
                } if *child_dom_id == nested => Some((*bounds.inner(), *content_offset)),
                _ => None,
            })
            .expect("the host display list mounts the nested dom")
    }

    /// Window-space rect of a node in a (possibly nested) dom, where the
    /// rasteriser puts it.
    fn window_rect(&self, dom: DomId, node: NodeId) -> Option<(LogicalPosition, LogicalSize)> {
        let lr = self.lw.get_layout_result(&dom)?;
        let idx = *lr.layout_tree.dom_to_layout.get(&node)?.first()?;
        let pos = lr.calculated_positions.get(idx.index()).copied()?;
        let size = lr.layout_tree.nodes.get(idx.index())?.used_size?;
        let lift = self.lw.window_space_offset_of_dom(dom);
        Some((LogicalPosition::new(pos.x + lift.x, pos.y + lift.y), size))
    }

    /// The first block in `dom` that is a `contenteditable` host, and the
    /// first `<p>` under it.
    fn first_editable_line(&self, dom: DomId) -> NodeId {
        let lr = self.lw.get_layout_result(&dom).expect("nested layout");
        let nd = lr.styled_dom.node_data.as_container();
        (0..nd.len())
            .map(NodeId::new)
            .find(|id| matches!(nd[*id].get_node_type(), NodeType::P))
            .expect("a materialised page has a paragraph")
    }

    /// The first page's editing host — the content root `.mw-doc` sits on, and
    /// the node `set_focus_to_path` resolves to.
    fn first_editing_host(&self, dom: DomId) -> NodeId {
        let lr = self.lw.get_layout_result(&dom).expect("nested layout");
        let nd = lr.styled_dom.node_data.as_container();
        (0..nd.len())
            .map(NodeId::new)
            .find(|id| nd[*id].is_contenteditable())
            .expect("a materialised page has a contenteditable content root")
    }

    /// Scrollbar items (track/thumb, either variant) in `dom`'s display list.
    fn scrollbar_count(&self, dom: DomId) -> usize {
        self.lw
            .get_layout_result(&dom)
            .expect("layout")
            .display_list
            .items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    DisplayListItem::ScrollBar { .. } | DisplayListItem::ScrollBarStyled { .. }
                )
            })
            .count()
    }

    /// How many carets the rasteriser would draw in `dom`. The blink-off phase
    /// still emits the item (with alpha 0), so this counts a caret that EXISTS,
    /// not one that happens to be lit this frame.
    fn caret_rects(&self, dom: DomId) -> usize {
        self.lw
            .get_layout_result(&dom)
            .expect("nested layout")
            .display_list
            .items
            .iter()
            .filter(|item| matches!(item, DisplayListItem::CursorRect { .. }))
            .count()
    }
}

/// A loaded document: clicking a quarter of the way into the first
/// paragraph's line must place the caret in that paragraph.
#[test]
fn clicking_a_paragraph_on_a_materialised_page_places_the_caret_in_it() {
    let mut h = Harness::new(Doc::Paragraphs);
    let (nested, _vv) = h.virtual_view();
    let para = h.first_editable_line(nested);
    let (origin, size) = h
        .window_rect(nested, para)
        .expect("the paragraph has a box");

    let target = LogicalPosition::new(origin.x + size.width * 0.25, origin.y + size.height * 0.5);
    h.click_at(target);

    let session =
        h.lw.text_edit_manager
            .multi_cursor
            .as_ref()
            .map(|mc| mc.node_id);
    assert!(
        session.is_some(),
        "clicking the text of a page sheet placed no caret at all \
         (click {target:?}, paragraph {origin:?} {size:?})"
    );
    assert_eq!(
        session.unwrap().dom,
        nested,
        "the caret session must live in the VirtualView's nested dom"
    );
}

/// A NEW document: the page is the implicit empty paragraph. Clicking that
/// line must still place a caret, or the document cannot be typed into.
///
/// The editing-host strut exists precisely so an empty editable keeps ONE line
/// box for a caret to stand on; `hittest_cursor` returning `None` for a layout
/// with no clusters made that line unclickable.
#[test]
fn clicking_the_empty_line_of_a_blank_document_places_a_caret() {
    let mut h = Harness::new(Doc::Blank);
    let (nested, _vv) = h.virtual_view();
    let line = h.first_editable_line(nested);
    let (origin, size) = h
        .window_rect(nested, line)
        .expect("the empty paragraph keeps a strut line box");
    assert!(
        size.height > 0.0,
        "the editing-host strut must give the empty paragraph a line box"
    );

    let target = LogicalPosition::new(origin.x + 20.0, origin.y + size.height * 0.5);
    h.click_at(target);

    assert!(
        h.lw.text_edit_manager.has_active_editing(),
        "clicking the blank page's empty line placed no caret, so typing goes \
         nowhere (click {target:?}, line {origin:?} {size:?})"
    );
}

/// Scrolled deep into the document the materialised window no longer starts at
/// row 0, so `content_offset` is a large negative number. The child's VIEWPORT
/// does not move with it: the pointer is still over the canvas, and a click
/// must still reach the page under it.
#[test]
fn clicking_a_page_after_scrolling_past_the_first_screenful_still_places_a_caret() {
    let mut h = Harness::new(Doc::Paragraphs);
    let (_nested, vv) = h.virtual_view();
    h.lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        vv,
        LogicalPosition::new(0.0, 3.0 * STRIDE),
        azul_core::task::Instant::from(std::time::Instant::now()),
    );
    h.relayout();
    let content_offset = h.lw.virtual_view_content_offset(DomId::ROOT_ID, vv);
    assert!(
        content_offset.y < -1.0,
        "the scenario needs a non-zero content offset to prove anything, got \
         {content_offset:?}"
    );

    let (nested, _) = h.virtual_view();
    // Every paragraph of the materialised window; click the first one whose
    // line is actually inside the canvas viewport.
    let lr = h.lw.get_layout_result(&nested).expect("nested layout");
    let nd = lr.styled_dom.node_data.as_container();
    let paras: Vec<NodeId> = (0..nd.len())
        .map(NodeId::new)
        .filter(|id| matches!(nd[*id].get_node_type(), NodeType::P))
        .collect();
    drop(lr);
    let visible = paras
        .into_iter()
        .filter_map(|p| h.window_rect(nested, p).map(|r| (p, r)))
        .find(|(_, (origin, size))| origin.y >= CHROME_H && origin.y + size.height <= WIN_H - 24.0)
        .expect("at least one materialised paragraph is inside the canvas viewport");
    let (_, (origin, size)) = visible;

    let target = LogicalPosition::new(origin.x + size.width * 0.25, origin.y + size.height * 0.5);
    h.click_at(target);

    assert!(
        h.lw.text_edit_manager.has_active_editing(),
        "after scrolling, a click on a visible page paragraph placed no caret \
         (click {target:?}, paragraph {origin:?} {size:?}, content_offset {content_offset:?})"
    );
}
/// AzWriter's STARTUP caret on a brand-new document, with no click involved:
/// 150 ms after the window opens the app calls `set_focus_to_path(ROOT,
/// ".mw-doc")`, which resolves to the editing HOST — the class sits on the
/// content root, never on a line inside it.
///
/// A blank document has no text node anywhere under that host, so
/// `find_last_text_child` answers `None` and the old fallback anchored the
/// editing session on the host itself. The host is a block container: it has
/// no inline layout, so `paint_cursor` returns before it reaches a caret and
/// there is nothing on screen to type into. The anchor has to be the host's
/// empty editable LINE, the one `layout_ifc` gave the editing-host strut.
#[test]
fn focusing_a_blank_documents_editing_host_anchors_the_caret_on_its_empty_line() {
    let mut h = Harness::new(Doc::Blank);
    let (nested, _vv) = h.virtual_view();
    let host = h.first_editing_host(nested);
    let line = h.first_editable_line(nested);
    assert!(
        h.caret_rects(nested) == 0,
        "premise: nothing is focused yet, so no caret is painted"
    );

    // `dll/.../shell2/common/event.rs`'s SetFocusTarget arm, verbatim: adopt
    // the focus, flag the deferred contenteditable caret, finalize it. (The
    // shell also scrolls the node into view between the first two; that moves
    // the viewport, never the anchor.)
    let focus = DomNodeId {
        dom: nested,
        node: NodeHierarchyItemId::from_crate_internal(Some(host)),
    };
    let window_state = h.lw.current_window_state.clone();
    h.lw.focus_manager.set_focused_node(Some(focus));
    let _ =
        h.lw.handle_focus_change_for_cursor_blink(Some(focus), &window_state);

    let pending =
        h.lw.focus_manager
            .pending_contenteditable_focus
            .clone()
            .expect("focusing a contenteditable flags a deferred caret");
    assert_eq!(
        pending.container_node_id, host,
        "focus itself still belongs to the editing host"
    );
    assert_eq!(
        pending.text_node_id, line,
        "the caret must be anchored on the empty editable LINE ({line:?}); \
         anchoring it on the host block ({host:?}) is an editing session whose \
         caret has no inline layout to be painted in"
    );

    assert!(
        h.lw.finalize_pending_focus_changes(),
        "the deferred caret must be seeded in the pass after layout"
    );
    assert!(
        h.lw.text_edit_manager.has_active_editing(),
        "a focused blank document is an open editing session"
    );

    h.lw.regenerate_display_list_for_dom(nested);
    assert_eq!(
        h.caret_rects(nested),
        1,
        "the blank page must paint exactly one caret — with the session \
         anchored on the host block instead, `paint_cursor` finds no inline \
         layout and the new document shows no caret at all"
    );
}

/// The paragraph in `dom` whose text contains `needle`.
fn paragraph_containing(h: &Harness, dom: DomId, needle: &str) -> NodeId {
    let lr = h.lw.get_layout_result(&dom).expect("nested layout");
    let nd = lr.styled_dom.node_data.as_container();
    (0..nd.len())
        .map(NodeId::new)
        .find(|id| {
            matches!(nd[*id].get_node_type(), NodeType::P)
                && h.edited_text(dom, *id).contains(needle)
        })
        .unwrap_or_else(|| panic!("no paragraph containing {needle:?}"))
}

impl Harness {
    /// The text the ENGINE currently holds for a node — overlay included.
    fn edited_text(&self, dom: DomId, node: NodeId) -> String {
        let content = self.lw.get_text_before_textinput(dom, node);
        self.lw.extract_text_from_inline_content(&content)
    }
}

/// Re-materialisation must be a RECONCILE, not a rebirth: state keyed
/// (DomId, NodeId) has to follow the CONTENT it was attached to when a fresh
/// arena renumbers every node.
///
/// The failure this pins was found live: with a document that had text, typing
/// painted the edited paragraph's text over a DIFFERENT paragraph after the
/// full-relayout funnel re-materialised the VirtualView — the overlay entry
/// kept its old NodeId, the new arena assigned that id to another block, and
/// only hover state was ever purged. Every prior test here asserts a caret
/// EXISTS; this one asserts the CONTENT lands where the user typed it.
#[test]
fn typed_text_follows_its_paragraph_across_a_rematerialisation() {
    let mut h = Harness::new(Doc::Paragraphs);
    let (nested, _vv) = h.virtual_view();

    // Type into paragraph 0 through the same seam the shells use.
    let target = paragraph_containing(&h, nested, "Paragraph 0");
    h.lw.focus_manager.set_focused_node(Some(DomNodeId {
        dom: nested,
        node: NodeHierarchyItemId::from_crate_internal(Some(target)),
    }));
    h.lw.record_text_input("ZZZ");
    let _ = h.lw.apply_text_changeset();
    assert!(
        h.edited_text(nested, target).contains("ZZZ"),
        "premise: the edit landed on paragraph 0"
    );

    // Re-materialise with a SHIFTED arena: a banner subtree now precedes the
    // pages, so every page/paragraph NodeId moves — the exact shape a
    // keystroke-driven full relayout produces on a paginating document.
    h.probe.lock().unwrap().prepend_banner = true;
    h.relayout();

    let (nested_after, _) = h.virtual_view();
    let p0_after = paragraph_containing(&h, nested_after, "Paragraph 0");
    assert_ne!(
        p0_after, target,
        "premise: the banner actually shifted paragraph 0's NodeId"
    );

    assert!(
        h.edited_text(nested_after, p0_after).contains("ZZZ"),
        "the edit must FOLLOW paragraph 0 across the re-materialisation — \
         without reconcile+remap it stays keyed to the old NodeId and lands \
         on whatever block bears that id in the new arena"
    );
    let banner = paragraph_containing(&h, nested_after, "banner");
    assert!(
        !h.edited_text(nested_after, banner).contains("ZZZ"),
        "the node now wearing the OLD id must not inherit the edit"
    );
}

/// A 12-page document inside the VirtualView is ~13,600 virtual px against a
/// 900 px window: `overflow: auto` (the UA default for a VirtualView) plus a
/// published `virtual_rect` that tall MUST produce a scrollbar. The layout-side
/// necessity test can never fire for a VirtualView (it has no flow content),
/// so the bar's existence rides entirely on the published virtual size
/// reaching the display-list build — the seam where it was lost on device
/// (AzWriter showed a page running past the viewport with no bar at all).
///
/// Two mechanisms are pinned, one per frame:
/// - frame 1: the host list is built BEFORE the callback publishes, so the
///   funnel must detect the changed scroll-geometry fingerprint and rebuild
///   (`regenerate_display_list_for_dom`) within the same pass;
/// - frame 2: the DL cache key includes the scroll-geometry fingerprint, so
///   an identical tree can no longer serve the pre-publication list back.
#[test]
fn the_virtualized_documents_scrollbar_is_painted_once_its_size_is_published() {
    let mut h = Harness::new(Doc::Paragraphs);
    let frame1 = h.scrollbar_count(DomId::ROOT_ID);
    h.relayout();
    let frame2 = h.scrollbar_count(DomId::ROOT_ID);
    assert!(
        frame1 >= 1,
        "the FIRST frame flashed bar-less: the publish-after-consume rebuild \
         did not fire (frame1={frame1})"
    );
    assert!(
        frame2 >= 1,
        "a 13,600px virtual document in a 900px window paints NO scrollbar on \
         the second full pass (frame1={frame1}, frame2={frame2}) — the \
         published virtual size is not reaching the display-list build"
    );
}

/// The IME/window-space lift must agree with the RENDERER's composition: the
/// nested dom's window-space origin is, by definition, where the host display
/// list's `VirtualView` item composites it (`bounds.origin + content_offset`).
/// `window_space_offset_of_dom` used to re-derive this from layout positions —
/// a fourth, chain-unaware answer — which is exactly how an IME candidate
/// window drifted off a caret that clicks could still hit.
#[test]
fn the_window_space_lift_agrees_with_what_the_renderer_composites() {
    let h = Harness::new(Doc::Paragraphs);
    let (nested, _host) = h.virtual_view();

    let item = h
        .lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("root layout")
        .display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::VirtualView {
                child_dom_id,
                bounds,
                content_offset,
                ..
            } if *child_dom_id == nested => Some((*bounds.inner(), *content_offset)),
            _ => None,
        })
        .expect("the host display list mounts the nested dom");
    let (bounds, content_offset) = item;

    let lifted = h.lw.window_space_offset_of_dom(nested);
    let composited = LogicalPosition::new(
        bounds.origin.x + content_offset.x,
        bounds.origin.y + content_offset.y,
    );
    assert!(
        (lifted.x - composited.x).abs() < 0.01 && (lifted.y - composited.y).abs() < 0.01,
        "window_space_offset_of_dom answers {lifted:?} but the renderer \
         composites the child at {composited:?} — the two derivations drifted"
    );
    assert!(
        composited.y > 0.0,
        "premise: the host does not sit at the window origin (chrome above it)"
    );
}

/// The device symptom "typed text sits at the very TOP of the page": the sheet
/// declares `padding: 96px`, so the first editable line's window-space y must
/// sit at least that far below the sheet's top edge. If this holds headlessly,
/// the AzWriter symptom is app-CSS-specific; if it fails, the padding is lost
/// inside the VirtualView child layout itself.
#[test]
fn the_blank_pages_first_line_respects_the_sheet_padding() {
    let h = Harness::new(Doc::Blank);
    let (nested, _vv) = h.virtual_view();
    let line = h.first_editable_line(nested);
    let (line_origin, _) = h
        .window_rect(nested, line)
        .expect("the empty paragraph keeps a strut line box");

    // The sheet is the page-sized ancestor of the line.
    let lr = h.lw.get_layout_result(&nested).expect("nested layout");
    let hierarchy = lr.styled_dom.node_hierarchy.as_container();
    let mut cur = line;
    let mut sheet = None;
    while let Some(parent) = hierarchy.get(cur).and_then(|n| n.parent_id()) {
        if let Some((_, size)) = h.window_rect(nested, parent) {
            if (size.width - PAGE_W).abs() < 2.0 && (size.height - PAGE_H).abs() < 2.0 {
                sheet = Some(parent);
                break;
            }
        }
        cur = parent;
    }
    let sheet = sheet.expect("a page-sized ancestor above the first line");
    let (sheet_origin, _) = h.window_rect(nested, sheet).expect("sheet rect");

    assert!(
        line_origin.y >= sheet_origin.y + PAGE_PAD - 1.0,
        "the first line sits {}px below the sheet top; the sheet declares \
         {PAGE_PAD}px padding — the padding is being lost inside the \
         VirtualView child layout",
        line_origin.y - sheet_origin.y,
    );
}

/// Wheel-scrolling the whole 12-page document and back, 20 px per tick (a
/// Wayland wheel notch), through the same steps the shells run per committed
/// offset. Three laws, all of which were broken on device:
///
/// 1. The callback runs ONCE per window advance — nine times down (the
///    3-page window walks page 0 → page 9) and nine times back up — never
///    once per tick. The physics timer used to force a `DomRecreated`
///    re-materialization on EVERY tick (60/s), which is the "VV scroll lag".
/// 2. Each of those runs carries the documented reason, `EdgeScrolled(Bottom)`
///    going down and `EdgeScrolled(Top)` coming back — and it fires again on
///    the next approach, which the old per-edge latch (never released by
///    scrolling) made impossible after the first page.
/// 3. After every tick the host item composites the materialized window at
///    `materialized_origin - scroll_offset`, and that window covers the whole
///    viewport: no frame shows a page a stride too far, none shows bare
///    background.
#[test]
fn wheel_scrolling_rematerializes_once_per_window_advance_and_never_per_tick() {
    let mut h = Harness::new(Doc::Paragraphs);
    let (nested, host) = h.virtual_view();
    let viewport_h = h.host_item().0.size.height;
    assert!(
        viewport_h > 500.0 && viewport_h < STRIDE,
        "premise: the canvas shows less than one page ({viewport_h} px)"
    );
    let end = TOTAL_PAGES as f32 * STRIDE - viewport_h;
    let baseline = h.probe.lock().unwrap().invocations.len();

    let check_frame = |h: &Harness, y: f32| {
        let (bounds, content_offset) = h.host_item();
        let materialized = h
            .lw
            .virtual_view_manager
            .materialized_window_origin(DomId::ROOT_ID, host)
            .expect("materialized");
        assert!(
            (content_offset.y - (materialized.y - y)).abs() < 0.01,
            "at offset {y}: the host item composites the window at content_offset \
             {content_offset:?}, but the window starts at {materialized:?} — the \
             page would show {}px off for this frame",
            content_offset.y - (materialized.y - y)
        );
        let (first, count) = {
            let p = h.probe.lock().unwrap();
            (p.first, p.count)
        };
        let win_top = first as f32 * STRIDE;
        let win_bottom = (first + count) as f32 * STRIDE;
        assert!(
            win_top <= y && win_bottom >= y + viewport_h,
            "at offset {y}: viewport {y}..{} is not covered by the materialized \
             pages {first}..{} ({win_top}..{win_bottom}) — bare background",
            y + viewport_h,
            first + count
        );
        assert!(bounds.size.height > 0.0);
    };

    // Down.
    let mut y = 0.0;
    let mut down = Vec::new();
    while y < end {
        y = (y + 20.0).min(end);
        if h.scroll_tick(y) {
            down.push((y, h.probe.lock().unwrap().first));
        }
        check_frame(&h, y);
    }
    let after_down = h.probe.lock().unwrap().invocations.len();
    assert_eq!(
        after_down - baseline,
        down.len(),
        "every drain that re-materialized ran the callback exactly once"
    );
    assert_eq!(
        down.len(),
        TOTAL_PAGES - 3,
        "one re-materialization per page advance, none per tick: {down:?}"
    );
    for w in down.windows(2) {
        assert_eq!(w[1].1, w[0].1 + 1, "the window advances one page per fire: {down:?}");
    }
    assert_eq!(h.probe.lock().unwrap().first, TOTAL_PAGES - 3);
    {
        let p = h.probe.lock().unwrap();
        assert!(
            p.invocations[baseline..]
                .iter()
                .all(|r| *r == VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom)),
            "going down every re-materialization is an EdgeScrolled(Bottom): {:?}",
            &p.invocations[baseline..]
        );
    }

    // And back up.
    let mut up = Vec::new();
    while y > 0.0 {
        y = (y - 20.0).max(0.0);
        if h.scroll_tick(y) {
            up.push((y, h.probe.lock().unwrap().first));
        }
        check_frame(&h, y);
    }
    assert_eq!(
        up.len(),
        TOTAL_PAGES - 3,
        "one re-materialization per page retreat: {up:?}"
    );
    for w in up.windows(2) {
        assert_eq!(w[1].1 + 1, w[0].1, "the window retreats one page per fire: {up:?}");
    }
    assert_eq!(h.probe.lock().unwrap().first, 0);
    {
        let p = h.probe.lock().unwrap();
        assert!(
            p.invocations[after_down..]
                .iter()
                .all(|r| *r == VirtualViewCallbackReason::EdgeScrolled(EdgeType::Top)),
            "coming back every re-materialization is an EdgeScrolled(Top): {:?}",
            &p.invocations[after_down..]
        );
    }

    // The nested dom is the same mount throughout — the host item was
    // re-pointed, not replaced.
    assert_eq!(h.virtual_view().0, nested);

    // Parked at the top again, the view is quiet: ten more ticks of nothing.
    for _ in 0..10 {
        assert!(!h.scroll_tick(0.0), "a stationary offset must not re-materialize");
    }
}

/// A programmatic jump past everything materialized ("go to page 10") is the
/// documented `ScrollBeyondContent`, which had no producer at all: the view
/// re-materializes around the new offset in ONE drain, and the host item
/// composites it there in the same frame.
#[test]
fn a_jump_past_the_materialized_pages_rematerializes_around_the_new_offset() {
    let mut h = Harness::new(Doc::Paragraphs);
    let (_nested, host) = h.virtual_view();
    let baseline = h.probe.lock().unwrap().invocations.len();

    let target = 10.0 * STRIDE;
    assert!(h.scroll_tick(target), "the jump must re-materialize");
    {
        let p = h.probe.lock().unwrap();
        assert_eq!(p.invocations.len(), baseline + 1);
        assert_eq!(
            p.invocations[baseline],
            VirtualViewCallbackReason::ScrollBeyondContent
        );
        assert_eq!(p.first, 9, "the window is rebuilt around page 10");
    }
    let (_bounds, content_offset) = h.host_item();
    let materialized = h
        .lw
        .virtual_view_manager
        .materialized_window_origin(DomId::ROOT_ID, host)
        .expect("materialized");
    assert!(
        (content_offset.y - (materialized.y - target)).abs() < 0.01,
        "the host item composites the new window at the new offset in the same frame"
    );

    // Sitting there is quiet.
    assert!(!h.scroll_tick(target));
    assert_eq!(h.probe.lock().unwrap().invocations.len(), baseline + 1);
}
