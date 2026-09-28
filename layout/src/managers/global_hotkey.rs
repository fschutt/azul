//! System-wide ("global") hotkeys: the process-wide registry, the backend
//! seam and the fire mailbox.
//!
//! # The shape, and why it is the tray's
//!
//! A hotkey fires while ANOTHER app has the keyboard: the Carbon handler runs
//! inside AppKit's event dispatch, `WM_HOTKEY` lands on a message-only window,
//! X11 reports a `KeyPress` on the root window of a second display connection,
//! and the Wayland portal's `Activated` signal arrives on a D-Bus thread. None
//! of those can hold a `CallbackInfo`. So the backends only [`push_fired`] an
//! id into this mailbox, and the run loop's pump takes the callbacks out with
//! [`take_fired`] and runs them against the app's first window - the same
//! route a tray menu click takes (`desktop::tray::pump_tray`).
//!
//! The registry is PROCESS-wide, not per window (see
//! `azul_core::global_hotkey` for why), so it is a static here rather than a
//! `LayoutWindow` field.
//!
//! # The backend seam
//!
//! `azul-layout` cannot reach the OS; the dll installs a
//! [`GlobalHotkeyBackend`] (Carbon, Win32, X11, the portal) at `App::create`,
//! and the headless shell replaces it with [`simulated_backend`], which grabs
//! nothing and is driven by [`simulate`] - that is how tests and `AZ_E2E`
//! scenarios press a hotkey. Same seam `widgets::capture_common` uses for the
//! camera: a table of plain function pointers, first-party only.
//!
//! # Threads
//!
//! `register` / `unregister` run on the event-loop thread (the one callbacks
//! run on - every platform needs that: Win32 delivers `WM_HOTKEY` to the
//! registering thread, Carbon wants the main thread). [`push_fired`] and
//! [`report`] may be called from any thread.

use alloc::{string::String, vec::Vec};

use azul_core::{
    global_hotkey::{GlobalHotkey, GlobalHotkeyError, GlobalHotkeyId, GlobalHotkeyStatus},
    menu::CoreMenuCallback,
};

/// What a backend answered to a registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendGrant {
    /// Grabbed now.
    Active,
    /// Asked for; the backend reports the answer later through [`report`]
    /// (the Wayland portal, whose bind may show the user a dialog).
    Pending,
}

/// The OS half of global hotkeys, installed by the dll.
///
/// # Contract
///
/// `register` and `unregister` are called on the event-loop thread and must
/// NOT call back into this module's process-wide functions synchronously -
/// [`install_backend`] calls them while it holds the registry. Reporting
/// later, from another thread, through [`report`] and [`push_fired`] is fine.
#[derive(Debug, Clone, Copy)]
pub struct GlobalHotkeyBackend {
    /// Shown by the capability probe, e.g. `"Carbon RegisterEventHotKey"`.
    pub name: &'static str,
    /// Is the backend usable in this session? `Err` carries the reason.
    pub probe: fn() -> Result<(), String>,
    /// Grab `hotkey` at the OS, reporting later fires as `id`.
    pub register: fn(GlobalHotkeyId, &GlobalHotkey) -> Result<BackendGrant, GlobalHotkeyError>,
    /// Release the grab for `id` (no-op for an id it does not hold).
    pub unregister: fn(GlobalHotkeyId),
    /// Per-loop-iteration work on the event-loop thread (X11 reads its grab
    /// connection here). Cheap and non-blocking.
    pub poll: fn(),
    /// Whether the run loop must wake up by itself while a hotkey is
    /// registered: `true` when fires arrive on something the loop does not
    /// wait on (a second X connection, a D-Bus thread, a headless
    /// simulation), `false` when the OS wakes the loop (Carbon, Win32).
    pub needs_loop_polling: bool,
}

/// One delivered fire: which registration, and the callback to run.
#[derive(Debug, Clone)]
pub struct FiredHotkey {
    pub id: GlobalHotkeyId,
    pub hotkey: GlobalHotkey,
    pub callback: CoreMenuCallback,
}

/// A person cannot press a hotkey more often than this between two loop
/// iterations; anything beyond it is a stuck sender, and the mailbox must
/// not grow for the life of the process.
pub const MAX_PENDING_FIRES: usize = 64;

/// The name [`simulated_backend`] reports.
pub const SIMULATED_BACKEND_NAME: &str = "headless (simulated)";

#[derive(Debug, Clone)]
struct Entry {
    id: GlobalHotkeyId,
    hotkey: GlobalHotkey,
    callback: CoreMenuCallback,
    status: GlobalHotkeyStatus,
}

/// The registrations, the installed backend and the fire mailbox.
///
/// A plain value so it can be tested with a fake backend; the process-wide
/// functions below wrap one static instance.
#[derive(Debug, Default)]
pub struct GlobalHotkeyRegistry {
    backend: Option<GlobalHotkeyBackend>,
    entries: Vec<Entry>,
    last_id: u32,
    fired: Vec<GlobalHotkeyId>,
}

// RED SKELETON: the types and the process-wide wrappers are final; the
// registry logic is stubbed so `layout/tests/global_hotkeys.rs` compiles and
// fails. The next commit implements it.
impl GlobalHotkeyRegistry {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            backend: None,
            entries: Vec::new(),
            last_id: 0,
            fired: Vec::new(),
        }
    }

    /// The installed backend, if any.
    #[must_use]
    pub const fn backend(&self) -> Option<GlobalHotkeyBackend> {
        self.backend
    }

    /// Install `backend`.
    pub fn set_backend(
        &mut self,
        backend: GlobalHotkeyBackend,
    ) -> Vec<(GlobalHotkeyId, GlobalHotkeyError)> {
        self.backend = Some(backend);
        Vec::new()
    }

    /// First half of a registration.
    ///
    /// # Errors
    /// Not implemented yet.
    pub fn begin_register(
        &mut self,
        hotkey: GlobalHotkey,
        callback: CoreMenuCallback,
    ) -> Result<(GlobalHotkeyId, GlobalHotkeyBackend), GlobalHotkeyError> {
        let _ = (hotkey, callback, &self.entries, self.last_id);
        Err(GlobalHotkeyError::Unsupported)
    }

    /// Second half of a registration.
    ///
    /// # Errors
    /// Not implemented yet.
    pub fn finish_register(
        &mut self,
        id: GlobalHotkeyId,
        outcome: Result<BackendGrant, GlobalHotkeyError>,
    ) -> Result<GlobalHotkeyId, GlobalHotkeyError> {
        let _ = (id, outcome);
        Err(GlobalHotkeyError::Platform("not implemented yet".into()))
    }

    /// Register in one step.
    ///
    /// # Errors
    /// Not implemented yet.
    pub fn register(
        &mut self,
        hotkey: GlobalHotkey,
        callback: CoreMenuCallback,
    ) -> Result<GlobalHotkeyId, GlobalHotkeyError> {
        let (id, backend) = self.begin_register(hotkey, callback)?;
        let outcome = (backend.register)(id, &hotkey);
        self.finish_register(id, outcome)
    }

    /// Forget `id`.
    pub fn remove(&mut self, id: GlobalHotkeyId) -> bool {
        let _ = (id, &self.fired);
        false
    }

    /// Forget `id` and release its grab.
    pub fn unregister(&mut self, id: GlobalHotkeyId) -> bool {
        self.remove(id)
    }

    /// A backend's late answer.
    pub fn report(&mut self, id: GlobalHotkeyId, outcome: Result<(), GlobalHotkeyError>) {
        let _ = (id, outcome);
    }

    /// Where `id` stands.
    #[must_use]
    pub fn status(&self, id: GlobalHotkeyId) -> GlobalHotkeyStatus {
        let _ = id;
        GlobalHotkeyStatus::NotRegistered
    }

    /// The live registration holding `hotkey`, if any.
    #[must_use]
    pub fn find(&self, hotkey: &GlobalHotkey) -> Option<GlobalHotkeyId> {
        let _ = hotkey;
        None
    }

    /// Every registration.
    #[must_use]
    pub fn registrations(&self) -> Vec<(GlobalHotkeyId, GlobalHotkey, GlobalHotkeyStatus)> {
        Vec::new()
    }

    /// Is any registration live?
    #[must_use]
    pub fn has_registrations(&self) -> bool {
        false
    }

    /// Must the run loop wake up by itself right now?
    #[must_use]
    pub fn needs_loop_polling(&self) -> bool {
        false
    }

    /// Park one fire.
    pub fn push_fired(&mut self, id: GlobalHotkeyId) {
        let _ = id;
    }

    /// Anything parked?
    #[must_use]
    pub fn has_pending_fires(&self) -> bool {
        false
    }

    /// Take every parked fire.
    pub fn take_fired(&mut self) -> Vec<FiredHotkey> {
        Vec::new()
    }

    /// Press `hotkey` as if the OS had reported it.
    pub fn simulate(&mut self, hotkey: &GlobalHotkey) -> bool {
        let _ = hotkey;
        false
    }
}

// ---------------------------------------------------------------------------
// The process-wide instance.
// ---------------------------------------------------------------------------

static REGISTRY: std::sync::Mutex<GlobalHotkeyRegistry> =
    std::sync::Mutex::new(GlobalHotkeyRegistry::new());

/// A poisoned lock is recovered, not unwrapped: a panic elsewhere must not
/// turn every later hotkey press into a second panic inside an OS callback.
fn lock() -> std::sync::MutexGuard<'static, GlobalHotkeyRegistry> {
    REGISTRY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Install (or replace) the backend. Live registrations move onto it; those
/// it refuses become `Failed` and are returned.
pub fn install_backend(backend: GlobalHotkeyBackend) -> Vec<(GlobalHotkeyId, GlobalHotkeyError)> {
    lock().set_backend(backend)
}

/// Install `backend` unless one is installed already. Returns whether it
/// was installed.
pub fn install_backend_if_none(backend: GlobalHotkeyBackend) -> bool {
    let mut registry = lock();
    if registry.backend().is_some() {
        return false;
    }
    let _ = registry.set_backend(backend);
    true
}

/// The installed backend's name, if one is installed.
#[must_use]
pub fn backend_name() -> Option<&'static str> {
    lock().backend().map(|b| b.name)
}

/// Register `hotkey` app-wide; `callback` runs with its `RefAny` whenever it
/// fires. Event-loop thread only.
///
/// # Errors
/// See [`GlobalHotkeyError`].
pub fn register(
    hotkey: GlobalHotkey,
    callback: CoreMenuCallback,
) -> Result<GlobalHotkeyId, GlobalHotkeyError> {
    let (id, backend) = lock().begin_register(hotkey, callback)?;
    // The backend runs WITHOUT the registry: a portal or an X server may
    // take a moment, and a fire pushed meanwhile from another thread must
    // not wait on it.
    let outcome = (backend.register)(id, &hotkey);
    let backend_granted = outcome.is_ok();
    let result = lock().finish_register(id, outcome);
    if result.is_err() && backend_granted {
        // Withdrawn while the backend answered: it holds a grab nobody owns.
        (backend.unregister)(id);
    }
    result
}

/// Release `id`. Returns whether it was registered. Event-loop thread only.
pub fn unregister(id: GlobalHotkeyId) -> bool {
    let backend = {
        let mut registry = lock();
        if !registry.remove(id) {
            return false;
        }
        registry.backend()
    };
    if let Some(backend) = backend {
        (backend.unregister)(id);
    }
    true
}

/// A backend's late answer for a `Pending` registration. Any thread.
pub fn report(id: GlobalHotkeyId, outcome: Result<(), GlobalHotkeyError>) {
    lock().report(id, outcome);
}

/// Where `id` stands.
#[must_use]
pub fn status(id: GlobalHotkeyId) -> GlobalHotkeyStatus {
    lock().status(id)
}

/// Is `id` still registered (live or failed)? Backend threads ask this before
/// finishing a slow handshake for a registration that may have been dropped.
#[must_use]
pub fn is_registered(id: GlobalHotkeyId) -> bool {
    !matches!(lock().status(id), GlobalHotkeyStatus::NotRegistered)
}

/// Every registration, live or failed.
#[must_use]
pub fn registrations() -> Vec<(GlobalHotkeyId, GlobalHotkey, GlobalHotkeyStatus)> {
    lock().registrations()
}

/// Park one fire reported by a backend. Any thread.
pub fn push_fired(id: GlobalHotkeyId) {
    lock().push_fired(id);
}

/// Anything parked?
#[must_use]
pub fn has_pending_fires() -> bool {
    lock().has_pending_fires()
}

/// Take every parked fire with its callback. Called by the run loop.
#[must_use]
pub fn take_fired() -> Vec<FiredHotkey> {
    lock().take_fired()
}

/// Press `hotkey` as if the OS had: headless tests and `AZ_E2E` drive
/// hotkeys through this. `false` when no live registration holds it.
pub fn simulate(hotkey: &GlobalHotkey) -> bool {
    lock().simulate(hotkey)
}

/// Is any registration live?
#[must_use]
pub fn has_registrations() -> bool {
    lock().has_registrations()
}

/// Must the run loop wake up by itself right now (see
/// [`GlobalHotkeyBackend::needs_loop_polling`])?
#[must_use]
pub fn needs_loop_polling() -> bool {
    lock().needs_loop_polling()
}

/// Run the backend's per-iteration work. Event-loop thread.
pub fn poll_backend() {
    let backend = lock().backend();
    if let Some(backend) = backend {
        (backend.poll)();
    }
}

/// What the capability probe reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalHotkeyProbe {
    pub available: bool,
    pub backend: &'static str,
    pub reason: String,
}

/// Ask the installed backend whether it is usable in this session.
#[must_use]
pub fn probe() -> GlobalHotkeyProbe {
    let backend = lock().backend();
    match backend {
        None => GlobalHotkeyProbe {
            available: false,
            backend: "none",
            reason: String::from("no global-hotkey backend exists on this platform"),
        },
        Some(backend) => match (backend.probe)() {
            Ok(()) => GlobalHotkeyProbe {
                available: true,
                backend: backend.name,
                reason: String::new(),
            },
            Err(reason) => GlobalHotkeyProbe {
                available: false,
                backend: backend.name,
                reason,
            },
        },
    }
}

// ---------------------------------------------------------------------------
// The headless backend.
// ---------------------------------------------------------------------------

fn simulated_probe() -> Result<(), String> {
    Ok(())
}

fn simulated_register(
    _id: GlobalHotkeyId,
    _hotkey: &GlobalHotkey,
) -> Result<BackendGrant, GlobalHotkeyError> {
    Ok(BackendGrant::Active)
}

fn simulated_unregister(_id: GlobalHotkeyId) {}

fn simulated_poll() {}

/// The backend a headless run installs: every registration succeeds, nothing
/// is grabbed at the OS, and a hotkey fires only through [`simulate`].
#[must_use]
pub fn simulated_backend() -> GlobalHotkeyBackend {
    GlobalHotkeyBackend {
        name: SIMULATED_BACKEND_NAME,
        probe: simulated_probe,
        register: simulated_register,
        unregister: simulated_unregister,
        poll: simulated_poll,
        // Nothing wakes a headless loop but its own timers, so it polls
        // while a hotkey is registered - a simulated press from outside a
        // callback would otherwise wait for an unrelated event.
        needs_loop_polling: true,
    }
}
