# Input wiring — work queue

## USER RULINGS — 2026-09-03 (unblock the decision-gated items)

- 2026-09-03 (decisions batch, answered in one message): pointer lock is NOT re-taken on focus
  return - emit an event and let the app re-request (9d-ii-c); system audio is a RUNTIME
  take-over API around playback, not a flag (9h-i-a-i-d-i); `regex-lite`, minimal features, for
  `pattern` (11b-i-b); `spatial-navigation-contain: auto` follows the spec (9a-i-b-i); peer
  carets shift with edits before them (U3-a); proximity is an enum Near / Far / Distance(value,
  unit) (8e-i-a-i, 8e-i-a-ii); pads get a per-instance identity on connect so two identical
  controllers work for multiplayer (8f-i-a-i).

Verbatim decisions, recorded because seven items were logged as "needs a decision, do NOT
guess" and are now answerable. Where a ruling changes an item, the item itself is edited too.

1. **Platform SDKs: "we implement blindly, everything, but research the web again for fixes."**
   So WinRT, CGEventTap, IOHIDManager, hidraw, D-Bus/MPRIS etc. are all IN SCOPE. They cannot be
   run here, so the standard is: research the CURRENT API on the web first (not from memory),
   dlopen/probe rather than link, and cross-compile-verify. "Blindly" means without a device,
   NOT without checking the API exists.
2. **D-Bus specifically: "we already have APIs, we implement and dlopen blindly."**
3. **10a-iv (soft keyboard via two paths): "ideally we can refactor to one, but not
   game-breaking if not possible."** So: attempt the unification; if the two paths turn out to
   carry genuinely different intent, leave both and say why.
4. **11b-i (form reset / validation): "put as an extra attribute on the `form` node type."**
   So `Reset` and `Invalid` get their state from a form-node attribute rather than a new concept.
5. **10a-iii (Android IME hints): "same thing, default to whatever is the default input purpose
   if not set."** An input-purpose attribute, with the platform default when absent.
6. **7c-i (Windows touchpad pinch): "both. ctrl-wheel synthesize if we have a kbd+mouse setup,
   otherwise also DirectManipulation. Make a flag in AppConfig whether to disable the synthetic
   kbd+mouse pinch."**
7. **9g-ii-a/b (tuple and NodeId returns): "no, we have `impl_option!` and `OptionNodeId`
   already, make new structs if needed."** So these are NOT design questions - build the named
   structs and expose them.
8. **10c-iv (`env()`): "I don't think we need env() for now, low priority, also not tier-1."**
9. **10b-i (full `UITextInput`): "is important (including IME)."** Highest-priority remaining
   feature item.


Branch `feat/input-event-wiring`, stacked on **PR #450** (`fix/tablet-and-clipboard-linux`, 28 commits)
rebased onto `origin/master` @ fcef148b2. Spec: `scripts/INPUT_METHODS_AUDIT_2026_09_01.md`.

## Absorbed from PR #450 (Linux-tested — do NOT redo)

- `PenState.report_rate_hz` — measured EMA of the sample interval; no protocol carries a nominal rate.
- `TabletDeviceInfo` + `TabletToolKind { Unknown, Stylus, Eraser, Pad, Touch }` + `TabletDeviceInfoVec`,
  `CallbackInfo::get_tablet_devices()`, backend-populated at window init and on hotplug. **Already in api.json.**
- Wayland: `handle_tablet_frame` drives the pointer pipeline (cursor, hover hit-test, tip=left, barrel=right);
  `tool_button`/`hardware_serial`/`capability`/`slider` listeners were noops and now feed `PenState`; per-tool
  identity applied on `proximity_in`; `proximity_out` clears pen state and releases synthesized buttons.
- X11: sparse XI2 valuators reuse previous pen state (absent axis = unchanged, not zero); tip contact tracks the
  tip BUTTON; barrel buttons reach `barrel_button_pressed`; `device_id` is the slave sourceid; `FocusOut` resets
  pad + pen state.
- So `tangential_pressure`, `barrel_button_pressed`, `tool_id` and `device_id` are **no longer ragged on
  Wayland/X11**. macOS and Win32 are untouched by #450.

⚠ **Interaction with item 1a:** #450's Wayland bridge exists precisely *because* pen events dispatch to nothing
today — it synthesizes `Mouse*` so something reacts. Once 1a lands, real `Pen*` events start dispatching too.
Check for double-handling (a node subscribed to both `MouseDown` and `PenDown` will now get both) and decide
whether the bridge should suppress its synthetic mouse events when a `Pen*` subscriber exists.

## Rules for this arc

- **NOTHING is removed from api.json.** Unemitted variants get wired, never deleted. (User ruling 2026-09-01.)
- **DO NOT COMPILE while iterating.** No `cargo check`, no `cargo build`, no `cargo test`. We fix up at the END.
- Commit after each item, even if it doesn't build. Message: `wip(input): <item id> <what>`.
- api.json changes go via `azul-doc autofix` only — never hand-edited. If autofix can't run without a compile,
  record the intended delta in `scripts/INPUT_WIRING_APIJSON_TODO.md` and move on.
- Every wiring fix that adds a dispatchable event should also add a `HeadlessEvent` variant so it can be tested
  at the end.
- **Four layers must agree** for a filter to fire: (1) shell constructs `EventType`, (2) `event_type_to_filters`
  returns the filter, (3) `matches_*_filter` accepts the pair, (4) `matches_filter_phase` passes the family.
  Touch all four or the work is invisible.

## Status legend

`[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (note why)

---

## Step 0 — ratchet

- [x] 0a `core/src/events_test.rs`: extend `event_type_to_filters_never_panics_and_stays_synced_with_the_hover_matcher`
      from 2 layers × Hover to 4 layers × {Hover, Focus, Window}. Assert planning emits the filter, matcher accepts,
      phase gate passes. Keep `KNOWN_DESYNC` as the subset allow-list; entries get deleted as items below land.
- [x] 0b Prune the 6 stale `KNOWN_DESYNC` entries (`MouseOut`, `FocusIn`, `FocusOut`, `Composition{Start,Update,End}`)
      — their matcher arms exist at `events.rs:1438-1443`, so the entries protect nothing.

## Step 1 — C1: planning omissions (14 variants, zero shell work)

- [x] 1a `event_type_to_filters_legacy_hint`: add `E::PenDown`, `E::PenMove`, `E::PenUp`, `E::PenEnter`, `E::PenLeave`
      arms emitting Hover + Focus (Down/Move/Up only) + Window filters.
- [x] 1b Same fn: add `E::DocumentEdit => vec![EF::Focus(F::DocumentEdit)]`.

### Found while doing 1a (same bug class, not originally listed)

- [x] 1c `matches_focus_filter` had **no Pen arms at all**, though `FocusEventFilter` has carried
      `PenDown`/`PenMove`/`PenUp` since it was introduced. Planning naming the filter would not have been enough.
- [x] 1d `E::TouchStart/Move/End/Cancel` planned only the Hover half, though `WindowEventFilter` owns all four
      with matching same-name arms in `matches_window_filter`. A window-level touch listener never fired.

## Step 2 — C2: planning de-sync

- [x] 2a Split `E::Scroll | E::ScrollStart | E::ScrollEnd => vec![EF::Hover(H::Scroll)]` into three arms, each
      emitting its own Hover + Focus + Window variant.
- [x] 2b Matcher arms: `(RightMouseDown, EventType::ContextMenu)`, `(TextInput, EventType::KeyPress)`,
      `(TextInput, EventType::Change)`.
- [x] 2c Emit `ScrollStart`/`ScrollEnd` from the `ScrollInputSource` transitions the physics timer already computes
      (`layout/src/managers/scroll_state.rs` + callers).
- [x] 2d Emit `ContextMenu` from right-button-up, the Menu/Apps key, and Shift+F10 on all four desktop shells.

## Step 3 — C3: missing producers

- [x] 3a Emit `EventType::MouseOut` alongside every `MouseLeave` site.
- [x] 3b Emit `EventType::FocusIn`/`FocusOut` alongside every `Focus`/`Blur` site.
- [x] 3c `CompositionEventData { data, cursor_begin, cursor_end }` + `EventData::Composition` variant +
      `CallbackInfo::get_composition_*` accessors.
- [x] 3d Emit `Composition*` at the IME sites: Win32 `WM_IME_STARTCOMPOSITION`/`COMPOSITION`/`ENDCOMPOSITION`,
      macOS `setMarkedText:`/`unmarkText`/`insertText:`, Wayland `preedit_string`/`commit_string`/`done`, X11 XIM.
- [x] 3e ALREADY DONE UPSTREAM (landed after the audit was written — verified at `common/event.rs:9114-9175`: the three SystemChanges are deferred past callback dispatch, the events are constructed and propagated, and `clip_prevented` gates `apply_system_change`). No change needed. Original item: dispatch `EventType::Copy`/`Cut`/`Paste` to the focused node BEFORE pushing
      `SystemChange::{CopyToClipboard, CutToClipboard, PasteFromClipboard}`; the existing
      `post_callback_filter_system_changes(prevent_default, …)` gate then makes them interceptable.

- [x] 2c-iv `settle_scroll_gesture()` now has its caller: the terminate branch of
      `scroll_physics_timer_callback`. The seam the note asked for is the CHANGE QUEUE, not the manager —
      a timer callback holds only its own downcast state, but `timer_info.callback_info` reaches
      `LayoutWindow` the same way `trigger_virtual_view_rerender` already did. New
      `CallbackChange::SettleScrollGesture` + `CallbackInfo::settle_scroll_gesture()`, applied in
      `apply_user_change`. `CallbackChange` is `#[derive(Debug, Clone)]` with no `repr(C)` and appears in
      api.json only inside doc-comment TEXT, so no autofix was needed.
      Unconditional at terminate: the manager no-ops unless a gesture is open, so a timer that ran for a
      trackpad flick (already closed by `TrackpadEnd`) or a programmatic `scroll_to` adds nothing.
      Drain-then-clear is intact — the transition is queued, the EventProvider drain turns it into
      `ScrollEnd`, and `event.rs:8793` clears it in the same post-determination block as every other
      provider, so it fires exactly once.

- [x] 4f-i DONE, and the item's premise was STALE in the same way several others in this arc
      have been. libXrandr IS dlopened: `try_subscribe_xrandr` loads
      `libXrandr.so.2`/`libXrandr.so`, calls `XRRQueryExtension`, subscribes with
      `XRRSelectInput(RRScreenChangeNotifyMask)`, and the screen-change event is already
      dispatched by `xrandr_event_base`. No loader entry was needed.
      THE ACTUAL GAP was the second half of the item — the count diff. The handler only
      refreshed the monitor CACHE, so `MonitorConnected`/`MonitorDisconnected` never fired on
      X11: an app could see the new monitor list if it went looking, and was never told to look.
      macOS (`didChangeScreenParameters`), Win32 (`WM_DISPLAYCHANGE`) and Wayland (`wl_output`
      global/global_remove) all report it; X11 was the only one that did not.
      `note_monitor_count_change(before, after)` already existed for exactly this, so the fix
      mirrors the macOS arm: count, refresh, count, report.
      NOTE: these consumers only became reachable two firings ago — `MonitorConnected` and
      `MonitorDisconnected` are `EventFilter::Application`, and planning never probed the
      Application category at all until that was fixed. Wiring this before then would have
      produced events nothing could subscribe to.
      EVIDENCE: the count-diff logic had NO tests; added
      `a_monitor_count_change_becomes_arrivals_and_departures` and
      `an_unchanged_monitor_count_emits_nothing` — the second matters because a screen-config
      event also fires for a resolution or position change, and reporting those as
      disconnect-plus-connect would make an app tear down per-monitor state on every mode
      switch (that is `WindowMonitorChanged`). azul-layout 7561, azul-dll 1944, host check,
      Linux-target check and 8-target gate green. NOT runtime-verified — needs a real X server
      and a monitor to unplug.

### Follow-ups opened by 11a/11b — REVISIT AT THE END

- [x] 11a-i DONE. `Change` fired on blur of any node that had been the edit target, with no
      comparison of any kind - `was_editing` only asked whether SOME node was being edited. So
      focusing a field and tabbing straight back out emitted `Change` on a field nobody typed
      into, which is enough to re-run validation, mark a form dirty, or fire a save.
      `Change` is not "the value was edited" - `TextInput` already reports that, per keystroke.
      It is "the value COMMITTED and it differs from what the user found", which is what every
      form on the web means by it, and there was nothing to subtract the starting value from.
      FIX: `TextEditManager::value_at_focus`, snapshotted at the focus-gained half of the same
      function that emits the blur events, and compared at the blur half.
      KEYED BY NODE, not a bare string: focus moving A -> B -> A must measure A against A's own
      starting value. An unkeyed snapshot would measure A against B's, and since a snapshot is
      taken for EVERY focused node (editability is not knowable at that site), that case is the
      common one rather than a corner.
      `value_changed_since_focus` returns `Option<bool>`, where `None` means UNANSWERABLE - no
      snapshot, or one belonging to another node, reachable when focus is set programmatically.
      The caller treats that as "no change", because inventing a `Change` from a missing snapshot
      is precisely the bug being fixed.
      EVIDENCE: 7 tests - an untouched field reports no change AND an edited one does (or the fix
      would just suppress `Change` entirely, a worse bug); clearing a field counts; a snapshot is
      never compared against another node, nor across DomIds; no snapshot is distinguishable from
      no change; and re-focusing REPLACES the baseline rather than accumulating.
      azul-layout 7575 (+7), azul-dll 1963, azul-core 2759, host, 8/8 mobile, autofix converged.
- [x] 11b-i DONE — the whole form family (Submit, Invalid, Reset) now produces.
      ⚠ The investigation found `Submit` was ALSO unproducible, not just Reset/Invalid.
      `DefaultAction::SubmitForm` existed with a consumer whose comment claimed "Enter on a
      focused control produced this action" - but nothing ever constructed it, and the producer
      site admitted as much: "Enter on non-activatable element - might submit form / For now, no
      action". FIXED with `find_form_ancestor` + the Enter arm (HTML implicit submission).
      INVALID: `azul_layout::form::validate_form` checks every control in the form against the
      constraint attributes that ALREADY EXISTED - `Required`, `MinLength`, `MaxLength`, `Min`,
      `Max`. The item said validation needed "a `required`/`pattern` attribute or a validator
      callback"; the attributes were in the DOM all along.
      Validation runs BEFORE the submit and CANCELS it on failure, which is the HTML order:
      submitting anyway would hand the app data it had already declared unacceptable. One
      `Invalid` per failing control, in document order - all of them, not just the first, or the
      second error would only appear after the first was fixed.
      Written as a PURE function over the styled DOM with the value read through a callback, so
      it is testable without a live window - which is why it has 10 tests rather than none.
      DETAILS THAT ARE EASY TO GET WRONG, each pinned by a test: whitespace does NOT satisfy
      `required` (the classic hole); `minlength` does not fire on an EMPTY value (an empty
      required field would otherwise report two errors for one mistake); lengths are counted in
      CHARACTERS not bytes ("éé" is 2 chars, 4 bytes); `min`/`max` compare NUMERICALLY (a string
      compare calls "9" greater than "20"); a non-numeric value against a numeric bound is not a
      failure; and `Disabled`/`Readonly` controls are BARRED from validation entirely, because a
      disabled required field must not block a submit the user cannot fix.
      EVIDENCE: 10 validation tests + 5 form-walk tests; the Submit producer was
      negative-controlled (stubbing it back to `None` makes the test fail with `left: None`).
      The gate proven compiled. azul-layout 7596, azul-core 2760, azul-dll 1973, host, 8/8 mobile.

- [x] 11b-i-a DONE — `Reset`, completing the family. `DefaultAction::ResetForm` (appended for
      ABI) is produced when a control whose `InputType` is "reset" is activated, and consumed by
      firing `EventType::Reset` on the FORM - the sibling of `SubmitForm`, naming the form rather
      than the button, because a reset handler belongs on the form.
      ⚠ THE ORDER OF THE TWO CHECKS IS LOAD-BEARING. A reset button IS activatable, so the
      generic `ActivateFocusedElement` arm would have swallowed it and the form would never
      reset. The reset check therefore runs BEFORE activation, where the submit check runs after
      it - the specific case has to win over the general one. Negative-controlled: stubbing the
      check out makes the test fail with `left: ActivateFocusedElement`, which is exactly the bug.
      THE EVENT FIRES FIRST AND IS CANCELLABLE, which is HTML's order: an app that wants to
      confirm ("discard your changes?") calls `prevent_default` on the `Reset`, and the values
      must still be there when it does. Restoring first would make the veto meaningless.
      `azul_layout::form::default_values` supplies the restore set: each control's `Value`
      ATTRIBUTE, which is what HTML restores and why the "initial values to restore" the original
      item called missing turned out to be already in the DOM - the engine never had to remember
      them. A control with no `value` resets to EMPTY, also HTML's rule.
      Disabled and readonly controls ARE reset here, unlike in validation where they are barred:
      a reset is the APP's action, not the user's, and a browser clears a readonly field too.
      Only `Input`/`TextArea`/`Select` are touched - a form full of divs would otherwise have
      every one of them blanked.
      EVIDENCE: 4 new tests (default value, empty default, disabled/readonly still reset, the
      trigger) plus the negative control. azul-layout 7600, azul-core 2760, azul-dll 1973, host,
      8/8 mobile, autofix converged.

- [x] 11b-i-b DONE per the USER RULING (2026-09-03): `regex-lite` 0.1.9, minimal configuration -
      `default-features = false, features = ["string"]`, its `std` only under the crate's own
      `std` feature, no dependencies of its own. `ValidityReason::PatternMismatch = 5` appended
      (a pure enum append, as the bitset was designed for). HTML semantics: the whole value
      must match (`^(?:p)$`, so `[0-9]{3}` rejects `1234`), an empty value is exempt, and a
      pattern that does not compile is IGNORED like a browser ignores an invalid `pattern`
      attribute - `pattern_matches` answers `None` for it. Known deviation documented in the
      module: no `v`-flag Unicode sets / `\p{..}`; such patterns fail to compile and are
      therefore ignored, never mis-matched. Four tests pin anchoring, the empty exemption, the
      ignore rule and Unicode-aware `.`. ✅ COMPILED AND RUN in the second batch pass of 2026-09-03: host check EXIT=0; api.json
      converged (autofix EXIT=0, 0 patches; the four new CallbackInfo methods and six new types
      staged through `autofix add`); codegen green; core 2807, layout lib 7679, layout `--test
      all` 999, dll 2021 tests green; the e2e corpus (62 scenarios) green with the new
      spatial-navigation scenario RED under its negative control; the 8-target gate 8/8 after one
      iOS fix (objc2 `error: _` wants a typed NSError; explicit out-pointers now); the Android
      Java classes compile against android-34.
- [x] 11b-i-c DONE, and THE NOTE'S PROPOSED DESIGN WOULD NOT HAVE WORKED. It said exposing the
      reason "means a new `EventData` variant (an ABI addition) plus api.json". `CallbackInfo`
      never sees the `SyntheticEvent`: it carries the hit node and read-only access to the
      `LayoutWindow`, and `EventData` is not in api.json at all. That variant would have been an
      ABI addition NO APPLICATION COULD OBSERVE - the exact "looks done while doing nothing"
      shape the standing rules warn about, and it was written into the item.
      What every payload an app can actually read does instead is park itself in a manager and
      get fetched by an accessor - `peek_raw_motion` is the same shape - so that is what this
      does: `FormValidationManager` on the `LayoutWindow`, published BEFORE the events dispatch
      (a callback reading it after would be told its own field is fine), and
      `CallbackInfo::get_validity_state` / `get_validity_state_of`.
      A SET, NOT A REASON. One field can be both too short and out of range, and HTML's
      `ValidityState` is a set of flags for exactly that. That exposed a DEFECT in 11b-i:
      `validate_form` pushed one entry per failed CONSTRAINT, so such a field produced TWO
      `Invalid` events on one node - HTML fires `invalid` once per control, and an app marking
      bad fields would have marked that one twice and reported two errors for one mistake. Now
      one entry per control carrying every failure.
      A BITSET rather than a struct of bools, so appending the `PatternMismatch` that 11b-i-b
      will add stays a pure enum append with no change to `ValidityState`'s layout - the same
      trade `GamepadState::buttons` makes. The discriminants ARE bit positions, so a test pins
      them: renumbering would leave every stored state meaning something else with nothing
      failing to compile.
      `has()` and `is_valid()` are EXPOSED, not just the `flags` field: a binding handed a bare
      `u32` has no way to know which bit is which, and would have to hardcode the numbering this
      change just pinned.
      An unvalidated control reads as VALID rather than unknown, and a successful submit CLEARS
      the map - a field the user just fixed must stop reporting the error it no longer has.
      EVIDENCE: 3 core tests (bit positions distinct and pinned, a state holding two failures at
      once and idempotent, the clear-on-new-pass) + a new layout test with a NEGATIVE CONTROL -
      restoring one-entry-per-constraint fails with "one control must produce one entry, got
      [.. flags: 2 .., .. flags: 16 ..]". Both dispatch seams proven COMPILED by deliberate type
      errors. `codegen all` + a real dll build put `AzCallbackInfo_getValidityState`,
      `AzCallbackInfo_getValidityStateOf`, `AzValidityState_has` and `AzValidityReason` in the C
      ABI. autofix converged at 0 patches, `azul-doc check` PASSED. Host, 8/8 mobile, azul-core
      2770, azul-layout 7611, azul-dll 1992, azul-doc 18.
- [x] 10f-i DONE — `scripts/android/AzulGamepad.java` owns the `InputManager.InputDeviceListener`
      and forwards buttons/axes, plus a one-time enumeration for pads connected before the listener
      existed. Hotplug is the only part the native queue cannot deliver.

### Follow-ups opened by 10e

- [ ] 10e-i Web equivalents: `PointerEvent.getCoalescedEvents()` and `getPredictedEvents()`. The wasm
      loader binds only `pointerdown` today (see the audit's web column), so this needs the pointer-event
      migration first. Windows has `GetPointerFrameInfoHistory` for the coalesced half; Wayland and X11
      have no equivalent — a compositor delivers every sample, so there is nothing to un-coalesce.

### Follow-ups opened by 10d

- [x] 10d-i DONE, and the premise was stale in the WORSE direction: the note said the delegate
      methods "exist and are registered". They existed and were NEVER REFERENCED - `grep
      pencil_did_tap|pencil_did_squeeze` found only their own definitions, and `sel!(pencil
      Interaction` returned ZERO. So neither method was registered on the class AND no interaction
      object existed, leaving `PenSqueeze` and `PenDoubleTap` with no producer on any platform
      while looking implemented in the source.
      FIXED all three layers: both selectors registered on the view class,
      `UIPencilInteractionDelegate` conformance declared (optionally, like `UIKeyInput` above it -
      the protocol is absent from pre-12.1 SDKs and `Protocol::get` returning None must not abort
      class registration), and a `UIPencilInteraction` allocated, delegated to the view and
      added via `addInteraction:`.
      Gated on the CLASS existing rather than on a version number: `Class::get("UIPencil
      Interaction")` returning None IS the right answer on anything older than 12.1. The squeeze
      selector needs no separate gate - registering a method the OS never calls is harmless, which
      is simpler than the 17.5 check the note proposed.
      EVIDENCE: the attach proven COMPILED by a deliberate type error reported under
      `--target aarch64-apple-ios`. 8/8 mobile. Compile-only by user direction.

### Follow-ups opened by 10c

- [x] 10c-i DONE. `SafeAreaInsets::keyboard` had no producer on iOS: the field existed,
      `CallbackInfo::get_keyboard_inset()` read it, and Android filled it from
      `WindowInsets.Type.ime()`, but on iOS it was permanently `None`. That became urgent with
      10b-ii in the same session - a keyboard that can now actually be RAISED, over the field it
      was raised for, with the app unable to learn that it did, is worse than no keyboard.
      `UIKeyboardWillChangeFrameNotification` observed on the VIEW (the handler must convert into
      the view's coordinate space, so it needs the view as `self`). Chosen over `WillShow`/
      `WillHide` because it is the one notification covering all three transitions plus the ones
      neither of the others reports: a hardware keyboard attaching and shrinking the software one
      to the shortcut bar, the floating iPad keyboard being dragged, and a height change on
      language switch.
      The frame is converted and INTERSECTED with the view rather than used raw, exactly as the
      item said: an iPad split-view app owns a fraction of the screen while the keyboard belongs
      to all of it, so the raw height over-insets by however much lies outside the view. The
      overlap is clamped to the view height, and a keyboard that covers nothing writes `None`
      rather than `Some(0)` - the field's own docs distinguish those, and an app laying out around
      a zero inset would reserve a row for a keyboard that is not there.
      EVIDENCE: both the handler body and the observer registration proven COMPILED by deliberate
      type errors reported under `--target aarch64-apple-ios`. Compile-only by user direction.
      8/8 mobile.
- [x] 10c-ii DONE — `installInsetsListener` registers `setOnApplyWindowInsetsListener` and calls
      `nativeOnWindowInsets`, using `systemBars() | displayCutout()` (a notch is not part of
      systemBars) and keeping the IME inset separate from the bottom inset.
- [x] 10c-iii APP-FACING HALF DONE, and the premise of the original note was WRONG — checked, not assumed.
      The chain now carries real values on device:
        Java `installInsetsListener` -> `nativeOnWindowInsets` -> `LayoutWindow.safe_area_insets`
        -> `CallbackInfo::get_safe_area_insets()` / `get_keyboard_inset()`
      Verified on a headless android-34 emulator with the keyboard up:
        `insets top=24 bottom=0 left=0 right=0 ime=235` (physical px)
      i.e. the status bar (24) and the live IME height (235) both arrive.

- [ ] 10c-iv LOW PRIORITY (user 2026-09-03: "I don't think we need env() for now, low
      priority, also not tier-1"). `env()` IS NOT PARSED AT ALL. The 10c-iii note claimed "`env(safe-area-inset-*)` already
      resolves", and `scripts/MOBILE_SESSION_LOG.md` says so twice for macOS and iOS. It does not:
      `parser2.rs` has ZERO occurrences of `env`, there is no `"env"` function token anywhere in
      `css/src/`, and the `env(safe-area-inset-bottom)` in `doc/templates/flora.css:2892` is silently
      dropped as an invalid value. The insets are readable ONLY from app code.
      So the CSS-facing half is not "add one more variable name" — it needs `env()` implemented as a
      CSS function with a resolution context plumbed through the cascade. Rescoped rather than left as
      a small-looking follow-up on a false premise.

- [x] 10c-v DONE. The open question ("does the ENGINE inset the root automatically on mobile, or
      each app") is answered the way browsers answer it: the engine insets by default
      (`viewport-fit=auto`) and a window opts out with `WindowFlags::extend_into_safe_area`
      (`viewport-fit=cover`) when it wants to draw under the bars - a full-screen video, a map.
      `LayoutWindow::inset_by_safe_area` shrinks the root viewport by the platform's absolute
      insets (origin in by left / top, size minus all four, clamped at zero) inside
      `layout_dom_recursive_with_viewport`; child DOMs are placed by their host and never inset;
      the on-screen keyboard is deliberately not part of it (a transient occlusion the app reads
      from `get_safe_area_insets().keyboard`); desktops report zero insets, so nothing moves
      there. Four tests (`layout/tests/safe_area_inset.rs`): inset by default, cover fills the
      surface, the desktop no-op, and the clamp. AzWriter is not in this repository, so its title
      band is its own change. ✅ COMPILED AND RUN in the sixth batch pass of 2026-09-04: host check EXIT=0 after the api.json
      pass (`WindowFlags.extend_into_safe_area`, one modification, codegen green); core 2809,
      layout lib 7681 and `--test all` (the inset and registry tests among them, after three test
      fixes), dll 2035 tests green; the 8-target gate green - the Linux target after the seventh
      batch's one visibility fix, the other seven first time.

- [x] 10c-v-b DONE. `PixelValue` had 7 CONSTRUCTORS and ZERO functions in api.json: a binding
      could build one and never read it back. So an app handed the `OptionPixelValue` that
      `get_safe_area_insets()` returns had no sanctioned way to reach a number.
      The premise needed one correction: `PixelValue.number` IS `pub` and `FloatValue::get` IS
      exposed, so `p.number.get()` really does work - the note was right that it is the only
      route, and right about why that is bad. It reports `24` for `24em` exactly as readily as
      for `24px`, so it is correct only while the caller happens to know the value is absolute
      and nothing in the type system says so.
      ADDED three functions (autofix, now in the C ABI as `AzPixelValue_isAbsolute` /
      `_toPixelsAbsolute` / `_toPixels`):
        - `is_absolute() -> bool` - true for px/pt/in/cm/mm.
        - `to_pixels_absolute() -> OptionF32` - the honest answer for values that are absolute by
          construction (a safe-area inset, a system scrollbar width). Returns `None` rather than a
          number for a relative unit, because a relative unit HAS no pixel value until something
          supplies the reference - inventing one is exactly how `to_pixels_internal` reports every
          viewport unit as `0.0`.
        - `to_pixels(percent_resolve, em_resolve, rem_resolve) -> f32` - the general form,
          delegating to the engine's own resolver so the two cannot drift.
      EVIDENCE: 4 tests - every absolute unit resolves through `to_pixels_absolute` (in/cm/mm
      included, which the escape hatch got wrong); every context-dependent unit returns `None`;
      `is_absolute` and `to_pixels_absolute` can never disagree; and `to_pixels` is byte-identical
      to `to_pixels_internal` rather than a second implementation. `codegen all` + dll build
      confirms the three reached the generated bindings, not just api.json. azul-css 2860 (+4),
      azul-core 2759, azul-layout 7564, azul-dll 1963, azul-doc 209, 8/8 mobile.

- [x] 10c-v-a DONE (the widget half; the CSS route is still 10c-iv). The previous attempt was
      measured wrong for a reason worth keeping: padding the title band by the inset changed
      NOTHING because `theme_bar` sets a FIXED `height: BAR_HEIGHT` (28) on the band root, so
      `padding-top: 24px` squashed 28px of content into 4px instead of displacing it.
      FIX: `QuickAccessBar::top_inset` (f32, logical px, default 0.0) ADDS to the band height and
      pads by the same amount, so the content box stays exactly `BAR_HEIGHT` tall and the band
      grows upward into the status bar - which also makes the band's own background fill the
      notch area instead of leaving a gap above it.
      Applied at `dom()` rather than in `theme_bar`, because the inset is a property of the WINDOW
      (which notch, which orientation) and not of the theme: two bands with the same look can need
      different insets, and `QuickAccessStyle` is shared. Appending overrides the height already
      pushed, which is the same later-declarations-win mechanism `merged_style` relies on to let
      the close button restyle the window button.
      Feeds from `get_safe_area_insets().top` via `PixelValue::to_pixels_absolute()` - which is
      why 10c-v-b had to land first: before it there was no sanctioned way to turn the returned
      `OptionPixelValue` into the `f32` this field takes.
      api.json ALSO gained the 12 struct fields it had been missing: it listed ONE (`style`) of
      the 14, so `title`, `actions`, `show_close` and the rest were invisible to every binding.
      That was pre-existing drift the sync surfaced, not something this change caused.
      EVIDENCE: 3 tests - the content box stays `BAR_HEIGHT` after insetting (asserting
      `height - padding == BAR_HEIGHT`, the exact relationship the padding-only attempt violated);
      a zero inset appends NO declarations, so desktop is byte-identical; and the default is 0.0
      on all three constructors. `codegen all` + dll build put `top_inset` and
      `AzQuickAccessBar_withTopInset` in the C ABI. autofix converged at 0 patches / 0 FFI errors.
      azul-css 2860, azul-core 2759, azul-layout 7567 (+3), azul-dll 1963, azul-doc 209, 8/8 mobile.


### Follow-ups opened by 10b

- [x] 10b-iv NEW, found by scanning iOS against the other backends (user: "scan ios, I think
      there's a lot of code simply missing" - they were right). iOS produced NO SCROLL INPUT OF
      ANY KIND. A finger drag set the emulated mouse position and nothing else, so no scroll
      container on iOS could ever move. `grep scroll_manager` under `ios/` returned ZERO, against
      1 in android and 7 each in macOS and Windows - and the single Android hit is the entire
      touch-pan producer, which iOS simply never grew.
      FIXED by mirroring Android: a per-window `touch_pan_last` anchor (a touch stream reports
      POSITIONS, the scroll manager consumes DELTAS, so something must remember the previous
      point - per-window rather than global because an iPad can pan two windows at once), deltas
      on move, and a `TrackpadEnd` with zero delta on the last finger lifting, which is what
      releases a rubber-banded axis back to its bounds.
      Placed AFTER `update_hit_test_at`: `record_scroll_from_hit_test` resolves which container
      to scroll through the hover manager, so it needs THIS touch's hit test, not the previous
      one's. Uses the finger delta, not its inverse - `record_scroll_input` applies the
      natural-scroll sign centrally, and Android measured on device that inverting here as well
      drove the offset negative and rubber-banded against the top edge.
      Only the LAST finger ends the pan, matching the `left_down` rule directly above it;
      otherwise lifting one finger of a two-finger gesture snaps the content back mid-scroll.
      EVIDENCE: proven COMPILED by a deliberate type error at the `record_scroll_from_hit_test`
      call being reported under `--target aarch64-apple-ios`. Compile-only by user direction
      (simulator later). Host + 8/8 mobile.

- [x] 10b-i DONE — full `UITextInput` conformance (27 required members, two `UITextPosition`/
      `UITextRange` subclasses, Apple's `UITextInputStringTokenizer`), in a new
      `ios/text_input.rs`.
      WHY IT MATTERED: `UIKeyInput` has three methods and no concept of a POSITION. Everything
      the system layers on a text field - an IME's MARKED TEXT, selection handles, the edit menu,
      dictation, Scan Text from Camera, the caret rect VoiceOver reads - goes through
      `UITextInput`. So a Japanese or Chinese user on iOS could only type what their IME had
      already committed: the candidate bar arrived as finished text and no preedit was ever shown.
      THE RESEARCH CHANGED THE DESIGN. I first built a document model that spliced the preedit at
      the caret, which needs a bridge from UIKit's flat byte offsets to azul's grapheme-cluster
      addressing (`TextCursor` is a `(source_run, start_byte_in_run)` pair) - the compiler
      rejected it, because `TextCursor` has no `char_index` and no `set_selection_byte_range`
      exists. Checking how macOS - the shell where IME ALREADY WORKS - solves it settled the
      design: it does not bridge either. Its `markedRange` reports `(0, preedit_len)` and its
      `selectedRange` is a fixed `(0, 0)`, with a comment recording that `NSNotFound` there stops
      the IME talking at all. This follows that proven shape instead of inventing an untestable
      bridge.
      TOTALITY IS THE SAFETY PROPERTY, and it is what the old note was really warning about:
      UIKit probes for the protocol and then CALLS it, so a nil where the header says non-null
      crashes inside UIKit rather than degrading. Every position returned is clamped into
      `0..=len`, every range is ordered on construction (UIKit hands out reversed pairs while
      dragging backwards), `selectionRectsForRange:` returns an EMPTY array rather than nil
      (UIKit enumerates it unconditionally), `caretRectForPosition:` never returns a zero-HEIGHT
      rect (that puts the magnifier off-screen), and every offset that reaches a slice goes
      through `clamp_to_char_boundary` first so a multi-byte candidate cannot panic.
      `unmarkText` COMMITS rather than discards - discarding would delete what the user just
      chose. An interior `replaceRange:` is deliberately DROPPED rather than applied as an
      insert: falling through would append the autocorrection instead of replacing it, silently
      duplicating text on every correction. Losing a correction is a limitation; corrupting the
      document is a bug.
      EVIDENCE: `register` proven COMPILED by a deliberate type error under
      `--target aarch64-apple-ios`; the protocol is declared only AFTER all 27 methods are added,
      so a class missing one can never be advertised as conforming. Host, 8/8 mobile, azul-dll
      1973. ⚠ COMPILE-ONLY - not run on a device or simulator.

- [x] 10b-i-a DONE. ⚠ "A LAYOUT-QUERY SEAM THAT DOES NOT EXIST" WAS WRONG - every piece existed
      and none of them were joined up. `byte_offset_to_cursor`, `UnifiedLayout::get_cursor_rect`
      and `UnifiedLayout::hittest_cursor` have all been there for a long time, and
      `get_focused_cursor_rect` is literally two of them called together for the ONE offset the
      engine happened to be holding. That is now the fourth item this arc whose premise was a
      missing READER rather than a missing model.
      Three new queries on `LayoutWindow`: `focused_rect_for_byte_offset`,
      `focused_rect_for_byte_range` and `focused_byte_offset_for_point` - the bridge between the
      shells' flat byte offsets and `TextCursor`'s grapheme-cluster ids, in both directions.
      WHAT IT FIXES, concretely: `caretRectForPosition:` put the IME candidate window and the
      loupe against the FIELD instead of the character; and `closestPositionToPoint:` answered
      "the end of the document" for every point, so a tap anywhere placed the caret at the end
      and a drag inside a selection jumped to its far edge.
      TWO TRAPS in the conversions. The hit test works in NODE-RELATIVE coordinates while the
      shell hands in window ones - mixing them is off by the node's position on screen, which on
      a scrolled page is the whole error. And a cursor names a cluster inside ONE RUN, so
      `cursor_byte_offset_in_run` alone puts every offset in a multi-run paragraph at the wrong
      character; the runs before it have to be counted back in.
      `firstRectForRange:` IS THE FIRST LINE'S PART, NOT A BOUNDING BOX - that is the protocol's
      contract, and a union would place the candidate window beside a rectangle covering text
      whose start the user cannot see. Extracted as `first_line_span` so the decision is
      testable, with a sub-pixel tolerance: two carets on one line differ by rounding, and an
      exact comparison would call that a line break and drop the range's width.
      The node box REMAINS the fallback, deliberately: a field with no live text layout has no
      glyph to point at, and answering zero would put the candidate window in the screen corner.
      EVIDENCE: 4 tests over the range contract with a NEGATIVE CONTROL - dropping the line check
      fails with "a multi-line range must not become a bounding box". All three iOS seams proven
      COMPILED under aarch64-apple-ios. Host, 8/8 mobile, azul-layout 7625. ⚠ No simulator run -
      compile-only, and no candidate window has actually moved.
- [x] 10b-i-b DONE - all three, and the note was right that they are ONE problem: the bridge
      10b-i-a built in the read direction needed a write direction, and then every limit fell out.
      `byte_offset_of_cursor` (extracted from 10b-i-a's point lookup),
      `focused_caret_byte_offset`, `focused_selection_byte_range` and
      `set_focused_selection_from_byte_range` are that bridge, both ways.
      1. THE PREEDIT IS SPLICED AT THE CARET. Appending is the natural shortcut and is wrong the
         moment someone composes mid-field: the preedit landed after text that comes AFTER it, so
         the candidate window pointed at the wrong place and every offset UIKit derived from the
         string was past the real insertion point.
      2. `selectedTextRange` REPORTS THE REAL SELECTION. It answered "a caret at the end of the
         document" for everything, so an IME asking where the user was typing was told "at the
         end" wherever they actually were.
      3. `setSelectedTextRange:` IS APPLIED, so a dragged selection handle no longer springs
         back. This is the one that needed the write direction: fabricating a cursor by counting
         characters would land between the bytes of a multi-byte grapheme, which is why the ends
         resolve against the SHAPED LAYOUT.
      4. AN INTERIOR `replaceRange:` IS APPLIED instead of dropped - select the range, delete,
         insert, in that order. It was dropped because falling through to an insert would have
         APPENDED the correction rather than replacing it, duplicating text on every autocorrect;
         the fix is the selection seam, not a different fallback.
      `splice_preedit` LIVES IN `azul_layout::window`, NOT IN THE iOS SHELL, for two reasons: the
      macOS shell has the identical limit and now has the fix available, and a file cfg-gated to
      one platform is a file whose tests never run here - the same lesson as `sensors/units.rs`.
      It clamps and snaps DOWN to a character boundary, which is not hypothetical: the caret is
      in bytes and CJK, the text an IME exists for, is three bytes per character - slicing one in
      half panics.
      EVIDENCE: 5 splice tests with a NEGATIVE CONTROL - restoring the append fails with
      "hello worldXY" against "helloXY world". All four iOS seams proven COMPILED under
      aarch64-apple-ios. Host, 8/8 mobile, azul-layout 7634. ⚠ No simulator - compile-only.
- [x] 10b-i-b-i DONE - and the reason it was a "wiring job" undersold it: `NSTextInputClient`
      measures EVERY range in UTF-16 units of the client's string, and the engine measures in
      bytes. The two agree on ASCII and on nothing else, which is exactly why the old client
      "worked": every range answer was wrong by construction for the scripts an IME exists for.
      What was there: `markedRange` = `(0, preedit_len)` in BYTES at location 0 (not where the
      caret is); `selectedRange` = `(0, 0)` always; `characterIndexForPoint:` = NSNotFound;
      `attributedSubstringForProposedRange:` = nil ("I have no text"); `firstRectForCharacterRange:`
      = the caret rect whatever range was asked for; and `setMarkedText:`'s `selectedRange` -
      UTF-16 units relative to the marked string - handed to `set_preedit` AS BYTES, so the
      composition caret sat a third of the way into the wrong kana. An explicit
      `replacementRange` on `insertText:` (autocorrect replacing a committed word) was reported
      and DROPPED, inserting the correction beside the word it replaced.
      NOW, on the same document iOS answers from: `LayoutWindow::ime_document` (moved out of
      the iOS shell - the committed text with the preedit spliced at the caret; iOS delegates
      to it) plus `utf16_offset_to_byte` / `byte_offset_to_utf16` / `utf16_range_to_bytes` /
      `byte_range_to_utf16` in `azul_layout::window` (host-tested: CJK, surrogate pairs snap
      DOWN to the scalar start, clamps, reversed ranges, boundary round-trips), and
      `ime_selected_byte_range` for the one rule that needed writing down: with a composition
      open the selection is INSIDE the marked text (the IME's own selected sub-range rebased onto
      the document), otherwise the committed selection, otherwise a caret at the END - a real
      insertion point, never NSNotFound, because that stops AppKit sending `insertText:` at all
      (the old `(0, 0)` stub's one valid reason, kept with the right location).
      `insertText:` with an explicit range that is neither the composition nor the selection is
      applied the iOS way: select, delete, insert, in that order. `characterIndexForPoint:` goes
      screen -> window -> azul top-left -> `focused_byte_offset_for_point` -> UTF-16.
      `firstRectForCharacterRange:` resolves the ASKED range through `focused_rect_for_byte_range`
      and fills `actualRange`, falling back to the live caret rect and then the cached IME
      position as before. An empty `setMarkedText:` string is now a cancel (AppKit clears marked
      text by marking the empty string), un-shaping the composition like `unmarkText`.
      DEDUP: the protocol impl existed TWICE, once per view class (`GLView`, `CPUView`), 230
      byte-identical lines drifting only in comments. Both are now thin wrappers over one set of
      `ime_*` functions; the view contributes its window pointer, its frame height (the y-flip)
      and its `ime_key_handled` latch, nothing else.
      EVIDENCE: 7 host tests on the conversions and the selection rule; NEGATIVE CONTROL:
      treating UTF-16 units as bytes fails the CJK, emoji and round-trip tests. Host check (the
      macOS shell IS the host), 8/8 mobile (the iOS delegation compiled on the three Apple
      targets). ⚠ NOT DRIVEN BY A REAL IME HERE - the user said to check at the end; the
      Japanese/Chinese composition path on macOS is the first thing to try then, and the traces
      (`[IME markedRange]` etc.) are in place to read.
- [x] 10b-i-b-i-a DONE for the half that has a contract, and the contract was re-read rather
      than remembered: the 10.6 `NSTextInputClient.h` says the receiver inserts the marked
      string "replacing the content specified by replacementRange", Apple's current page adds
      "if there is no marked text, the current selection is replaced", and every reference
      client (WebKit, Chromium, Flutter's macOS embedder) reads the range as DOCUMENT content and
      selects it before acting. So the "replace this range with a composition" seam is the three
      steps a reconversion is made of: select the range, delete it (the caret lands at its
      start), compose there - the same select/delete the interior `insertText:` already did.
      ONE RULE for both members, `azul_layout::window::ime_replacement_action` (host-tested):
      `NSNotFound`, an empty range, the composition itself, or the current selection = act at
      the caret; a committed range with no composition open = `ReplaceCommitted`; a foreign
      range DURING a composition = `NotHonoured`, reported. `setMarkedText:` additionally deletes
      a live selection even with an implicit range - "the current selection is replaced" - which
      it never did: composing with text selected used to shape the preedit beside the selection.
      ⚠ `unmarkText` WAS A DISCARD AND IS NOW AN ACCEPT. AppKit: "the text view should accept the
      marked text as if it had been inserted normally"; Flutter's embedder commits
      (`CommitComposing`), WebKit and Chromium likewise. Our implementation treated it as a
      cancel and dropped the composition - and `unmarkText` is what AppKit sends when FOCUS
      LEAVES mid-composition (a click into another field, Cmd-Tab), so the half-typed word
      vanished. A real cancel arrives as `setMarkedText:` with the empty string, which is still
      handled as one. This is also what keeps a reconversion from losing the committed text it
      replaced. The old comment's reason ("composed glyphs stayed on screen") is preserved: the
      accept path un-shapes the preedit exactly as `insertText:` does before inserting.
      EVIDENCE: 4 host tests on the rule (implicit/empty; the composition itself and the
      selection either way round; a committed range replaced with a reversed one ordered; a
      foreign range during a composition reported). NEGATIVE CONTROL: answering `Implicit` for a
      committed range fails the reconversion test. Host check (the macOS shell is the host),
      8/8 mobile. ⚠ Not driven by a real IME - reconversion (select committed text, Kotoeri's
      "reconvert") is the second thing to try at the end-check, after ordinary composition.
- [ ] 10b-i-b-i-b An explicit `replacementRange` DURING a composition that is not the marked
      text (`NotHonoured`) is still applied at the caret. The offsets index the document WITH the
      preedit spliced in, and honouring them means un-shaping the preedit, re-basing the range
      onto the committed text, and re-composing - three steps whose ordering no reference client
      spells out, and no IME on hand sends this shape. Logged, not guessed.
- [x] 10b-ii DONE. On iOS the keyboard is not something you show - it is a CONSEQUENCE of a view
      becoming first responder while conforming to `UIKeyInput`. The view had conformed and
      answered `canBecomeFirstResponder = true` since 10b, and implemented `insertText:` /
      `deleteBackward` / `hasText`, but NOTHING ever asked it to become first responder. So the
      whole conformance sat unused and no keyboard could appear on any iOS device: not a missing
      capability, a missing call.
      TWO halves, both needed, and the item only named one:
        1. The FOCUS-DRIVEN raise. `set_soft_keyboard_visible()` in `common/event.rs` is shared by
           every backend and had an Android arm only, so tapping a text field raised nothing on
           iOS. It now routes to `becomeFirstResponder`/`resignFirstResponder`. This is the half
           that makes ordinary typing work and it was NOT in the item's description.
        2. The APP's explicit request - `CallbackInfo::request_soft_keyboard()` ->
           `CallbackChange::RequestSoftKeyboard` -> `take_soft_keyboard_request()` - drained on
           the `CADisplayLink` tick, the same per-frame slot that already drains a11y actions and
           polls the appearance. This is the half the item named.
      Both call one `set_soft_keyboard_visible` in `ios/mod.rs`, so the two paths cannot drift.
      The responder chain can REFUSE (a system alert holding first responder), so the BOOL is
      bound and logged rather than discarded - the request is advisory, exactly as on Android.
      EVIDENCE: all three edits proven COMPILED rather than cfg'd out, which a green
      `--target aarch64-apple-ios` build alone does not show. Deliberate type errors inside the
      iOS `set_soft_keyboard_visible` body and inside the tick drain were both reported; and
      RENAMING the iOS function broke `common/event.rs:576`, proving the shared arm really
      resolves to it on an iOS build rather than being compiled out.
      NOT verified on a device - there is no Xcode in this environment (see
      [[mobile_tooling_2026_09_01]]), so this is a compile-and-inspection result, not a
      keyboard-appeared-on-screen result. Host check, 8/8 mobile, azul-core 2759,
      azul-layout 7567, azul-dll 1963.
- [x] 10b-iii DONE, and the premise UNDERSTATED it. `key.modifierFlags` was indeed never read -
      but the deeper cause is that iOS never maintained `pressed_virtual_keycodes` AT ALL:
      `handle_presses` set only `current_virtual_keycode`. `modifiers` is DERIVED from the pressed
      set by `sync_modifiers()` (9e-i), so with that set permanently empty every modifier read as
      up and no Cmd- or Shift-shortcut could match however the key was pressed. iOS was the one
      backend absent from 9e-i's nine sync sites - a grep for `sync_modifiers|pressed_virtual_
      keycodes` across the shells returns ZERO hits in `ios/`, against 28 in x11 and 17 in wayland.
      FIXED all three layers: the pressed set is now maintained on press and release, `locks.
      caps_lock` is read from `UIKeyModifierAlphaShift` (a lock is a toggle no key event
      describes, so it must come from the OS - same as macOS/X11/Windows), and `sync_modifiers()`
      runs after each mutation.
      `modifierFlags` is read from the KEY rather than accumulated from presses, for the same
      reason macOS reads `NSEventModifierFlags`: it is the live state, so it stays right across a
      press that arrived while the app was backgrounded.
      Compile-verified for iOS only (user: "just cross compile to see that it compiles, we'll test
      in the simulator later"). Host + 8/8 mobile.
- [x] 10a-i DONE — `scripts/android/NativeTextBridge.java` exists and ships in the APK: an
      `AzulInputView` (focusable, `onCheckIsTextEditor`) supplying a `BaseInputConnection` that
      forwards commit/compose/finish/delete into the existing JNI entry points, plus showKeyboard/
      hideKeyboard via `InputMethodManager`. Verified on a headless android-34 emulator: the keyboard
      opens from a tap and `commitText("hi ")` reaches Rust.

- [x] 10a-iv DONE, per the ruling ("ideally we can refactor to one"). THE MECHANISM IS NOW ONE;
      the two POLICIES stay, with the reason stated.
      The two paths were not just duplicated, they were UNORDERED. The shell called
      `set_soft_keyboard_visible` directly from the blink-timer arms while the app's request was
      drained separately per platform - two writers to one piece of OS state with nothing
      sequencing them, so an app that focused a field and then asked for the keyboard to stay
      down raced its own request. Both now write `pending_soft_keyboard` and ONE drain at
      `process_window_events` depth 0 applies it, beside the haptic drain and for the same
      reasons. The per-platform drain in the Android loop is gone: a second drain would race the
      first, and whichever ran would consume the request while the other saw nothing.
      LAST WRITER WINS is the whole ordering rule, and it is the right one: a callback runs AFTER
      the focus change that triggered it, so the app's call is an OVERRIDE of the focus default
      rather than a coin flip.
      ⚠ AND THE DOC COMMENT WAS LYING. `request_soft_keyboard` claimed "focusing a text field
      does NOT do this implicitly" - the engine has always raised the keyboard on focus for the
      mobile shells, which is precisely why tapping a field in an existing app works when no app
      calls this API. The contract as written would have left every mobile text field dead until
      each app opted in. The doc now describes the override the code actually implements, and
      names the cases the old comment named (restoring focus after a dialog, a programmatic focus
      at startup) as what the `false` call is FOR.
      That is also the answer to the policy question the item deferred: the engine cannot
      distinguish user focus from programmatic focus, but it does not need to - the app can, and
      the override is how it says so.
      EVIDENCE: an ordering test over both directions plus the drain-once rule. Host, Android,
      8/8 mobile, azul-layout 7637, autofix 0 patches.
- [x] 10a-iii DONE. ⚠ THE "DESIGN QUESTION" WAS ALREADY ANSWERED IN THE DOM. The note said this
      "needs an input-purpose attribute on the DOM node first, which is a design question, not
      plumbing" - but `AttributeType::InputType` is HTML's `type` attribute and has existed all
      along, exactly as `NodeType::Form` and the validation attributes did for 11b-i. Three items
      in a row have now turned out to be missing a READER, not a model.
      `onCreateInputConnection` hardcoded `TYPE_CLASS_TEXT | TYPE_TEXT_FLAG_MULTI_LINE` with
      `IME_ACTION_NONE` for every field: a phone-number field had no phone pad, an email field no
      `@` key, and a single-line input showed a NEWLINE key with no way to dismiss the keyboard.
      `input_purpose` maps the `type` attribute onto the 8 purposes that change what a keyboard
      shows (a deliberate subset - `type="checkbox"` has no keyboard, so it is `Text` rather than
      a variant nobody would branch on), and `is_multiline` distinguishes `TextArea` and
      contenteditable from single-line controls.
      MULTILINE AND AN ACTION KEY ARE MUTUALLY EXCLUSIVE: Enter has to be a newline, so multiline
      forces `IME_ACTION_NONE`. Setting both makes Enter ambiguous and some IMEs drop the newline.
      Unknown and absent `type` both give `Text` - the PLATFORM DEFAULT the ruling asked for, so
      a `type` a future HTML adds cannot produce the wrong keyboard.
      THE DISCRIMINANTS ARE A WIRE FORMAT: `NativeTextBridge.java` switches on these exact ints,
      and renumbering the enum would silently give every field the wrong keyboard. Same hazard as
      the sensor codes, same guard - a test pins all 8.
      Hints are PACKED into one JNI call (`purpose | multiline << 8`) because
      `onCreateInputConnection` runs while the IME is opening and every crossing is a chance for
      the window to have gone.
      EVIDENCE: 8 new tests including the wire-code guard and case-insensitivity; the JNI seam
      proven COMPILED under `--target aarch64-linux-android` with `_internal_deps`; and the JAVA
      COMPILED against android-34 with `javac`, which nothing in the Rust build validates. Host,
      8/8 mobile, azul-layout 7604. ⚠ No device - compile-only.
- [x] 9h-i LINUX HALF DONE (both backends). macOS remains open as 9h-i-b.
      Neither Linux backend mapped a SINGLE multimedia keysym: `grep XF86` across the whole x11
      directory returned ZERO, so Play/Pause, Volume, Prev/Next and the browser keys produced
      nothing at all on X11 or Wayland - while the engine had modelled every one of those
      `VirtualKeyCode`s all along and Win32 already emitted them via `WM_APPCOMMAND`.
      22 keysyms added to `defines.rs` and mapped in `keysym_to_virtual_keycode`, which Wayland
      SHARES (it calls the x11 function at two sites), so one table fixed both backends.
      The constant VALUES were extracted from the real `/opt/homebrew/include/X11/XF86keysym.h`
      on this machine rather than recalled, and a test asserts each against its literal: a
      constant that is merely self-consistent maps a real key to the WRONG action, silently,
      which is the worst possible failure for this kind of table.
      Two mapping decisions worth naming: `AudioPlay` and `AudioPause` are separate keysyms and
      fold onto one `PlayPause` code, so a keyboard with a dedicated Pause key is not dropped;
      and `XF86XK_Back`/`Forward` map to `WebBack`/`WebForward`, NOT to `NavigateBackward`/
      `NavigateForward`, which are the two extra MOUSE buttons - a test pins that they do not.
      EVIDENCE: 6 tests (constants-vs-header, transport, volume, browser-vs-mouse, launch/power,
      and a negative control that unmapped keysyms INSIDE the range still return `None`, which a
      range-based implementation would get wrong). They run on Linux only - the x11 module is
      cfg'd out on the host and linux test binaries cannot link on macOS - so they are verified
      by `cargo check --tests --target x86_64-unknown-linux-gnu` here and execute in CI.
      ALSO FIXED, pre-existing and unrelated: `dll/tests/headless_window_features.rs` had not
      been updated when `TouchPoint` gained `major`/`minor`/`orientation_rad`/`tool_type`, so
      `cargo check --tests -p azul-dll` was RED on every target and had been for some time. That
      is what stopped the new tests being verifiable at all; confirmed pre-existing by stashing.

- [x] 9h-i-b DONE — macOS media keys via `MPRemoteCommandCenter`, NOT a CGEventTap.
      ⚠ THE NOTE OFFERED TWO ROUTES AND THE FIRST IS THE WRONG ONE. It said this "needs a
      CGEventTap (and therefore the accessibility permission) or the MediaPlayer framework's
      remote-command centre". The tap needs the ACCESSIBILITY TCC gate - the one that lets an app
      read every keystroke system-wide - which is a heavy thing to ask for a play button, and it
      intercepts the keys from every other app. `MPRemoteCommandCenter` needs NO permission and
      is the sanctioned API. Firefox uses it for exactly this, and reading their
      `MediaHardwareKeysEventSourceMacMediaCenter.mm` is what confirmed the shape.
      IT IS THE SAME BARGAIN AS MPRIS UNDER A DIFFERENT NAME: macOS delivers media keys only to
      the app it considers "now playing", and an app becomes that by setting
      `MPNowPlayingInfoCenter.playbackState`. So registering puts the app in Control Center and
      the Now Playing widget, exactly as claiming a bus name puts it in GNOME's media applet.
      WITHOUT the playbackState line the commands never fire at all - it is not decoration.
      SO THE FLAG WAS RENAMED: `expose_mpris_media_controls` -> `expose_system_media_controls`.
      One concept ("publish this app to the OS as a media player") with two implementations, and
      the old name was Linux-specific. It landed earlier in this same session and had not
      shipped, so renaming it now is free; leaving it would have meant a second flag for the same
      decision.
      Handlers are ObjC BLOCKS via `RcBlock`, the mechanism the camera-authorisation and
      audio-sink backends already use. Each block is deliberately LEAKED: the command centre
      holds it and calls it later, so it must outlive the registering scope - there is one per
      command for the life of the process and this backend never unregisters.
      The handler PARKS into the same channel the MPRIS backend uses rather than acting: it runs
      on whatever thread the media daemon calls on, and the engine's key pass belongs to the main
      thread.
      MediaPlayer.framework is dlopen'd, like ScreenCaptureKit and IOKit beside it.
      EVIDENCE: macOS is the HOST, so this is a real build of the real path - all three seams
      (registration, handler body, playbackState) proven COMPILED by deliberate type errors, zero
      warnings from the new module. Linux, Windows, 8/8 mobile, azul-core 2760, azul-layout 7581,
      azul-dll 1973, autofix converged and `codegen all` re-ran for the renamed field.
      ⚠ Not RUN: registering would put this machine's build in Control Center, and there is no
      way to assert a media key arrived.
- [x] 9h-i-a DONE — Linux MPRIS over D-Bus, opt-in.
      9h-i's keysym table is the whole answer only WHEN NOTHING GRABBED the keys, and every
      mainstream desktop grabs them: GNOME and KDE bind the media row globally and route it to
      registered players, so `XF86AudioPlay` never reaches the focused window as a keysym at all.
      The transport in that case is MPRIS, and it arrives on a D-Bus thread rather than in a
      window's event stream.
      Built on `zbus 5`, which is already a NON-OPTIONAL Linux dependency (the GeoClue2
      geolocation client uses it) - so nothing new is linked and it cross-compiles. The blocking
      API on a dedicated thread mirrors that backend exactly.
      OPT-IN via `AppConfig::expose_mpris_media_controls`, default FALSE, and that is a product
      decision rather than caution: registering makes the app APPEAR IN THE DESKTOP'S MEDIA
      CONTROLS as a player. Correct for a music app, wrong for a text editor, and no engine-side
      signal distinguishes them - so the app says which it is. Same AppConfig mechanism the user
      blessed for the pinch flag; the default differs because a pinch is invisible and a media
      widget entry is not.
      METHOD CALLS BECOME ORDINARY KEY PRESSES, matching the contract every other producer
      follows (`WM_APPCOMMAND` and the keysym table both deliver `PlayPause` as a normal key), so
      an app binding it works whether the key arrived raw or over D-Bus. Press AND release
      together, for the same reason `WM_APPCOMMAND` does it: neither transport has a release and a
      latched key looks held forever.
      Drained in `process_window_events` beside the haptic drain, NOT in the capability pump -
      emitting a key needs a PASS, and `pump()` only has the `LayoutWindow`. Found by trying the
      pump first and reading its signature.
      THE PROPERTIES ARE STUBBED AND THAT IS LOAD-BEARING, not laziness: `PlaybackStatus` is
      required by the spec and read by every desktop, and omitting it makes some of them treat
      the player as broken and hide it - taking the transport buttons with it. So the stub is
      what makes the KEYS work. `CanSeek` is the one flag reported FALSE: seeking needs a
      position azul does not have, and claiming it would put a dead scrubber in the desktop UI.
      `Quit`/`Raise` are inert with `CanQuit`/`CanRaise` false - a media widget's close button
      terminating the app would be a surprise, and raising is a window-manager action the shell
      owns.
      EVIDENCE: all three seams (serve, PlayPause handler, drain) proven COMPILED under
      `--target x86_64-unknown-linux-gnu`. 2 channel tests. Host, Windows, 8/8 mobile,
      azul-layout 7581, azul-dll 1973, autofix converged and `codegen all` re-ran for the new
      `AppConfig` field. ⚠ No Linux desktop here - compile-only, never registered on a real bus.

- [x] 9h-i-a-i DONE. NOT blocked on 11c after all, and the note said why without drawing the
      conclusion: "an app that IS a media player has the state and no way to publish it". The
      missing piece was an API for the app to PUSH, not a state machine for the engine to derive
      - and an engine-side machine could never have covered the real cases anyway, because an app
      playing through `rodio`, a system framework or the network knows what it is playing exactly
      when the toolkit cannot see it.
      `CallbackInfo::set_now_playing(NowPlayingInfo)` -> `CallbackChange::SetNowPlaying` ->
      `MediaSessionManager` -> drained at `process_window_events` depth 0 beside the haptics ->
      `publish_now_playing`. Same five-link shape as the haptic queue, and for the same reason:
      the sink is a D-Bus connection or an Objective-C singleton and neither belongs on the
      layout thread.
      TWO BACKENDS, because both platforms already had the object. Linux MPRIS now answers
      `PlaybackStatus`/`Metadata`/`Position` from what the app published and emits
      `PropertiesChanged`; macOS fills `MPNowPlayingInfoCenter`, which is not a bonus but a
      REQUIREMENT - macOS delivers media keys only to the app it considers "now playing", so
      publishing is what keeps the keys 9h-i registered for arriving.
      THE DIRTY FLAG IGNORES `position_ms`, AND THAT IS THE DESIGN. A player calls this every
      frame with an advancing position; announcing that would put 60 D-Bus broadcasts a second on
      the session bus, waking every listening process. It is also what the spec says: `Position`
      must never appear in `PropertiesChanged` because it advances continuously - clients
      extrapolate it and read the property when they need it. So the getters ANSWER from the
      stored value at any time and only real changes are ANNOUNCED. Comparing the whole struct
      would have been the obvious implementation and the wrong one.
      `mpris:trackid` is minted only when the TRACK changes, never on a pause: a desktop keys its
      progress bar and its "song changed" notification on that id, so a serial per publish would
      reset the bar and pop a notification every time the user hit pause.
      UNIT TRAPS, one per platform and they disagree: MPRIS wants MICROSECONDS as a signed
      integer (milliseconds there makes every track look 1000x short and the bar finish
      instantly), macOS wants SECONDS as a double. The struct stores milliseconds and each
      backend converts. `u64` ms and not `u32`, because `u32` MICROseconds overflows at 71
      minutes - an ordinary audiobook chapter.
      macOS also needs `MPNowPlayingInfoPropertyPlaybackRate`: Control Center advances the
      elapsed time by extrapolating from it, so without it the display freezes between publishes.
      The dictionary KEYS are exported `NSString * const` symbols whose string values are not
      documented, so with the framework dlopen'd they are looked up by `dlsym` rather than
      guessed - a hardcoded `"title"` would silently produce a dictionary the framework ignores.
      EVIDENCE: 7 core tests, with a NEGATIVE CONTROL - replacing the field-wise comparison with
      `self != other` makes two of them FAIL on an assert (not on a compile error) with
      "a position-only change announced itself at frame 1". All four Linux seams and both macOS
      seams proven COMPILED by deliberate type errors. `codegen all` + a real dll build put
      `AzCallbackInfo_setNowPlaying` and `AzNowPlayingInfo` in the C ABI; autofix converged at
      0 patches, `azul-doc check` PASSED. Host, 8/8 mobile, azul-core 2767, azul-layout 7604,
      azul-dll 1975. Also a FOURTH word-boundary guard on `DIFFICULT_TYPE_MODULES`: a `"Media"`
      prefix would have dragged the CSS `MediaType` into the audio module, so both entries are
      spelled in full and a test pins it.
      WARNING WHEN THE FLAG IS OFF: publishing without `expose_system_media_controls` logs once
      rather than silently doing nothing - silently doing nothing when the app asked for
      something visible is the exact failure mode this whole backlog is about.
      No Linux desktop and no Input-Monitoring-free way to observe Control Center here, so this
      is compile-only on both halves. Never seen a real media widget.
- [x] 9h-i-a-i-a DONE, with the event kind the note asked for. `EventType::MediaControl` (APPENDED)
      + `ApplicationEventFilter::MediaControl` + `EventData::MediaControl(MediaControlEventData { kind,
      position_us })`, planned and matched at the application level like `DeviceConnected`,
      emitted at the ROOT by `MediaSessionManager`'s new `EventProvider` impl - a seek is a
      window-level command like a media key, not a node's. The request itself is
      `MediaControlRequest { kind: Relative | Absolute | OpenUri, position_us, uri, track_id }`
      (repr(C), `OptionMediaControlRequest`), read with `CallbackInfo::get_media_control_request()`
      (the one being delivered, or the last delivered). It rides the media-KEY queue's twin
      (`media_keys::push_media_control` / `drain_media_controls`, NOT de-duplicated: two `Seek(+5s)`
      mean ten seconds), drained at the top of the pass into the manager and cleared with the
      other pending flags after dispatch. Providers: the dll slice AND the runner's.
      MPRIS: `Seek(x)` -> Relative, `SetPosition(o, x)` -> Absolute with the object path as
      `track_id` (so an app can drop a seek made against a track that ended - the spec's rule),
      `OpenUri(s)` -> OpenUri; `CanSeek` is TRUE now that they reach the app. THE OTHER
      DIRECTION: `Position` is kept out of `PropertiesChanged` by spec, so a jump the APP makes
      (its own progress bar) left every desktop widget extrapolating from the old position; the
      manager now notes a same-track jump of more than 2 s (`POSITION_JUMP_THRESHOLD_US` - far
      above a frame's advance, far below any human seek) and the drain announces it as the
      MPRIS `Seeked` signal once. SMTC and the Apple command centre read the position back from
      the published session and need nothing.
      EVIDENCE: 2 core tests (a queued seek is delivered once and stays readable as the last; a
      per-frame advance is not a seek, a 2 s+ jump is, announced once, backwards too).
      ✅ COMPILED AND RUN in the 2026-09-03 batch pass (evidence: the pass note on 9h-i-a-i-b): api.json gains `EventType::MediaControl`,
      `ApplicationEventFilter::MediaControl`, `MediaControlKind` / `MediaControlRequest` /
      `OptionMediaControlRequest` and `CallbackInfo.get_media_control_request` through `autofix` in
      that pass - the Rust compiles without them (appended variants keep the C layout), the C
      side just cannot name them yet.
- [x] 9h-i-a-i-a-i DONE on all three, each checked against its current documentation first
      (ruling 1). Windows: `SystemMediaTransportControls.PlaybackPositionChangeRequested`, whose
      `PlaybackPositionChangeRequestedEventArgs.RequestedPlaybackPosition` is a `TimeSpan` in
      100-ns ticks - the unit the timeline is already published in - so microseconds are
      ticks / 10; registering the handler is what makes the flyout's bar draggable. Apple:
      `changePlaybackPositionCommand` (macOS 10.12.2+ / iOS 8+), the one command whose event
      carries a value, `MPChangePlaybackPositionCommandEvent.positionTime` in SECONDS; its own
      block, because the key-pushing `register` closure cannot carry a position. Android:
      `MediaSession.Callback.onSeekTo(long pos)` in MILLISECONDS -> `nativeOnMediaSeek` (new JNI
      entry beside `nativeOnMediaButton`), plus `ACTION_SEEK_TO` in the published actions, which
      is the gate for the system UI offering a bar at all. All three land as
      `MediaControlKind::SeekAbsolute` on the seek queue, so the app sees one `MediaControl` event whatever
      the platform. ✅ COMPILED AND RUN in the 2026-09-03 batch pass (evidence: the pass note on 9h-i-a-i-b); the Java re-compile
      against android-34 is part of that pass.
- [x] 9h-i-a-i-b DONE. Both halves the note asked for existed once 9h-i-a-i-a landed, so this is
      the volume on top of that path. The app-level concept is `NowPlayingInfo::volume`
      (`OptionF32`: `0.0` silent .. `1.0` full, `None` = the app exposes no volume): azul plays no
      audio, so like the position it is the app's own value, published for the desktop to show
      and to ask about. MPRIS `Volume` now exists - it answers the published value or `1.0` when
      `None` (the spec makes the property mandatory, and a player that reports nothing is at
      full volume as far as the desktop can tell), a WRITE queues a request of kind `SetVolume`
      (negative clamped to silence) that arrives as the same event the seeks do, and a changed
      volume is an ANNOUNCED field (`PropertiesChanged`, unlike `Position`, because it moves when
      someone moves it and not continuously). THE RENAME: a volume is not a seek, so the inbound
      type 9h-i-a-i-a introduced under the name `MediaSeekRequest` is now `MediaControlRequest`
      with `MediaControlKind { SeekRelative, SeekAbsolute, OpenUri, SetVolume }`, the event is
      `EventType::MediaControl` / `ApplicationEventFilter::MediaControl` /
      `EventData::MediaControl(MediaControlEventData { kind, position_us, volume })`, and the
      callback getter is `get_media_control_request`. Done now, before any of these names reach
      `api.json` in the end pass, which is the last moment a rename is free. `NowPlayingInfo`
      lost `Eq` (an `f32` cannot have it) and gained a spelled-out `Default`. No other platform
      has a per-player volume to wire: Windows SMTC, `MPRemoteCommandCenter` and Android's
      `MediaSession` for local playback route volume to the system mixer and carry none, so
      the field is MPRIS-only by the platforms' own design, not by omission.
      COMPILED AND RUN in the batch pass of 2026-09-03: host check EXIT=0; core 2794, layout 7671 +
      999 (`--test all`), dll 2015 tests green; the e2e corpus (57 scenarios, incl. the new
      second-seat scenario) green; the 8-target gate green after one Android fix (the pan block
      read a vector the touch-state refresh had moved - caught by the gate, not by the host). api.json picked up the new names through autofix (EXIT=0, converged).
- [x] 9h-i-a-i-c DONE. Windows now has a media session: title, artist, album and ALBUM ART in the
      volume flyout and on the lock screen, plus a playback status and a timeline.
      `ISystemMediaTransportControlsInterop::GetForWindow(HWND)` rather than the UWP
      `GetForCurrentView`, which needs a `CoreWindow` a Win32 app does not have. The HWND is
      PASSED IN (`ensure_started` / `publish_now_playing` now take the window handle) rather than
      guessed with `GetForegroundWindow`, which would attach the session to whatever the user
      happened to be looking at when the app started. Every other platform ignores the argument.
      ⚠ THE DANGEROUS PART WAS THE KEYS, NOT THE PUBLISHING. Registering SMTC means one physical
      play press can arrive as `WM_APPCOMMAND`, as an SMTC `ButtonPressed`, or as both, and which
      of those happens is a platform detail that cannot be settled from here. GUESSING EITHER WAY
      IS UNSAFE: subscribe and assume `WM_APPCOMMAND` stops, and every press doubles (play, then
      immediately pause, landing back where it started); do not subscribe and assume it keeps
      arriving, and the keys go silent for any app that turns the flag on.
      So this subscribes AND leaves `WM_APPCOMMAND` alone, and the fix went into the CHANNEL:
      `push_media_key` drops a key already waiting in the current batch. One press produces one
      key however many transports saw it. NOT A TIMER and no constant - the queue drains once per
      pass, so "already pending" means "since the last frame"; a person cannot press play twice
      inside one frame, and a genuine double press lands in separate batches and survives. That
      also makes the old `MAX_PENDING` cap unreachable, so its test now asserts the stronger
      invariant it actually has: a sender repeating four keys forever leaves exactly four.
      A THIRD TIME UNIT, and the one most likely to pass review unnoticed because it looks like a
      duration rather than a count: WinRT `TimeSpan` counts 100-NANOSECOND ticks, where MPRIS
      wants microseconds and macOS wants seconds. Milliseconds into a `TimeSpan` shows a
      three-minute track as 18 microseconds with the scrubber pinned at zero.
      Two SMTC traps documented in place: `IsEnabled` must be set or nothing appears AND no
      button events arrive; and `DisplayUpdater::SetType` must come BEFORE the music properties,
      because setting it afterwards clears what was just written. `Update()` is what publishes -
      forgetting it is the classic "flyout still shows the previous track" bug.
      ALBUM ART WORKS HERE, unlike macOS: `RandomAccessStreamReference::CreateFromUri` takes the
      URI `artwork_url` already holds, where `MPMediaItemArtwork` wants decoded pixels
      (9h-i-a-i-e). A URI that does not parse is skipped rather than failing the whole update - a
      missing cover must not cost the title.
      EVIDENCE: 2 new tests with NEGATIVE CONTROLS on both - flattening the tick factor fails
      `winrt_ticks_are_hundred_nanoseconds_and_clamp`, and removing the dedup fails with "a
      sender repeating four keys forever must leave exactly four, got 64". All three SMTC seams
      proven COMPILED under `--target x86_64-pc-windows-gnu`. The media-key tests also needed
      SERIALISING - `PENDING` is a process-global and the harness runs them in parallel, so one
      test was draining another's keys. Host, 8/8 mobile, azul-core 2774, azul-layout 7612,
      azul-dll 1992, autofix 0 patches. ⚠ No Windows machine here - compile-only.
- [x] 9h-i-a-i-d DONE on both. Every shell now has a media session.
      iOS COST ALMOST NOTHING, and that is the finding: it is the SAME API as macOS - both
      classes, every selector used, the framework path, and `playbackState` (iOS 13 / macOS
      10.12). So `media_keys/macos.rs` became `apple.rs` and compiles for both, rather than a
      second copy that would drift. Checked against the real SDK headers, not assumed.
      ANDROID IS BOTH HALVES AT ONCE, unlike every other platform: on Linux and macOS the media
      KEYS and the media SESSION are separate objects that happen to share a flag, but a
      `MediaSession` receives the transport buttons through its callback AND carries the
      metadata, so one registration does both.
      THE TWO DIRECTIONS HAVE OPPOSITE DRIFT HAZARDS, which is what decided where each mapping
      lives. Buttons come back as ANDROID's own `KEYCODE_MEDIA_*` values, so both sides name the
      same platform constants and nothing can drift - but the table still moved into the shared
      `mod.rs`, because `android.rs` is cfg-gated to a target this machine never runs tests on
      and a mapping table is exactly what fails silently. The playback state goes OUT as
      `MediaPlaybackState`'s own discriminants, which IS an azul numbering crossing a boundary,
      so it gets the sensor-code treatment: a test pins all three.
      Java-side details that would each have silently produced nothing: a `MediaSession` with no
      CALLBACK swallows media keys rather than passing them on; a `PlaybackState` that advertises
      no ACTIONS draws a notification with no transport buttons however many callbacks exist; and
      the state's SPEED must be 1.0 while playing or the system freezes the progress bar between
      updates. A headset button arrives as a raw `KeyEvent` through `onMediaButtonEvent` rather
      than the typed callbacks, and only the DOWN is forwarded - the pair would report one press
      twice.
      Android is also the ONE platform whose time unit already matches: milliseconds both sides,
      where MPRIS wants microseconds, WinRT 100ns ticks and macOS seconds.
      ⚠ WHAT iOS DELIBERATELY DOES NOT DO: set or activate an `AVAudioSession`. The remote
      command centre only delivers to an app with an active playback session, so this is a real
      prerequisite - but the audio session is the APP's own policy (ducking, the silent switch,
      recording), and a toolkit silently activating `.playback` would interrupt whatever the user
      was listening to before the app played a note. Logged as 9h-i-a-i-d-i.
      EVIDENCE: 2 keycode tests that RUN ON THE HOST (they did not, until the table moved - the
      dll count was unchanged at 1992, which is how the gap showed) plus a NEGATIVE CONTROL on
      the state codes: renumbering `Stopped` fails with "left: 7, right: 0". Both Apple seams
      proven COMPILED on aarch64-apple-ios AND x86_64-apple-ios, both Android seams under
      `_internal_deps`, and the JAVA COMPILED against android-34 - all six helpers together,
      since `AzulActivity` now starts and stops the session. Host, 8/8 mobile, azul-core 2775,
      azul-layout 7612, azul-dll 1994, autofix 0 patches. ⚠ No device - compile-only.
- [x] 9h-i-a-i-d-i DONE per the USER RULING (2026-09-03): a RUNTIME call, not a flag.
      `CallbackInfo::set_system_audio_takeover(active)` -> `CallbackChange::SetSystemAudioTakeover`:
      iOS activates the shared `AVAudioSession` with `AVAudioSessionCategoryPlayback`
      (AVFAudio dlopen'd like MediaPlayer, constants read with the double dereference) and on
      release deactivates with `NotifyOthersOnDeactivation` so the apps it interrupted resume;
      Android requests audio focus (`AudioFocusRequest`, `AUDIOFOCUS_GAIN`, media attributes,
      delayed grants accepted) through `AzulMediaSession.requestAudioFocus(Activity)` /
      `abandonAudioFocus()`; desktop mixers share, so the call is a no-op that answers
      `Granted`. The answer and every later change arrive as `EventType::SystemAudioChange` /
      `ApplicationEventFilter::SystemAudioChange` with `SystemAudioChange { Granted,
      Interrupted, Ducked, Resumed, Ended, Lost }` (the union of iOS's interruption
      notification - began, ended with / without the should-resume hint - and Android's focus
      changes - GAIN after a loss = Resumed, LOSS = Lost, LOSS_TRANSIENT = Interrupted,
      LOSS_TRANSIENT_CAN_DUCK = Ducked), readable through `get_system_audio_change`, with
      `is_system_audio_active` for the claim (`Lost` clears it by itself). Plumbing mirrors the
      seeks: a thread-safe queue in `managers::media_keys`, drained at the top of the pass into
      `MediaSessionManager`, whose `EventProvider` impl emits both kinds. API shapes confirmed
      on the web before writing (Apple's interruption-handling guide and `SetActiveOptions`;
      Android's audio-focus guide and `AudioFocusRequest.Builder`). Core test pins delivery,
      readability and the `Lost` rule. ✅ COMPILED AND RUN in the second batch pass of 2026-09-03: host check EXIT=0; api.json
      converged (autofix EXIT=0, 0 patches; the four new CallbackInfo methods and six new types
      staged through `autofix add`); codegen green; core 2807, layout lib 7679, layout `--test
      all` 999, dll 2021 tests green; the e2e corpus (62 scenarios) green with the new
      spatial-navigation scenario RED under its negative control; the 8-target gate 8/8 after one
      iOS fix (objc2 `error: _` wants a typed NSError; explicit out-pointers now); the Android
      Java classes compile against android-34.
      api.json then gets `SystemAudioChange`, `OptionSystemAudioChange`, the three
      `CallbackInfo` methods and the filter variant through autofix.
- [ ] 9h-i-a-i-d-i-a macOS has `AVAudioSession` too since macOS 11, but a Mac's remote command
      centre works without an active session and a Mac does not interrupt other apps' audio the
      way a phone does, so the takeover is a no-op there by design. If a macOS app is found that
      needs `MPNowPlayingInfoCenter` gated on a session, add the same dlopen path under
      `target_os = "macos"`; nothing in hand shows one.
- [x] 9h-i-a-i-e DONE for LOCAL artwork on both Apple platforms; the remote half is
      9h-i-a-i-e-i and is genuinely a feature.
      The note treated "fetch a URL and decode an image" as one problem. It is two, and only one
      of them is hard: a cover on DISK needs no fetch, no cache and no failure path - the
      platform decodes it - and a local file is what a player that just opened a track actually
      has. Splitting them is the whole item.
      `initWithBoundsSize:requestHandler:` is the ONLY initialiser on macOS (`initWithImage:` is
      iPhone-only and deprecated), so there was no simpler route to weigh up. The handler ignores
      the requested size and returns the full image; the system scales.
      THE BLOCK OUTLIVES THE CALL, which is the memory-management question this raised:
      `MPMediaItemArtwork` COPIES the handler and calls it later, off this stack, so the image it
      returns must be retained. Retaining per publish would leak one image per track, so a
      single slot holds the current one and releases the previous - bounded at one.
      NSURL PARSES, not `strip_prefix("file://")`: a real cover path is percent-encoded, and a
      space is `%20`, so stripping the scheme by hand hands the decoder a filename that does not
      exist. A BARE PATH is not a URL at all, so `cover.png` falls back to the string itself -
      which is what an app that stored a filename rather than a URI has.
      THE LOCAL/REMOTE POLICY MOVED TO CORE (`artwork_is_remote`) rather than living inside the
      ObjC path: it is the one part that is pure, any future backend that has to LOAD rather than
      link an image asks the same question, and the Apple file is cfg-gated where it cannot be
      tested on a Linux host. A scheme that is not `file` is remote; NO SCHEME is a plain path
      and is local, and a Windows drive letter is a path rather than a one-character scheme.
      EVIDENCE: 1 core test over 12 URIs with a NEGATIVE CONTROL - collapsing the policy to
      "contains a colon" fails with "`file:///Users/me/cover.png` is loadable from disk and must
      not be skipped". Both artwork seams proven COMPILED on the macOS host AND
      aarch64-apple-ios. Host, both iOS targets, 8/8 mobile, azul-core media_session 10, autofix
      0 patches. ⚠ No device - compile-only, and no cover has actually been drawn.
- [x] 9h-i-a-i-e-i DONE, on the USER'S DIRECTION (2026-09-03: "we have a Rust URL parser
      already" and "we do have http fetching, so we just need a unified fetch api that also
      covers file://"). Both premises were right and both were things I had worked around
      instead of using: `azul_core::url::Url` wraps the real `url` crate, and
      `azul_layout::http` is a working client. My previous entry called this "a feature with a
      cache" because I had looked at neither.
      `azul_layout::fetch` is that unified API: one `route_of` that decides file / bare path /
      http(s) / unsupported, and one `fetch_uri` that returns bytes. It replaces the NSURL path
      dance AND the hand-rolled `artwork_is_remote` scheme scan in core, which is now deleted -
      one parser instead of three half-parsers.
      THE ROUTE IS SEPARATE FROM THE FETCH on purpose: a caller on the event loop needs to ask
      "would this block on a network?" WITHOUT performing it, which is exactly the question the
      artwork publish has.
      A WINDOWS DRIVE LETTER IS CHECKED BEFORE THE PARSER, not after: the URL spec says `C:` is
      a perfectly good one-character scheme, so `C:\cover.png` parses as a URL. The parser is
      not wrong, it is answering a different question.
      `file:` PATHS ARE PERCENT-DECODED. A real cover path has `%20` for a space, and the URL
      crate's `path()` returns it ENCODED - handing that to the filesystem asks for a file
      nobody has. A `%` that is not a valid escape is left alone, so a `100%_done` folder
      survives.
      REMOTE ARTWORK NEEDS NO RE-PUBLISH MACHINERY, which is the part that made this small: the
      fetch runs on a thread into a cache, that publish goes out without a cover, and the NEXT
      one picks it up - and a player publishes its position continuously, so "the next one" is a
      frame away. One fetch per URL, because a 60 Hz publisher would otherwise open sixty
      connections for one cover.
      `initWithData:` replaced `initWithContentsOfFile:`, which is what lets one code path serve
      a downloaded cover and a local file alike.
      EVIDENCE: 7 fetch tests with a NEGATIVE CONTROL - dropping the drive-letter guard and the
      percent-decode fails with "a space is `%20` in a real cover path, and the filesystem wants
      the space". Host, both iOS targets, 8/8 mobile, azul-layout fetch 7, azul-core
      media_session 9, autofix 0 patches. ⚠ No device - compile-only.
- [x] 9h-i-a-ii DONE, and it turned into the WINDOW-ACTIVATION path azul did not have at all -
      `WindowState::has_focus` and `request_user_attention` are both applied by nobody on any
      platform, so this is the first thing that can bring a window forward.
      The seam is the one the note predicted: a request parked in
      `managers::window_activation` and taken by the owning window's next pass, like the media
      keys. KEYED BY `registry_window_id`, NOT DRAINED WHOLESALE - an app can have several
      windows and MPRIS is per-PROCESS, so a plain drain would have whichever window reached the
      loop first answer a request meant for another. That is a coin flip, and the negative
      control pins it.
      EVERY PLATFORM DISAGREES ABOUT WHETHER AN APP MAY DO THIS, which is why it is four
      implementations rather than one call:
        - macOS needs BOTH `activateIgnoringOtherApps:` and `makeKeyAndOrderFront:` - activating
          brings the APPLICATION forward but leaves whichever window was key still key, and
          ordering a window front inside a background app only sorts it among that app's own.
        - Windows REFUSES unless the app already owns the foreground, and reports that by
          returning false rather than erroring. It also has to un-minimise first: a minimised
          window cannot be foreground, so `SetForegroundWindow` succeeds and leaves it in the
          taskbar, which looks exactly like the raise being ignored.
        - X11 sends `_NET_ACTIVE_WINDOW` to the ROOT window, not `XRaiseWindow` - raising
          restacks without focusing, so the window comes to the front and then ignores the
          keyboard. `data[0] = 2` is the honest source indication ("another application asked");
          claiming `1` would be a lie some WMs check. The mask must be
          SubstructureNotify|Redirect or the message is delivered nowhere.
        - WAYLAND REFUSES BY DESIGN and cannot be worked around: `xdg_activation_v1` needs a
          token minted from a recent INPUT SERIAL, and a request from another process has none.
          That is the protocol working, not a gap.
      A refusal is returned rather than swallowed, so the loop logs "declined by the platform"
      instead of claiming success. `CanRaise` is now TRUE - a desktop greys out its "open the
      player" affordance when it is false - and stays true on Wayland, because it advertises what
      the APP supports and the refusal is the compositor's answer.
      EVIDENCE: 4 channel tests with a NEGATIVE CONTROL - replacing the per-window take with a
      wholesale drain fails with "window 9 answered window 7's raise". All four seams proven
      COMPILED on their own targets: x11 under linux-gnu, windows under windows-gnu, macOS on the
      host, and the drain on all three. Host, 8/8 mobile, azul-layout window_activation 4.
      ⚠ No Linux desktop and no Windows machine here - compile-only.
- [x] 9f-i LINUX DONE (`/dev/hidraw*`); macOS is 9f-i-a and Windows 9f-i-b.
      The consumer side had been complete for a while - `HidManager`, `get_hid_devices()`,
      `get_hid_reports()`, and since 9g-ii-c the `HidDeviceVec`/`HidReportVec` types that let a
      binding actually read them. NOTHING produced, on any platform, so every one of those
      returned empty forever.
      THREE PIECES were needed, not one: there was also no CHANNEL. `HidManager` is pure data in
      azul-core with no cross-thread path into it, unlike sensors/geolocation which have one. So
      this adds `layout/src/managers/hid.rs` (mirroring `sensors.rs` verbatim), the Linux backend,
      and the drain in the capability pump.
      hidraw over libudev: it needs NO library - `open`/`read`/`ioctl` on a character device - so
      there is nothing to dlopen and nothing to link. libudev would only add hotplug.
      THE IOCTL NUMBERS ARE THE RISK. A wrong request does not fail loudly: `ioctl` returns EINVAL
      and the device silently reports vendor 0. So they are COMPUTED from the kernel's `_IOC`
      encoding with the struct sizes from `include/uapi/linux/hidraw.h` (fetched, not recalled),
      and a test pins three of them against the known-good literals.
      Reports are a QUEUE (each is a state change; dropping one loses a button press) and devices
      are a SNAPSHOT (only the newest matters). The queue is BOUNDED at 4096 and drops the OLDEST:
      a 1000 Hz gaming mouse with nothing draining would otherwise grow it for the life of the
      process, and dropping the newest instead would freeze the device's apparent state at the
      moment it overflowed. `take_hid_devices()` returning `None` means UNCHANGED, not empty -
      treating it as empty would clear the list on every pass that did not re-enumerate.
      NOT listener-gated, unlike sensors: `get_hid_devices()` is a poll-style accessor an app can
      call without ever subscribing to a `HidReport`, so gating enumeration on listeners would
      make it return empty forever.
      `vendor_id`/`product_id` are reinterpreted, not widened: the kernel types them SIGNED 16-bit
      while a USB id is unsigned, so a vendor above 0x7FFF arrives negative.
      PERMISSIONS are the real constraint and are handled as normal, not as failure: `/dev/hidraw*`
      is root-only by default and distributions ship udev rules only for devices they care about,
      so `EACCES` is the expected outcome for an arbitrary device and those are skipped silently.
      EVIDENCE: 4 channel tests (order, boundedness dropping the oldest, unchanged-vs-empty) and
      5 descriptor-parse tests (two-byte usage pages, truncated descriptors, long items, empty)
      - the parse is a real hazard because a malformed descriptor from an untrusted device must
      not overrun. All three seams proven COMPILED by deliberate type errors under
      `--target x86_64-unknown-linux-gnu`; `cargo check --tests` green for that target. Host, 8/8
      mobile, azul-layout 7579, azul-dll 1973. ⚠ No Linux machine here - compile-only.

- [x] 9f-i-a DONE — macOS `IOHIDManager`, dlopen'd like the ScreenCaptureKit and CoreGraphics-TCC
      paths beside it, so nothing is a link-time dependency and a missing symbol degrades rather
      than failing to launch.
      SIGNATURES CAME FROM THE REAL SDK HEADERS on this machine
      (`MacOSX15.2.sdk/.../IOKit.framework/Headers/hid/`), not from memory or from the docs page -
      which is what settled the callback shape: `IOHIDManagerRegisterInputReportCallback` takes NO
      buffer (unlike the per-device variant), and `IOHIDReportCallback` is
      `(context, result, sender, type, reportID, report, reportLength)`.
      INPUT MONITORING is the defining constraint and the reason this is not just "the Linux
      backend again": `IOHIDManagerOpen` returns `kIOReturnNotPermitted` unless the user granted
      it in System Settings, and that permission gates ALL HID access, not just keyboards, despite
      what its own description says. `IOHIDCheckAccess` (10.15+) is called BEFORE opening so the
      common denied case is a quiet log rather than an error path, and an EMPTY device list is
      published so `get_hid_devices()` answers definitively instead of looking like it never ran.
      Missing on pre-10.15 means granted, matching the screen-capture preflight beside it.
      `IOHIDRequestAccess` is deliberately NOT called: it raises a system privacy prompt, and a UI
      toolkit must not do that on its own initiative merely because an app linked it. See
      9f-i-a-i.
      NO `poll()` on this platform - reports arrive through the RUN LOOP, so the callback fires on
      the main thread and parks into the same channel the Linux sweep uses. The Linux backend
      sweeps file descriptors only because hidraw has no callback.
      Device identity is resolved ONCE into a `IOHIDDeviceRef -> HidDevice` map, because the
      callback would otherwise do a CF property round trip per report at up to 1000 Hz.
      EVIDENCE: this is the HOST platform, so `cargo check -p azul-dll` is a real build of the
      real path rather than a cross-compile - it compiled first try with zero warnings from the
      new module, and all three seams (device publish, callback registration, report push) were
      proven compiled by deliberate type errors. Linux still green, 8/8 mobile, azul-dll 1973,
      azul-layout 7579. ⚠ Not RUN: no Input Monitoring grant here and no way to assert on a real
      device, so this has never seen a report.

- [x] 9f-i-a-i DONE - YES. `Capability::InputMonitoring`, and the answer was not really a
      judgement call once the shape was written out: macOS gates raw HID with a TCC permission
      that has the same `Check`/`Request` pair as Camera and Microphone, so modelling it as
      anything else would have been the inventive choice.
      WHAT THIS ACTUALLY BUYS: `IOHIDRequestAccess` now has a caller. 9f-i-a deliberately never
      called it, because enumerating devices must not raise a privacy dialog as a side effect -
      so a denied machine reported an empty device list FOREVER with no way to ask. An explicit
      subscribe to the capability IS the app asking, exactly as it is for Camera. The HID backend
      still never prompts on its own.
      THE TRI-STATE IS THE POINT, not the boolean the HID backend already had.
      `kIOHIDAccessType` distinguishes Denied from Unknown, and an app may prompt for the second
      and must not for the first; collapsing both to "no" would make every machine that has never
      been asked look permanently refused. So `access_granted` was replaced by
      `input_monitoring_access` returning all three, and the old boolean is now derived from it.
      MACOS ONLY, and that is not an omission: Linux gates `/dev/hidraw*` with file permissions
      and udev rules that no runtime API can request, Windows RawInput needs no permission, and
      neither mobile platform exposes raw HID. Those backends answer `NotDetermined`, which is
      honest - there is nothing to ask.
      APPENDED, because the enum crosses the C API: inserting it anywhere else renumbers every
      variant after it and a binding built against the old header would silently request the
      wrong permission. A test pins that.
      TWO EXISTING GUARDS EARNED THEIR KEEP: `all_capabilities_are_distinct_and_totally_ordered`
      failed to compile until the variant was added to `ALL_CAPS`, and the Android backend's
      exhaustive match caught the missing arm on the 8-target gate rather than at runtime.
      EVIDENCE: the REAL macOS arm proven compiled by a deliberate type error (the cfg has a
      `libloading`-off stub, and a silent stub is exactly what this item was about), the
      discriminant test, `codegen all` + dll build putting `AzCapability_InputMonitoring` in the
      C ABI, autofix converged at 0 patches, `azul-doc check` PASSED, host and 8/8 mobile,
      azul-layout permission suite 58. ⚠ Not RUN: no way to grant or revoke Input Monitoring in
      a test here.
- [x] 9f-i-b DONE — Windows completes HID on all three desktop platforms.
      ⚠ THE ITEM'S PREMISE WAS WRONG ON THE HARD PART. It said the vid/pid "needs
      `HidD_GetAttributes` from **hid.dll**, a library this codebase does not load at all". It does
      not: `GetRawInputDeviceInfoW(RIDI_DEVICEINFO)` fills a `RID_DEVICE_INFO_HID` carrying vendor
      id, product id, usage page AND usage - every field `HidDevice` has except the name. Checked
      against the SDK docs before writing anything, so NO new library is loaded and the item was
      substantially smaller than logged.
      What the item got RIGHT is the variable-length report, and it is the real difference from
      the mouse arm: `RIM_TYPEHID` carries `dwSizeHid * dwCount` trailing bytes, so the buffer is
      sized from a zero-length `GetRawInputData` probe. A fixed-size read would truncate every
      report from any device with more than a few bytes of state. The payload is also `dwCount`
      reports back to back, not one - treating it as a single report would merge a coalesced batch
      into nonsense.
      REGISTRATION is a SECOND `RegisterRawInputDevices` call rather than a longer array: the
      array fails as a unit, so a driver rejecting one usage would otherwise cost the mouse
      registration too. Usages 4/5/8 on the Generic Desktop page (joystick, gamepad, multi-axis)
      are the top-level collections a non-mouse, non-keyboard HID reports under; there is no
      wildcard.
      NOT gated on the pointer lock, unlike the mouse arm beside it. That gate exists because raw
      MOUSE motion describes the user's movements across the whole desktop and is a privacy leak
      while unfocused; a joystick axis carries no desktop position, and gating it would make
      `get_hid_reports()` silent unless a game happened to hold a lock.
      Windows enumerates at WINDOW CREATION (it needs the loaded `User32Functions` table) and has
      no `poll()`: reports arrive through `WM_INPUT`, as macOS's arrive through the run loop. Only
      Linux sweeps, because hidraw has no callback.
      EVIDENCE: `RID_DEVICE_INFO` is 32 bytes and the OS VALIDATES `cbSize` - a wrong value fails
      every call with no diagnostic - so the layout is pinned by a compile-time assertion beside
      the existing mouse ones, and it passed. All three seams proven COMPILED by deliberate type
      errors under `--target x86_64-pc-windows-gnu`. All four desktop targets green, 8/8 mobile,
      azul-dll 1973, azul-layout 7579. ⚠ Compile-only - no Windows machine here.
- [x] 9g-i DONE for the drain + macOS + Android. The drain was the real defect: `CallbackInfo::
      play_haptic()` -> `CallbackChange::PlayHaptic` -> `HapticManager::play()` was a complete chain
      with NO drain on ANY platform, so every request ever made accumulated in a Vec nothing read.
      The public API did nothing, everywhere, and the queue grew for the window's lifetime.
      DRAIN: end of `PlatformWindow::process_window_events`, gated on `depth == 0`. All eight
      backends route through it, so there is ONE site instead of the ten `process_accessibility_
      actions` has. The depth gate is load-bearing: the queue's coalescing is adjacent-dedup, which
      only collapses a per-frame drag callback if the drain is LESS frequent than the callback.
      Draining inside the recursion would flush between two callbacks of the same pass and the
      device would buzz continuously - the exact failure the coalescing exists to prevent.
      VOCABULARY REDESIGNED first (user: "there are many different haptic patterns on Android -
      research and design a good api"). The old 4 (Tick/Click/Thud/Warning) named TEXTURES and were
      the intersection of the platforms, which is nearly empty - macOS has three patterns total.
      Now 19 semantic intents (Selection, Impact{Light,Medium,Heavy,Soft,Rigid}, Success/Warning/
      Error, KeyPress/KeyRelease/LongPress/ContextClick/TextHandleMove, GestureStart/GestureEnd,
      Rise/Fall/Spin) = the UNION of the platform vocabularies, because a caller wanting
      `TextHandleMove` on Android should not be denied it just because macOS renders it generically.
      Anything a platform lacks degrades along `HapticPattern::fallback()`, a chain that provably
      terminates at `Selection` (every device has it) - the degradation is a property of the
      PATTERN, so all six backends cannot each invent a different one.
      `HapticRequest` gained `intensity` (Android primitives, iOS `impactOccurred(intensity:)` and
      gamepad amplitude all take a scale) and `duration_ms` (continuous motors only).
      macOS: `NSHapticFeedbackManager.defaultPerformer`, raw `msg_send!` so it needs no new
      objc2-app-kit feature. 19 patterns collapse onto its 3 by weight. `intensity` is DROPPED, not
      emulated - the API has no strength axis and faking one with repeated taps reads as a stutter.
      Android: `View.performHapticFeedback` on the decor view, chosen over `Vibrator`/
      `VibrationEffect` because the latter needs the `VIBRATE` manifest permission (the APP's call,
      not a toolkit's) and bypasses the user's touch-feedback setting. Constants are read
      REFLECTIVELY by name: `HapticFeedbackConstants` gained values across API levels, and a
      hard-coded int fires a WRONG effect on an older device rather than none, because the
      framework accepts an unknown int and picks something. `NoSuchFieldError` is caught, cleared
      (a pending exception aborts the process at the next JNI boundary) and degraded via fallback.
      EVIDENCE: 9 new azul-core tests incl. `every_fallback_chain_terminates_at_selection` (a cycle
      there would be an infinite loop inside the event loop, on-device only); the Android path was
      proven COMPILED, not cfg'd out, by a deliberate type error at that line being reported under
      `--target aarch64-linux-android --features _internal_deps` (the 8-target gate does NOT enable
      `jni`, so the gate alone would have proved nothing). Host + 8/8 mobile + 2759/7561/1953/208.

- [x] 9g-i-a DONE. ⚠ THE BLOCKER I LOGGED WAS STALE: the note said this "requires
      `android.permission.VIBRATE` in the app's manifest, which is the APP's decision and not
      something a UI toolkit can declare on its behalf". Azul SHIPS a manifest
      (`scripts/android/AndroidManifest.xml`) and it has declared VIBRATE all along - line 26,
      alongside INTERNET and USE_BIOMETRIC. So an app built from azul's template was never
      blocked; only an app supplying its own manifest is, and that case degrades rather than
      fails (see below). Checking the file took one grep and the item had been sitting on a
      wrong premise.
      WHAT THE VIBRATOR PATH BUYS, and the reason it is NOT the default: `performHapticFeedback`
      needs no permission and HONOURS the user's touch-feedback setting, so it stays the path for
      ordinary taps - routing those through `Vibrator` would make an app ignore "turn off
      haptics". Only the two things it cannot express are routed here: a SCALED intensity
      (`addPrimitive(id, scale)` - there is no way to ask `performHapticFeedback` for a
      half-strength tap) and the CHIRP primitives (rise/fall/spin have no
      `HapticFeedbackConstants` at all, so before this they fell all the way down
      `HapticPattern::fallback` to a plain tick).
      So `HapticRequest::intensity` now means something on Android, where it previously did not.
      Primitives are read REFLECTIVELY BY NAME, the same discipline as the
      `HapticFeedbackConstants`: they arrived across API levels (SPIN and the chirps in 31), and
      a hard-coded id fires a WRONG effect on an older device because the framework accepts an
      unknown id and picks something. A `NoSuchFieldError` falls back to
      `performHapticFeedback` instead.
      The vibrator is fetched through `VibratorManager.getDefaultVibrator()` (API 31+) with the
      direct `getSystemService("vibrator")` as the fallback, and `hasVibrator()` gates the play.
      A MISSING PERMISSION arrives as a `SecurityException` from `vibrate`, which is CAUGHT AND
      CLEARED: a pending JNI exception aborts the process at the next boundary, so an app with
      its own manifest lacking VIBRATE would otherwise crash far from the cause instead of just
      not buzzing. It returns `false` and the `performHapticFeedback` path runs.
      EVIDENCE: both seams (routing decision, composition build) proven COMPILED under
      `--target aarch64-linux-android` with `_internal_deps` - the 8-target gate does not enable
      `jni`, so it alone would prove nothing. Host, 8/8 mobile, azul-dll 1973, 9 haptics tests,
      and `AzulSensors.java` still compiles against android-34. ⚠ No device - compile-only.
- [x] 9g-i-b DONE. My own note here was WRONG about the blocker: it said `prepare()` is needed
      "to avoid the actuator spin-up latency, which does not fit the current fire-and-forget
      drain". `prepare()` is a latency OPTIMISATION, not a precondition - `impactOccurred` works
      without it - so nothing was actually blocked. Re-checked instead of trusted.
      And `prepare()` is deliberately still NOT called: it helps only when called AHEAD of an
      anticipated event, while this drain runs at the moment the app has already asked to play.
      Calling it there would power the Taptic Engine on every request for no latency benefit.
      What DOES matter is that a `UIFeedbackGenerator` is a long-lived OBJECT, not a message:
      allocating one per tap is the documented way to get the worst latency, because the engine
      spins up per instance. Seven are cached - one per impact style plus notification and
      selection - so a repeated tap reuses a warm object. That is the real fix the note was
      groping at.
      iOS maps onto the vocabulary more directly than any other platform, so almost nothing folds:
      all five impact weights are 1:1, all three notification types are 1:1, and the six light
      discrete events share the selection generator. Only LongPress/ContextClick/Spin and
      Rise/Fall degrade, and to exactly what `HapticPattern::fallback` would have reached anyway.
      It is also the ONLY platform where `HapticRequest::intensity` can be honoured at all:
      `impactOccurredWithIntensity:` (iOS 13+) is probed with `respondsToSelector:` rather than a
      version check, the same idiom the pencil and gamepad-battery paths use.
      EVIDENCE: all three generator paths (impact, notification, selection) proven COMPILED by
      deliberate type errors under `--target aarch64-apple-ios`; no `static_mut_refs` warnings.
      Host, 8/8 mobile, 9 haptics tests green. Compile-only by user direction.
- [x] 9g-i-c DONE, and NARROWER than the note implied. It said this "reaches Surface Pen and
      some gamepads" - but the gamepad half is already 9g-i-d, which routes
      `HapticTarget::Gamepad` through gilrs, and gilrs owns that actuator on Windows too. A
      second path here would have rumbled TWICE for one request. Checked before writing rather
      than after.
      So what is genuinely new is `HapticTarget::Pen`, which NO backend had ever driven: macOS
      has no public API even for Apple Pencil Pro, no Android pen has an actuator, and Windows
      was the remaining platform where a pen can buzz. That variant existed in the type and was
      skipped everywhere.
      THE POINTER ID is the awkward part and shapes the design: `PenDevice::GetFromPointerId` is
      the only route to a pen's controller and needs an id that exists ONLY while the pen is
      tracked. So it is captured from the `WM_POINTER*` stream and the haptic addressed to
      whichever pen most recently reported; a request arriving after the pen left proximity finds
      a stale id and does nothing, which is honest - there is no pen to buzz.
      WAVEFORMS ARE LOOKED UP, NOT ASSUMED. A pen supports a SUBSET of
      `KnownSimpleHapticsControllerWaveforms` and publishes which through `SupportedFeedback`;
      sending an unsupported one fails at runtime. The wanted waveform is searched for and, if
      absent, the pen's FIRST supported one is used - a buzz of the wrong texture beats silence,
      matching how `HapticPattern::fallback` degrades everywhere else.
      Intensity goes through `SendHapticFeedbackWithIntensity` only where
      `IsIntensitySupported()` says so: the plain call is not equivalent to intensity 1.0 on
      hardware that lacks the axis.
      Needed two new `windows` crate features (`Devices_Haptics`, `Devices_Input`) and no new
      library.
      EVIDENCE: all three seams (pointer capture, dispatch, send) proven COMPILED under
      `--target x86_64-pc-windows-gnu`. All four desktop targets, 8/8 mobile, azul-dll 1973, 9
      haptics tests. ⚠ Compile-only - no Windows machine and no Surface Pen here.
- [x] 9g-i-d DONE for the gilrs backend (Windows + Linux + macOS at once); mobile is 9g-i-d-a.
      `HapticTarget::Gamepad` was skipped by EVERY backend - macOS, Android and iOS each return
      early on anything but `System`, correctly, because a phone body is not a controller's motors.
      Nothing anywhere handled the gamepad case, so `play_haptic` to a pad did nothing.
      Handled in the SHARED `play_haptic_native` rather than per-platform, because the actuator
      belongs to the CONTROLLER, not the machine: gilrs owns it identically on all three desktop
      platforms, so a per-platform copy would be the same code three times.
      ⚠ THE STOP PATH IS THE WHOLE ITEM, and the naive version is wrong. A gilrs `Effect` is a
      HANDLE, and letting it drop sends `HandleDropped`, which - read in `gilrs/src/ff/server.rs`
      - only calls `effects.remove(id)`. It does NOT stop the motor. So relying on Drop leaves a
      controller buzzing with nothing left to stop it, which is exactly what the item warned
      about. Every teardown here calls `stop()` EXPLICITLY before releasing the handle, and
      `stop_all_rumble()` runs at termination - before `std::process::exit(0)`, which runs no
      destructors that could have caught it.
      A new effect STOPS the previous one on that pad first: two overlapping effects sum in the
      driver, so a repeated tap would climb to full amplitude and stay there.
      STRONG vs WEAK is which MOTOR, not how hard: a controller has a low-frequency motor that
      thuds and a high-frequency one that buzzes, and the pattern's weight picks between them.
      Driving both at once is a muddier sensation, not a louder one.
      `is_ff_supported()` is checked before building: a pad with no actuator errors, and on some
      backends that error is only visible as a failed play.
      `duration_ms == 0` means "natural", which for a motor is not zero - 150ms is gilrs's own
      example figure and about the shortest pulse an ERM motor can spin up and down within.
      EVIDENCE: all three seams (dispatch, build, stop) proven COMPILED by deliberate type errors.
      The call site is cfg-gated off mobile, where `gamepad::desktop` does not exist - caught by
      the 8-target gate rather than by inspection. All four desktop targets green, 8/8 mobile,
      azul-dll 1973, 9 haptics tests, autofix converged. ⚠ No controller here - compile-only.

- [x] 9g-i-d-a ANDROID DONE; iOS is 9g-i-d-a-i and is a genuinely different architecture.
      `HapticTarget::Gamepad` was DROPPED ON THE FLOOR on Android: `play_haptic` returned early
      for anything that was not `System`, so an app calling `play_haptic(pattern, Gamepad(id))`
      on a phone with a controller attached got silence, with the API reporting success.
      The pad's motor is reached through `InputDevice.getVibratorManager()` (API 31) or
      `getVibrator()` below that - the SAME `Vibrator` interface as the phone's own, a different
      device through one API - which is why this lives beside `play_via_vibrator` rather than in
      the gamepad backend. `pad` is the Android input device id, which is exactly what
      `gamepad::android` already publishes as the `GamepadId`, so there is no mapping table and a
      stale id from an unplugged pad simply yields a null device.
      NOT THE COMPOSITION API the phone path uses: those primitives are TAPS. A rumble is a
      sustained buzz, which is `createOneShot(ms, amplitude)` - and that is API 26, so unlike the
      primitives it needs no version fallback.
      `hasVibrator()` is the real test and is load-bearing: most controllers have no motor and
      `getVibrator()` still returns a NON-NULL stub for them, so without it every rumble would
      look like it worked.
      THREE RULES MOVED TO CORE so both backends answer them identically and so they can be
      TESTED - the Android code is cfg-gated to a target this machine never runs tests on:
      `rumble_duration_ms` (0 means "natural", which for a MOTOR is not zero - a zero-length
      rumble is not something a person can feel; 150ms), `wants_strong_motor` (the pattern picks
      the low- or high-frequency motor; driving both is muddier, not louder), and `amplitude_u8`.
      That last one has a real trap: ANDROID REJECTS AMPLITUDE 0 as invalid rather than treating
      it as silence, so it floors at 1 and the backend returns early on a zero intensity instead.
      EVIDENCE: 3 core tests with a NEGATIVE CONTROL - removing the amplitude floor and the
      duration default fails with "intensity 0 produced amplitude 0, which Android rejects".
      Both new seams proven COMPILED under `--target aarch64-linux-android` with `_internal_deps`.
      VIBRATE is already in the manifest (line 26). Host, 8/8 mobile, azul-core 2773, azul-dll
      1992, autofix 0 patches. ⚠ No controller here - compile-only.
- [x] 9g-i-d-a-i DONE under ruling 1 ("implement blindly, everything, but research the web
      again"). Researched against Apple's current documentation (the JSON behind the developer
      pages, since the HTML is script-rendered): `GCController.haptics` is
      `@property (readonly, nullable) GCDeviceHaptics *` (iOS 14+), `- (CHHapticEngine *)
      createEngineWithLocality:(GCHapticsLocality)` where `GCHapticsLocality` is `typedef
      NSString *` with `Default / All / Handles / LeftHandle / RightHandle / Triggers /
      LeftTrigger / RightTrigger` constants, `supportedLocalities` an `NSSet`; CoreHaptics (iOS
      13+): `CHHapticEventParameter initWithParameterID:value:` (float),
      `CHHapticEvent initWithEventType:parameters:relativeTime:duration:` (NSTimeInterval),
      `CHHapticPattern initWithEvents:parameters:error:`, `CHHapticEngine startAndReturnError:` /
      `createPlayerWithPattern:error:` (`id<CHHapticPatternPlayer>`), the player's
      `startAtTime:error:` / `stopAtTime:error:` (BOOL), the `CHHapticEventTypeHapticContinuous`
      and `CHHapticEventParameterIDHapticIntensity` / `...Sharpness` string constants, and
      `CHHapticTimeImmediate` as a `#define` (0.0, spelled out because a macro has no symbol).
      SHAPE: `gamepad::apple::rumble(pad, intensity, duration_ms, strong)` mirrors the gilrs
      entry point. The PLAN is pure and lives in `gamepad/mod.rs` so it is tested on the host:
      strong = the LEFT grip at sharpness 0 (the large low-frequency motor of a
      DualShock/DualSense - the woofer in Apple's own left/right discussion), weak = the RIGHT grip
      at sharpness 1, intensity capped, zero/negative/NaN = do not play (a zero-intensity
      continuous event would still occupy the actuator) but DO stop what was playing - the
      documented early-stop on every backend. The iOS half: CoreHaptics and GameController are
      DLOPEN'D, their `NSString * const` constants read by `dlsym` (values undocumented, like the
      MediaPlayer keys); one engine per (pad, locality) is created, `autoShutdownEnabled`,
      started ONCE and cached (a start per request is the stutter); a locality the pad does not
      list in `supportedLocalities` falls back to `Default`, which the framework guarantees; the
      previous player on an engine is STOPPED and released before the next starts (two patterns
      sum in the driver - the gilrs rule); `stop_all_rumble` now covers iOS at termination. The
      pad id is the address-derived id `poll` reports, looked up on `GCController.controllers`.
      ⚠ ALSO FIXED, found by writing the same helper: `media_keys/apple.rs::info_key` read
      MediaPlayer's `NSString * const` keys with `Symbol<*mut AnyObject>` and ONE dereference,
      which is the ADDRESS OF THE VARIABLE (what `dlsym` returns) handed to the framework as if
      it were the string - libloading's own example is `**awesome_variable`, two. Every
      now-playing dictionary key on macOS was a pointer into MediaPlayer's data segment posing as
      an object; unseen because 9h-i-b was compile-only too. Both helpers now dereference twice.
      EVIDENCE: 3 host tests on the plan; NEGATIVE CONTROL: swapping the grips fails the first.
      The iOS path is proven COMPILED on the three iOS targets (8/8 mobile) and nothing more -
      no controller here. Host check green.
- [ ] 9g-i-d-a-i-a The one assumption a device settles in a minute: that a DualShock/DualSense
      reports the strong motor as `GCHapticsLocalityLeftHandle` and the weak as `RightHandle`
      through the Game Controller framework (Sony's own layout, and the woofer/tweeter order in
      Apple's discussion). If a real pad has them the other way round, swap the two arms of
      `rumble_plan` - the mapping is in exactly one place for that reason. Also unverified:
      whether `startAndReturnError:` once per engine survives the app backgrounding with
      `autoShutdownEnabled` (the documented design), or needs a restart on failure.
      USER RULING 2026-09-04 (the hardware / platform group): "just implement blindly and we cross-compile
      at the end. Real verification will come with time."
- [x] 9g-ii-a DONE, per the ruling ("make new structs if needed"). Both accessors returned
      non-empty tuples, which have no C representation, so neither could be exposed and no
      binding could read an IME caret or a raw motion delta.
      `get_raw_mouse_motion` needed NO new struct: `RawMotionEventData { dx, dy, device_id }`
      already existed in core as the payload of `EventType::RawMotionMotion`, with the exact
      field set the accessor was flattening into a tuple. It only lacked `#[repr(C)]` - which it
      needed anyway, being handed across the boundary - plus an `impl_option!`. Checking for an
      existing type before writing one is what kept this from adding a duplicate.
      `get_composition_cursor` DID need one: `CompositionCursor { begin, end }`. `SelectionRange`
      was the obvious candidate to reuse and is wrong for it - two `TextCursor`s, positions in
      the DOCUMENT, where a preedit is uncommitted text with no document position at all until
      it commits.
- [x] 9g-ii-b DONE, and it needed no new type either: `OptionNodeHierarchyItemId` ALREADY EXISTS
      (`core/src/styled_dom.rs`), and `NodeHierarchyItemId` is precisely what `NodeId`'s own docs
      call "the FFI wrapper type". (`OptionNodeId` does not exist - the earlier grep hit was
      `OptionNodeIdNodeMap`, an unrelated node-graph type.)
      So `find_scroll_target` returns the FFI id, and the two INTERNAL callers - the auto-scroll
      timer wrapper and the drag-autoscroll site in the dll - convert back to `NodeId`, which is
      what they index with. The public API is FFI-correct and the engine keeps the ergonomic type.
      EVIDENCE for both: `codegen all` + dll build put `AzCallbackInfo_getCompositionCursor`,
      `_getRawMouseMotion` and `_findScrollTarget` in the C ABI, returning
      `AzOptionCompositionCursor` / `AzOptionRawMotionEventData` / `AzOptionNodeHierarchyItemId`.
      autofix converged at 0 patches / 0 errors. azul-core 2760, azul-layout 7575, azul-dll 1973,
      host, 8/8 mobile.

- [x] 9g-ii-d DONE, and like 9g-ii-a/b it needed NO new type - checking first is what keeps this
      from growing duplicates. `tilt` becomes `PenTilt`, which already existed and which
      `PenState::tilt` right beside it ALREADY USED: the tuple here was the odd one out, and the
      conversion that happened one level down (`PenTilt { x_tilt: tilt.0, .. }`) just moved up.
      `touch_radius` becomes `LogicalSize` - a width/height pair in logical pixels IS a logical
      size. NOT `TouchPoint`, which models the same physical contact but as a major/minor ellipse
      plus an id, a position and a force this sample already carries separately.
      `#[repr(C)]` added; the sample is deliberately NOT `Copy` (its `Instant` owns a boxed clock
      reading), so the accessor returns an OWNED `OptionInputSample` rather than the borrow it
      used to - a reference into engine state has no C representation and no lifetime to protect
      it, the same call `get_hid_reports` makes.
      ⚠ THE REAL FIND IS A HOLE IN THE CHECKER, and it nearly shipped. api.json recorded the
      timestamp's type as `CoreInstant` - this module's `use azul_core::task::Instant as
      CoreInstant` alias - and EVERY CHECK STAYED GREEN, because lint 1 compares api.json against
      the SOURCE and an alias matches itself perfectly. The codegen was set to emit a field of
      type `AzCoreInstant`, which is declared nowhere; it would have surfaced as a C compile
      error in whichever binding a user built first, with nothing pointing back at the alias.
      So the field is now spelled `azul_core::task::Instant` and there is a NEW LINT: every type
      api.json references must be a type api.json defines. It found two more things immediately -
      `c_int` was used by three `glGet*Location` returns while missing from `PRIMITIVE_TYPES`,
      and the collector was counting the `self` receiver KIND (`ref`/`refmut`/`value`) as a type.
      Both fixed.
      Module: `InputSample` auto-landed in `dom`, which is actively misleading for a pointer
      sample; moved to `callbacks` with the `PenState`/`PenTilt` it now carries, via a
      `DIFFICULT_TYPE_MODULES` entry and a test that also pins `OptionInputSample` staying in
      `option`.
      EVIDENCE: NEGATIVE CONTROL - putting `CoreInstant` back in api.json makes the new lint fail
      with "`CoreInstant` (first seen at InputSample.timestamp)". `codegen all` + a real dll build
      put `AzCallbackInfo_getLastInputSample` in the C ABI returning `AzOptionInputSample`, with
      `AzInstant`/`AzPenTilt`/`AzLogicalSize` fields and ZERO occurrences of `AzCoreInstant`.
      autofix converged at 0 patches, `azul-doc check` PASSED with the new lint green. Host, 8/8
      mobile, azul-layout 7604, azul-doc 211.
- [x] 9g-ii-e DONE, by exposing the ANSWER instead of the STRUCTURE. The note is right that
      `FullHitTest` cannot cross the C ABI - a `BTreeMap` of `DomId` to a `HitTest` that is four
      more maps would need five map types nothing else uses - and wrong that this makes the
      information unreachable. No app wants that shape; it wants "what is under the cursor". So
      `hovered_node_ids()` and `topmost_node()` answer that, and
      `CallbackInfo::get_hovered_nodes` / `get_hovered_nodes_frames_ago` expose it. Same trade
      `get_hid_reports` makes by returning an owned vec rather than a borrowed slice.
      ORDERED BY `hit_depth`, NOT BY NODE ID, and that is a real difference rather than a
      preference: two overlapping absolutely-positioned nodes can have any id relationship, and
      the one on top is the one the backend reported at the lower depth. Both producers fill it
      (WebRender from its own hit test, the CPU tester from its front-to-back walk).
      Only the REGULAR hits: scroll frames, scrollbar parts and cursor areas are hit-tested
      separately, and a scrollbar is not the content beneath it.
      ⚠ I ALMOST ADDED A DUPLICATE. `get_topmost_hovered_node` was written and then WITHDRAWN
      (source and api.json) on finding `get_deepest_hovered_node` already exposed - a second
      "which node" accessor with subtly different semantics is worse than one. Checking for an
      existing function before adding one is the same discipline that kept 9g-ii-a and 9g-ii-b
      from adding duplicate types; this time it caught me one step late.
      EVIDENCE: 4 tests with a NEGATIVE CONTROL - ordering by node id instead of depth reverses
      the answer. `codegen all` + a dll build put `AzCallbackInfo_getHoveredNodes` and
      `AzDomNodeIdVec` in the C ABI; autofix converged at 0 patches, `azul-doc check` PASSED.
      Host, 8/8 mobile, azul-core 2779.
- [x] 9g-ii-e-i DONE - and the confirmation it asked for came back NEGATIVE, which is why it
      was worth asking. Both producers number hits front to back WITHIN a DOM, and they DISAGREE
      ACROSS DOMs: WebRender walks ONE scene for every DOM in reverse paint order
      (`webrender/core/src/hit_test.rs`: `scene.items.iter().rev()`), so a `VirtualView` page
      composited over its host gets the lower depth; the CPU tester walked `node_rects` - a
      `BTreeMap<DomId, _>` - in ASCENDING id order, so the HOST's nodes came first and the page
      on top of them got the higher numbers. `FullHitTest::topmost_node` (the global minimum,
      "already written and tested") was therefore right on the GPU path and wrong on the CPU
      path exactly where a page sat over its host - which is every headless E2E run and every
      CPU-rendered window. Not device-level after all: both producers are code, and the order
      falls out of one line each.
      FIXED IN THE PRODUCER, not papered over in the consumer: the CPU tester now visits DOMs
      highest-id first (a child DOM always has a higher id than its host and is composited on
      top - the engine's stated model, the same assumption `deepest_node_across_doms` makes),
      so its numbering matches WebRender's globally and `topmost_node` is right on both.
      `get_deepest_hovered_node` delegates to `HoverManager::current_hover_node_full` - the
      front-most regular hit of the front-most DOM, which is the node the pointer events are
      targeted at - so the auto-scroll anchor and the drop target are one node. The largest-id
      proxy is gone.
      EVIDENCE: a CPU test with a host node under a VirtualView page (page first in the hit list,
      depth 0 for the page and 1 for the host, `topmost_node` names the page, outside the page
      only the host) - NEGATIVE CONTROL: restoring ascending DOM order fails it while the
      single-DOM `hit_test_returns_topmost_first` still passes; a `CallbackInfo` test where node 5
      sits behind node 1 and a page dom sits over both (picks the page; single-DOM picks node 1,
      not 5; nothing hovered is `None`). The four integration suites that consume CPU hit order
      (`click_into_a_virtual_view_page`, `virtualview_hit_matches_render`, `hover_manager`,
      `drag_selection_scroll`: 15 tests) still pass. Host, 8/8 mobile.
- [x] 9g-ii-e-ii DONE - and the scan existed TWICE, byte-for-byte in shape: the e2e runner's
      and the dll's `clicked_focusable_node`. Both walked EVERY hit DOM (ascending id) and let
      the LAST focusable win (the `break` left only the ancestor walk, not the DOM loop). That
      is not "host first" as logged but "any DOM's focusable, host included": a click on
      UNFOCUSABLE page content - which is a BLUR in every browser, the click target's own chain
      having nothing to focus - focused the host node the page was covering, because the host's
      chain was walked too. One rule now, `HoverManager`-side `focusable_under_pointer`: the
      nearest focusable ancestor of the FRONT-MOST hit (`deepest_node_across_doms`), walking
      that node's OWN DOM only. It takes `is_focusable` / `parent` as closures so the rule is
      tested on the host without a laid-out document and both callers pass the same two lookups
      over `layout_results`.
      EVIDENCE: 3 host tests (unfocusable page content over a focusable host = `None`, the
      defect; a page with a focusable root beats the host's; the walk starts at the front-most
      hit, not the largest id). NEGATIVE CONTROL: the old any-DOM-last-wins loop fails the first.
      The five click-to-focus integration suites (`textinput_first_draw_and_focus`,
      `caret_reveal_and_session_identity`, `text_edit_seam_regressions`,
      `click_into_a_virtual_view_page`, `focus_ring_tween`: 44 tests) still pass. Host, 8/8 mobile.
- [x] 9g-ii-f DONE per the USER RULING (2026-09-04, "We can always transform that to a wrapped
      RefAny + C callback with downcasting"). `MarginBoxContent::Custom` is no longer a boxed Rust
      closure: it is `Custom { callback: MarginBoxCallback, data: RefAny }` - the ordinary azul
      callback shape (`extern "C" fn(&mut RefAny, PageInfo) -> AzString` behind `impl_callback!`,
      so the foreign-callable `ctx` slot the bindings use is there too), `PageInfo` is `#[repr(C)]`,
      the generator gets a refcount-bumped handle to the app's data like every other callback, and
      `MarginBoxContent::custom(cb, data)` builds one. The two hook tests now downcast an
      `Arc<AtomicUsize>` out of the `RefAny` from an `extern "C" fn` - the shape a C or Python
      caller would use. WHY the gap existed: the pagination family was written Rust-first and the
      closure was the one member no binding could ever express; with it gone nothing in
      `query_pagination -> FakePageConfig -> PageSequence -> MarginBoxContent` is C-incompatible any
      more. NOT YET IN api.json: `query_pagination` itself is not exposed (nothing references the
      family from an exposed type), so autofix at the eighth batch's end pass will not pull it in
      by itself - exposing the family is a separate, now UNBLOCKED item (9g-ii-f-i). ✅ COMPILED in the eighth batch pass of 2026-09-04: host check EXIT=0; autofix converged at 0 patches / 0 FFI errors after two source fixes (`natural_scroll` moved beside `log_level` so the alignment checker sees no padding; the runner and the seat-focus arm gained the missing match arms); `codegen all` EXIT=0; azul-core 2809, azul-layout lib 7683 / `--test all` 1003, e2e corpus 62 scenarios, azul-dll 2040 (+5), all 0 failed; 8/8 gate targets green including windows-gnu, which compiles the hid.dll and registry code.
- [x] 9g-ii-f-i DONE. `CallbackInfo::query_pagination` is in api.json with the whole family
      behind it - 35 types, all through `autofix add` + the drift loop (0 patches, 0 FFI errors),
      filed under `pdf` beside `Pdf` (the print path these types were written for) via the
      override table, plus `vec` / `option`. The conversion the note asked for, done at the
      SOURCE: `FakePageConfig` (`OptionString` texts, `OptionPageSequence`), `HeaderFooterConfig`,
      `PageSetup`, `PageSequence` (the `BTreeMap<usize, PageSetup>` became `PageSetupOverrideVec`
      with `set_override` / `override_for`), `MarginBoxContent` (`AzString` texts,
      `MarginBoxContentVec`, and `Custom(MarginBoxCustom)` - the FFI rule is ONE field per
      variant), `PaginationInfo` (`PageBreakPositionVec`), `PageBreakPosition` (`OptionNodeId`,
      new in core), `BreakKind::Avoided(f32)` and `PageCounterFormatted(CounterFormat)` as tuple
      variants (codegen builds data variants positionally), `BreakPolicy` / `CounterFormat` /
      `PageInfo` `#[repr(C)]`, every struct ordered widest-first for the alignment checker, and
      the duplicate `PageMargins` unified onto `azul_core::paged::PageMargins`. WHY the gap
      existed: the family was Rust-first (a boxed closure, a `BTreeMap`, `String`s) - 9g-ii-f
      removed the closure, this removed the rest. Evidence: host check EXIT=0; `codegen all`
      EXIT=0; azul-core 2809, azul-layout lib 7683 / `--test all` 1003 (the pagination and DOM
      break tests among them), e2e 62 scenarios, azul-dll 2071 (+31, the generated vec / option
      tests), 0 failed; 8/8 gate targets green.


### Follow-ups opened by 9e

- [x] 9e-i DONE for `modifiers` (every backend) and for `locks`/`is_repeat` where the OS
      answers unambiguously. `current_physical_key` is 9e-ii and untouched.
      `modifiers` is a pure function of `pressed_virtual_keycodes` — and `determine_all_events`
      ALREADY derived it correctly into a local for other events' `EventData`, then compared the
      never-written STRUCT FIELD for the `ModifiersChanged` diff. So the producer was right all
      along and the diff compared two identical defaults forever. The derived accessors
      (`shift_down()` and friends) were also right, which is why shortcuts worked and the gap
      stayed invisible.
      FIX: `KeyboardState::derived_modifiers()` + `sync_modifiers()`, called at every one of the
      9 sites that move the pressed set (shared win32 applier, wayland, x11 ×3, windows ×2,
      android, macOS). TRAP: the coverage grep for those sites used `pressed_virtual_keycodes = `
      WITH A TRAILING SPACE and so missed macOS entirely, whose assignment wraps to the next
      line — found only by reading the macOS applier for another reason.
      `locks` is NOT derivable (a lock is a toggle no key event describes) and is now read from
      the OS: macOS `NSEventModifierFlags::CapsLock`; X11 `LockMask` + `Mod2`; Windows
      `GetKeyState` low bit for `VK_CAPITAL`/`VK_NUMLOCK`/`VK_SCROLL`.
      `is_repeat` was ALREADY computed on macOS (`isARepeat`) and Windows (`lParam` bit 30) to
      fix the state diff, and thrown away instead of stored; both now store it and clear it on
      key-up.
      EVIDENCE: `modifiers_changed_fires_when_the_held_modifier_set_moves` (press → 1 event,
      hold → 0, release → 1) and `an_unsynced_modifier_set_emits_nothing`, which pins the exact
      shape of the bug. 8-target gate green; azul-core 2736 green; azul-layout unchanged at the
      20 pre-existing scroll_timer/gamepad failures.
- [x] 9e-i-a DONE. `locks` is now fed on every backend that has them.
      The blocker was exactly as recorded: `mods_locked` is a mask of KEYMAP-SPECIFIC modifier
      INDICES, so its bit positions mean nothing without a name lookup, and no lookup symbol was
      bound. Added `xkb_state_mod_name_is_active` to the Xkb table — which Wayland SHARES with
      X11 (`x11/dlopen.rs`), so one symbol served it — and queried it in
      `keyboard_modifiers_handler` right after `xkb_state_update_mask`, so it answers for THIS
      event.
      Bound as OPTIONAL: an xkbcommon too old to export it leaves locks unreported rather than
      failing to load the library and taking the whole Wayland backend with it.
      xkb's own modifier names are used rather than invented ones — caps lock is "Lock", num
      lock is "Mod2" — with `XKB_STATE_MODS_LOCKED` as the component. Scroll lock is left alone
      for the same reason the X11 path leaves it: xkb has no conventional name for it, and
      guessing at a Mod3/Mod5 bit is right on one keymap and wrong on the next.
      Host check, Linux-target check and 8-target gate green. NOT runtime-verified — needs a
      real compositor with caps lock pressed.
- [x] 9e-i-b DONE on all three, and it turned out X11 and Wayland share ONE rule.
      X11: `XkbSetDetectableAutoRepeat` is ALREADY enabled on the connection (x11/mod.rs, gated
      on `owns_display`), which the item did not know. With it on, a held key delivers repeated
      KeyPress with NO intervening KeyRelease — so "a press for a keycode already recorded in
      `pressed_key_vks`" IS the repeat test. If a server does not support it, repeats arrive as
      Release+Press pairs and the key is absent at the Press, so this under-reports rather than
      claiming a repeat that did not happen.
      Wayland: the same rule, for a different reason. Compositors do not repeat keys; this
      backend already synthesises them client-side off a timerfd and replays the held key through
      `handle_key(keycode, 1)`. The key was never released, so it is still in `pressed_key_vks`.
      Android: `KeyEvent::repeat_count()` exists in android-activity 0.6.1 (the item assumed it
      needed JNI plumbing, as 9e-ii-a assumed of the scan code) — nonzero means repeat.
      THE REAL GAP was further downstream and affected ALL FIVE backends: `KeyboardEventData.repeat`
      was HARDCODED `false` in `determine_all_events`. macOS and Win32 had been filling
      `KeyboardState.is_repeat` since 9e-i, and the payload an app actually reads still said
      `false`. Now carried through on KeyDown; KeyUp stays hardcoded `false` deliberately, so a
      release cannot report a repeat even if a backend left the flag set.
      EVIDENCE: `a_keydown_carries_the_platforms_repeat_verdict` (both directions) — verified it
      bites by restoring the hardcoded `false` — and `a_keyup_is_never_a_repeat`, which sets the
      flag on purpose. Host, Linux-target and Android-target checks green; 8-target gate green;
      azul-layout 7555.
- [x] 9e-ii DONE. `core/src/physical_key.rs` holds the three tables every backend needs and
      `current_physical_key` is now filled on macOS, Windows, X11 and Wayland.
      THREE conventions cover every desktop backend, not four: `wl_keyboard.key` IS an evdev
      code, X11 keycodes are `evdev + 8` BY PROTOCOL (so `from_x11_keycode` is the same table
      minus the offset and needs no keymap lookup), Windows is PS/2 set 1, macOS is Carbon.
      The Windows `E0` bit (`lParam` bit 24) is NOT optional detail: without it Enter and
      NumpadEnter, ControlLeft and ControlRight, and the entire arrow cluster versus the numpad
      are the SAME scancode — `the_windows_extended_bit_separates_the_duplicated_scancodes`
      pins all 12 such pairs.
      The macOS table was cross-checked entry by entry against `macos_keycode_to_virtual_key`,
      the table this codebase already trusts for the LOGICAL key; the two agree on every code
      both name, which is what makes the additions (0x0A ISO_Section, F13-F20, the JIS keys)
      trustworthy.
      FIXED ALONG THE WAY: macOS `update_keyboard_state` returned early when the LOGICAL table
      had no entry for a key, so media / OEM / non-US keys updated nothing at all. The physical
      position, the locks and the modifier set are true regardless of whether we can name the
      key's layout meaning — the early return is now an `if let` around only the pressed-
      virtual-key bookkeeping that actually needs the `vk`. This is the same reasoning the
      Windows arm already documents for its scancode write.
      EVIDENCE: 5 tests, including `the_same_position_gets_the_same_name_on_every_platform`
      which asserts 21 positions agree across all four entry points — a table typo otherwise
      shows up only as a wrong binding on one OS. Host check green, 8-target gate green,
      azul-core 2741 green (2736 + 5), azul-layout unchanged at its 20 pre-existing failures.
      NOT runtime-verified on the macOS NSEvent path: it needs real OS key events and the
      headless backend does not go through it. Compile- and logic-verified only.
- [x] 9e-ii-a DONE, and the item's premise was WRONG in a useful way. It assumed the scan code
      needed new JNI plumbing (`KeyEvent.getScanCode()` / `AKeyEvent_getScanCode`). It does not:
      `android_activity::KeyEvent::scan_code()` already exists in the 0.6.1 crate this repo
      depends on (native_activity/input.rs:529). The backend simply threw the value away — it
      collected `(action, keycode)` pairs and never read the third field the event carries.
      So `key_updates` now carries the scan code and `current_physical_key` is filled from
      `PhysicalKey::from_evdev`, with no new plumbing at all. Android input sits on evdev, so the
      scan code IS an evdev code and the table added in 9e-ii applies unchanged.
      SOFT KEYBOARD: an IME event has no physical key behind it and reports scan code 0. evdev 0
      is `KEY_RESERVED`, so the existing table already answers `Unidentified` for exactly that
      case — asserted by `an_unnamed_code_is_unidentified`, which pins `from_evdev(0)`.
      ALSO FIXED, same datum: `pressed_scancodes` was never populated on Android either, for the
      same reason the physical key was not. It is filled here rather than left as a second gap
      behind the one being closed (skipping scan code 0, which names no physical key).
      Deliberately NOT derived from the Android keyCode, which is the LOGICAL key: mapping that
      to a position would be wrong on every non-US layout, which is the exact failure
      `PhysicalKey` exists to prevent.
      Host check green, 8-target gate green (both Android targets). NOT runtime-verified on a
      device — the emulator has no hardware keyboard attached in this setup, and a soft-keyboard
      event is precisely the case that reports no scan code.

### Follow-ups opened by 9d

- [x] 9d-i DONE for Win32 — the raw-input path did not exist AT ALL there (no `WM_INPUT` arm, no
      `RegisterRawInputDevices`, none of the structs). With this and 9d-ii, Windows now has the
      same lock + relative-motion pair X11 already had.
      WHY IT WAS NEEDED: `WM_MOUSEMOVE` reports an ABSOLUTE client position, so the moment a
      pointer lock confines the cursor it stops changing and the deltas vanish exactly when a
      first-person camera needs them. Raw input is the only source of unbounded relative motion.
      DECISION on `RIDEV_INPUTSINK`, which the item flagged: NOT used. It delivers raw motion
      while another application holds the foreground, which is precisely the privacy leak the X11
      producer documents. Pointer lock implies focus, so foreground-only costs nothing.
      TRAPS handled: `GetRawInputData` returns a BYTE COUNT and signals failure with `u32::MAX`,
      not a BOOL — treating nonzero as success accepts the error code as a length. Some devices
      (RDP, KVMs, some tablets) set `MOUSE_MOVE_ABSOLUTE`, whose `lLastX/Y` are POSITIONS not
      deltas; accumulating those sends the camera to a screen corner on the first event, so they
      are dropped. `WM_INPUT` is passed to `DefWindowProc` so the system frees the raw-input
      buffer.
      EVIDENCE: the four `#[repr(C)]` layouts are pinned by `const _: () = assert!(...)` on size
      AND alignment, checked at COMPILE time by the `x86_64-pc-windows-gnu` target — a wrong
      offset here would silently read a different field, and a wrong `RAWINPUTHEADER` size makes
      every `GetRawInputData` call fail at runtime with no diagnostic. Verified the guard bites by
      widening `ulRawButtons` to u64 and confirming the build fails on
      `size_of::<RAWMOUSE>() == 24`. Host check, Windows target check and the 8-target gate all
      green. NOT runtime-verified — needs a real Windows box with a mouse.
- [ ] 9d-i-a WEB raw motion only (`movementX`/`movementY` + `requestPointerLock`). The Wayland
      half of this item is DONE — see 9d-ii-a, they landed together. Web remains because it is a
      different shell entirely (the wasm one), not a protocol binding.
- [x] 9d-ii DONE for X11, Windows and macOS. `CallbackInfo::set_pointer_lock(bool)` exists
      (api.json via `autofix add` + `apply`, bindings regenerated) and three backends take a real
      grab. 9d is now usable end-to-end on X11, where the raw-motion producer already existed.
      CORRECTION to this item's premise: `is_cursor_locked` COULD already be set, through
      `modify_window_state` (that arm copies `mouse_state` wholesale). It was useless because
      nothing ACTED on it — the flag flipped, `XI_RawMotion` started being delivered, and the
      cursor still wandered out of the window, which is the opposite of a pointer lock. The gap
      was the platform grab, not the flag.
      KEY DESIGN POINT: `handle_set_pointer_lock` RETURNS whether the lock is actually held, and
      the caller stores THAT rather than what was asked for. A grab is genuinely refusable —
      `XGrabPointer` fails when another client holds the pointer (a menu, a drag, a screen
      locker) or the window is not viewable — and since `RawMouseMotion` is gated on the flag, a
      false positive would deliver relative motion while the cursor still roams. The default
      trait impl returns `false`, so a backend that has not implemented the grab reports honestly
      instead of silently claiming success.
      Per platform: X11 `XGrabPointer` with `confine_to` = our own window (the confinement IS the
      lock) and `owner_events = True` so ordinary events keep routing. Win32 `ClipCursor` over the
      client rect mapped to SCREEN coordinates via `ClientToScreen` (GetClientRect answers in
      client space, origin always 0,0 — clipping to it raw would confine to the top-left of the
      display), plus `ShowCursor`, which is a COUNTER not a flag, so the toggle runs only on a
      real transition or a double-lock hides the cursor process-wide forever. macOS
      `CGAssociateMouseAndMouseCursorPosition(false)` + `CGDisplayHideCursor` — that is stronger
      than the other two: it disconnects the mouse from the cursor entirely, so the cursor
      freezes while deltas keep arriving.
      The headless backend applies the same gate, so a scenario that forgets to lock sees nothing
      exactly as the app would.
      VERIFIED: host check, `x86_64-pc-windows-gnu` check, 8-target gate, generated bindings
      carry the `bool`, and `set_pointer_lock` queues the right change in both directions. NOT
      runtime-verified — a real grab needs a real display server, and none of the three can be
      exercised headlessly.
- [x] 9d-ii-a + 9d-i-a (Wayland) DONE together, as the notes said they had to be. Both
      protocols are bound from scratch: `zwp_relative_pointer_manager_v1`,
      `zwp_relative_pointer_v1`, `zwp_pointer_constraints_v1`, `zwp_locked_pointer_v1` — types,
      interface descriptors, request wrappers, listeners and registry binds. 9d is now complete
      on every desktop backend.
      WHY THEY ARE A PAIR, concretely: constraints stop the cursor moving, relative-pointer
      supplies the deltas that replace it. Constraints alone freeze the cursor and report
      nothing; relative-pointer alone still stops at the screen edge. A compositor can advertise
      one and not the other, so `set_pointer_lock` requires BOTH globals and refuses otherwise.
      THREE THINGS WAYLAND DOES DIFFERENTLY from X11/Win32, each of which would be a bug if
      copied across:
      1. There is NO ungrab request. A constraint lives exactly as long as its object, so
         DESTROYING the object is the release.
      2. The compositor can end the lock on its own (focus loss, session switch) and is the only
         party that knows. The `unlocked` event therefore writes `is_cursor_locked = false`
         directly — the flag follows the compositor, not the app's last request, or
         `RawMouseMotion` would look armed while no deltas arrive. That also makes 9d-ii-b
         (release-on-focus-loss) already correct here for free.
      3. The relative-motion event carries BOTH an accelerated and an unaccelerated delta. The
         UNACCELERATED pair is used: pointer acceleration makes the same physical movement travel
         further when done quickly, which is right for hitting a button and wrong for aiming a
         camera — and the accelerated pair is what `wl_pointer.motion` already reflects.
      No `is_cursor_locked` gate on the motion handler, unlike X11 and Win32: the relative-pointer
      object only EXISTS while the lock is held, so the compositor has already made the guarantee
      those two have to assert for themselves.
      EVIDENCE: `const _: () = assert!(..)` pins each listener struct's arity against the
      `event_count` its descriptor hard-codes (1 for relative-pointer, 2 for locked-pointer) —
      `wl_proxy_add_listener` dispatches BY EVENT INDEX, so a descriptor claiming one event more
      than the struct has would call whatever follows it in memory, which is the exact hazard the
      existing gesture bindings warn about in a comment. `LIFETIME_PERSISTENT` is used so
      alt-tabbing away and back does not silently drop the lock. Host check, Linux target check
      and the 8-target gate green; suites unchanged. NOT runtime-verified — needs a real
      compositor.
- [x] 9d-ii-b DONE for Win32, X11 AND macOS (the item said X11/Win32; macOS needed it too, and
      needed it most). Wayland needs no caller — the compositor ends the lock and says so through
      `zwp_locked_pointer_v1.unlocked`, which already writes the flag.
      Every backend needs this for a DIFFERENT reason, which is why it routes through one shared
      `release_pointer_lock_on_focus_loss` rather than each arm clearing the flag:
      * Win32 drops the `ClipCursor` clip ITSELF on deactivation, so the flag is already a lie —
        and `ShowCursor` is a COUNTER, so clearing the flag without running the release would
        strand the cursor hidden for the whole PROCESS with no matching show left.
      * macOS keeps `CGAssociateMouseAndMouseCursorPosition(false)` in force across focus
        changes, leaving the user with a FROZEN, INVISIBLE cursor inside whatever application
        took focus. That is the one failure here that cannot be recovered without killing the app.
      * X11 holds the grab until explicitly released — it survives focus changes by design — so
        an unfocused window keeps the pointer confined with no way out.
      THE TRAP, and the reason the X11 arm is safe: X sends a `NotifyGrab` FocusOut as a SIDE
      EFFECT of any grab activating, including our own `XGrabPointer`. Releasing on that would
      make the lock instantly undo itself. The X11 FocusOut arm was already guarded by
      `is_grab_focus_change` for the menu case, so the release sits inside the branch that has
      already excluded grab-induced changes.
      The helper deliberately does NOT re-acquire on focus return: whether a lock silently comes
      back is a product decision (a game wants it, a drawing app does not) and guessing it would
      re-grab the pointer behind the user's back. That half stays open as 9d-ii-c.
      Host, Linux-target and Windows-target checks green; 8-target gate green; azul-dll 1935 with
      only its 8 pre-existing headless failures.
- [x] 9d-ii-c DONE per the USER RULING (2026-09-03): the lock is never re-taken silently; the
      app is TOLD and re-requests. `EventType::PointerLockChange` /
      `WindowEventFilter::PointerLockChange` fire from the state diff whenever
      `mouse_state.is_cursor_locked` changes, in BOTH directions like the browser's
      `pointerlockchange` (the flag is the payload); so a lock the platform ends - every
      grab-based backend drops it on focus loss and already wrote the flag false, a Wayland
      compositor through `unlocked` - reaches the app without any backend emitting anything by
      hand. The divergence is settled the other way round from Wayland: `handle_keyboard_leave`
      now destroys the constraint like the three grabs, because a `LIFETIME_PERSISTENT` lock
      re-activating by itself on focus return is exactly the behaviour ruled out. A game gets
      the lock back by calling `set_pointer_lock(true)` from `WindowFocusReceived`; a drawing
      app is never re-captured. The e2e runner already honours the flag, so the diff covers
      headless scenarios too; layout test `pointer_lock_transitions_emit_a_change_each_way...`
      pins taken / lost / steady. ✅ COMPILED AND RUN in the second batch pass of 2026-09-03: host check EXIT=0; api.json
      converged (autofix EXIT=0, 0 patches; the four new CallbackInfo methods and six new types
      staged through `autofix add`); codegen green; core 2807, layout lib 7679, layout `--test
      all` 999, dll 2021 tests green; the e2e corpus (62 scenarios) green with the new
      spatial-navigation scenario RED under its negative control; the 8-target gate 8/8 after one
      iOS fix (objc2 `error: _` wants a typed NSError; explicit out-pointers now); the Android
      Java classes compile against android-34.

### Follow-ups opened by 9c

- [x] 9c-i ANDROID DONE (Wear crown). Win32 is 9c-i-c; the Apple Digital Crown is watchOS-only
      and azul has no watchOS target, so it is not a backend that can be written here at all.
      A crown turned and nothing in the engine heard it, even though `DialState`'s own docs name
      `SOURCE_ROTARY_ENCODER` as one of the four platforms that converged on the dial primitive.
      THE TRAP: a crown arrives as a `MotionEvent`, but it is NOT a pointer - it reports on
      `AXIS_SCROLL` with no coordinates at all. Letting it fall through the existing motion path
      would have fabricated a pointer at (0, 0), i.e. a phantom touch in the top-left corner on
      every turn. So the rotary source is detected and returned FIRST, before any of the
      touch/mouse handling.
      Samples are SUMMED before applying: several arrive per frame on a fast spin and
      `update_dial_state` arms one `DialRotate` per call, so applying each would fire a burst of
      events for one gesture.
      `delta_rad` stays 0.0 and `detent_count` carries the value, which is the honest split rather
      than a shortcut: `AXIS_SCROLL` on a rotary encoder is a scroll-like magnitude that Android's
      docs never relate to a physical rotation, so there is no radians conversion to make without
      inventing a constant. It mirrors the Wayland producer exactly, which fills `delta_rad` and
      leaves `detent_count` at 0.0 - each backend fills the axis its platform actually measures,
      and `update_dial_state` already arms on EITHER being non-zero, so the event fires.
      EVIDENCE: both the collector and the drain proven COMPILED by deliberate type errors under
      `--target aarch64-linux-android` with `_internal_deps` (the 8-target gate does not enable
      `jni`, so it alone would prove nothing). Host, 8/8 mobile, 45 dial tests still green.

- [x] 9c-i-a DONE - YES, AND MY OWN NOTE WAS WRONG. It said "a radians-per-detent constant that
      Android does not document". Android documents exactly that, just not where a rotary-input
      guide would send you: `InputDevice.MotionRange.getResolution()` is "the number of units per
      millimeter, or per RADIAN for rotational axes", and `AXIS_SCROLL` on a
      `SOURCE_ROTARY_ENCODER` is a rotational axis. The rotary guide only ever mentions
      `getScaledScrollFactor` - units to PIXELS, for scrolling - which is why the angular one was
      missed.
      THE CONSTANT COMES FROM THE DEVICE, which is the part that makes it honest on hardware
      nobody here owns: watch crowns differ by detent count and gearing, so no single constant
      could be right for more than one of them, and the note was right to refuse to invent one.
      Asking the device is a third option neither of us had considered.
      THE SOURCE-QUALIFIED `getMotionRange(axis, source)` OVERLOAD, not the one-argument form: a
      device can report the same axis on several sources, and the unqualified call answers for
      whichever the platform picks - which on a watch that also has a touchscreen is not the
      crown.
      UNKNOWN STAYS UNKNOWN. `getResolution` returns 0 when the driver reported none, and many
      do; dividing gives an INFINITY that would travel into an app's rotation maths. `0.0` still
      means "this platform did not measure that" - it is now the fallback rather than the rule.
      Cached per device id: the resolution is a property of the hardware, and a JNI round trip
      per sample at crown-spin rates would cost more than the value.
      The conversion lives in `managers::gesture` beside `DialState`, not in the Android shell,
      so its zero and NaN cases are TESTED - the shell file is cfg-gated to a target this machine
      never runs tests on.
      EVIDENCE: 3 tests with a NEGATIVE CONTROL - removing the guards fails with "resolution 0
      must answer 0.0 ... got inf". Both new seams proven COMPILED under aarch64-linux-android
      with `_internal_deps`. Host, 8/8 mobile, azul-layout 7634. ⚠ No watch - compile-only.
- [x] 9c-i-b CLOSED as NOT DELIVERABLE, and the note named the wrong keycode. It said a crown
      click arrives as `KEYCODE_STEM_PRIMARY`. Google's own physical-buttons guide documents
      `KEYCODE_STEM_1/2/3` as the buttons an app receives, and describes the PRIMARY stem as the
      power button, which every Wear device has and which is NOT assigned to app actions - the
      system owns it. So the crown's click is not something an app can be told about.
      `KEYCODE_STEM_1/2/3` ARE deliverable, but they are separate multifunction buttons rather
      than the dial's press. Feeding one into `DialState::pressed` would report a side shortcut
      button as a crown click, which is precisely the "looks done while doing nothing" shape -
      worse than the honest `false`.
      `DialState::pressed` is not an unemitted field: Wayland's pad dial and the Surface Dial
      both fill it. Only Android cannot, and now says so with a citation rather than a TODO.
      A future item could map `KEYCODE_STEM_1/2/3` to new `VirtualKeyCode` variants - they have
      NO producer at all today - but that is a keyboard feature, not this one.
- [x] 9c-i-c DONE — Surface Dial, completing the dial on every platform that has one.
      It is the most CAPABLE dial backend, not just another one. `RotationDeltaInDegrees` is a
      real angular delta, so this is the one place `DialState::delta_rad` is honest - the Wayland
      pad dial reports rotation but no position, and the Android Wear crown reports detents with
      no angle at all (9c-i-a records that there is no honest conversion there). It is also the
      ONLY backend that can ever fill `contact_position`: a Dial placed on a Surface Studio's
      display reports a screen contact point, which is what lets an app draw a radial menu around
      the physical object.
      `CreateForWindow` IS THE WHOLE TRICK. `RadialController::CreateForCurrentView()` is the UWP
      entry point and is useless to a Win32 app - there is no CoreWindow - so the desktop route
      is the `IRadialControllerInterop` activation factory, obtained by casting the factory for
      `Windows.UI.Input.RadialController`. That is why this needs `Win32_UI_Input_Radial` on top
      of `UI_Input`, and why the item had been sitting behind "needs WinRT interop".
      `detent_count` stays 0.0 even though the Dial HAS physical detents: the API reports a
      continuous angle and never says one was crossed, so a count would be invented. The exact
      mirror of Android, which reports detents and no angle - each backend fills the axis its
      platform actually measures, which is now the third time that rule has decided a field.
      The controller is HELD for the window's lifetime rather than made per event: dropping it
      unregisters the app from the Dial's menu.
      EVIDENCE: all three seams (construction, rotation handler, click handler) proven COMPILED
      under `--target x86_64-pc-windows-gnu`. All four desktop targets, 8/8 mobile, 45 dial
      tests, azul-dll 1973. ⚠ Compile-only - no Windows machine and no Surface Dial here.
- [x] 9c-ii DONE — all four layers plus the registration the layer model does not mention.
      `DialState` has been readable through `CallbackInfo::get_dial_state()` since the type
      landed, with no `EventType` and no filter behind it, so a dial could only be POLLED from
      an unrelated callback that happened to run.
      Added, each appended at the END for ABI: `EventType::DialRotate`/`DialClick`,
      `HoverEventFilter::DialRotate`/`DialClick`, `WindowEventFilter::DialRotate`/`DialClick`,
      their matcher arms, their `ALL_HOVER`/`ALL_WINDOW` entries (planning is DERIVED from those
      lists), the `event_type_to_filters_legacy_hint` rows, and the two exhaustive
      filter-mapping tables the compiler pointed at (`to_focus_event_filter` answers `None` — a
      dial is delivered by POSITION or to the window, never by focus).
      A FIFTH thing the four-layer model does not name: the PROVIDER has to be in the dll's
      provider slice. `GestureAndDragManager` was not, and the comment on that slice says
      exactly why that matters — "without being in this slice the provider is simply never
      polled and the whole chain stays invisible". It now implements `EventProvider` and is
      registered, with its pending flags drained beside the others.
      WINDOW-SCOPED, deliberately: only a Surface Dial placed on a Surface Studio's display
      reports a contact point. Every other dial — tablet pad, Wear crown, Digital Crown — is
      used off-screen, so no node is under it and the window is the only honest target; the
      Hover filters still receive it by propagation from the root.
      THE CLICK IS AN EDGE. A dial reports `pressed` as a LEVEL, so arming on the level would
      emit `DialClick` once per frame for as long as it is held — the same trap the gamepad
      press edges avoid. Rotation is armed per update rather than diffed: two identical deltas
      in a row are two turns, not one.
      api.json: both filter enums synced through `autofix` (the `TabletPadState` modify patch it
      also proposed was skipped — that is the known `add_custom_impls` drift), bindings
      regenerated.
      EVIDENCE: `the_dial_filters_are_reachable_from_planning` pins the round trip for all four
      filters — verified it bites by deleting the two `ALL_HOVER` entries ("DialRotate must plan
      DialRotate, got [Window(DialRotate)]") — and `a_held_dial_click_does_not_re_fire` pins the
      edge across held, released and re-pressed. azul-core 2750, azul-layout 7556, azul-dll
      1944, host check and 8-target gate green.

### Follow-ups opened by 9b

- [x] 9b-i DONE for macOS and Win32; X11 is 9b-i-a. G2's "tell a touchpad from a mouse" now
      works on three backends instead of one.
      macOS: `hasPreciseScrollingDeltas` on the scroll event, which is the exact analogue of
      Wayland's `axis_source` — the only place AppKit says whether a finger or a ratcheting wheel
      produced the scroll, since the motion events are otherwise identical. A Magic Mouse reports
      precise deltas and is therefore classified `Touchpad`: the question the field answers is
      "can this device scroll continuously and gesture", not "is it shaped like a mouse".
      Win32: there is no per-event device field on the classic mouse messages at all. Instead,
      when a message was SYNTHESIZED from a pen or a finger, the injector stamps
      `GetMessageExtraInfo` with `MI_WP_SIGNATURE` in the upper bits and a per-contact id in the
      low byte. That gives Pen / Touchscreen / Mouse from one call.
      THE TRAP, and why the logic does not live in the window proc: the low byte is a CONTACT ID,
      so an unmasked equality test against the signature matches only contact 0 — the first
      finger would classify as touch and every finger after it as a mouse. The classifier is
      therefore a pure function in `shell2/common/event.rs`
      (`win32_pointer_source_from_extra_info`), which is compiled on every host and so can be
      tested without Windows. Three tests sweep all 128 contact ids for both the touch and pen
      cases and pin that an unsigned message is a real mouse; verified they bite by replacing the
      masked compare with `==` and watching contact 0x00 fail.
      Host, Windows-target and 8-target gate green. The macOS half is NOT runtime-verified —
      it needs a real trackpad scroll on a real window, which the headless backend does not go
      through.
- [x] 9b-i-a DONE. `pointer_source` is now fed on all FOUR desktop backends.
      Everything needed was already present and unused: `XIQueryDevice`/`XIFreeDeviceInfo` are in
      the dlopen table, `XIDeviceInfo` is declared, `XI_HierarchyChanged` is already selected on
      `XIAllDevices` and handled, and every `XIDeviceEvent` (motion, button, touch) carries
      `sourceid`. So this was a lookup plus a cache, not new plumbing.
      X11 has NO device-type field — `XIDeviceInfo.use_` only says master/slave and
      pointer/keyboard — so the kind lives in the NAME, which the libinput/evdev drivers spell
      consistently: "SynPS/2 Synaptics TouchPad", "TPPS/2 IBM TrackPoint", "Wacom Intuos Pen
      stylus" / "... Pen eraser".
      THE CACHE IS THE POINT, and why this was split out: `XIQueryDevice` is a synchronous ROUND
      TRIP to the X server and `sourceid` arrives on every motion event, so an uncached lookup
      would put a server round trip in the middle of the pointer path. Cached per device id and
      invalidated WHOLESALE on `XI_HierarchyChanged` — an id is reused after a hotplug, so a
      stale entry would describe a different device.
      TWO DELIBERATE CHOICES: an unrecognised name answers `Unknown`, NOT `Mouse` — defaulting to
      Mouse makes "is this a touchpad?" confidently wrong on untested hardware, and `Unknown` is
      a value the enum carries for exactly this. And `stylus`/`eraser` are matched rather than a
      bare "pen", because "pen" is a substring of ordinary words; the Wacom naming always carries
      one of the two suffixes, so nothing is lost.
      EVIDENCE: the heuristic is a pure function in `shell2/common/event.rs` (beside the Win32
      one, for the same reason — it compiles on every host, so it is testable without Linux). 4
      tests: the real driver names above, case-insensitivity, `Unknown`-not-`Mouse` for
      unrecognised names, and that "OpenMoko"/"Pentax" do not become styluses. Host check green,
      Linux target check green, 8-target gate green. NOT runtime-verified — needs a real X
      server with a real touchpad.
- [x] 9b-ii DONE (user re-scoped 2026-09-03: "we need to refactor the mouse state"). The state
      now fans out per pointer SEAT, with the existing global as the primary seat's entry.
      ⚠ THE KEY IS A SEAT, NOT A DEVICE - the item as logged said `BTreeMap<DeviceId, ...>`, and
      that would have been wiring it wrong. A seat is an independent CURSOR; a device is the
      hardware that drove one. On Windows, macOS, Android and iOS the OS merges every mouse into
      ONE cursor: a click from the second mouse after a move from the first lands where the
      first left the cursor, because that IS where the cursor is. Keyed by device, that click
      would have been dispatched at a stale position in a phantom second entry nobody could
      see. Only X11 (MPX master pointers) and Wayland (one `wl_seat` per user) can present a
      second cursor, and only there does a second entry ever exist. `pointer_device_id` stays
      the physical device WITHIN a seat.
      MODEL: `PointerSeat { seat_id, state: MouseState }` + `PointerSeatVec` in core;
      `FullWindowState.pointer_seats` holds the NON-primary seats only, sorted - the primary is
      `mouse_state` itself and is deliberately not duplicated (two copies of one cursor are a
      desync waiting to happen), so every existing reader of "the mouse" keeps meaning what it
      meant. `pointer_seat(id)` / `pointer_seat_mut(id)` (creates on first touch) /
      `remove_pointer_seat` / `pointer_seats_with_primary` fold seat 0 in.
      EVENTS: the button / click / context-menu / move derivation is ONE shared
      `pointer_seat_events` for the primary and every other seat (dedup before features), run
      per seat against that seat's own previous entry and targeted through its own hover
      history (`InputPointId::Seat(id)`; `for_seat` folds 0 into `Mouse`). A new seat diffs
      against a default state so its first press is a MouseDown; a vanished seat diffs the other
      way and releases what it held. Press targets are keyed `(seat, button)`, so a second
      cursor's release cannot complete the first's click. `MouseEventData.seat_id` APPENDED
      (Rust-only, the struct is not in api.json).
      ⚠ BOTH HALVES OF 9b-i WERE DEAD: `MouseState.pointer_device_id` had NO writer on any
      platform and `MouseEventData.device_id` was never set on a real event (`..Default`), so
      the fields appended to answer "which mouse" answered 0 everywhere. Now stamped from the
      seat's state on every derived event, and written by: X11 (`sourceid`, the slave), Windows
      (`WM_INPUT` `hDevice`, read for its header whether or not the pointer is locked - the
      privacy gate is about MOTION and a device handle carries none; raw is queued ahead of the
      legacy message it becomes, so `WM_MOUSEMOVE` stamps the mouse that produced it), Android
      (`MotionEvent.getDeviceId()`). macOS/iOS/Wayland expose no physical mouse identity, so 0
      stays honest there.
      SECOND CURSOR PRODUCER: X11 MPX. Events were already selected on `XIAllMasterDevices`, so
      a second master's presses had always arrived - and were applied to the one global,
      teleporting cursor A's buttons to cursor B's position. `deviceid` == 2 (the virtual core
      pointer, reserved by XI2) is the primary; any other master is a seat, applied to its own
      state, hit-tested into its own hover history, and run through the ordinary diff pass.
      `get_pointer_seat_state(seat_id)` on `CallbackInfo` reads a seat from a callback
      (`get_current_mouse_state` is the primary by definition). `ModifyWindowState` /
      `QueueWindowStateSequence` copy and hit-test the seats; `first_differing_state_field`
      classifies `pointer_seats` as event-bearing (the exhaustive destructure caught it).
      ALSO: `InputSample` fields reordered by alignment - the api checker had been exiting 1 on
      its padding since 9g-ii-d exposed it, so "autofix converged" was being read off a red exit.
      EVIDENCE: 10 layout tests (accessors; second seat's press targets the node under THAT
      seat with its seat id and position; primary events carry seat 0 and the stamped device id;
      vanished seat releases; new seat diffs from default; click per seat; single-seat state
      derives exactly what it did before; press targets per seat) + 1 dll test. NEGATIVE
      CONTROL: making `for_seat` always answer `Mouse` fails the two targeting tests. Host check,
      8/8 mobile, autofix converged at EXIT=0, `codegen all`, C header carries
      `AzPointerSeatVec pointer_seats` and `AzCallbackInfo_getPointerSeatState`. ⚠ No MPX setup
      here: the X11 path is compiled (linux-gnu is in the gate), not seen.
- [x] 9b-ii-a DONE - and the single-seat shell was WORSE than "one cursor for two seats". The
      registry arm assigned `window.seat` for EVERY `wl_seat` global, so on a two-seat
      compositor the second seat silently REPLACED the first as "the" seat (text input, the
      clipboard data device, the tablet seat and xdg move/resize all followed it), while both
      seats' pointers - each with the same listener and the same user data - kept writing the
      one global mouse state: cursor A's buttons at cursor B's position.
      DESIGN: the seat id is the registry global's NAME, the first seat bound is the primary
      (`FullWindowState::mouse_state`, everything pre-existing), and the pointer handlers tell
      seats apart by the `wl_pointer` PROXY an event names - NOT by per-listener user data as
      the item proposed, because `rebind_listeners` re-points every tracked proxy's user data
      wholesale to the window once it is boxed, and a per-seat payload would have been
      overwritten on the first pump. "Unknown pointer means primary" keeps every path the
      single-seat shell had exactly as it was: a second seat can be recognised, never invented.
      The bookkeeping (`SeatTable<P>`: first = primary, id by name, pointer capability
      set/cleared, removal refuses the primary) lives in `shell2/common/seats.rs`, generic over
      the proxy type, so its 4 tests RUN HERE rather than sitting behind `cfg(linux)` - the
      `sensors/units.rs` lesson again.
      PRODUCER: a non-primary seat binds its POINTER only; enter / leave / motion / button go to
      `handle_seat_pointer_*`, which write the seat's own `MouseState`, hit-test into its own
      hover history and run the ordinary diff pass (which is what makes the click arrive with
      the seat id on the event); the axis family is dropped for a second seat, as on X11
      (9b-ii-b: the frame accumulator is the window's, and mixing a second seat's axis events in
      would scroll under the first cursor). A seat losing its pointer capability, or its global
      being removed, removes the seat's state and its hover point, destroys its proxies, and
      runs the pass so held buttons release. `last_input_serial` stays the primary's: it
      authorises the primary's data device, and another seat's serial would be rejected there.
      `pointer_device_id` stays 0 on Wayland by design (libinput is behind the compositor).
      EVIDENCE: 4 host tests on the table (primary-by-order not by name; pointer lookup with
      unknown = primary; capability loss; removal refuses the primary and keeps ids by name).
      NEGATIVE CONTROL: answering "primary" for every pointer fails the lookup test. Host check
      (the table + tests), 8/8 mobile (the Wayland shell compiled on linux-gnu). ⚠ No multi-seat
      compositor here (Sway/wlroots with `seat seat1 attach ...` is the way to see it).
- [x] 9b-ii-a-i DONE end to end: the engine half (per-seat `KeyboardState`, the stamped events,
      the diff, the shell's application, the headless ops) and both producers - Wayland
      (9b-ii-a-i-b, a `wl_keyboard` + xkb state per seat) and X11 (9b-ii-a-i-a, XI2 key events
      routed to the master keyboard's paired-pointer seat). What stays shared by design is FOCUS
      (9b-ii-a-i-d, now ruled wanted per seat - the next arc) and, per producer, the input method,
      compose and key repeat. Touch per seat is 9b-ii-a-i-c.
- [x] 9b-ii-a-i-a DONE, and not the rewrite the note feared: the core key path is UNTOUCHED. XI2
      `XI_KeyPress` / `XI_KeyRelease` are selected on `XIAllMasterDevices` beside the pointer
      events; on arrival the virtual core keyboard's are dropped (its keys already come as core
      `KeyPress` / `KeyRelease`, with the input method and compose sequences, so nothing doubles)
      and any OTHER master keyboard routes to the seat of its paired master pointer -
      `master_keyboards`, master keyboard id -> `attachment`, read off `XIQueryDevice` at
      creation and refreshed on `XI_HierarchyChanged`; a keyboard paired with the virtual core
      pointer, or an unknown master, counts as the primary rather than a phantom seat.
      `handle_seat_key_event` is the per-seat twin of `handle_keyboard`: virtual keycode from
      `unmodified_keysym` (the keycode at group 0), text from a core `XLookupString` over a
      synthesised `XKeyEvent` carrying the event's effective modifiers, modifier masks and key
      state onto `keyboard_seat_mut(seat)` through the same `apply_*` helpers, the text at the
      SHARED focus. Not the seat's: the input method, compose and key repeat (one of each per
      window). Per-layout xkb state per master keyboard is not modelled - the keycode's group-0
      symbol is read from the display's map; a second seat with a different layout gets its
      symbols from the primary's map (9b-ii-a-i-a-i). No X server on this machine: implemented
      blindly per the user's ruling, the Linux target in the gate is the check.
      ✅ COMPILED in the seventh batch pass of 2026-09-04: the Linux target checks green after one
      visibility fix (`unmodified_keysym` was private to its module); the other seven gate targets
      do not compile the X11 backend. Blind by design - no X server here.
- [ ] 9b-ii-a-i-a-i A second MPX seat with its OWN keyboard layout: XI2 key events carry the
      master's `group` state, but the keysym is looked up in the display's one core keymap, so a
      seat on a different layout is translated through the primary's. Needs an xkb keymap per
      master keyboard (`xkb_x11_keymap_new_from_device`) - the same shape the Wayland seats have.
- [x] 9b-ii-a-i-b DONE. The seat table carries a `wl_keyboard` per seat (`keyboard_of` /
      `set_keyboard` / `seat_id_for_keyboard`, keyed by the PROXY like the pointers); the
      capabilities handler binds a non-primary seat's keyboard the moment it is advertised and
      drops it (`handle_seat_keyboard_gone`, which removes the keyboard seat from the window
      state) when it goes. Each such seat owns its xkb objects - the keymap event is per
      keyboard, so `parse_xkb_keymap` (the primary's mmap + compile + state, factored out) fills
      the seat's `WaylandKeyboardState` - and its own scancode -> keycode map. The six
      handlers route by `seat_id_for_keyboard`: keymap, key (`handle_seat_key`: keysym from
      the seat's state, `apply_key_state_change` on `keyboard_seat_mut(seat)`, the typed text
      through `record_text_input` at the SHARED focus), modifiers (the seat's mask + lock
      flags), enter (held keys onto the seat), leave (everything released). Deliberately the
      primary's alone: the popup route, compose sequences and the IME (one composition per
      window), and key repeat (9b-ii-a-i-b-i). No Wayland session on this machine, so the
      Linux target in the gate is the check. ✅ COMPILED AND RUN in the sixth batch pass of 2026-09-04: host check EXIT=0 after the api.json
      pass (`WindowFlags.extend_into_safe_area`, one modification, codegen green); core 2809,
      layout lib 7681 and `--test all` (the inset and registry tests among them, after three test
      fixes), dll 2035 tests green; the 8-target gate green - the Linux target after the seventh
      batch's one visibility fix, the other seven first time.
- [ ] 9b-ii-a-i-b-i Key REPEAT for a second seat: `key_repeat_fd` / `key_repeat_keycode` are one
      timer, the primary's, so a second seat's held key types once. Needs a timerfd per seat
      (or one timer with a seat tag) fed from that seat's `repeat_info`.
- [ ] 9b-ii-a-i-c TOUCH per seat: `TouchState` is one list; a second seat's touchscreen would mix
      its ids into the primary's. Same shape as the keyboard seats (`TouchSeatVec`), gated on a
      producer that has one - X11 XI2 touch events carry the master, Wayland `wl_touch` is per seat.
- [~] 9b-ii-a-i-d Per-seat FOCUS. X11 MPX gives every master keyboard its own focus; azul has one
      focused node, so today every seat types into the same field. A per-seat focus is a product
      question (two people editing two fields of one window), not a wiring gap; logged, not guessed.
      USER RULING 2026-09-04: NOT a product question - "that should be possible, maybe with a UUID
      owner". Per-seat focus is wanted: two people editing two fields of one window. Design
      direction: focus keyed by seat (the primary = seat 0) the way selections are keyed by
      `SelectionOwner`; a seat's keys target that seat's focused node; click-to-focus and Tab per
      seat; the text-edit session per seat follows. An arc; the engine-side keyboard seats
      (9b-ii-a-i) are its input.
- [x] 9b-ii-a-i-d-i DONE - the focus STATE per seat and the three routes that move it. Seat 0
      IS `focused_node` (the pointer-seat rule: the primary keeps its old field, the others live
      beside it); `FocusManager::seat_focus` holds the rest with `focused_node_for` /
      `set_focused_node_for` / `has_focus_for` / `seats_focusing`, remapped on a DOM rebuild
      like the primary (unmounted = cleared). ROUTES: (1) a seat's KeyDown / KeyUp is stamped at
      THAT seat's focused node (`event_determination`; root when none), the `Focus(..)` filter
      dispatch reads the event's seat, and the input interpreter resolves the key against the
      seat's focus (`InputInterpreterInfo::seat_focus` from `seat_focus_of_events`, so a second
      seat's arrows and shortcuts see its own field); (2) click-to-focus: a non-primary seat's
      press moves ITS focus onto the focusable under ITS cursor (`SystemChange::SetSeatFocus`),
      leaving the primary's focus, caret and ring alone; (3) Tab / arrows / Escape: the default
      action reads the key seat's keyboard and walks the key seat's focus. API:
      `CallbackInfo::set_focus_for_seat` / `clear_focus_for_seat` / `get_focused_node_for_seat`
      / `is_node_focused_for_seat` (`CallbackChange::SetSeatFocusTarget`, resolved against that
      seat's current focus). `seat_of_event` now reads the keyboard seat too - a keyboard seat is
      its paired pointer seat on both producers. WHY the gap existed: focus predates seats and
      was one field read from ~270 sites; the "product question" note was refuted by the ruling.
      Tests: the manager's independence and remap. ✅ COMPILED in the eighth batch pass of 2026-09-04: host check EXIT=0; autofix converged at 0 patches / 0 FFI errors after two source fixes (`natural_scroll` moved beside `log_level` so the alignment checker sees no padding; the runner and the seat-focus arm gained the missing match arms); `codegen all` EXIT=0; azul-core 2809, azul-layout lib 7683 / `--test all` 1003, e2e corpus 62 scenarios, azul-dll 2040 (+5), all 0 failed; 8/8 gate targets green including windows-gnu, which compiles the hid.dll and registry code.
- [x] 9b-ii-a-i-d-ii DONE - a seat's TYPING lands in its own field at its own caret. The queued
      edit carries its seat (`QueuedTextEdit::seat_id`, `record_input_for_seat`), the window has
      `record_text_input_for_seat` (`record_text_input` = seat 0), and `apply_one_text_changeset`
      applies a non-primary seat's edit at THAT seat's caret (`TextEditManager::seat_carets`, a
      `SeatCaret { node, cursor }` per seat; none yet in that node = the end of its text, where a
      fresh focus puts the primary's too), then sets the seat's caret from the edit result and
      shifts every other caret on the node across the change - the primary's via
      `shift_all_across`, the other seats' via `shift_seat_carets_across` - the U3 peer rule
      applied to seats. The primary's blink reset, cursor-rect undo positions and multi-cursor
      update stay the primary's. Seat carets follow a DOM rebuild (`remap_node_ids`, unmounted =
      cleared). Producers: the X11 `handle_seat_key_event` and Wayland `handle_seat_key` text
      paths now feed the seat, which is exactly where a second person's keystrokes used to land
      in the primary's field. WHY the gap existed: the edit pipeline read one focused node and one
      caret; per-seat focus (d-i) made the node per seat but the caret and the record path were
      still singular. Tests (`seat_text_session.rs`): a seat appends to its own field while the
      primary edits another at byte 0, its caret follows and its second keystroke continues there,
      the primary's field and caret are untouched; both carets shift across each other's edits in
      one field; a seat without focus types into nothing. Evidence: host check EXIT=0; azul-layout
      lib 7683 / `--test all` 1006 (+3), e2e 62 scenarios, azul-dll 2071, 0 failed; 8/8 gate.
- [ ] 9b-ii-a-i-d-ii-a The seat caret is tracked but not DRAWN: the display list emits the
      primary caret (and peer carets from the multi-cursor's node only). Draw `seat_carets` in the
      owner colour scheme the peer carets use, blinking on the primary's clock.
- [x] 9b-ii-a-i-d-ii-b DONE for the caret OPS: `SystemChange::ApplySelectionOp` carries its
      `seat_id` (stamped from the key event by `handle_key_down`), the shell and the runner apply
      it through `apply_selection_op_for_seat`, and a non-primary seat's Move / Extend / Delete act
      on ITS caret in ITS node: a `SeatCaret` now has an `anchor`, so Shift+arrows make a seat
      selection and typing over it replaces it; Move collapses an anchored selection to its edge on
      a character step (the multi-cursor's rule); Delete removes the selection or one step (a word
      / line step extends first), records the same undo entry as the primary's delete (the undo
      block of `delete_selection` was factored into `record_delete_undo` for both), and shifts the
      primary's and the other seats' carets on the node across the change. A seat with no caret in
      the node starts at the layout's LAST-CLUSTER cursor (a trailing cursor, the shaped layout's
      own end-of-text form), because the byte-past-the-end cursor the edit path uses is not a
      cluster the step resolver can walk from - the primary showed the same 4 -> 1 jump from a
      stale layout. FOUND ON THE WAY, fixed in the shared primitive: `edit_text_outcome` decided
      "applied" from the byte and run deltas, so overwriting a one-character selection with one
      character was reported `EverySelectionMissed` and dropped - for the primary too; it now
      compares content. Tests: seat Left / Backspace / Shift+Right / overwrite / no-op Delete with
      the primary's caret and field untouched throughout, plus the primitive's regression.
      Evidence: host check EXIT=0; azul-core 2809, azul-layout lib 7683 / `--test all` 1008 (+2),
      e2e 62 scenarios, azul-dll 2071, 0 failed; 8/8 gate. Still the primary's: the SHORTCUT
      changes (ii-b-i).
- [ ] 9b-ii-a-i-d-ii-b-i A seat's SHORTCUTS: `CutToClipboard`, `CopyToClipboard`,
      `PasteFromClipboard`, `SelectAllText`, `UndoTextEdit` / `RedoTextEdit`, `SelectNextOccurrence`
      and the Enter split (`SplitBlockAtCursor`) carry a `target` but no seat, and their arms read
      the multi-cursor - a second seat's Ctrl+A selects the primary's field. Same recipe as the
      ops: a `seat_id` on each change and a seat branch in the arm; Cut / Copy need the seat's
      selection extracted (`get_selected_content_for_clipboard` reads the multi-cursor).
- [ ] 9b-ii-a-i-d-ii-c IME / preedit per seat: `ime_document`, `preedit_text` and the composition
      phase are the primary's; a second seat's input method composes into the primary's field.
      Wayland has one text-input per seat, so the producer exists; the engine side does not.
- [ ] 9b-ii-a-i-d-ii-d Undo attribution: a seat's edits enter the one document undo stack with
      `Uninitialized` cursor positions; Cmd+Z on any seat undoes them in order. Fine for a shared
      document, wrong if per-person undo is wanted - a product question, logged not guessed.
- [ ] 9b-ii-a-i-d-iii `:focus` styling and the a11y focus are single: a seat's focused node gets
      no ring / no `:focus` restyle (`apply_focus_restyle` runs for the primary only) and
      accesskit sees one focus. A per-seat ring needs a pseudo-class per seat or a seat-coloured
      overlay; a11y has no multi-focus model at all - log, do not invent.
- [ ] 9b-ii-a-i-d-iv The e2e runner and the headless ops drive the primary seat only
      (`seat_focus: &[]`, `CallbackChange::SetSeatFocusTarget` falls to its `_` arm); a seat-focus
      scenario needs a headless "press on seat N" op plus the runner's port of `SetSeatFocus`.
- [ ] 9b-ii-a-i-d-v One default action per dispatch pass: when the primary's and a seat's KeyDown
      land in the SAME pass, the first KeyDown's seat wins and the other seat's Tab is dropped.
      Rare (two people hitting Tab in one frame), but a per-seat default-action loop is the fix.

- [x] 9b-ii-a-ii DONE. "Whatever the compositor shows for an unset cursor" is NOTHING: a
      Wayland pointer has no cursor over a surface until the client answers its `enter` with
      `set_cursor`, so a second seat was an invisible cursor. `set_cursor` is now a wrapper over
      `set_cursor_for(seat_id, ..)`: the primary keeps `pointer_state`'s pointer / serial /
      surface, a seat uses its table pointer, the serial of its last enter (`seat_serials`, set
      in the seat enter handler, which now receives the serial) and its own cursor surface
      (`seat_cursor_surfaces`, one per pointer so no surface has to serve two cursor roles;
      destroyed with the seat). `sync_seat_cursor_image` runs on the seat's enter and every
      motion, resolving the icon from THAT seat's hit test through the same
      `compute_cursor_type_hit_test` the primary uses, defaulting to the arrow with nothing hit
      - an unanswered enter is invisible, not an arrow - and re-sending only on change.
      ✅ COMPILED AND RUN in the 2026-09-03 batch pass (evidence: the pass note on 9h-i-a-i-b); the Linux target is the one to watch.
- [x] 9b-ii-a-iii DONE. The "real per-seat question" had a plain answer once split in two: the
      POPUP is not per seat (any cursor clicking an item selects it, so the popup's own
      enter / motion / button / leave handlers serve every seat unchanged), while KNOWING WHOSE
      EVENTS ARE THE POPUP'S is - and that knowledge was one bool (`pointer_over_popup`), so it
      was only ever the primary's, and a second seat entering the popup surface had its
      popup-relative coordinates hit-tested against the main window. `seat_over_popup` (a set of
      seat ids) is the per-seat twin: the seat enter handler now receives `over_popup`, resolved
      in the listener where the raw `wl_surface` is compared against the popup's exactly as for
      the primary, and the seat's motion, button and leave route to the popup while it is in the
      set. Forgotten when the seat goes. The xdg_popup GRAB itself stays the primary seat's - the
      grab is what makes clicking outside dismiss the menu, and xdg-shell allows one grabbing seat
      per popup; a second seat's outside-click therefore does not dismiss (9b-ii-a-iii-a).
      COMPILED AND RUN in the batch pass of 2026-09-03: host check EXIT=0; core 2794, layout 7671 +
      999 (`--test all`), dll 2015 tests green; the e2e corpus (57 scenarios, incl. the new
      second-seat scenario) green; the 8-target gate green after one Android fix (the pan block
      read a vector the touch-state refresh had moved - caught by the gate, not by the host).
- [x] 9b-ii-a-iii-a DONE. The X11 rule ported turned out to need no bounds at all on Wayland:
      X11 checks bounds because the grab delivers EVERY press to the menu window, inside or out;
      on Wayland a second seat's press that reaches the PARENT surface while a popup is open is
      by construction outside the popup (a press over the popup surface was already routed to it
      by 9b-ii-a-iii). So `handle_seat_pointer_button` now treats "a press on the parent while
      `active_popup` is open" as the click-outside: it marks the popup dismissed through the same
      signal the compositor's `popup_done` uses (`WaylandPopup::mark_dismissed`, `is_open=false`
      before the popup is configured), so the run loop drops it through the ONE dismissal path -
      `drive_active_popup` -> `dismiss_active_popup` - and a `<transient-window>` popup's parent
      mailbox learns of it exactly as for the primary seat. The press is swallowed, as X11 swallows
      its outside press and as the compositor swallows the primary's: the press that closes a menu
      does not click through to what was under it.
      COMPILED AND RUN in the batch pass of 2026-09-03: host check EXIT=0; core 2794, layout 7671 +
      999 (`--test all`), dll 2015 tests green; the e2e corpus (57 scenarios, incl. the new
      second-seat scenario) green; the 8-target gate green after one Android fix (the pan block
      read a vector the touch-state refresh had moved - caught by the gate, not by the host).
- [x] 9b-ii-b DONE for the wheel, pointer capture and the dedup; the gestures are 9b-ii-b-i.
      THE WHEEL: `ScrollManager::record_scroll_from_hit_test` already took an `InputPointId`, so
      the scroll physics had been per input point all along - what was missing was a producer
      that hit-tested into the seat's own history and asked about `Seat(id)`. X11:
      `handle_scroll_input` is now a wrapper over `handle_scroll_input_for_seat`; a second
      master's legacy buttons 4-7 (press only, the emulated pair beside a smooth valuator
      skipped, exactly the core rule) and its XI_Motion smooth-scroll valuators both go through
      it. Wayland: a `SeatAxisFrame` per non-primary seat (`seat_axis` map) accumulates that
      seat's `axis` / `axis_discrete` / `axis_value120` / `axis_source` until ITS `frame`, then
      `flush_seat_axis` hit-tests at the seat's own cursor and hands the delta to the new shared
      `dispatch_scroll_delta(input_id, ..)` - the record-and-arm-the-physics-timer block that
      `flush_pending_axis` and the seat flush now share. The `Scroll` EVENT names the seat too:
      `ScrollEventData.seat_id` (Rust-only) and `ScrollManager.pending_wheel_seat`, and
      `determine_all_events` aims it at `hover_node_full_for(Seat(id))`, so a wheel-as-zoom widget
      under the second cursor is the one that zooms.
      ⚠ FOUND ON THE WAY, a 9b-ii-a hole: the axis guards were inserted by matching the handler's
      `let window = ... as *mut WaylandWindow` line, and `axis_value120` and
      `axis_relative_direction` spell the type `super::WaylandWindow` - so those two were never
      guarded (a second seat's high-resolution wheel fed the PRIMARY's frame) and the discrete
      handler got THREE guards. All seven now route by seat, checked per handler.
      POINTER CAPTURE: `PointerCapture { seat_id, node }`; `CallbackChange::CapturePointer` carries
      a seat the DISPATCHER stamps - the callback cannot know it, so `capture_pointer` writes the
      primary and `dispatch_events_propagated` rewrites it from the planned invocation's event
      (`PlannedInvocation.seat_id`, `hover::seat_of_event`). `hover::apply_pointer_capture` is the
      retarget rule (host-tested): only the capturing seat's moves and release are retargeted, and
      only that seat's release ends it. Both the dll and the e2e runner use the one type.
      DEDUP: `deduplicate_synthetic_events` keys on the seat as well, so two cursors pressing one
      node are two presses; one seat pressing twice in a pass still coalesces; non-pointer events
      key on the primary and behave exactly as before.
      EVIDENCE: a determination test (a delta recorded for seat 7 aims `Scroll` at the node under
      seat 7 with `seat_id` 7; the primary's still at the primary's), the capture test (the other
      seat's move passes through, its release does not end the capture, the captured seat's does)
      - NEGATIVE CONTROL: dropping the seat filter fails it - and a core dedup test. Host, the dll
      test target, the Linux target directly (X11 + Wayland compile) and 8/8 mobile.
- [x] 9b-ii-b-i DONE - the manager is keyed by SEAT, not rewritten. `InputSession` carries
      `seat_id` (0 = the primary seat's mouse and touches), `seat_sessions` mirrors the touch
      map, and `seat_down` / `seat_move` / `seat_up` are the per-seat twin of the touch API. The
      load-bearing change: every "current session" reader (`get_current_session`,
      `end_current_session`, the sample recorder) now means the last PRIMARY session, so a second
      cursor's press can no longer hijack the primary's drag / long-press / double-click / click
      count - it used to, silently, since sessions were one flat list. Per-seat detectors
      (`detect_drag_for`, `detect_long_press_for`, `detect_double_click_for`,
      `detect_click_count_for`, `current_session_for`) read one seat's sessions only; the
      primary's detectors delegate to seat 0. Feeding is platform-independent: the shell derives
      each seat's press / move / release from the window-state diff of `pointer_seats` before the
      event pass (`feed_seat_gesture_sessions`), so X11 MPX and Wayland seats get gestures
      without backend wiring. Events: `pointer_seat_events` emits a second seat's `DragStart` /
      `Drag` / `DragEnd` (on the node it pressed, edges computed from the samples - no latch),
      `DoubleClick` and `LongPress`, all stamped with the seat. Wayland: a second seat's
      `axis_stop` records `TrackpadEnd` under the seat's own input point (its momentum phase
      releases its own rubber band) and `axis_relative_direction` lands on the seat's frame -
      recorded like the primary's, which turned out to be unconsumed too (see 9b-ii-b-i-a).
      Pen stays primary-only (9b-ii-b-i-b). ✅ COMPILED AND RUN in the fourth batch pass of 2026-09-03: host check EXIT=0 first try;
      api.json untouched (autofix: 0 modifications); layout lib 7679 and `--test all` 999 tests
      green after the one break - eight test literals of `InputSession` without `seat_id` -
      was fixed (d42e0d757); dll lib tests green; the 8-target gate 8/8.
- [x] 9b-ii-b-i-a DONE per the USER RULING (2026-09-04). `AppConfig::natural_scroll:
      NaturalScroll { Disabled (default), Enabled, System }`, published app-globally like the
      pinch setting and read by every `ScrollManager::new` (the `AZ_NATURAL_SCROLL` env override
      still wins). `Enabled` flips every delta in the engine; `System` reads the platform's
      preference once at startup (`extra/natural_scroll.rs`: macOS `NSUserDefaults`
      `com.apple.swipescrolldirection`, absent = on; Windows the precision touchpad's
      `ScrollDirection` registry key, `0` = natural - both blind per the ruling; Wayland keeps
      it current from `axis_relative_direction`, which is the compositor's own word; X11 and
      mobile report nothing) and makes it readable - `CallbackInfo::get_natural_scroll` (the
      engine's flip), `get_system_natural_scroll` / `has_system_natural_scroll` (the platform's
      answer). THE POINT THAT DECIDED THE SEMANTICS: every desktop platform already applies the
      user's preference to the deltas it hands over (macOS, the precision touchpad, libinput on
      Wayland and X11), so `System` must NOT flip again - it would undo the user's setting; the
      previously recorded-but-unread Wayland flag was exactly a double inversion waiting to
      happen, and it is now consumed as a READING. The "check on a real compositor" the note
      asked for still stands as the device check (9b-ii-b-i-a-i). ✅ COMPILED in the eighth batch pass of 2026-09-04: host check EXIT=0; autofix converged at 0 patches / 0 FFI errors after two source fixes (`natural_scroll` moved beside `log_level` so the alignment checker sees no padding; the runner and the seat-focus arm gained the missing match arms); `codegen all` EXIT=0; azul-core 2809, azul-layout lib 7683 / `--test all` 1003, e2e corpus 62 scenarios, azul-dll 2040 (+5), all 0 failed; 8/8 gate targets green including windows-gnu, which compiles the hid.dll and registry code.
- [ ] 9b-ii-b-i-a-i Device check (the user: "we'll check later"): on a v9 compositor with natural
      scrolling on, confirm the deltas already arrive inverted and `get_system_natural_scroll`
      reads true; on macOS confirm the absent-key default; on a Windows precision touchpad the
      `ScrollDirection` values.
- [x] 9b-ii-b-i-b DONE. `seat_pens: BTreeMap<u64, SeatPen>` holds the non-primary seats' pens
      (state, previous, pending flag, own report-rate estimate); the primary keeps its three
      fields. `update_pen_state_full_for` / `clear_pen_state_for` / `set_pen_hover_distance_for`
      / `set_pen_tool_kind_for` / `pen_state_for` route seat 0 to the primary path unchanged
      and everything else to its slot; `clear_pen_event_pending` clears the seats' flags with
      the primary's, one clear per pass. The event pass diffs each seat's slot into
      `PenEnter` / `PenLeave` / `PenDown` / `PenUp` / `PenMove` / `PenHover` on the node under
      THAT seat's cursor, source `Pen`, stamped with the seat. Producer: X11 - the stylus is a
      slave of one master pointer and the master IS the seat (`ev.deviceid`, virtual core
      pointer = 0), so a second MPX seat's stylus is its own pen now. Wayland's producer stays
      on seat 0 (9b-ii-b-i-b-i); Windows, macOS, iOS and Android are single-seat.
      ✅ COMPILED AND RUN in the fourth batch pass of 2026-09-03: host check EXIT=0 first try;
      api.json untouched (autofix: 0 modifications); layout lib 7679 and `--test all` 999 tests
      green after the one break - eight test literals of `InputSession` without `seat_id` -
      was fixed (d42e0d757); dll lib tests green; the 8-target gate 8/8.
- [ ] 9b-ii-b-i-b-i Wayland binds ONE `zwp_tablet_seat_v2` - the primary `wl_seat`'s - so a
      second seat's tablet never reports at all. A tablet seat per bound seat (the seat table
      of 9b-ii-a already carries the `wl_seat` proxies) with the tablet listeners' user data
      naming the seat, then `update_pen_state_full_for(seat_id, ..)` in `handle_tablet_frame`.
      The engine side is ready; this is the protocol binding.
- [x] 9b-ii-c DONE - as a FIELD, not a new op: `mouse_move` / `mouse_down` / `mouse_up` take
      `"seat": N` (default 0, the ordinary mouse), applied through `FullWindowState::pointer_seat_mut`,
      whose seat 0 IS `mouse_state` - so the three appliers have ONE code path and a seat op is
      the same op with one more key. `ModifyWindowState` then carries the seats (9b-ii) and the
      runner's arm now hit-tests each changed seat into its own hover history
      (`update_hit_test_for(InputPointId::for_seat(id), pos)`, with `update_hit_test_at` a
      wrapper over it - the dll's `update_seat_hit_test_at` split).
      ⚠ THE SCENARIO EXPOSED A GAP the determination tests could not: click-to-focus in BOTH
      dispatchers read the PRIMARY's hit test for every press, so a second seat's press would
      have focused whatever the first cursor hovered. Both now take the hit test of the seat
      that pressed (`seat_of_event` on the first `MouseDown` of the pass).
      `e2e/seat-second-cursor-click.json`: primary rests on r1, seat 7 presses r3 -> focus r3;
      the primary clicks r1 -> focus r1; seat 7 presses r3 again -> focus r3 (its position
      persisted across the primary's activity). `click` and the higher-level ops stay
      primary-only; a seat click is `mouse_down` + `mouse_up` with `seat`.
      ✅ COMPILED AND RUN in the 2026-09-03 batch pass (evidence: the pass note on 9h-i-a-i-b); written uncompiled per the user ("put all the compilation at the end"),
      this and the following items are checked against rust-analyzer only; the batch compiles
      once at the end, and the scenario runs with the corpus then.

### QUEUED BY THE USER - 2026-09-03, collaborative editing

These three came in together and belong together: they are the model a multi-user editing
session needs. Recorded verbatim so the framing is not lost.

- [x] U1 DONE - the MODEL, the injection API and the PAINT. A shared editing session can now
      show several people's carets, each in their own colour.
      `SelectionOwner` is a 128-bit id (two `u64`s, because `u128` has no settled C layout) and
      NOT a reuse of `SelectionId`: that one is engine-allocated and local, while an owner has to
      survive a NETWORK - two machines cannot agree on a counter one of them allocated. `LOCAL`
      is all-zero, so `Default` is a local selection and every existing construction site kept
      meaning what it meant.
      ⚠ THE MERGE WAS THE REAL HAZARD. `merge_overlapping` collapsed any two adjacent selections,
      so two participants whose carets touched would have become ONE - silently deleting someone
      from the session, their caret absorbed into another person's and repainted in that person's
      colour. Merging is now confined to a single owner, and the sort is by owner first so the
      adjacency check only ever compares two of the same person's.
      INJECTION REPLACES, IT DOES NOT ACCUMULATE: a remote participant's state is a SNAPSHOT, and
      adding to it would leave a stale caret behind every time a message was missed. And `LOCAL`
      is REFUSED at that door - the local caret is the engine's, and letting an app overwrite it
      there would put every text-editing invariant the engine maintains into the app's hands.
      COLOUR IS PER OWNER, NOT PER NODE, which is why it is a registry rather than a CSS property:
      `caret-color` answers "what colour is the caret in this field", and a session needs "what
      colour is Alice" - a different axis, and one no stylesheet can know because the participants
      are decided at runtime. An unregistered owner falls back to `caret-color`, which is what the
      LOCAL caret always does, so a single-user app is untouched by any of this.
      `(DomId, NodeId, TextCursor)` became a named `CursorLocation` carrying the owner: the paint
      site is where the colour is chosen, and looking the owner up from a parallel list would be a
      desync waiting to happen.
      EVIDENCE: 6 model tests with a NEGATIVE CONTROL - allowing the merge to cross owners fails
      with "local + two peers, all at offset 0". `codegen all` + a dll build put
      `AzSelectionOwner` and `AzCallbackInfo_setRemoteSelections` in the C ABI; autofix converged
      at 0 patches and `azul-doc check` PASSED. Host, 8/8 mobile, azul-core 2785, azul-layout
      7638. ⚠ No two-machine session here - the paint path is compiled and unit-tested, not seen.
- [x] U1-a DONE - and it was hiding a worse defect than the one logged. The logged gap was "a
      remote range draws in the node's `::selection` colour". The actual state after U1 was that
      `session_selection_ranges` walked EVERY selection in the list regardless of owner, so a
      peer's range was reported as the LOCAL session's: painted in the local colour, yes, but
      also handed to everything downstream that reads "what has the local user selected" - the
      `::selection` glyph recolour and the copy path among them. Injecting a remote owner made
      the local user appear to have selected somebody else's text.
      Two accessors now, on purpose: `session_selection_ranges` is LOCAL-only (a `continue` on
      `!owner.is_local()`), and a new `remote_selection_ranges` reports the peers'. Not a field
      on the local result, because that result is `None` for a caret-only session by design ("a
      bare caret is not a selection") - and "I have a caret, you have a range" is THE ordinary
      collaborative state. Hanging the remote ranges off the local accessor would have made
      everybody else's highlight vanish the moment the local user collapsed their own. For that
      case `build_text_selections_map` emits a collapsed `TextSelection` at the local caret with
      the remote map filled: `is_collapsed` suppresses the local highlight, the remote pass runs.
      `TextSelection.remote_ranges: BTreeMap<NodeId, Vec<(SelectionOwner, SelectionRange)>>` is
      a SEPARATE map from `affected_nodes` rather than a tag on each range, because the two mean
      different things and paint differently; mixing them is exactly what the defect was.
      Paint (`display_list.rs` selection pass): remote ranges FIRST so a local range over the
      same span stays on top, each in `remote_selection_tint(owner_colors[owner])` = the owner's
      hue at `min(a, 0x66)` - an owner colour is chosen to be legible as a 1px opaque caret and
      would hide the text as a fill; `min` so an app that picked a faint colour stays faint. An
      owner with no registered colour still paints (in `::selection`) - an invisible range is
      indistinguishable from a bug. The remote pass is deliberately NOT gated on `is_collapsed`,
      which describes the local anchor/focus only.
      Evidence: 5 host tests in `text_edit.rs` (local-only filtering; the map separates the two;
      a remote range SURVIVES a collapsed local caret; a single-user session allocates no remote
      map; the tint keeps hue and caps alpha). Negative control: removing the owner `continue`
      fails 3 of them (the two separation tests and the collapsed-caret one). Host check, 8/8
      mobile. ALSO FIXED on the way: `layout/tests/menubar_item_clip.rs` had not been updated for
      U1's `owner_colors` argument (two `layout_document` calls, 20 of 21 args) - broken since
      the previous item, unseen because `--lib` never builds `tests/`. ⚠ Still no two-machine
      session here: the paint path is compiled and unit-tested at the map level, not seen.
- [x] U2 DONE on both platforms, and BOTH had a gap - the mirror image of each other.
      ⚠ ANDROID'S TOOLBAR HAS NEVER APPEARED. `NativeTextBridge.startSelectionToolbar` is
      complete Java - an `ActionMode.Callback2`, the menu items, a content rect that follows a
      moving selection - with NO CALLER anywhere in Rust. My own note for this item said Android
      "already routes ActionMode cut/copy/paste/select-all through `SystemChange`", which is true
      of the ACTIONS and false of the bar that sends them: nothing ever started it. Classic "live
      in Java, dead in the product".
      iOS had the opposite half missing. UIKit's selection toolbar is not a widget an app builds -
      it is UIKit asking the FIRST RESPONDER by selector which standard edit actions it can
      perform - and a responder implementing none of them gets no menu at all. So a long-press on
      azul text on iOS offered nothing.
      Both now route to the SAME `SystemChange` variants rather than growing a second clipboard
      path that could drift.
      `canPerformAction:` ANSWERS FROM REAL STATE, not `true`: copy and cut need a selection,
      paste asks the PASTEBOARD (another app can put something there while this one is
      backgrounded, so a cached answer is stale exactly when the user comes back to paste), and
      select-all needs text that is not already all selected. A blanket yes would offer Copy on an
      empty document and Paste with an empty clipboard.
      `select:` IS NOT `selectAll:`. UIKit sends it for the first long-press on unselected text
      and it means "select the word here"; answering it with select-all gives the user the
      document when they asked for a word. `LayoutWindow::select_word_at_caret` is the new seam,
      built on the `select_word_at_cursor` the double-click path already used.
      CUT NAMES ITS TARGET where the others do not - it deletes, so the engine has to know from
      which node - and does nothing without a focused one rather than inventing a target.
      The Android bar is driven off the ENGINE's selection rather than off a gesture, and on the
      TRANSITION only: `startActionMode` is a UI-thread hop, and the bar refreshes its own
      position through `onGetContentRect`. Driving it from the engine also means a selection made
      by Select All, or by a remote participant under U1, raises the same bar as a double-tap.
      EVIDENCE: all three iOS seams proven COMPILED on aarch64-apple-ios AND x86_64-apple-ios,
      both Android seams under `_internal_deps`, and the JAVA re-compiled against android-34.
      Host, 8/8 mobile, azul-layout 7638, autofix 0 patches. ⚠ No device or simulator - neither
      toolbar has actually been seen.
- [x] U2-a DONE, on both platforms, by two different routes - and the premise was half wrong.
      ⚠ iOS DOES HAND A CUSTOM VIEW ITS HANDLES. The note above ("no platform API hands them to
      a custom view") is true of Android and false of iOS: `UITextInteraction` (iOS 13+) exists
      for exactly this - the grab handles, the loupe and the tap / long-press gestures for a
      custom `UITextInput` view - and it drives them through the very seams 10b-i wired
      (`closestPositionToPoint:`, `setSelectedTextRange:`, `firstRectForRange:`). Nothing had
      created one, so a range made by `select:` had no handles to drag. Now
      `+[UITextInteraction textInteractionForMode:]` (editable = 0) with `textInput = view` is
      added next to the pencil interaction, gated on the class existing. The engine paints NO
      handles there: two sets for one selection is worse than none.
      ANDROID IS THE ENGINE'S. `TextView`'s `Editor` draws the teardrops for itself and nobody
      else, so the engine does the whole job: `SelectionHandleGeometry` (a circle of radius 11 -
      Android's own 22dp asset - hanging UNDER the caret rect at each end of the LOCAL primary
      range, with an 8px hit slop because a finger is not a pointer), painted in
      `paint_selections` right after the highlight as two fully-rounded `SelectionRect`s in the
      `::selection` colour at full alpha; `LayoutWindow::selection_handle_geometry` computes the
      SAME rule in window coordinates for the hit test, so what is seen is what a press finds.
      DRAG: `begin_selection_handle_drag` runs BEFORE the press is treated as a click (a click
      collapses the selection, removing the handle being reached for); the anchor is the OTHER
      end in document order, so the start handle keeps the end and vice versa; crossing the
      anchor makes the range backward - the representation a backward mouse drag already uses -
      and the next paint re-labels the handles by document order; landing exactly on the anchor
      is IGNORED rather than collapsing, since a collapsed selection has no handle for the finger
      to be on. The dll routes `TextSelectionDrag` to the handle while one is held, and a press
      on a handle ARMS the drag anchor even though the handle hangs below the text (the "press on
      editable" rule would otherwise never build a `TextSelectionDrag` for it); the finger lifting
      ends it. `selection_handles` is a `TextEditManager` flag the Android shell sets at window
      creation; iOS and desktop leave it off, so nothing changes there.
      ALSO FIXED: the layout path of `layout_document` built its context with
      `owner_colors: Default::default()` - the U1 parameter was accepted and never read, so a
      peer's caret colour reached the paint only through `regenerate_display_list_for_dom` and
      every relayout painted it in the CSS colour. Found because the new flag went through the
      same eight sites. Also `dl_input_fingerprint` keys on the flag.
      EVIDENCE: 7 integration tests on a real laid-out paragraph (a caret has no handles, a range
      two, under the line, labelled by document order; the end handle drags the end and keeps
      the start; the start handle keeps the end and the labels follow document order; onto the
      anchor does not collapse; off the handles is not a handle drag; handles off means nothing
      to grab; the display list has exactly two more `SelectionRect`s with handles on and none
      for a caret). NEGATIVE CONTROL: anchoring on the wrong end fails 3 of them. Host, 8/8
      mobile (iOS path compiled on the three Apple targets). ⚠ No device: the Android handles are
      painted and dragged in the engine, not seen under a finger; the iOS interaction is compiled,
      not seen.
- [x] U2-a-i DONE. The geometry half was the easy one: `rect_for_cursor_in(block, cursor)` is the
      session seam with the block as a parameter, and `selection_ends_in_document_order` hands
      both shapes of selection over as `[(block, cursor); 2]` - the cross-block anchor/focus pair
      sorted by `is_forward`, or the single-block primary's two cursors - so the handles hang under
      each block's own line and the paint/hit rule did not change.
      THE DRAG WAS THE REAL HALF, and it needed a session move. The cross-block mouse drag
      extends from the SESSION's caret to the pointer, so the block the session sits in IS the
      anchor block - and the session sits where the press that made the selection was. Grabbing
      the handle at the session's end (the START handle of a forward selection) must keep the
      OTHER end fixed, which means the session has to move there first: `begin_selection_handle_drag`
      re-anchors it with `initialize_editing` at the far end's block and cursor (same
      contenteditable key), keeps `cross_block` painted until the first move rebuilds it, and the
      drag then runs through `process_mouse_drag_for_selection` - which resolves the pointer
      against every block and collapses back to a single-block range inside the anchor block,
      exactly as the mouse does. "Onto the anchor is ignored" is checked up front through
      `hittest_text_position_global`, because that machinery would collapse to a caret.
      EVIDENCE: 4 new integration tests on the three-paragraph fixture (a handle under each
      block's end, two lines apart; the end handle moves the far end P3 -> P2 with the start
      anchored; the start handle re-anchors the session at P3 and moves the near end P1 -> P2
      with the end kept at byte 5 and `is_forward` false; dragging back into the anchor block
      collapses to a single-block range that still has handles). NEGATIVE CONTROL: dropping the
      re-anchor fails those three drag tests. All 11 handle tests pass; host, 8/8 mobile.
- [x] U2-a-ii DONE, and the premise ("needs the drag transition surfaced to the Java side") was
      wrong: the bar is driven by a per-frame TRANSITION hook on one bool (`has_selection` vs
      `selection_toolbar_shown`), so folding the drag into the "wanted" bit is the whole change.
      `has_selection && !selection_handle_drag_active()`: the press on a handle flips it off
      (`stopSelectionToolbar`), the release flips it back on (`startSelectionToolbar`, which
      positions itself against the NEW range through `onGetContentRect`). No Java change.
      Same behaviour as Android's own fields, for the same reason - the bar floats over the
      selection being resized under the finger.
      ✅ COMPILED AND RUN in the 2026-09-03 batch pass (evidence: the pass note on 9h-i-a-i-b); the hook is a
      one-expression change on a path that is already exercised by U2.
- [x] U2-a-iii DONE, and "pointer 0" was the whole defect: Android's pointer INDEX 0 is not an
      identity. When the finger holding a handle lifted with a second finger down, the second
      became index 0 and inherited the mouse - cursor position AND the still-down button - so the
      handle jumped to it; a fresh third finger reuses the lowest free ID and would have become
      "the mouse" mid-gesture the same way. The mouse pipe now follows the PRIMARY pointer by ID
      (`primary_pointer_id`, the finger `ACTION_DOWN` starts the group with): no later finger
      inherits it, its `ACTION_POINTER_UP` is a mouse release even while others stay down (which
      is what ends the held handle through the existing `!left_down` seam), other fingers' moves
      do not move the cursor, and the next primary is born only on the next `ACTION_DOWN` - the
      W3C pointer-events rule. The one-finger PAN got its OWN active finger (`pan_pointer_id`)
      because scrolling has the opposite rule: `ScrollView.onSecondaryPointerUp` hands the pan
      to the remaining finger, re-seeded so the hand-over is not a jump. Hovers (a mouse, a
      hovering stylus) have no finger group and keep using the one pointer they carry.
      iOS had the same class one step worse - `touches` is an NSSet whose order is arbitrary
      between calls, so with two fingers moving "the first touch" alternated and the emulated
      cursor jumped finger to finger, and the button released only when the LAST finger lifted -
      and got the same rule by `UITouch` identity (`primary_touch_id` / `pan_touch_id`). The
      handles there are UIKit's own, so what this fixes on iOS is the mouse pipe itself.
      COMPILED AND RUN in the batch pass of 2026-09-03: host check EXIT=0; core 2794, layout 7671 +
      999 (`--test all`), dll 2015 tests green; the e2e corpus (57 scenarios, incl. the new
      second-seat scenario) green; the 8-target gate green after one Android fix (the pan block
      read a vector the touch-state refresh had moved - caught by the gate, not by the host).
- [x] U3 ANSWERED, and the answer turned out to need code: a selection is identified by an
      OWNER-SCOPED id, `(SelectionOwner, SelectionId)`, and the engine acts on the LOCAL owner's
      set and on nothing else. The PRIMARY is always local; the platform's idea of "the
      selection" (`selectedTextRange`, the IME's marked range, the Android selection bridge,
      copy) is the local primary; peers' selections are DISPLAY-ONLY SNAPSHOTS that enter through
      `set_owner_selections`, leave through `remove_owner`, and are not shifted by a local edit -
      the sync layer that carried the snapshot is the one that knows how the edit moved the
      peer's caret, and it replaces the snapshot. Written on `MultiCursorState`'s invariants.
      ⚠ WHAT THE QUESTION WAS HIDING. Every engine operation still walked EVERY owner's entry,
      so with one peer's caret injected: `to_selections` handed the peer's caret to `edit_text`,
      which inserts at each selection it is given - a local keystroke was TYPED AT THE PEER'S
      CARET; `update_from_edit_result` then rebuilt the whole list as LOCAL, absorbing the peer
      after one keystroke; a plain click (`set_single_cursor` / `set_single_range`) cleared the
      peer out of view; `move_all_cursors` moved the peer's caret with the local arrow keys;
      `get_primary` / `ensure_primary_valid` fell back to `selections.last()`, which on the
      owner-sorted list IS the peer whenever one exists - so losing the local primary would have
      made the peer's caret "the selection" for the IME, the platform and copy; the smart-paste
      `len()` counted the peer's caret as a line target; Ctrl+D searched on from the peer's caret
      (`last()`); Cut was armed by a peer's range; copy extracted a peer's range.
      NOW: `local_selections()` / `local_selections_mut()` / `local_len()`; `to_selections` is
      the LOCAL edit set; `update_from_edit_result` writes back to the local entries only and
      carries the peers over with owner and id intact; the collapse-to-one calls retain the
      peers; movement is local-only; the primary fallback is the last LOCAL entry, and with no
      local entry left `get_primary` answers `None` rather than a peer. Every consumer that means
      "what is the user doing" (Ctrl+D, copy, Cut arming, smart paste, `get_selection`) reads
      the local set; painting still walks everything. `focused_selection_byte_range` and
      `set_focused_selection_from_byte_range` (the iOS/Android seams) go through `get_primary`
      and the collapse calls, so they inherit all of it. This is the shape U2-a's handles talk
      to: a handle drag moves the LOCAL primary through `set_focused_selection_from_byte_range`.
      EVIDENCE: 5 core tests (typing never lands on a peer's caret and the peer survives an edit
      with id and place; a click keeps the peers in view; the primary is never a peer - `None`
      with no local left, the other local otherwise; arrows move local carets only; `local_len`
      vs `len`). NEGATIVE CONTROL: restoring the all-owners `to_selections` fails the typing
      test. The 6 U1 owner tests still pass. Host check, 8/8 mobile. ⚠ Still no two-machine
      session here; the invariants are pinned at the model, not seen in a session.
- [x] U3-a DONE per the USER RULING (2026-09-03): a peer's caret is anchored at a logical
      position and moves with the text. `RunTextChange { run, start, end, inserted }` (core) is
      the shape of one change to one run's text; `RunTextChange::between(old, new)` derives it
      as the replaced middle between the common prefix and suffix, backed off to char
      boundaries on both sides; `transform(byte)` is the ruling verbatim - a change entirely
      before the caret shifts it by the delta, one after leaves it, one spanning it collapses
      it to the change's start, and a caret AT a pure insert's position moves after the new
      text (it is attached to the character that follows). `MultiCursorState::shift_peers_across`
      applies it to every non-local selection (ranges move both ends and may collapse; local
      selections are untouched - the edit result already placed them). Both edit sites in the
      window (`apply_one_text_changeset`'s insert and `delete_selection`) feed it
      `run_text_changes(old, new)`, one change per changed text run. LIMIT, logged as U3-a-i:
      when an edit changes the run COUNT (split / merge across styled runs) there is no
      run-stable mapping and no shift is applied - a wrong shift is worse than a stale caret,
      and the app's next snapshot re-places the peer. Nine core tests pin the diff, the
      boundaries, each transform case, ranges, locality and run isolation.
      ✅ COMPILED AND RUN in the second batch pass of 2026-09-03: host check EXIT=0; api.json
      converged (autofix EXIT=0, 0 patches; the four new CallbackInfo methods and six new types
      staged through `autofix add`); codegen green; core 2807, layout lib 7679, layout `--test
      all` 999, dll 2021 tests green; the e2e corpus (62 scenarios) green with the new
      spatial-navigation scenario RED under its negative control; the 8-target gate 8/8 after one
      iOS fix (objc2 `error: _` wants a typed NSError; explicit out-pointers now); the Android
      Java classes compile against android-34.
- [ ] U3-a-i Peer shift across an edit that changes the run COUNT: a delete spanning two
      styled runs merges them, a styled paste splits one; `run_text_changes` answers nothing
      then, so peers on the affected runs stay where they were until the next snapshot. Needs
      a run-mapping (old run -> new run + byte base) computed alongside the edit, which
      `edit_text_outcome` knows and currently discards.
- [x] U3-b DONE (the user's question, 2026-09-03: "isn't this already the case where we
      preserve the caret position ... across layout() RefreshDom events?" - it was not: the
      engine preserved the caret's NODE across a refresh and kept its (run, byte) verbatim, so
      text the app changed under it left every caret stale). Now the layout funnel
      (`layout_and_generate_display_list`) keeps `caret_text_snapshot` = the session node's
      RESOLVED text (overlay first, DOM second - exactly what is shown) keyed by the session's
      `contenteditable_key`, and at the end of every pass, AFTER the convergence GC (only then
      does the node resolve to what will be shown), diffs it against the new generation:
      a difference is text the APP changed (a remote participant's edit applied through its
      model, an app-side rewrite, an acked local edit merged with a remote one) and
      `MultiCursorState::shift_all_across` moves the local carets and the peers by the same
      `RunTextChange` transform as U3-a. Engine edits refresh the snapshot at their chokepoint
      (`update_text_cache_after_edit`), so a keystroke never registers as an app change and is
      never applied twice; a session on a new node is snapshotted, never diffed against another
      node's text; no session clears it. Independent of remap timing (the runner remaps before
      the funnel, the shells hand-roll their diff) because identity is the session key, not the
      node id. Same run-count limit as U3-a (U3-a-i). Tests: core `shift_all_moves_the_local_
      caret_as_well`; layout `a_generation_that_changes_the_text_shifts_every_caret` (snapshot
      pass, remote insert shifts local + peer, unchanged pass moves nothing, a delete spanning
      the caret collapses it). ✅ COMPILED AND RUN in the second batch pass of 2026-09-03: host check EXIT=0; api.json
      converged (autofix EXIT=0, 0 patches; the four new CallbackInfo methods and six new types
      staged through `autofix add`); codegen green; core 2807, layout lib 7679, layout `--test
      all` 999, dll 2021 tests green; the e2e corpus (62 scenarios) green with the new
      spatial-navigation scenario RED under its negative control; the 8-target gate 8/8 after one
      iOS fix (objc2 `error: _` wants a typed NSError; explicit out-pointers now); the Android
      Java classes compile against android-34.

### Follow-ups opened by 9a

- [x] 9a-ii DONE. `DefaultAction` gains `FocusUp`/`FocusDown`/`FocusLeft`/`FocusRight`, mapped
      by `default_action_to_focus_target` onto `FocusTarget::Directional(..)`.
      NOTE: `DefaultAction` is NOT an api.json type (only `FocusTarget` and `FocusDirection`
      are), so this needed no autofix pass — checked before touching it.
- [x] 9a-i DONE for the half that had no conflict: the GAMEPAD D-PAD now drives spatial focus.
      `resolve_focus_target` has handled `Directional` since it was written (`next_in_direction`
      in focus_cursor.rs) — the gap was that no `DefaultAction` could ask for it, so a resolver
      with a real implementation sat unreachable behind a missing 4-variant enum arm.
      `determine_gamepad_default_action` maps the four D-pad bits; the dll seam resolves and
      applies it exactly the way Tab does, including "nothing in that direction is a MISS, not a
      clear" so walking into a wall does not drop focus.
      THE HARD PART WAS THE EDGE, NOT THE MAPPING: a pad is polled at ~60 Hz and reports a HELD
      button in every snapshot, so a consumer reading `buttons` cannot tell a fresh press from a
      held one and focus would run away across the UI while the D-pad is down. Only
      `GamepadManager::set_state` sees both the old and new bitset, so the edge is computed there
      into `pending_pressed` and drained by `take_pending_pressed()` — the same shape as the
      existing `pending_hotplug`, and separate from `pending_event` for the same reason (that
      one coalesces). The mask is drained UNCONDITIONALLY, not only under `!prevent_default`,
      or a press vetoed in one pass would fire in the next.
      Face buttons deliberately NOT bound: "A activates" is a platform convention (it is B on
      Nintendo layouts), not a fact to hardcode. Diagonals resolve to one direction rather than
      to nothing, vertical first — answering `None` would make the D-pad feel dead exactly when
      the thumb is moving fastest.
      EVIDENCE: `the_dpad_drives_spatial_focus_and_the_rest_of_the_pad_does_not`,
      `a_diagonal_dpad_press_resolves_to_one_direction`,
      `pending_pressed_is_an_edge_and_a_held_button_does_not_repeat` (5 polls of a held button
      produce no further edges), `pending_pressed_accumulates_until_drained`, plus the existing
      completeness guards updated to cover the 4 new actions (`mapping_is_injective_over_the_
      focus_actions` now asserts 9). Host check green, 8-target gate green, azul-core 2741,
      azul-layout 7519 passed with its 20 pre-existing failures unchanged.
- [x] 9a-i-a DONE — arrows do spatial navigation, with the W3C ORDERED FALLBACK the user ruled
      for. This closes the item that blocked 9a-i from the start.
      The block was never plumbing: taking the arrows outright breaks every scroll container,
      and leaving them on scroll means a keyboard user deep in one part of a UI cannot reach a
      visually adjacent control without tabbing through everything between — the original
      complaint. CSS Spatial Navigation Level 1 dissolves it by making the arrow try things IN
      ORDER: look for a focusable in that direction, and only if there is none does it scroll.
      IMPLEMENTED IN THE DECISION FUNCTION, not the dll seam: the fallback is then one decision
      in one place and testable without a window. The seam would have had to UNDO an
      already-dispatched focus action in order to scroll instead.
      Uniform for the D-pad too — `FocusUp`/`Down`/`Left`/`Right` now mean "spatially navigate",
      whoever asked.
      TWO GUARDS kept: a focused TEXT INPUT still claims the arrows for caret movement (as
      browsers do), and spatial navigation is only attempted from a LIVE anchor. That second one
      is load-bearing: `resolve_focus_target` is lenient — handed a focus naming no live node it
      falls back to a first-focusable, which would have turned every arrow into a focus jump.
      The existing arrow test caught exactly that.
      EVIDENCE: `an_arrow_moves_focus_when_something_is_there_to_focus` pins the focus half, and
      the pre-existing `arrow_keys_map_to_their_own_direction_and_scroll_by_line` was rewritten
      to pin the FALLBACK half (it encoded "arrows always scroll", the pre-ruling behaviour).
      azul-layout 7557, azul-dll 1944, azul-core 2750, host check and 8-target gate green.
- [x] 9a-i-b DONE. Both are real CSS properties end to end: type, parser, name table, cascade,
      prop cache, solver getter, api.json and a resolver that honours them.
      `spatial-navigation-action` is read off the nearest SCROLL CONTAINER at or above the
      focused node - not off the focused node itself - because the property answers "what does an
      arrow do when THIS element is the container being navigated", and the element an arrow
      would scroll is the one `ScrollFocusedContainer` acts on. "Scroll container" here is the
      layout's own answer (`scrollbar_info` present) rather than "does it overflow right now":
      the stricter test needs the scroll manager, which the decision function deliberately does
      not have, and an author who wrote `scroll` meant it either way.
      `Scroll` answers BEFORE the spatial search runs, so a map or a canvas pays nothing for a
      search whose result it would discard. `Focus` means nothing found = NOTHING HAPPENS, not a
      scroll; the spec's "continue outward" is already covered because `next_in_direction`
      searches the whole candidate pool rather than one container.
      THE DECISION IS A PURE FUNCTION (`resolve_arrow_action`) and that is deliberate: the
      alternative - asserting through `determine_keyboard_default_action` - needs a fixture with
      a real layout tree carrying `scrollbar_info`, which no test in that file has. Same move as
      `sensors/units.rs` and `PadAccumulator`: put the logic where it can be tested.
      `spatial-navigation-contain` narrows the candidate pool to one subtree FIRST and widens
      back to the whole document when nothing inside answers - the spec's move-to-the-parent-
      container step, and what stops an arrow at the edge of a panel from dying there.
      ⚠ `auto` DELIBERATELY DOES NOT make scroll containers into containers, although the spec
      says it should. Honouring that would silently change what every existing arrow key does -
      navigation would become confined to whatever scroll box the focus happens to sit in, which
      is not what 9a-i-a shipped and not something any stylesheet asked for. Logged as 9a-i-b-i
      rather than done quietly.
      Both are classified `RelayoutScope::None` and not-relayout: they describe what an ARROW KEY
      does and move no box and paint no pixel. Both defaults are the WRONG way round for a new
      property - unlisted means `can_trigger_relayout() == true` and `RelayoutScope::Full` - so
      being unlisted would have charged a full layout pass per declaration.
      TWO FFI TRAPS, both invisible to `azul-doc check`: the owned parse errors needed
      `#[repr(C, u8)]` (a payload enum with no repr compiles silently and is undefined across the
      boundary) and `AzString` rather than `String` (the codegen builds its mirror from
      `AzString`; with `String` the generated C ABI failed to compile with a size-mismatch
      transmute). api.json's own checker was green through both. Caught only by `codegen all` +
      a real dll build, exactly as the autofix memory says.
      EVIDENCE: 5 parser tests including an END-TO-END one that goes through the real
      `CssPropertyType::from_str` + `parse_css_property` (a keyword parser that works in
      isolation proves nothing about the name table, which lives in a different file), the full
      6-case action truth table, 3 containment tests, and a NEGATIVE CONTROL - removing the
      containment filter fails with "a contained search must land inside the panel". autofix
      converged at 0 patches, `azul-doc check` PASSED, `codegen all` + dll build green. Host,
      8/8 mobile, azul-css 2865, azul-core 2767, azul-layout 7610, azul-dll 1990.
- [x] 9a-i-b-i DONE per the USER RULING (2026-09-03): `spatial-navigation-contain: auto` (the
      initial value) now makes every SCROLL CONTAINER (`overflow` other than `visible`/`clip`
      on either axis - css-overflow-3's definition, read off the cascade so it holds without a
      layout) a spatial navigation container, exactly as `css-nav-1` says. The hold's own
      condition is met: the resolver now searches the full container CHAIN -
      `spatial_navigation_containers` lists every container on the way up, innermost first,
      the `Directional` arm tries each in turn and the whole document last - so a candidate
      inside the innermost box wins over a nearer one outside, and an arrow at the edge of a box
      still escapes outward instead of dying there. Also fixed a doc comment that had been
      glued to the wrong function. Tests: `auto_makes_a_container_of_a_scroll_container_only`
      (auto / scroll / hidden yes, visible no) and
      `the_container_chain_runs_innermost_first_to_the_outermost`; and the "real visual test"
      the hold asked for: `e2e/spatial-nav-auto-scroll-container.json` - #f1 top-left inside an
      `overflow: auto` box, #f2 far bottom-right inside it, #out directly below the box and
      nearer; ArrowDown must pick #f2 (inside wins), and from #f2 must escape to #out.
      ✅ COMPILED AND RUN in the second batch pass of 2026-09-03: host check EXIT=0; api.json
      converged (autofix EXIT=0, 0 patches; the four new CallbackInfo methods and six new types
      staged through `autofix add`); codegen green; core 2807, layout lib 7679, layout `--test
      all` 999, dll 2021 tests green; the e2e corpus (62 scenarios) green with the new
      spatial-navigation scenario RED under its negative control; the 8-target gate 8/8 after one
      iOS fix (objc2 `error: _` wants a typed NSError; explicit out-pointers now); the Android
      Java classes compile against android-34. END-PASS CHECK REQUIRED: run the scenario
      with the `auto` arm stubbed to `false` and confirm it goes RED (a scenario that cannot
      fail proves nothing - see the harness lesson).
- [x] 8f-i BATTERY DONE on every platform that has a gamepad backend; the IMU/touchpad half is
      8f-i-a. `GamepadState::battery` was modelled, documented, and filled by NOBODY - it read as
      its `-1.0` "not reported" default everywhere, which the note called "honest but inert".
      DESKTOP (gilrs, so Windows + Linux + macOS at once): `Gamepad::power_info()` mapped through
      a `power_info_to_battery` helper.
      APPLE (iOS/tvOS/macOS GameController): `GCDeviceBattery` via `respondsToSelector:`, the same
      probe every other optional control on that pad already uses - the selector is the thing that
      must exist, and an older SDK just answers false, which is more robust than a version check.
      That also RETIRES apple.rs's own reason for abstaining: its comment said filling battery
      there alone "would make iOS the odd one out". With desktop filled in the same change, it no
      longer would.
      Two mapping decisions, both about the SENTINEL rather than about numbers:
        - `Wired` -> `-1.0`, NOT `1.0`. A wired pad has no cell, so reporting it as full makes
          "plugged in" and "fully charged" indistinguishable and draws a battery icon for a
          controller that has none. The field's own docs say wired pads report `-1.0`.
        - `Charging(pct)` -> the LEVEL, not the sentinel. The level is real and known while
          charging, and an app dimming a low-battery warning during a charge needs the number.
      EVIDENCE: 6 tests over the mapping, which is the part that can be silently wrong - unknown
      and wired both hit the sentinel exactly (a `0.0` would be indistinguishable from a flat
      battery), charging does NOT collapse to the sentinel, and a bad driver percentage cannot
      escape the sentinel-or-`[0,1]` contract every consumer trusts. Host, iOS, 8/8 mobile,
      azul-dll 1969 (+6).

- [x] 8f-i-a DONE on APPLE and ANDROID; the desktop half is 8f-i-a-i and is no longer a
      hand-wave. The note said these "need SDL or raw HID on desktop and `GCMotion` on Apple",
      which was right about Apple and understated the rest: Android has `InputDevice
      .getSensorManager()` (API 31), a `SensorManager` SCOPED TO ONE CONTROLLER, which is exactly
      the distinction `GamepadState::gyro_*` exists for - a game that aims with the pad must not
      read the phone.
      APPLE, two traps and neither is a property read:
        - THE SENSORS ARE OFF UNTIL ASKED. A DualSense reports
          `sensorsRequireManualActivation`, and until `sensorsActive` is set every read returns
          zeroes for the life of the process - INDISTINGUISHABLE from a pad with no gyro, which
          is how this would have shipped looking done while doing nothing.
        - THE VECTORS ARE STRUCT RETURNS. 24 bytes of three doubles comes back in registers on
          arm64 (an HFA) but through a hidden pointer on x86_64 (`objc_msgSend_stret`), and the
          device target is arm64 while the SIMULATOR is x86_64 - both are live. That is why those
          two reads go through `objc2`, which picks the variant from the type's encoding, while
          the rest of the file stays on the `objc` 0.2 calls around it. Both targets were
          compiled, not just the one.
      `acceleration` is iOS 14+; older SDKs get `gravity + userAcceleration`, which RECONSTRUCTS
      it exactly rather than approximating it. `touchpadPrimary` exists only on the DualShock and
      DualSense profiles, so the selector probe is also the "has a touchpad" test; `touchState` is
      the honest active flag but is declared on a superclass that may not carry it, so it is
      probed and the fallback (either axis non-zero) is documented as unable to tell "no finger"
      from "a finger exactly centred".
      ANDROID NEEDED A BUG FIXED FIRST, and it was a real one nobody had noticed:
      `GamepadManager::set_state` REPLACES a pad's slot, and every Android entry point published
      a partial state with `..Default::default()`. So pressing a button ZEROED THE STICKS and
      moving a stick RELEASED EVERY HELD BUTTON. Android is the only backend where this could
      happen, because it is the only push-driven one - the polled backends build a complete
      snapshot by construction. Adding a third partial producer (the IMU) would have made it
      worse, so the per-pad union is now accumulated and the full snapshot republished.
      THE ACCUMULATOR LIVES IN THE SHARED `mod.rs`, NOT IN `android.rs`, for the same reason
      `sensors/units.rs` does: that file is cfg-gated to a target this machine never runs tests
      on, and the accumulation is precisely the logic that fails silently.
      The Android sensor wire code IS Android's own `Sensor.getType()`, passed through unchanged,
      so unlike the `SensorKind` codes there is no second numbering that could drift; any type
      other than the two is ignored rather than mapped, so a pad that also reports a magnetometer
      cannot land in the gyro fields.
      ALSO documented the touchpad ORIGIN in the field itself (bottom-left, y up): it was
      unstated, the hardware disagrees with itself (a DualShock's raw HID counts y downward while
      GameController normalizes it upward), and this is the first producer.
      EVIDENCE: 4 accumulator tests that RUN HERE with a NEGATIVE CONTROL - restoring the
      fresh-default-per-event behaviour fails with "a button press zeroed a stick". Both Apple
      seams proven COMPILED on aarch64-apple-ios AND x86_64-apple-ios (the struct-return ABI
      differs between them), the Android seam under `_internal_deps`, and the JAVA COMPILED
      against android-34 with `javac` - nothing in the Rust build validates
      `InputDevice.getSensorManager()` or the API-31 guard. Host, 8/8 mobile, azul-core 2767,
      azul-dll 1984, autofix still 0 patches and `azul-doc check` PASSED. No controller with an
      IMU here - compile-only.
- [x] 8f-i-a-i STEPS (1) AND (2) of the USER RULING (2026-09-03) DONE; the parser (3) and the
      gilrs correlation (4) are logged below as their own items, because each is a real piece.
      (1) `HidDevice` carries `serial` (the device's own: USB `iSerial`, a DualSense's Bluetooth
      address) and `instance: u64`, a per-instance identity that is never 0 for a real device -
      `HidDevice::serial_instance` (FNV-1a over vendor, product, serial; reconnect-stable) when
      a serial is reported, `handle_instance` over the platform's own handle otherwise. Linux
      reads `HIDIOCGRAWUNIQ` (5.13+, empty on older kernels) and falls back to the hidraw path;
      macOS reads `kIOHIDSerialNumberKey` and falls back to the `IOHIDDeviceRef`; Windows hashes
      the raw-input device path, which Windows builds from the USB serial when the device has one
      (so twins stay apart across reconnects) - the serial STRING there needs hid.dll (8f-i-a-i-a).
      Every `HidReport` carries the device, so each pad's stream is keyed apart from its twin's.
      (2) The "single consumer" worry was about the static queue, and it is answered by the
      manager: the capability pump is the queue's ONE drainer, and `HidManager::reports()` holds
      the pass's reports for every reader (`get_hid_reports` copies) - the pad parser of step 3
      reads the same slice after the fold and steals nothing. FOUND AND FIXED on the way:
      nothing ever cleared that buffer (`take_reports` had no caller), so `get_hid_reports`
      answered the whole process history and the buffer grew without bound; the pump now clears
      it at the top of each fold, so a callback sees this pass's reports. Two core tests pin
      distinct-twins, stability, the non-zero rule and serial-over-handle. ✅ COMPILED AND RUN in the third batch pass of 2026-09-03: host check EXIT=0; api.json
      converged (`HidDevice` + `serial`, `instance`); codegen green; core 2809, layout 7679, dll 2030
      tests green (the seven decoder tests and the two GUID tests among them); the 8-target gate
      8/8. One compile error, in 8f-i-a-i-c: the gilrs fork keeps `devpath()` private - see that
      entry.
      uncompiled until its end pass (api.json: `HidDevice` gains `serial` and `instance`).
- [x] 8f-i-a-i-a DONE, BLIND per the ruling. `extra/hid/windows.rs::hid_dll` loads hid.dll on
      first use (`LoadLibraryW` + `GetProcAddress`, the two `HidD_*` entry points), `describe`
      opens the raw-input device NAME - which IS the HID interface path - with `CreateFileW`
      (access 0, shared, hidapi's enumeration mode, so an exclusively-held keyboard still opens),
      reads `HidD_GetSerialNumberString` into 256 UTF-16 units and closes the handle at once. The
      identity is now `instance_for(vid, pid, serial, path)` - serial-keyed like Linux and macOS,
      so it pairs with gilrs' serial (8f-i-a-i-c), the path hash only as the fallback. The same
      module carries `feature_report` (`HidD_GetFeature`) for 8f-i-a-i-b-i. Cargo: winapi
      `fileapi` + `handleapi`. NOT RUN on Windows: the cross-compile at the eighth batch's end
      pass (13d-windows) is the check available here; a real pad settles the rest. ✅ COMPILED in the eighth batch pass of 2026-09-04: host check EXIT=0; autofix converged at 0 patches / 0 FFI errors after two source fixes (`natural_scroll` moved beside `log_level` so the alignment checker sees no padding; the runner and the seat-focus arm gained the missing match arms); `codegen all` EXIT=0; azul-core 2809, azul-layout lib 7683 / `--test all` 1003, e2e corpus 62 scenarios, azul-dll 2040 (+5), all 0 failed; 8/8 gate targets green including windows-gnu, which compiles the hid.dll and registry code.
- [x] 8f-i-a-i-b DONE. `extra/gamepad/playstation.rs`: a pure, platform-free decoder for the
      DualSense (0x0ce6, Edge 0x0df2) and DualShock 4 (0x05c4, 0x09cc, dongle 0x0ba0) input
      reports, layouts checked against the kernel's `hid-playstation` before writing - USB 0x01
      (struct at 1), Bluetooth 0x31 / 0x11 (struct at 2 / 3, IEEE CRC32 over the rest seeded
      with 0xA1, verified and a bad CRC dropped); gyro raw/1024 deg/s -> rad/s, accel raw/8192 g
      -> m/s², sticks -1..1 with y UP, triggers 0..1, the first finger normalized on the 1920x1080
      (DS4: 942) surface with the bottom-left origin `GamepadState` specifies; all sixteen
      buttons plus touchpad-click and mute mapped onto `GamepadButton`. Seven unit tests with
      synthetic reports cover both pads, both transports, the CRC (reference vector
      0xcbf43926) and the rejects. Publishing: the pump ingests each pass's reports off
      `HidManager::reports()` (nothing stolen from `get_hid_reports`) into a per-instance
      last-sample map; the gilrs poll lays that motion over its own pad state as it builds it -
      ONE writer per slot, so an idle pad raises no event and a pass without a fresh report
      keeps the last motion instead of snapping to zero; pads with no gilrs twin (Windows,
      where gilrs is XInput and never sees a DualSense) or several identical twins are their
      own complete devices under `HID_PAD_ID_FLAG | instance`. ✅ COMPILED AND RUN in the third batch pass of 2026-09-03: host check EXIT=0; api.json
      converged (`HidDevice` + `serial`, `instance`); codegen green; core 2809, layout 7679, dll 2030
      tests green (the seven decoder tests and the two GUID tests among them); the 8-target gate
      8/8. One compile error, in 8f-i-a-i-c: the gilrs fork keeps `devpath()` private - see that
      entry.
      its end pass.
- [x] 8f-i-a-i-b-i DONE, BLIND per the ruling. The raw HID layer can now ASK: `extra/hid::
      feature_report(device, id, len)` - Linux `HIDIOCGFEATURE` on the open hidraw fd (a GET
      needs no write mode), macOS `IOHIDDeviceGetReport(kIOHIDReportTypeFeature)` on the matched
      `IOHIDDeviceRef`, Windows `HidD_GetFeature` on a handle opened for the call (8f-i-a-i-a's
      hid.dll). The decoder gained `Transport` (off the input report id; the DS4 dongle is USB),
      `calibration_report(pad, transport)` (DualSense 0x05/41; DS4 0x02/37 USB, 0x05/41 BT),
      `parse_calibration` in the kernel's `ps_calibration_data` layout (bias at 1/3/5; the six
      gyro plus/minus - `+ - + - + -` on the DualSense and DS4-USB but `+ + + - - -` on DS4-BT;
      speed at 19/21; accel plus/minus at 23..33; BT reports CRC-tailed with the FEATURE seed
      0xA3), and `parse_with(pad, bytes, calibration)` applying `(raw - bias) * numer / denom` in
      raw units before the nominal 1024 / 8192 conversion; `parse` stays nominal. `gamepad/mod.rs`
      reads each pad's report ONCE per instance on first sight (`PS_CALIBRATIONS`, one retry for
      the DS4-BT first-answer-garbage habit, a zero-range answer rejected rather than applied),
      forgets a pad that vanishes. Tests: scale + bias applied and nominal untouched, the BT CRC
      gate, the zero-range rejection, the DS4-BT interleave. NOT RUN on a pad - the layouts are
      from the kernel source (hid-playstation.c), the user's ruling; a real pad settles them
      (8f-i-a-i-b-i-a). ✅ COMPILED in the eighth batch pass of 2026-09-04: host check EXIT=0; autofix converged at 0 patches / 0 FFI errors after two source fixes (`natural_scroll` moved beside `log_level` so the alignment checker sees no padding; the runner and the seat-focus arm gained the missing match arms); `codegen all` EXIT=0; azul-core 2809, azul-layout lib 7683 / `--test all` 1003, e2e corpus 62 scenarios, azul-dll 2040 (+5), all 0 failed; 8/8 gate targets green including windows-gnu, which compiles the hid.dll and registry code.
- [ ] 8f-i-a-i-b-i-a Device check: a DualSense over USB and BT, a DualShock 4 over USB, BT and
      the dongle - the calibration report answers, the CRC seed, the DS4-BT interleave, and that
      a calibrated 1 g at rest reads within a percent.
- [x] 8f-i-a-i-c DONE, without the fork change the previous note asked for: `Gamepad::devpath()`
      is a method of the `LinuxGamepadExt` EXTENSION TRAIT, which gilrs-azul re-exports at its
      root - the batch-pass error ("no method named devpath") was the trait not being in scope,
      not a private method. `pad_serial` now brings the trait in on Linux and reads evdev's `uniq`
      through sysfs beside the pad's event node (`sysfs_uniq_path`, tested) - the exact string
      hidraw answers `HIDIOCGRAWUNIQ` with, both from `hdev->uniq` - so a table of identical
      DualSenses pairs each gilrs pad with its own HID stream by serial and publishes no duplicate.
      `overlay_hid_motion` pairs by serial first and by the unique vendor/product rule second;
      vendor and product come from gilrs's own `vendor_id()` / `product_id()` with the SDL GUID as
      the fallback. macOS has no gilrs-side serial (8f-i-a-i-c-ii); Windows never sees a DualSense
      through XInput, so the HID device IS the pad there. Verified: the Linux target checks green.
- [x] 8f-i-a-i-c-i WITHDRAWN - no fork change needed: `devpath()` lives on the exported
      `LinuxGamepadExt` trait (see 8f-i-a-i-c). The user had cleared forking and publishing a
      gilrs-azul release for it; not spent.
- [ ] 8f-i-a-i-c-ii macOS: pairing several identical pads needs a gilrs-side serial. gilrs's IOKit
      backend exposes only the SDL GUID; the IOHIDDeviceRef it holds is the same object the raw
      HID layer enumerated (`kIOHIDSerialNumberKey` is readable from it), but gilrs does not hand
      the ref out. A fork-side accessor (`gilrs-azul` is already a fork) would close it.
- [ ] 8f-i-a-ii The pad TOUCHPAD on Android, which the platform does not expose: Android turns a
      DualShock touch surface into an on-screen MOUSE POINTER rather than reporting the surface,
      so there is nothing to read. Filling it would mean claiming the pointer is a finger, which
      is wrong for a pad used alongside a real mouse.
      USER RULING 2026-09-04 (the hardware / platform group): "just implement blindly and we cross-compile
      at the end. Real verification will come with time."
- [x] 8e-i ANDROID DONE (all eleven kinds, `HingeAngle` included); apple/linux/windows are 8e-i-a.
      The note's premise was STALE in the part that mattered: it said the work was blocked behind
      a Java helper, but `scripts/android/AzulSensors.java` had already SHIPPED - the module's own
      doc comment still read "Pending (non-Rust): the `AzulSensors.java` helper ... Until it ships,
      `find_class` fails and `start` is a no-op". Both halves were simply narrow: the Java switch
      registered 3 sensors and `map_kind` mapped 3 codes.
      All eight remaining kinds map onto the EXISTING `(kind, x, y, z)` JNI signature with no
      protocol change, because `SensorKind`'s own docs already specify each one's slot - the fused
      triples use x/y/z, and light/proximity/pressure/step-count/hinge-angle put their single
      value in `x` (which the Java side's `0f` defaults for absent `values[1]/[2]` already
      produce). So this was two additive lists, not a new transport.
      `TYPE_HINGE_ANGLE` is API 30 and needs NO version guard: it compiles against the android-34
      SDK the build already uses, and on an older device `getDefaultSensor` returns null and
      `register()` no-ops - the same path as a device that simply has no hinge.
      THE REAL RISK is not the mapping but its DRIFT. The wire codes are `SensorKind`'s
      discriminants, and that contract lives in two files that cannot see each other. Reordering
      the enum renumbers both silently: nothing fails to compile, and a barometer's hPa starts
      arriving as a step count in the wrong unit with nothing downstream able to notice. Pinned by
      `the_sensor_kind_discriminants_are_the_jni_wire_codes` in azul-core, which runs on the HOST
      (the Android `map_kind` is cfg-gated and cannot be tested here) and says in its failure
      message that the fix is to APPEND, not to renumber.
      EVIDENCE: the Java was COMPILED, not eyeballed - `javac -classpath android-34/android.jar`
      exits 0, which is what proves `Sensor.TYPE_HINGE_ANGLE` and the rest exist at that SDK
      level. Android target compiles with `_internal_deps` (the gate does not enable `jni`).
      Host, 8/8 mobile, azul-core 2760 (+1).

- [x] 8e-i-a DONE on all three. THE NOTE'S PREMISE WAS HALF WRONG in the part that decided the
      priority: "Linux and Windows have no fused-sensor concept at all outside of tablets" is true
      about the HARDWARE and false about the APIs. iio has had dedicated `in_gravity_*`,
      `in_accel_linear_*` and `in_rot_quaternion_raw` channels for years, and
      `Windows.Devices.Sensors` ships a full fused set. A machine without the hardware simply has
      no such file and no such default - which is the same no-op the raw three already got. So
      this was three more cases per backend, not a new concept.
      APPLE: `CMDeviceMotion` joins the existing PULL API, so the fused three are one extra
      `deviceMotion()` read per frame with no new plumbing - `gravity`, `userAcceleration` and
      `attitude.quaternion`. Both platforms.
      LINUX: the five scalar channels plus the three fused ones, with the channel names taken
      FROM THE KERNEL'S OWN ABI DOCUMENT rather than from memory, because they are spelled
      inconsistently enough that guessing produces files that never exist - which is
      indistinguishable from a machine without the sensor. `in_gravity_x_raw` but
      `in_accel_linear_x_raw` (a MODIFIER on the accelerometer, not its own channel);
      `in_steps_input` with no `_raw` sibling; `in_angl_raw` for the hinge.
      `_input` is tried BEFORE `_raw`: both spellings exist, drivers ship one or the other, and
      reading `_raw` when an `_input` is present would apply the scale to an already-converted
      value. The offset is applied BEFORE the scale, per the ABI's own formula - it carries a
      barometer's calibration.
      THE QUATERNION IS ONE FILE HOLDING FOUR NUMBERS (the driver implements `read_raw_multi`),
      so a plain `str::parse` fails on it and reads as "no rotation sensor". That is what
      `parse_multi_value` exists for.
      WINDOWS: `OrientationSensor` for the quaternion, and - the part that is easy to miss -
      gravity and linear acceleration are the SAME `Accelerometer` class opened with a different
      `AccelerometerReadingType`, not separate classes. Plus `LightSensor` and `Barometer`.
      TWO WINDOWS SENSORS HAVE NO `GetDefault()` AT ALL, only `GetDefaultAsync()`, and blocking
      the layout thread on a WinRT async call once per frame is not an option. Both are resolved
      ONCE on a background thread; after that `Pedometer::GetCurrentReadings()` is synchronous and
      joins the poll, while `HingeAngleSensor` has no synchronous read and is driven by its
      `ReadingChanged` event - which suits it, since a hinge angle changes when someone folds the
      machine and not at 60 Hz. The pedometer's total is the SUM over step kinds: a device that
      distinguishes walking from running reports two counters and neither alone is "steps".
      iOS ALSO GETS the two push-only sensors. `CMAltimeter` and `CMPedometer` have no pull API,
      so they take handler blocks; they are gated to iOS even though both classes turned out to be
      PRESENT on this macOS (checked with `objc_getClass`, not assumed) - a Mac has neither
      sensor, and linking a class the docs mark iOS-only would make an older macOS fail to LAUNCH
      rather than quietly report nothing. NOTE the iOS step counter counts from APP START, not
      from boot as Android's does: iOS counts from a date you give it. Still monotonic, which is
      what `SensorKind::StepCounter` actually asks for.
      THE UNITS ARE THE REAL HAZARD and they disagree per platform: iio pressure is KILOpascals
      and WinRT's is already hectopascals; iio proximity is METRES and WinRT's is MILLIMETRES;
      iio angles are RADIANS and both Android and WinRT are degrees. A missed factor of ten
      produces a number that still looks like a reading and nothing downstream can tell. So every
      conversion moved into `sensors/units.rs`, which is compiled on EVERY platform - the
      backends are cfg-gated and their tests would never run on this host.
      EVIDENCE: 5 unit tests that RUN HERE, with a NEGATIVE CONTROL - flattening the pressure and
      angle conversions to identities makes exactly those two FAIL on asserts. All seven new
      seams proven COMPILED by deliberate type errors, on their own targets: iio scalar + iio
      quaternion under linux-gnu, three WinRT seams under windows-gnu, the iOS push-only path
      under aarch64-apple-ios, and device motion on BOTH ios and the macOS host. Host, 8/8
      mobile, azul-dll 1980. No device with any of these sensors here - compile-only.
- [x] 8e-i-a-i DONE per the USER RULING (2026-09-03): proximity is a typed answer now -
      `Proximity { Near, Far, Distance(ProximityDistance { value, unit }) }` with
      `DistanceUnit { Millimeters, Centimeters, Meters }` (the sensor's NATIVE unit, kept so no
      precision is invented; `in_millimeters/centimeters/meters` convert). `Proximity::is_near`
      answers `Some` only for the binary variants: a distance is a measurement, not a verdict.
      `SensorManager.proximity` + `CallbackInfo::get_proximity()` (latest-wins channel
      `push_proximity` / `take_proximity`, drained in the capability pump; a change raises
      `SensorChanged`). iOS: `UIDevice.proximityMonitoringEnabled` on start, `proximityState`
      polled and published on change as `Near` / `Far` - the boolean that blocked this item is
      the whole truth in the enum. `AmbientLight` on Apple stays unfilled: no public API (the
      camera-exposure route is not a sensor). The raw `SensorKind::Proximity` reading keeps
      the centimetres where a platform reports them. Core tests pin the conversions and the
      verdict rule. Landed in two commits (fe6e5876f carried only the core half: the edit
      script stopped on a stale anchor after the shell had already staged the commit - a
      `&&`-less chain, the same trap class as the exit-code one; the follow-up carries the
      rest). ✅ COMPILED AND RUN in the second batch pass of 2026-09-03: host check EXIT=0; api.json
      converged (autofix EXIT=0, 0 patches; the four new CallbackInfo methods and six new types
      staged through `autofix add`); codegen green; core 2807, layout lib 7679, layout `--test
      all` 999, dll 2021 tests green; the e2e corpus (62 scenarios) green with the new
      spatial-navigation scenario RED under its negative control; the 8-target gate 8/8 after one
      iOS fix (objc2 `error: _` wants a typed NSError; explicit out-pointers now); the Android
      Java classes compile against android-34.
      `ProximityDistance`, `DistanceUnit`, `OptionProximity` and `CallbackInfo.get_proximity`
      through `autofix add`.
- [x] 8e-i-a-ii DONE per the USER RULING (2026-09-03). The two stacked reasons are both
      answered: `Devices_Enumeration` is a cargo feature now, `ProximitySensor` is found by
      `DeviceInformation::FindAllAsyncAqsFilter(GetDeviceSelector())` on the same
      CoInitialize'd worker the pedometer's async resolve uses and opened by the synchronous
      `FromId`; and the optional `DistanceInMillimeters` maps exactly onto the enum - present
      (a ranging sensor) = `Distance` in millimetres, absent = `Near` / `Far` from `IsDetected`.
      Polled each pass BEFORE the IMU gate, since a machine can have one without the other.
      Android got the same typed answer from its own rule (max range = far, zero = near, in
      between = a distance in cm, via a new `nativeOnProximity(distance, maxRange)` JNI hook),
      Linux iio publishes its scaled metres as a `Distance`. ✅ COMPILED AND RUN in the second batch pass of 2026-09-03: host check EXIT=0; api.json
      converged (autofix EXIT=0, 0 patches; the four new CallbackInfo methods and six new types
      staged through `autofix add`); codegen green; core 2807, layout lib 7679, layout `--test
      all` 999, dll 2021 tests green; the e2e corpus (62 scenarios) green with the new
      spatial-navigation scenario RED under its negative control; the 8-target gate 8/8 after one
      iOS fix (objc2 `error: _` wants a typed NSError; explicit out-pointers now); the Android
      Java classes compile against android-34.
      its end pass (Windows-gnu target in the gate; the Java class recompiles then).

### Follow-ups opened by 8c/8d

- [x] 8d-i `WacomPadState.dial_delta` is modelled but has NO producer: `get_tablet_pad_dial_v2_interface()`
      exists (from #450) yet no `zwp_tablet_pad_dial_v2` listener is ever registered, and the pad-group
      listener has no `dial` member. Bind it the way ring/strip are bound. This is also the field a future
      `DialState` (item 9c) should read, so do 8d-i first.
- [x] 8d-ii DONE. `WacomPadState` -> `TabletPadState` and `get_wacom_pad` -> `get_tablet_pad`,
      through `autofix` only. NO deprecation alias — USER RULING: a 0.x API takes the clean
      break rather than carrying a deprecation notice. (The alias would not have preserved the C
      symbol anyway: FFI names are generated from api.json, so `AzWacomPadState` becomes
      `AzTabletPadState` for every binding regardless.)
      USER RULING on the api.json rule: DELETING an entry is allowed. The old rule ("never
      delete") was aimed at an agent removing entries to make something compile, not at a
      deliberate rename.
      TOOLING, because the sequence is not obvious: `autofix apply` on a plain sync patch could
      NOT remove the type — it reported "Successfully applied: 1 / Total changes made: 0", which
      is easy to read as success. `autofix difficult remove <Type>` is the command that removes a
      whole type. And the order matters: the accessor has to be renamed FIRST (`autofix remove
      CallbackInfo.get_wacom_pad`, then `add CallbackInfo.get_tablet_pad`), because while
      `CallbackInfo.get_wacom_pad` still returned `OptionWacomPadState` the type was pinned by a
      live reference — removing the Option wrapper first left api.json referring to a type that
      no longer existed.
      CLASSIFIER BUG FOUND AND FIXED: the new type landed in `css`, not `gesture`, because
      `determine_module` matches keywords as raw SUBSTRINGS and the css keyword "table" is inside
      "TABLEt-PadState". Worse than a misfiling: a CONFIDENT keyword match that agrees with the
      current module short-circuits the external-path check in
      `get_correct_module_with_path`, so `autofix modules` then reported the type as CORRECTLY
      PLACED and it could never move. Fixed by giving `gesture` a keyword list containing
      "tablet" — matches are ranked by keyword LENGTH, so six-letter "tablet" outranks
      five-letter "table".
      A boundary-aware whole-word matcher was implemented and then REJECTED, deliberately: it
      fixes this case but cannot recover acronyms no camel splitter can split (`GLfloat`), and it
      proposed several actively WRONG moves that substring ranking gets right today
      (`FontMetrics` -> css, `SvgParseOptions` -> xml). Registering a class is rare and the API
      moves little, so the explicit keyword is the cheaper and more auditable fix. That reasoning
      is recorded in the code beside the keyword so the next person does not retry it.
      EVIDENCE: `autofix modules` goes from "all correct" (wrongly) to naming exactly the 2
      affected types, and back to "All types are in correct modules!" after the moves are
      applied; 0 occurrences of the old name survive in the generated bindings; codegen, host
      check and the 8-target gate are green, azul-layout 7553.
- [x] 8c-i DONE. `tool_kind` is now fed on all three backends that have pen input.
      Both backends ALREADY KNEW which end of the stylus it was and were already passing it —
      Win32 reads `PEN_FLAG_ERASER` out of `POINTER_PEN_INFO` and X11 tests membership of
      `eraser_devices` (on X11 the eraser is a DEVICE of its own, classified at
      `init_xinput2`). Both handed that boolean to `update_pen_state_full` as the SAMPLE's
      `is_eraser`, and stopped there.
      `tool_kind` is separate state, set through `set_pen_tool_kind`, and only Wayland ever
      called it. So `CallbackInfo::get_pen_tool_kind()` answered `Unknown` on Windows and X11
      no matter how clearly the hardware had said "eraser" — the fact was in hand at the call
      site and simply never assigned. One call each, from the same boolean, mirroring the
      Wayland producer.
      Host, Linux-target and Windows-target checks green; 8-target gate green; azul-layout 7553.
      NOT runtime-verified — needs a real tablet on each platform.

### Follow-ups opened by 7c

- [x] 7c-i CTRL+WHEEL HALF DONE, with the flag; DirectManipulation is 7c-i-a (researched, call
      sequence recorded, not built).
      A Windows PRECISION TOUCHPAD does not deliver pinch through `WM_GESTURE` - that message is
      the touchSCREEN path, which 7c already wired. A touchpad reports pinch as Ctrl+
      `WM_MOUSEWHEEL`, the same thing every browser zooms on, so nothing reached
      `DetectedPinch` on the laptops that make up most Windows machines.
      SYNTHESIZED in the wheel arm: one notch = a 10% scale step, centred on the pointer, feeding
      the same `inject_native_gesture` path macOS magnification uses.
      The wheel event is STILL DELIVERED alongside it. Swallowing it would break Ctrl+wheel for
      anything reading it directly, and an app wanting only the pinch can ignore a scroll whose
      modifiers include Ctrl.
      FLAG per the ruling: `AppConfig::synthesize_pinch_from_ctrl_wheel`, default `true`. It has
      to exist because a real MOUSE with a real Ctrl key is indistinguishable from a touchpad at
      this layer, so an app where Ctrl+wheel means something else (a CAD zoom step, a font-size
      nudge) needs the synthesis off.
      PLUMBED through the `set_global_system_animations` pattern rather than a new field on the
      window: the Win32 wheel handler is deep inside a window procedure with no path back to the
      `AppConfig` the app was built with, and that global is the established seam for exactly
      this. `CommonWindowState` has no `config` field - checked, not assumed.
      TRAP: placing the `bool` between two align-8 fields made autofix reject `AppConfig` for
      wasting 8 bytes of padding per instance - an error that did NOT exist before the field and
      was caused by where it sat, not that it existed. Moved beside the other bools; converged at
      0 patches / 0 errors.
      EVIDENCE: the synthesis proven COMPILED by a deliberate type error under
      `--target x86_64-pc-windows-gnu`; `codegen all` + host build (the api.json size change broke
      the generated transmutes until synced, which is what proves the field really crossed).
      azul-core 2760, azul-layout 7575, azul-dll 1973, 8/8 mobile.

- [x] 7c-i-a DONE — DirectManipulation viewport, completing the ruling's "both" half.
      7c-i made pinch WORK on Windows laptops by synthesizing it from Ctrl+wheel, which is what a
      touchpad reports to an app that has not opted into anything else. That is quantised: every
      notch is a fixed 10% step, there is no pan, and a real mouse with Ctrl held is
      indistinguishable from a touchpad. DirectManipulation is the API that gives the actual
      two-finger geometry - a continuous scale - from the touchpad ONLY.
      The research from last firing held up exactly: `CoCreateInstance(DirectManipulationManager)`
      -> `GetUpdateManager` -> `CreateViewport(None, hwnd)` -> `ActivateConfiguration(INTERACTION
      | SCALING | TRANSLATION_X | TRANSLATION_Y)` -> `SetViewportOptions(MANUALUPDATE)` ->
      `AddEventHandler` -> `SetViewportRect` -> `Activate` + `Enable`.
      FIVE SEAMS, and the item would have been dead code with any one missing - which is the
      failure this arc keeps finding, so each was proven compiled by a deliberate type error:
        1. CONSTRUCT after the HWND and client size exist (CreateViewport needs both).
        2. `SetContact(pointerId)` on `WM_POINTERDOWN` - a viewport with no contact stays idle no
           matter how many fingers are on the pad, so this is what STARTS a gesture, and it must
           be the DOWN rather than the update.
        3. `Update(None)` every timer tick - MANUALUPDATE means nothing moves without it: no
           content update, no `OnContentUpdated`, no pinch.
        4. `SetViewportRect` on `WM_SIZE` - the rect is client-relative and does not follow the
           window, so a stale one keeps hit-testing the old area.
        5. `OnContentUpdated` -> `GetContentTransform(&mut [f32; 6])[0]` -> a pinch.
      MANUALUPDATE is deliberate: without it DM drives its own clock and delivers updates on a
      thread of its choosing, which is wrong for an engine that already has a frame loop.
      The transform is ABSOLUTE-since-gesture-start while `DetectedPinch` carries a per-event
      scale like every other backend, so the two are DIFFERENCED, with the baseline reset on any
      transition out of RUNNING. Sub-per-mille deltas are dropped as DM settling - forwarding them
      would emit a pinch every frame while a finger merely rests on the pad.
      `None` FROM `new()` IS A NORMAL OUTCOME, not an error: DM is absent on Server SKUs without
      the desktop experience and `CoCreateInstance` fails with no touchpad stack. The Ctrl+wheel
      path keeps working there, which is why the two coexist rather than one replacing the other.
      FEASIBILITY WAS CHECKED FIRST, not assumed: the `windows` 0.62 crate is already a dependency
      reachable through `link-static` -> `_internal_deps`, the `Win32_Graphics_DirectManipulation`
      feature exists, and `dnd.rs` already implements a COM callback with `#[implement]`. A probe
      confirmed the DM types and the handler trait resolve on `x86_64-pc-windows-gnu` BEFORE any
      of this was written.
      EVIDENCE: all five seams proven COMPILED under `--target x86_64-pc-windows-gnu`; host and
      8/8 mobile green; azul-dll 1973; autofix converged 0/0. ⚠ COMPILE-ONLY - no Windows machine
      or touchpad here, so this has not been observed to pinch.
- [x] 7c-ii ALREADY FIXED - verified, not implemented. The item was written as a suspicion ("may
      not exist under that name") and both halves turned out stale:
      `screen_to_logical_client` exists NOWHERE in `dll/src` (0 hits), so nothing references it -
      had the `WM_GESTURE` arm called it, the Windows target would not compile at all, which is
      the check that settles this kind of "may not exist" note in one grep.
      The coordinate concern it raised is also handled: the arm converts `ptsLocation` with
      `ScreenToClient` and then divides by the hidpi factor, exactly as the wheel path does, with
      a comment saying why. Only a DUPLICATED comment block was left behind by whoever fixed it,
      removed here so the next reader does not think it is unfinished.
      No behaviour change: this closes a stale entry rather than landing work.

### Follow-ups opened by 5b

- [x] 5b-i DONE, and the item was half stale. `PenState.hover_distance` DOES exist now (8c
      landed it) with `set_pen_hover_distance` and `CallbackInfo::get_pen_hover_distance`, and
      Wayland already wires `tool_distance` through — so the Wayland half was finished.
      THE REMAINING GAP was X11, which the item did not name: `init_xinput2` interned only
      `Abs Pressure`, `Abs Tilt X` and `Abs Tilt Y`. `xf86-input-wacom` also reports
      `Abs Distance`, and it was never asked for — so hover distance was Wayland-only.
      REFACTOR FIRST, per the user: `pen_valuators` was a bare
      `HashMap<c_int, (i32, i32, i32, f64)>` — four unlabelled fields whose order only its two
      call sites knew, and adding distance would have made it six. Its siblings `ScrollAxes` and
      `PadAxes` were already named structs; the pen was the outlier. Replaced with `PenAxes`
      (pressure / tilt_x / tilt_y / pressure_max / distance / distance_max), whose `Default`
      seeds the two maxima to 1.0 rather than 0.0 — they DIVIDE, and a device advertising an
      axis with no usable range would otherwise turn every sample into NaN.
      Distance is normalised to 0..1 like pressure and reuses the previous value when the axis
      is absent, because XI2 valuators are SPARSE — an absent axis means "unchanged", and
      zeroing it would snap the pen to the surface mid-hover. It is applied through
      `set_pen_hover_distance` after the sample, because `update_pen_state_full` has no
      parameter for it — the same shape Wayland uses, whose `tool_distance` arrives on a
      separate tablet event.
      TRAP hit during the refactor: inserting the new struct above `struct ScrollAxes` put it
      BETWEEN ScrollAxes' `#[derive]` and its declaration, silently moving the derive onto the
      new type. The compiler caught it as a conflicting `Debug` impl.
      Host check, Linux-target check, 8-target gate green; azul-layout 7561, azul-dll 1944. NOT
      runtime-verified — needs a real tablet on X11.
- [x] 5b-ii DONE by 10d — `PenSqueeze` / `PenDoubleTap` now have a producer. Original: they had no producer on any platform — they are `UIPencilInteraction`
      only, which is item 10d. The EventType, planning and matcher arms are in place waiting for it.

### Follow-ups opened by 4b/4c

- [x] 4c-i Register `DeviceEventManager` on `LayoutWindow` (field + `new()` + the destructure at
      `window.rs:830`) and add it to the `EventProvider` slice — same registration debt as 2c-ii and 3d-i.
      These three should land together.

### Follow-ups opened by 3c/3d

- [x] 3d-i Register `TextEditManager` in the `&[&dyn EventProvider]` slice, and clear
      `pending_composition` after the drain (same shape as 2c-ii/iii).
- [x] 3d-ii DONE, and both halves of the question are answered with evidence.
      THE OPEN QUESTION — are XIM preedit callbacks installed at all? YES. `events.rs` negotiates
      `XIMPreeditCallbacks | XIMStatusCallbacks` first, falls back to
      `XIMPreeditCallbacks | XIMStatusNothing`, then OverTheSpot
      (`XIMPreeditPosition`), and only lastly to Rooted (`XIMPreeditNothing`, where the IM
      server draws its own window and the client genuinely sees no preedit). So the preedit path
      is real wherever the IM server supports it.
      THE SUSPICION — `CompositionEnd` comes from the cancel path with empty text — CONFIRMED.
      X11 was the ONLY backend that never called `commit_composition`: macOS (`insertText:`),
      iOS, Wayland (`commit_string`), Android and Win32 (GCS_RESULTSTR) all do. X11 has no
      separate commit event — `Xutf8LookupString` hands the committed string back directly and
      the preedit simply goes empty — so commit and cancel looked identical, and
      `clear_preedit` reports End with an EMPTY payload, which is what a CANCEL means. An app
      watching composition saw every X11 commit as a cancel and had to recover the text from the
      ordinary text input that followed.
      FIX: on an emptied preedit, if a composition WAS running and the lookup returned text,
      call `commit_composition(text)`. The gate matters on X11 specifically —
      `Xutf8LookupString` also returns text for ORDINARY keystrokes while an IME is attached, so
      committing unconditionally would report a CompositionEnd for every letter typed.
      `preedit_text` still holds the previous preedit at that point, which is exactly the "was
      composing" signal.
      EVIDENCE: `a_commit_carries_its_text_and_a_cancel_does_not` pins the distinction that was
      lost, and `clearing_nothing_reports_nothing` pins that a focus change does not fabricate an
      End. azul-layout 7559, azul-dll 1944, host check, Linux-target check and 8-target gate
      green. NOT runtime-verified — needs a real X server with an IM server (fcitx/ibus).

### Follow-ups opened by 2c

- [x] 2c-i `note_scroll_phase` is called from `record_scroll_from_hit_test`, the single entry point every
      backend already funnels through with an already-classified `source` — so macOS, Wayland, X11 and Win32
      are all covered by one call and cannot drift on what starts a gesture. Original wording: call `ScrollManager::note_scroll_phase(source)` from every platform scroll path
      (macOS `scrollWheel:`, Wayland `pointer_axis*`, X11 scroll, Win32 `WM_MOUSEWHEEL`) and
      `settle_scroll_gesture()` from the physics timer when velocity reaches zero — a discrete wheel has no
      end-of-gesture signal, so without the settle call a `WheelDiscrete` gesture never closes.
- [x] 2c-ii Register `ScrollManager` in the `&[&dyn EventProvider]` slice passed to
      `determine_events_from_managers`, or the impl is never polled.
- [x] 2c-iii Clear `pending_scroll_phase` after the drain (`get_pending_events` takes `&self`; the other
      managers use a `pending_event` flag cleared elsewhere in the pass — match whatever they do).

## Step 4 — C4: open the Application phase

- [x] 4a `matches_filter_phase`: replace the `EventFilter::Application(_) => false` arm with a real
      `matches_application_filter(f, event, phase)`; write that fn.
- [x] 4b Producer: gilrs gamepad connect/disconnect → `EventType::DeviceConnected`/`DeviceDisconnected`
      (already pumped via `capability_pump`, all four desktops).
- [x] 4c Producer: Wayland — `wl_registry.global_remove` for `wl_output` → monitor events;
      `wl_seat.capabilities` + `zwp_tablet_seat_v2` add/remove → device events. All handlers already exist.
- [x] 4d Producer: Win32 `WM_DISPLAYCHANGE` monitor-list diff; add `WM_DEVICECHANGE` handling.
- [x] 4e Producer: macOS `windowDidChangeScreen:` diff + observe `NSApplicationDidChangeScreenParameters`.
- [x] 4f Producer: X11 (XI_HierarchyChanged done; RandR monitor hotplug still owed — see 4f-i) RandR `XRRSelectInput` + `XI_HierarchyChanged`.

## Step 5 — C5: new EventTypes and missing match arms

- [x] 5a Append `EventType::PenSqueeze`, `PenDoubleTap`, `PenHover` at the END of the enum (ABI stability, same
      convention `Copy`/`Cut`/`Paste`/`DocumentEdit` used). Add planning arms + Hover/Window matcher arms.
- [x] 5b `PenHover` producers: Wayland `proximity_in`/`distance`, Win32 `POINTER_FLAG_INRANGE`, Android
      `ACTION_HOVER_MOVE` (already handled), macOS `NSEventSubtype::TabletProximity` on the existing mouse path.
- [x] 5c `matches_component_filter`: add the missing `DefaultAction` and `Selected` arms.

## Step 6 — C7: accessibility actions

- [x] 6a NO WORK NEEDED — the audit was WRONG on this point. There is a shared
      `azul_layout::managers::a11y::map_accesskit_action` that every platform adapter calls through
      `poll_action`, and it is exhaustive over `accesskit::Action` (no `_` arm). `LayoutWindow::
      process_accessibility_action` likewise handles all 23 `AccessibilityAction` variants. The audit's
      "12 of 23 have no adapter arm" came from reading the per-platform files, which delegate. Original item:
      route the 12 unmapped `AccessibilityAction` variants in the accesskit adapters
      (`{windows,linux/x11,macos,android,ios}/accessibility.rs`) to the engine fns that already exist:
      `ScrollIntoView`→`scroll_node_into_view`, `ScrollToPoint`/`SetScrollOffset`→`scroll_to`/`scroll_to_unclamped`,
      `SetTextSelection`→`TextOpSetSelection`, `ReplaceSelectedText`→text-edit manager,
      `ShowContextMenu`→`open_menu_for_node`, `Show`/`HideTooltip`→`show`/`hide_tooltip_from_callback`,
      `SetValue`/`SetNumericValue`→widget setters, `SetSequentialFocusNavigationStartingPoint`→focus engine,
      `CustomAction`→callback passthrough.

## Step 7 — shell wiring, no API change

- [x] 7a Wayland: bind `zwp_pointer_gestures_v1` (swipe, pinch, hold) → existing pinch/rotate/swipe filters.
- [x] 7b X11: XInput 2.4 `XI_GesturePinch*` / `XI_GestureSwipe*`.
- [x] 7c Windows: TOUCHSCREEN pinch/rotate via WM_GESTURE. The touchpad half is NOT this API — see 7c-i. Original item: touchpad pinch — handle `WM_GESTURE`, or recognise from the pointer stream.
- [x] 7d Wayland: raise `seat_version` cap from `min(7)` to 9; add `axis_value120` and `axis_relative_direction`
      listeners; keep `axis_discrete` as the v5–v7 fallback.
- [x] 7e macOS: read `isDirectionInvertedFromDevice` (natural-scroll flag).
- [x] 7f macOS: add `pressureChangeWithEvent:` (Force Touch `stage` / `stageTransition`).
- [x] 7g Wayland: fill the empty `touch_shape_handler` / `touch_orientation_handler` bodies (needs 8b first).
- [x] 7h NO WORK NEEDED — the audit was wrong. `WM_MOUSEWHEEL` already divides as `delta as f32 / WHEEL_DELTA as f32`, so the fractional remainder is preserved; there was no truncation to fix.

## Step 8 — api.json deltas (record intent; run `azul-doc autofix` at fix-up time)

- [x] 8a `MouseState.other_down: u8` + `Back`/`Forward` `MouseDown`/`MouseUp` on Hover/Focus/Window; extend the
      `button_specific_down` helper. All four layers.
- [x] 8b `TouchPoint += { major, minor, orientation_rad, tool_type }` + `TouchToolType { Unknown, Finger, Stylus,
      Eraser, Palm, Mouse }`.
- [x] 8c `PenState.hover_distance` (proximity Z). The ragged tail is DONE on Wayland/X11 via #450 — what remains
      is macOS + Win32 parity. Do NOT invent `PenToolType`: #450 shipped `TabletToolKind { Unknown, Stylus,
      Eraser, Pad, Touch }`; either widen that toward the 8-value `zwp_tablet_tool_v2` set (Brush, Pencil,
      Airbrush, Lens) or leave it — but reuse it, don't duplicate it.
- [x] 8d (struct grown + strip/mode wired; dial has no producer yet — see 8d-i) `WacomPadState` → `TabletPadState` + `{ strip, strip_active, dial_delta, mode, mode_count }`.
      #450 left this struct at 2 of 5 pad controls, so it is still fully open. `TabletToolKind::Pad` and
      `TabletDeviceInfo.button_count` now exist to hang it off.
- [x] 8e `SensorKind += RotationVector, Gravity, LinearAcceleration, AmbientLight, Proximity, Barometer,
      StepCounter, HingeAngle`.
- [x] 8f (state + buttons done; `GamepadRumble` is OUTPUT and is 9g's haptics item, and the backends do not yet fill the new fields — see 8f-i) `GamepadState += { battery, touchpad, imu }`; buttons `Misc1, Paddle1..4, Touchpad`; `GamepadRumble`.

## Step 9 — new capability

- [x] 9a `FocusTarget += Directional(FocusDirection)`, `FocusDirection { Up, Down, Left, Right }`, geometric
      nearest-neighbour over the existing focusable set. No shell code.
- [x] 9b (types + Wayland producers done; other backends leave it Unknown — see 9b-i) `PointerSource { Unknown, Mouse, Touchpad, Trackball, Trackpoint, Touchscreen, Pen, Eraser }` on pointer
      events; `device_id` on mouse and key events. NOTE: #450 already delivered the *tablet* half
      (`TabletDeviceInfo`, matching `device_id` on `PenState`/`WacomPadState`) — this item is now the
      mouse/keyboard half plus the per-event `PointerSource` discriminator. Model it on `TabletDeviceInfo`.
- [x] 9c (type + Wayland producer done; Surface Dial / crown backends are 9c-i) `DialState { device_id, delta_rad, detent_count, pressed, contact_position }` + `DialRotate`/`DialClick`
      filters; wire Wayland `zwp_tablet_pad_dial_v2` (already bound) as the first producer.
- [x] 9d (types + X11 producer done; Win32/Wayland/web are 9d-i, and locking itself is 9d-ii) `RawMouseMotion` window filter + pointer-lock request path; Win32 `WM_INPUT`,
      Wayland `zwp_relative_pointer_v1` + `zwp_pointer_constraints_v1`, X11 `XI_RawMotion`.
- [x] 9e (types + ModifiersChanged + accessors done; the shells do not fill the new state yet — see 9e-i) `PhysicalKey` positional enum + `ModifiersChanged` filter + `KeyboardState += { modifiers, locks,
      is_repeat }`.
- [x] 9f (types + manager + accessors; no backend enumerates yet — 9f-i) `HidDevice { vendor_id, product_id, usage_page, usage, name }` + `HidReport { bytes }`.
- [x] 9g (types + manager + `CallbackInfo::play_haptic`; no backend plays yet — 9g-i) `Haptic::play(pattern)` — macOS `NSHapticFeedbackManager`, Win32 `SimpleHapticsController`,
      Android `performHapticFeedback`.
- [x] 9h Win32 `WM_APPCOMMAND` → media/browser app-command channel.

## Step 10 — mobile parity

- [x] 10a (native side + JNI entry points + keyboard request; the Java class is 10a-i) Android `InputConnection` (JNI bridge) → text input + the composition events from 3d.
- [x] 10b (UIKeyInput + UIPress done; full UITextInput is 10b-i) iOS `UITextInput` → text input; `UIPress`/`UIKeyCommand` for hardware keyboard.
- [x] 10c (keyboard inset modelled + Android bridge; safe-area itself already existed on macOS/iOS. iOS keyboard notifications are 10c-i) Insets / safe area / keyboard avoidance as a layout input (Android `WindowInsets`, iOS safe area).
- [x] 10d iOS `UIPencilInteraction` → `PenSqueeze` + `PenDoubleTap`.
- [x] 10e (iOS done; the web equivalent is 10e-i) `coalescedTouches` / `predictedTouches` (iOS) and the equivalent elsewhere.
- [x] 10f (both written; the Java half is 10f-i) Real gamepad backends to replace `gamepad/android.rs` (16 lines) and `gamepad/apple.rs` (17 lines).

## Step 11 — C6: full-stack stragglers

- [x] 11a `Submit` off the existing `DefaultAction::SubmitForm`; `Change` as commit-on-blur off
      `TextInputOnFocusLost`. Add the filter variants first (all four layers), then update the
      `events_test.rs` unmapped pin.
- [x] 11b DONE - the parent entry outlived its sub-items: 11b-i (Submit / Invalid / Reset produce),
      11b-i-a (Reset), 11b-i-b (`pattern` through regex-lite), 11b-i-c (validity exposed) are all
      done, so the form family is produced, planned, matched and readable end to end. The "original
      note" below it is history.
- [!] 11c BLOCKED — REVISIT AT THE END. Media: `Play`/`Pause`/`Ended`/`TimeUpdate`/`VolumeChange`/
      `MediaError`. Verified, not assumed: `dll/src/unified/` has a decoder (`decode_mp4_h264`), an
      encoder (`VideoEncoder`), a sink (`AudioSink::play(frame)`) and a screen recorder — but NO PLAYER.
      There is no transport, no `is_playing`, no position, no duration, no volume. Every one of these six
      events describes a state change in a player that does not exist, so there is nothing to emit them
      from and no honest way to fake it. They correctly remain in the `events_test.rs` unmapped pin.
      **Prerequisite: a playback state machine** — `{ playing, position, duration, volume }` plus a
      transport API — which is a feature, not wiring, and outside this arc. BLOCKED on a real playback state
      machine — `dll/src/unified/audio.rs:62` is `pub fn play(&self, _frame: AudioFrame) {}`. Build the state
      machine first, then emit.

## Step 12 — headless/test surface

- [x] 12a `HeadlessEvent`: add scroll-phase, pen, gesture, gamepad, sensor and composition injection variants so
      everything above is reachable from the e2e runner.

## Step 13 — FIX-UP (only after everything above)

- [x] 13a `cargo check --workspace` — fix compile errors.
- [x] 13b `cargo run --release -p azul-doc codegen all` (target/codegen is wiped by `cargo clean`).
- [x] 13c (ran to convergence: 0 add / 0 remove / 0 modify / 0 pathfix / 0 FFI errors) `azul-doc autofix` for every api.json delta recorded in step 8.
- [x] 13d-cross CROSS-COMPILE (added at the user's request, ahead of tests):
      - `aarch64-apple-ios` — was **680 errors before this arc**, now 0. Required a real
        `UIPasteboard` transport (`ios/clipboard.rs`) and moving `rich-clipboard`/`rclip-core` out of
        `cfg(not(any(android, ios)))` into the shared table, plus fixing two `run_headless` call sites
        that omitted `tray`/`font_manager`/`app_icon`.
      - `aarch64-apple-ios-sim` — 0 errors.
      - ⚠ The iOS SIMULATOR cannot be run on this machine: `xcode-select -p` is
        `/Library/Developer/CommandLineTools`, there is no `Xcode.app`, `simctl` is absent and there are
        no CoreSimulator runtimes. Compiling for the simulator ABI is as far as verification goes here.
- [x] 13d-android `aarch64-linux-android` — 0 errors. Two real bugs found that the host could never see:
      `with_window` imported from `super::` when it lives in the sibling `jni_bridge` module, and the
      `azul_css::OptionPixelValue` path again.
- [x] 13d-linux `x86_64-unknown-linux-gnu` — 0 errors, and the biggest catch of the sweep: 46 errors
      including a `..Default::default()` appended inside a struct DEFINITION, field initializers inserted
      into a tuple DESTRUCTURING PATTERN, and an invented `SendPtr`/`OnceLock` pair where the module uses
      `SyncInterface`. `cargo check` does not link, so no cross C toolchain was needed after all.
- [ ] 13d-windows `x86_64-pc-windows-msvc` — the one target still unchecked. Needs the MSVC libs, which
      are not obtainable on macOS; `x86_64-pc-windows-gnu` may check without them. **The Win32 code in
      this arc (WM_GESTURE, WM_APPCOMMAND, WM_DEVICECHANGE, the pointer/pen paths) is therefore still
      uncompiled.**
      USER RULING 2026-09-04 (the hardware / platform group): "just implement blindly and we cross-compile
      at the end. Real verification will come with time."

### iOS simulator — investigated, NOT reachable on this machine

- `xcode-select -p` = `/Library/Developer/CommandLineTools`; no `Xcode.app`, no `simctl`, no
  CoreSimulator runtimes, and `/Library/Developer/CommandLineTools/SDKs` holds only MacOSX SDKs — so
  there is no `iphoneos`/`iphonesimulator` sysroot and iOS cannot even be LINKED here, only `check`ed.
- `scripts/build-ios.sh` + `doc/guide/en/deploying/mobile.md` describe bundling a `.app` without an Xcode
  PROJECT, which is true — but it still needs the iOS SDK for the linker sysroot.
- **vphone-cli** (suggested 2026-09-01) does not close the gap: it boots a full iOS VM via
  Virtualization.framework, and its prerequisites are a SUPERSET of what is missing — it still requires
  Xcode + the iOS SDK to cross-compile its guest daemon, AND requires relaxing SIP/AMFI. This machine is
  Apple silicon on macOS 15.5 (both fine) but SIP is ENABLED, and disabling it needs a recoveryOS boot.
  It also boots a jailbroken iOS VM rather than a simulator, which is a heavier and different thing.
- **baguette** (suggested 2026-09-01) is iOS-simulator-only and requires **Xcode 26** — it links against
  the private `SimulatorKit` / `CoreSimulator` frameworks that ship WITH Xcode, so it cannot bootstrap
  one. Worth revisiting the moment Xcode exists though: it does headless boot, 60fps frame streaming,
  **touch/gesture INPUT INJECTION** and accessibility-tree inspection from a CLI — which is precisely
  what would exercise this arc's work end to end.
- **Shortest real path: install Xcode.** Both suggested tools gate on it, and it also yields the iOS SDK,
  so iOS could be LINKED rather than only checked and a `.app` assembled per the deploy guide.

### Android emulator — reachable, but it cannot exercise this arc yet

- Present on this machine already: `sdkmanager`, `avdmanager`, `adb` (Homebrew) and `cargo-ndk`.
- Missing: a **JDK** (sdkmanager/avdmanager will not run without one), the `emulator` package, a system
  image, and an SDK root — none of which need Android Studio, per the standard command-line-tools route.
- ⚠ **But the emulator could not test the input work.** The Android bridges written in this arc
  (`NativeTextBridge` for text/IME/insets, `AzulGamepad` for pads) are JNI entry points whose **Java
  counterparts do not exist** — items 10a-i and 10f-ii. Nothing would call them, so a booted APK would
  show a window and drive none of the new input paths. Build the Java glue FIRST; the emulator only
  becomes useful after that.
- [x] 13d DONE. azul-css 2865, azul-core 2775, azul-layout 7613, azul-dll 1995, azul-doc 211.
- [x] 13e DONE, and it was worth every second: 988 pass, and it found FOUR things `--lib` cannot.
      ⚠ THE E2E TARGET HAD NOT COMPILED SINCE 8f. `layout/tests/synthetic_events.rs` built a
      `GamepadState` field-by-field, so the battery/touchpad/IMU fields broke it - and because
      `--lib` never builds `--test all`, every "azul-layout 76xx passed" in this whole arc was
      reported against a target that was RED. That is the single most important thing this item
      found, and it is an argument for running it more than once.
      1. THE GAMEPAD CHANGE DETECTOR IGNORED TEN FIELDS. `state_bitwise_eq` never compared
         `battery`, `touchpad_*`, `gyro_*` or `accel_*`, so a pad whose only change was an IMU
         sample, a finger on the touch surface or a battery step reported UNCHANGED and fired no
         `GamepadInput` - leaving the producers 8f-i and 8f-i-a filled writing into a value
         nothing downstream read. A gyro-aiming game got no events at all. Fixed, with a
         field-completeness test and a negative control ("changing `battery` did not register as
         a change").
      2. The keycode MANIFEST drifted: 9h-i mapped 20 X11 keysyms (media, web, power) and never
         updated the table that pins them, and three keys sat on the EXEMPTION list while being
         mapped - which masks a real row. The test also caught my own first attempt: I guessed
         Win32 maps `WebBack`, and it maps `NavigateBackward` instead.
      3. A REAL INTERMITTENT FAILURE in the dll suite: `validation_enabled` latched `AZ_VALIDATE`
         in a `OnceLock`, so whichever test read it first decided for the whole binary - and when
         that was an unrelated test, all 13 validation tests failed with "the gate is OFF in this
         test binary". Observed once, then two clean runs of the same command. The latch is right
         for the product (read once per input pass) and wrong under `cargo test`, so it is now
         skipped under `cfg(test)`; the parsing moved into `validation_from_value` so it can be
         tested WITHOUT touching the environment, which is what made the race in the first place.
      4. `synthetic_gamepad_state_fires_gamepadinput` asserted ONE event where hotplug now makes
         two - the arrival before the first sample, which is the intended order.
      ⚠ The e2e run is expensive (160-185s plus a rebuild). USER RULING mid-item: implement and
      write tests, do not run the batteries between items - so 13e is the checkpoint, not a
      per-item gate.
- [x] 13f DONE, and the allow-list was ALREADY EMPTY - which is exactly why this needed doing.
      ⚠ THE RATCHET COVERED 68 OF 104 EVENT TYPES. `KNOWN_DESYNC` being empty meant nothing,
      because the `cases` list it guards had never been complete: 36 variants were absent,
      including `MouseMove` - about as core as it gets - and every type this arc added
      (`Submit`, `Reset`, `Invalid`, `HidReport`, `DialRotate`, `DialClick`, `RawMouseMotion`,
      the three new Pen types, the media and monitor families). A desync in any of them would
      have landed green. Same shape as the `TIER1_SLOTS` table that let `cursor` overwrite
      `align-self`: a guard over an incomplete table proves nothing.
      THE FIX IS NOT "ADD THE 36". A hand-written list drifts again the moment someone adds a
      variant, so the enumeration is now DERIVED from the enum: `next_event_type` is an
      exhaustive match returning each variant's successor, and `all_event_types` walks it. A new
      variant fails to COMPILE until it is spliced into the chain, and splicing it in
      automatically puts it in the walk - which a parallel array cannot guarantee, because the
      array and the match can drift apart. That is the difference between this and the guards
      that failed before.
      THE ANSWER TO THE ITEM'S QUESTION: with all 104 covered and the allow-list still empty, the
      test PASSES - so there is no planning/matcher/phase desync left anywhere in the enum. That
      is a real result rather than a tidy-up: it is the first time the claim has been checked
      over the whole surface.
      EVIDENCE: NEGATIVE CONTROL - removing `MouseMove` from `cases` fails with "the ratchet does
      not cover 1 of 104 EventTypes, so a desync in them lands green: [MouseMove]". azul-core
      2775, host check green.

---

## 14 — on-device E2E (2026-09-02)

**14a DONE** — `AZ_E2E` now works on Android. `run()` always called
`setup_debug_and_e2e`, but bound the request receiver to `_debug_request_rx` and
forwarded it only on the headless path, so nothing drained the channel on a real
window: tests queued, no op dispatched, the result printer sat until its 600 s
timeout. Fixed with `ANDROID_DEBUG_CHANNEL` + one `register_debug_timer` call in
`android_main` (the loop's existing `process_timers_and_threads()` drives it).
Scenario path arrives via the `debug.az.e2e` system property, because an
activity cannot be given an env var. Driven by `azul-doc mobile run --e2e`.

**14b OPEN — `record_frame` is called by NO real backend.**

`LayoutWindow::record_frame` (layout/src/window.rs:1699 →
`record_frame_at_generation`:655) is what advances `frames_since_reset` and
accumulates paint/present damage. It has exactly **two** call sites in the
tree, both in `dll/src/desktop/shell2/headless/mod.rs` (:1653, :1798).

macOS, Windows, X11, Wayland and Android never call it. So on every real window
`frames_since_reset` stays 0 and the damage accumulators stay empty, which makes
these ops vacuous or wrong outside the headless harness:

    assert_work_bounded            (fails outright: "NO FRAME was rendered")
    assert_damage
    assert_damage_covers_changes
    assert_damage_sound
    assert_damage_incremental
    reset_frame_counters
    get_frame_report

Found by running `e2e/op-focus-blur.json` on an Android emulator through the new
transport: PASS in-process (25 ms), FAIL on device at step 17. Same scenario,
same dispatcher — so the difference is the backend, not the platform code under
test. This is the same shape as the input-filter defects this branch is about:
implemented once, wired in one place, silently dead everywhere else.

Fix = a `record_frame(paint, present)` call on each backend's present path with
the damage rects it actually submitted. Five backends, and the damage values
have to be real or the assertions become confidently wrong instead of vacuous —
so it wants its own change, not a tail-end commit here.

Until then: treat a device/desktop e2e run's frame and damage assertions as
untrustworthy, and keep those scenarios on `azul-doc e2e`.


## 15 — review of the prop-cache perf work (2026-09-02)

Asked to review it after a report that the AzWriter BACKSTAGE stopped rendering.
Commits in this branch (rebased SHAs): `59b20b3ce` cascade trace, `94dcd67cd` docs,
`91cb14e31` share identical property runs, `a915d15ff` `cursor` into the tier-1
bitfield, `fcef148b2` `writing-mode` inheritance mask + self-check.

FOUND AND FIXED HERE:
- `azul-core`'s TEST build did not compile, so NONE of the PR's own tests could
  run — including `every_inheritable_tier1_property_is_actually_inherited`, the
  self-check `fcef148b2` added specifically to stop the mask rotting. Two causes:
  my own `safe_area` field (callbacks_test.rs, 2 literals) and this arc's
  `MouseEventData::{source, device_id}` (core/tests/events.rs, 2 literals).
  Now: 2736 lib tests pass and the mask self-check RUNS and passes.
- `ALL_HOVER` / `ALL_FOCUS` were missing `Submit`/`Change`/`Reset`/`Invalid`.
  Planning is DERIVED by probing those arrays, so a filter absent from them can
  never be planned — the arc's own dead-filter shape, reintroduced by the arc.
  Fixed; `event_type_to_filters_omits_button_specific_filter_for_exotic_buttons`
  passes again.
- A duplicated, unreachable block of four `Self::Submit/Change/Reset/Invalid`
  match arms in `events.rs` (rebase artifact).

- [x] 15a DONE. `core/tests/prop_cache.rs` has TWO failing
      tests, PRE-EXISTING (verified by stashing all of tonight's edits and
      re-running): `test_computed_values_exist_for_all_nodes` and
      `test_non_inheritable_property_not_inherited`. Both fail at the same point —
      `computed_values.values_for_opt(node)` returns `None` where they expect
      `Some`.
      Measured what the store actually holds for `div > p > text` with no author
      CSS: node 0 = 0 values, node 1 = 0 values, node 2 = 1 value (`cursor`, the
      property `a915d15ff` moved into the tier-1 bitfield).
      This is EITHER correct by design (the store is the INHERITED store, and a
      subtree that inherits nothing legitimately holds nothing, so the tests'
      `is_some()` is a stale proxy for "was cascaded") OR the transpose lost
      data. I could not settle which without the intended invariant, and
      `values_for_opt` has NO production consumers — only tests — so the failures
      alone do not prove a rendering bug.
      SETTLED (3cc22a150). Both answers were partly right: an empty entry set
      IS legal, and the accessor was still wrong. `values_for_opt` returned
      `None` for "no properties" and for "no such node" alike, with no bound
      check to separate them - so the tests read "missing" from "plain". It now
      keys off `node_count` (which the store already tracked): an existing node
      with nothing yields `Some(vec![])`, out-of-range yields `None`.
      `values_for_opt_separates_an_empty_node_from_a_missing_one` pins both
      directions. azul-core is fully green.
      This was NOT the backstage regression. That was a separate defect in the
      same PR - see 15b.

- [x] 15b DONE (9c0fd955d). THE BACKSTAGE REGRESSION, root-caused and fixed.
      `cursor: pointer` on a flex item silently changed its `align-self`:
      `CURSOR_SHIFT` was 53 (5 bits), which is where `ALIGN_SELF` (53),
      `JUSTIFY_SELF` (56), `GRID_AUTO_FLOW` (59) and `JUSTIFY_ITEMS` (61) have
      lived since April. Those four are declared in a SECOND constant block 24
      lines below the first, and every audit that called bit 53 "the first free
      slot" had read only the first block - the commit message, the
      `the_cursor_slot_does_not_collide` test (which checked the slot against
      BORDER_COLLAPSE below it but not ALIGN_SELF above it), and `TIER1_SLOTS`
      in compact_test.rs (whose table stops before the second block, which is
      why `the_tier1_slots_do_not_overlap` stayed green while cursor sat on top
      of align_self - it never named the field being corrupted).
      Because `cursor` is inheritable, the corruption reached every descendant.
      FIX: `cursor` moves to `CompactNodePropsCold::cursor`, a u8 that lands in
      padding the struct already had (size_of stays 48). GUARDS:
      `tier1_bit_ranges_do_not_overlap` checks all 25 fields pairwise and names
      both sides; `TIER1_SLOTS` gains the four missing rows.
      TRAP for the next person: a full revert of the OTHER five prop-cache
      commits rendered the backstage PIXEL-IDENTICAL, which looks like an
      exoneration and is not - the defect was in the sixth (a915d15ff), which
      was not in that revert set. Verify a revert covers the whole PR.
- [x] G2-a DONE. The open question was "is that movement-based `MouseOver` emitter correct at
      all". Answer: the BEHAVIOUR is right and the NAME is wrong. Azul had no movement event -
      `MouseMove` did not exist in any enum - so `MouseOver` was carrying `mousemove` semantics
      under the `mouseover` name, which is why the gain side of the hover chain had no bubbling
      half to emit: the event that should have been it was already spoken for.
      Settled per the user's standing W3C ruling ("do whatever w3c recommends here, not what gtk
      does"). W3C defines TWO mirror pairs: enter/leave do not bubble, over/out do, and all four
      fire on ENTRY or EXIT. Movement is `mousemove`, a third thing.
      ADDED `EventType::MouseMove` + `HoverEventFilter`/`FocusEventFilter`/`WindowEventFilter`
      ::MouseMove + `On::MouseMove`, all APPENDED for C ABI stability, and wired through all four
      dispatch layers (planning arm, three matcher arms, `to_hover`/`to_focus` converters, and the
      ALL_HOVER/ALL_FOCUS/ALL_WINDOW arrays - a filter absent from those can never be planned).
      MOVED the movement emitter to `MouseMove` (unchanged behaviour, renamed) and gave the
      hover-chain gain branch its `MouseOver`, beside `MouseEnter`, mirroring the loss branch.
      BLAST RADIUS, which is what made this "not a test fix": eight call sites subscribed to
      `MouseOver` expecting MOVEMENT and would have frozen on entry-only semantics. All were
      identified by their handler names and switched to `MouseMove`: eyedropper (`on_loupe_move`),
      slider (`on_slider_pointer_move`), split-pane (`on_split_pointer_move`), map
      (`map_on_pointer_move`), node-graph (`nodegraph_drag_graph_or_nodes`), colour plane (the
      pointer-capture drag), plus the core drag-SELECTION handler (`handle_mouse_over` ->
      `handle_mouse_move`) and the pointer-capture retarget in `common/event.rs`, which retargeted
      `MouseOver | MouseUp` and would have dropped every captured move.
      KEPT on `MouseOver` (entry is what they actually want, and entry-only is strictly better
      than the old firehose): the submenu-open handler in `menu_renderer.rs`, and text_input's
      `default_on_mouse_hover`, which is an inert stub.
      TRAP: the new `On::MouseMove` arm in `impl From<On> for EventFilter` was initially a
      CATCH-ALL BINDING, not a variant match - `MouseMove` was not in that site's `use On::{...}`
      list, so it silently matched every value and made all 40+ arms below it unreachable. The
      library still COMPILED; only the test build surfaced it, via `unreachable pattern` warnings.
      Adding a variant to a `use`-list-style match is a silent-shadowing hazard, not a syntax one.
      EVIDENCE: 3 new tests pinning the split - movement within a node fires `MouseMove` and NOT
      `MouseOver`; `MouseOver`/`MouseEnter` fire together on the same node; and both mirror pairs
      are complete on one hover change. 4 existing tests that asserted `MouseOver` counts for
      MOVEMENT were retargeted at `MouseMove` (they encoded the old semantics). Host check, 8/8
      mobile, azul-core 2759, azul-layout 7564, azul-dll 1963, azul-doc 209.

- [x] G2-a-i DONE. This was a regression the MouseOver/MouseMove split (G2-a) introduced and I
      logged rather than fixed: once azul's `MouseOver` became the ENTRY event, `html_render`
      emitted "mouseover" for it while `loader_js` still decoded that name to `EVT_MOUSEMOVE`. So
      `MouseOver` and `MouseMove` registered the SAME wire kind, `cb_node_kinds` could not tell
      them apart, and a web `MouseOver` subscriber went on firing continuously while the pointer
      travelled - exactly the firehose the split was meant to end.
      FIXED with a kind of its own on both sides (`EVT_MOUSEOVER` / `event_kind::MOUSEOVER`, 15,
      appended) plus the listener that produces it - there was none, so the constant alone would
      have been inert. `mouseover` BUBBLES, unlike `mouseenter`, so a plain body listener sees
      every descendant's entry and `azDispatch` resolves the node as `mousemove` does. It is
      deliberately NOT coalesced into an animation frame the way `mousemove` is: a move is a
      stream where only the newest sample matters, but an ENTRY is a discrete transition and
      dropping one loses it entirely.
      GUARDED: `the_loader_event_kinds_match_the_rust_wire_codes` checks all 16 constants against
      `event_kind`. They are a wire protocol split across two LANGUAGES held together by nothing
      but a comment, and drift is silent - renumbering one side routes every event of that kind
      to the wrong callback, or to none. Same hazard as the Android sensor codes, same guard.
      EVIDENCE: all 16 constants verified in sync; the extracted loader parses cleanly under
      `deno`; the Rust changes are baseline-compared against a stashed tree.
      ⚠ The test CANNOT RUN TODAY - see G2-a-ii.

- [ ] G2-a-ii LOW PRIORITY (user: "ignore the web for now, the web feature is not that important
      right now"). The `web` FEATURE DOES NOT BUILD, and has not for some time: `cargo check -p
      azul-dll --features web` fails with 4 errors that predate anything in this arc, confirmed by
      stashing - `rust_fontconfig::AZ_IN_WASM_SOLVE` no longer exists, `WebConfig` is missing a
      `prelift` field, and two `html_render.rs` call sites use APIs that have since changed
      arity/shape. It is straightforward drift from the rest of the codebase moving while this
      backend was not being compiled.
      NOTHING CATCHES IT: no CI job builds the `web` feature (rust.yml's dll check omits it), which
      is precisely why it rotted. That also means the G2-a-i guard above compiles nowhere today.
      Fixing the four errors is a separate piece of work from the event-kind wiring, which is why
      it was logged rather than folded in.
- [x] TOOLING (azul-doc): the module classifier now matches DIFFICULT names manually FIRST and
      everything else automatically, per user direction — so a collision is fixed by naming the
      one bad case rather than by tuning a keyword (which reranks every other type) or by
      hard-coding a module per class.
      `DIFFICULT_TYPE_MODULES` is a PREFIX table checked at priority 4, after the structural
      Option/Vec/Error rules (so an entry can never steal `OptionTabletPadState` from `option`)
      and before keyword matching. One entry today: `("Tablet", "gesture")`, because the css
      keyword "table" matches as a substring inside "TABLEt-PadState".
      This REPLACES the `"tablet"` keyword added in 8d-ii, which worked only by out-ranking
      "table" on length — a fix that depends on nobody adding a longer css keyword later.
      EVIDENCE: 4 tests — the override resolves the whole `Tablet*` family confidently; it does
      NOT capture `TableLayout`/`StyleTableLayout` (prefix, not substring); structural types
      still win over it; and ordinary names (`StyledDom`, `FontMetrics`) are untouched so they
      keep being classified automatically. Verified the override is load-bearing by deleting its
      one entry: `TabletPadState` resolves to "css" again.
- [x] CLEANUP: `find_class_optional` (dll/src/desktop/extra/mod.rs) removed — it had NO callers
      and is strictly weaker than the `find_app_class` that superseded it. Both clear the pending
      JVM exception, but only `find_app_class` also goes through the Activity's class loader,
      which is the bug that made every Rust->Java call fail; all six optional Android helpers use
      it. Wiring the old one back in would have REINTRODUCED the classloader bug, so this is a
      superseded helper rather than an unwired one. Caught by `lint_orphans`, which was failing
      in `cargo test -p azul-doc` (206 tests now green).
- [x] CLICK SYNTHESIS was keyed on a proxy that a backend need not satisfy — three of the eight
      failing headless tests were this, and so is every `HoverEventFilter::Click` widget on that
      path.
      `determine_all_events` emitted `Click` when `previous_hover_node == current_hover_node`,
      as a stand-in for "released on the node it was pressed on". The comment said so
      ("proper click synthesis requires tracking mousedown target across frames") and the tests
      were already NAMED after the real rule
      (`click_is_synthesized_when_release_lands_on_the_press_node`).
      The proxy only holds if the hover manager pushed a hit test for the PRESS as well as the
      move. A backend is under no obligation to: the headless one pushes only on MouseMove, so
      `previous_hover` stayed `None` across press and release, the comparison never matched, and
      NO Click was ever emitted. Ribbon tab headers use `HoverEventFilter::Click`, so tab
      switching was inert — the tests reported it as "the click did not reach the tab header",
      which reads like a hit-test miss and is not: the hit test resolved the right node
      (verified by probe: hover = NodeId(7), inside tab 1's rect 62x26 @ (58,16)).
      FIX: use `HoverManager::press_target(MouseButton::Left)`, which already existed —
      `apply_press_target_capture` records it on MouseDown and removes it on MouseUp, both AFTER
      `determine_all_events`, so during the release pass the press is still on file. This is the
      W3C rule rather than a coincidence, and it holds for any backend that runs a pass per
      event.
      EVIDENCE: azul-dll headless failures 8 -> 5, with all three tab/ribbon ones fixed; the two
      pre-existing Click tests now record a real press through the same
      `apply_press_target_capture` the dll calls, so they test the rule their names claim.
      azul-layout 7555, host check and 8-target gate green.
- [x] PLANNING NEVER PROBED Component OR Application FILTERS — the biggest instance of this
      arc's recurring shape, and the root of 2 more of the 8 headless failures.
      `event_type_to_filters` is the LIVE planning function and it derives its answer by probing
      `ALL_HOVER`, `ALL_FOCUS` and `ALL_WINDOW`. There was no `ALL_COMPONENT` and no
      `ALL_APPLICATION`, so it returned an EMPTY filter list for every lifecycle and
      application event. Measured directly: `event_type_to_filters(Resize) -> []`.
      Everything else was correct and looked correct: `matches_component_filter` pairs all nine
      Component filters with their EventTypes, the dispatcher has a proper
      `EventFilter::Component(_)` arm that targets the node without bubbling, the producer
      creates the event and queues it, and `dispatch_pending_lifecycle_events` hands it over.
      The event reached dispatch with the right type and target and was then planned against
      nothing. There IS a match naming `E::Resize => Component(NodeResized)` — in
      `event_type_to_filters_legacy_hint`, which planning does not call.
      CONSEQUENCE: every `EventFilter::Component(..)` callback in every app was dead —
      `AfterMount`, `BeforeUnmount`, `NodeResized`, `Updated`, `Dismissed`, `TornOff`, `Docked`,
      `DefaultAction`, `Selected` — and every `EventFilter::Application(..)` one
      (device/monitor hotplug) with them.
      FIX: `ALL_COMPONENT` (9) and `ALL_APPLICATION` (4), probed alongside the other three.
      EVIDENCE: `every_component_and_application_filter_is_reachable_from_planning` asserts the
      round trip for all 13; verified it bites by deleting the Component probe loop ("Mount must
      plan AfterMount, got []"). azul-dll headless failures 5 -> 3
      (`node_resized_fires_after_a_relayout` and
      `a_capture_tile_reports_its_device_size_to_its_worker`, the latter an `AfterMount` test).
      azul-core 2749, azul-layout 7555, host check and 8-target gate green.
- [x] TEXT-SELECTION DRAG had a handler and NO PRODUCER — dragging a selection did nothing until
      the pointer reached the window edge.
      `SystemChange::TextSelectionDrag` has a full arm in `apply_system_change` (node-drag
      suppression gate, logging, the `process_mouse_drag_for_selection` call, the
      ShouldUpdateDisplayListCurrentWindow result) and NOTHING in the dll ever constructed it.
      Grepped: the only constructions anywhere are in `layout/src/e2e/runner.rs`, which
      deliberately PORTS the dll arm for headless scenarios. So the sole path that reached
      `process_mouse_drag_for_selection` in a real app was the drag-auto-scroll timer, which only
      fires once the pointer is dragged past the window edge.
      The give-away is a comment on the block right above the fix, which still reads
      "TextSelectionDrag was the only StartAutoScrollTimer trigger" — written when this WAS
      wired.
      FIX: emit it from the event pass when a `Drag` event is present and the left button is
      held. Both positions are the current pointer, because the anchor lives on the multi-cursor
      state and the handler ignores the start argument — the auto-scroll path passes the pointer
      twice for exactly that reason. A pointer with no editing session behind it makes the
      handler a no-op, which is what a node or file drag wants, and the existing gate already
      suppresses text selection during a node drag. The result is folded into the pass result,
      or the selection would extend without repainting.
      EVIDENCE: `a_text_selection_survives_a_relayout` passes; azul-dll headless failures 3 -> 2.
      azul-layout 7555, host check and 8-target gate green.
      NOTE for later: `SystemChange::TextSelectionClick` and `SystemChange::AddCursorAtClick`
      are in the SAME state — arms present, never constructed outside the e2e runner. They are
      not covered here because click-to-place-caret demonstrably works on device, so something
      else drives it and wiring these blind could double-handle the click. Logged as G3-a.
- [x] G3-a CLOSED, and it corrected a WRONG FIX of mine. The premise was false: these changes
      are NOT unproduced. `SystemChange` is declared and produced in **azul-core**
      (`core/src/events.rs`), and I had only grepped `dll/src` and `layout/src` — so
      `TextSelectionClick` (events.rs:4962) and `TextSelectionDrag` (events.rs:4992) both have
      real producers. Proved by backtrace: a click reaches
      `process_mouse_click_for_selection` through `apply_system_change`, and a new test
      (`clicking_into_text_places_the_caret_at_the_click`) shows the caret landing at the click
      rather than at the field start.
      CONSEQUENCE: the emission I added to the dll for `TextSelectionDrag` was a DUPLICATE at
      the wrong layer, and worse — it bypassed the anchor, so it armed a selection drag for
      ANY drag with the left button down. That is exactly the bug the anchor logic documents
      itself as fixing ("dragging the window by its custom titlebar became a selection drag,
      armed drag-autoscroll and scrolled the UI to the top"). REVERTED.
      THE REAL BUG was the anchor's arming rule: `text_selection_drag_anchor` was born only if
      the press landed on a CONTENTEDITABLE. Ordinary document text is selectable in every
      browser and native text view, and a non-editable `<p>` is precisely what a cross-block
      selection spans — so `handle_mouse_move` in azul-core returned early on
      `drag_start_position?` and no drag change was ever built for plain text.
      FIX: arm on any SELECTABLE TEXT — contenteditable, or a text-bearing node that
      `is_text_selectable` accepts (so `user-select: none` is honoured, using the engine's own
      predicate). The guard the narrow rule stood in for is kept and made explicit instead of
      incidental: a press inside a window DRAG REGION never starts a selection, via the existing
      `node_is_window_drag_region`.
      TRAP: the hit test names the BLOCK (`<p>`), not the text run inside it — a text node
      carries no tag of its own — so the "is this text" test has to look at the node AND its
      children. Checking only `NodeType::Text` on the hit node matched nothing and the test
      still failed.
      EVIDENCE: azul-dll 1944 (the new caret test included), azul-layout 7555, azul-core 2749,
      host check and 8-target gate green.
- [x] PLACEHOLDER-ON-FOCUS: the headless test encoded a SUPERSEDED ruling and could never have
      passed. Two independent problems in one assertion, neither of them an engine bug.
      1. It looked for a `__azul-native-text-input-placeholder` NODE. The
         placeholder-as-engine-attribute refactor deleted that node; the class now appears
         NOWHERE else in the workspace, so `rects_by_class` could only ever return `[]`
         whatever the engine painted.
      2. It demanded the prompt stay visible on focus. That is the 2026-08-21 reading, from
         before the strut caret existed. The CURRENT rule is the dated 2026-08-31 ruling
         recorded on `focus_writes_no_placeholder_css_at_all`: "focusing an empty field HIDES
         its placeholder - the rule TextArea always had. The empty-editable strut caret marks
         the focused field, so the old blank-field concern is gone."
      The engine agrees with the ruling and says so twice — `maybe_paint_placeholder_prompt`
      documents itself as painting for an "EMPTY, unfocused" host and early-returns on
      `is_focus_within_or_above`, whose own doc calls it "the engine placeholder's hide rule" —
      and two widget tests pin it. Measured to be sure: before focus the field paints 1 Text
      item with glyphs, after focus 0, and the caret is painted at 1x13.2 @ (31,30). So the
      product behaves as ruled and the report the test came from (a BLANK focused box) is fixed
      by the CARET, not by the prompt.
      Updated the assertion to the current rule and corrected the test's header comment, which
      still described the superseded one — the file contradicted itself.
      EVIDENCE: azul-dll headless failures 2 -> 1. azul-core 2749, azul-layout 7555, host check
      and 8-target gate green.
- [x] THE HEADLESS SUITE IS GREEN: 8 failures -> 0, and the whole workspace with it.
      The last one was the third victim of the same deleted class:
      `the_patched_display_list_equals_the_wholesale_build_for_the_widgets_scene` watched
      `__azul-native-text-area-placeholder` as a stability witness, and the
      placeholder-as-engine-attribute refactor deleted that node — the class appears NOWHERE in
      the workspace, so the lookup could only ever return `[]`.
      Dropped it from the watched tuple rather than replacing it: the prompt is PAINTED by the
      engine, and this test's own `assert_builders_agree` compares the WHOLE display list
      between the patched and wholesale builders, so the prompt's glyphs are already checked
      directly — stronger than watching the box that used to contain them.
      FINAL TALLY for the eight, which were NOT one bug: three were `Click` never being
      synthesized (a coincidence-based rule no backend had to satisfy); two were planning never
      probing Component/Application filters (every lifecycle callback in every app dead); one
      was `TextSelectionDrag` having a handler and no producer; two were tests encoding
      superseded rulings or deleted classes.
      Workspace: azul-css 2856, azul-core 2749 + 21 integration targets, azul-layout 7555,
      azul-dll 1943, azul-doc 206 — all green. Host check and 8-target gate green.
- [x] FLAKE FIXED: `malloc_heap_bytes_actually_tracks_live_heap` raced the rest of the suite.
      The probe is PROCESS-WIDE and the test runs beside every other test in the crate, so a
      neighbour freeing more than the 8 MiB ballast inside the measurement window drove `during`
      BELOW `before` — an observed "before=178012256, during=177253248", a NET DROP across an
      8 MiB allocation. Intermittent at first, about one run in two once the suite passed 7 500
      tests. RETRIED (8 attempts) rather than loosened: our own 8 MiB is deterministic and the
      churn is not, so a genuinely broken probe fails every attempt while a noisy window costs
      one. Verified in both directions — three consecutive green full-suite runs, and with
      `during` pinned to `before` all eight attempts fail and the panic lists them.

