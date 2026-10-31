//! "Document modified, save?" lives in ONE place: the DOM event
//! `EventFilter::Window(WindowEventFilter::CloseRequested)`, vetoed with
//! `CallbackInfo::prevent_window_close()` (or the `CloseGuard` widget around
//! the content). This proves, on the headless backend, that EVERY way a
//! window is asked to close goes through that event before the window goes:
//!
//! - the window manager's close (`HeadlessEvent::Close`, the headless twin of
//!   the title-bar X / Alt+F4 / WM_CLOSE / WM_DELETE_WINDOW);
//! - the app's own `CallbackInfo::close_window()` (`CallbackChange::CloseWindow`);
//! - the CSD titlebar's close button (`csd_close` queues a window state with
//!   `flags.close_requested` raised);
//!
//! that a veto keeps the window open AND keeps what the vetoing callback
//! changed (the title), that an unvetoed close closes after exactly one
//! `CloseRequested`, and that the event reaches the DOM the app's last
//! callback asked for (a save finished on a thread: `dirty = false` +
//! `close_window` + `RefreshDom` in one writeback must not be held by the
//! stale DOM's "unsaved" guard).
//!
//! The desktop backends share the protocol (`PlatformWindow::request_window_close`
//! and `confirm_app_close`), so what this proves for headless is the shared half.

use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    },
};

use azul::desktop::shell2::{
    common::event::PlatformWindow,
    headless::{HeadlessEvent, HeadlessWindow},
};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo, Update},
    dom::Dom,
    events::{EventFilter, WindowEventFilter},
    icon::{IconProviderHandle, SharedIconProvider},
    refany::{OptionRefAny, RefAny},
    resources::AppConfig,
};
use azul_css::AzString;
use azul_layout::{
    callbacks::{Callback, CallbackChange, CallbackInfo},
    widgets::close_guard::{CloseGuard, CloseGuardEvent, CloseGuardEventKind, CloseGuardOnEventCallbackType},
    window_state::WindowCreateOptions,
};
use rust_fontconfig::FcFontCache;

/// The title a vetoing callback sets in the same callback as its veto.
const KEPT_TITLE: &str = "Report (unsaved) - still open";

/// What the app shares with its callbacks.
#[derive(Clone, Default)]
struct Probe {
    /// `CloseRequested` callbacks run.
    asked: Arc<AtomicU32>,
    /// The document has unsaved work: the layout wires the vetoing
    /// `CloseRequested` callback only then (like `CloseGuard`).
    dirty: Arc<AtomicBool>,
    /// The `CloseRequested` callback vetoes (when wired).
    veto: Arc<AtomicBool>,
    /// What a `CloseGuard` reported.
    guard_events: Arc<Mutex<Vec<CloseGuardEventKind>>>,
    /// Lay the content out inside a `CloseGuard` instead.
    use_guard: Arc<AtomicBool>,
}

extern "C" fn on_close_requested(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(probe) = data.downcast_ref::<Probe>().map(|p| Probe::clone(&p)) else {
        return Update::DoNothing;
    };
    probe.asked.fetch_add(1, Ordering::SeqCst);
    if probe.veto.load(Ordering::SeqCst) {
        let mut state = info.get_current_window_state().clone();
        state.title = AzString::from(KEPT_TITLE);
        info.modify_window_state(state);
        info.prevent_window_close();
    }
    Update::DoNothing
}

extern "C" fn on_guard_event(mut data: RefAny, _info: CallbackInfo, event: CloseGuardEvent) -> Update {
    if let Some(probe) = data.downcast_ref::<Probe>() {
        probe.guard_events.lock().unwrap().push(event.kind);
    }
    Update::RefreshDom
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let Some(probe) = data.downcast_ref::<Probe>().map(|p| Probe::clone(&p)) else {
        return Dom::create_body();
    };
    let content = Dom::create_div().with_child(Dom::create_p_with_text("the document"));
    if probe.use_guard.load(Ordering::SeqCst) {
        let guard = CloseGuard::create(content, AzString::from("Report"))
            .with_dirty(probe.dirty.load(Ordering::SeqCst))
            .with_on_event(
                RefAny::new(probe.clone()),
                on_guard_event as CloseGuardOnEventCallbackType,
            );
        return Dom::create_body().with_child(guard.dom());
    }
    let mut body = Dom::create_body().with_child(content);
    if probe.dirty.load(Ordering::SeqCst) {
        body = body.with_child(Dom::create_div().with_callback(
            EventFilter::Window(WindowEventFilter::CloseRequested),
            RefAny::new(probe.clone()),
            Callback {
                cb: on_close_requested,
                ctx: OptionRefAny::None,
            }
            .to_core(),
        ));
    }
    body
}

fn window_with(probe: &Probe) -> HeadlessWindow {
    let mut options = WindowCreateOptions::default();
    options.window_state.title = AzString::from("Report");
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut window = HeadlessWindow::new(
        options,
        Arc::new(RefCell::new(RefAny::new(probe.clone()))),
        azul::desktop::shell2::common::event::SharedUndoManager::new(),
        AppConfig::default(),
        SharedIconProvider::from_handle(IconProviderHandle::default()),
        Arc::new(FcFontCache::default()),
        None,
    )
    .expect("HeadlessWindow construction must succeed");
    window.regenerate_layout().expect("the first layout");
    let _ = window.common.take_regeneration();
    window
}

/// A window with unsaved work whose `CloseRequested` callback vetoes.
fn dirty_window_that_vetoes() -> (Probe, HeadlessWindow) {
    let probe = Probe::default();
    probe.dirty.store(true, Ordering::SeqCst);
    probe.veto.store(true, Ordering::SeqCst);
    let window = window_with(&probe);
    (probe, window)
}

fn assert_held_open(probe: &Probe, window: &HeadlessWindow, how: &str) {
    assert_eq!(
        probe.asked.load(Ordering::SeqCst),
        1,
        "{how}: CloseRequested ran exactly once before anything closed"
    );
    assert!(window.is_open(), "{how}: the veto keeps the window open");
    let state = window.get_current_window_state();
    assert!(
        !state.flags.close_requested,
        "{how}: nothing is left closing after a veto"
    );
    assert_eq!(
        state.title.as_str(),
        KEPT_TITLE,
        "{how}: the veto keeps what the vetoing callback changed"
    );
}

#[test]
fn the_window_managers_close_asks_close_requested_and_a_veto_keeps_the_window_open() {
    let (probe, mut window) = dirty_window_that_vetoes();
    window.inject_event(HeadlessEvent::Close);
    window.pump_once(false);
    assert_held_open(&probe, &window, "HeadlessEvent::Close");
}

#[test]
fn close_window_from_app_code_asks_close_requested_too() {
    let (probe, mut window) = dirty_window_that_vetoes();
    // What `CallbackInfo::close_window()` queues, applied as the engine
    // applies a callback's changes.
    let _ = window.apply_user_change(&CallbackChange::CloseWindow);
    window.pump_once(false);
    assert_held_open(&probe, &window, "CallbackInfo::close_window");
}

#[test]
fn the_titlebars_close_button_asks_close_requested_too() {
    let (probe, mut window) = dirty_window_that_vetoes();
    // What the CSD titlebar's close button (`titlebar::callbacks::csd_close`)
    // queues: the current state with `flags.close_requested` raised.
    let mut state = window.get_current_window_state().clone();
    state.flags.close_requested = true;
    let _ = window.apply_user_change(&CallbackChange::ModifyWindowState { state });
    window.pump_once(false);
    assert_held_open(&probe, &window, "the titlebar's close button");
}

#[test]
fn a_close_nobody_vetoes_closes_after_one_close_requested() {
    for how in ["window manager", "close_window"] {
        let probe = Probe::default();
        probe.dirty.store(true, Ordering::SeqCst);
        let mut window = window_with(&probe);
        if how == "window manager" {
            window.inject_event(HeadlessEvent::Close);
        } else {
            let _ = window.apply_user_change(&CallbackChange::CloseWindow);
        }
        window.pump_once(false);
        assert_eq!(probe.asked.load(Ordering::SeqCst), 1, "{how}");
        assert!(!window.is_open(), "{how}: nobody vetoed, so the window closed");
    }
}

#[test]
fn a_clean_window_closes_at_once() {
    let probe = Probe::default();
    let mut window = window_with(&probe);
    window.inject_event(HeadlessEvent::Close);
    window.pump_once(false);
    assert_eq!(probe.asked.load(Ordering::SeqCst), 0, "no listener in a clean DOM");
    assert!(!window.is_open());
}

#[test]
fn close_requested_reaches_the_dom_the_apps_last_callback_asked_for() {
    // The save ran on a thread; its writeback marks the document saved,
    // asks for a new DOM and closes - in ONE callback. The DOM on screen
    // still carries the "unsaved" veto; the close must be judged by the DOM
    // the writeback asked for, or the user is asked again after saving.
    let (probe, mut window) = dirty_window_that_vetoes();
    probe.dirty.store(false, Ordering::SeqCst);
    window
        .common
        .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
    let _ = window.apply_user_change(&CallbackChange::CloseWindow);
    window.pump_once(false);
    assert_eq!(
        probe.asked.load(Ordering::SeqCst),
        0,
        "the stale DOM's veto must not answer for the saved document"
    );
    assert!(!window.is_open(), "the saved document's window closes");
}

#[test]
fn the_close_guard_holds_the_window_managers_close_and_asks() {
    let probe = Probe::default();
    probe.use_guard.store(true, Ordering::SeqCst);
    probe.dirty.store(true, Ordering::SeqCst);
    let mut window = window_with(&probe);

    window.inject_event(HeadlessEvent::Close);
    window.pump_once(false);

    assert!(window.is_open(), "CloseGuard vetoed the close of unsaved work");
    assert!(!window.get_current_window_state().flags.close_requested);
    assert_eq!(
        *probe.guard_events.lock().unwrap(),
        vec![CloseGuardEventKind::Ask],
        "the app heard Ask, once"
    );

    // The same guard holds the app's own close.
    let _ = window.apply_user_change(&CallbackChange::CloseWindow);
    window.pump_once(false);
    assert!(window.is_open());
    assert_eq!(
        *probe.guard_events.lock().unwrap(),
        vec![CloseGuardEventKind::Ask, CloseGuardEventKind::Ask]
    );

    // Saved: the guard lets the next close through.
    probe.dirty.store(false, Ordering::SeqCst);
    window
        .common
        .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
    window.inject_event(HeadlessEvent::Close);
    window.pump_once(false);
    assert!(!window.is_open(), "a clean document closes at once");
}
