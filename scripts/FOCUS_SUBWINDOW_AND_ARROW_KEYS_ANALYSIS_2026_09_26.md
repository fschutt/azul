# Focus across the ColorInput picker sub-window + arrow-key semantics — analysis (2026-09-26)

Branch `fix/input-bugs-2026-09-19` (PR #476), HEAD 411e7c95f + the working tree.
This was an analysis only: nothing was compiled or run. Other sessions are editing
`dll/src/desktop/shell2/common/event.rs`, so its line numbers drift. Every `event.rs`
citation also quotes the comment anchor, which you can grep for.

User reports:
1. Focus is not cleared when it moves into the ColorInput picker sub-window.
2. The ring was on the colour gradient, but arrows and Ctrl+arrows did nothing. The ring
   on the main window's colour swatch stayed as well, so the two were out of sync.
3. Analyse the "focus ring vs sub-window" problem and the arrow-key default actions
   (spatial navigation).

---

## 0. Summary

A `<transient-window>` popup is a **separate OS window with its own `LayoutWindow`, its own
`FocusManager` and its own ring painter**. No component decides which of the two windows
currently owns the keyboard. The popup's content is a cloned subtree of the parent's DOM,
and the `RefAny` state is shared. The design keeps the parent's focus on the invoker
("focus never left the invoker"). At the same time, the popup autofocuses its first tab
stop and inherits the invoker's `:focus-visible` modality. So when a picker is opened
from the keyboard there are two focused nodes and two rings at once. The ring gate looks
only at `focus_is_visible` and never at window activation.

How keys reach the popup differs per backend:
- **macOS** and **Win32**: the OS makes the popup the key/active window.
- **Wayland**: the parent forwards keys to its one `active_popup`.
- **X11**: the popup is override-redirect and never gets input focus, and no code
  forwards keys to it. Every key lands in the parent. There, Down arrow runs spatial
  navigation from the swatch to the Slider underneath the popup, and the next Left/Right
  changes that Slider's value.

On X11 this fully explains report 2: the ring is on the gradient, the arrows go to the
main window, and the picker "does nothing". On macOS, Win32 and Wayland the engine path
into the popup looks correct. However, **no test drives an arrow key into a popup**.
The existing "arrow" test re-implements the arithmetic and never calls the handler. So
report 2 on those platforms is unconfirmed rather than disproven, and the first RED test
below settles it.

Report 1 (two rings, desync) reproduces on **every** platform. Nothing in the engine
reacts to window activation for focus indication, and `:backdrop` is dead code. There
are two "window focused" flags, and only Win32 writes the one the cascade reads.

---

## A. Focus across windows

### A1. What "the picker sub-window" is in the code

| Piece | Where | What it holds |
|---|---|---|
| `<transient-window>` node, child of the swatch | `layout/src/widgets/color_input.rs:276-290`, `:339` (`.with_child(popup)`) | the template. It is `display:none` in the parent (`layout/src/transient.rs:10-14`) and has no tab stops there (`focus_cursor.rs:486-534`, "dead parent-side copy") |
| `TransientWindowManager` (parent's `LayoutWindow`) | `layout/src/transient.rs:552-625` | the open set, `content_dom` ids (`transient_dom_id`, `:297`), `focus_before_open` (`:600`), `pending_focus_restore` (`:609`) |
| Parent-side measurement | `layout/src/window.rs:5236-5260` | "**Nothing is written into `layout_results`**: the popup is a separate window that lays the same subtree out itself" |
| The popup window | `dll/src/desktop/shell2/common/transient.rs:9-19, 217-275` | a full `WindowCreateOptions` of `WindowType::Menu`. Its layout callback returns `extract_subtree_as_dom(...)` (`core/src/transient.rs:516`), which clones `NodeData`, so every callback shares the same `RefAny` |
| Parent→popup mailbox | `common/transient.rs:77-131` | content, placement, `closed` / `dismissed`, and `focus_visible` (`:113`): "a popup is a separate OS window with its own `FocusManager`" |

**So focus is per `LayoutWindow`, one per OS window.** There is no global or app-level
focus owner, and nothing arbitrates between the parent's and the popup's `FocusManager`
(`layout/src/managers/focus_cursor.rs:51-100`). Seat focus (`seat_focus`, `:95-100`) is
also per window.

### A2. The exact path when a picker is opened from the keyboard

1. Tab walks to the swatch in the parent. `SetFocus{visible:true}` sets `focus_is_visible = true`
   (`event.rs` "KEYBOARD DEFAULT ACTIONS", ~12079).
2. Space → `ActivateFocusedElement` → synthetic click → `on_color_input_clicked`
   (`color_input.rs:1033-1045`) → `set_transient_window_open(transient, true)`.
3. The dll arm `CallbackChange::SetTransientWindowOpen` (`event.rs` ~6225, "FOCUS RETURN:
   remember...", ~6284-6300) records `(node, swatch, visible=true)` and **does not move
   the parent's focus**.
4. The next layout reconciles. `sync_parent` creates the popup and copies
   `focus_visible = lw.focus_manager.focus_is_visible` into the mailbox
   (`common/transient.rs:555`).
5. In its first `process_window_events`, the popup autofocuses its first tab stop, which
   is the plane: `SetFocus{visible: inherit}` (`event.rs` "AUTOFOCUS A POPUP", ~10398-10465).
6. Result: **the parent's focus is the swatch with `focus_is_visible=true`, and the
   popup's focus is the plane with `focus_is_visible=true`.** Both windows paint a ring.
   The ring gate is `if editing_active || !self.focus_manager.focus_is_visible`
   (`layout/src/window.rs:9029`, inside `apply_text_tweens`, `:8987`). It never reads
   `window_focused`.

### A3. What each backend does to the parent when the popup appears

| Backend | Popup gets keys? | Parent's `window_focused` | Evidence |
|---|---|---|---|
| macOS | yes: `AzulPopupWindow` answers `canBecomeKeyWindow = YES` and is shown with `makeKeyAndOrderFront` | false (`windowDidResignKey`) | `macos/mod.rs:156-179`, `:5971`, `:3239-3300` |
| Win32 | yes: `ShowWindow(SW_SHOWNORMAL)` activates the owned popup | false (`WM_KILLFOCUS`) | `windows/mod.rs:~1545-1552`, `:6163-6175` |
| Wayland | yes, by **forwarding**: the parent's `handle_key` sends every key to `active_popup.key_event` | leave(parent) then enter(popup) both reach the **parent** (the surface argument is ignored), so false and then true again | `wayland/mod.rs:3576-3617`, `:10228-10250`; `wayland/events.rs:3956-4008` (`_surface`) |
| X11 | **no**: override-redirect, no `XSetInputFocus`/`XGrabKeyboard` anywhere in `x11/`, events are routed by `event.any.window` | stays true | `common/transient.rs:416` (`x11_override_redirect = true`); `x11/mod.rs:3106-3127`; the code says so at `x11/mod.rs:8302` ("X11 never gives the input focus to (it is override-redirect)") |

### A4. Is there a notion of "window has OS focus" that suppresses the ring? No.

These are all falsifiable. Each item gives the input, what happens now, and what should
happen.

- **`window_focused` is invisible to focus indication.** Backends write it
  (macOS `:3213/:3250`, X11 `:5280/:5311`, Wayland `:5565/:6015`, Win32 `:6136/:6170`).
  The only consumers are the caret-blink pause (`event.rs` "pause the caret blink",
  ~11144) and drag cancel. Nothing clears or dims the ring, and `:focus` styling is not
  suspended.
  - Input: focus a button with Tab, then activate another app.
  - Now: the ring is still painted.
  - Expected: no ring while the window is inactive, the way browsers and OSes do it
    (HTML's "currently focused area" is null without system focus, so `:focus` and
    `:focus-visible` stop matching; AppKit, GTK and Win32 draw focus only in the
    key/active window). The ring comes back with the same modality on reactivation.
- **`:backdrop` is dead.**
  - `match_pseudo_state` reads `node_state.backdrop` (`css/src/dynamic_selector.rs:1416-1433`).
  - No code outside tests ever sets `StyledNodeState::backdrop`. A grep for writes finds
    only `core/src/styled_dom_test.rs`.
  - `DynamicSelectorContext::window_focused` is set from `flags.has_focus`
    (`layout/src/window.rs:5232`), but no matcher reads it.
  - Only Win32 writes `flags.has_focus` (`windows/mod.rs:6135/6169`). macOS, X11 and
    Wayland write the other flag, `window_focused`.
  - The titlebar's inactive dimming relies on `:backdrop` (`widgets/titlebar.rs:371-380`).
  - Input: a titlebar with `background_inactive`, window deactivated.
  - Now: it is not dimmed on any platform.
  - Expected: dimmed.
- **Both flags default to `true`** (`layout/src/window_state.rs:381`, `core/src/window.rs:1681`).
  A window the OS never focused, such as an X11 popup, believes it is focused, so the
  popup's FocusLost dismissal edge (`common/transient.rs:975-977`) can never fire there.

### A5. Where the parent's ring would be cleared, and why it isn't

The ring is emitted per display-list build from `(focused_node in this dom) && focus_is_visible`
(`window.rs:9020-9066`). Three routes could clear it:
- (a) the parent's focus moves: by design it doesn't (A2 step 3);
- (b) `focus_is_visible` becomes false: nothing does that when a popup opens;
- (c) an activation gate: none exists.

So it stays until the popup closes, focus is returned to the swatch (same node) and
nothing changes. **The desync is structural, not a missed repaint.**

---

## B. Arrow keys on the picker's plane / hue / alpha

### B1. What the widget installs
- The plane, hue and alpha bars are `TabIndex::Auto` tab stops with
  `EventFilter::Focus(FocusEventFilter::VirtualKeyDown)` → `on_plane_key` / `on_hue_key`
  / `on_alpha_key` (`color_input.rs:747-766, 779-807, 813-834, 852-873`).
- `nudge_hsv` (`:1183-1233`) steps 1%, or 10% when `ks.ctrl_down() || ks.super_down()`
  (`:1191`), calls `info.prevent_default()` (`:1218`, `:1229`) and commits through the
  same `publish` the drag uses.
- Focus-filter callbacks fire **only on the focused node**, without bubbling
  (`event.rs` `EventFilter::Focus(_) => {`, ~9219). Callbacks are dispatched before
  default actions (`dispatch_events_propagated`, ~11503), and default actions run only
  `if !prevent_default` (~11988). **Order and veto are correct.**

### B2. The default action for arrows on a focused non-text node
`default_actions.rs:228-311`:
- A text input (a node with `Focus(TextInput)`) gets no default.
- Otherwise `spatial-navigation-action` (`:507-564`) decides: `scroll` scrolls; `auto`
  runs a spatial search (`focus_cursor.rs` `Directional`, `:895-918`, `next_in_direction`
  `:2652`) and moves focus if a candidate exists, else scrolls; `focus` moves focus or
  does nothing.
- **Modifiers are ignored**: Ctrl/Alt/Cmd+arrow do exactly the same (`:229-311` never
  reads `ctrl_down`).

### B3. Why arrows "do nothing", per platform

- **X11 (certain, structural).**
  - Input: Tab to the swatch, Space, then Down.
  - Now: the popup shows a ring on the plane (inherited modality), but the key is
    delivered to the parent (A3). The parent's focus is the swatch, which has no key
    handler, so the default action runs spatial navigation Down. In AzWidgets the
    ColorInput sits directly above the Slider (`examples/azul-widgets/src/lib.rs:459-476`),
    so focus moves to the Slider, which is **hidden under the popup**. The next
    Left/Right changes the Slider's value (the Slider consumes arrows,
    `widgets/slider.rs`). Ctrl+arrow: same path. The picker never sees a key, and the
    hex/RGB fields in the popup cannot be typed into.
  - Expected: the keys drive the plane.
- **macOS / Win32 / Wayland (probably working, not proven).** The popup gets the key,
  its `focus_target` is the autofocused plane (`event_determination.rs:986-989`), the
  handler runs and prevents the default. No test proves this end to end:
  `arrow_steps_are_one_percent_and_ctrl_steps_are_ten` (`color_input.rs:3378`)
  re-implements `(s ± step).clamp(..)` and **never calls `nudge_hsv`**. The headless
  `e2e` runner does not create popup windows (`e2e/runner.rs:3203-3222`), and
  `dll/tests/transient_window_layout.rs` never sends a key to a popup.
- **Ctrl+arrow on macOS.** With the default Mission Control shortcuts, macOS takes
  Ctrl+←/→/↑/↓ (switch Space / Mission Control / App Exposé) before the app sees them.
  So "Ctrl = 10%" cannot work on stock macOS. `super_down()` (Cmd) is accepted but is
  not a convention for this.
- **Wayland risks** (unverified): `xdg_popup_grab` passes `pointer_state.serial`
  (`wayland/mod.rs:9680`), which may be an *enter* serial. For a keyboard-opened popup,
  the valid serial is the key's (`last_input_serial`). Strict compositors (Mutter)
  dismiss a popup whose grab serial is invalid. Also there is only one `active_popup`,
  and a second transient window replaces it (`:1847`).

### B4. The intended behaviour, as a specification
- The focused plane/hue/alpha owns **unmodified arrows and Shift+arrows**, and always
  calls `prevent_default`, so spatial navigation never runs from inside the picker.
  **Tab/Shift+Tab** leave the control (the popup's own tab order). **Escape** closes the
  popup and returns focus to the swatch.
- Plane: ←/→ saturation, ↑/↓ brightness. Hue and alpha: ←/↓ decrease, →/↑ increase.
  Step 1%.
- Large step (10%):
  - **Shift+arrow on all platforms**: the design-tool convention (Figma, Photoshop,
    Illustrator nudge 10× with Shift), and not reserved by any OS;
  - **PageUp/PageDown** (the WAI-ARIA slider "large step"; y-axis on the plane);
  - **Home/End** = min/max (x-axis on the plane);
  - keep Ctrl (Windows/Linux) and Cmd (macOS) as aliases. Do not rely on Ctrl on macOS.
- Alt+arrow and Cmd+←/→ should fall through, i.e. no `prevent_default`, so OS and app
  shortcuts survive.

---

## C. The whole class: every place focus or its indication can desync

Each item states the defect, its evidence, and whether it has been verified.

1. **Two focus owners, no arbiter.** The parent keeps the invoker focused while the popup
   autofocuses (A2). This produces two rings, or a parent ring that stays behind (report 1).
   Verified by code reading.
2. **Activation does not gate indication** (A4), on all platforms. Verified.
3. **`window_focused` vs `flags.has_focus`**: two flags, one written per platform, and the
   cascade reads the Win32-only one (A4). `:backdrop` is dead. Verified.
4. **Keyboard routing differs by backend** (A3). X11 never routes keys to popups, and
   Wayland has one popup slot plus a possibly invalid grab serial. Verified for X11;
   the Wayland serial issue is a risk.
5. **Escape on X11 has two effects.**
   - `process_transient_dismissal` → `dismiss_on_escape` (`common/transient.rs:1012`)
     runs at the start of the parent's pass (`event.rs:4851`). The same Escape KeyDown is
     not consumed there, so the parent's default `ClearFocus`
     (`default_actions.rs:219-226`) blurs the swatch.
   - The owed restore is paid at the top of the **next** `process_window_events`
     ("FOCUS OWED BACK BY A DISMISSAL", ~10353).
   - Input: Escape while the picker is open on X11.
   - Now: the app sees Blur and then Focus on the swatch, and there is a window with no
     focus until the next pass.
   - Expected: no blur. Popups on other backends consume their own Escape
     (`discard_input_delta`, `common/transient.rs`).
   - Verified by code reading.
6. **Opening through the `open` attribute never records `focus_before_open`.**
   `remember_focus_before_open` is only called in the `SetTransientWindowOpen` arm
   (`event.rs` ~6284-6300). `reconcile` (`layout/src/transient.rs:824-925`) and the
   attribute path record nothing, and the attribute is the documented primary API
   ("The app never touches a window. It toggles `open`", `core/src/transient.rs:15`).
   - Input: an app-driven `open=true` popup, then Escape.
   - Now: focus is not returned (and, per item 5, possibly cleared on X11).
   - Expected: returned to where it was.
   - Verified by code reading.
7. **Closing does not blur the popup's focused node.**
   - A popup window is destroyed with its `FocusManager`, and no Blur/FocusLost is
     dispatched to its focused node.
   - The picker's hex field commits in `with_on_focus_lost(on_hex_committed)`
     (`color_input.rs:898-901`).
   - Input: type `#00ff00` in the hex field, then click in the parent (light dismiss).
   - Predicted now: the colour does not change.
   - Expected: a light dismiss commits, like a browser `change` on blur. Escape may be
     treated as cancel.
   - Unverified; falsifiable with the first dll test below.
8. **Popup autofocus re-arms on every pass while focus is None** ("AUTOFOCUS A POPUP",
   `needs_autofocus`, ~10408).
   - Input: click a non-focusable spot in the popup (the preview).
   - Now: the click-to-focus route clears focus (~11970), and the next pass re-focuses
     the plane with the modality inherited at creation, so a pointer click can bring
     back a ring.
   - Expected: autofocus once per popup.
   - Verified by code reading.
9. **Tear-off re-creates the window** (`TransientWindowManager::recreate`, `layout/src/transient.rs:928-939`).
   The new window autofocuses its first stop again, so focus inside the palette resets
   to the plane, and its modality is re-read from the parent's *current*
   `focus_is_visible` (`common/transient.rs:555`). Verified by code reading.
10. **Combobox and any "listbox" popup use the wrong focus model.**
    - Every transient popup autofocuses its first tab stop. For a ComboBox that is the
      first option (`combobox.rs:772-790`, `TabIndex::Auto`).
    - On macOS, Win32 and Wayland, keys now go to the popup. Typing into the combobox
      field while its list is open goes to a `<p>` with no TextInput handler.
    - WAI-ARIA combobox: DOM focus stays on the field and arrows move
      `aria-activedescendant`; the popup never takes focus.
    - The field's key handler knows only Backspace (`combobox.rs:934-960`), so arrow
      navigation of the list does not exist anyway.
    - Predicted, falsifiable (dll test: popup `focused_node` after opening a combobox).
11. **Headless runner diverges from the dll**: the e2e runner's
    `SetTransientWindowOpen` arm (`e2e/runner.rs:3206-3222`) does not record
    `focus_before_open`, so e2e scenarios cannot cover focus return. Verified.
12. **Spatial navigation ignores modifiers** (B2). Ctrl/Alt/Cmd+arrow on any focused
    non-text control moves focus or scrolls. Verified.
13. **No trace of key routing.** `AZ_FOCUS_TRACE` (`event.rs:180-192`) logs popup
    open/autofocus/restore, but not "which window got key X, focused node, prevented,
    default action". Report 2 was therefore not diagnosable from a device log.

Not found: a ring painted from a stale layout (the ring is recomputed per display-list
build inside its scroll frame, `window.rs:9036-9066`).

---

## D. Spatial navigation vs widget-owned arrows

**The rule, per CSS Spatial Navigation L1 and browsers.** Spatial navigation is the
*default action* of an arrow `keydown`. A focused control that uses arrows itself
(text fields, `<input type=range>`, `<select>`, composite widgets) takes them, and
`preventDefault()` cancels the navigation.
- `spatial-navigation-action: auto | focus | scroll` picks what a container does with
  unclaimed arrows.
- `spatial-navigation-contain` scopes the search.
- Composite widgets follow WAI-ARIA APG, and all of them are **one tab stop with a
  roving tabindex or aria-activedescendant**:

| Pattern | Arrows (owner = widget, prevent_default) | Large step / ends | Tab |
|---|---|---|---|
| slider | ←↓ −step, →↑ +step | PgUp/PgDn, Home/End | leaves |
| 2-D colour area (no APG pattern; de-facto) | x on ←→, y on ↑↓ | Shift+arrow / PgUp/PgDn / Home/End | leaves |
| radiogroup | move **and check** | – | leaves the group (one stop) |
| tablist | move between tabs (auto/manual activation) | Home/End | into the panel |
| listbox / tree | move the active option; tree ←→ collapse/expand | Home/End, typeahead | leaves |
| grid (date picker) | move the day | PgUp/PgDn month, Home/End week | leaves |
| spinbutton (NumberInput, time column) | ↑↓ ±step | PgUp/PgDn | leaves |
| combobox | focus **stays on the input**; ↓ opens and moves the active descendant | – | leaves; Esc closes |

**What the code does now.**
- Only ColorInput and Slider claim arrows through `Focus(VirtualKeyDown)` +
  `prevent_default`. TextInput/TextArea claim them because they count as "text input"
  in `default_actions.rs:433-452`.
- RadioGroup rows (`radio_group.rs:497`), Segmented items (`segmented.rs:415`), list
  rows (`list_view.rs:1082`), tree rows (`tree_view.rs:464`), date cells
  (`date_picker.rs:738/799/902`), pagination and stepper buttons are **each their own
  Tab stop with no key handler**. Arrows run generic spatial navigation: focus moves,
  but selection does not (radio, segmented, tabs deviate from APG), and Tab visits every
  row.
- The mechanism is right (callbacks first, then a vetoable default). The widgets don't
  use it yet, and the default ignores modifiers.

---

## E. Prioritised fix plan (each fix with the RED test that proves it)

**The first test settles what platform-independent facts we have.**

**P0-0 — pin the popup keyboard path (a test only).**
`dll/tests/transient_window_layout.rs`, new test
`an_arrow_in_the_picker_popup_moves_saturation_by_one_percent`:
1. Open the picker through `click_at` (as `a_drag_on_the_plane_keeps_following_the_pointer_outside_it` does).
2. Build the popup `HeadlessWindow` and run `process_window_events` once, which
   autofocuses the plane.
3. Set `current_virtual_keycode = Right`, run a pass, and read `ColorPickerData` through
   the plane's callback `RefAny`.
4. Assert that `hsv.s` rose by 0.01. Repeat with LShift/LControl held and assert +0.10.

If it passes, report 2 is backend routing (X11) and the macOS/Win32/Wayland reports need
a device trace (P0-4). If it fails, the engine path is broken and becomes P0-1.
Blast radius: none.

**P0-1 — window activation gates focus indication (report 1, all platforms).**
- Keep focus, hide indication. Paint the ring only when
  `focus_is_visible && window_focused` (`window.rs:9029`), and regenerate the display
  list when `WindowFocusIn/Out` fire while the ring is visible (the `visibility_changed`
  path in the `SystemChange::SetFocus` handler shows how).
- Unify `flags.has_focus` / `window_focused`: every backend writes both, or one is
  derived from the other.
- Feed `:backdrop` by setting `StyledNodeState::backdrop = !window_focused` on restyle,
  or match `ctx.window_focused` in `match_pseudo_state`.

RED tests:
- `layout/src/window.rs` tests, next to the ring tests near `:26214`:
  `no_focus_ring_while_the_window_is_inactive`. Set a visible focus, set
  `window_focused=false`, build the display list, and assert there is no `Border` item
  with the accent colour; set it back to true and assert the ring returns.
- `dll/tests/transient_window_layout.rs`:
  `a_keyboard_opened_picker_leaves_exactly_one_ring`. Tab and Space on the swatch, then
  simulate the parent resigning (`window_focused=false`) and the popup activating.
  Assert the parent's list has 0 rings and the popup's list has 1, on the plane.
- `css`: `backdrop_matches_when_the_window_is_unfocused` (titlebar dimming).

Blast radius: small–medium. Every inactive window loses its ring (the intended platform
behaviour). Headless windows default to focused, so screenshots are unaffected.

**P0-2 — one keyboard-routing rule for transient popups (report 2 on X11).**
- Lift Wayland's forwarding (`wayland/mod.rs:3576-3617`) into shared code: the parent
  owns the physical keyboard, and while a focus-taking popup is open, keys and text are
  forwarded to that popup's window through the registry or mailbox.
- Alternatively, on X11, `XSetInputFocus(popup, RevertToParent)` after map (GTK grabs
  the keyboard for its popups). Forwarding is preferred because it works the same way
  for override-redirect windows and has no WM race.

RED test (headless and backend-neutral, once the decision lives in common code):
`a_key_the_parent_receives_while_its_picker_is_open_drives_the_picker`. Deliver
`Right` to the parent `HeadlessWindow` with the popup open, then assert that the
picker's saturation moved and the parent's `focused_node` is still the swatch.
Today it is RED: the parent runs spatial navigation.

Blast radius: medium (X11 and Wayland key paths, fallback menus).

**P0-3 — pick one focus model per popup kind.** Add a config field such as
`focus="move"|"keep"`, or derive it (role Dialog or tab stops → move; listbox/menu →
keep).
- *move* (picker, date picker): the invoker keeps `document.activeElement`-style focus
  but is not ringed while its popup holds the keyboard (this follows from P0-1 once the
  popup is the active keyboard window), and focus returns on close.
- *keep* (combobox): no popup autofocus; the parent forwards arrows to the list as an
  active descendant.

RED test: `a_combobox_list_popup_does_not_take_focus`, asserting the popup's
`focused_node == None` after it opens. RED today (C10).
Blast radius: medium (combobox, date picker, time picker, menus).

**P0-4 — trace key routing.** Extend `focus_trace!` at KeyDown dispatch to log: window
id and `is_popup`, the key, focused node, prevented, and the default action. This makes
the macOS/Wayland question in B3 answerable from one device run. No test needed.
Blast radius: none.

**P1-5 — ColorInput key semantics (B4).**
- Shift = coarse; PgUp/PgDn/Home/End; ignore Alt; only `prevent_default` handled keys.
- Replace the tautological test with ones that drive `nudge_hsv` through the existing
  `with_info` harness (`color_input.rs:1761-1823`) using a real `KeyboardState`:
  `shift_arrow_is_the_coarse_step` (RED today), `page_down_darkens_by_ten_percent`
  (RED), `home_goes_to_zero_saturation` (RED), `alt_arrow_is_not_consumed` (RED).

Blast radius: this widget only.

**P1-6 — X11 Escape double effect (C5).** The parent should consume the Escape that
dismissed a popup (`discard_input_delta` or `prevented`).
RED test in the dll file: open the popup, send Escape to the **parent**, and assert no
Blur was dispatched to the swatch and its focus is unchanged in the same pass.
Blast radius: small.

**P1-7 — focus return for attribute-opened popups (C6).** Record `focus_before_open` in
`reconcile` when a window opens, not only in the callback arm. Mirror the dll arm in the
e2e runner (C11).
RED test: `layout/src/transient.rs` `manager_tests` — a popup opened by `wanted` and
then dismissed yields a `pending_focus_restore`.
Blast radius: small.

**P1-8 — blur-commit on popup close (C7).** Before a popup window closes because of a
light dismiss, dispatch Blur to its focused node.
RED test: type a hex value in the popup, press outside in the parent, and assert the
picker colour equals the typed hex.
Blast radius: small–medium (every popup close).

**P1-9 — Wayland: route `wl_keyboard.enter/leave` by surface** (the parent becomes
inactive when the popup has the keyboard) and use `last_input_serial` for
`xdg_popup_grab`.
RED test: a pure unit test of an extracted
`keyboard_focus_target(surface, parent, popup)` helper.
Blast radius: Wayland only.

**P2-10 — spatial navigation only on unmodified arrows** (C12).
RED test in `default_actions.rs` tests:
`ctrl_or_alt_arrow_on_a_button_has_no_default_action`.
Blast radius: small.

**P2-11 — autofocus once per popup** (C8), using a flag in the mailbox.
RED test: click a non-focusable spot in the popup and assert that no ring comes back on
the next pass.

**P2-12 — APG roving tabindex for RadioGroup / Segmented / Tabs / ListView / TreeView /
DatePicker grid.** The group is one tab stop, and arrows move and select. There is one
RED test per widget (Tab from the item before → item after the group skips its other
rows; → checks the next radio).
Blast radius: large; do it after P0/P1.

---

## Open questions

1. Which platform was yesterday's arrow-key test on? X11 is fully explained. On macOS,
   Win32 or Wayland, P0-0 and P0-4 decide.
2. Is Escape in the picker a *cancel* (restore the colour from when it opened) or just a
   close? This matters for P1-8.
3. Should an inactive window keep a dimmed ring (some GTK themes do) instead of none?
   The proposal is none, the browser and AppKit behaviour.
4. For a torn-off palette (a long-lived toplevel), is the swatch still its "invoker" for
   focus return, or does closing the palette leave focus where the user last was?
