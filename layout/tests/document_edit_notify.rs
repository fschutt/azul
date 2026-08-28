//! C11: push notification for pending structural edits + the documented
//! drop-unacked promise.
//!
//! - `LayoutWindow::document_edit_event_provider` emits ONE
//!   `EventType::DocumentEdit` per recorded changeset (the app's apply loop
//!   is prompt and race-free instead of polling on its next callback).
//! - Once the notification is DELIVERED (`mark_document_edit_notified`), an
//!   app re-render without an ack REJECTS the edit: the next
//!   `layout_and_generate_display_list` drops it with a warning — honoring
//!   the promise documented on `pending_document_edit` since day one.
//! - The preview-materializing relayout right after `record_document_edit`
//!   must NOT drop (notification not yet delivered at that point).

use azul_core::dom::{Dom, DomId, IdOrClass, NodeId};
use azul_core::events::{EventData, EventProvider, EventType};
use azul_core::geom::LogicalSize;
use azul_core::resources::RendererResources;
use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
use azul_core::styled_dom::StyledDom;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn cursor(byte: u32) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity: CursorAffinity::Leading,
    }
}

fn three_paragraph_dom() -> StyledDom {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        body { font-size: 14px; width: 600px; }
        .p { display: block; }
    "#;
    let class =
        |name: &str| -> azul_core::dom::IdOrClassVec { vec![IdOrClass::Class(name.into())].into() };
    let mut dom = Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "first paragraph",
                )),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "second paragraph",
                )),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "third paragraph",
                )),
        );
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    StyledDom::create(&mut dom, css)
}

fn relayout(lw: &mut LayoutWindow) {
    let window_state = lw.current_window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    lw.layout_and_generate_display_list(
        three_paragraph_dom(),
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();
}

/// A window with a laid-out document and ONE recorded structural edit
/// (cross-block delete P1@6 → P2@6 = trim + merge, records one changeset).
fn window_with_pending_edit() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state;
    relayout(&mut lw);

    let ok = lw.set_cross_block_selection(
        DomId::ROOT_ID,
        NodeId::new(1),
        cursor(6),
        NodeId::new(3),
        cursor(6),
    );
    assert!(ok, "selection must be accepted");
    lw.delete_cross_block_selection()
        .expect("cross-block delete records a changeset");
    assert!(
        lw.get_pending_document_edit().is_some(),
        "delete_cross_block_selection leaves a pending changeset"
    );
    lw
}

fn now() -> azul_core::task::Instant {
    azul_core::task::Instant::from(std::time::Instant::now())
}

#[test]
fn provider_emits_exactly_one_document_edit_event_per_changeset() {
    let mut lw = window_with_pending_edit();
    let pending_id = lw.get_pending_document_edit().unwrap().id;
    let target = lw.get_pending_document_edit().unwrap().target;

    // First pass: the notification fires, aimed at the changeset's node.
    let events = lw.document_edit_event_provider().get_pending_events(now());
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].event_type, EventType::DocumentEdit);
    assert_eq!(events[0].target, target);
    match &events[0].data {
        EventData::DocumentEdit(d) => assert_eq!(d.changeset_id, pending_id),
        other => panic!("expected DocumentEdit data, got {other:?}"),
    }

    // Delivered → later passes stay silent for the SAME changeset.
    lw.mark_document_edit_notified();
    let events = lw.document_edit_event_provider().get_pending_events(now());
    assert!(
        events.is_empty(),
        "one notification per changeset, got {events:?}"
    );
}

#[test]
fn a_new_changeset_notifies_again() {
    let mut lw = window_with_pending_edit();
    lw.mark_document_edit_notified();
    assert!(lw
        .document_edit_event_provider()
        .get_pending_events(now())
        .is_empty());

    // Recording a REPLACEMENT changeset (same one-pending-slot model) resets
    // the delivered flag: the new edit gets its own notification.
    let replacement = {
        let cur = lw.get_pending_document_edit().unwrap().clone();
        azul_layout::managers::changeset::DocumentChangeset::new(
            cur.target,
            cur.operation.clone(),
            cur.resume.clone(),
            now(),
        )
    };
    let new_id = lw.record_document_edit(replacement);
    let events = lw.document_edit_event_provider().get_pending_events(now());
    assert_eq!(events.len(), 1);
    match &events[0].data {
        EventData::DocumentEdit(d) => assert_eq!(d.changeset_id, new_id),
        other => panic!("expected DocumentEdit data, got {other:?}"),
    }
}

#[test]
fn unnotified_pending_edit_survives_the_preview_relayout() {
    // The relayout right after record is what MATERIALIZES the preview — it
    // must never be treated as the app's rejection (the app hasn't even been
    // notified yet).
    let mut lw = window_with_pending_edit();
    let id = lw.get_pending_document_edit().unwrap().id;
    relayout(&mut lw);
    assert_eq!(
        lw.get_pending_document_edit().map(|c| c.id),
        Some(id),
        "un-notified edit survives the preview relayout"
    );
}

#[test]
fn notified_pending_edit_is_dropped_at_the_next_re_render() {
    let mut lw = window_with_pending_edit();
    lw.mark_document_edit_notified();
    relayout(&mut lw);
    assert!(
        lw.get_pending_document_edit().is_none(),
        "the app was notified and re-rendered without acking: the edit is \
         rejected and dropped (the documented promise)"
    );
}

#[test]
fn acked_edit_is_cleared_and_nothing_is_dropped_later() {
    let mut lw = window_with_pending_edit();
    lw.mark_document_edit_notified();
    let id = lw.get_pending_document_edit().unwrap().id;
    assert!(
        lw.mark_document_edit_applied(id),
        "handshake ids must match"
    );
    assert!(lw.get_pending_document_edit().is_none());
    // The post-ack re-render is the APPLY path, not a rejection.
    relayout(&mut lw);
    assert!(lw.get_pending_document_edit().is_none());
}
