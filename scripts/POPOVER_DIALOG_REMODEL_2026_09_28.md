# Popover -> HTML `<dialog>` remodel (2026-09-28)

Branch `wt/popover-dialog`, based on `5414bfa6b` (PR #476,
`fix/input-bugs-2026-09-19`).

**Nothing was compiled or run.** No cargo, rustc or rust-analyzer was used.
Every "today" value below is a prediction from reading the code. The parent's
single compile confirms or refutes it.

User report (2026-09-28): "popover cannot be closed again (entire thing should
be remodeled towards html dialog)".

## 1. Root cause

The demo builds `Popover::create(..).with_open(false)` on every layout, and its
`on_toggle` returns `RefreshDom`. The popover keeps its state in two places
that a rebuild pulls apart:

- **The open flag.** It lives in the trigger's callback payload
  (`PopoverStateWrapper`). Every build mints a fresh payload from
  `with_open(false)`, so after the app's rebuild the payload says "closed".
- **The panel's visibility.** A runtime `set_css_property(display: block)`
  override shows the panel. `migrate_user_overrides_from` carries that override
  across every rebuild, so the panel stays visible.

After the first click the panel is visible while the payload says closed. The
next click flips "closed" to "open" and writes `display: block` again. The
popover can never be hidden.

The Modal has the same class of bug in the other direction. Its close button
hides the backdrop with a `display: none` override, which also survives
rebuilds. A modal that the app later reopens with `with_open(true)` stays
hidden. The demo's Modal also had no way to be opened at all (`with_open(false)`
and no trigger).

## 2. Design

One core, `layout/src/widgets/dialog.rs` (`build_dialog(DialogParts)`), built
on the transient-window machinery. Nothing in that machinery is duplicated:
popups as real OS windows, one DOM, mailbox dismissal, focus return, autofocus
and key forwarding are all reused. `Dialog` is the new public widget.
`Popover` and `Modal` are front-ends over the same core.

```text
wrapper                      dataset = DialogData (+ merge callback)
 ├─ invoker (optional)       Click -> show / close
 └─ <transient-window>       Dismissed -> close event; KeyDown -> Escape;
     │                       (modal) LeftMouseDown -> backdrop light dismiss
     └─ panel                role Dialog, named by the title; dataset = DialogData
         ├─ title row        (title, or a spacer for the close button)
         ├─ content
         └─ close "x"        Click -> close(); LAST in tab order
```

| HTML | here |
|---|---|
| `open`, `show()` | `with_open`, `show()` (declarative, like the attribute). The invoker click is imperative. |
| `showModal()` | `show_modal()` = `with_modal(true).with_open(true)` |
| `commandfor` / `popovertarget` | `with_invoker(dom)`: a click shows the dialog; clicking again while it shows closes it |
| `close(v)` / `requestClose(v)` | `Dialog::close_from(info, node, v)` / `Dialog::request_close_from(..)`, called from a control inside the dialog |
| `returnValue` | `DialogState::return_value`; persists across rebuilds; reset when the dialog is shown again |
| `cancel` (cancelable) | `with_on_cancel`; `info.prevent_default()` keeps the dialog open |
| `close` event | `with_on_close`; always fires in the app's window |
| `closedby` | `DialogClosedBy {Auto, Any, CloseRequest, None}`; Auto is CloseRequest for modal, None for non-modal |
| `::backdrop` | the modal window's root: dim, flex-centred; `with_backdrop_style` |
| top layer | a real OS window above its parent. Modal: `TransientAnchor::Viewport` covers the parent exactly |
| inert background | modal: the parent is covered (pointer) and the dialog window holds the keyboard |
| focus in / back | the engine autofocuses the dialog's `autofocus` node, else its first tab stop, and returns focus on close |
| `role=dialog` + name | panel `AccessibilityRole::Dialog`, named by the title ("Dialog" if there is none) |

### Key decisions

- **No private open flag.** The widget asks the engine
  (`CallbackInfo::is_transient_window_open`). This removes the root-cause class.
  There are no runtime `display` overrides either.
- **State across rebuilds.** `DialogData` holds the return value, `closedby`,
  the callbacks and the legacy `DialogCompat` callbacks. It is the wrapper's
  dataset with a merge callback, so the engine's `repoint_orphaned_refanys`
  keeps all clones (callbacks, the panel's dataset) on one allocation. The
  panel's clone lets `close_from` find the state from inside the popup window.
- **Escape.**
  - The engine no longer handles Escape for dialogs: the policy is `None`, or
    `OutsideOnly` for a non-modal `closedby=any`.
  - A `Focus(VirtualKeyDown)` handler on the transient node answers it instead.
    That node is the root of the popup window, so every key bubbles there.
  - The handler runs `cancel`, then reads `info.is_default_prevented()`, then
    closes with `set_transient_window_open(root, false)`. This is the existing
    popup-side self-dismiss: it posts `dismissed` to the parent, and the parent
    fires `Dismissed`.
  - It then calls `prevent_default` so the Escape does no ClearFocus.
- **Light dismiss.**
  - Non-modal: the engine's outside press or focus loss (`OutsideOnly`). This
    path is not cancelable, like an HTML popover.
  - Modal: `LeftMouseDown` on the backdrop root, only outside the panel rect,
    only with `closedby=any`. It goes through the same cancelable close
    request.
- **Engine additions:**
  - `TransientDismiss::OutsideOnly`;
  - `TransientAnchor::Viewport` with `TransientPlacement::cover_viewport`,
    applied in the dll's `reconcile_transient_windows` with the parent's size;
  - `set_forced_open(false)` wins over an open attribute;
  - `CallbackInfo::{is_default_prevented, is_transient_window_open}`.

## 3. Commits (in order) and expected REDs

`chore(scripts)` checkpoint commits are omitted. Commits marked "stub" add the
API with today's behaviour, so their REDs are runtime failures, not compile
errors.

| Commit | Kind | Test -> today vs expected |
|---|---|---|
| 3ee347c7f | test | `the_widgets_demo_popover_closes_again_on_a_second_click` (dll/tests/transient_window_layout.rs): after the 2nd click, content shown = **true**, expected false |
| 164780263 | test | `closing_through_the_api_wins_over_an_open_attribute` (layout/src/transient.rs): `set_forced_open(n,false)` = false, expected true; reconcile `closed` = [], expected [the window] |
| e04967969 | fix | `set_forced_open(false)` also holds an OPEN window closed (edge-triggered like a dismissal) and returns true |
| 77fb2c217 | test (stubs) | `is_default_prevented_sees_this_callbacks_prevent_default`: false, expected true. `is_transient_window_open_reads_the_engines_popup_set`: forced / shown -> false, expected true |
| 6c2763fcd | feat | both queries implemented |
| 45de182f7 | test (adds the variant) | `an_outside_only_popup_leaves_escape_to_its_content_but_closes_on_an_outside_press`: Escape in popup -> `close_requested` **true** (expected false); outside press -> stays open (expected closed) |
| b30888b98 | fix | OutsideOnly in `popup_dismiss_cause`, `dismiss_on_escape`, `dismiss_outside_on_press`, the e2e runner's Escape port, and the web dismiss JS |
| 11377690a | test (stub) | `a_viewport_placement_covers_the_parent_window`: `anchor_rect` = (120,80,40,20), expected (0,0,800,600); size None, expected 800x600 |
| 0f418384c | feat | Viewport: `cover_viewport`, `resolve`/`resolve_within` (no flip/slide), dll reconcile maps it, Wayland positioner (TOP_LEFT/BOTTOM_RIGHT), web `position:fixed` |
| 7e12cc7fd | test (stubbed handlers) | dialog.rs: 10 runtime REDs, e.g. `escape_runs_cancel_then_closes_the_dialog` (cancel 0, expected 1; no close change, expected (window,false)), `close_from_...` (false, expected true), `a_rebuild_keeps_the_return_value...` ("" expected "ok"). dll e2e: Escape (cancels 0 / not closed), prevent-default (cancels 0), backdrop press with `closedby=any` (stays open), OK button `close_from` (stays open). Guards green today: closedby resolution, the structure tests, closedby=none/other keys, the forget-on-reopen merge rule. |
| a473f4d6a | feat | the real handlers |
| aec8782ea | fix | Popover = front-end over the core -> 3ee347c7f green; popover.rs tests rewritten |
| 4d7e46cfe | test | `a_modal_the_app_reopens_after_its_close_button_closed_it_shows_again` (dll): reopened modal shown = **false**, expected true |
| faf98ab34 | fix | Modal = front-end over the core -> 4d7e46cfe green; modal.rs tests rewritten |
| f20fd0bee | feat | AzWidgets demo on the new API |
| 821ebfd27 | refactor | drop the unused `Callback` imports |

For the combined RED pass (`git apply -R` per fix):

- **Pure fixes.** Revert these alone: e04967969, b30888b98, 0f418384c
  (the dll `cover_viewport` mapping only), aec8782ea and faf98ab34.
- **API-adding feats.** Keep these in: 6c2763fcd and a473f4d6a. They are
  stub -> real bodies, so reverting them makes their tests fail, as the REDs
  say.

## 4. Public API changes (for `azul-doc autofix`; api.json was NOT edited)

**New (module `widgets`, `external: azul_layout::widgets::dialog::*`):**

- **Types:**
  - `Dialog { dialog_state: DialogStateWrapper, title: String, content: Dom, invoker: OptionDom, show_close_button: bool, anchor: TransientAnchor, panel_style: OptionCssPropertyWithConditionsVec, backdrop_style: OptionCssPropertyWithConditionsVec }` (repr C);
  - `DialogStateWrapper { inner: DialogState, closed_by: DialogClosedBy, on_cancel: OptionDialogOnCancel, on_close: OptionDialogOnClose }`;
  - `DialogState { open: bool, modal: bool, return_value: String }`;
  - enum `DialogClosedBy { Auto, Any, CloseRequest, None }` (repr C, default Auto).
- **Callback families (via `impl_widget_callback!` + `impl_managed_callback!`):**
  - `DialogOnCancel { refany, callback }`, `OptionDialogOnCancel`,
    `DialogOnCancelCallback { cb, ctx }`, and
    `DialogOnCancelCallbackType = extern "C" fn(RefAny, CallbackInfo, DialogState) -> Update`;
  - the same shape for `DialogOnClose*`;
  - C exports `AzApp_setDialogOnCancelCallbackInvoker`,
    `AzDialogOnCancelCallback_createFromHostHandle(+Byref)`, and the same for
    Close.
- **`Dialog` methods:**
  - constructor `create(content: Dom)`;
  - setter pairs `set_/with_`: `title`, `content`, `invoker(Dom)`,
    `open(bool)`, `modal(bool)`, `closed_by(DialogClosedBy)`,
    `return_value(String)`, `close_button(bool)`, `anchor(TransientAnchor)`,
    `on_cancel(data, cb)`, `on_close(data, cb)`, `panel_style`,
    `backdrop_style`. The `set_*_style` variants take `Option...Vec`; the
    `with_*_style` variants take the Vec;
  - `show()`, `show_modal()`, `swap_with_default()`, `dom()`;
  - static `close_from(info: &mut CallbackInfo, node: DomNodeId, return_value: String) -> bool`;
  - static `request_close_from(info: &mut CallbackInfo, node: DomNodeId, return_value: String) -> Update`.
  - `From<Dialog> for Dom`.
- **`DialogClosedBy` const methods (optional in api.json):**
  - `resolve(modal)`;
  - `allows_close_request(modal)`;
  - `allows_light_dismiss(modal)`;
  - `transient_dismiss(modal)`.
- **`CallbackInfo`:**
  - `is_default_prevented(&self) -> bool`;
  - `is_transient_window_open(&self, node: DomNodeId) -> bool`.
- **New enum variants, appended last (repr C discriminants unchanged):**
  - `TransientDismiss::OutsideOnly` ("outside-only");
  - `TransientAnchor::Viewport` ("viewport").

**Not in api.json (internal):**

- `DialogData`, which has read accessors `return_value`, `is_modal` and
  `closed_by`;
- the `pub(crate)` items `DialogParts`, `DialogClasses`, `DialogCompat`,
  `build_dialog`, `default_backdrop_style` and the `on_dialog_*` handlers;
- `TransientPlacement::cover_viewport`.

**Unchanged in layout (no by-value ABI change):** `Popover`,
`PopoverStateWrapper`, `PopoverState`, `PopoverOnToggle*`, `Modal`,
`ModalStateWrapper`, `ModalState`, `ModalOnClose*`, and all of their methods.
Only doc text changed. api.json's `Popover::set_open` doc still says
"recomputing the panel style"; the autofix may refresh it.

**Demo binding names are guessed** (fix after codegen if they differ):

- `azul::widgets::{Dialog, DialogClosedBy, DialogState}` (through the
  `widgets::*` glob);
- `Dialog::create`, `.with_title(&str)`, `.with_invoker(Dom)`,
  `.with_modal(bool)`, `.with_closed_by(..)`, `.with_close_button(bool)`,
  `.with_on_close(data, fn)`, `.dom()`;
- `Dialog::close_from(&mut info, hit, "keep".into())`.

## 5. Least sure to compile

1. `dialog.rs` `const fn show / show_modal` chain two `const fn (mut self)`
   builders on a type that holds `Dom`. `Popover::with_open` has the same
   shape and compiles, but not chained.
2. `dialog.rs` `build_dialog`:
   - the conditional move of `title` into `AttributeType::Title(title)` after
     earlier `title.clone()`s;
   - `panel_style.unwrap_or_else(|| ..)`;
   - `backdrop_style.unwrap_or_else(default_backdrop_style)` (a fn item as
     `FnOnce`).
3. `dialog.rs` `on_dialog_backdrop_press`:
   `let (Some(cursor), Some(rect)) = (..) else { .. };`.
4. `dialog.rs` `request_close`:
   - `callback.invoke(refany, *info, state)` with `info: &mut CallbackInfo`;
   - `is_default_prevented` read afterwards on the same info.
5. `DialogCompat::notify`: `on_toggle.as_ref()` -> `Some(PopoverOnToggle { callback, refany })`
   with binding by reference, then `callback.invoke(refany.clone(), ..)`.
6. `callbacks.rs` `is_default_prevented`:
   - the `#[cfg(feature = "std")]` / `not(std)` pair;
   - the std body `(*self.changes).lock().is_ok_and(..)`.
7. Tests:
   - `use DialogClosedBy::{Any, Auto, CloseRequest, None as Never};` inside a
     test fn;
   - the generic `with_info(.., prepare: impl FnOnce(&mut LayoutWindow), f: impl FnOnce(CallbackInfo) -> R)`
     harness copies in dialog.rs, popover.rs and modal.rs (their field lists
     for `LayoutTree`/`DomLayoutResult` are copied from the old popover tests).
8. dll test helpers `with_probe` / `with_modal_probe`: `f(&mut guard)` relies
   on DerefMut coercion of the `RefMut` guard. `with_state` does the same.
9. `dll/src/web/html_render.rs` match arm with a block body, and the
   Wayland arm (not compiled on macOS).
10. The `layout/src/e2e/runner.rs` nested `matches!` in a match guard.
11. The demo's generated binding names (see §4).

## 6. Behaviour changes

- **Popover:**
  - It is now a real OS popup below its anchor, never clipped.
  - It closes on a second click, an outside press, focus loss, or Escape.
  - `on_toggle` also fires for the closes the user causes (outside press,
    focus loss, Escape).
  - `with_open` is declarative: a change of it opens or closes the popover.
  - `content_style` has no `display` any more.
  - The trigger is no longer its own tab stop, and no longer has the Tooltip
    role. The anchor, typically a Button, is the tab stop.
  - Focus moves into the panel (which is announced as a Dialog) and back out.
- **Modal:**
  - It is now a window covering its parent: top layer, background inert.
  - **Escape closes it.** `on_close` hears it.
  - Focus moves in and back.
  - `resolved_backdrop_style` no longer depends on `open`.
  - The class `__azul-native-modal` is now on the wrapper. The backdrop is
    the modal window's root.
- **Transient windows:**
  - `set_transient_window_open(node,false)` now closes an attribute-opened
    popup. That popup stays closed until the attribute goes false and true
    again. It also hands focus back for attribute-opened popups.
  - New `dismiss="outside-only"` and `anchor="viewport"`.
- **Demo:**
  - "Modal (starts closed)" is replaced by a modal Dialog with a "Delete
    file..." invoker, a title, a visible close button (x), and Keep / Delete
    buttons that close it with a return value. A status line shows how it
    last closed.
  - "Popover" is a non-modal Dialog with a visible close button and
    `closedby=any`.

## 7. Open items

- **Non-modal light dismiss is not cancelable.** The engine decides it before
  any callback runs. HTML popovers behave the same, but HTML dialogs with
  `closedby=any` fire a cancel.
- **Escape while the PARENT holds the keyboard.** This happens on macOS/Win32
  after the user clicks back into the parent. Escape then does not close a
  dialog: the engine policy is None, and the parent does not forward keys on
  those platforms (`popups_route_keys_natively`). X11 forwards keys, so it
  works there. Wayland's parent forwards to `active_popup`.
- **Modal keyboard inertness depends on the dialog window staying key.** On
  macOS the parent can be re-activated (Window menu, Mission Control, the
  native titlebar of a decorated window). Its keys then reach the parent.
  Consider a "modal" flag on the mailbox that makes the parent forward every
  key and refuse focus.
- **Invoker vs focus-loss race (macOS/Win32).** A click on the invoker while
  the popup is key: the popup's `FocusLost` (OutsideOnly) and the invoker's
  toggle race. The toggle can reopen a popover that the focus loss just
  closed. ColorInput has the same race.
- **The e2e runner (`layout/src/e2e/runner.rs`) never reconciles transient
  windows** and fires no `Dismissed`. Dialog behaviour is covered by the dll
  headless harness only.
- **Stylesheet support is missing:**
  - no `::backdrop` selector (style it through `with_backdrop_style`);
  - no `:open` / `:modal` pseudo-classes.
- **a11y gaps:**
  - `aria-modal` cannot be expressed (`AccessibilityInfo` has no modal
    state);
  - the invoker exposes no `aria-haspopup="dialog"` / `aria-expanded`;
  - the title is not linked as `labelled_by` (the name is copied instead).
- **The return value resets on each new showing** (invoker or `open`
  false->true). HTML keeps `returnValue` until the next `close(v)`. The
  deviation is deliberate: otherwise an Escape reports the previous OK.
- **Other widgets still keep private open flags:** ColorInput, DatePicker and
  ComboBox. They are mitigated by `Dismissed` plus a dataset merge. They could
  use `is_transient_window_open` too.
- **Web export (`html_render`).**
  - The viewport anchor maps to `position: fixed`.
  - The dialog's own Escape/backdrop handlers are Rust callbacks. It is
    unverified whether they run in the web build.
  - The web dismiss JS knows `outside-only`.
- **Old tests removed.** The old Popover (~40) and Modal (~60) unit tests
  exercised the removed display-override mechanism. They are replaced by
  smaller suites for the new model.
