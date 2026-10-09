//! System-wide ("global") hotkeys: the App-owned manager that reconciles the
//! DECLARED set against the OS, the backend seam, and the mailbox the OS
//! threads report through.
//!
//! # Declared, not registered
//!
//! Hotkeys are derived from app state. Every source - each window's `layout()`
//! (`LayoutCallbackInfo::add_global_hotkey`) and the `AppConfig` (its static
//! list and its derived callback) - DECLARES the whole set it wants now, and
//! [`GlobalHotkeyManager::sync`] makes the OS grabs equal the union:
//!
//! - an accelerator nobody wanted before is grabbed;
//! - an accelerator nobody wants any more is released (before any grab of the same batch, so an
//!   A-for-B swap never holds both);
//! - an accelerator that stayed is NOT touched: its callback and `RefAny` are swapped in place. An
//!   identical re-declaration - the common case, since `layout()` re-runs on every `RefreshDom` -
//!   costs zero backend calls.
//!
//! Identity is the accelerator ([`GlobalHotkey`], struct equality): what every
//! OS keys a grab on, what `Dom::with_callback` keys on (the event), and what
//! gives the Wayland portal a stable shortcut id across launches.
//!
//! # One grab, one callback per press
//!
//! The OS set is the UNION over all sources, so N windows running the same
//! `layout()` share one grab. A press runs exactly ONE callback: a window's
//! declaration shadows the app's, and among windows the most recently focused
//! declarer wins, then the oldest window (the owner rule, resolved at fire
//! time, never cached).
//!
//! # Failures are sticky
//!
//! A refused accelerator is not asked for again - declared or not - until the
//! app retries it ([`GlobalHotkeyManager::retry`]) or the backend changes. An
//! app that falls back from a taken first choice would otherwise flip between
//! the two forever, and a declined Wayland dialog would come back on every
//! relayout.
//!
//! # Ownership and threads ("an Arc or similar")
//!
//! The manager is owned by the `App` and handed out as a
//! [`SharedGlobalHotkeys`] (`Arc<Mutex<GlobalHotkeyManager>>`), the way the
//! app-global undo manager is: every `LayoutWindow` of the app, the run loop
//! and the tray-only stub hold a clone. Declaring, syncing and delivering run
//! on the event-loop thread. The OS threads and handlers (the Carbon handler,
//! the `WM_HOTKEY` window procedure, the portal's D-Bus threads) never touch
//! the manager: they hold a [`HotkeySink`], a small bounded mailbox with an
//! optional loop waker, and the manager drains it on the loop thread.
//!
//! Which App a window belongs to is found without threading the handle
//! through every window constructor: `App::run` makes its handle the event
//! loop thread's CURRENT app ([`SharedGlobalHotkeys::enter`]) for as long as
//! the loop runs, and every `LayoutWindow` built on that thread joins it
//! ([`SharedGlobalHotkeys::current_or_detached`]). A `LayoutWindow` built
//! anywhere else (a test, the web server) gets a detached manager of its own
//! with no backend, where every declaration reads `Failed(Unsupported)`.
//!
//! # The backend seam
//!
//! `azul-layout` cannot reach the OS; the dll implements
//! [`GlobalHotkeyBackend`] (Carbon, Win32, X11, the portal) and the run
//! installs one - the platform's in a desktop run, [`SimulatedBackend`] in a
//! headless one, so a headless or CI run never grabs a real key.

use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet, VecDeque},
    string::String,
    sync::Arc,
    vec::Vec,
};
use core::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use azul_core::{
    global_hotkey::{
        take_recorded_global_hotkeys, GlobalHotkey, GlobalHotkeyCallbackData, GlobalHotkeyError,
        GlobalHotkeyEvent, GlobalHotkeyId, GlobalHotkeyInfo, GlobalHotkeyInfoVec,
        GlobalHotkeyOwner, GlobalHotkeyState, GlobalHotkeyStatus, GlobalHotkeysCallback,
        GlobalHotkeysCallbackInfo,
    },
    menu::CoreMenuCallback,
    refany::RefAny,
};
use azul_css::AzString;

/// A poisoned lock is recovered, not unwrapped: a panic elsewhere must not
/// turn every later hotkey press into a second panic inside an OS callback.
fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---------------------------------------------------------------------------
// The backend seam
// ---------------------------------------------------------------------------

/// What a backend answered to a grab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendGrant {
    /// Grabbed now.
    Active,
    /// Asked for; the backend reports the answer later through its sink as
    /// [`BackendEvent::Settled`] (the Wayland portal, whose bind may show the
    /// user a dialog).
    Pending,
}

/// The OS half of global hotkeys. First-party only; each backend keeps its
/// own state and reports fires and late answers through the [`HotkeySink`]
/// it was built with - never through a static.
///
/// # Contract
///
/// Every method runs on the event-loop thread, UNDER the manager's lock: it
/// must not run user code and must not block on the sink. Reporting later,
/// from any thread, through the sink is fine.
pub trait GlobalHotkeyBackend: Send {
    /// Shown by the capability probe, e.g. `"Carbon RegisterEventHotKey"`.
    fn name(&self) -> &'static str;
    /// Is the backend usable in this session? `Err` carries the reason.
    fn probe(&self) -> Result<(), String>;
    /// Grab `hotkey`; report its presses and any late answer under `os_id`.
    /// `description` is what the desktop shows the user for it (the portal).
    ///
    /// # Errors
    /// The platform's refusal.
    fn register(
        &mut self,
        os_id: GlobalHotkeyId,
        hotkey: &GlobalHotkey,
        description: &str,
    ) -> Result<BackendGrant, GlobalHotkeyError>;
    /// Release the grab held under `os_id` (a no-op for one it does not hold).
    fn unregister(&mut self, os_id: GlobalHotkeyId);
    /// End of one reconcile batch. The portal binds every `register` of the
    /// batch here, in ONE session (one approval dialog); grab backends do
    /// nothing.
    fn commit(&mut self) {}
    /// Per-iteration work on the event-loop thread (X11 reads its grab
    /// connection here). Cheap and non-blocking.
    fn poll(&mut self) {}
    /// An fd a Linux loop can add to its poll set so that a press wakes it
    /// (X11's grab connection). `None` where presses arrive through the sink.
    fn wake_fd(&self) -> Option<i32> {
        None
    }
    /// Input the backend already read off [`Self::wake_fd`] and holds
    /// unprocessed (Xlib's event queue), which the fd will therefore not
    /// announce again: a loop must not park on top of it.
    fn has_buffered_input(&self) -> bool {
        false
    }
    /// Whether presses arrive on something the loop does not wait on (a
    /// second X connection, a D-Bus thread, a headless simulation), so the
    /// loop has to wake up by itself unless a waker is attached to the sink.
    /// `false` where the OS wakes the loop (Carbon, Win32).
    fn needs_loop_polling(&self) -> bool {
        false
    }
}

/// What a backend (on any thread) tells the loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendEvent {
    /// The combination grabbed under `os_id` was pressed (or released).
    Fired {
        os_id: GlobalHotkeyId,
        state: GlobalHotkeyState,
        timestamp_ms: u64,
    },
    /// The late answer to a `Pending` grab: `Ok` with the trigger as the
    /// desktop shows it (empty = keep the display string), or the refusal.
    Settled {
        os_id: GlobalHotkeyId,
        result: Result<AzString, GlobalHotkeyError>,
    },
    /// The user re-bound the shortcut in the desktop's settings (the
    /// portal's `ShortcutsChanged`).
    TriggerChanged {
        os_id: GlobalHotkeyId,
        trigger: AzString,
    },
}

/// A person cannot press a hotkey more often than this between two loop
/// iterations; anything beyond it is a stuck sender, and the mailbox must not
/// grow for the life of the process.
pub const MAX_PENDING_FIRES: usize = 64;

/// Answers and trigger changes are bounded by the number of grabs; this caps a
/// backend that reports in a loop.
pub const MAX_PENDING_REPORTS: usize = 1024;

/// The name [`SimulatedBackend`] reports.
pub const SIMULATED_BACKEND_NAME: &str = "headless (simulated)";

/// Wakes the event loop from another thread (a condvar notify, an eventfd
/// write). See [`SharedGlobalHotkeys::attach_loop_waker`].
pub type LoopWaker = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct SinkQueue {
    events: VecDeque<BackendEvent>,
    fires: usize,
}

#[derive(Default)]
struct SinkInner {
    queue: Mutex<SinkQueue>,
    waker: Mutex<Option<LoopWaker>>,
}

/// The only thing an OS thread or handler ever holds: a bounded mailbox plus
/// an optional loop waker. A leaf lock - nothing is called while it is held.
#[derive(Clone, Default)]
pub struct HotkeySink {
    inner: Arc<SinkInner>,
}

impl core::fmt::Debug for HotkeySink {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let queued = lock_or_recover(&self.inner.queue).events.len();
        f.debug_struct("HotkeySink")
            .field("queued", &queued)
            .field("has_waker", &self.has_waker())
            .finish()
    }
}

impl HotkeySink {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue one report and wake the loop. A press beyond
    /// [`MAX_PENDING_FIRES`] (or a report beyond [`MAX_PENDING_REPORTS`]) is
    /// dropped: a stuck sender must not grow the mailbox.
    pub fn push(&self, event: BackendEvent) {
        let accepted = {
            let mut queue = lock_or_recover(&self.inner.queue);
            let is_fire = matches!(event, BackendEvent::Fired { .. });
            if is_fire {
                if queue.fires >= MAX_PENDING_FIRES {
                    false
                } else {
                    queue.fires += 1;
                    queue.events.push_back(event);
                    true
                }
            } else if queue.events.len().saturating_sub(queue.fires) >= MAX_PENDING_REPORTS {
                false
            } else {
                queue.events.push_back(event);
                true
            }
        };
        if accepted {
            // Cloned out first: the waker runs without the sink's locks.
            let waker = lock_or_recover(&self.inner.waker).clone();
            if let Some(waker) = waker {
                let wake: &(dyn Fn() + Send + Sync) = &*waker;
                wake();
            }
        }
    }

    /// A press of the grab held under `os_id`, now.
    pub fn fired(&self, os_id: GlobalHotkeyId) {
        self.push(BackendEvent::Fired {
            os_id,
            state: GlobalHotkeyState::Pressed,
            timestamp_ms: 0,
        });
    }

    /// Install (or remove) the waker [`Self::push`] calls.
    pub fn set_waker(&self, waker: Option<LoopWaker>) {
        *lock_or_recover(&self.inner.waker) = waker;
    }

    /// Is a loop waker installed?
    #[must_use]
    pub fn has_waker(&self) -> bool {
        lock_or_recover(&self.inner.waker).is_some()
    }

    /// Nothing queued?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        lock_or_recover(&self.inner.queue).events.is_empty()
    }

    fn drain(&self) -> Vec<BackendEvent> {
        let mut queue = lock_or_recover(&self.inner.queue);
        queue.fires = 0;
        queue.events.drain(..).collect()
    }
}

// ---------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------

/// The creation order of a `LayoutWindow`: monotonic, never reused, so "the
/// oldest window" is well defined and a closed window's number never comes
/// back.
pub type WindowSeq = u64;

static NEXT_WINDOW_SEQ: AtomicU64 = AtomicU64::new(1);

/// The next window's sequence number.
#[must_use]
pub fn next_window_seq() -> WindowSeq {
    NEXT_WINDOW_SEQ.fetch_add(1, Ordering::Relaxed)
}

/// Which declaration set: the `AppConfig`'s, or one window's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HotkeySource {
    App,
    Window(WindowSeq),
}

#[derive(Debug, Clone, Default)]
struct Declared {
    items: BTreeMap<GlobalHotkey, GlobalHotkeyCallbackData>,
    /// The pass that declared this read a status: re-run it when one moves.
    read_status: bool,
}

#[derive(Debug, Clone)]
struct Held {
    os_id: GlobalHotkeyId,
    pending: bool,
    trigger: AzString,
}

/// The `AppConfig`'s declarations: a static list, and optionally a callback
/// deriving more from the app's state.
#[derive(Debug, Clone)]
struct AppSource {
    static_items: Vec<GlobalHotkeyCallbackData>,
    callback: Option<GlobalHotkeysCallback>,
    data: RefAny,
    /// The callback must run before the next sync.
    dirty: bool,
}

/// One run of the app's hotkeys callback, cloned out of the manager so the
/// callback (app code) runs without the manager's lock.
struct AppJob {
    callback: GlobalHotkeysCallback,
    data: RefAny,
    static_items: Vec<GlobalHotkeyCallbackData>,
    snapshot: GlobalHotkeyInfoVec,
}

/// What one [`GlobalHotkeyManager::sync`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncOutcome {
    /// Some status or trigger moved.
    pub changed: bool,
    /// The sources whose last pass READ a status and so must run again.
    /// Empty when nothing changed - which is what bounds status feedback to
    /// one extra pass.
    pub relayout: Vec<HotkeySource>,
}

/// One press to run: whose declaration, which callback, which press.
#[derive(Debug, Clone)]
pub struct HotkeyDelivery {
    pub target: HotkeySource,
    pub callback: CoreMenuCallback,
    pub event: GlobalHotkeyEvent,
}

/// What the capability probe reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalHotkeyProbe {
    pub available: bool,
    pub backend: &'static str,
    pub reason: String,
}

/// Builds a platform backend reporting through the given sink - how a desktop
/// run defers the choice of backend until it knows the run is not headless.
pub type BackendFactory = fn(HotkeySink) -> Option<Box<dyn GlobalHotkeyBackend>>;

// ---------------------------------------------------------------------------
// The headless backend
// ---------------------------------------------------------------------------

/// What the simulated backend answers to the next grab of one accelerator
/// (the `global_hotkey_answer` E2E op programs it; the default is `Grant`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimulatedAnswer {
    Grant,
    Pending,
    Refuse(GlobalHotkeyError),
}

type AnswerTable = Arc<Mutex<BTreeMap<GlobalHotkey, SimulatedAnswer>>>;

/// The backend a headless run installs: nothing is grabbed at the OS, every
/// grab is granted unless an answer was programmed, and a hotkey fires only
/// through [`GlobalHotkeyManager::simulate`].
#[derive(Debug)]
pub struct SimulatedBackend {
    answers: AnswerTable,
}

impl GlobalHotkeyBackend for SimulatedBackend {
    fn name(&self) -> &'static str {
        SIMULATED_BACKEND_NAME
    }

    fn probe(&self) -> Result<(), String> {
        Ok(())
    }

    fn register(
        &mut self,
        _os_id: GlobalHotkeyId,
        hotkey: &GlobalHotkey,
        _description: &str,
    ) -> Result<BackendGrant, GlobalHotkeyError> {
        // One-shot: a programmed answer is consumed by the grab it answers.
        match lock_or_recover(&self.answers).remove(hotkey) {
            None | Some(SimulatedAnswer::Grant) => Ok(BackendGrant::Active),
            Some(SimulatedAnswer::Pending) => Ok(BackendGrant::Pending),
            Some(SimulatedAnswer::Refuse(e)) => Err(e),
        }
    }

    fn unregister(&mut self, _os_id: GlobalHotkeyId) {}

    fn needs_loop_polling(&self) -> bool {
        // Nothing wakes a headless loop but its own timers: without a waker
        // on the sink, a press simulated from outside a callback would wait
        // for an unrelated event.
        true
    }
}

// ---------------------------------------------------------------------------
// The manager
// ---------------------------------------------------------------------------

/// Declarations per source, the grabs held, statuses, remembered failures and
/// focus stamps. A plain value - tests drive it directly; the app shares one
/// through [`SharedGlobalHotkeys`].
pub struct GlobalHotkeyManager {
    backend: Option<Box<dyn GlobalHotkeyBackend>>,
    /// A platform backend chosen but not built yet (see
    /// [`Self::set_pending_backend`]).
    pending_backend: Option<BackendFactory>,
    /// Present while the simulated backend is installed.
    simulated_answers: Option<AnswerTable>,
    sink: HotkeySink,
    /// A loop waits on the sink's waker AND on [`Self::wake_fd`].
    loop_watches_wake_fd: bool,
    declared: BTreeMap<HotkeySource, Declared>,
    held: BTreeMap<GlobalHotkey, Held>,
    failures: BTreeMap<GlobalHotkey, GlobalHotkeyError>,
    retry: BTreeSet<GlobalHotkey>,
    focus: BTreeMap<WindowSeq, u64>,
    focus_clock: u64,
    fires: VecDeque<(GlobalHotkeyId, GlobalHotkeyState, u64)>,
    next_os_id: u32,
    dirty: bool,
    status_changed: bool,
    generation: u64,
    pending_relayout: BTreeSet<HotkeySource>,
    app: Option<AppSource>,
}

impl core::fmt::Debug for GlobalHotkeyManager {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GlobalHotkeyManager")
            .field("backend", &self.backend.as_ref().map(|b| b.name()))
            .field("declared", &self.declared.len())
            .field("held", &self.held.keys().collect::<Vec<_>>())
            .field("failures", &self.failures)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl Default for GlobalHotkeyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalHotkeyManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            backend: None,
            pending_backend: None,
            simulated_answers: None,
            sink: HotkeySink::new(),
            loop_watches_wake_fd: false,
            declared: BTreeMap::new(),
            held: BTreeMap::new(),
            failures: BTreeMap::new(),
            retry: BTreeSet::new(),
            focus: BTreeMap::new(),
            focus_clock: 0,
            fires: VecDeque::new(),
            next_os_id: 0,
            dirty: false,
            status_changed: false,
            generation: 0,
            pending_relayout: BTreeSet::new(),
            app: None,
        }
    }

    /// The `AppConfig`'s declarations (`global_hotkeys`, and the derived
    /// `global_hotkeys_callback` over the app's `data`). A static list alone
    /// is declared right away; a callback runs at the next
    /// [`SharedGlobalHotkeys::refresh_app_declarations`].
    pub fn set_app_declarations(
        &mut self,
        static_items: Vec<GlobalHotkeyCallbackData>,
        callback: Option<GlobalHotkeysCallback>,
        data: RefAny,
    ) {
        let has_callback = callback.is_some();
        if !has_callback {
            self.declare(HotkeySource::App, static_items.clone(), false);
        }
        self.app = Some(AppSource {
            static_items,
            callback,
            data,
            dirty: has_callback,
        });
    }

    /// The app state may have changed (a callback returned `RefreshDom`):
    /// the app's hotkeys callback runs again before the next sync.
    pub const fn mark_app_dirty(&mut self) {
        if let Some(app) = self.app.as_mut() {
            if app.callback.is_some() {
                app.dirty = true;
            }
        }
    }

    fn take_app_job(&mut self) -> Option<AppJob> {
        let due = self
            .app
            .as_ref()
            .is_some_and(|app| app.dirty && app.callback.is_some());
        if !due {
            return None;
        }
        let snapshot = self.snapshot_for(HotkeySource::App);
        let app = self.app.as_mut()?;
        app.dirty = false;
        Some(AppJob {
            callback: app.callback.clone()?,
            data: app.data.clone(),
            static_items: app.static_items.clone(),
            snapshot,
        })
    }

    /// The mailbox this manager's backends report through.
    #[must_use]
    pub fn sink(&self) -> HotkeySink {
        self.sink.clone()
    }

    /// Install (or replace) the backend. Everything held on the previous one
    /// is released there, every remembered failure is forgotten (it was that
    /// backend's answer), and the next [`Self::sync`] grabs the declared set
    /// on the new one.
    pub fn install_backend(&mut self, backend: Box<dyn GlobalHotkeyBackend>) {
        self.pending_backend = None;
        self.simulated_answers = None;
        self.replace_backend(Some(backend));
    }

    /// Install the headless simulation: nothing is grabbed at the OS.
    pub fn install_simulated_backend(&mut self) {
        let answers: AnswerTable = Arc::new(Mutex::new(BTreeMap::new()));
        self.install_backend(Box::new(SimulatedBackend {
            answers: answers.clone(),
        }));
        self.simulated_answers = Some(answers);
    }

    /// Choose the platform backend without building it: the next
    /// [`Self::sync`] builds and installs it, unless a backend (the headless
    /// simulation) was installed first. This is what keeps a run that turns
    /// out headless from ever touching the real OS - and a Wayland desktop
    /// from starting a portal handshake for a CI run.
    pub fn set_pending_backend(&mut self, factory: BackendFactory) {
        if self.backend.is_none() {
            self.pending_backend = Some(factory);
        }
    }

    fn install_pending_backend(&mut self) {
        if let Some(factory) = self.pending_backend.take() {
            if let Some(backend) = factory(self.sink.clone()) {
                self.replace_backend(Some(backend));
            }
        }
    }

    fn replace_backend(&mut self, backend: Option<Box<dyn GlobalHotkeyBackend>>) {
        let had_grabs = !self.held.is_empty();
        if let Some(old) = self.backend.as_mut() {
            for held in self.held.values() {
                old.unregister(held.os_id);
            }
            if had_grabs {
                old.commit();
            }
        }
        self.held.clear();
        self.failures.clear();
        self.retry.clear();
        // Queued presses and answers carry the OLD backend's ids.
        self.fires.clear();
        drop(self.sink.drain());
        self.backend = backend;
        self.dirty = true;
        self.status_changed = true;
    }

    /// The installed backend's name.
    #[must_use]
    pub fn backend_name(&self) -> Option<&'static str> {
        self.backend.as_ref().map(|b| b.name())
    }

    /// The installed backend's probe, `None` without a backend.
    #[must_use]
    pub fn probe(&self) -> Option<Result<(), String>> {
        self.backend.as_ref().map(|b| b.probe())
    }

    /// `source` now wants exactly `items` (the WHOLE set - an accelerator it
    /// declared before and not now is released at the next sync unless
    /// another source wants it). A duplicate inside `items`: the last one
    /// wins. Callbacks and data are swapped HERE; the OS is only touched by
    /// [`Self::sync`], and only when the set of accelerators moved.
    pub fn declare(
        &mut self,
        source: HotkeySource,
        items: Vec<GlobalHotkeyCallbackData>,
        read_status: bool,
    ) {
        let mut wanted: BTreeMap<GlobalHotkey, GlobalHotkeyCallbackData> = BTreeMap::new();
        for item in items {
            let hotkey = item.hotkey;
            wanted.insert(hotkey, item);
        }
        let set_moved = match self.declared.get(&source) {
            Some(old) => !same_accelerators(&old.items, &wanted),
            None => !wanted.is_empty(),
        };
        if set_moved {
            self.dirty = true;
        }
        self.declared.insert(
            source,
            Declared {
                items: wanted,
                read_status,
            },
        );
    }

    /// `source` is gone (a window closed): what only it wanted is released at
    /// the next sync.
    pub fn forget_source(&mut self, source: HotkeySource) {
        if let Some(old) = self.declared.remove(&source) {
            if !old.items.is_empty() {
                self.dirty = true;
            }
        }
        if let HotkeySource::Window(seq) = source {
            self.focus.remove(&seq);
        }
        self.pending_relayout.remove(&source);
    }

    /// Window `seq` took the keyboard focus. The owner rule prefers the most
    /// recently focused declarer.
    pub fn note_focus(&mut self, seq: WindowSeq) {
        self.focus_clock = self.focus_clock.wrapping_add(1);
        self.focus.insert(seq, self.focus_clock);
    }

    /// Forget `hotkey`'s remembered failure: the next sync asks the OS again
    /// if it is still declared (on Wayland: the portal dialog is shown again).
    pub fn retry(&mut self, hotkey: GlobalHotkey) {
        self.retry.insert(hotkey);
        self.dirty = true;
    }

    /// Bring the OS in line with the union of every declaration, and fold in
    /// what the backends reported meanwhile. The returned outcome names the
    /// sources that read a status and must run again; they are also kept for
    /// [`Self::take_relayout`].
    pub fn sync(&mut self) -> SyncOutcome {
        self.install_pending_backend();
        self.absorb_sink();
        let mut changed = core::mem::take(&mut self.status_changed);
        if self.dirty {
            self.dirty = false;
            if self.reconcile() {
                changed = true;
            }
        }
        if !changed {
            return SyncOutcome::default();
        }
        self.generation = self.generation.wrapping_add(1);
        let relayout: Vec<HotkeySource> = self
            .declared
            .iter()
            .filter(|(_, declared)| declared.read_status)
            .map(|(source, _)| *source)
            .collect();
        for source in &relayout {
            match source {
                // The app's callback re-runs at the next refresh.
                HotkeySource::App => self.mark_app_dirty(),
                HotkeySource::Window(_) => {
                    self.pending_relayout.insert(*source);
                }
            }
        }
        SyncOutcome {
            changed: true,
            relayout,
        }
    }

    /// Every accelerator some source declares, with the description of the
    /// declaration that owns it.
    fn wanted(&self) -> BTreeMap<GlobalHotkey, AzString> {
        let mut out = BTreeMap::new();
        for declared in self.declared.values() {
            for hotkey in declared.items.keys() {
                if out.contains_key(hotkey) {
                    continue;
                }
                let description = self
                    .pick_owner(hotkey)
                    .and_then(|owner| self.declared.get(&owner))
                    .and_then(|d| d.items.get(hotkey))
                    .map(|item| item.description.clone())
                    .filter(|d| !d.as_str().is_empty())
                    .unwrap_or_else(|| hotkey.to_display_string());
                out.insert(*hotkey, description);
            }
        }
        out
    }

    fn reconcile(&mut self) -> bool {
        let mut changed = false;
        for hotkey in core::mem::take(&mut self.retry) {
            if self.failures.remove(&hotkey).is_some() {
                changed = true;
            }
        }
        let wanted = self.wanted();
        let mut called = false;

        // 1. Release first, so an A-for-B swap inside one batch never holds
        //    both.
        let gone: Vec<GlobalHotkey> = self
            .held
            .keys()
            .filter(|hotkey| !wanted.contains_key(*hotkey))
            .copied()
            .collect();
        for hotkey in gone {
            if let Some(held) = self.held.remove(&hotkey) {
                if let Some(backend) = self.backend.as_mut() {
                    backend.unregister(held.os_id);
                    called = true;
                }
                changed = true;
            }
        }

        // 2. Grab what is new. Everything kept is untouched; a remembered
        //    failure is not asked again.
        for (hotkey, description) in &wanted {
            if self.held.contains_key(hotkey) || self.failures.contains_key(hotkey) {
                continue;
            }
            changed = true;
            if let Err(e) = hotkey.validate() {
                self.failures.insert(*hotkey, e);
                continue;
            }
            if self.backend.is_none() {
                self.failures
                    .insert(*hotkey, GlobalHotkeyError::Unsupported);
                continue;
            }
            let os_id = self.next_os_id();
            let answer = match self.backend.as_mut() {
                Some(backend) => backend.register(os_id, hotkey, description.as_str()),
                None => Err(GlobalHotkeyError::Unsupported),
            };
            called = true;
            match answer {
                Ok(grant) => {
                    self.held.insert(
                        *hotkey,
                        Held {
                            os_id,
                            pending: grant == BackendGrant::Pending,
                            trigger: hotkey.to_display_string(),
                        },
                    );
                }
                Err(e) => {
                    self.failures.insert(*hotkey, e);
                }
            }
        }

        // 3. The portal binds the whole batch in one session here.
        if called {
            if let Some(backend) = self.backend.as_mut() {
                backend.commit();
            }
        }
        changed
    }

    fn next_os_id(&mut self) -> GlobalHotkeyId {
        // Never reused, never 0 (0 reads as "none" in C).
        self.next_os_id = self.next_os_id.wrapping_add(1).max(1);
        GlobalHotkeyId {
            id: self.next_os_id,
        }
    }

    fn hotkey_of(&self, os_id: GlobalHotkeyId) -> Option<GlobalHotkey> {
        self.held
            .iter()
            .find(|(_, held)| held.os_id == os_id)
            .map(|(hotkey, _)| *hotkey)
    }

    /// Fold what the backends reported into the statuses; presses stay
    /// queued for [`Self::take_deliveries`].
    fn absorb_sink(&mut self) {
        for event in self.sink.drain() {
            match event {
                BackendEvent::Fired {
                    os_id,
                    state,
                    timestamp_ms,
                } => {
                    if self.fires.len() < MAX_PENDING_FIRES {
                        self.fires.push_back((os_id, state, timestamp_ms));
                    }
                }
                BackendEvent::Settled { os_id, result } => {
                    let Some(hotkey) = self.hotkey_of(os_id) else {
                        // Released while the desktop was answering.
                        continue;
                    };
                    match result {
                        Ok(trigger) => {
                            if let Some(held) = self.held.get_mut(&hotkey) {
                                held.pending = false;
                                if !trigger.as_str().is_empty() {
                                    held.trigger = trigger;
                                }
                            }
                        }
                        Err(e) => {
                            self.held.remove(&hotkey);
                            self.failures.insert(hotkey, e);
                        }
                    }
                    self.status_changed = true;
                }
                BackendEvent::TriggerChanged { os_id, trigger } => {
                    let Some(hotkey) = self.hotkey_of(os_id) else {
                        continue;
                    };
                    if let Some(held) = self.held.get_mut(&hotkey) {
                        held.trigger = trigger;
                    }
                    self.status_changed = true;
                }
            }
        }
    }

    /// Take every press reported since the last call, resolved to its CURRENT
    /// owner's callback and data. A press for an accelerator released
    /// meanwhile is dropped. Clone the callbacks out and run them WITHOUT the
    /// manager's lock.
    pub fn take_deliveries(&mut self) -> Vec<HotkeyDelivery> {
        self.absorb_sink();
        let fires: Vec<(GlobalHotkeyId, GlobalHotkeyState, u64)> = self.fires.drain(..).collect();
        let mut out = Vec::new();
        for (os_id, state, timestamp_ms) in fires {
            let Some(hotkey) = self.hotkey_of(os_id) else {
                continue;
            };
            if let Some(held) = self.held.get_mut(&hotkey) {
                // The portal can report an activation before its bind
                // answer: a press is proof enough.
                if held.pending {
                    held.pending = false;
                    self.status_changed = true;
                }
            }
            let Some(target) = self.pick_owner(&hotkey) else {
                continue;
            };
            let Some(item) = self
                .declared
                .get(&target)
                .and_then(|declared| declared.items.get(&hotkey))
            else {
                continue;
            };
            out.push(HotkeyDelivery {
                target,
                callback: CoreMenuCallback {
                    refany: item.refany.clone(),
                    callback: item.callback.clone(),
                },
                event: GlobalHotkeyEvent {
                    hotkey,
                    state,
                    timestamp_ms,
                },
            });
        }
        out
    }

    /// Whose declaration a press of `hotkey` runs: among the windows that
    /// declare it the most recently focused, then the oldest; else the app's.
    fn pick_owner(&self, hotkey: &GlobalHotkey) -> Option<HotkeySource> {
        let mut best: Option<(WindowSeq, u64)> = None;
        // Ascending sequence order, and only a strictly newer focus stamp
        // replaces: a tie keeps the older window.
        for (source, declared) in &self.declared {
            let HotkeySource::Window(seq) = source else {
                continue;
            };
            if !declared.items.contains_key(hotkey) {
                continue;
            }
            let stamp = self.focus.get(seq).copied().unwrap_or(0);
            match best {
                Some((_, best_stamp)) if best_stamp >= stamp => {}
                _ => best = Some((*seq, stamp)),
            }
        }
        if let Some((seq, _)) = best {
            return Some(HotkeySource::Window(seq));
        }
        let app_declares = self
            .declared
            .get(&HotkeySource::App)
            .is_some_and(|declared| declared.items.contains_key(hotkey));
        app_declares.then_some(HotkeySource::App)
    }

    /// Where `hotkey` stands now.
    #[must_use]
    pub fn status(&self, hotkey: &GlobalHotkey) -> GlobalHotkeyStatus {
        if let Some(held) = self.held.get(hotkey) {
            return if held.pending {
                GlobalHotkeyStatus::Pending
            } else {
                GlobalHotkeyStatus::Active
            };
        }
        match self.failures.get(hotkey) {
            Some(e) => GlobalHotkeyStatus::Failed(e.clone()),
            None => GlobalHotkeyStatus::NotRegistered,
        }
    }

    /// Every accelerator the app wants, holds or failed to get, with status,
    /// trigger and owner - the owner relative to `viewer`.
    #[must_use]
    pub fn infos_for(&self, viewer: HotkeySource) -> Vec<GlobalHotkeyInfo> {
        let mut keys: BTreeSet<GlobalHotkey> = BTreeSet::new();
        for declared in self.declared.values() {
            keys.extend(declared.items.keys().copied());
        }
        keys.extend(self.held.keys().copied());
        keys.extend(self.failures.keys().copied());
        keys.into_iter()
            .map(|hotkey| {
                let owner = match self.pick_owner(&hotkey) {
                    None => GlobalHotkeyOwner::Nobody,
                    Some(HotkeySource::App) => GlobalHotkeyOwner::App,
                    Some(source) if source == viewer => GlobalHotkeyOwner::ThisWindow,
                    Some(_) => GlobalHotkeyOwner::OtherWindow,
                };
                let trigger = self
                    .held
                    .get(&hotkey)
                    .map_or_else(|| hotkey.to_display_string(), |held| held.trigger.clone());
                GlobalHotkeyInfo {
                    hotkey,
                    status: self.status(&hotkey),
                    trigger,
                    owner,
                }
            })
            .collect()
    }

    /// [`Self::infos_for`] as the FFI vector a callback info carries.
    #[must_use]
    pub fn snapshot_for(&self, viewer: HotkeySource) -> GlobalHotkeyInfoVec {
        GlobalHotkeyInfoVec::from_vec(self.infos_for(viewer))
    }

    /// Press `hotkey` as if the OS had reported it (tests, `AZ_E2E`). `false`
    /// when nothing holds it.
    pub fn simulate(&mut self, hotkey: &GlobalHotkey) -> bool {
        let Some(os_id) = self.held.get(hotkey).map(|held| held.os_id) else {
            return false;
        };
        self.sink.fired(os_id);
        true
    }

    /// Play a backend's late answer for the grab of `hotkey` (the portal's
    /// `Response`). `false` when nothing holds it.
    pub fn settle(
        &mut self,
        hotkey: &GlobalHotkey,
        result: Result<AzString, GlobalHotkeyError>,
    ) -> bool {
        let Some(os_id) = self.held.get(hotkey).map(|held| held.os_id) else {
            return false;
        };
        self.sink.push(BackendEvent::Settled { os_id, result });
        true
    }

    /// Program the simulated backend's answer to the next grab of `hotkey`.
    /// `false` when the simulation is not installed.
    pub fn program_answer(&mut self, hotkey: GlobalHotkey, answer: SimulatedAnswer) -> bool {
        match self.simulated_answers.as_ref() {
            Some(answers) => {
                lock_or_recover(answers).insert(hotkey, answer);
                true
            }
            None => false,
        }
    }

    /// The backend's per-iteration work (X11 reads its grab connection).
    pub fn poll_backend(&mut self) {
        if let Some(backend) = self.backend.as_mut() {
            backend.poll();
        }
    }

    /// Anything grabbed (or being grabbed) right now?
    #[must_use]
    pub fn has_grabs(&self) -> bool {
        !self.held.is_empty()
    }

    /// Must the run loop wake up by itself right now? True while something is
    /// grabbed on a backend whose presses arrive on something the loop does
    /// not wait on, until a loop attaches a waker (and, for a backend with a
    /// wake fd, watches that fd).
    #[must_use]
    pub fn needs_loop_polling(&self) -> bool {
        let Some(backend) = self.backend.as_ref() else {
            return false;
        };
        if self.held.is_empty() {
            return false;
        }
        if backend.wake_fd().is_some() && !self.loop_watches_wake_fd {
            return true;
        }
        backend.needs_loop_polling() && !self.sink.has_waker()
    }

    /// The backend's wake fd (X11), for a loop's poll set.
    #[must_use]
    pub fn wake_fd(&self) -> Option<i32> {
        self.backend.as_ref().and_then(|b| b.wake_fd())
    }

    /// Is a press owed that no wake fd will announce - one already in the
    /// sink, or input the backend buffered off its fd?
    #[must_use]
    pub fn has_buffered_input(&self) -> bool {
        !self.sink.is_empty() || self.backend.as_ref().is_some_and(|b| b.has_buffered_input())
    }

    /// A loop wakes on `waker` (and, when `watches_wake_fd`, on
    /// [`Self::wake_fd`]): it no longer has to poll.
    pub fn attach_loop_waker(&mut self, waker: LoopWaker, watches_wake_fd: bool) {
        self.sink.set_waker(Some(waker));
        self.loop_watches_wake_fd = watches_wake_fd;
    }

    /// Did a sync ask `source` to run again? Consumes the request.
    pub fn take_relayout(&mut self, source: HotkeySource) -> bool {
        self.pending_relayout.remove(&source)
    }

    /// The window an APP-level press runs against: the most recently focused
    /// of `live`, then the oldest.
    #[must_use]
    pub fn app_target(&self, live: &[WindowSeq]) -> Option<WindowSeq> {
        let mut best: Option<(WindowSeq, u64)> = None;
        for seq in live {
            let stamp = self.focus.get(seq).copied().unwrap_or(0);
            best = match best {
                Some((best_seq, best_stamp))
                    if best_stamp > stamp || (best_stamp == stamp && best_seq < *seq) =>
                {
                    Some((best_seq, best_stamp))
                }
                _ => Some((*seq, stamp)),
            };
        }
        best.map(|(seq, _)| seq)
    }

    /// Bumped whenever a status or trigger moved.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
}

/// Same accelerators, same descriptions? (Callbacks and data may differ: those
/// are swapped without the OS.)
fn same_accelerators(
    a: &BTreeMap<GlobalHotkey, GlobalHotkeyCallbackData>,
    b: &BTreeMap<GlobalHotkey, GlobalHotkeyCallbackData>,
) -> bool {
    a.len() == b.len()
        && a
            .iter()
            .zip(b.iter())
            .all(|((ka, va), (kb, vb))| ka == kb && va.description == vb.description)
}

// ---------------------------------------------------------------------------
// The App's handle
// ---------------------------------------------------------------------------

/// The App's handle on its [`GlobalHotkeyManager`]: cloned into every
/// `LayoutWindow`, the run loop and the tray-only stub.
#[derive(Clone)]
pub struct SharedGlobalHotkeys {
    manager: Arc<Mutex<GlobalHotkeyManager>>,
    sink: HotkeySink,
}

impl core::fmt::Debug for SharedGlobalHotkeys {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // `try_lock`: printing a window (whose Debug reaches this) while the
        // same thread holds the manager must not deadlock.
        match self.manager.try_lock() {
            Ok(manager) => f
                .debug_tuple("SharedGlobalHotkeys")
                .field(&*manager)
                .finish(),
            Err(_) => f.write_str("SharedGlobalHotkeys(<locked>)"),
        }
    }
}

impl Default for SharedGlobalHotkeys {
    fn default() -> Self {
        Self::new()
    }
}

std::thread_local! {
    /// The App whose loop runs on this thread (see [`SharedGlobalHotkeys::enter`]).
    static CURRENT_APP: core::cell::RefCell<Option<SharedGlobalHotkeys>> =
        const { core::cell::RefCell::new(None) };
    /// The press whose callback runs right now (see [`with_delivered_event`]).
    static DELIVERING: core::cell::Cell<Option<GlobalHotkeyEvent>> =
        const { core::cell::Cell::new(None) };
}

/// Restores the previous current App when the scope ends (see
/// [`SharedGlobalHotkeys::enter`]).
#[must_use = "the App stays current only while this scope is alive"]
#[derive(Debug)]
pub struct AppHotkeysScope {
    previous: Option<SharedGlobalHotkeys>,
}

impl Drop for AppHotkeysScope {
    fn drop(&mut self) {
        let previous = self.previous.take();
        CURRENT_APP.with(|current| *current.borrow_mut() = previous);
    }
}

impl SharedGlobalHotkeys {
    /// A new manager with no backend.
    #[must_use]
    pub fn new() -> Self {
        let manager = GlobalHotkeyManager::new();
        let sink = manager.sink();
        Self {
            manager: Arc::new(Mutex::new(manager)),
            sink,
        }
    }

    /// The manager, locked. Never call a user callback while holding it.
    #[must_use]
    pub fn lock(&self) -> MutexGuard<'_, GlobalHotkeyManager> {
        lock_or_recover(&self.manager)
    }

    /// The mailbox backends report through.
    #[must_use]
    pub fn sink(&self) -> HotkeySink {
        self.sink.clone()
    }

    /// Two handles on the same manager?
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.manager, &other.manager)
    }

    /// Make this the CURRENT App of the calling (event-loop) thread until the
    /// returned scope is dropped: every `LayoutWindow` built on this thread
    /// meanwhile joins it. `App::run` does this around the whole loop.
    pub fn enter(&self) -> AppHotkeysScope {
        let previous = CURRENT_APP.with(|current| current.borrow_mut().replace(self.clone()));
        AppHotkeysScope { previous }
    }

    /// The App whose loop runs on this thread, if any.
    #[must_use]
    pub fn current() -> Option<Self> {
        CURRENT_APP.with(|current| current.borrow().clone())
    }

    /// The current App's handle, or a detached manager of its own (no
    /// backend: every declaration reads `Failed(Unsupported)`).
    #[must_use]
    pub fn current_or_detached() -> Self {
        Self::current().unwrap_or_default()
    }

    /// See [`GlobalHotkeyManager::install_backend`].
    pub fn install_backend(&self, backend: Box<dyn GlobalHotkeyBackend>) {
        self.lock().install_backend(backend);
    }

    /// See [`GlobalHotkeyManager::install_simulated_backend`].
    pub fn install_simulated_backend(&self) {
        self.lock().install_simulated_backend();
    }

    /// See [`GlobalHotkeyManager::set_pending_backend`].
    pub fn set_pending_backend(&self, factory: BackendFactory) {
        self.lock().set_pending_backend(factory);
    }

    /// See [`GlobalHotkeyManager::backend_name`].
    #[must_use]
    pub fn backend_name(&self) -> Option<&'static str> {
        self.lock().backend_name()
    }

    /// See [`GlobalHotkeyManager::declare`].
    pub fn declare(
        &self,
        source: HotkeySource,
        items: Vec<GlobalHotkeyCallbackData>,
        read_status: bool,
    ) {
        self.lock().declare(source, items, read_status);
    }

    /// See [`GlobalHotkeyManager::forget_source`].
    pub fn forget_source(&self, source: HotkeySource) {
        self.lock().forget_source(source);
    }

    /// See [`GlobalHotkeyManager::note_focus`].
    pub fn note_focus(&self, seq: WindowSeq) {
        self.lock().note_focus(seq);
    }

    /// See [`GlobalHotkeyManager::retry`].
    pub fn retry(&self, hotkey: GlobalHotkey) {
        self.lock().retry(hotkey);
    }

    /// See [`GlobalHotkeyManager::sync`].
    #[must_use]
    pub fn sync(&self) -> SyncOutcome {
        self.lock().sync()
    }

    /// See [`GlobalHotkeyManager::status`].
    #[must_use]
    pub fn status(&self, hotkey: &GlobalHotkey) -> GlobalHotkeyStatus {
        self.lock().status(hotkey)
    }

    /// See [`GlobalHotkeyManager::infos_for`].
    #[must_use]
    pub fn infos_for(&self, viewer: HotkeySource) -> Vec<GlobalHotkeyInfo> {
        self.lock().infos_for(viewer)
    }

    /// See [`GlobalHotkeyManager::snapshot_for`].
    #[must_use]
    pub fn snapshot_for(&self, viewer: HotkeySource) -> GlobalHotkeyInfoVec {
        self.lock().snapshot_for(viewer)
    }

    /// See [`GlobalHotkeyManager::simulate`].
    #[must_use]
    pub fn simulate(&self, hotkey: &GlobalHotkey) -> bool {
        self.lock().simulate(hotkey)
    }

    /// See [`GlobalHotkeyManager::settle`].
    #[must_use]
    pub fn settle(
        &self,
        hotkey: &GlobalHotkey,
        result: Result<AzString, GlobalHotkeyError>,
    ) -> bool {
        self.lock().settle(hotkey, result)
    }

    /// See [`GlobalHotkeyManager::program_answer`].
    #[must_use]
    pub fn program_answer(&self, hotkey: GlobalHotkey, answer: SimulatedAnswer) -> bool {
        self.lock().program_answer(hotkey, answer)
    }

    /// See [`GlobalHotkeyManager::poll_backend`].
    pub fn poll_backend(&self) {
        self.lock().poll_backend();
    }

    /// See [`GlobalHotkeyManager::needs_loop_polling`].
    #[must_use]
    pub fn needs_loop_polling(&self) -> bool {
        self.lock().needs_loop_polling()
    }

    /// See [`GlobalHotkeyManager::wake_fd`].
    #[must_use]
    pub fn wake_fd(&self) -> Option<i32> {
        self.lock().wake_fd()
    }

    /// See [`GlobalHotkeyManager::has_buffered_input`].
    #[must_use]
    pub fn has_buffered_input(&self) -> bool {
        self.lock().has_buffered_input()
    }

    /// See [`GlobalHotkeyManager::attach_loop_waker`].
    pub fn attach_loop_waker(&self, waker: LoopWaker, watches_wake_fd: bool) {
        self.lock().attach_loop_waker(waker, watches_wake_fd);
    }

    /// What the capability probe reports for this app's backend.
    #[must_use]
    pub fn probe(&self) -> GlobalHotkeyProbe {
        let manager = self.lock();
        match (manager.backend_name(), manager.probe()) {
            (Some(backend), Some(Ok(()))) => GlobalHotkeyProbe {
                available: true,
                backend,
                reason: String::new(),
            },
            (Some(backend), Some(Err(reason))) => GlobalHotkeyProbe {
                available: false,
                backend,
                reason,
            },
            _ => GlobalHotkeyProbe {
                available: false,
                backend: "none",
                reason: String::from("no global-hotkey backend is installed for this app"),
            },
        }
    }
}

/// What one run-loop turn has to do, for the live windows it was given (see
/// [`SharedGlobalHotkeys::begin_turn`]). Indices are into that list.
#[derive(Debug, Default)]
pub struct HotkeyTurn {
    /// Each press, with the index of the window it runs against: its owner
    /// window, or for an app-level press the most recently focused live
    /// window, else the oldest.
    pub deliveries: Vec<(usize, HotkeyDelivery)>,
    /// The windows whose last `layout()` read a status that moved: each must
    /// lay out once more.
    pub relayout: Vec<usize>,
    /// Presses with no live window to run against (the owner is not in the
    /// list, or an app-level press while no window exists). Never handed to
    /// some other window instead.
    pub undeliverable: Vec<HotkeyDelivery>,
}

impl SharedGlobalHotkeys {
    /// One turn of a run loop's hotkey pump, given its live windows'
    /// sequence numbers in any order: re-derive the `AppConfig`'s set if it
    /// is due, poll the backend, sync, and route every press to its OWNER's
    /// window - not to whichever window a platform registry lists first
    /// (address order on macOS, `HWND` order on Windows, hash order on
    /// Linux). Run the deliveries WITHOUT the manager's lock (this returns
    /// with it released).
    #[must_use]
    pub fn begin_turn(&self, live: &[WindowSeq]) -> HotkeyTurn {
        let _ = self.refresh_app_declarations();
        let mut manager = self.lock();
        manager.poll_backend();
        drop(manager.sync());
        let presses = manager.take_deliveries();
        let app_window = manager
            .app_target(live)
            .and_then(|seq| live.iter().position(|s| *s == seq));
        let mut turn = HotkeyTurn::default();
        for delivery in presses {
            let index = match delivery.target {
                HotkeySource::Window(seq) => live.iter().position(|s| *s == seq),
                HotkeySource::App => app_window,
            };
            match index {
                Some(index) => turn.deliveries.push((index, delivery)),
                None => turn.undeliverable.push(delivery),
            }
        }
        for (index, seq) in live.iter().enumerate() {
            if manager.take_relayout(HotkeySource::Window(*seq)) {
                turn.relayout.push(index);
            }
        }
        turn
    }

    /// See [`GlobalHotkeyManager::take_relayout`].
    #[must_use]
    pub fn take_relayout(&self, source: HotkeySource) -> bool {
        self.lock().take_relayout(source)
    }

    /// See [`GlobalHotkeyManager::set_app_declarations`].
    pub fn set_app_declarations(
        &self,
        static_items: Vec<GlobalHotkeyCallbackData>,
        callback: Option<GlobalHotkeysCallback>,
        data: RefAny,
    ) {
        self.lock()
            .set_app_declarations(static_items, callback, data);
    }

    /// See [`GlobalHotkeyManager::mark_app_dirty`].
    pub fn mark_app_dirty(&self) {
        self.lock().mark_app_dirty();
    }

    /// Run the `AppConfig`'s hotkeys callback if it is due (at start, after
    /// the app state may have changed, or after a status it read moved) and
    /// declare the static list plus what it declared as the App's set.
    /// Returns whether it ran.
    ///
    /// Event-loop thread only, and never from inside a `layout()` call: the
    /// callback is app code and runs WITHOUT the manager's lock, recording
    /// into the same thread-local recorder `layout()` uses.
    #[must_use]
    pub fn refresh_app_declarations(&self) -> bool {
        let Some(job) = self.lock().take_app_job() else {
            return false;
        };
        let AppJob {
            callback,
            data,
            static_items,
            snapshot,
        } = job;
        drop(take_recorded_global_hotkeys());
        callback.invoke(data, GlobalHotkeysCallbackInfo::new(&snapshot));
        let recorded = take_recorded_global_hotkeys();
        if recorded.overflowed {
            azul_core::diagnostics::emit(alloc::format!(
                "[azul][warn] [global-hotkey] the AppConfig hotkeys callback declared more than \
                 {} global hotkeys; the rest were dropped",
                azul_core::global_hotkey::GLOBAL_HOTKEY_DECLARATION_CAP
            ));
        }
        let mut items = static_items;
        for item in recorded.declared {
            items.retain(|existing| existing.hotkey != item.hotkey);
            items.push(item);
        }
        self.lock()
            .declare(HotkeySource::App, items, recorded.read_status);
        true
    }
}

// ---------------------------------------------------------------------------
// One window's membership
// ---------------------------------------------------------------------------

/// A `LayoutWindow`'s place in its App's manager: the shared handle, and the
/// window's sequence number (its [`HotkeySource::Window`]).
///
/// Dropping it forgets the window's declaration - every close path ends in
/// the `LayoutWindow` being dropped, so no shell has to remember to - and the
/// next sync releases what only this window wanted.
#[derive(Debug)]
pub struct WindowHotkeys {
    shared: SharedGlobalHotkeys,
    seq: WindowSeq,
}

impl WindowHotkeys {
    /// A new window of the App behind `shared`.
    #[must_use]
    pub fn new(shared: SharedGlobalHotkeys) -> Self {
        Self {
            shared,
            seq: next_window_seq(),
        }
    }

    /// A new window of the App whose loop runs on this thread, or of a
    /// detached manager of its own (see [`SharedGlobalHotkeys::current_or_detached`]).
    #[must_use]
    pub fn for_current_app() -> Self {
        Self::new(SharedGlobalHotkeys::current_or_detached())
    }

    /// The App's handle.
    #[must_use]
    pub const fn shared(&self) -> &SharedGlobalHotkeys {
        &self.shared
    }

    /// This window's sequence number.
    #[must_use]
    pub const fn seq(&self) -> WindowSeq {
        self.seq
    }

    /// This window as a declaration source.
    #[must_use]
    pub const fn source(&self) -> HotkeySource {
        HotkeySource::Window(self.seq)
    }

    /// The statuses a `layout()` pass of this window reads (owner relative to
    /// this window), taken right before the pass.
    #[must_use]
    pub fn snapshot(&self) -> GlobalHotkeyInfoVec {
        self.shared.snapshot_for(self.source())
    }

    /// Hand what one `layout()` pass declared to the manager and bring the
    /// OS in line before returning. A status change asks the sources that
    /// read one to run again ([`SharedGlobalHotkeys::take_relayout`]).
    #[must_use]
    pub fn declare_recorded(
        &self,
        recorded: azul_core::global_hotkey::RecordedGlobalHotkeys,
    ) -> SyncOutcome {
        let mut manager = self.shared.lock();
        manager.declare(self.source(), recorded.declared, recorded.read_status);
        manager.sync()
    }

    /// This window took the keyboard focus (the owner rule prefers the most
    /// recently focused declarer).
    pub fn note_focus(&self) {
        self.shared.note_focus(self.seq);
    }
}

impl Drop for WindowHotkeys {
    fn drop(&mut self) {
        self.shared.forget_source(self.source());
    }
}

/// Run `f` - a fired global hotkey's callback - with `event` readable through
/// [`delivered_event`] (`CallbackInfo::get_global_hotkey_event`).
pub fn with_delivered_event<R>(event: GlobalHotkeyEvent, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<GlobalHotkeyEvent>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0;
            DELIVERING.with(|delivering| delivering.set(previous));
        }
    }
    let previous = DELIVERING.with(|delivering| delivering.replace(Some(event)));
    let _restore = Restore(previous);
    f()
}

/// The press whose callback runs right now on this thread; `None` outside a
/// global hotkey's own callback.
#[must_use]
pub fn delivered_event() -> Option<GlobalHotkeyEvent> {
    DELIVERING.with(core::cell::Cell::get)
}
