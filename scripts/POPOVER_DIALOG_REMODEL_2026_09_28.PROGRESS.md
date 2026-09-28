# Popover -> HTML `<dialog>` remodel: progress checkpoint

Branch `wt/popover-dialog` (from `5414bfa6b`). No compilation allowed; RED
commit first, then fix. Commit this file after every commit. The last commit
deletes it and adds `scripts/POPOVER_DIALOG_REMODEL_2026_09_28.md`.

## Root cause (found)

The demo builds `Popover::new(..).with_open(false)` on every layout, and its
toggle callback returns `RefreshDom`. The trigger's payload
(`PopoverStateWrapper`) is minted fresh on each build ("closed"). The panel is
shown by a runtime `set_css_property(display: block)` override, and
`migrate_user_overrides_from` carries that override across the rebuild. So
the panel stays visible while the payload says closed. The next click flips
closed -> open and writes `display: block` again. It can never be hidden.

## Design (decided)

- The dialog is a `<transient-window>` (a real OS popup from the one DOM).
  Its state lives in a dataset + merge callback on the wrapper. `open` is read
  from the engine (`CallbackInfo::is_transient_window_open`), never from a
  private flag. No runtime CSS overrides.
- Modal = `TransientAnchor::Viewport`: the window covers the parent. Its root
  (the former transient node) is the `::backdrop` (dim, flex-centred). The
  panel sits inside. The parent is pointer-inert because it is covered.
- Non-modal = anchored `Bottom` to the invoker (the transient node's parent is
  the wrapper that shrink-wraps the invoker).
- `closedby` -> `DialogClosedBy {Auto, Any, CloseRequest, None}`. Auto is
  CloseRequest for modal and None for non-modal. Engine dismiss policy:
  non-modal + Any -> `TransientDismiss::OutsideOnly`; everything else ->
  `TransientDismiss::None`. The widget answers Escape itself (and the modal
  backdrop press), so the `cancel` step is cancelable.
- Escape: a `Focus(VirtualKeyDown)` handler on the transient node (it is the
  popup's root, so every key bubbles there). It runs `on_cancel`, then checks
  `info.is_default_prevented()`. If not prevented, it self-dismisses with
  `set_transient_window_open(root, false)` (the popup-side path posts
  `dismissed`). It also calls `prevent_default` (no ClearFocus). It maps
  `RefreshDom` -> `RefreshDomAllWindows` (it runs in the popup).
- Modal backdrop: `Hover(LeftMouseDown)` on the root. It closes only if the
  cursor is outside the panel rect (`get_first_child(root)`), and only for
  closedby Any.
- The x close button (last in tree order, absolutely positioned) calls
  `close()` with no value: no cancel, return value unchanged.
- `Dialog::close_from(info, node, return_value)` / `request_close_from(..)`
  are for the app's buttons inside the dialog. They find the panel's dataset
  (`DialogData`) by walking up.
- `on_close` always fires from the parent's `Dismissed` handler. Exception:
  the invoker's toggle-close (API close, no Dismissed) fires it directly.
- Focus: the engine already autofocuses (an `autofocus` attribute, else the
  first tab stop) and returns focus on dismissal. The panel role is Dialog
  -> `transient_takes_focus` is true.
- Popover and Modal become front-ends over the dialog core
  (`build_dialog(DialogParts)` + `DialogCompat::{Popover(on_toggle),
  Modal(on_close)}`).

## DONE

- 3ee347c7f test(popover): the demo popover closes again on a second click (RED, dll e2e)
- 164780263 test(transient): API close wins over an open attribute (RED)
- e04967969 fix(transient): `set_forced_open(false)` also holds an open window closed
- 77fb2c217 test(callbacks): `is_default_prevented` / `is_transient_window_open` (RED stubs)
- 6c2763fcd feat(callbacks): both queries implemented
- 45de182f7 test(transient): an outside-only popup leaves Escape to its content (RED; adds `TransientDismiss::OutsideOnly`)
- b30888b98 fix(transient): OutsideOnly in popup_dismiss_cause / dismiss_on_escape / dismiss_outside_on_press / runner / web JS
- 11377690a test(transient): a viewport placement covers the parent window (RED stub; adds `TransientAnchor::Viewport`, `cover_viewport` stub)
- 0f418384c feat(transient): Viewport implemented (resolve, resolve_within, dll reconcile maps `cover_viewport(window size)`, Wayland, web)
- 0b928f480 chore: this checkpoint file
- 7e12cc7fd test(dialog): dialog.rs with STUBBED handlers + 15 unit tests + 4 dll e2e tests (RED)
- 9a797a7b8 chore: checkpoint
- a473f4d6a feat(dialog): real handlers (invoker, Escape, backdrop, close button, Dismissed, merge, close_from)
- 70519f117 chore: checkpoint
- aec8782ea fix(popover): Popover is a front-end over the dialog core (fixes 3ee347c7f); unit tests rewritten; handlers pub(crate)

## IN PROGRESS

- Modal: the RED dll test first (the app reopens a modal after its x
  closed it -> it stays hidden), then Modal on the core.

## NEXT (in order)

1. (F1 done, registration done.)
2. (Popover done.)
3. Modal: a RED dll test first (a modal the app reopens after its x closed
   it stays hidden: the display override persists), then Modal on the core
   (modal=true, closedby Auto). Rewrite modal.rs tests.
4. Demo (`examples/azul-widgets/src/lib.rs`): the Dialog with an invoker
   button, a title, OK/Cancel via `Dialog::close_from`, the x button, and a
   status line showing the return value. The Popover gets
   `.with_close_button(true)`. Guess the generated binding names; list them.
5. Final report `scripts/POPOVER_DIALOG_REMODEL_2026_09_28.md` (design,
   commits + expected REDs, API for autofix, least-sure-to-compile spots,
   behaviour changes, open items); delete this file.

## API changes so far (for the autofix)

- `CallbackInfo::is_default_prevented(&self) -> bool`
- `CallbackInfo::is_transient_window_open(&self, node: DomNodeId) -> bool`
- `TransientDismiss::OutsideOnly` (appended)
- `TransientAnchor::Viewport` (appended)

## Open questions / notes

- An engine light dismiss (outside press / focus loss) of a NON-modal dialog
  is not cancelable: the engine decides it before any callback runs. HTML
  popovers are not cancelable either.
- When the parent holds the keyboard (macOS/Win32 after a click back into the
  parent), Escape in the parent does not close a closedby=closerequest dialog
  (engine policy None). X11 forwards keys to the popup, so it works there.
- Modal keyboard inertness depends on the popup keeping key focus (macOS: the
  user can still activate the parent via the Window menu or Mission Control).
