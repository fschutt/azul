//! Live-shape reproduction of "Enter in AzWriter does not move the caret".
//!
//! `layout/tests/vview_contenteditable_e2e.rs` proves the ENGINE pieces one
//! by one (determine → record → notify; manual apply → ack → restore). What
//! it cannot see is the SHELL: the real pipeline where the Return keypress is
//! diffed out of window state, the DocumentEdit event is dispatched to the
//! app's Focus callback, the callback's `Update::RefreshDom` regenerates the
//! DOM, and the caret is restored against the new generation. The live bug
//! sits somewhere on that path — AzWriter's log shows keystrokes arriving
//! (incremental relayouts) but `regenerate_layout` never running after Enter.
//!
//! This test drives a `HeadlessWindow` (the same `PlatformWindow` pipeline
//! the desktop shells run) against the AzWriter shape: a VirtualView
//! materializing a contenteditable page whose `DocumentEdit` callback
//! applies the changeset to the app model and acks. Every stage asserts, so
//! whichever link is broken names itself.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use azul_core::callbacks::{
    LayoutCallback, LayoutCallbackInfo, Update, VirtualViewCallback, VirtualViewCallbackInfo,
    VirtualViewReturn,
};
use azul_core::dom::{Dom, DomId, IdOrClass, IdOrClassVec, NodeType, OptionDom};
use azul_core::events::{EventFilter, FocusEventFilter, ProcessEventResult};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::icon::{IconProviderHandle, SharedIconProvider};
use azul_core::refany::RefAny;
use azul_core::resources::AppConfig;
use azul_core::window::{OptionVirtualKeyCode, VirtualKeyCode};
use azul_css::css::{CssPath, CssPathSelector};
use azul_layout::callbacks::{Callback, CallbackInfo};
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;

use azul::desktop::shell2::common::event::SharedUndoManager;
use azul::desktop::shell2::common::PlatformWindow;
use azul::desktop::shell2::headless::HeadlessWindow;

/// The app model, AzWriter-shaped: `content` is a Dom whose children are the
/// block elements (`<p>` here), and the DocumentEdit callback applies engine
/// changesets to it with `host_path = []` (the content root IS the editing
/// host, exactly like a one-page AzWriter document).
struct Model {
    content: Dom,
    edits_applied: u32,
    apply_errors: Vec<String>,
}

#[derive(Clone)]
struct Shared(Arc<Mutex<Model>>);

fn fresh_model() -> Shared {
    let content = Dom::create_div().with_child(Dom::create_p().with_child(
        Dom::create_text_do_not_use_without_block_level_wrapper("hello world"),
    ));
    Shared(Arc::new(Mutex::new(Model {
        content,
        edits_applied: 0,
        apply_errors: Vec::new(),
    })))
}

/// AzWriter's `on_document_edit`, minus pagination: read the changeset,
/// apply it to the model, remember the inverse, ack, RefreshDom.
extern "C" fn on_document_edit(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(changeset) = info.get_document_edit_clone().into_option() else {
        return Update::DoNothing;
    };
    let shared = match data.downcast_ref::<Shared>() {
        Some(s) => s.clone(),
        None => return Update::DoNothing,
    };
    let mut m = shared.0.lock().unwrap();
    let applied = match changeset
        .apply_to_dom(&mut m.content, Vec::<u32>::new().into())
        .into_result()
    {
        Ok(a) => a,
        Err(e) => {
            m.apply_errors.push(format!("{e:?}"));
            return Update::DoNothing;
        }
    };
    m.edits_applied += 1;
    drop(m);
    info.mark_document_edit_applied_with_inverse(changeset.id, applied.inverse);
    Update::RefreshDom
}

/// The VirtualView payload: the app RefAny, cloned into the page's callback
/// at materialization time — the same indirection AzWriter's `PagesVv` uses.
struct VvPayload {
    app: RefAny,
}

extern "C" fn pages_view(mut data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let app = match data.downcast_ref::<VvPayload>() {
        Some(p) => p.app.clone(),
        None => return VirtualViewReturn::default(),
    };
    let content = {
        let mut app2 = app.clone();
        let Some(shared) = app2.downcast_ref::<Shared>() else {
            return VirtualViewReturn::default();
        };
        let shared = shared.clone();
        let m = shared.0.lock().unwrap();
        m.content.clone()
    };

    // The content root is the editing host (AzWriter: the pagination clones
    // keep the model root's contenteditable) and carries the DocumentEdit
    // callback + the `.mw-doc` focus-anchor class.
    let mut page = content
        .with_callback(
            EventFilter::Focus(FocusEventFilter::DocumentEdit),
            app,
            Callback {
                cb: on_document_edit,
                ctx: azul_core::refany::OptionRefAny::None,
            }
            .to_core(),
        )
        .with_ids_and_classes(IdOrClassVec::from(vec![IdOrClass::Class("mw-doc".into())]))
        .with_css("display: block; width: 600px; min-height: 400px; background: white;");
    page.set_contenteditable(true);

    let size = LogicalSize::new(600.0, 400.0);
    VirtualViewReturn {
        dom: OptionDom::Some(page),
        materialized: LogicalRect::new(LogicalPosition::zero(), size),
        virtual_rect: LogicalRect::new(LogicalPosition::zero(), size),
    }
}

extern "C" fn layout_cb(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let app = data.clone();
    Dom::create_body().with_child(
        Dom::create_virtual_view(
            RefAny::new(VvPayload { app }),
            VirtualViewCallback::create(pages_view),
        )
        .with_css("width: 640px; height: 500px; overflow: hidden;"),
    )
}

fn make_window(shared: Shared) -> HeadlessWindow {
    let fc_cache = Arc::new(FcFontCache::build());
    let app_data = Arc::new(RefCell::new(RefAny::new(shared)));
    let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());

    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    options.window_state.layout_callback = LayoutCallback {
        cb: layout_cb,
        ctx: azul_core::refany::OptionRefAny::None,
    };

    HeadlessWindow::new(
        options,
        app_data,
        SharedUndoManager::new(),
        AppConfig::default(),
        icon_provider,
        fc_cache,
        None,
    )
    .expect("HeadlessWindow construction must succeed")
}

/// The KeyDown arm of `HeadlessWindow::run`, inlined (run() is a loop; the
/// test needs one deterministic step): snapshot → mutate keyboard state →
/// event pass → honor the result the way `service_frame` does.
fn press_key(window: &mut HeadlessWindow, vk: VirtualKeyCode) -> ProcessEventResult {
    window.snapshot_window_state_baseline("test.key_down");
    window.common.keyboard_state_mut().current_virtual_keycode = OptionVirtualKeyCode::Some(vk);
    window
        .common
        .keyboard_state_mut()
        .pressed_virtual_keycodes
        .insert_hm_item(vk);
    let down = window.process_window_events(0);
    honor(window, down);

    window.snapshot_window_state_baseline("test.key_up");
    window.common.keyboard_state_mut().current_virtual_keycode = OptionVirtualKeyCode::None;
    window
        .common
        .keyboard_state_mut()
        .pressed_virtual_keycodes
        .remove_hm_item(&vk);
    let up = window.process_window_events(0);
    honor(window, up);
    down.max(up)
}

/// `service_frame`'s routing, without the render: a regenerate-tier result
/// re-invokes the app's layout() (RefreshDom), an incremental result re-lays
/// the existing tree.
fn honor(window: &mut HeadlessWindow, tier: ProcessEventResult) {
    use azul_core::events::ProcessEventResult as R;
    if tier >= R::ShouldRegenerateDomCurrentWindow {
        window
            .regenerate_layout()
            .expect("regenerate_layout after RefreshDom");
    } else if tier == R::ShouldIncrementalRelayout {
        let _ = window.relayout_only();
    }
    // The run-loop's VirtualView drain: a pre-cascade-skip regenerate QUEUES
    // re-invocations (the parent fingerprint cannot see model changes behind
    // the view's RefAny) and the frame path drains them before painting.
    window.common.drain_virtual_view_updates();
}

/// The nested (VirtualView) dom id + the editable host node in it.
fn nested_host(window: &HeadlessWindow) -> (DomId, azul_core::id::NodeId) {
    let lw = window
        .common
        .layout_window
        .as_ref()
        .expect("layout_window exists");
    for (dom_id, lr) in &lw.layout_results {
        if *dom_id == DomId::ROOT_ID {
            continue;
        }
        let nodes = lr.styled_dom.node_data.as_container();
        for i in 0..nodes.len() {
            let nid = azul_core::id::NodeId::new(i);
            if nodes.get(nid).is_some_and(|n| n.is_contenteditable()) {
                return (*dom_id, nid);
            }
        }
    }
    panic!("no contenteditable host materialized in any nested dom");
}

/// All `<p>` text contents under the nested dom, in document order.
fn paragraph_texts(window: &HeadlessWindow, dom_id: DomId) -> Vec<String> {
    let lw = window.common.layout_window.as_ref().unwrap();
    let lr = lw.layout_results.get(&dom_id).expect("nested dom exists");
    let nodes = lr.styled_dom.node_data.as_container();
    let hierarchy = lr.styled_dom.node_hierarchy.as_container();
    let mut out = Vec::new();
    for i in 0..nodes.len() {
        let nid = azul_core::id::NodeId::new(i);
        let Some(nd) = nodes.get(nid) else { continue };
        if !matches!(nd.get_node_type(), NodeType::P) {
            continue;
        }
        let mut text = String::new();
        let mut child = hierarchy.get(nid).and_then(|h| h.first_child_id(nid));
        while let Some(c) = child {
            if let Some(cd) = nodes.get(c) {
                if let NodeType::Text(t) = cd.get_node_type() {
                    text.push_str(t.as_str());
                }
            }
            child = hierarchy
                .get(c)
                .and_then(azul_core::styled_dom::NodeHierarchyItem::next_sibling_id);
        }
        out.push(text);
    }
    out
}

#[test]
fn enter_splits_the_paragraph_and_moves_the_caret_through_the_shell_pipeline() {
    let shared = fresh_model();
    let mut window = make_window(shared.clone());

    window.regenerate_layout().expect("initial layout");
    let (nested, host) = nested_host(&window);

    // Startup focus, the AzWriter way: focus the `.mw-doc` host by path.
    // (The live app does this from its create callback; the log shows it
    // resolving to the nested dom's host node.)
    window.snapshot_window_state_baseline("test.focus");
    let r = window.apply_user_change(&azul_layout::callbacks::CallbackChange::SetFocusTarget {
        target: azul_core::callbacks::FocusTarget::Path(azul_core::callbacks::FocusTargetPath {
            dom: nested,
            css_path: CssPath {
                selectors: vec![CssPathSelector::Class("mw-doc".into())].into(),
            },
        }),
    });
    honor(&mut window, r);

    {
        let lw = window.common.layout_window.as_ref().unwrap();
        let focused = lw.focus_manager.get_focused_node().copied();
        assert_eq!(
            focused.map(|f| f.dom),
            Some(nested),
            "focus must land in the nested dom (got {focused:?})"
        );
        assert!(
            lw.text_edit_manager.get_primary_cursor().is_some(),
            "focusing a contenteditable host must seed a caret \
             (text_input_v3 'enabled for contenteditable focus' is this exact hook)"
        );
    }
    let _ = host;

    // ── Return #1: the caret sits at the end of "hello world" ────────────
    let result = press_key(&mut window, VirtualKeyCode::Return);

    {
        let m = shared.0.lock().unwrap();
        assert!(
            m.apply_errors.is_empty(),
            "changeset apply failed in the app callback: {:?}",
            m.apply_errors
        );
        assert_eq!(
            m.edits_applied, 1,
            "Return must reach the app's DocumentEdit callback exactly once \
             (0 = the split was never determined/recorded, or the DocumentEdit \
             event was never dispatched to the Focus callback; result was {result:?})"
        );
        assert_eq!(
            m.content.children.as_ref().len(),
            2,
            "the model root must hold TWO blocks after the split"
        );
    }

    // The RefreshDom re-rendered the VV: the nested dom now shows two <p>s.
    let (nested2, _) = nested_host(&window);
    let paras = paragraph_texts(&window, nested2);
    assert_eq!(
        paras.len(),
        2,
        "the re-rendered page must show two paragraphs, got {paras:?}"
    );
    assert_eq!(paras[0], "hello world", "text stays in the first paragraph");
    assert_eq!(paras[1], "", "the new paragraph is empty");

    // THE LIVE SYMPTOM: the caret must now sit in the SECOND paragraph.
    {
        let lw = window.common.layout_window.as_ref().unwrap();
        let mc = lw
            .text_edit_manager
            .multi_cursor
            .as_ref()
            .expect("a caret exists after the split was acked");
        let caret_node = mc
            .node_id
            .node
            .into_crate_internal()
            .expect("caret on a real node");
        assert_eq!(mc.node_id.dom, nested2, "caret stays in the nested dom");

        let lr = lw.layout_results.get(&nested2).unwrap();
        let hierarchy = lr.styled_dom.node_hierarchy.as_container();
        let nodes = lr.styled_dom.node_data.as_container();
        // Walk up from the caret node to its <p> ancestor.
        let mut p_of_caret = caret_node;
        while let Some(nd) = nodes.get(p_of_caret) {
            if matches!(nd.get_node_type(), NodeType::P) {
                break;
            }
            match hierarchy
                .get(p_of_caret)
                .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id)
            {
                Some(p) => p_of_caret = p,
                None => break,
            }
        }
        // Which <p> is it, in document order?
        let mut p_index = None;
        let mut seen = 0usize;
        for i in 0..nodes.len() {
            let nid = azul_core::id::NodeId::new(i);
            if nodes
                .get(nid)
                .is_some_and(|n| matches!(n.get_node_type(), NodeType::P))
            {
                if nid == p_of_caret {
                    p_index = Some(seen);
                }
                seen += 1;
            }
        }
        assert_eq!(
            p_index,
            Some(1),
            "after Enter the caret must sit in the SECOND paragraph \
             (caret node {caret_node:?}, its <p> {p_of_caret:?}) — \
             'the cursor does not reposition' is exactly this assert firing"
        );
    }

    // ── Return #2: splits the (empty) second paragraph again ─────────────
    let _ = press_key(&mut window, VirtualKeyCode::Return);
    {
        let m = shared.0.lock().unwrap();
        assert!(
            m.apply_errors.is_empty(),
            "second apply failed: {:?}",
            m.apply_errors
        );
        assert_eq!(m.edits_applied, 2, "second Return applies too");
        assert_eq!(
            m.content.children.as_ref().len(),
            3,
            "three blocks after hello world<enter><enter>"
        );
    }
}

/// Total glyph count across all Text items of one dom's display list.
fn glyph_count(window: &HeadlessWindow, dom_id: DomId) -> usize {
    use azul_layout::solver3::display_list::DisplayListItem;
    let lw = window.common.layout_window.as_ref().unwrap();
    let dl = &lw.layout_results.get(&dom_id).unwrap().display_list;
    dl.items
        .iter()
        .map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => glyphs.len(),
            _ => 0,
        })
        .sum()
}

/// Live bug: "I enter 'hello world' and scroll and then only the cursor
/// still appears." Typed-but-uncommitted text lives in `content_overlay`;
/// an EdgeScrolled re-materialization rebuilds the nested dom from the app
/// model (which does not have the keystrokes yet) — the reconcile remaps
/// the overlay entry onto the new generation, but nothing re-APPLIED it to
/// the fresh layout, so the glyphs reverted to the DOM text.
#[test]
fn typed_overlay_text_survives_a_virtual_view_rematerialization() {
    let shared = fresh_model();
    let mut window = make_window(shared.clone());
    window.regenerate_layout().expect("initial layout");
    let (nested, _host) = nested_host(&window);

    window.snapshot_window_state_baseline("test.focus");
    let r = window.apply_user_change(&azul_layout::callbacks::CallbackChange::SetFocusTarget {
        target: azul_core::callbacks::FocusTarget::Path(azul_core::callbacks::FocusTargetPath {
            dom: nested,
            css_path: CssPath {
                selectors: vec![CssPathSelector::Class("mw-doc".into())].into(),
            },
        }),
    });
    honor(&mut window, r);

    let before_typing = glyph_count(&window, nested);

    // Type through the canonical text pipeline (the platform IME path).
    window.snapshot_window_state_baseline("test.text_input");
    let r = window.apply_user_change(&azul_layout::callbacks::CallbackChange::CreateTextInput {
        text: "xyz".into(),
    });
    honor(&mut window, r);

    let after_typing = glyph_count(&window, nested);
    assert!(
        after_typing > before_typing,
        "typing must add glyphs to the nested display list \
         (before {before_typing}, after {after_typing})"
    );

    // The scroll-shaped re-invoke: EdgeScrolled queues exactly this.
    window
        .common
        .layout_window
        .as_mut()
        .unwrap()
        .queue_all_virtual_view_reinvoke();
    window.common.drain_virtual_view_updates();

    let after_scroll = glyph_count(&window, nested);
    assert_eq!(
        after_scroll, after_typing,
        "a re-materialization must not lose the typed (overlay) text — \
         dropping back to {before_typing} is the 'scrolling clears what I typed' bug"
    );
}
