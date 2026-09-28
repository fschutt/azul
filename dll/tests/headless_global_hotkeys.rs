//! Global hotkeys declared from `layout()`, end to end on a headless window.
//!
//! The app's `layout()` declares Ctrl+Alt+K while its state says so
//! (`LayoutCallbackInfo::add_global_hotkey`), exactly like it would attach
//! a `with_callback`. After every pass the engine brings the OS grabs in
//! line with what was declared - here against a recording backend, whose
//! log is read back:
//!
//! - the first layout grabs what it declares, and a window built while the app runs joins the
//!   app's manager;
//! - a second identical pass touches nothing at the OS;
//! - a refresh that stops declaring releases;
//! - a sync with no new layout keeps the last declaration;
//! - dropping the window releases what only it declared;
//! - a pass that reads a status costs exactly ONE extra layout.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{Arc, Mutex, PoisonError},
};

use azul::desktop::shell2::{common::event::PlatformWindow, headless::HeadlessWindow};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo, RelayoutReason, Update},
    dom::Dom,
    global_hotkey::{GlobalHotkey, GlobalHotkeyError, GlobalHotkeyId, GlobalHotkeyStatus, HotkeyModifiers},
    icon::{IconProviderHandle, SharedIconProvider},
    refany::RefAny,
    resources::AppConfig,
    window::VirtualKeyCode,
};
use azul_layout::{
    callbacks::{Callback, CallbackInfo},
    managers::global_hotkey::{BackendGrant, GlobalHotkeyBackend, SharedGlobalHotkeys},
    window_state::WindowCreateOptions,
};
use rust_fontconfig::FcFontCache;

fn summon() -> GlobalHotkey {
    GlobalHotkey {
        modifiers: HotkeyModifiers {
            ctrl: true,
            alt: true,
            shift: false,
            meta: false,
        },
        key: VirtualKeyCode::K,
    }
}

/// The backend's calls, per test.
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<String>>>);

impl Log {
    fn push(&self, entry: String) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(entry);
    }

    /// Everything since the last call, without the batch `commit`s.
    fn grabs(&self) -> Vec<String> {
        core::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
            .into_iter()
            .filter(|e| e != "commit")
            .collect()
    }
}

struct RecordingBackend {
    log: Log,
    ids: BTreeMap<u32, GlobalHotkey>,
}

impl GlobalHotkeyBackend for RecordingBackend {
    fn name(&self) -> &'static str {
        "recording"
    }

    fn probe(&self) -> Result<(), String> {
        Ok(())
    }

    fn register(
        &mut self,
        os_id: GlobalHotkeyId,
        hotkey: &GlobalHotkey,
        _description: &str,
    ) -> Result<BackendGrant, GlobalHotkeyError> {
        self.log
            .push(format!("register {}", hotkey.to_display_string_for(false)));
        self.ids.insert(os_id.id, *hotkey);
        Ok(BackendGrant::Active)
    }

    fn unregister(&mut self, os_id: GlobalHotkeyId) {
        let released = self.ids.remove(&os_id.id);
        self.log.push(format!(
            "unregister {}",
            released.map_or_else(|| "?".to_string(), |h| h.to_display_string_for(false))
        ));
    }

    fn commit(&mut self) {
        self.log.push("commit".to_string());
    }
}

/// An app's manager with a recording backend, and the log.
fn app() -> (SharedGlobalHotkeys, Log) {
    let shared = SharedGlobalHotkeys::new();
    let log = Log::default();
    shared.install_backend(Box::new(RecordingBackend {
        log: log.clone(),
        ids: BTreeMap::new(),
    }));
    (shared, log)
}

/// The app state the hotkey is derived from.
#[derive(Default)]
struct HotkeyState {
    enabled: bool,
    read_status: bool,
    layouts: usize,
    seen: Vec<GlobalHotkeyStatus>,
}

extern "C" fn on_summon(_data: RefAny, _info: CallbackInfo) -> Update {
    Update::DoNothing
}

extern "C" fn declaring_layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // The declaration keeps its own handle on the state, taken before the
    // state is borrowed.
    let handle = data.clone();
    let (enabled, read_status) = match data.downcast_mut::<HotkeyState>() {
        Some(mut s) => {
            s.layouts += 1;
            (s.enabled, s.read_status)
        }
        None => return Dom::create_body(),
    };
    if enabled {
        info.add_global_hotkey(summon(), handle, Callback::from_ptr(on_summon));
    }
    if read_status {
        let status = info.get_global_hotkey_status(summon());
        if let Some(mut s) = data.downcast_mut::<HotkeyState>() {
            s.seen.push(status);
        }
    }
    Dom::create_div()
}

fn make_window(state: HotkeyState) -> HeadlessWindow {
    let fc_cache = Arc::new(FcFontCache::default());
    let app_data = Arc::new(RefCell::new(RefAny::new(state)));
    let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());
    let mut options = WindowCreateOptions::default();
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = declaring_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    HeadlessWindow::new(
        options,
        app_data,
        azul::desktop::shell2::common::event::SharedUndoManager::new(),
        AppConfig::default(),
        icon_provider,
        fc_cache,
        None,
    )
    .expect("HeadlessWindow construction must succeed")
}

fn with_state<R>(window: &HeadlessWindow, f: impl FnOnce(&mut HotkeyState) -> R) -> R {
    let app_data = window.get_app_data();
    let mut app_data = app_data.borrow_mut();
    let mut state = app_data
        .downcast_mut::<HotkeyState>()
        .expect("the window's data is the HotkeyState");
    f(&mut state)
}

#[test]
fn the_first_layout_grabs_what_it_declares() {
    let (shared, log) = app();
    let _app = shared.enter();
    let mut window = make_window(HotkeyState {
        enabled: true,
        ..HotkeyState::default()
    });
    let joined = window
        .common
        .layout_window
        .as_ref()
        .expect("a layout window")
        .global_hotkeys
        .shared()
        .ptr_eq(&shared);
    assert!(joined, "a window built while the app runs joins its manager");

    window.regenerate_layout().expect("first layout");
    assert_eq!(log.grabs(), vec!["register Ctrl+Alt+K"]);
    assert_eq!(shared.status(&summon()), GlobalHotkeyStatus::Active);
}

/// `layout()` re-runs on every `RefreshDom`: an unchanged declaration must
/// cost nothing at the OS.
#[test]
fn a_second_identical_layout_touches_nothing_at_the_os() {
    let (shared, log) = app();
    let _app = shared.enter();
    let mut window = make_window(HotkeyState {
        enabled: true,
        ..HotkeyState::default()
    });
    window.regenerate_layout().expect("first layout");
    let _ = log.grabs();
    window.regenerate_layout().expect("second layout");
    assert_eq!(log.grabs(), Vec::<String>::new());
}

#[test]
fn a_refresh_that_stops_declaring_releases() {
    let (shared, log) = app();
    let _app = shared.enter();
    let mut window = make_window(HotkeyState {
        enabled: true,
        ..HotkeyState::default()
    });
    window.regenerate_layout().expect("first layout");
    let _ = log.grabs();

    with_state(&window, |s| s.enabled = false);
    window.regenerate_layout().expect("refresh");
    assert_eq!(log.grabs(), vec!["unregister Ctrl+Alt+K"]);
    assert_eq!(
        shared.status(&summon()),
        GlobalHotkeyStatus::NotRegistered
    );
}

/// A pass that does not run `layout()` (the resize fast path, a restyle, a
/// plain pump turn) declares nothing new: the last declaration stands.
#[test]
fn a_sync_without_a_new_layout_keeps_the_grab() {
    let (shared, log) = app();
    let _app = shared.enter();
    let mut window = make_window(HotkeyState {
        enabled: true,
        ..HotkeyState::default()
    });
    window.regenerate_layout().expect("first layout");
    let _ = log.grabs();
    for _ in 0..3 {
        let _ = azul::desktop::global_hotkey::pump_headless(&mut window);
    }
    assert_eq!(log.grabs(), Vec::<String>::new());
    assert_eq!(shared.status(&summon()), GlobalHotkeyStatus::Active);
}

/// Every close path ends in the window being dropped; that is where its
/// declaration goes. The grab survives while another window still wants it.
#[test]
fn closing_the_window_releases_what_only_it_declared() {
    let (shared, log) = app();
    let _app = shared.enter();
    let mut first = make_window(HotkeyState {
        enabled: true,
        ..HotkeyState::default()
    });
    let mut second = make_window(HotkeyState {
        enabled: true,
        ..HotkeyState::default()
    });
    first.regenerate_layout().expect("first window");
    second.regenerate_layout().expect("second window");
    assert_eq!(log.grabs(), vec!["register Ctrl+Alt+K"], "one shared grab");

    drop(first);
    let _ = shared.sync();
    assert_eq!(log.grabs(), Vec::<String>::new(), "the second still wants it");

    drop(second);
    let _ = shared.sync();
    assert_eq!(log.grabs(), vec!["unregister Ctrl+Alt+K"]);
}

/// A `layout()` that shows the status sees `NotRegistered` on the pass that
/// first declares (the snapshot is taken before the pass), is asked to run
/// ONCE more to show `Active`, and then nothing further is owed.
#[test]
fn a_status_read_costs_exactly_one_extra_layout() {
    let (shared, _log) = app();
    let _app = shared.enter();
    let mut window = make_window(HotkeyState {
        enabled: true,
        read_status: true,
        ..HotkeyState::default()
    });
    window.regenerate_layout().expect("first layout");
    let _ = window.common.take_regeneration();

    let _ = azul::desktop::global_hotkey::pump_headless(&mut window);
    assert!(
        window.common.regeneration_pending(),
        "the status moved and this layout read it: one more pass is owed"
    );
    assert_eq!(window.pending_relayout_reason(), RelayoutReason::Other);
    let _ = window.common.take_regeneration();

    window.regenerate_layout().expect("the status-driven layout");
    // Only what the PUMP asks for is under test: retire anything the pass
    // itself left pending.
    let _ = window.common.take_regeneration();
    let _ = azul::desktop::global_hotkey::pump_headless(&mut window);
    assert!(
        !window.common.regeneration_pending(),
        "the second pass declared the same set: nothing moved, nothing is owed"
    );

    let (layouts, seen) = with_state(&window, |s| (s.layouts, s.seen.clone()));
    assert_eq!(layouts, 2);
    assert_eq!(
        seen,
        vec![GlobalHotkeyStatus::NotRegistered, GlobalHotkeyStatus::Active]
    );
}
