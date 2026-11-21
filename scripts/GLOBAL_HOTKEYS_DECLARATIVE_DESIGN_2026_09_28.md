# Global hotkeys, declared from state — design, 2026-09-28

The user's direction (verbatim):

> "for the global hotkeys we should do this in an arc or similar? Global hotkeys can be state
> dependent so it would make sense to expose some API on both the callbackinfo and layout callback
> info so that the current active global hotkeys can be derived from application state (similar
> api to dom::with_callback only on the layout callback info). And on the AppConfig in case its a
> headless app: there is a global-hotkeys crate."

This is a research and design report. No source file was edited and nothing was compiled. It
builds on the imperative implementation already on this branch (`2f13d67bb`, `0e99b14b2`,
`6e8d852cd`, `e60991e36`, uncompiled; its report is `scripts/GLOBAL_HOTKEYS_2026_09_28.md`).

---

## 0. Recommendation

1. **Declare, don't register.**
   - Add `LayoutCallbackInfo::add_global_hotkey(hotkey, data, callback)`. It mirrors `Dom::with_callback(event, data, callback)`, with the accelerator in the event filter's place.
   - Every `layout()` call declares the set its window wants now, derived from app state. After the call the engine reconciles the OS grabs against the union of all declarations:
     - register what is new;
     - release what is gone;
     - for an accelerator that stays, swap its callback and `RefAny` in place, with **no OS call**.
   - The declaration is recorded through a thread-local recorder drained right after `layout()` returns. That is the exact pattern of `depends_on_system_style` and `window_width_less_than` (`core/src/callbacks.rs:979-1257` and `:1399`, drained at `dll/src/desktop/shell2/common/layout.rs:497-526`). So the FFI-frozen, `Copy` `LayoutCallbackInfo` struct does not change.
2. **Identity = the normalised accelerator** (`GlobalHotkey`, struct equality).
   - It is what every OS keys a grab on.
   - It is what `with_callback` keys on (the event).
   - It is what Electron and Tauri key on.
   - It gives the Wayland portal a stable shortcut id across restarts.
3. **Apps without a window** declare through `AppConfig`:
   - a static list (`AppConfig::add_global_hotkey`) for the summon-key case;
   - an optional state-derived `GlobalHotkeysCallback` over the app's `RefAny`. It runs at start, after every callback that returned `RefreshDom*`, and when a status it read changes.
4. **"Arc or similar" → yes.** The registry becomes a `GlobalHotkeyManager` owned by the `App` and handed out as an `Arc<Mutex<…>>` handle (`SharedGlobalHotkeys`) to every window, the run loop and the tray-only stub. This is exactly how `SharedUndoManager` is threaded today (`common/event.rs:2544`).
   - OS threads and handlers never touch the manager.
   - They push into a separate small `Arc`'d **sink** (a mailbox plus a waker).
   - The process-global `static REGISTRY` goes away.
5. **`CallbackInfo` becomes read + nudge, not register.**
   - It keeps: `get_global_hotkey_status(hotkey)`, `get_global_hotkeys()`, `get_global_hotkey_event()` (inside a fired callback), `retry_global_hotkey(hotkey)` and `raise_window()`.
   - **Drop the imperative `register/unregister_global_hotkey`** (CallbackInfo and App). They are unreleased and not yet in `api.json`, so removing them costs nothing. Keeping them would make two sources of truth for one OS resource.
6. **Keep our own backends; do not depend on the `global-hotkey` crate** (§1).
   - It has no Wayland support, and its status model is synchronous-only.
   - It would add new crates plus a 4th `windows-sys` version, and hard-link Carbon/CoreGraphics.
   - The new backend seam is a trait, so the crate *could* be slotted in later as one backend if that ever pays.
7. **Delivery goes to the declaring window**, not "the first window".
   - Ties between windows go to the most recently focused declarer, then the oldest window.
   - App-level hotkeys run against the most recently focused window, or the headless stub in a tray-only app.
   - While reading, I found that "first window" is **arbitrary** today (§6.4).

---

## 1. The `global-hotkey` crate vs our backends

`global-hotkey` 0.8.0 (tauri-apps, released 2026-05-01; 0.7.0 was 2025-05-07; about 5.7 M downloads; MIT/Apache).

| | azul (on this branch) | `global-hotkey` 0.8.0 |
|---|---|---|
| **API** | a per-registration callback + `RefAny`; `GlobalHotkeyId` | `GlobalHotKeyManager::{new, register, unregister, register_all, unregister_all}`. Events come through ONE process-global channel (`GlobalHotKeyEvent::receiver()` or `set_event_handler`), `id` = a hash of the `HotKey` |
| **key model** | `VirtualKeyCode` + `HotkeyModifiers`; one `NAMED_KEYS` table feeds the parser, the display string, the xkb name and the portal trigger | `keyboard_types::Code` (W3C `code`, positional) + `Modifiers`. It would need a second mapping table |
| **macOS** | Carbon `RegisterEventHotKey` on the application target, HIToolbox **dlopen'd** (degrades to `Unavailable`); pressed only | Carbon `RegisterEventHotKey`, Carbon **linked** (`#[link(name="Carbon", kind="framework")]`); pressed **and released**. Media keys via `CGEventTapCreate` (needs Accessibility trust) |
| **Windows** | `RegisterHotKey` + an `HWND_MESSAGE` window created on the loop thread; `MOD_NOREPEAT`; pressed only | `RegisterHotKey` + a hidden `WS_EX_TOOLWINDOW\|LAYERED\|TRANSPARENT\|NOACTIVATE` window; `MOD_NOREPEAT`. Release comes from a thread polling `GetAsyncKeyState` every 50 ms |
| **X11** | `XGrabKey` on the root of a 2nd Xlib connection (dlopen'd), read on the loop thread. Lock-mask subsets include ScrollLock. `BadAccess` is caught | `x11rb` on a dedicated thread with its own connection. Requests go over a crossbeam channel with a synchronous reply. 4 lock-mask variants; a 50 ms sleep loop; press + release |
| **Wayland** | xdg-desktop-portal `GlobalShortcuts` via zbus (already a dll dep) | **none**. An X grab only sees XWayland-focused keys. Tracked in tauri-apps/global-hotkey#28; Tauri users run a fork for the portal |
| **async / approval** | `Pending` → `report()` → `Active` / `Failed` | none: `register` is `Result<(), Error>` right now |
| **threading** | register on the event-loop thread | "On Windows … create the manager on the same thread as the event loop"; "On macOS … on the main thread" |
| **new deps for azul** | none | `keyboard-types`, `xkeysym`, `windows-sys 0.59` (the lock already has 0.45 / 0.52 / 0.61), plus a static link to Carbon + CoreGraphics. `x11rb 0.13`, `crossbeam-channel`, `once_cell` and `thiserror 2` are already in `Cargo.lock` |

### Verdict: keep ours

- **The crate covers 3 of the 4 platforms.** The 4th, the portal, is the one whose semantics shape the architecture: asynchronous, user-approved, bind-once per session. We would ship our portal backend anyway, and the crate's model (a global event channel, hash ids, synchronous `Result`) would have to be wrapped to fit the declarative manager.
- **Supply chain.** It brings new crates plus another `windows-sys` major. Its link-time Carbon/CoreGraphics dependency contradicts the dlopen-and-degrade rule the rest of the dll follows (MediaPlayer, HIToolbox, user32, Xlib are all dlopen'd).
- **The backends are already written** (about 1,300 lines). They share the engine's keycode tables, so a hotkey and an in-window shortcut name the same physical key.
- **What to borrow from it:**
  - **Key release.** Carbon `kEventHotKeyReleased`, X11 `KeyRelease` (with detectable autorepeat), Win32 `GetAsyncKeyState` polling, and the portal's `Deactivated`. Push-to-talk needs it (open question Q5).
  - Its `x11rb`-thread shape, if the Xlib dlopen ever goes.

### Other toolkits (the shapes the design borrows)

| toolkit | shape | lesson |
|---|---|---|
| Electron `globalShortcut` | `register(accelerator, cb)`, `registerAll`, `isRegistered`, `unregister(accelerator)`, `unregisterAll`, `setSuspended`. "When the accelerator is already taken by other applications, this call will **silently fail**." Uses the portal on Wayland. | Identity = accelerator. Silent failure is the anti-pattern: we report `Failed(TakenByAnotherApp)` and feed it back into layout. |
| Tauri `global-shortcut` plugin | a wrapper over `global-hotkey`: `register`, `on_shortcut`, `unregister(_all)`, `is_registered`; events carry `ShortcutState::{Pressed, Released}` | Pressed/Released in one event type. |
| xdg-desktop-portal `GlobalShortcuts` (v2) | `CreateSession` → `BindShortcuts(session, [(id, {description, preferred_trigger})])`. "An application can only attempt to bind shortcuts of a session **once**." `ListShortcuts` returns "shortcuts that were successfully bound in a previous session by this application". Signals `Activated` / `Deactivated` / `ShortcutsChanged` (with `activation_token`). `ConfigureShortcuts` since v2. | **The whole set is declared up front and approved once, under stable ids.** A declarative model maps onto this directly; an imperative one-at-a-time model means one session and one dialog per hotkey (what the branch does now). |

---

## 2. How azul already derives UI from state (the patterns reused)

| mechanism | where | reused for |
|---|---|---|
| `layout(data, LayoutCallbackInfo) -> Dom`, re-run on `RefreshDom`, resize flips, theme, route | `core/src/callbacks.rs:872`, `common/layout.rs:236` `regenerate_layout` | the declaration point |
| **side-channel recorders** on the FFI-frozen `LayoutCallbackInfo`: `depends_on_system_style`, `window_width_less_than` → a thread-local, drained after the callback and stored on `LayoutWindow` (`recorded_size_queries`, `recorded_style_dependencies`) | `core/src/callbacks.rs:979-1257`, `common/layout.rs:497-526` | `add_global_hotkey` records the same way. A pass that skips `layout()` (resize fast path, restyle) keeps the last declaration, like `recorded_size_queries` |
| `Dom::with_callback(EventFilter, RefAny, callback)` → `CoreCallbackData { event, callback, refany }` | `core/src/dom.rs:3655`, `core/src/callbacks.rs:1867` | `GlobalHotkeyCallbackData { hotkey, description, callback, refany }` |
| precascade skip: an identical DOM keeps its retained `StyledDom` but gets the **fresh** callbacks/`RefAny`s transferred | `common/layout.rs:583-640` | an unchanged accelerator keeps its OS grab and gets the fresh callback |
| `<transient-window>`: `collect_open_transient_windows` → `TransientWindowManager::reconcile(wanted)` → a `TransientDiff` of opened/moved/closed, matched by source node so rebuilds don't flicker | `layout/src/transient.rs:909`, `common/layout.rs:2241` | the reconcile shape (wanted vs held, diff, no churn for survivors) |
| window menu from the root node's `menu_bar`, re-read after every relayout, hash-guarded against rebuilding the `NSMenu` | `macos/mod.rs:7317` `apply_menu_bar_from_dom` | "read after layout, only touch the OS when it changed" |
| tray: `App::set_tray` (static, applied at `run()`); clicks go into a process mailbox and run through `invoke_menu_callback` against "the first window", or the `HeadlessWindow` stub in `run_tray_only` | `dll/src/desktop/tray/mod.rs`, `run.rs:2771`, `run.rs:2899` | the delivery path and the tray-only stub |
| `SharedUndoManager`: an `Arc<Mutex<…>>` owned by the App and threaded into every window's `CommonWindowState` | `common/event.rs:2544`, `:2913`, `:3218` | ownership of the hotkey manager |
| `AppInternal.tray` doc: "Owned by the App rather than a process global: it is per-App state, and a global would silently pick the wrong one if a process ever ran two." | `dll/src/desktop/app.rs:589` | the argument against the current `static REGISTRY` |

---

## 3. The API

### 3.1 Core model (`core/src/global_hotkey.rs`)

`GlobalHotkey`, `HotkeyModifiers`, `GlobalHotkeyError`, the parser/validator/display, `NAMED_KEYS`, `xkb_keysym_name` and `portal_trigger` are **kept unchanged**. The additions:

```rust
/// One global hotkey a layout pass (or the app) wants, and what runs when it
/// fires. The global-hotkey twin of `CoreCallbackData` (`event` -> `hotkey`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GlobalHotkeyCallbackData {
    pub hotkey: GlobalHotkey,
    /// What the desktop shows for it (the Wayland portal's approval dialog and
    /// its shortcut settings). Empty = "<app name>: Ctrl+Alt+K".
    pub description: AzString,
    pub callback: CoreCallback,
    pub refany: RefAny,
}
impl_vec!(GlobalHotkeyCallbackData, GlobalHotkeyCallbackDataVec, /* … */);
impl_option!(GlobalHotkeyCallbackData, OptionGlobalHotkeyCallbackData, copy = false, [/* … */]);

/// Whose declaration a press of this accelerator runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum GlobalHotkeyOwner {
    /// `AppConfig`'s static list or its derived callback.
    App,
    /// The window whose `layout()` / `CallbackInfo` is asking.
    ThisWindow,
    /// Another window of this app (see the owner rule, §5.1).
    OtherWindow,
}

/// What an app can read about one accelerator.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct GlobalHotkeyInfo {
    pub hotkey: GlobalHotkey,
    pub status: GlobalHotkeyStatus,
    /// The trigger as the DESKTOP reports it. On Wayland the user may have
    /// picked another one in the portal's dialog (`trigger_description`);
    /// everywhere else, `hotkey.to_display_string()`.
    pub trigger: AzString,
    pub owner: GlobalHotkeyOwner,
}
impl_vec!(GlobalHotkeyInfo, GlobalHotkeyInfoVec, /* … */);

/// The press being delivered; readable from the fired callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct GlobalHotkeyEvent {
    pub hotkey: GlobalHotkey,
    pub state: GlobalHotkeyState, // Pressed (Released: Q5)
    /// Milliseconds on the backend's clock; 0 where the OS gives none.
    pub timestamp_ms: u64,
}
impl_option!(GlobalHotkeyEvent, OptionGlobalHotkeyEvent, [/* … */]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum GlobalHotkeyState { Pressed, Released }
```

**`GlobalHotkeyStatus`** keeps its four variants, re-documented per accelerator:
- `NotRegistered`: nobody declares it and no failure is remembered.
- `Pending`: the backend has not answered (the portal).
- `Active`.
- `Failed(e)`: **sticky** until a retry (§7).

**`GlobalHotkeyId` becomes internal.** It stays the OS-side handle (Carbon `EventHotKeyID.id`, the Win32 id, the portal shortcut's owner), but leaves the public API together with `OptionGlobalHotkeyId` and `ResultGlobalHotkeyIdGlobalHotkeyError`.

### 3.2 Layout side (`core/src/callbacks.rs`)

```rust
impl LayoutCallbackInfo {
    /// Declare that, in the state this layout is built from, `hotkey` should
    /// be a system-wide hotkey running `callback` with `data`.
    ///
    /// Declarations are the WHOLE wanted set: an accelerator this call does
    /// not declare (and no other window / the AppConfig declares) is released
    /// after the pass. Re-declaring an accelerator with a new callback or
    /// data swaps them without touching the OS. Declaring the same
    /// accelerator twice in one pass: the last declaration wins.
    pub fn add_global_hotkey<C: Into<CoreCallback>>(
        &self, hotkey: GlobalHotkey, data: RefAny, callback: C,
    );

    /// As above, with the text the desktop shows (Wayland asks the user with it).
    pub fn add_global_hotkey_with_description<C: Into<CoreCallback>>(
        &self, hotkey: GlobalHotkey, description: AzString, data: RefAny, callback: C,
    );

    /// Where `hotkey` stood when this pass began. RECORDED: a later status
    /// change (the grab answered, the user approved / declined, another app
    /// took it) re-runs this window's `layout()` once.
    #[must_use]
    pub fn get_global_hotkey_status(&self, hotkey: GlobalHotkey) -> GlobalHotkeyStatus;

    /// Every accelerator the app currently wants or failed to get, with
    /// status, trigger and owner. RECORDED like the one above.
    #[must_use]
    pub fn get_global_hotkeys(&self) -> GlobalHotkeyInfoVec;
}

/// What one `layout()` call declared (drained on the same thread right after it returns).
pub struct RecordedGlobalHotkeys {
    pub declared: alloc::vec::Vec<GlobalHotkeyCallbackData>,
    /// It read a status: re-run it when a status changes.
    pub read_status: bool,
    /// More than GLOBAL_HOTKEY_DECLARATION_CAP (256) declarations: the tail was dropped, loudly.
    pub overflowed: bool,
}
#[must_use]
pub fn take_recorded_global_hotkeys() -> RecordedGlobalHotkeys; // no_std: always empty
```

**Where the status getters read from.** `LayoutCallbackInfoRefData` grows one field, the same way the `monitors` snapshot works (azul-core is `no_std`, so it holds no `Mutex`):

```rust
pub struct LayoutCallbackInfoRefData<'a> {
    /* … */
    /// Snapshot of the app's global hotkeys, owner relative to THIS window,
    /// taken right before the layout call.
    pub global_hotkeys: GlobalHotkeyInfoVec,
}
```

It has five construction sites: `common/layout.rs:457`, `web/html_render.rs:670`, and three in `core/src/callbacks_test.rs`.

**C API:**

```c
AzLayoutCallbackInfo_addGlobalHotkey(&info, hotkey, AzRefAny_clone(&data), on_summon);
AzGlobalHotkeyStatus s = AzLayoutCallbackInfo_getGlobalHotkeyStatus(&info, hotkey);
```

### 3.3 App side, for apps with no window (`core/src/resources.rs` `AppConfig`)

```rust
pub struct AppConfig {
    /* … */
    /// App-level global hotkeys held whatever the state: a tray utility's
    /// summon key. Declared by the source `App` alongside the callback below.
    pub global_hotkeys: GlobalHotkeyCallbackDataVec,
    /// App-level global hotkeys DERIVED from the app's state (the app's
    /// `RefAny`). For apps that have no `layout()` - tray-only, background -
    /// or whose hotkeys belong to no window. See §5.3 for when it re-runs.
    pub global_hotkeys_callback: OptionGlobalHotkeysCallback,
}

impl AppConfig {
    pub fn add_global_hotkey<C: Into<CoreCallback>>(&mut self, hotkey: GlobalHotkey, data: RefAny, callback: C);
    pub fn with_global_hotkeys_callback(self, cb: GlobalHotkeysCallbackType) -> Self;
}

pub type GlobalHotkeysCallbackType = extern "C" fn(RefAny, GlobalHotkeysCallbackInfo);

#[repr(C)]
pub struct GlobalHotkeysCallback {
    pub cb: GlobalHotkeysCallbackType,
    pub ctx: OptionRefAny, // FFI host handle, as on LayoutCallback
}
impl_callback!(GlobalHotkeysCallback, GlobalHotkeysCallbackType);
impl_option!(GlobalHotkeysCallback, OptionGlobalHotkeysCallback, copy = false, [Debug, Clone]);

/// The same declaring vocabulary as `LayoutCallbackInfo`, without a window.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct GlobalHotkeysCallbackInfo {
    ref_data: *const GlobalHotkeyInfoVec, // the snapshot, owner relative to App
    callable_ptr: *const OptionRefAny,
    _abi_mut: *mut c_void,
}
impl GlobalHotkeysCallbackInfo {
    pub fn add_global_hotkey<C: Into<CoreCallback>>(&self, hotkey: GlobalHotkey, data: RefAny, callback: C);
    pub fn add_global_hotkey_with_description<C: Into<CoreCallback>>(&self, hotkey: GlobalHotkey, description: AzString, data: RefAny, callback: C);
    pub fn get_global_hotkey_status(&self, hotkey: GlobalHotkey) -> GlobalHotkeyStatus;
    pub fn get_global_hotkeys(&self) -> GlobalHotkeyInfoVec;
    pub fn get_ctx(&self) -> OptionRefAny;
}
```

**One recorder, two info types.** Both write into the same thread-local, so the drain after either is the same code. `layout()` has to return a `Dom`, so a recorder is the only channel it has. The app callback could return a `GlobalHotkeyCallbackDataVec` instead, but "declare with `add_global_hotkey` wherever you are handed an info" is one rule for every binding language.

**`App`:** `register_global_hotkey` / `unregister_global_hotkey` are removed. `App::create` no longer installs a backend (§4.1).

### 3.4 Event side (`layout/src/callbacks.rs` `CallbackInfo`)

```rust
impl CallbackInfo {
    /// Live status (not a snapshot) of `hotkey`, app-wide.
    #[must_use] pub fn get_global_hotkey_status(&self, hotkey: GlobalHotkey) -> GlobalHotkeyStatus;
    /// Live list, owner relative to this callback's window.
    #[must_use] pub fn get_global_hotkeys(&self) -> GlobalHotkeyInfoVec;
    /// `Some` only while a global hotkey's own callback runs: which accelerator,
    /// pressed or released, when. Lets one callback serve several accelerators.
    #[must_use] pub fn get_global_hotkey_event(&self) -> OptionGlobalHotkeyEvent;
    /// Forget `hotkey`'s remembered failure; the next sync asks the OS again
    /// (for Wayland: shows the portal dialog again). Queued as
    /// `CallbackChange::RetryGlobalHotkey { hotkey }`.
    pub fn retry_global_hotkey(&mut self, hotkey: GlobalHotkey);
    /// Unchanged.
    pub fn raise_window(&mut self);
}
```

**Re-derivation needs no new trigger.** A callback that changed the state hotkeys derive from returns `Update::RefreshDom` (or `RefreshDomAllWindows`). That re-runs `layout()` and, via §5.3, the app callback. An explicit `refresh_global_hotkeys()` that skips the relayout is open question Q7.

**Recommendation on the imperative escape hatch: none.**
- Every imperative use has a declarative spelling. "Let the user record a new shortcut" = store the accelerator in app state and declare it.
- A second, imperative source would need its own lifetime rules: which window owns it, what happens on close, whether the next layout that does not declare it releases it. Those rules are exactly the confusion the declarative model removes.
- If one is wanted later, it is an extra `HotkeySource::Imperative` whose set only callbacks mutate. The reconciler below takes any number of sources.

### 3.5 The demo, rewritten

```rust
// state: s.hotkey.enabled: bool, s.hotkey.fired: usize
// inside the showcase layout(data, info):
if s.hotkey.enabled {
    info.add_global_hotkey_with_description(
        demo_hotkey(), "Bring AzWidgets to the front".into(), data.clone(), on_hotkey);
}
let status = info.get_global_hotkey_status(demo_hotkey()); // re-runs layout when it moves
hotkey_section(&data, &s.hotkey, status) // Register/Unregister just flips `enabled` + RefreshDom

extern "C" fn on_toggle(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<Showcase>() else { return Update::DoNothing };
    s.hotkey.enabled = !s.hotkey.enabled;
    Update::RefreshDom
}
```

- The `id` / `status` / `error` strings the demo keeps in its state today disappear.
- The late-portal-answer staleness the first report lists in §7 ("the status text only refreshes when the section re-renders") goes away, because the status read triggers a relayout.
- A "Retry" button appears on `Failed(TakenByAnotherApp | Denied)` and calls `info.retry_global_hotkey(demo_hotkey())`.

---

## 4. Registry ownership and threading ("an Arc or similar")

### 4.1 The three options

| option | verdict |
|---|---|
| **A. process-global `static Mutex<GlobalHotkeyRegistry>`** (today) | It mirrors the OS (grabs are per process). But:<br>(1) every test in a binary shares it, so `layout/tests/global_hotkeys.rs` needs a `SERIAL` lock, and any e2e test that presses a hotkey without that lock races it;<br>(2) `App::create` installs the REAL backend, so `App::register_global_hotkey` before `run()` grabs at the OS, and on Wayland starts a portal handshake, even for a run that then turns out headless (it is moved to the simulation afterwards);<br>(3) nothing ever releases it (no owner to drop);<br>(4) it contradicts `AppInternal`'s own rule for the tray. |
| **B. `Arc<Mutex<GlobalHotkeyManager>>` owned by the App, cloned into every window, the loop and the stub; OS threads only get a separate `Arc`'d sink** | **Recommended.** It matches `SharedUndoManager`. Each test builds its own manager with its own fake backend, so the tests need no global lock. The backend is chosen once, in `run()`, before anything is declared. Dropping the App drops the backend, which releases every grab (and closes portal sessions). |
| **C. owned by the run loop only** | Callbacks and `layout()` could not read statuses without new plumbing through every shell. |

`Rc<RefCell<…>>` would be enough in practice, because windows and the loop live on one thread. It is still rejected: `LayoutWindow` and `RefAny` are `Send` by contract (`core/src/refany.rs:702`), and `Arc<Mutex>` is the house style for app-wide handles.

### 4.2 The pieces (`layout/src/managers/global_hotkey.rs`)

```rust
/// Which declaration set: the AppConfig's, or one window's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HotkeySource { App, Window(WindowSeq) }

/// Creation order of a LayoutWindow: monotonic, never reused (core already has an
/// unused `azul_core::window::WindowId` atomic counter that can serve).
pub type WindowSeq = u64;

/// The OS half, owned by the manager. `&mut self`, so each backend keeps its own
/// state and its own sink (no statics for fires); first-party only.
pub trait GlobalHotkeyBackend: Send {
    fn name(&self) -> &'static str;
    fn probe(&self) -> Result<(), String>;
    /// Grab `hotkey`; report later answers / fires under `os_id` through the sink.
    fn register(&mut self, os_id: GlobalHotkeyId, hotkey: &GlobalHotkey, description: &str)
        -> Result<BackendGrant, GlobalHotkeyError>;
    fn unregister(&mut self, os_id: GlobalHotkeyId);
    /// End of one reconcile batch. The portal binds every `register` of the batch here,
    /// in ONE session (one approval dialog); grab backends do nothing.
    fn commit(&mut self) {}
    /// Per-iteration work on the loop thread (X11 reads its connection).
    fn poll(&mut self) {}
    /// An fd the Linux loops should add to their poll set, instead of polling.
    fn wake_fd(&self) -> Option<i32> { None }
}

/// What backends (any thread) tell the loop.
pub enum BackendEvent {
    Fired { os_id: GlobalHotkeyId, state: GlobalHotkeyState, timestamp_ms: u64 },
    Settled { os_id: GlobalHotkeyId, result: Result<AzString /* trigger */, GlobalHotkeyError> },
    TriggerChanged { os_id: GlobalHotkeyId, trigger: AzString }, // portal ShortcutsChanged
}

/// The only thing an OS thread / handler ever holds. Leaf lock; bounded.
#[derive(Clone)]
pub struct HotkeySink { inner: Arc<SinkInner> } // Mutex<VecDeque<BackendEvent>> + Option<waker>
impl HotkeySink {
    pub fn push(&self, ev: BackendEvent); // drops Fired beyond MAX_PENDING_FIRES (64)
    pub fn set_waker(&self, waker: Arc<dyn Fn() + Send + Sync>);
}

/// Everything else: declarations per source, the grabs held, statuses, remembered
/// failures, focus stamps. A plain value - tests drive it directly.
pub struct GlobalHotkeyManager { /* see §5 */ }

/// The App's handle; cloned into CommonWindowState + LayoutWindow + the tray-only stub.
#[derive(Clone)]
pub struct SharedGlobalHotkeys { manager: Arc<Mutex<GlobalHotkeyManager>>, sink: HotkeySink }
```

### 4.3 Thread rules

| who | thread | touches |
|---|---|---|
| `layout()` / the app callback | the event-loop thread | the thread-local recorder only |
| `declare`, `sync`, `take_deliveries`, status getters | the event-loop thread | the manager lock. `sync` calls the backend UNDER the lock, which is safe because no backend call runs user code or blocks on the sink. A `debug_assert!` checks the thread id captured at `install_backend`. |
| Carbon handler | main thread, inside `[NSApp sendEvent:]` | the sink, reached through `InstallEventHandler`'s `user_data` (`Arc::into_raw`), not a static |
| Win32 `WM_HOTKEY` wndproc | the loop thread, inside `DispatchMessageW`. **Must be the thread that registered.** It is, because every `sync` runs on the loop thread. | the sink, via `GWLP_USERDATA` |
| X11 | `poll()` on the loop thread (unchanged) | the sink |
| portal handshake + listener threads | their own threads | the sink, plus a clone of the zbus connection |
| user callbacks | the loop thread, **never under the manager lock**: deliveries are cloned out first (as `take_fired` does today) | anything |

**Wakers** remove the polling the branch needs today:
- **Linux / portal:** `signal_wake_fd` on an eventfd in the poll set. This is the `linux/timer.rs:186` `new_wake_fd` pattern the theme watcher uses. It replaces the 100 ms park cap.
- **X11:** `ConnectionNumber()` of the grab connection goes into the poll set.
- **Headless:** a condvar notify replaces the 60 Hz poll while a hotkey is held.
- **macOS and Win32:** the OS wakes the loop already.

---

## 5. Reconciliation, multi-window and headless rules

### 5.1 State and the owner rule

```text
GlobalHotkeyManager {
  backend:   Option<Box<dyn GlobalHotkeyBackend>>
  declared:  BTreeMap<HotkeySource, Declared>        // last declaration per source
             Declared { items: BTreeMap<GlobalHotkey, GlobalHotkeyCallbackData>, read_status: bool }
  held:      BTreeMap<GlobalHotkey, Held>            // what we asked the OS for
             Held { os_id, status: Pending|Active, trigger: AzString }
  failures:  BTreeMap<GlobalHotkey, GlobalHotkeyError> // sticky, declared or not (§7)
  retry:     BTreeSet<GlobalHotkey>
  focus:     BTreeMap<WindowSeq, u64>                // last-focus stamps
  next_os_id: u32, dirty: bool, generation: u64
}

pick_owner(hk):                                     // resolved at FIRE time, never cached
  windows = { w | declared[Window(w)] has hk }
  if windows nonempty:
      return argmax over w of (focus[w] or 0, -w)    // most recently focused, then OLDEST window
  if declared[App] has hk: return App
  none
```

**Multi-window rules:**
- **One grab per accelerator.** The OS set is the **union** over all sources, so N windows running the same `layout()` share one grab.
- **Exactly one callback runs per press**, never one per declarer. Otherwise a summon key in a 3-window app would run three times.
- **Which one:** a window's declaration shadows the App's (the more specific context wins). Among windows, the most recently focused declarer wins, then the oldest window.
  - "Most recently focused" is deterministic given the focus history, which the E2E harness controls.
  - It matches what the user expects: the hotkey acts on the window they last worked in.
  - The stamp is written by the focus-in path of each shell (`manager.note_focus(seq)`), which is cheap.
- **Duplicates inside one source:** the last declaration wins, and a debug log names both.
- **A window that closes:** a guard on `LayoutWindow` calls `forget_source(Window(seq))` on drop. That covers every close path. The next `sync` releases what only that window wanted; the grab survives if another source still wants it.

### 5.2 Declare + sync (the algorithm)

```text
// ── in regenerate_layout, right after the user callback (common/layout.rs:497-526) ──
clear_recorder()                                           // like the size-query recorder
ref_data.global_hotkeys = shared.snapshot_for(Window(seq)) // statuses BEFORE this pass
dom = layout(app_data, info)
rec = take_recorded_global_hotkeys()
shared.declare(Window(seq), rec.declared, rec.read_status)
out = shared.sync()                                        // OS == union before we return
for src in out.relayout: request_regeneration(src, RelayoutReason::Other)  // bounded, see §7
// A pass that does NOT call layout() (resize fast path, restyle-only, E2E mount
// override) declares nothing new: the source keeps its last declaration.

declare(source, items, read_status):
    new = dedupe(items)                        // last duplicate wins
    old = declared.get(source)
    if keys(new) != keys(old) or descriptions differ: dirty = true
    declared[source] = Declared { items: new, read_status }  // callbacks/RefAnys swapped HERE, no OS call

sync() -> SyncOutcome:
    changed = false
    // 1. what backends reported since the last sync (any thread -> sink); fires stay queued
    for ev in sink.drain_non_fires():
        Settled{os_id, Ok(trigger)} -> held[hk].status = Active; held[hk].trigger = trigger; changed
        Settled{os_id, Err(e)}      -> held.remove(hk); failures[hk] = e; changed
        TriggerChanged{os_id, t}    -> held[hk].trigger = t; changed
    if !dirty and retry.is_empty(): goto 5
    dirty = false
    wanted = union over declared[*].items.keys(), with the owner's description
    // 2. release first, so an A->B swap inside one batch never holds both
    for hk in held.keys() - wanted:
        backend.unregister(held[hk].os_id); held.remove(hk); changed
    // 3. grab what is new, or asked to retry; everything kept is untouched
    for hk in wanted - held.keys():
        if failures.contains(hk) and !retry.contains(hk): continue   // sticky
        retry.remove(hk); failures.remove(hk)
        match backend:
          None    -> failures[hk] = Unsupported; changed; continue
          Some(b) -> id = next_os_id()             // never reused, never 0
                     match b.register(id, hk, description(hk)):
                       Ok(Active)  -> held[hk] = Held{id, Active,  display(hk)}
                       Ok(Pending) -> held[hk] = Held{id, Pending, display(hk)}
                       Err(e)      -> failures[hk] = e
                     changed
    backend.commit()                           // portal: ONE session, ONE dialog for the batch
    // 5. who has to hear about it
    if changed: generation += 1
    return SyncOutcome { relayout: if changed { sources with read_status } else { [] } }

take_deliveries() -> Vec<HotkeyDelivery>:      // loop thread; lock released before invoking
    for Fired{os_id, state, t} in sink.drain_fires():
        hk = held.find_by_os_id(os_id) else continue    // released meanwhile: drop
        if held[hk].status == Pending: held[hk].status = Active  // portal fired before its Settled
        owner = pick_owner(hk) else continue
        d = declared[owner].items[hk]
        push HotkeyDelivery { target: owner,
                              callback: CoreMenuCallback { refany: d.refany.clone(), callback: d.callback.clone() },
                              event: GlobalHotkeyEvent { hotkey: hk, state, timestamp_ms: t } }
```

**Properties the tests pin** (§8):
- An identical re-declaration makes **zero** backend calls.
- A callback or data swap makes zero backend calls, and the next press runs the new callback.
- A release comes before a grab within one batch.
- A failure is not re-asked until a retry.
- The first press after a swap goes to the current owner.

### 5.3 The run-loop pump (every backend's loop, the headless Phase 1c, the tray-only timer)

```text
pump():
    shared.poll_backend()                                   // X11 reads its connection
    if app_hotkeys_dirty:                                   // see "when it re-runs"
        rec = run AppConfig.global_hotkeys_callback(app_data, info{snapshot_for(App)})
        shared.declare(App, config.global_hotkeys ++ rec.declared, rec.read_status)
    out = shared.sync()
    for src in out.relayout: Window(w) -> request_regeneration(w); App -> app_hotkeys_dirty = true
    for d in shared.take_deliveries():
        window = match d.target {
            Window(w) -> the live window with seq w,
            App       -> the most recently focused window, else the OLDEST,
                         else the tray-only HeadlessWindow stub, else: log + drop }
        r = window.invoke_global_hotkey_callback(d.callback, d.event)  // invoke_menu_callback + event context
        if d.target == App and r regenerates: request_regeneration_all_windows(); app_hotkeys_dirty = true
```

**When the `AppConfig` callback re-runs** (`app_hotkeys_dirty` is set):
1. Once at `run()` / `run_tray_only()` / `run_headless()`, after the backend is installed and before the loop parks.
2. After any callback returns `Update::RefreshDom` or `RefreshDomAllWindows` (window, tray, notification, timer, thread or hotkey callback). That is the only "app state may have changed" signal azul has, and the callback is tiny.
3. When a status it **read** changes.

It does **not** re-run on resize or theme changes: app-level hotkeys depend on no window.

### 5.4 Headless, tray-only, and apps with no window

| run | what declares | backend | a fired callback runs against |
|---|---|---|---|
| desktop, windows | each window's `layout()` + `AppConfig` | the platform's, installed in `run()` | the owner window; App-owned → the last-focused window |
| `run_tray_only` (macOS) | `AppConfig` only (the stub's default layout declares nothing) | the platform's | the `HeadlessWindow` stub (as tray clicks already do, `run.rs:2899`) |
| `RunForever` with every window closed | `AppConfig` (window sources were forgotten on close) | the platform's | the stub. Today such a fire is logged and dropped (`run.rs:2793`). Recommend a lazily created app stub, shared with tray and notifications (Q9). |
| `AZ_BACKEND=headless` / `AZ_E2E` | the headless window's `layout()` + `AppConfig` | the **simulation**, installed before the first layout, so nothing is ever grabbed at the OS (fixes trap 2 in §4.1) | the headless window |
| layout-crate tests / `layout/src/e2e/runner.rs` | the `LayoutWindow`'s own `SharedGlobalHotkeys::detached()` (default: simulated backend) | simulated | that `LayoutWindow` — which closes the "layout-level runner does not pump hotkeys" gap |
| web / iOS / Android | declarations are accepted | none | nothing. Every declared accelerator reads `Failed(Unsupported)`, so the API is the same everywhere |

**E2E.** The `global_hotkey` op keeps its JSON and now presses the **declared** set:
- `{"op":"global_hotkey","accelerator":"Ctrl+Alt+K"}` → `shared.simulate(&hk)` pushes a `Fired` into the sink, and the pump delivers it on the next iteration.
- When nothing declares the accelerator, the error lists the effective set with status and owner.

New ops, so status feedback is testable headless:
- `{"op":"global_hotkey_answer","accelerator":"Ctrl+Alt+K","answer":"pending|active|taken|denied|unsupported"}` programs the simulated backend's next answer.
- `{"op":"global_hotkey_settle","accelerator":"Ctrl+Alt+K","result":"active|denied"}` plays the portal's late `Response`.
- `{"op":"assert_global_hotkeys","expect":[{"accelerator":"Ctrl+Alt+K","status":"active","owner":"window"}]}`.

**Manager-accounting gates.** The `UNOBSERVABLE_MANAGERS` and `not_fingerprintable()` reasons for `global_hotkey` (`layout/src/e2e/full.rs:7715`, `:8836`) become false, because the window now owns a declaration. That declaration (accelerators + statuses) is fingerprintable, so `global_hotkey` moves to the fingerprinted managers. X10 can then check "every accelerator this window declares is in the effective set".

---

## 6. Status feedback

### 6.1 Lifecycle (all platforms)

| event | status | who hears it | when |
|---|---|---|---|
| pass N declares K for the first time | snapshot (taken before the pass) says `NotRegistered`; after `sync`: `Active` / `Pending` / `Failed(e)` | windows (and the App callback) whose last pass **read** a status | pass N+1, requested by `sync`'s outcome. At most one extra pass: pass N+1 declares the same set, so nothing changes. |
| Carbon / Win32 / X11 refuse | `Failed(TakenByAnotherApp \| KeyNotMappable \| Platform)` synchronously | same | same |
| portal: bind queued | `Pending` | same | pass N+1 shows "waiting for the desktop" |
| portal: user accepts | `Active`, `trigger` = the portal's `trigger_description` | a `Settled` event on the handshake thread → sink → waker → pump `sync` | the next pass |
| portal: user cancels | `Failed(Denied)`, sticky | same | the next pass; the app can offer "Retry" |
| portal: user rebinds in system settings | `Active`, new `trigger` | a `TriggerChanged` event from the `ShortcutsChanged` listener | the next pass |
| K no longer declared | `NotRegistered`, unless a failure is remembered (then `Failed(e)`) | status readers | the next pass |

### 6.2 The getters

- **In `layout()` or the app callback:** `get_global_hotkey_status(hk)` / `get_global_hotkeys()` read the pass snapshot, and **record** that a status was read.
- **In event callbacks:** `CallbackInfo::get_global_hotkey_status(hk)` / `get_global_hotkeys()` read the live manager.

### 6.3 Why failures are sticky, declared or not

Take an app that falls back when its first choice is taken ("Ctrl+Alt+K is taken → use Ctrl+Alt+J"):

- With failures forgotten on undeclare, it loops forever:
  1. K fails.
  2. The next pass declares J and drops K.
  3. K reads `NotRegistered`.
  4. The next pass declares K again, which fails again, and so on.
- With a sticky failure cache the fallback converges in one extra pass.

Failures are cleared only by:
- `retry_global_hotkey(hk)`;
- a backend change (`install_backend`).

Two further safety nets:
- **Portal:** stickiness also guarantees that a declined dialog is never re-shown by an ordinary relayout.
- **Loop guard:** at most 2 status-driven relayouts per window per loop turn; beyond that, `plog_warn!` names the window.

### 6.4 Side findings while reading (worth fixing regardless)

1. **"The first window" is arbitrary.** It is used today for hotkeys, tray clicks and notifications.
   - macOS keys its registry by `NSWindow` pointer in a `BTreeMap` (address order). The `all_ns_windows` doc says "creation order", which is wrong.
   - Linux uses a `HashMap`.
   - Windows uses a `BTreeMap` by `HWND` value.
   - So in a multi-window app, `pump_into_first_*` runs a hotkey (and a tray click) against an effectively random window.
2. **Registering before `run()` grabs at the real OS**, even in headless or CI runs. `App::create` installs the platform backend. On a Wayland desktop this starts a portal handshake thread, and possibly a dialog, before `run_headless` swaps in the simulation.
3. **Portal shortcut ids are `azul-hotkey-{counter}`** (`portal.rs:176`). They depend on registration order and change across runs. The portal remembers bound shortcuts per app (`ListShortcuts`), so each launch looks like new shortcuts: fresh dialogs and stale entries. The bind `Response`'s `trigger_description` is discarded.
   - **Fix:** use the canonical accelerator (`portal_trigger(hk)`) as the id, plus one session per reconcile batch.
4. **The branch's design reserves one portal session per hotkey.** That means one dialog per hotkey at startup. With batching, N declared hotkeys cost one dialog.
5. **macOS: `F13`–`F20` read `KeyNotMappable`.**
   - `keycode_of` inverts `macos_keycode_to_virtual_key`, whose table stops at `F12`.
   - `kVK_F13`–`kVK_F20` are `0x69 0x6B 0x71 0x6A 0x40 0x4F 0x50 0x5A`.
   - `F13` is the parse doc's own example and a common summon key.
   - Media keys also cannot be Carbon hot keys (they are `NX_SYSDEFINED` events, not keycodes), although `validate()` lets them stand alone.
   - Extending the table touches `keycode_table_manifest_is_exhaustive`.
6. **Portal removal is coarse.** "An application can only attempt to bind shortcuts of a session once": a shortcut cannot be unbound from a multi-shortcut session.
   - When every shortcut of a session is undeclared: `Session.Close`.
   - When only some are: tombstone them (drop their `Activated`), and fold the survivors into the next batch's new session (compaction).
   - This lives inside the portal backend's `unregister` / `commit`; the manager does not see it.

---

## 7. Mapping onto the code on this branch

| stays as written | changes | goes |
|---|---|---|
| `core/src/global_hotkey.rs`: `GlobalHotkey`, `HotkeyModifiers`, the parser / normaliser / validator, display, `NAMED_KEYS`, `xkb_keysym_name`, `portal_trigger`, `GlobalHotkeyError`, `GlobalHotkeyStatus` | `GlobalHotkeyId` → internal only; + `GlobalHotkeyCallbackData(Vec)`, `GlobalHotkeyInfo(Vec)`, `GlobalHotkeyOwner`, `GlobalHotkeyEvent`, `GlobalHotkeyState` | `OptionGlobalHotkeyId`, `ResultGlobalHotkeyIdGlobalHotkeyError` |
| the OS calls in `macos.rs` / `windows.rs` / `x11.rs` / `portal.rs` (Carbon, `RegisterHotKey` + `HWND_MESSAGE`, `XGrabKey` + lock masks + error handler, the zbus handshake) | fn-pointer table → `GlobalHotkeyBackend` trait. Static `push_fired` / `report` → the instance's `HotkeySink` (Carbon `user_data`, `GWLP_USERDATA`). Portal: batch in `commit`, stable ids, `trigger_description`, `ShortcutsChanged`, tombstones. Wake fds. | `static HOT_KEYS` / `STATE` maps that only exist for id lookup can move into the backend struct |
| the simulated backend | + a programmable answer table (for the e2e ops) + a condvar waker | `needs_loop_polling` (replaced by wakers) |
| `layout/src/managers/global_hotkey.rs`: the mailbox bound (64), never-reused ids, "backend without the registry" discipline, `simulate` | `GlobalHotkeyRegistry` → `GlobalHotkeyManager` (§5). Process functions → `SharedGlobalHotkeys` methods. | `static REGISTRY`, `install_backend_if_none`, `is_registered`, `register` / `unregister` / `begin_register` / `finish_register` |
| `dll/src/desktop/global_hotkey/mod.rs`: `platform_backend()` (session-based choice), the pump slots in `run.rs` (`:999`, `:1261`, `:1838`, `:2421`, `:2993`) and the headless Phase 1c | pumps route by owner (§5.3). Install moves from `App::create` to `run()`. `probe()` no longer installs. | `pump_into_first_*` (renamed to `pump`) |
| `CallbackInfo::raise_window`, `CallbackChange::RaiseWindow`, `extra::window_activation` | + `get_global_hotkeys`, `get_global_hotkey_event`, `retry_global_hotkey` (+ `CallbackChange::RetryGlobalHotkey`); `get_global_hotkey_status` keyed by accelerator | `CallbackInfo::{register,unregister}_global_hotkey`, `App::{register,unregister}_global_hotkey` |
| `PlatformCapability::global_hotkeys()` | pure probe, no install side effect | — |
| `doc/src/autofix/module_map.rs` fix (`GlobalHotkey*` → `app`, not `gl`) | + the new types | — |
| e2e `global_hotkey` op | presses the declared set; + `global_hotkey_answer`, `global_hotkey_settle`, `assert_global_hotkeys`; gate reasons updated | — |
| `examples/azul-widgets/src/hotkeys.rs` layout of the section | the state becomes `enabled` + `fired`; declared in `layout()` (§3.5) | the id / status / error bookkeeping |

**New plumbing:**
- `AppInternal.global_hotkeys: SharedGlobalHotkeys`;
- `CommonWindowState.global_hotkeys` (passed like `undo_manager`);
- `LayoutWindow.global_hotkeys` + `LayoutWindow.hotkey_source: WindowSeq` + a drop guard;
- `LayoutCallbackInfoRefData.global_hotkeys`;
- the recorder in `core/src/callbacks.rs`;
- `AppConfig.global_hotkeys` / `global_hotkeys_callback`;
- `note_focus` calls in each shell's focus-in path.

---

## 8. Migration plan (ordered commits, RED first)

The repo's convention applies: a `test(...)` commit that is RED against stubs, then the `feat` / `fix` that turns it green. Nothing is compiled per item: rust-analyzer only, one compile at the end.

**1. `test(hotkeys): a declared set reconciles against the backend without churn` — RED.**
- File: rewrite `layout/tests/global_hotkeys.rs` around `GlobalHotkeyManager` and a `RecordingBackend { log: Arc<Mutex<Vec<String>>>, answers }`. Per-instance state, so no `SERIAL` lock.
- The parser tests carry over unchanged.
- Rows:
  - `declaring_registers_each_new_accelerator_once`
  - `an_identical_redeclaration_calls_nothing` (**the core property**)
  - `a_new_callback_for_a_kept_accelerator_swaps_without_a_backend_call_and_the_next_press_runs_it`
  - `an_undeclared_accelerator_is_released_and_its_queued_press_is_dropped`
  - `a_release_comes_before_a_grab_in_one_batch` (A→B)
  - `two_windows_share_one_grab` / `forgetting_one_keeps_it` / `forgetting_both_releases_it`
  - `the_most_recently_focused_declarer_runs` / `an_unfocused_tie_goes_to_the_oldest_window`
  - `a_window_declaration_shadows_the_app_one`
  - `a_failure_is_sticky_until_a_retry` (Taken → re-declare → 0 calls → `retry` → 1 call)
  - `a_fallback_after_a_failure_converges` (K taken → declare J → K still reads `Failed`)
  - `without_a_backend_every_declaration_reads_unsupported_and_nothing_is_called`
  - `declarations_before_a_backend_wait_for_it` (install later → registered then, never before)
  - `a_pending_grab_settles_from_another_thread_through_the_sink` (a spawned thread pushes `Settled`; `sync` → `Active`; the outcome relayouts only status readers)
  - `one_batch_commits_once`
  - `a_stuck_sender_cannot_grow_the_sink`
  - `a_simulated_press_reaches_the_current_owner`
- Stubs: `sync` returns the default, `take_deliveries` is empty, `status` is `NotRegistered`.

**2. `feat(hotkeys): an App-owned GlobalHotkeyManager, a HotkeySink and SharedGlobalHotkeys replace the process-global registry`.**

**3. `test(core): add_global_hotkey is recorded per layout call` — RED** (`core/src/callbacks_test.rs`, next to the style-dependency recorder tests at `:1650-1710`):
- one call's declarations drain once, and a second drain is empty;
- the last duplicate wins;
- `get_global_hotkey_status` answers from the ref-data snapshot and sets `read_status`;
- `get_global_hotkeys` reports the `owner` relative to the window;
- the cap latches `overflowed`.

**4. `feat(core): LayoutCallbackInfo declares global hotkeys (recorder, snapshot, GlobalHotkeyCallbackData / GlobalHotkeyInfo)`.**

**5. `test(hotkeys): a layout pass keeps the grabs in sync, headless` — RED.**
- File: dll headless tests. A `HeadlessWindow` whose `layout()` declares K while `state.enabled`; the simulated backend's log is read back.
- Rows:
  - `the_first_layout_grabs_what_it_declares`
  - `a_refresh_that_stops_declaring_releases`
  - `a_layout_that_is_skipped_keeps_the_grab` (resize fast path: `layout()` is not invoked)
  - `closing_the_window_releases_what_only_it_declared`
  - `a_status_read_costs_exactly_one_extra_layout` (a layout counter reads 2, not 3+)

**6. `feat(hotkeys): regenerate_layout declares into the App's manager; windows get a source id and release on drop`.**

**7. `test(app): AppConfig hotkeys` — RED.**
- the static list is grabbed at `run_headless` start with no window layout;
- the derived callback re-runs after a `RefreshDom` from any callback, and after a status change it read;
- a tray-only-style run (headless stub, no layout) delivers a press.

**8. `feat(app): AppConfig::global_hotkeys + global_hotkeys_callback, evaluated by run / run_tray_only / run_headless; backend installed in run(), not App::create`.**

**9. `test(hotkeys): presses run against their owner` — RED.**
- two headless windows declare K: the press runs against the focused one;
- an App-level press that returns `RefreshDom` regenerates every window;
- `get_global_hotkey_event()` is `Some` only inside the fired callback.

**10. `feat(hotkeys): pumps deliver to the owner window (no more "first window")`.**

**11. `refactor(hotkeys): backends report through their sink; the portal binds one session per batch under stable ids`.**
- Platform code has no unit tests: the manual recipes of the first report §5 apply.
- The portal's batching and tombstone planner can be split into a pure function with its own unit test first.
- Also in this commit: the Linux wake fd, the X11 fd in the poll set, and `trigger_description` / `ShortcutsChanged`.

**12. `refactor(api): drop imperative register/unregister and GlobalHotkeyId from the public API; CallbackInfo reads statuses by accelerator, get_global_hotkeys, get_global_hotkey_event, retry_global_hotkey`.**
- RED first: `callback_info_flag_mutators_queue_exactly_one_matching_change` gains a `retry_global_hotkey` row.

**13. `test(e2e)` + `feat(e2e)`.**
- The `global_hotkey` op presses the declared set.
- Adds `global_hotkey_answer` / `global_hotkey_settle` / `assert_global_hotkeys`.
- The manager-accounting gates fingerprint the window's declaration.
- A scenario for the demo: register → press → counter 1 → `answer: taken` → status line reads "taken" → Retry.

**14. `feat(examples): the Global hotkey section declares from state` (§3.5).**

**15. `chore`:** `api.json` via `azul-doc autofix` only (never hand-curated); this report's successor.

**Optional:** `fix(macos): kVK_F13…F20 in the keycode table` (§6.4 item 5), RED on `F13` parsing and mapping to a Carbon keycode.

---

## 9. Open questions for the user

1. **Identity.**
   - Recommended: the accelerator.
   - Alternative: an app-chosen id (`"summon"`) + description + preferred accelerator. That is the portal's own model, and it would let a Wayland user's rebind and an in-app "change shortcut" setting refer to one stable thing. The cost is a naming burden on every declaration.
   - Adding it later as an optional field changes what the reconciler keys on, so decide before the API lands.
2. **Owner rule between windows.** The most recently focused declarer (recommended) or strictly the oldest window (simpler, focus-independent)?
3. **Imperative API.**
   - Recommended: drop `register/unregister_global_hotkey` entirely.
   - Alternatively: keep it as an `Imperative` source that only callbacks mutate and nothing releases implicitly.
4. **Description.** Optional, as proposed, or mandatory? Wayland shows it to the user in the approval dialog and in system settings. The fallback "<app>: Ctrl+Alt+K" is poor there.
5. **Key release (push-to-talk).**
   - Deliver `Released` too, via `get_global_hotkey_event().state`?
   - It needs Carbon `kEventHotKeyReleased`, X11 `KeyRelease`, a Win32 `GetAsyncKeyState` polling thread (what `global-hotkey` does) and the portal's `Deactivated`.
   - Per declaration opt-in (`wants_release`) or always?
6. **Windowless apps beyond macOS tray-only.**
   - A hotkey-only daemon (no tray, no window) has no run mode today, and `run_tray_only` is macOS-only.
   - Add `App::run_background()` (runs while the AppConfig declares something)?
7. **Re-derivation trigger.** Re-run the `AppConfig` callback after *every* `RefreshDom*` (proposed; cheap), or only on an explicit `CallbackInfo::refresh_global_hotkeys()`?
8. **`RelayoutReason`.** Append a `GlobalHotkeyStatus` variant (an ABI-appended enum, like the others) so `layout()` can tell a status-driven pass apart, or reuse `Other`?
9. **App-level delivery target for the tray and notifications.** Adopt the same "last-focused window, else a lazily created app stub" routing? It fixes the arbitrary "first window" (§6.4 item 1) for all three, and makes `RunForever` with zero windows deliver instead of logging and dropping.
10. **A DOM route for widgets.** Widgets' `.dom()` builders never see `LayoutCallbackInfo`. Should a widget be able to declare a hotkey, e.g. a `Dom::with_global_hotkey(...)` collected after layout like `menu_bar`? That would be a third source kind (per node, transferred by the precascade skip like callbacks).
11. **Portal extras.** Expose `ConfigureShortcuts` (portal v2) as `CallbackInfo::configure_global_hotkeys()` (it opens the desktop's shortcut settings)? Use `Activated`'s `activation_token` for an `xdg_activation`-based `raise_window()` on Wayland?

Sources: [global-hotkey docs.rs](https://docs.rs/global-hotkey/latest/global_hotkey/), [global-hotkey source](https://github.com/tauri-apps/global-hotkey) (`Cargo.toml`, `src/platform_impl/{macos,windows,x11}`), [global-hotkey#28 Wayland](https://github.com/tauri-apps/global-hotkey/issues/28), [xdg-desktop-portal GlobalShortcuts](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html), [Electron globalShortcut](https://www.electronjs.org/docs/latest/api/global-shortcut), [Tauri global-shortcut plugin](https://v2.tauri.app/plugin/global-shortcut/), crates.io API for release dates.
