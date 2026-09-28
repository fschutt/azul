# Global hotkeys, declarative - implementation PROGRESS (checkpoint)

Branch `wt/hotkeys-declarative` (from `5414bfa6b`, PR #476 base). Source of truth:
`scripts/GLOBAL_HOTKEYS_DECLARATIVE_DESIGN_2026_09_28.md`. Rules: no cargo / rustc / LSP; RED
commit first, then the implementation; `git commit -F -`; stage explicit paths; never touch
`layout/src/solver3/page_breaks.rs` or `layout/tests/a_padded_table_cell_stays_in_its_row.rs`;
do not edit `api.json` (list additions/removals for `azul-doc autofix`). This file is deleted in
the last commit; `scripts/GLOBAL_HOTKEYS_DECLARATIVE_IMPL_2026_09_28.md` replaces it.

## Architecture decisions taken (read before continuing)

- `layout/src/managers/global_hotkey.rs` = `GlobalHotkeyManager` (declare / forget_source /
  note_focus / retry / sync / take_deliveries / status / infos_for / simulate / settle /
  program_answer / app_target / take_relayout), `HotkeySink` (bounded mailbox + `LoopWaker`),
  trait `GlobalHotkeyBackend`, `SimulatedBackend` (+ `SimulatedAnswer`), `SharedGlobalHotkeys`
  (`Arc<Mutex<GlobalHotkeyManager>>` + sink), `with_delivered_event` / `delivered_event`
  (thread-local, for `CallbackInfo::get_global_hotkey_event`).
- NOT threaded through ~70 window-constructor sites: `App::run` / `run_tray_only` call
  `SharedGlobalHotkeys::enter()` (thread-local "current App" for the loop thread, scope guard)
  and `choose_platform_backend()` (lazy `set_pending_backend`, built at first sync);
  `run_headless` installs the simulation before the first layout. Windows join via
  `SharedGlobalHotkeys::current_or_detached()` (detached = no backend = `Failed(Unsupported)`).
  The App owns the Arc: `AppInternal.global_hotkeys`.
- dll `desktop/global_hotkey/mod.rs`: `platform_backend(sink)`, `app()`,
  `choose_platform_backend()`, `install_simulated_backend()`, `needs_loop_polling()` (kept name:
  the X11/Wayland/headless loops call it unchanged), pure `probe()`, `deliver()`,
  `deliver_fired()`, pumps still named `pump_into_first_*` (first window) until the owner-routing
  commit.
- Backends (Carbon / Win32 / X11 / portal) are trait impls reporting through their own sink; each
  releases its grabs on `Drop`. Portal is still one session per grab, ids `azul-hotkey-{os_id}`
  (the batch/stable-id fix is its own later commit with a pure planner RED).
- Planned: `LayoutWindow.global_hotkeys: WindowHotkeys { shared, seq }` with `Drop` ->
  `forget_source(Window(seq))`; `note_focus` hooked in `LayoutWindow::apply_window_activation`
  (common chokepoint, no shell edits); must be added to the two exhaustive `LayoutWindow`
  destructures in `layout/src/window.rs` (`memory_walk_coverage_is_exhaustive` ~line 885 and the
  NodeId-remap one ~line 21684).
- Planned: pumps call `SharedGlobalHotkeys::begin_turn(live_seqs)` -> `HotkeyTurn { deliveries:
  Vec<(index, HotkeyDelivery)>, relayout: Vec<index>, undeliverable }`; App-owned presses go to
  `app_target(live)` (most recently focused, else oldest). Status-driven relayout =
  `request_regeneration(RelayoutReason::Other)`.
- Waker for the other agent (Linux loop park rework): `SharedGlobalHotkeys::attach_loop_waker(
  waker: Arc<dyn Fn() + Send + Sync>, watches_wake_fd: bool)` + `wake_fd()`; planned dll wrappers
  `crate::desktop::global_hotkey::{attach_loop_waker, wake_fds}`.

## DONE (numbered against the design's 15-commit plan)

1. `b4b976577` test(hotkeys): a declared set reconciles against the backend without churn (RED,
   `layout/tests/global_hotkeys.rs` rewritten; does not compile before 2).
2. `da59726db` feat(hotkeys): App-owned GlobalHotkeyManager + HotkeySink + SharedGlobalHotkeys;
   core model types; backends -> trait + sink; imperative API removed; App::create installs
   nothing; probe pure. (Also carries the design's commit-11 "backends report through their sink"
   half and the commit-12 removal half.)
3. `d0ee0d7ac` test(core): add_global_hotkey is recorded per layout call (RED, does not compile:
   `core/src/callbacks_test.rs` module `global_hotkey_recorder_tests`).
   (`9e7d1c173` = this checkpoint file.)
4. feat(core): LayoutCallbackInfo declares global hotkeys (recorder in `core/src/global_hotkey.rs`,
   `LayoutCallbackInfoRefData.global_hotkeys`, the four `LayoutCallbackInfo` methods; empty
   snapshot + recorder clear in `common/layout.rs` and `web/html_render.rs`). The commit whose
   subject starts `feat(core): LayoutCallbackInfo declares` - see `git log`.

5. test(hotkeys): a layout pass keeps the grabs in sync, headless - RED, does not compile
   (`dll/tests/headless_global_hotkeys.rs`; needs `LayoutWindow.global_hotkeys.shared()` and
   `azul::desktop::global_hotkey::pump_headless(&mut HeadlessWindow) -> ProcessEventResult`).

6. feat(hotkeys): regenerate_layout declares into the App's manager (`WindowHotkeys` in the
   manager module; `LayoutWindow.global_hotkeys` + both exhaustive destructures; note_focus in
   `apply_window_activation`; `pump_headless` in the dll handles presses + relayout requests;
   headless Phase 1c and the tray-only timer call it). Desktop pumps do NOT yet handle relayout
   requests (step 10).

   (+ `342b9a18b` test fixup: the status-read test retires what the pass itself leaves pending.)
7. test(app): AppConfig hotkeys - RED, does not compile: `layout/tests/global_hotkeys.rs`
   section 6 (`GlobalHotkeysCallback`, `GlobalHotkeysCallbackInfo`, `SharedGlobalHotkeys::
   {set_app_declarations(Vec, Option<GlobalHotkeysCallback>, RefAny), refresh_app_declarations()
   -> bool, mark_app_dirty()}`) + `dll/tests/headless_global_hotkeys.rs::an_app_config_hotkey_is_
   grabbed_and_pressed_without_any_layout` (pump_headless must refresh the app source).

8. feat(app): `AppConfig.global_hotkeys` / `global_hotkeys_callback` (+ `add_global_hotkey`,
   `with_global_hotkeys_callback`, `set_global_hotkeys_callback`), `GlobalHotkeysCallback(Type)`
   + `impl_managed_callback!` exports, `OptionGlobalHotkeysCallback`, `GlobalHotkeysCallbackInfo`;
   manager `AppSource` + `set_app_declarations` / `mark_app_dirty` / `refresh_app_declarations`
   (callback runs without the lock); App::run declares the config set in `enter_global_hotkeys`;
   pumps refresh before sync; `deliver(window, &shared, delivery)` marks the app dirty and
   regenerates all windows for an App-level press that asked for a rebuild; regenerate_layout
   marks the app dirty on `RelayoutReason::RefreshDom`. `deliver_fired` removed (unused).

9. test(hotkeys): presses run against their owner - RED, does not compile:
   `layout/tests/global_hotkeys.rs` section 7 needs `SharedGlobalHotkeys::begin_turn(&[WindowSeq])
   -> HotkeyTurn { deliveries: Vec<(usize, HotkeyDelivery)>, relayout: Vec<usize>,
   undeliverable: Vec<HotkeyDelivery> }`.

10. feat(hotkeys): pumps deliver to the owner window - `HotkeyTurn` + `SharedGlobalHotkeys::
    begin_turn(&[WindowSeq])` in layout; dll `pump_macos_windows` / `pump_win32_windows` /
    `pump_linux_windows` (renamed from `pump_into_first_*`, 4 one-line call-site edits in run.rs)
    and `pump_headless` all go through `begin_turn`; relayouts now also on desktop.

11a. test(hotkeys): the portal batch planner - RED by assertion: `dll/src/desktop/global_hotkey/
    portal_plan.rs` (pure, compiled on every host, `#[cfg(test)] mod tests`) with stubbed
    `shortcut_id` (None) and `plan_commit` (empty plan).

11b. fix(hotkeys): planner implemented; portal backend rewritten (queue in `register`, ONE
    session per batch in `commit`, id = `portal_trigger`, tombstones, close / compaction, failed
    batch reports only its fresh grabs); dll `attach_loop_waker(LoopWaker, bool)` / `wake_fds()`
    integration points; headless run() attaches a condvar waker. trigger_description /
    ShortcutsChanged parsing left OPEN (zvariant `a(sa{sv})` decoding too risky uncompiled).

12a. `904da24a8` test(api): CallbackInfo reads hotkeys by accelerator, retries, sees the fired
    event - RED, does not compile (`layout/src/callbacks.rs` tests: retry row in
    `callback_info_flag_mutators_queue_exactly_one_matching_change`,
    `callback_info_reads_the_live_global_hotkey_status_by_accelerator`,
    `the_global_hotkey_event_is_only_readable_inside_the_fired_callback`).

12b. feat(api): CallbackInfo `get_global_hotkey_status(hotkey)` (live), `get_global_hotkeys`,
    `get_global_hotkey_event`, `retry_global_hotkey` + `CallbackChange::RetryGlobalHotkey { hotkey }`
    (arms in dll `apply_user_change` and `layout/src/e2e/runner.rs`).

13a. test(e2e): the global-hotkey ops - RED, does not compile: `layout/src/e2e/full.rs` module
    `global_hotkey_op_tests` needs `DebugEvent::{GlobalHotkeyAnswer, GlobalHotkeySettle}`,
    `simulated_answer_from_name`, `settle_result_from_name`, `global_hotkey_status_matches`,
    `global_hotkey_status_name`, `global_hotkey_owner_name`.

13b. feat(e2e): `global_hotkey` presses through the WINDOW's manager; new ops
    `global_hotkey_answer`, `global_hotkey_settle`; new assertion `assert_global_hotkeys`
    (`expect: [{accelerator, status?, owner?}]`, `count?`); gate reasons rewritten (global_hotkey
    stays in UNOBSERVABLE_MANAGERS / not_fingerprintable - moving it to the fingerprinted set is
    OPEN).

14. feat(examples): the demo declares from state (`HotkeyDemo { enabled, fired }`,
    `hotkey_section(data, demo, &info)`, Enable/Disable + Retry buttons; layout() passes its
    info) + scenario `examples/azul-widgets/e2e/global_hotkey.json` (UNVERIFIED: needs
    regenerated bindings and a headless run; Linux/Windows accelerator spelling).

## IN PROGRESS

(nothing uncommitted)

## NEXT
15. optional fix(macos) F13-F20: RED = manifest rows F13..F20 macOS column 0 -> 1 in
    `layout/tests/keycode_table_manifest_is_exhaustive.rs`; fix = arms 0x69 F13, 0x6B F14, 0x71
    F15, 0x6A F16, 0x40 F17, 0x4F F18, 0x50 F19, 0x5A F20 in `macos_keycode_to_virtual_key`.
16. final report `scripts/GLOBAL_HOTKEYS_DECLARATIVE_IMPL_2026_09_28.md`, delete this file;
    module_map check for new type names.

## API additions / removals so far (for `azul-doc autofix`)

Removed (in api.json today -> `autofix remove`):
- `CallbackInfo::register_global_hotkey`, `CallbackInfo::unregister_global_hotkey`,
  `CallbackInfo::get_global_hotkey_status(id: GlobalHotkeyId)` (re-added keyed by accelerator in
  step 12 - a signature change).
- `App::register_global_hotkey`, `App::unregister_global_hotkey`.
- types `OptionGlobalHotkeyId`, `ResultGlobalHotkeyIdGlobalHotkeyError`.
- `GlobalHotkeyId` STAYS (payload of `GlobalHotkeyError::AlreadyRegistered`, now never produced).

Added (azul_core::global_hotkey):
- `GlobalHotkeyCallbackData { hotkey, description: AzString, callback: CoreCallback, refany }`
  + `create(hotkey, data, callback)`; `GlobalHotkeyCallbackDataVec` (+Destructor, DestructorType,
  Slice), `OptionGlobalHotkeyCallbackData`.
- `GlobalHotkeyOwner { App, ThisWindow, OtherWindow, Nobody }`.
- `GlobalHotkeyInfo { hotkey, status, trigger: AzString, owner }`, `GlobalHotkeyInfoVec`
  (+family), `OptionGlobalHotkeyInfo`.
- `GlobalHotkeyState { Pressed, Released }`; `GlobalHotkeyEvent { hotkey, state, timestamp_ms:
  u64 }`, `OptionGlobalHotkeyEvent`.
- Internal (no api.json): `status_in`, `RecordedGlobalHotkeys`, `take_recorded_global_hotkeys`,
  `GLOBAL_HOTKEY_DECLARATION_CAP`.
- (step 4) `LayoutCallbackInfo::add_global_hotkey(&self, hotkey, data: RefAny, callback:
  CallbackType)` (fn_body `object.add_global_hotkey(hotkey, data,
  azul_layout::callbacks::Callback::create(callback).to_core())`),
  `add_global_hotkey_with_description(&self, hotkey, description: String, data, callback)`,
  `get_global_hotkey_status(&self, hotkey) -> GlobalHotkeyStatus`, `get_global_hotkeys(&self) ->
  GlobalHotkeyInfoVec`. `LayoutCallbackInfoRefData.global_hotkeys` is an internal field.
- (step 8) `AppConfig` fields `global_hotkeys: GlobalHotkeyCallbackDataVec`,
  `global_hotkeys_callback: OptionGlobalHotkeysCallback`; methods `AppConfig::add_global_hotkey(
  &mut self, hotkey, data: RefAny, callback: CallbackType)` (body wraps with
  `azul_layout::callbacks::Callback::create(callback).to_core()`),
  `AppConfig::with_global_hotkeys_callback(self, cb: GlobalHotkeysCallbackType) -> AppConfig`,
  `AppConfig::set_global_hotkeys_callback(&mut self, cb)`.
- (step 8) callback typedef `GlobalHotkeysCallbackType = extern "C" fn(RefAny,
  GlobalHotkeysCallbackInfo)`; `GlobalHotkeysCallback { cb, ctx }` (+ `create`), managed-FFI
  exports `AzApp_setGlobalHotkeysCallbackInvoker`, `AzGlobalHotkeysCallback_createFromHostHandle`
  (+`Byref`); `OptionGlobalHotkeysCallback`; `GlobalHotkeysCallbackInfo` (opaque, Copy) with
  `add_global_hotkey`, `add_global_hotkey_with_description`, `get_global_hotkey_status`,
  `get_global_hotkeys`, `get_ctx`.

- (step 12) `CallbackInfo::get_global_hotkey_status(&self, hotkey: GlobalHotkey) ->
  GlobalHotkeyStatus` (SIGNATURE CHANGE: was `id: GlobalHotkeyId`), `CallbackInfo::
  get_global_hotkeys(&self) -> GlobalHotkeyInfoVec`, `CallbackInfo::get_global_hotkey_event(&self)
  -> OptionGlobalHotkeyEvent`, `CallbackInfo::retry_global_hotkey(&mut self, hotkey:
  GlobalHotkey)`; `CallbackChange::RetryGlobalHotkey { hotkey }` (internal enum, appended).
  `CallbackInfo::raise_window` unchanged.

## Open questions / notes

- Deviation from "threaded like SharedUndoManager": ambient per-loop-thread current App (see
  above). Reason: ~70 constructor/call sites across 7 shells, several in Linux files the parallel
  agent is editing. The App still owns the Arc.
- `dll/src/web/html_render.rs::call_layout` already lacks the `locale` / `accessed_*` /
  `text_direction` fields of `LayoutCallbackInfoRefData` (pre-existing: it cannot compile if the
  `web` feature builds it); the new field was added anyway.
- Intermediate commits are meant to be consistent, but only the tip is compiled by the parent.
