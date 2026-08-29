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

use azul_core::callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn};
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
