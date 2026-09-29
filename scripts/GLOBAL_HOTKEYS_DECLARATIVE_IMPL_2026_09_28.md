# Global hotkeys, declared from state — implementation, 2026-09-28

Branch `wt/hotkeys-declarative`, based on `5414bfa6b` (PR #476, `fix/input-bugs-2026-09-19`).
Implements `scripts/GLOBAL_HOTKEYS_DECLARATIVE_DESIGN_2026_09_28.md` with its recommended answers:

- identity = the normalised accelerator;
- between windows, the most recently focused declarer wins, then the oldest window;
- a window's declaration beats the app's;
- the imperative `register_global_hotkey` / `unregister_global_hotkey` are gone;
- status, retry and `raise_window` stay.

Nothing was compiled (parallel-agent rule). The parent compiles once and runs `azul-doc autofix`
(the API list is in §3). **`api.json` still carries entries whose `fn_body` calls removed
methods, so the generated C API does not build until `autofix remove` runs (§3.1).**

## 1. What was built

| piece | where | what |
|---|---|---|
| model | `core/src/global_hotkey.rs` | `GlobalHotkeyCallbackData`, `GlobalHotkeyInfo`, `GlobalHotkeyOwner`, `GlobalHotkeyState`, `GlobalHotkeyEvent`, the thread-local declaration recorder, `GlobalHotkeysCallback` + `GlobalHotkeysCallbackInfo` |
| layout side | `core/src/callbacks.rs` | `LayoutCallbackInfo::{add_global_hotkey, add_global_hotkey_with_description, get_global_hotkey_status, get_global_hotkeys}`; a status snapshot in `LayoutCallbackInfoRefData.global_hotkeys` |
| app side | `core/src/resources.rs` | `AppConfig.global_hotkeys` (a static list) and `AppConfig.global_hotkeys_callback` (derived from the app's `RefAny`) |
| manager | `layout/src/managers/global_hotkey.rs` | `GlobalHotkeyManager` (declare, sync, owner rule, sticky failures, app source), `HotkeySink` (the OS threads' mailbox + loop waker), `SharedGlobalHotkeys` (`Arc<Mutex<..>>`), `WindowHotkeys` (a window's membership, drop = forget), `HotkeyTurn` / `begin_turn` (owner routing), `SimulatedBackend` (programmable answers) |
| window | `layout/src/window.rs` | `LayoutWindow.global_hotkeys`; `apply_window_activation` stamps the focus |
| event side | `layout/src/callbacks.rs` | `CallbackInfo::{get_global_hotkey_status(hotkey), get_global_hotkeys, get_global_hotkey_event, retry_global_hotkey}`, `CallbackChange::RetryGlobalHotkey` |
| backends | `dll/src/desktop/global_hotkey/{macos,windows,x11,portal}.rs` | trait impls reporting through their own sink, each releasing its grabs on `Drop`. The portal binds one session per BATCH under stable ids (`portal_plan.rs`, a pure planner) |
| loops | `dll/src/desktop/global_hotkey/mod.rs`, `shell2/run.rs`, `shell2/headless/mod.rs`, `shell2/common/layout.rs` | `regenerate_layout` declares + syncs; `pump_macos_windows` / `pump_win32_windows` / `pump_linux_windows` / `pump_headless` route presses to their owner and run status-driven relayouts |
| app | `dll/src/desktop/app.rs` | `AppInternal.global_hotkeys`; `App::run` / `run_tray_only` enter it as the loop thread's current App, choose the platform backend lazily and hand over the `AppConfig` set |
| e2e | `layout/src/e2e/full.rs`, `layout/src/e2e/runner.rs` | `global_hotkey` presses through the window's manager; new `global_hotkey_answer`, `global_hotkey_settle`, assertion `assert_global_hotkeys` |
| demo | `examples/azul-widgets/src/{hotkeys,lib}.rs`, `examples/azul-widgets/e2e/global_hotkey.json` | the hotkey is declared in `layout()` while `enabled`; the status line updates by itself; a Retry button on failure |

### How it runs

1. `layout()` calls `info.add_global_hotkey(hk, data, cb)` while the state wants it. The call is
   recorded in a thread-local, exactly like `window_width_less_than` / `depends_on_system_style`.
2. Right after the callback, `regenerate_layout` hands the recording to the window's
   `WindowHotkeys::declare_recorded`, which declares it and syncs.
3. The sync:
   - releases what no source wants any more, before any grab of the same batch;
   - grabs what is new;
   - does nothing for accelerators that stayed (their callback and `RefAny` are swapped);
   - commits the batch once (the portal's single session).
4. A status that moved asks the passes that READ a status to run again (`RelayoutReason::Other`).
5. The pump of every loop turn:
   - runs the `AppConfig` callback if it is due (without the manager's lock);
   - syncs;
   - routes each press to its owner's window;
   - runs the owed relayouts.

**Deviation from "threaded like `SharedUndoManager`" (deliberate).** The App owns the
`Arc<Mutex<GlobalHotkeyManager>>`. It is NOT passed through the ~70 window-constructor and `run()`
sites, several of which are in the Linux loop files the parallel agent is rewriting. Instead:

- `App::run` / `run_tray_only` call `SharedGlobalHotkeys::enter()`. This makes the App the event
  loop thread's CURRENT App, with a scope guard, for the whole run.
- Every `LayoutWindow` built on that thread joins it (`WindowHotkeys::for_current_app`).
- Anything built elsewhere gets a detached manager with no backend (`Failed(Unsupported)`). That
  covers tests, the web server, and a mobile window created after `run()` returned.

The design's reasons against the static registry all still hold:

- there is no process global: each App (and each test) has its own manager;
- no backend is installed before `run()`;
- dropping the App drops the backend, which releases every grab;
- ownership is per App.

To thread it explicitly later, pass the handle into `CommonWindowState::new` and set
`LayoutWindow.global_hotkeys` there.

## 2. Commits

In the design's numbering (1-15). Each RED states what fails today.

| design # | commit | subject | RED today |
|---|---|---|---|
| 1 | `b4b976577` | test(hotkeys): a declared set reconciles against the backend without churn | does not compile (`GlobalHotkeyManager`, `HotkeySource`, … missing); 22 rows, see the commit |
| 2 (+ 11a, 12a) | `da59726db` | feat(hotkeys): App-owned GlobalHotkeyManager, HotkeySink, SharedGlobalHotkeys replace the process-global registry | GREEN for 1. It also contains: backends to trait + sink; imperative API removed; `App::create` installs nothing; the probe is pure |
| 3 | `d0ee0d7ac` | test(core): add_global_hotkey is recorded per layout call | does not compile (recorder / methods / ref-data field missing) |
| — | `9e7d1c173` | chore(scripts): progress checkpoint | — |
| 4 | `2a1a7a5a9` | feat(core): LayoutCallbackInfo declares global hotkeys (recorder, snapshot) | GREEN for 3 |
| 5 | `f6537463b` | test(hotkeys): a layout pass keeps the grabs in sync, headless | does not compile. Behaviourally, the first layout's log is `[]` where `["register Ctrl+Alt+K"]` is expected |
| 6 | `eb29763f6` | feat(hotkeys): regenerate_layout declares into the App's manager; windows release on drop | GREEN for 5 |
| — | `342b9a18b` | test(hotkeys): the status-read test only measures what the pump asks for | fixup of 5 |
| 7 | `84cfa94b9` | test(app): AppConfig hotkeys, for apps with no window | does not compile. Behaviourally, an `AppConfig` hotkey reads `NotRegistered` where `Active` is expected |
| 8 | `00cb3834a` | feat(app): AppConfig::global_hotkeys + global_hotkeys_callback; the backend is chosen in run() | GREEN for 7 |
| 9 | `201d6ca04` | test(hotkeys): presses run against their owner, never "the first window" | does not compile (`begin_turn` / `HotkeyTurn`). Behaviourally, every press ran against `registry.first()` |
| 10 | `08fcf29b7` | fix(hotkeys): pumps deliver to the owner window | GREEN for 9 |
| 11 | `99f1b064b` | test(hotkeys): the Wayland portal binds one session per batch under stable ids | RED by assertion: `shortcut_id` is `None` where `Some("CTRL+ALT+k")` is expected; `plan_commit` returns an empty plan where `bind == [K, J]` is expected; one control row passes |
| 11 | `80aa017d2` | fix(hotkeys): the portal binds one session per batch under stable ids; loops can attach a waker | GREEN for 11 |
| 12 | `904da24a8` | test(api): CallbackInfo reads global hotkeys by accelerator, retries, sees the fired event | does not compile (the methods and `RetryGlobalHotkey` are missing) |
| 12 | `6d164c200` | feat(api): CallbackInfo reads by accelerator, lists, sees the fired press, retries | GREEN for 12 |
| 13 | `1c6db8101` | test(e2e): scenarios can program, settle and assert global hotkeys | does not compile (ops / helpers missing) |
| 13 | `666250542` | feat(e2e): global_hotkey presses the declared set; answer / settle / assert_global_hotkeys | GREEN for 13 |
| 14 | `88b049e4b` | feat(examples): the Global hotkey section declares its hotkey from state | needs the regenerated bindings |
| optional | `ce70dca8e` | test(macos): F13..F20 must be in the macOS keycode table | RED by assertion: `every_platform_table_matches_the_manifest`, macOS maps 0 codes to F13 where the manifest says 1 |
| optional | `e5af297d8` | fix(macos): kVK_F13..kVK_F20 in the keycode table | GREEN |
| 15 | `652bc23ff` + this commit | test(doc): module_map guard; chore: this report | guard is green on arrival |

### The four side bugs of the design (§6.4)

1. **"First window" is arbitrary** (`08fcf29b7`), for hotkeys. The owner rule and
   `SharedGlobalHotkeys::app_target(live)` pick the window; the focus stamps come from
   `LayoutWindow::apply_window_activation`, the one chokepoint every backend's activation goes
   through.
   - Tray clicks and notifications still use the registry's first window. That belongs to the
     parallel agent's "which window receives app-level events" helper.
   - The hotkey code does not call that helper, so there is no compile dependency on its name.
   - Suggested integration once it exists: its `app_level_target_window()` (whatever the final
     name is) could feed `live` in the desktop pumps. Alternatively, the helper could reuse
     `SharedGlobalHotkeys::app_target` together with the focus stamps.
2. **Registering before `run()` grabbed at the real OS** (`da59726db`).
   - `App::create` installs nothing.
   - `App::run` only CHOOSES the platform backend (`set_pending_backend`); it is built at the
     first sync.
   - `run_headless` installs the simulation before the first layout, so the platform backend is
     never built and no portal handshake starts.
3. **Unstable portal ids** (`80aa017d2`): the shortcut id is `portal_trigger(hk)`.
4. **One portal session per hotkey** (`80aa017d2`): `register` queues, and `commit` binds the whole
   batch in one session. A release leaves a tombstone. A session with nothing left is closed; a
   partly released one is folded into the next batch and closed once that batch has bound.
5. **macOS F13–F20 `KeyNotMappable`** (`e5af297d8`).

## 3. API changes for `azul-doc autofix`

### 3.1 Removals (`autofix remove`)

- `CallbackInfo::register_global_hotkey`
- `CallbackInfo::unregister_global_hotkey`
- `CallbackInfo::get_global_hotkey_status(id: GlobalHotkeyId)` — a **signature change**. It is
  re-added as `get_global_hotkey_status(hotkey: GlobalHotkey)`, see §3.2.
- `App::register_global_hotkey`
- `App::unregister_global_hotkey`
- `OptionGlobalHotkeyId`
- `ResultGlobalHotkeyIdGlobalHotkeyError`

`GlobalHotkeyId` STAYS: it is the payload of `GlobalHotkeyError::AlreadyRegistered`. That variant
is kept for ABI stability, but the declarative manager never produces it (see Open items).

### 3.2 Additions

**`azul_core::global_hotkey`** (autofix module `app`; the Vec / Option wrappers go to `vec` /
`option`):

- `GlobalHotkeyCallbackData` (`#[repr(C)]`):
  - fields `hotkey: GlobalHotkey`, `description: String`, `callback: CoreCallback`, `refany: RefAny`;
  - derives Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash;
  - constructor `create(hotkey, data: RefAny, callback: CoreCallback)`;
  - `GlobalHotkeyCallbackDataVec` with Destructor, DestructorType and Slice;
  - `OptionGlobalHotkeyCallbackData`.
- `GlobalHotkeyOwner` (`#[repr(C)]`): `App`, `ThisWindow`, `OtherWindow`, `Nobody`.
- `GlobalHotkeyInfo` (`#[repr(C)]`):
  - fields `hotkey`, `status: GlobalHotkeyStatus`, `trigger: String`, `owner: GlobalHotkeyOwner`;
  - `GlobalHotkeyInfoVec` (+ family) and `OptionGlobalHotkeyInfo`.
- `GlobalHotkeyState` (`#[repr(C)]`): `Pressed`, `Released`.
- `GlobalHotkeyEvent` (`#[repr(C)]`, Copy):
  - fields `hotkey`, `state: GlobalHotkeyState`, `timestamp_ms: u64`;
  - `OptionGlobalHotkeyEvent`.
- Callback typedef `GlobalHotkeysCallbackType = extern "C" fn(RefAny, GlobalHotkeysCallbackInfo)`,
  returning nothing.
- `GlobalHotkeysCallback { cb, ctx: OptionRefAny }`:
  - `create(cb)`;
  - `impl_callback!` and `impl_managed_callback!`, which export `AzApp_setGlobalHotkeysCallbackInvoker`,
    `AzGlobalHotkeysCallback_createFromHostHandle` and `AzGlobalHotkeysCallback_createFromHostHandleByref`;
  - `OptionGlobalHotkeysCallback`.
- `GlobalHotkeysCallbackInfo`: opaque, Copy, the same shape as `LayoutCallbackInfo`. Methods:
  - `add_global_hotkey(&self, hotkey, data: RefAny, callback: CallbackType)`;
  - `add_global_hotkey_with_description(&self, hotkey, description: String, data, callback)`;
  - `get_global_hotkey_status(&self, hotkey) -> GlobalHotkeyStatus`;
  - `get_global_hotkeys(&self) -> GlobalHotkeyInfoVec`;
  - `get_ctx(&self)`.
- Internal, not for api.json: `status_in`, `RecordedGlobalHotkeys`, `take_recorded_global_hotkeys`,
  `GLOBAL_HOTKEY_DECLARATION_CAP`, `record_declaration`, `record_status_read`.

**`LayoutCallbackInfo`** (`core/src/callbacks.rs`):

- `add_global_hotkey(&self, hotkey: GlobalHotkey, data: RefAny, callback: CallbackType)`
  - fn_body: `object.add_global_hotkey(hotkey, data, azul_layout::callbacks::Callback::create(callback).to_core())`
- `add_global_hotkey_with_description(&self, hotkey, description: String, data: RefAny, callback: CallbackType)`
  - same body shape
- `get_global_hotkey_status(&self, hotkey: GlobalHotkey) -> GlobalHotkeyStatus`
- `get_global_hotkeys(&self) -> GlobalHotkeyInfoVec`
- `LayoutCallbackInfoRefData.global_hotkeys` is an internal field.

**`CallbackInfo`** (`layout/src/callbacks.rs`):

- `get_global_hotkey_status(&self, hotkey: GlobalHotkey) -> GlobalHotkeyStatus` (live)
- `get_global_hotkeys(&self) -> GlobalHotkeyInfoVec`
- `get_global_hotkey_event(&self) -> OptionGlobalHotkeyEvent`
- `retry_global_hotkey(&mut self, hotkey: GlobalHotkey)`
- `raise_window` is unchanged.
- `CallbackChange::RetryGlobalHotkey { hotkey }` is appended (an internal enum).

**`AppConfig`** (`core/src/resources.rs`):

- new fields `global_hotkeys: GlobalHotkeyCallbackDataVec` and `global_hotkeys_callback: OptionGlobalHotkeysCallback`;
- `add_global_hotkey(&mut self, hotkey, data: RefAny, callback: CallbackType)`
  - body: `object.add_global_hotkey(hotkey, data, azul_layout::callbacks::Callback::create(callback).to_core())`
- `with_global_hotkeys_callback(self, cb: GlobalHotkeysCallbackType) -> AppConfig`
- `set_global_hotkeys_callback(&mut self, cb: GlobalHotkeysCallbackType)`

**Unchanged:**

- `PlatformCapability::global_hotkeys()`: only its doc text changed. It now names
  `LayoutCallbackInfo::add_global_hotkey`, and the probe is pure.
- `GlobalHotkey`, `HotkeyModifiers`, `GlobalHotkeyError`, `GlobalHotkeyStatus` (docs updated),
  `ResultGlobalHotkeyGlobalHotkeyError`.

**Internal, not for api.json:**

- everything in `azul_layout::managers::global_hotkey`;
- `azul_dll::desktop::global_hotkey::*`, including the integration points `attach_loop_waker`,
  `wake_fds`, `pump_headless` and `platform_backend`;
- the e2e ops;
- `WindowHotkeys`.

## 4. Least sure to compile

1. **`core/src/global_hotkey.rs`**:
   - `crate::impl_managed_callback!` form 1 for `GlobalHotkeysCallback` with `return_ty: ()` and
     `default_ret: Default::default()` (the `ThreadCallback` shape);
   - `impl HostCtxCarrier for GlobalHotkeysCallbackInfo`;
   - `impl_callback!` on a fn-pointer type taking a `Copy` info;
   - the `std::thread_local!` `const { RefCell::new(RecordedGlobalHotkeys { declared: Vec::new(), .. }) }`.
2. **`layout/src/managers/global_hotkey.rs`**:
   - disjoint field borrows in `replace_backend`, where `self.backend.as_mut()` is live while
     iterating `self.held.values()`;
   - `let Some(job) = self.lock().take_app_job() else { .. }` (the guard temporary);
   - the `thread_local!` statics `CURRENT_APP: RefCell<Option<SharedGlobalHotkeys>>` and
     `DELIVERING: Cell<Option<GlobalHotkeyEvent>>`;
   - the local `struct Restore` + `impl Drop` inside `with_delivered_event`;
   - `Send` for `GlobalHotkeyManager`, which holds `GlobalHotkeysCallback` (a fn pointer +
     `OptionRefAny`) and `Box<dyn GlobalHotkeyBackend>`.
3. **`dll/src/desktop/global_hotkey/mod.rs`**:
   - `platform_backend` built from `#[cfg]`'d `let backend: Box<dyn GlobalHotkeyBackend> = …`
     statements;
   - `run_turn<W: PlatformWindow>` over `&[*mut W]` with the closure `|window, _| window.request_redraw()`;
   - the Linux pump's `#[cfg(target_os = "linux")] LinuxWindow::Wayland` arms inside `match`
     expressions in closures;
   - `seq_of(&*window)`.
4. **`portal.rs`**:
   - `for session in &mut lock(&self.state).sessions` and `core::mem::take(&mut lock(..).sessions)`
     (mutable borrows through guard temporaries);
   - the batched `BindShortcuts` body `(ObjectPath, Vec<(&str, HashMap<&str, Value>)>, "", HashMap)`;
   - the `move` closure with `return` in `spawn_bind`.
5. **`windows.rs`**:
   - the static `ROUTES: Mutex<BTreeMap<usize, Arc<Routing>>>` (needs `Routing: Send + Sync`);
   - `.filter_map(registry::get_window)` in `pump_win32_windows` (the `HWND` argument type);
   - the newly required `DestroyWindow` symbol.
6. **`macos.rs`**: `RemoveEventHandler` is now a required symbol; `Box<HotkeySink>` is the
   Carbon `user_data`.
7. **`x11.rs`**:
   - `XConnectionNumber` and `XCloseDisplay` are now required symbols (both exist in libX11);
   - `u64::from(key.time)`.
8. **Test modules** that rely on glob imports reaching `WindowSize` / `DarkLightMode`:
   `core/src/callbacks_test.rs::global_hotkey_recorder_tests` (the neighbouring modules do the same).
9. **`layout/src/e2e/full.rs`**:
   - the new `DebugEvent` arms (patterns on a reference);
   - `describe_global_hotkeys(callback_info)`, which works whether `callback_info` is `&` or `&mut`.
10. **`dll/tests/headless_global_hotkeys.rs`**: `with_state`'s `&mut guard` to `&mut HotkeyState`
    deref coercion.
11. **The demo** depends on the GENERATED bindings:
    - method names `add_global_hotkey_with_description`, `get_global_hotkey_status(hotkey)`,
      `retry_global_hotkey`;
    - `"…".into()` for the `String` description;
    - `LayoutCallbackInfo` coming from the prelude.
12. **Pre-existing, not caused here:** `dll/src/web/html_render.rs::call_layout` already lacks the
    `locale` / `accessed_*` / `text_direction` fields of `LayoutCallbackInfoRefData`. The new
    `global_hotkeys` field was added anyway.

## 5. Open items

- **For the parallel Linux loop-waker agent:**
  - Once the shared loop waker exists, call
    `crate::desktop::global_hotkey::attach_loop_waker(waker, watches_wake_fds)`.
  - Add `crate::desktop::global_hotkey::wake_fds()` (X11's grab connection) to the poll set.
  - After that, `needs_loop_polling()` answers false and the 100 ms park cap disappears.
  - The X11 fd only exists after the first grab, so re-read `wake_fds()` before each park.
  - Headless already attaches its condvar.
- **Portal:**
  - The bind `Response`'s `trigger_description` and the `ShortcutsChanged` signal are NOT decoded:
    the `a(sa{sv})` zvariant decoding is too risky to write uncompiled. The manager already
    handles `BackendEvent::Settled(Ok(trigger))` and `TriggerChanged`, so only the decoding is
    missing.
  - The host-app id registration and `activation_token` are still unused.
  - Changing a kept accelerator's description does not re-bind.
- **Manager accounting:** `global_hotkey` stays in `UNOBSERVABLE_MANAGERS` / `not_fingerprintable`
  (the reasons are rewritten). Fingerprinting a window's own declaration is the follow-up.
- **The layout-level e2e runner** (`layout/src/e2e/runner.rs`) still does not PUMP hotkeys (only
  the retry arm was added). Presses run under the dll's headless shell.
- **The demo scenario** `examples/azul-widgets/e2e/global_hotkey.json` is unverified. It needs:
  - regenerated bindings and a headless run;
  - the Linux / Windows spelling (a Mac host uses Cmd+Shift+K);
  - clicks on buttons that may be scrolled far down the showcase.
- **Design questions still open:**
  - Q5: key release / push-to-talk; the backends report presses only.
  - Q6: `App::run_background`.
  - Q9: a lazily created app stub. With `RunForever` and zero windows, an app-level press is
    reported undeliverable.
  - Q10: a DOM route for widgets.
  - Q11: `ConfigureShortcuts`.
  - Q8: `RelayoutReason::Other` is reused for status-driven passes.
- **No per-turn warning:** there is no "at most 2 status relayouts per turn" warning. The bound is
  structural: each turn takes each window's relayout request at most once, and an identical
  re-declaration moves nothing.
- `GlobalHotkeyError::AlreadyRegistered` is never produced. Dropping it would also drop
  `GlobalHotkeyId` from the public API.
- Two `App`s alive at once on macOS: both Carbon handlers see `'AZHK'` hot keys (they share the
  signature).
- Unchanged platform limits:
  - Windows OEM punctuation reads `KeyNotMappable`;
  - X11 assumes Mod1/Mod4 and does not re-grab on `MappingNotify`;
  - media keys cannot be Carbon hot keys.
