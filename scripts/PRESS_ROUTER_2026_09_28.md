# One press router in `LayoutWindow` (PR #476, part A) - 2026-09-28

Branch `wt/press-router`, based on `a1985a456`. Implements §3 A / §4 step 4 of
`scripts/SELECTION_WHEN_CLIPPED_ARCHITECTURE_2026_09_26.md`. Nothing was compiled or
run (per instructions). Every claim about behaviour below comes from reading the code.

## What it is

Before: the scrollbar-vs-content decision lived in each shell, before the shared event
pass (`perform_scrollbar_hit_test` → `handle_scrollbar_click` / `handle_track_click` /
`handle_scrollbar_drag` in the dll's `common/event.rs`). A scripted press
(`DebugEvent::MouseDown`, `click`, any `modify_window_state`) went through the
`ModifyWindowState` arm straight to the event pass, and the layout E2E runner had no
scrollbar routing at all. So no test and no AZ_E2E script could reproduce a press that a
scrollbar took on a device.

After: `layout/src/press_router.rs` holds the one arbiter, as methods on `LayoutWindow`:

| Method | What it does |
|---|---|
| `route_press(point, button, now) -> PressTarget` | `Scrollbar(ScrollbarHit)` or `Content`. Hit-tests the bars (`ScrollManager::hit_test_scrollbars`, unchanged) and **acts**: a press on the thumb starts a drag, a press on the track pages or jumps (the OS `track_click` preference from `LayoutWindow::system_style`), an arrow button steps one line. Only `MouseButton::Left` operates a bar. A Left press while a thumb is still held (so its release was lost) ends that stale drag first. |
| `route_move(point, now) -> bool` | While a thumb is held, it scrolls that thumb's box. `true` means the move is the scrollbar's. |
| `route_release(button, now) -> bool` | A Left release lets go of the held thumb. |
| `scrollbar_drag() -> Option<&ScrollbarDragState>` | The held thumb. This is the existing `currently_dragging_thumb` field, now `pub(crate)`. `remap_node_ids` already carries it across a DOM rebuild. The dll's own copy (`CommonWindowState::scrollbar_drag_state`) never had that. |
| `route_pointer_transition(&mut baseline, &current, now) -> bool` | The same arbitration for a pointer **state push**. Whatever the scrollbar takes (the primary button and the cursor) is written into the event-diff baseline, so the pass does not also turn the press into a `MouseDown` on the content. This is the scripted counterpart of the shells' `discard_input_delta`. |

The thumb-drag and track-click arithmetic moved over from the dll unchanged: the
`(track - thumb)` ratio, the 0.9 page, the `get_scroll_node_info` VirtualView rule, and
the arrow step. Only the arrow step is now a named constant,
`press_router::SCROLLBAR_ARROW_STEP_PX`, pinned equal to the dll's
`WHEEL_SCROLL_PIXELS_PER_LINE` by a dll test.

### Why the logic moved to layout (not just a query)

- The E2E runner (`layout/src/e2e/runner.rs`) cannot link the dll. If the router were
  only a query in layout, with the acting half left in the dll, the runner would need a
  second copy of the drag and track arithmetic. That duplication is exactly the fork
  pattern the runner's un-fork pins exist to prevent.
- The drag state belongs on `LayoutWindow`. `currently_dragging_thumb` already existed
  there, unused, and it is remapped at reconciliation. The dll copy was not remapped.
- The dll keeps only platform work: recording the button and cursor in its
  `CommonWindowState`, the sanctioned swallow, `SetCapture`/`ReleaseCapture`, redraw and
  result fan-out.

### dll surface

Three shared `PlatformWindow` entry points. Every backend calls them.

- `route_pointer_press(pos, button, site) -> Option<ProcessEventResult>`: runs the router.
  On a scrollbar hit it records the button and cursor
  (`apply_pointer_button_state`) and calls `discard_input_delta`. It replaces
  `handle_scrollbar_press`.
- `route_pointer_move(pos, site) -> Option<…>`: while a thumb is held, it runs
  `route_move`, records the cursor and swallows the delta.
- `end_scrollbar_drag(pos, button, site) -> Option<…>`: reimplemented over
  `route_release`. It clears the button and swallows the delta.
- `route_pointer_transition(&mut baseline, &current) -> bool`: the dll wrapper (it
  supplies `now`), used by the `ModifyWindowState` and `QueueWindowStateSequence` arms.

Call sites:
- macOS: `handle_mouse_down`, `handle_mouse_move`, `handle_mouse_up` (unchanged call).
- Windows: `WM_LBUTTONDOWN`, `WM_MOUSEMOVE`, `WM_LBUTTONUP` (unchanged call).
- X11: `handle_mouse_button` press and release, `handle_mouse_move`.
- Wayland: `handle_pointer_button` press and release, `handle_pointer_motion`.
- Headless: the `run()` `MouseMove`/`MouseDown`/`MouseUp` arms and the test helper
  `step()`.
- Scripted: the dll `ModifyWindowState` and `QueueWindowStateSequence` arms, plus the
  runner's ports of both.
- `layout/src/e2e/full.rs` `DebugEvent::MouseDown`: only the comment changed. The op is
  a state push, and the host arm routes it. Putting the routing at the host arm, not in
  the op, also covers `click`, `double_click`, the drag ops and any app callback.

## Commits (in order)

1. `85fa66ef0` **test(e2e): RED**, `e2e::runner::tests::a_scripted_drag_on_a_scrollbar_thumb_scrolls_the_box_like_a_physical_drag`.
   - Setup: a 200x100 box with a CLASSIC bar (`-azul-scrollbar-visibility: always;
     scrollbar-width: auto`, so the test is independent of the presence fix). It reads
     the thumb centre from the ScrollManager, then runs
     `mouse_move` / `mouse_down` / `mouse_move(+20px)` / `mouse_up`.
   - **Expected RED:** the last assert `(offset.y - expected).abs() < 0.5`. Today
     `offset.y == 0.0`; expected ≈ 120 (20 / (100 − 16.7) × 500).
2. `3117597ac` **feat(layout): the router + runner routing.** Adds
   `layout/src/press_router.rs`, `pub mod press_router`, and makes the field
   `pub(crate)`.
   - The runner's `ModifyWindowState` and `QueueWindowStateSequence` arms route the
     primary pointer.
   - RED 1 goes green. The fix commit adds one assert: the release let go of the thumb.
   - It also adds 11 unit tests in `press_router::tests`: press beside a bar, thumb grab,
     non-primary press, drag ratio, clamp and return, move without a drag, primary-only
     release, track page (90 px), arrow step (20 px), pushed press/move/release, and
     pushes that stay in the delta.
3. `afd28bbc4` **test(dll): RED**, `desktop::shell2::headless::tests::a_scripted_press_on_a_scrollbar_thumb_drags_it_like_a_physical_press`.
   - Setup: two identical headless windows with the same classic bar. One gets the
     gesture through `HeadlessEvent`s (physical), the other through `ModifyWindowState`
     pushes (scripted).
   - **Expected RED:** `assert!(scripted_held, …)`. Today the scripted press leaves
     `get_scrollbar_drag_state()` as `None` (false); expected `Some` (true), as for the
     physical press.
   - The next assert would also fail: scripted scroll 0.0 against physical ≈ 120.
4. `32c2aef68` **feat(dll): every shell and the scripted path go through the router.**
   - Removed: `perform_scrollbar_hit_test`, `handle_scrollbar_click`,
     `handle_track_click`, `handle_scrollbar_drag` and `gpu_scroll` (it had no other
     callers).
   - Added: the three entry points plus the transition wrapper; all shells rerouted.
   - RED 2 goes green.
   - The two existing press/release unit tests now seed a real scroll box instead of a
     hand-made `ScrollbarHitId`.
   - New unit tests: a press beside the bar is left to the caller; a move with the thumb
     held records the cursor and swallows the delta; the arrow step equals one wheel
     line.
5. This report.

## Public type / field changes (for the api.json autofix)

`grep` of `api.json` finds none of these types (`ScrollbarDragState`,
`ScrollbarHitId`, `ScrollbarHit`, `PressTarget`), and `LayoutWindow` methods are not
exported. **No autofix is expected.** Listed anyway:

- New in `azul_layout`:
  - `press_router::PressTarget { Scrollbar(ScrollbarHit), Content }` with `is_scrollbar()`
  - `press_router::SCROLLBAR_ARROW_STEP_PX`
  - `LayoutWindow::{route_press, route_move, route_release, scrollbar_drag, route_pointer_transition}`
- Changed in `azul_layout`: `LayoutWindow::currently_dragging_thumb` is now `pub(crate)`
  (it was private).
- `azul-dll` `PlatformWindow`:
  - Removed: `perform_scrollbar_hit_test`, `handle_scrollbar_click`,
    `handle_scrollbar_press`, `handle_track_click`, `handle_scrollbar_drag`,
    `gpu_scroll`, `get_scrollbar_drag_state_mut`, `set_scrollbar_drag_state`.
  - Added: `route_pointer_press`, `route_pointer_move`, `route_pointer_transition`.
  - Kept: `get_scrollbar_drag_state`, which now delegates to `LayoutWindow`.
  - Removed: the `CommonWindowState::scrollbar_drag_state` field.

## Least sure to compile

1. **The `impl_platform_window_getters!` macro's `get_scrollbar_drag_state`.** It becomes
   `self.$field.layout_window.as_ref().and_then(|lw| lw.scrollbar_drag())`. It expands
   into 8 impls, 6 of them cfg'd (ios, android, macos, windows, x11, wayland plus the
   popup). It keeps the `ScrollbarDragState` imports in those files used, so no import
   was touched.
2. **The headless test `step()`** has
   `release_at.is_some_and(|p| PlatformWindow::end_scrollbar_drag(&mut *window, p, button, …).is_some())`.
   The closure captures `*window` (a `&mut HeadlessWindow` parameter) mutably, and
   `window` is used again after it.
3. **UFCS with an implicit reborrow**, e.g. `PlatformWindow::route_pointer_press(window, …)`
   where `window: &mut HeadlessWindow`, in `step()` and `push_pointer()`. The old code
   did the same thing with `handle_scrollbar_drag(window, …)`.
4. **`self.get_scrollbar_drag_state()?;` as a statement** in `route_pointer_move` and
   `end_scrollbar_drag`. The shared borrow must end before `get_layout_window_mut()`.
5. **`pub const fn scrollbar_drag(&self) -> Option<&ScrollbarDragState>`** uses
   `Option::as_ref` in a const fn (stable since 1.48).
6. **`press_router` tests** build `MouseState { cursor_position, left_down, ..MouseState::default() }`.
   This record update needs every field to be public; they all are, as far as I read.
7. **Runner `ModifyWindowState`:** `let mut old = mem::replace(…)`, then
   `route_pointer_transition(&mut old.mouse_state, &current, now)` inside a `&& { … }`
   block, then `old` is moved under `anything_changed || pointer_to_scrollbar`.
8. **cfg(windows):** `route_pointer_move(&mut *window, …)` and
   `route_pointer_press(&mut *window, …)` follow the same pattern as the old
   `handle_scrollbar_drag(&mut *window, …)`. **cfg(linux):** X11 passes its local
   `button: MouseButton`; Wayland passes `mouse_button` and relies on the module-level
   `PlatformWindow` import (the old inner `use` lines are gone).
9. **`#[allow(clippy::float_cmp)]`** is on the `press_router` test module and on
   `a_scrollbar_arrow_step_is_one_wheel_line`, in case pedantic lints are on.

Runtime premises that the two RED tests guard with explicit asserts:
- The inline CSS gives a classic bar whose thumb centre (`track.y + button_size +
  thumb_offset + thumb_length/2`) hit-tests as `Thumb`.
- The box is the only registered state that has a vertical bar.

## Behaviour changes

1. **Only the primary button operates a scrollbar.** On macOS, X11 and Wayland a right or
   middle press on a bar used to grab the thumb, or page. It now goes to the content (the
   context menu, `MouseDown(Right)`). Windows and headless already behaved this way.
   **The user should confirm this.** A right-click on an element scrollbar now opens
   that element's context menu.
2. **Only a Left release ends a drag.** macOS, X11 and Wayland used to end it on any
   button's release. A right click during a left thumb drag now reaches the content.
3. **X11/Wayland now record the drag in the pointer state.** They record
   `left_down`/cursor on a bar press, and clear them on the release before continuing
   their release path (`scrollbar_stops_the_button_event` is unchanged). This is the
   "should adopt this too" from the old `handle_scrollbar_press` doc.
4. **Headless: no `MouseUp` after a thumb drag.** The press was recorded and swallowed,
   so the release is too. The event pass still runs. macOS and Windows never emitted
   one. I updated the docs of `scrollbar_stops_the_button_event` and its "THE LAW" test
   to say this.
5. **Every backend tracks the cursor during a thumb drag.** macOS, Windows and X11 used to
   leave `cursor_position` stale for the whole drag.
6. **A track or arrow press always answers `ShouldReRenderCurrentWindow`.** It used to
   answer `DoNothing` when the scroll node was gone.
7. **A Left press while a thumb is still held ends the stale drag first.**
8. **Scripted pointer pushes now reach the router.** A scripted press on a bar no longer
   dispatches `MouseDown` to the content. A pointer the scrollbar took gets no hover
   re-hit-test. A `ModifyWindowState` that changed nothing else runs no event pass.
9. **A DOM rebuild that unmounts the dragged box now ends the drag**, through the existing
   remap. Before, the dll drag kept a stale `NodeId`.

## Overlaps / touch points with the other agents

- **Scrollbar-presence agent**
  - I **call** `hit_test_scrollbars`, `get_scrollbar_state`,
    `calculate_scrollbar_states`, `get_scroll_node_info`, `set_scroll_position` and
    `begin/end_thumb_drag`, and change none of them. Their fix reaches every press path
    through `route_press` automatically, including whether a faded overlay bar is
    hittable.
  - My tests seed state with
    `ScrollManager::register_or_update_scroll_node(dom, node, rect, size, now, 12.0, 12.0, false, true)`
    in `window_with_a_scroll_box` (`layout/src/press_router.rs`) and
    `seed_a_scroll_box_thumb` (`dll/.../common/event.rs` tests). **If they change that
    signature, update these two call sites.**
  - `layout/src/solver3/scrollbar.rs` (their file) still mentions `handle_scrollbar_drag`
    in docs at lines 10, 135 and 1181. The logic is now
    `press_router::thumb_drag_target`.
- **Scroll-chain agent:** no code overlap. The router consumes window-space track rects,
  which their `ancestor_scroll_offset` shift produces.
- **ColorInput key agent:** my `common/event.rs` edits are all outside key handling:
  - the module doc (top)
  - the `scrollbar_stops_the_button_event` docs (~1102-1165)
  - the `CommonWindowState` field and init
  - the `impl_platform_window_getters!` scrollbar getter
  - the trait decls (~3887)
  - the `ModifyWindowState` and `QueueWindowStateSequence` arms (~5119-5360)
  - the removed `gpu_scroll` (was ~9005)
  - the new entry points (~12445-12565)
  - the tests (~14031-14245)
- **`layout/tests/a_press_on_an_overflowing_field_selects.rs` (other agent's file, not
  touched): swap its gate at integration.**
  - Replace the `hit_test_scrollbars(p).is_some()` gate with
    `lw.route_press(p, MouseButton::Left, now).is_scrollbar()`. Remember that
    `route_press` acts: it grabs or pages.
  - While `lw.scrollbar_drag().is_some()`, send moves to `lw.route_move(p, now)` and
    finish with `lw.route_release(MouseButton::Left, now)`.
  - A harness that pushes whole states can call
    `lw.route_pointer_transition(&mut baseline, &current, now)` instead.
  - `textinput_resize_selection.rs` `Harness::click` still bypasses the gate (M2).

## Still open

- **The scroll manager's own drag record goes stale after a rebuild.**
  `ScrollManager::thumb_drag` (the fade's "is this bar held") is not remapped at
  reconciliation. `LayoutWindow`'s drag is. If a rebuild renumbers or unmounts the
  dragged box, the manager's key is stale until the next Left press or release. This
  predates the change. Suggested fix: re-key or end it in `remap_node_ids`.
- **Unmount mid-drag:** later moves reach the content with `left_down` still true and no
  `MouseDown`. This is an edge case.
- **Focus or capture loss mid-drag** (`WM_CAPTURECHANGED`, X11 `FocusOut`, Wayland leave)
  still does not end a thumb drag. This predates the change.
- **Touch:** iOS and Android touch never consults scrollbars. Unchanged, and out of scope.
- **Windows right/middle buttons** do not call the router. It would answer `Content`
  anyway.
- **Mixed release policy:** macOS and Windows return early on a release that ended a
  drag. X11, Wayland and headless continue into their release path. Both leave the same
  observable state: the button is up, and there is no `MouseUp`.
