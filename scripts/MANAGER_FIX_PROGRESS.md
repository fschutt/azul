# Manager-wiring fix arc — progress + control file

STATUS: ACTIVE (phases A → B → C, then D compile gate)
Branch: `fix/manager-wiring` (NO pushes; merge decision is the user's)
Checklist source: `scripts/MANAGER_WIRING_AUDIT_2026_07_03.md` (audited @30162fa9f — line numbers drift, ALWAYS re-locate by symbol)
Raw audit JSON (deep detail, may not survive reboot): `/private/tmp/claude-501/-Users-fschutt-Development-azul-mobile/896d2157-9818-4f1d-b5df-2f21b1ce94a9/tasks/wkn87j5an.output`; per-agent journal: `/Users/fschutt/.claude/projects/-Users-fschutt-Development-azul-mobile/896d2157-9818-4f1d-b5df-2f21b1ce94a9/subagents/workflows/wf_47ed3a06-26b/journal.jsonl`

## Rules (binding for every tick)
1. NO compilation in phases A–C: no cargo build/check/test/run, no azul-doc, no rustc. Code + tests + rg/Read only. Phase D lifts this.
2. Re-locate every fix site by SYMBOL from the audit, not line number. If the code materially changed or is already fixed → mark item OBSOLETE + note, move on.
3. Fix each item across ALL backends it names before taking the next item.
4. Tests: unit tests for platform-independent logic (layout managers, core event logic). Headless E2E only for paths headless actually supports (land MWA-A4 first). Never write a test that cannot work without an OS; record test-debt in the item note instead.
5. Public API changes: edit api.json ONLY (never touch generated bindings, never run generators now); record under API-STAGED. Codegen via the azul-doc autofix workflow happens in Phase D (memory rule: autofix only, no hand-curated patches).
6. Commit per item: `fix(<area>): <summary> [<item-id>]` ending with `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`. Update this file's item status + short sha in the same or an immediate follow-up commit. Touch `scripts/.manager_fix.lock` after each commit (heartbeat).
7. Never push. Never commit on master.
8. Blocked? Mark BLOCKED + one-line question here; continue with the next item. DECISIONS below are standing answers — do not re-ask them.
9. Behavior you cannot runtime-verify without compiling (esp. Wayland/X11/Windows): implement per protocol/API docs and tag the item NEEDS-RUNTIME-VERIFY; Phase D's smoke list picks these up.
10. Lock protocol: on start, if `scripts/.manager_fix.lock` exists with mtime < 45 min → another tick is active, exit silently. Else write timestamp+pid to it. Remove it on clean exit.

## DECISIONS (defaults are ACTIVE until the user overrides — override by editing this section or telling the interactive session)
- D1 SCOPE — wiring-only. Make existing paths honest: events synthesized, errors surfaced, loop woken, capability reports truthful. NO net-new OS platform backends (WinRT Geolocator, Windows Hello, ashpd portals, fprintd, OS drag-source protocol impls that amount to new subsystems) — collect those under FOLLOW-UPS. [DEFAULT — pending user confirm]
- D2 macOS titlebar — use `performWindowDragWithEvent:` from the titlebar node's mouse-down path (OS-smooth, snapping, correct on every monitor); keep the manual-math path only as fallback/programmatic move, but FIX it anyway (Y-flip must use screen frame origin; carry fractional residual; restore-before-drag when maximized; no 5px threshold for window-drag). [DEFAULT]
- D3 Capability pump — ONE shared lazily-started pump thread (first gamepad/sensor/geo/biometric/keyring subscription starts it; stops when unused) pushing into the existing process-global channels and waking the loop via each backend's existing waker (the WR frame-ready wake path). Drain moves from regenerate_layout to early process_window_events, synthesizes manager events, and requests an event pass. [DEFAULT]
- D4 Shortcuts — introduce a "primary modifier" (Cmd on macOS, Ctrl elsewhere) in core keyboard state; `KeyboardShortcut::from_key` keys off primary; Windows normalizes generic VK_CONTROL/VK_SHIFT/VK_MENU to sided variants (or GetKeyState sync). Native macOS Edit menu = separate item MWA-B14. [DEFAULT]
- D5 API staging — api.json edits allowed during A–C, generators deferred to Phase D. [DEFAULT]
- D6 End game — when A–C are exhausted the loop AUTO-ENTERS Phase D (compile gate) rather than stopping. [DEFAULT — please confirm]
- D7 Ordering — per-subsystem across all backends simultaneously (not per-OS passes). [DEFAULT]
- D8 a11y feature gating — wiring fixed under the existing `a11y` feature; whether it joins default features (binary size vs out-of-box a11y) = USER CALL. [OPEN]

## Phase A — architecture first (in order)
- [ ] MWA-A1 Capability pump + loop wake (pattern 1). Move gilrs poll out of regenerate_layout (common/layout.rs, sole call site, fn that polls gilrs ~L891); shared pump per D3; drain early in process_window_events; synthesize GamepadInput / sensor / geolocation / biometric / keyring / permission events + request an event pass (kills the +1-pass latency); make iOS/Android gamepad capability report honest (stubs currently claim supported=true, capability.rs). Unit tests: pump lifecycle, drain→event synthesis. STATUS: TODO
- [ ] MWA-A2 Primary-modifier + Windows VK normalization (pattern 2). core/src/events.rs `KeyboardShortcut::from_key` (Ctrl-only guard) → primary modifier; core KeyboardState gains is_primary_down (Cmd on macOS — currently arrives as LWin/super, macos/events.rs; Ctrl elsewhere); windows/win_event.rs maps generic VK_CONTROL/VK_SHIFT/VK_MENU to sided variants. Unit tests: from_key matrix under both platform conventions. Unlocks copy/cut/paste/select-all/undo/redo on macOS + Windows in one stroke. STATUS: TODO
- [ ] MWA-A3 Output-drain sweep — the confirmed "computed but never delivered" criticals (pattern 3):
  - a) Windows a11y: capture and `.raise()` the QueuedEvents returned by `update_if_active` (windows/accessibility.rs, currently `let _ =`); implement `set_focus` (currently `_has_focus` no-op).
  - b) X11/Wayland a11y: call `update_window_focus_state(true/false)` from X11 FocusIn/FocusOut and wl_keyboard enter/leave (x11/accessibility.rs set_focus stub has a false "automatically managed" comment).
  - c) hover: feed HoverChange into `restyle_on_state_change` (common/event.rs passes `None, // hover`) from the MouseEnter/Leave diff → incremental :hover restyle on every backend.
  - d) window activation: snapshot previous_window_state, flip window_focused, run process_window_events in macOS windowDidBecomeKey/ResignKey, X11 focus handlers, Windows WM_ACTIVATE, wayland keyboard enter/leave → WindowFocusIn/Out finally dispatch; blur pauses caret + dims selection (focus_cursor tie-in).
  - e) a11y incremental text updates: push the computed update (layout/window.rs, text-edit tree-update fn) to the active platform adapter (+ raise on Windows).
  STATUS: TODO
- [ ] MWA-A4 Headless input parity (test enabler). Feed record_input_sample sessions from headless MouseMove/Down/Up (headless/mod.rs mouse handlers only set mouse_state today); route wheel into ScrollManager; add simulate_file_drop; tick scroll easing in headless. After this, E2E tests for drag / double-click / auto-scroll / drop become writable. STATUS: TODO

## Phase B — confirmed big-ticket bugs (after A)
- [ ] MWA-B1 Horizontal wheel: add WM_MOUSEHWHEEL arm (windows/mod.rs wndproc) + X11 buttons 6/7 → handle_scroll(±x) (x11/events.rs button handling). STATUS: TODO
- [ ] MWA-B2 Nested scroll: innermost-first wheel target + chain leftover delta to ancestors (scroll_state.rs target resolution; scroll_timer.rs). Unit tests for chaining. STATUS: TODO
- [ ] MWA-B3 Pure-Wayland clipboard: real wl_data_source on copy + pipe-read of data_offer on paste (wayland/clipboard.rs is x11-clipboard/XWayland-only; wayland/events.rs selection handler is an empty stub); add primary-selection protocol; drop X11 dependency on pure-Wayland sessions. STATUS: TODO
- [ ] MWA-B4 Touch→gesture bridge: create per-touch-id input sessions from touch events on Windows/X11/Wayland (currently touch only fills window touch_state); macOS: implement magnifyWithEvent:/rotateWithEvent: → inject_native_gesture. Makes pinch/rotate detectable everywhere. STATUS: TODO
- [ ] MWA-B5 X11 CSD: set _MOTIF_WM_HINTS decorations=0 at window creation + in sync_window_state when WindowDecorations::None (x11/mod.rs) — kills double chrome under the fake titlebar. STATUS: TODO
- [ ] MWA-B6 Wayland decorations: honor flags.decorations in zxdg_decoration negotiation (wayland/mod.rs requests SSD unconditionally; events.rs discards the compositor's configured mode) — request client_side when CSD wanted; force CSD injection when the protocol is absent. STATUS: TODO
- [ ] MWA-B7 File drop completeness: target HoveredFile/DroppedFile/HoveredFileCancelled at the node under the drag position with root fallback (event_determination.rs targets root; event_type_to_filters only emits Hover — add proper filters); multi-file: FileDropManager stores a StringVec (currently single Option<AzString>), all 8 ingress sites pass the full list; API-STAGED: get_dropped_files; macOS/Windows use the OS-provided drag location for the hit test instead of the stale cached cursor (macos/events.rs dragging handlers; windows dnd.rs). STATUS: TODO
- [ ] MWA-B8 Drag auto-scroll: auto_scroll_timer_callback treats CursorPosition::OutOfWindow(pos) as valid coordinates (terminate only on button-up/drag-end — fixes X11 LeaveNotify kill + Windows WM_MOUSELEAVE-under-capture); StartAutoScrollTimer also pushed for active node-drags and OS file hovers (currently TextSelectionDrag only, core/src/events.rs); Windows scrollbar-thumb click branch gets SetCapture (windows/mod.rs returns before the SetCapture line). STATUS: TODO
- [ ] MWA-B9 macOS titlebar per D2: performWindowDragWithEvent: from titlebar mouse-down; fix manual path anyway (Y-flip uses screen().frame() height AND origin; fractional residual carried in gesture manager instead of `as i32` truncation; unmaximize-then-drag; exempt window-drag from the 5px threshold; handle screen()==None). STATUS: TODO
- [ ] MWA-B10 a11y scroll surface: advertise ScrollUp/Down/Left/Right + ScrollIntoView actions and scroll_x/scroll_y(+min/max) on scrollable nodes in the tree builder (managers/a11y.rs — the INBOUND handler at layout/window.rs is already complete); map the public AccessibilityInfo.supported_actions field in build_node (currently never read). STATUS: TODO
- [ ] MWA-B11 CSD resize edges on all 4 backends (wayland xdg_toplevel.resize(serial,edge); X11 _NET_WM_MOVERESIZE edge codes; Windows WM_NCHITTEST HTLEFT..HTBOTTOMRIGHT synthesis for frameless; macOS resizable styleMask / trackable edges) + double-click-titlebar maximize parity. STATUS: TODO
- [ ] MWA-B12 Long-press: real timer so a motionless 500ms press fires (no event pass happens today without motion); wire mark_long_press_callback_invoked (zero callers → re-fires every pass). STATUS: TODO
- [ ] MWA-B13 Wayland scroll direction: reconcile raw wl axis sign with the positive=up chokepoint (wayland/mod.rs axis handler). NEEDS-RUNTIME-VERIFY. STATUS: TODO
- [ ] MWA-B14 macOS native Edit menu (Cmd key equivalents wired to the same actions as A2) per D4. STATUS: TODO

## Phase C — full per-manager sweep (report §5 is the checklist; a manager is DONE only when EVERY gap in its section is FIXED, OBSOLETE, or SKIPPED-with-reason here)
- [ ] MWA-C-gesture STATUS: TODO
- [ ] MWA-C-scroll STATUS: TODO
- [ ] MWA-C-a11y STATUS: TODO
- [ ] MWA-C-file_drop STATUS: TODO
- [ ] MWA-C-drag_drop (D1 applies: intra-app DnD + honest errors; per-backend OS drag-SOURCE protocols likely FOLLOW-UPS) STATUS: TODO
- [ ] MWA-C-hover STATUS: TODO
- [ ] MWA-C-focus_cursor STATUS: TODO
- [ ] MWA-C-text_input STATUS: TODO
- [ ] MWA-C-text_edit STATUS: TODO
- [ ] MWA-C-clipboard (incl. rich-text styled_runs path documented in managers/selection.rs header; HTML clipboard format → API-STAGED if needed) STATUS: TODO
- [ ] MWA-C-undo_redo STATUS: TODO
- [ ] MWA-C-gamepad STATUS: TODO
- [ ] MWA-C-virtual_view STATUS: TODO
- [ ] MWA-C-gpu_state STATUS: TODO
- [ ] MWA-C-changeset STATUS: TODO
- [ ] MWA-C-permission (D1 wiring-only) STATUS: TODO
- [ ] MWA-C-geolocation (D1 wiring-only) STATUS: TODO
- [ ] MWA-C-biometric (D1 wiring-only) STATUS: TODO
- [ ] MWA-C-keyring (D1 wiring-only) STATUS: TODO
- [ ] MWA-C-sensors (D1 wiring-only) STATUS: TODO
- [ ] MWA-C-csd (whatever B5/B6/B9/B11 did not cover) STATUS: TODO

## Phase D — compile gate (auto-entered per D6, only when A–C exhausted)
1. `cargo check -p azul-layout -p azul-core` then `-p azul-dll --features build-dll`, then workspace — fix everything (a pile is expected; that is the deal).
2. Apply API-STAGED through the azul-doc autofix workflow (autofix commands only — never hand-curated patches), regen bindings, re-check.
3. `cargo test` workspace (unit + headless E2E incl. all tests written in A–C).
4. Walk the NEEDS-RUNTIME-VERIFY list; record per-OS smoke results here.
5. Set STATUS: DONE at the top, remove the lock, CronList → CronDelete the job named `manager-wiring-fixes`, write a final summary + merge recommendation for the user.

## API-STAGED (api.json edits awaiting Phase D regen)
(none yet)

## FOLLOW-UPS (out of scope per D1)
(none yet)

## Session log
(one line per tick: UTC time — items touched — commits — notes)
