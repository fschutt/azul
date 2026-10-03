# Context menus and the SplitPane - fix report (2026-09-28/29)

Branch `wt/pointer-menus-splitpane`, from `5414bfa6b` (PR #476, `fix/input-bugs-2026-09-19`).
Nothing here was compiled or run (wave rule): every test result below is the EXPECTED one.

## 1. What was found

### Bug 1 - "the context menu does not open" (right-click, Control-click; the user wrote "cmd+click")

- **No regression in the macOS shell.** The right-button and Control-click path in
  `dll/src/desktop/shell2/macos/{mod,events}.rs` is byte-identical to the one verified on screen on
  2026-08-24 (`d2c826ac7`), except the new press router, and that is neutral: `route_press`
  returns `PressTarget::Content` for every non-left button, `end_scrollbar_drag` returns `None` for
  a right release, and `handle_mouse_down` still writes `right_down`. `rightMouseDown:`,
  `otherMouseDown:`, the Control-click latch (`ctrl_click_as_right`), `pending_context_menu` and
  `present_pending_context_menu` are all intact; this round's macOS diff is rustfmt plus the router.
- **Command + click is not the macOS secondary click.** Control + click, a right click and a
  two-finger trackpad click are. If the user only tried Command + click, no menu is the correct
  result. The 2026-08-24 session had the same mix-up. Ask the user to retry with Control + click.
- **If right-click really fails on the device,** the cause is in the engine's hit test, not in the
  shells. The one right-click-relevant change of this round is `7d7b25c0e` (the CPU hit tester
  sorts by paint order), plus the ScrollChain frames. The new guard
  `a_right_press_on_the_menu_box_of_a_scrolled_page_opens_its_menu` rebuilds the AzWidgets Menus
  card in a scrolled `overflow-y: auto` page and right-clicks the box's surface and its label
  through the real CPU hit tester. If that guard is red on the build, it is the reproduction.
- **Real defects found and fixed.** Each shell picked the menu's node its own way:
  - macOS: front-most hit, then a walk up its own dom.
  - X11 and Wayland: the highest `NodeId` of the LOWEST dom.
  - Windows: the first node with a menu in `NodeId` order, which is the OUTERMOST. It did not walk,
    and it skipped the whole MouseUp pass when a menu opened.

  No shell crossed from a `VirtualView` page (video, progress bar, virtualized list) into its host.
  So a right click on the demo's video, if it sat inside a box with a menu, opened nothing. The
  engine now makes one pick (`azul_layout::context_menu`), and every shell presents that answer.

### Bug 2 - the SplitPane is "pretty much unusable"

Six causes. The first two break every drag in the demo:

1. **The drag dies with the rebuild it asks for.** The demo's `on_resize` returns `RefreshDom` via
   `bump`, and the demo built the pane with a fixed `with_ratio(0.5)`. On the first move the app
   rebuilt the pane with a FRESH state `RefAny`: idle, at 0.5. The next move found no drag and the
   divider snapped back. The slider solved the same problem long ago with `merge_slider_state`; the
   SplitPane had no dataset or merge callback.
2. **Leaving the divider ended the drag.** Every event bubbles to the container, including the 6px
   divider's `MouseLeave`, and that leave was wired to the release handler. The first pixels of a
   drag leave the divider, because it follows the cursor only after the relayout.
3. **No pointer capture.** A drag past the pane's edge stopped, and its release went elsewhere.
4. **The divider lags the cursor.** The drag mapped the cursor with `delta / container` although
   the panes share `container - 6`, so the divider fell 3% behind the cursor on a 200px pane. The
   grab zone was centred on `ratio * W`, off the laid-out centre `ratio * (W - 6) + 3`. A NaN cursor
   or container size wrote `flex-grow: 0` on both panes (this was pinned as a known defect).
5. **The grab zone was invisible.** A press grabbed within ±9px of the centre (18px), but only the
   6px bar showed the resize cursor.
6. **No keyboard, and the wrong a11y node.** The CONTAINER was the tab stop and had role `Grip`.
   A separator's children are presentational, so that hid both panes' content from screen readers.

## 2. Commits

The RED column is the expected result on the commit before the fix.

| commit | kind | what / expected RED |
|---|---|---|
| `78fd7f148` | refactor | `layout/src/context_menu.rs`: `is_secondary_press`, `nearest_context_menu`, `context_menu_under_hit`, `LayoutWindow::context_menu_under_pointer` / `_under_seat`. This is the macOS rule verbatim, and the macOS shell uses it. No behaviour change. |
| `418aa49b3` | RED | New `layout/tests/a_context_menu_opens_from_a_secondary_press.rs` (in all.rs). RED `a_right_press_on_a_page_that_a_menu_box_hosts_opens_the_boxs_menu`: `expect` panics today (None); expected Some((dom 0 node 1, the 3-item menu)). Guards (green): scrolled Menus card right-click on the surface and on the label; Control-click on macOS; the `is_secondary_press` table; innermost of two nested menus; no menu opens nothing. |
| `e1b6e8121` | fix | The walk crosses `VirtualView` hosts (`host_of_nested_dom`), and doms are tried front-most first. X11, Wayland (both seat paths) and Windows use the shared pick. `get_first_hovered_node*` removed. Windows WM_RBUTTONUP always runs the pass (the menu is parked by PostMessage). |
| `2dcbe6868` | RED | `a_divider_drag_survives_the_rebuild_its_on_resize_asks_for` (reconcile_dom + transfer_states): `is_dragging` false today; expected true, anchor 100, ratio 0.7. Guard: `an_idle_split_pane_takes_the_ratio_the_app_rebuilds_it_with`. |
| `32e554055` | fix | The container carries `.with_dataset(state)` and `.with_merge_callback(merge_split_pane_state)`. Demo: `Showcase.split_ratio` is stored by `on_splitpane` and passed to `with_ratio`. |
| `2562870f4` | RED | `a_drag_that_leaves_the_divider_keeps_resizing_until_the_release`: today dragging is false, the ratio stays 0.5 and there are 0 writes; expected dragging, ratio > 0.7 and 2 writes. `a_press_on_the_divider_captures_the_pointer_for_the_split_pane`: no CapturePointer today; expected one. Guards: lost-release leave; press beside the divider. |
| `4b2745969` | fix | The press calls `capture_pointer(container)`. MouseLeave runs the new `on_split_pointer_leave`, which ends the drag only when `left_down` is false. Two pins updated. |
| `c24d3fa57` | RED | `the_divider_is_the_split_panes_focusable_separator` (today the divider has no tab index or a11y info, and the container is Grip); `the_arrow_keys_move_a_focused_divider` and `a_stacked_divider_moves_on_up_and_down_only` (today there is no key handler, so `expect` panics). |
| `c14eae0e2` | lint | `#[must_use]`, a `const fn`, and `allow(single_use_lifetimes)` on the two fns whose `dyn Fn` returns a reference. |
| `b363231cf` | fix | `on_split_key` on the divider: Left/Right or Up/Down by 1% (10% with Ctrl or Cmd), Home/End to MIN/MAX, `prevent_default`, both panes and `on_resize`. The tab stop, `Grip`, name "Resize panes" and value "NN" move from the container to the divider. Two pins updated. |
| `a5281d2b6` | RED | `the_divider_stays_under_the_cursor_that_drags_it`: centre 148.5 / 61.2 / 177.6 today vs 150 / 60 / 180. `the_grab_zone_is_centred_on_an_off_centre_divider`: presses 8.5 and 9px from the laid-out centre miss today. `a_non_finite_move_leaves_the_split_where_it_was`: ratio NaN today vs 0.5. |
| `e1dd25104` | fix | New `pane_space` and `divider_centre` (const fns) and a NaN guard in move. Pins now go through a `tracked()` helper; the two NaN "poisons" pins are removed. |
| `e6684c365` | RED | `the_grab_area_reaches_past_the_thin_visible_line`: today the divider has no `position` and no children; expected `position: relative` plus an absolute 18px sash at -6px with the resize cursor. |
| `677786d04` | fix | `sash_style` and `SASH_REACH`. The divider is `position: relative`. Pins: 7 divider declarations, the divider holds the sash, and the key test's second pane is node 5. |
| `6d1b84893` | lint | `NodeHierarchyItem::parent_id` in place of a redundant closure. |

The `chore(scripts): progress checkpoint` commits only carried the checkpoint file, which this
report replaces.

**RED pass (`git apply -R` per fix).** The later split-pane fixes edit test lines that earlier ones
also touched:
- `dom_registers_every_pointer_event_on_the_container`: S2 and S3.
- The key test's second-pane NodeId: S5.

So revert them newest first (S5, S4, S3, S2, S1), or revert one fix at a time on HEAD and expect
fuzz. None of the RED commits needs its fix to compile.

## 3. API changes (no api.json edit made)

- New module `azul_layout::context_menu`:
  - `is_secondary_press(&Platform, MouseButton, bool) -> bool`
  - `nearest_context_menu(..)`
  - `context_menu_under_hit(..)`
  - `LayoutWindow::context_menu_under_pointer() -> Option<(DomNodeId, Menu)>`
  - `LayoutWindow::context_menu_under_seat(u64) -> Option<(DomNodeId, Menu)>`

  These are Rust-side engine API. They could be exposed to C only if wanted; nothing needs them
  there.
- `azul_layout::widgets::split_pane::merge_split_pane_state` (pub `extern "C"`, like
  `merge_slider_state`; not in api.json).
- No struct layout change. `SplitPaneStateWrapper` and `SplitPaneState` are unchanged, so no
  autofix is needed.
- The SplitPane DOM shape changed: the divider now holds a sash child (the class
  `__azul-native-split-pane-sash`). The divider, not the container, now carries `TabIndex::Auto`,
  role `Grip` and a key callback. The container gained a dataset and a merge callback.

## 4. Least sure to compile

1. `layout/tests/a_context_menu_opens_from_a_secondary_press.rs`:
   - the closures passed as `&dyn Fn(DomId) -> Option<&'a StyledDom>`;
   - `NodeData::has_context_menu` passed as `impl Fn(&NodeData) -> bool`;
   - `node_data.as_ref()` on `StyledDom`;
   - `Menu::create(vec.into())`.
2. `layout/src/context_menu.rs`: the explicit `'a` on the `dyn Fn` return, and
   `#[allow(single_use_lifetimes)]` in case rustc flags it anyway.
3. `split_pane.rs`: the `const fn`s with float compares and arithmetic (fine on 1.91);
   `on_split_key`'s match on `(SplitDirection, VirtualKeyCode)`; `format!` plus `AzString::from` in
   `dom()`.
4. Tests in `split_pane.rs`:
   - `crate::callbacks::Callback::from_core(..).invoke(..)`;
   - `Some(key).into()` → `OptionVirtualKeyCode`;
   - `pressed.into()` → `VirtualKeyCodeVec`;
   - `a11y.accessibility_value.clone().into_option()`;
   - `CssProperty::Left(l) => l.get_property().map(|l| px(l.inner))`.
5. Shells that cannot be checked here:
   - Wayland: `try_show_context_menu_for`, and the `if a && b && self.try_show_..()` rewrites;
   - Windows: `let Some(..) else`, and `owner.node.into_crate_internal()` into
     `show_native_context_menu`'s `azul_core::dom::NodeId`;
   - X11: the `HitTestNode` import was dropped; `NodeId` in the `dom::{DomId, NodeId}` import may
     now be unused, which is a warning only.
6. `layout/src/widgets/split_pane.rs` `dom()`: `mk` borrows `state` for `divider_callbacks`
   before `state` moves into `with_dataset`.

## 5. Behaviour changes

- Context menus:
  - Every shell opens the INNERMOST menu under the front-most hit, and walks out of a
    `VirtualView` page into its host.
  - Windows used to open the outermost menu. It now also dispatches MouseUp(Right) and ContextMenu
    to a node that has a menu (the pass used to be skipped).
  - X11 and Wayland used to start from the lowest dom's highest NodeId.
- SplitPane:
  - The drag survives app rebuilds, captures the pointer, ignores leaves while the button is held,
    and tracks the cursor exactly. The grab zone is centred on the laid-out divider, and NaN moves
    are dropped.
  - An 18px sash with the resize cursor covers the grab zone. The sash is positioned, so it paints
    and hit-tests over 6px of each pane's edge; a click there grabs the divider (as it already did
    geometrically).
  - The divider is the tab stop and takes the arrow keys. The container is no longer focusable.
  - Controlled-widget rule: once the pointer is up, the app's ratio wins. An app that does not
    store the ratio from `on_resize` snaps back on its next rebuild, which is why the demo now
    stores it.
- `on_split_pointer_up` no longer handles MouseLeave.

## 6. Open items

- Device check on macOS:
  - Control + click and right click on the Menus box.
  - If nothing opens, run the scrolled-card guard. If it is red, bisect `7d7b25c0e` and the scroll
    chain.
  - Also check that Command + click opening nothing is accepted.
- The e2e runner still stores `pointer_capture` without applying it (`apply_pointer_capture` is
  called only in the dll), so a runner-driven split-pane drag outside the pane is not retargeted.
- Headless and runner still present no context menu. `CallbackChange::OpenMenu` is "unsupported"
  in the runner, and nothing records a menu request for an AZ_E2E assertion. The engine pick now
  makes that a small addition (`context_menu_under_pointer` after a right release).
- The keyboard spellings (Menu key, Shift+F10) and the a11y `ShowContextMenu` action fire the
  ContextMenu event, but no shell presents the native menu for them. A shared "present the menu
  of the focused node" hook would close that.
- Accessibility:
  - `AccessibilityRole::Separator` maps to `GenericContainer` in `managers/a11y.rs`. `Grip`
    (→ `Splitter`) is what the divider uses; revisit the Separator mapping.
  - The divider's value is set at build time. Between rebuilds it is not updated live.
  - The divider's name "Resize panes" is English-only; there is no builder to name it after the
    primary pane (APG).
- A pixel minimum per pane (for example min 100px) would need new API. Clamping is by ratio
  (0.05 to 0.95).
- The sash as an absolutely positioned child of a flex item relies on abspos-in-flex layout and
  on paint-order hit testing. If the sash is misplaced on screen, the geometric grab still works.
