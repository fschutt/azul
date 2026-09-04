//! `TextChanged` — the post-commit text event — through the shell pipeline,
//! from the paths the EVENT PASS never sees.
//!
//! X11 and Wayland type through `text_input_manager.record_input` inside the
//! event pass, so a drain at that pass's tail would cover them. Everything
//! else commits text somewhere else: the headless shell and macOS deliver
//! typed characters as a direct `apply_user_change(CreateTextInput)` outside
//! any pass, the E2E harness's `text_input` op and any app timer commit from
//! the TIMER pass, thread writebacks likewise. The law is that notifications
//! are drained wherever a pass ends (exactly like
//! `arm_animation_drivers_if_needed`), so this test drives those shapes
//! through `HeadlessWindow` — the real `PlatformWindow` pipeline — and
//! asserts what a live word count needs:
//!
//! 1. `TextChanged` reaches the focused host's Focus callback from the timer
//!    pass, AFTER the commit, with the new text visible through
//!    `get_unsynced_text_edits`.
//! 2. A callback that acks (`mark_text_revision_synced`) and returns
//!    `Update::DoNothing` — the live-label shape, no re-render — is safe: the
//!    committed text survives the next relayout and is NOT re-committed (no
//!    second `TextChanged`, revision stable). Only the app's own re-render
//!    retires it, at which point the app's model is the truth.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use azul_core::callbacks::{
    FocusTarget, FocusTargetPath, LayoutCallback, LayoutCallbackInfo, TimerCallbackReturn, Update,
};
use azul_core::dom::{Dom, DomId, IdOrClass, IdOrClassVec, NodeType};
use azul_core::events::{EventFilter, FocusEventFilter, ProcessEventResult};
use azul_core::geom::LogicalSize;
use azul_core::icon::{IconProviderHandle, SharedIconProvider};
use azul_core::id::NodeId;
use azul_core::refany::{OptionRefAny, RefAny};
use azul_core::resources::AppConfig;
use azul_core::task::TerminateTimer;
use azul_css::css::{CssPath, CssPathSelector};
use azul_layout::callbacks::{Callback, CallbackChange, CallbackInfo, ExternalSystemCallbacks};
use azul_layout::timer::{Timer, TimerCallbackInfo, TimerCallbackType};
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;

use azul::desktop::shell2::common::event::SharedUndoManager;
use azul::desktop::shell2::common::PlatformWindow;
use azul::desktop::shell2::headless::HeadlessWindow;

/// What the app's `TextChanged` callback observed.
#[derive(Default)]
struct Seen {
    /// Every unsynced edit the callback was handed, as (text, revision), in
    /// firing order.
    edits: Vec<(String, u64)>,
    /// How often `TextChanged` reached the callback.
    fired: u32,
}

#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Seen>>);

/// The live word-count shape: read the edits, remember them, ack, and do NOT
/// re-render — the label is patched in place elsewhere.
extern "C" fn on_text_changed(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let shared = match data.downcast_ref::<Shared>() {
        Some(s) => s.clone(),
        None => return Update::DoNothing,
    };
    let edits = info.get_unsynced_text_edits();
    let mut newest = 0u64;
    {
        let mut seen = shared.0.lock().unwrap();
        seen.fired += 1;
        for edit in edits.as_ref() {
            seen.edits
                .push((edit.text.as_str().to_string(), edit.revision));
            newest = newest.max(edit.revision);
        }
    }
    if newest > 0 {
        info.mark_text_revision_synced(newest);
    }
    Update::DoNothing
}

/// An app timer committing text — the E2E harness's `text_input` op is this
/// exact shape.
extern "C" fn type_world(_data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    info.callback_info.create_text_input(" world".into());
    TimerCallbackReturn {
        should_update: Update::DoNothing,
        should_terminate: TerminateTimer::Terminate,
    }
}

/// body > div.mw-doc[contenteditable] > p > "hello"; the host carries the
/// `TextChanged` callback. The model never changes: a re-render always says
/// "hello" again, which is what lets the last test tell overlay text from
/// model text.
extern "C" fn layout_cb(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let app = data.clone();
    let mut host = Dom::create_div()
        .with_child(Dom::create_p().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("hello"),
        ))
        .with_callback(
            EventFilter::Focus(FocusEventFilter::TextChanged),
            app,
            Callback {
                cb: on_text_changed,
                ctx: OptionRefAny::None,
            }
            .to_core(),
        )
        .with_ids_and_classes(IdOrClassVec::from(vec![IdOrClass::Class("mw-doc".into())]))
        .with_css("display: block; width: 600px; min-height: 400px;");
    host.set_contenteditable(true);
    Dom::create_body().with_child(host)
}

fn make_window(shared: Shared) -> HeadlessWindow {
    let fc_cache = Arc::new(FcFontCache::build());
    let app_data = Arc::new(RefCell::new(RefAny::new(shared)));
    let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());

    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    options.window_state.layout_callback = LayoutCallback {
        cb: layout_cb,
        ctx: OptionRefAny::None,
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

/// `service_frame`'s routing, without the render.
fn honor(window: &mut HeadlessWindow, tier: ProcessEventResult) {
    use azul_core::events::ProcessEventResult as R;
    if tier >= R::ShouldRegenerateDomCurrentWindow {
        window
            .regenerate_layout()
            .expect("regenerate_layout after RefreshDom");
    } else if tier == R::ShouldIncrementalRelayout {
        let _ = window.relayout_only();
    }
    window.common.drain_virtual_view_updates();
}

/// The editable host and its `<p>` in the root dom.
fn host_and_paragraph(window: &HeadlessWindow) -> (NodeId, NodeId) {
    let lw = window.common.layout_window.as_ref().expect("layout_window");
    let lr = lw.layout_results.get(&DomId::ROOT_ID).expect("root dom");
    let nodes = lr.styled_dom.node_data.as_container();
    let (mut host, mut paragraph) = (None, None);
    for i in 0..nodes.len() {
        let nid = NodeId::new(i);
        let Some(nd) = nodes.get(nid) else { continue };
        if nd.is_contenteditable() {
            host = Some(nid);
        } else if matches!(nd.get_node_type(), NodeType::P) {
            paragraph = Some(nid);
        }
    }
    (host.expect("a contenteditable host"), paragraph.expect("a <p>"))
}

/// The paragraph's text as the engine reads it for the next edit: overlay
/// first, then the committed tree.
fn paragraph_text(window: &HeadlessWindow, paragraph: NodeId) -> String {
    let lw = window.common.layout_window.as_ref().expect("layout_window");
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, paragraph);
    lw.extract_text_from_inline_content(&content)
}

/// Startup focus, the AzWriter way: the `.mw-doc` host by path. Focusing a
/// contenteditable host seeds the caret at the END of its text.
fn focus_host(window: &mut HeadlessWindow) {
    window.snapshot_window_state_baseline("test.focus");
    let r = window.apply_user_change(&CallbackChange::SetFocusTarget {
        target: FocusTarget::Path(FocusTargetPath {
            dom: DomId::ROOT_ID,
            css_path: CssPath {
                selectors: vec![CssPathSelector::Class("mw-doc".into())].into(),
            },
        }),
    });
    honor(window, r);
    let lw = window.common.layout_window.as_ref().unwrap();
    assert!(
        lw.focus_manager.get_focused_node().is_some(),
        "the host must take focus"
    );
    assert!(
        lw.text_edit_manager.get_primary_cursor().is_some(),
        "focusing a contenteditable host must seed a caret"
    );
}

fn world_timer() -> Timer {
    Timer::create(
        RefAny::new(()),
        type_world as TimerCallbackType,
        ExternalSystemCallbacks::rust_internal().get_system_time_fn,
    )
}

#[test]
fn text_changed_fires_from_the_timer_pass_after_the_commit() {
    let shared = Shared::default();
    let mut window = make_window(shared.clone());
    window.regenerate_layout().expect("initial layout");
    let (_host, paragraph) = host_and_paragraph(&window);
    assert_eq!(paragraph_text(&window, paragraph), "hello", "premise");
    focus_host(&mut window);

    // A delay-less timer runs on the first timer pass.
    window.start_timer(4242, world_timer());
    window.snapshot_window_state_baseline("test.timer");
    let _ = window.process_timers_and_threads();

    {
        let seen = shared.0.lock().unwrap();
        assert_eq!(
            seen.fired, 1,
            "TextChanged must reach the host's Focus callback exactly once from \
             the timer pass (0 = the commit never queued a notification, or the \
             timer pass does not drain; 2 = drained twice)"
        );
        assert_eq!(
            seen.edits,
            vec![("hello world".to_string(), 1)],
            "the callback runs POST-commit: it sees the committed text and its \
             revision through get_unsynced_text_edits"
        );
    }
    assert_eq!(
        paragraph_text(&window, paragraph),
        "hello world",
        "the engine's own read of the paragraph agrees with the notification"
    );
    let lw = window.common.layout_window.as_ref().unwrap();
    assert_eq!(lw.document_text_revision, 1);
    assert_eq!(
        lw.acked_text_revision, 1,
        "the callback's ack landed (mark_text_revision_synced from inside a \
         TextChanged callback that did not re-render)"
    );
    assert!(
        lw.unsynced_text_edits().is_empty(),
        "nothing is left unsynced after the ack"
    );
}

#[test]
fn text_changed_fires_for_a_direct_create_text_input_outside_any_pass() {
    // The headless shell and macOS deliver typed characters this way: a bare
    // `apply_user_change(CreateTextInput)` from the platform layer, followed
    // by the frame loop's `process_timers_and_threads`.
    let shared = Shared::default();
    let mut window = make_window(shared.clone());
    window.regenerate_layout().expect("initial layout");
    let (_host, paragraph) = host_and_paragraph(&window);
    focus_host(&mut window);

    window.snapshot_window_state_baseline("test.type");
    let r = window.apply_user_change(&CallbackChange::CreateTextInput {
        text: " world".into(),
    });
    honor(&mut window, r);
    let _ = window.process_timers_and_threads();

    let seen = shared.0.lock().unwrap();
    assert_eq!(
        seen.fired, 1,
        "exactly one TextChanged for one committed character run"
    );
    assert_eq!(seen.edits, vec![("hello world".to_string(), 1)]);
    drop(seen);
    assert_eq!(paragraph_text(&window, paragraph), "hello world");
}

#[test]
fn an_ack_without_a_re_render_survives_the_next_relayout_and_is_not_re_committed() {
    let shared = Shared::default();
    let mut window = make_window(shared.clone());
    window.regenerate_layout().expect("initial layout");
    let (_host, paragraph) = host_and_paragraph(&window);
    focus_host(&mut window);
    window.start_timer(4242, world_timer());
    window.snapshot_window_state_baseline("test.timer");
    let _ = window.process_timers_and_threads();
    assert_eq!(shared.0.lock().unwrap().fired, 1, "premise: one commit, one event");

    // A relayout of the SAME generation (a resize, a scroll-driven relayout,
    // a VirtualView materialization) re-LANDS the overlay text; it must not
    // re-commit it, and the acked text must not be retired by it either.
    let _ = window.relayout_only();
    window.common.drain_virtual_view_updates();
    assert_eq!(
        paragraph_text(&window, paragraph),
        "hello world",
        "acked overlay text survives a relayout (the app has not re-rendered)"
    );
    {
        let lw = window.common.layout_window.as_ref().unwrap();
        assert_eq!(
            lw.document_text_revision, 1,
            "re-landing the overlay is not a commit: the revision is stable"
        );
    }

    // Neither pass kind finds anything to notify.
    window.snapshot_window_state_baseline("test.idle-timers");
    let _ = window.process_timers_and_threads();
    window.snapshot_window_state_baseline("test.idle-events");
    let r = window.process_window_events(0);
    honor(&mut window, r);
    assert_eq!(
        shared.0.lock().unwrap().fired,
        1,
        "a relayout must not fire a second TextChanged"
    );

    // Only the app's own re-render retires acked text: the model (which this
    // app never updated) is the truth again.
    window.regenerate_layout().expect("re-render");
    window.common.drain_virtual_view_updates();
    assert_eq!(
        paragraph_text(&window, paragraph),
        "hello",
        "the new generation shows the app's model; the acked overlay entry is \
         retired at the generation swap, not before"
    );
    assert_eq!(
        shared.0.lock().unwrap().fired,
        1,
        "a re-render is not a text commit either"
    );
}
