//! D15: contenteditable INSIDE a virtual view — the paged-editor lifecycle.
//!
//! The Word-app model: the app's document lives in ITS model; a VirtualView
//! mounts only a WINDOW of pages as a nested DOM (stable nested DomId, arena
//! re-created per window). This e2e walks the full loop and pins the three
//! landmines from AZUL-STILL-TODO #15:
//!
//!   edit on page 0 (structural split, record → app-apply → ack-with-inverse)
//!   → scroll to page 40 (nested arena RE-CREATED, page 0 unmounted)
//!   → undo (re-records the inverse; its DomNodeIds point at the DEAD
//!     generation — engine must not panic, the app applies via the
//!     generation-stable resume)
//!   → re-render with page 0 still unmounted (caret restore must cleanly
//!     no-op, not corrupt)
//!   → scroll back: page 0 shows the ORIGINAL text.

use std::sync::{Arc, Mutex};

use azul_core::callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn};
use azul_core::dom::{Dom, DomId, DomNodeId, NodeId, NodeType, OptionDom};
use azul_core::geom::{LogicalPosition, LogicalSize};
use azul_core::refany::RefAny;
use azul_core::resources::RendererResources;
use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
use azul_core::styled_dom::{NodeHierarchyItemId, StyledDom};
use azul_layout::managers::changeset::{
    DocOpMergeNodes, DocOpSplitNode, DocumentOperation, NodePosition,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const PAGE_COUNT: usize = 40;

/// The APP's document model: paragraphs per page + the mounted window.
#[derive(Debug)]
struct PagedModel {
    pages: Vec<Vec<String>>,
    window: std::ops::Range<usize>,
    /// #16 signal capture: (scroll_y, viewport_h, virtual_h) from the last
    /// VirtualViewCallbackInfo the engine handed the callback.
    captured_signal: Option<(f32, f32, f32)>,
}

type SharedModel = Arc<Mutex<PagedModel>>;

fn fresh_model() -> SharedModel {
    Arc::new(Mutex::new(PagedModel {
        pages: (0..PAGE_COUNT)
            .map(|i| vec![format!("page {i} content")])
            .collect(),
        window: 0..5,
        captured_signal: None,
    }))
}

/// The VirtualView callback: renders the model's current page window.
extern "C" fn pages_view(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let model = data.downcast_ref::<SharedModel>().expect("model").clone();
    let mut model = model.lock().unwrap();
    // #16: the engine's reinvoke signal must carry document-space offsets
    // the app can feed straight into page_of_y ("user is looking at pages
    // N..M"). Capture what the engine actually delivered.
    model.captured_signal = Some((
        info.scroll_offset.y,
        info.bounds.get_logical_size().height,
        info.virtual_rect.size.height,
    ));

    let mut root = Dom::create_div().with_css("display: block;");
    for page_idx in model.window.clone() {
        let mut page = Dom::create_div().with_css("display: block; height: 100px;");
        for para in &model.pages[page_idx] {
            let mut p = Dom::create_div().with_css("display: block;");
            p.set_contenteditable(true);
            p = p.with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                para.as_str(),
            ));
            page = page.with_child(p);
        }
        root = root.with_child(page);
    }

    let window_len = model.window.len() as f32;
    VirtualViewReturn {
        dom: OptionDom::Some(root),
        materialized: azul_core::geom::LogicalRect::new(
            LogicalPosition::new(0.0, model.window.start as f32 * 100.0),
            LogicalSize::new(600.0, window_len * 100.0),
        ),
        virtual_rect: azul_core::geom::LogicalRect::new(
            LogicalPosition::zero(),
            LogicalSize::new(600.0, PAGE_COUNT as f32 * 100.0),
        ),
    }
}

fn relayout(lw: &mut LayoutWindow, model: &SharedModel) {
    let mut dom = Dom::create_body().with_child(
        Dom::create_virtual_view(
            RefAny::new(model.clone()),
            VirtualViewCallback::create(pages_view),
        )
        .with_css("width: 600px; height: 500px; overflow: hidden;"),
    );
    let (css, _) =
        azul_css::parser2::new_from_str("* { margin: 0; padding: 0; } body { font-size: 14px; }");
    let styled_dom = StyledDom::create(&mut dom, css);
    let window_state = lw.current_window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();
}

/// Find `(paragraph element, its text child)` of the FIRST paragraph whose
/// text contains `needle`, in the nested dom.
fn find_para(lw: &LayoutWindow, nested: DomId, needle: &str) -> Option<(NodeId, NodeId)> {
    let lr = lw.get_layout_result(&nested)?;
    let node_data = lr.styled_dom.node_data.as_container();
    let hierarchy = lr.styled_dom.node_hierarchy.as_container();
    for i in 0..node_data.len() {
        let id = NodeId::new(i);
        if let NodeType::Text(t) = node_data[id].get_node_type() {
            if t.as_str().contains(needle) {
                let parent = hierarchy.get(id)?.parent_id()?;
                return Some((parent, id));
            }
        }
    }
    None
}

fn dom_node(dom: DomId, node: NodeId) -> DomNodeId {
    DomNodeId {
        dom,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    }
}

fn cursor(byte: u32) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity: CursorAffinity::Leading,
    }
}

#[test]
fn edit_page_one_scroll_far_undo_reverts_without_corruption() {
    let model = fresh_model();
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state;

    // ── Mount pages 0..5.
    relayout(&mut lw, &model);
    let nested = lw
        .virtual_view_manager
        .get_nested_dom_id(DomId::ROOT_ID, NodeId::new(1))
        .expect("virtual view mounted a nested dom");
    assert!(
        lw.get_layout_result(&nested).is_some(),
        "nested dom has its own layout result"
    );

    // ── Caret into page 0's paragraph; STRUCTURAL split at "page 0 |content".
    let (para, text) = find_para(&lw, nested, "page 0 content").expect("page 0 mounted");
    lw.focus_manager
        .set_focused_node(Some(dom_node(nested, para)));
    lw.text_edit_manager
        .initialize_editing(cursor(7), nested, text, 0);

    let split_id = lw
        .record_structural_default_action(&azul_core::events::DefaultAction::SplitBlockAtCursor {
            target: dom_node(nested, para),
        })
        .expect("split records");
    let split = lw.get_pending_document_edit().unwrap().clone();
    let DocumentOperation::SplitNode(DocOpSplitNode { at, .. }) = &split.operation else {
        panic!("expected SplitNode, got {:?}", split.operation);
    };
    assert_eq!(*at, NodePosition::in_text_child(0, 7));

    // ── The APP applies the split to ITS model and acks WITH INVERSE
    //    (merge of the two halves) — the edit becomes undoable.
    {
        let mut m = model.lock().unwrap();
        m.pages[0] = vec!["page 0 ".to_string(), "content".to_string()];
    }
    let inverse = DocumentOperation::MergeNodes(DocOpMergeNodes {
        first: dom_node(nested, para),
        second: dom_node(nested, NodeId::new(para.index() + 2)),
        join: NodePosition::in_text_child(0, 7),
    });
    assert!(lw.mark_document_edit_applied_with_inverse(split_id, inverse));
    relayout(&mut lw, &model); // the app's re-render: page 0 now two paragraphs
    assert!(
        find_para(&lw, nested, "page 0 ").is_some() && find_para(&lw, nested, "content").is_some(),
        "the split rendered"
    );

    // ── Scroll far away: pages 38..42 — the nested ARENA is re-created,
    //    page 0 is unmounted (same nested DomId, new node ids).
    model.lock().unwrap().window = 38..42.min(PAGE_COUNT);
    relayout(&mut lw, &model);
    assert!(
        find_para(&lw, nested, "page 0").is_none(),
        "page 0 is unmounted"
    );
    assert!(
        find_para(&lw, nested, "page 39 content").is_some(),
        "the far window is mounted"
    );

    // ── UNDO while the edit's page is unmounted. The re-recorded inverse
    //    carries DomNodeIds of the DEAD generation — the engine must accept
    //    the record (the app resolves via the generation-stable resume) and
    //    must not panic previewing against the re-created arena.
    let undo_id = lw
        .undo_structural_edit()
        .expect("undo re-records the inverse even with the page unmounted");
    let undo_changeset = lw.get_pending_document_edit().unwrap().clone();
    assert!(
        matches!(undo_changeset.operation, DocumentOperation::MergeNodes(_)),
        "undo of a split is a merge"
    );
    assert!(
        !undo_changeset.resume.node_path.as_ref().is_empty()
            || undo_changeset.resume.position != NodePosition::before_child(u32::MAX),
        "the resume is present (generation-stable addressing for the app)"
    );

    // ── The APP applies the inverse to its model (IT knows the edit was
    //    page 0 — resume-keyed addressing), acks, re-renders. Page 0 is
    //    STILL unmounted: the caret-restore for the acked edit must cleanly
    //    no-op at the layout tail (anchor not in the mounted window).
    {
        let mut m = model.lock().unwrap();
        m.pages[0] = vec!["page 0 content".to_string()];
    }
    assert!(lw.mark_document_edit_applied(undo_id));
    relayout(&mut lw, &model);
    assert!(
        lw.get_pending_document_edit().is_none(),
        "nothing pending after the undo ack"
    );

    // ── Scroll back: page 0 shows the ORIGINAL text again.
    model.lock().unwrap().window = 0..5;
    relayout(&mut lw, &model);
    let (para_after, _) =
        find_para(&lw, nested, "page 0 content").expect("page 0 re-mounted with the reverted text");
    assert!(
        find_para(&lw, nested, "page 0 ").map(|(p, _)| p) == Some(para_after),
        "no leftover split half: the only 'page 0 ' hit is the merged paragraph"
    );

    // ── And the redo stack still works from here (the entry survived the
    //    unmount round-trip).
    let redo_id = lw.redo_structural_edit().expect("redo available");
    assert!(lw.get_pending_document_edit().is_some());
    // The app rejects it this time (re-renders without acking after the
    //    notification) — exercise the C11 drop path end-to-end.
    lw.mark_document_edit_notified();
    let _ = redo_id;
    relayout(&mut lw, &model);
    assert!(
        lw.get_pending_document_edit().is_none(),
        "a notified-but-unacked redo drops at the re-render (C11 promise)"
    );
}

/// AZUL-STILL-TODO #16: the reinvoke path hands the app a CLEAN
/// "user is now looking at pages N..M" signal — document-space scroll
/// offset + viewport + virtual extent — consumable by page_of_y math.
#[test]
fn reinvoke_signal_carries_document_space_offsets_for_page_math() {
    let model = fresh_model();
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state;

    relayout(&mut lw, &model);
    // Scroll the virtual view to y = 230 in DOCUMENT space (page height
    // 100 → the user is looking at pages 2..=(2 + viewport/100)).
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        NodeId::new(1),
        LogicalPosition::new(0.0, 230.0),
        azul_core::task::Instant::from(std::time::Instant::now()),
    );
    relayout(&mut lw, &model);

    let (scroll_y, viewport_h, virtual_h) = model
        .lock()
        .unwrap()
        .captured_signal
        .expect("the engine reinvoked the callback with an info payload");
    assert_eq!(
        virtual_h,
        PAGE_COUNT as f32 * 100.0,
        "virtual extent round-trips (the callback declared it last invoke)"
    );
    assert!(
        (scroll_y - 230.0).abs() < 0.6,
        "the reinvoke signal carries the DOCUMENT-SPACE scroll offset the          app feeds into page_of_y, got {scroll_y}"
    );
    // The app-side page math the ledger asks for:
    let first = (scroll_y / 100.0).floor() as usize;
    let last = ((scroll_y + viewport_h) / 100.0).ceil() as usize;
    assert_eq!(first, 2, "top of the viewport is on page 2");
    assert!(
        (3..=PAGE_COUNT).contains(&last),
        "bottom edge maps to a sane page: {last}"
    );
}

/// #28 (a): `SetVirtualViewGeometry` — the streaming-pagination writeback's
/// op — updates the VirtualView's VIRTUAL geometry (manager + scroll
/// bounds, i.e. the scrollbar math) WITHOUT re-invoking the callback and
/// WITHOUT touching the rendered window. This pins the apply sequence both
/// host arms execute (dll `event.rs` + the e2e runner mirror, which are
/// line-identical by the runner's port contract).
#[test]
fn set_virtual_view_geometry_updates_scrollbar_math_without_reinvoke() {
    let model = fresh_model();
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state;

    relayout(&mut lw, &model);
    let vv_node = NodeId::new(1);
    let (declared_scroll, declared_virtual) = lw
        .virtual_view_manager
        .get_declared_sizes(DomId::ROOT_ID, vv_node);
    let declared_scroll = declared_scroll.expect("VV invoked once by relayout");
    let old_virtual = declared_virtual.expect("virtual size declared");

    // Invocation probe: `pages_view` records `captured_signal` on EVERY
    // invoke — clearing it makes any re-invocation observable.
    model.lock().unwrap().captured_signal = None;

    // The op's apply sequence — line-identical in both host arms
    // (full-geometry form: Some = set, None = keep; USER design per
    // guide/en/dom/virtual-views.md).
    let new_virtual = LogicalSize::new(old_virtual.width, old_virtual.height * 4.0);
    let phase2_scroll = LogicalSize::new(declared_scroll.width, declared_scroll.height * 2.0);

    // Phase 1: virtual-only update (scroll_size None = keep).
    {
        let (kept_scroll, kept_virtual) = lw
            .virtual_view_manager
            .get_declared_sizes(DomId::ROOT_ID, vv_node);
        let eff_virtual = Some(new_virtual).or(kept_virtual).unwrap();
        let eff_scroll = None.or(kept_scroll).unwrap_or(eff_virtual);
        assert_eq!(
            eff_scroll, declared_scroll,
            "None keeps the declared window, it does not invent one"
        );
        let _ = lw.virtual_view_manager.update_virtual_view_info(
            DomId::ROOT_ID,
            vv_node,
            azul_core::geom::LogicalPosition::zero(),
            eff_scroll,
            eff_virtual,
        );
        lw.scroll_manager
            .update_virtual_scroll_bounds(DomId::ROOT_ID, vv_node, eff_virtual, None);
        lw.scroll_manager.calculate_scrollbar_states();
    }
    let (after_scroll, after_virtual) = lw
        .virtual_view_manager
        .get_declared_sizes(DomId::ROOT_ID, vv_node);
    assert_eq!(after_virtual, Some(new_virtual), "virtual extent updated");
    assert_eq!(
        after_scroll,
        Some(declared_scroll),
        "rendered window untouched by a None scroll_size"
    );

    // Phase 2: FULL geometry — the rendered window updates too.
    {
        let (kept_scroll, kept_virtual) = lw
            .virtual_view_manager
            .get_declared_sizes(DomId::ROOT_ID, vv_node);
        let eff_virtual = None.or(kept_virtual).unwrap();
        let eff_scroll = Some(phase2_scroll).or(kept_scroll).unwrap_or(eff_virtual);
        let _ = lw.virtual_view_manager.update_virtual_view_info(
            DomId::ROOT_ID,
            vv_node,
            azul_core::geom::LogicalPosition::zero(),
            eff_scroll,
            eff_virtual,
        );
        lw.scroll_manager
            .update_virtual_scroll_bounds(DomId::ROOT_ID, vv_node, eff_virtual, None);
        lw.scroll_manager.calculate_scrollbar_states();
    }
    let (after_scroll2, after_virtual2) = lw
        .virtual_view_manager
        .get_declared_sizes(DomId::ROOT_ID, vv_node);
    assert_eq!(
        after_scroll2,
        Some(phase2_scroll),
        "Some scroll_size updates the rendered window"
    );
    assert_eq!(
        after_virtual2,
        Some(new_virtual),
        "None virtual keeps the corrected extent"
    );

    // Law 2: none of it re-invoked the callback.
    assert!(
        model.lock().unwrap().captured_signal.is_none(),
        "SetVirtualViewGeometry must not re-invoke the VirtualView callback"
    );
}

/// Where the content ends up on screen, read off the display list the
/// rasteriser consumes.
fn vv_content_offset(lw: &LayoutWindow) -> Option<LogicalPosition> {
    use azul_layout::solver3::display_list::DisplayListItem;
    lw.get_layout_result(&DomId::ROOT_ID)?
        .display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::VirtualView { content_offset, .. } => Some(*content_offset),
            _ => None,
        })
}

/// The whole point of a scrollable VirtualView, pinned end to end: scrolling
/// must move the CONTENT, not just the scrollbar.
///
/// This is the user-visible half of the original bug. The callback's window
/// origin was written into a field nothing read, and a VirtualView opts out of
/// scroll frames — so the rasteriser composited the child display list at the
/// container origin with no scroll delta, forever. The thumb moved and the
/// page did not.
#[test]
fn scrolling_a_virtual_view_moves_its_content_not_only_its_scrollbar() {
    let model = fresh_model();
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state;

    relayout(&mut lw, &model);
    let vv_node = NodeId::new(1);
    let before = vv_content_offset(&lw).expect("a VirtualView item in the display list");

    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        vv_node,
        LogicalPosition::new(0.0, 230.0),
        azul_core::task::Instant::from(std::time::Instant::now()),
    );
    relayout(&mut lw, &model);

    let after = vv_content_offset(&lw).expect("a VirtualView item in the display list");
    assert_ne!(
        after, before,
        "the content offset never changed, so nothing moved on screen"
    );

    // And it moved by exactly the placement law, not by some other amount:
    // container.origin + (materialized.origin - scroll_offset).
    let origin = lw
        .virtual_view_manager
        .materialized_window_origin(DomId::ROOT_ID, vv_node)
        .expect("a materialized window after two invokes");
    let offset = lw
        .scroll_manager
        .get_current_offset(DomId::ROOT_ID, vv_node)
        .expect("a scroll offset");
    assert_eq!(
        after,
        LogicalPosition::new(origin.x - offset.x, origin.y - offset.y)
    );
}

/// The IME caret rect must be in WINDOW space, not nested-dom space.
///
/// A VirtualView's child dom builds its display list at origin zero and the
/// rasteriser composites it at `host.origin + content_offset`. Every geometry
/// accessor therefore measures as if the host sat at the window origin — and
/// all four shells hand `get_focused_cursor_rect_viewport` straight to the
/// platform IME, which places its candidate window in screen coordinates. So
/// the popup appeared at the top-left of the window instead of under the caret.
#[test]
fn the_ime_caret_rect_is_lifted_out_of_the_nested_dom() {
    let model = fresh_model();
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state;

    relayout(&mut lw, &model);
    let nested = lw
        .virtual_view_manager
        .get_nested_dom_id(DomId::ROOT_ID, NodeId::new(1))
        .expect("virtual view mounted a nested dom");

    // The root dom is already window space; nothing to lift.
    assert_eq!(
        lw.window_space_offset_of_dom(DomId::ROOT_ID),
        LogicalPosition::zero(),
        "the root dom IS window space"
    );

    // Scroll the view so the composite offset is genuinely non-zero — an
    // assertion that only ever compares zero to zero proves nothing.
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        NodeId::new(1),
        LogicalPosition::new(0.0, 230.0),
        azul_core::task::Instant::from(std::time::Instant::now()),
    );
    relayout(&mut lw, &model);

    // The nested dom is not window space: it sits wherever its host was placed
    // and wherever the rasteriser then shifts its content.
    let lifted = lw.window_space_offset_of_dom(nested);
    assert_ne!(
        lifted,
        LogicalPosition::zero(),
        "the probe must be able to read a non-zero lift, or it proves nothing"
    );
    let host_origin = lw
        .get_layout_result(&DomId::ROOT_ID)
        .and_then(|lr| lr.layout_tree.dom_to_layout.get(&NodeId::new(1)).cloned())
        .and_then(|ix| ix.first().copied())
        .and_then(|ix| {
            lw.get_layout_result(&DomId::ROOT_ID)
                .and_then(|lr| lr.calculated_positions.get(ix.index()).copied())
        })
        .expect("the host has a resolved position");
    let content = lw.virtual_view_content_offset(DomId::ROOT_ID, NodeId::new(1));

    assert_eq!(
        lifted,
        LogicalPosition::new(host_origin.x + content.x, host_origin.y + content.y),
        "the lift must be exactly where the rasteriser composites the child"
    );
}
