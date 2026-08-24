# Input state stuck — selection, Switch, AzMap "+", AzPaint (2026-08-22)

Read-only investigation on branch `fix/open-bugs-wave-2026-08-22` (worktree
`debug-slider-scroll-2026-08-22`). No code was changed, nothing was built or run;
every claim below is a file:line reading. Where a claim depends on AppKit
behaviour that is not visible in this repo it is marked **UNVERIFIED**.

The demos the user tested predate `91f0eb7a0`, `b44804467`, `dd90d4938` (this
branch) and `ae9442beb` / `800c14757` (master, 2026-08-21: release buttons and
keys on blur). The macOS shell defaults to the **CPU backend**
(`dll/src/desktop/shell2/common/compositor.rs:169-177`), so every pointer event
on macOS resolves through the CPU hit-tester, not WebRender — that matters for
finding F2.

## The four reports (verbatim)

1. AzWidgets: "multi-node selection doesn't actually work — if I select over the
   'Azul Widgets Showcase' [heading] + drag off → it flickers the 'Every built-in
   widget' [subtitle] but doesn't select it?"
2. AzWidgets: "'Switch' not responding to clicks (click hang?)"
3. AzMap: "renders correctly but does not respond anymore to '+' or zoom in → UI is
   not locked, responsive on window resize — probably the same 'mouse button
   stuck' bug"
4. AzPaint: "on clicking sometimes selects text → maybe same 'cursor stuck'
   problem"

## Status per symptom

| # | Symptom | Status | Root cause(s) |
|---|---------|--------|---------------|
| 1 | heading→subtitle selection flickers, ends unselected | **Distinct, engine bug, NOT fixed by any listed commit.** Selection state is right; the *painting* of it is wrong. | F5 (every relayout path paints zero selection highlights and then clears the "repaint me" latch), with F6 (`hittest_cursor` never fails) shaping the end state. |
| 2 | Switch does not respond | **Demo bug + widget design gap; not a stuck button.** Same family as the slider fix in `b44804467`, which did not touch the Switch. | F7 (demo never stores `SwitchState.checked`, rebuild resets it; Switch has no dataset/merge). Possibly compounded by F2 if the window had been resized first. No hang path found (3 layout passes per click — F7). |
| 3 | AzMap "+" dead after some interaction, resize still repaints | **Most likely F2 (macOS/X11/Wayland never rebuild the CPU hit-tester on the resize / incremental-relayout paths)** — exactly "renders correctly, responsive on resize, ignores clicks". The "mouse button stuck" family (F1/F9) cannot produce this symptom: a latched `left_down` still delivers `MouseUp` and the "+" handler is a `MouseUp` handler. | F2 primary; F3/F4 make the map's own pan state sticky but do not block "+". |
| 4 | AzPaint click selects text | **Two independent, verified mechanisms; neither needs a stuck button, a stuck button makes both permanent.** | F8 (a click on any selectable text opens an engine "editing session" that is never closed by a click elsewhere; every later button-held move extends it, and `hittest_cursor` always returns a cluster) and F3 (a release outside the canvas/window never reaches the canvas: stroke stays open, hover paints). F9/F1 turn "while the button is held" into "forever". |

The "one family" intuition is right in a different way than "left_down stuck":
**azul has no notion of a press target / pointer capture** (grep for
`press_target|pointer_capture|mouse_down_target` finds nothing). Every widget
latches state on `MouseDown` and can only release it through a `MouseUp` or
`MouseLeave` that happens to be *dispatched to that widget*. Anything that
routes the release elsewhere (release outside the window, over a sibling, on
a node the rebuild replaced, into a modal, into a native window drag) leaves the
widget mid-gesture. F1–F4 and F9 are the instances.

## Findings

### F1. How `MouseDown` / `MouseUp` / `Click` are derived, and what a release elsewhere does

* `layout/src/event_determination.rs:360-398` — `MouseDown`/`MouseUp` are pure
  state diffs (`curr_down && !prev_down` / `!curr_down && prev_down`), targeted at
  `hover_manager.current_hover_node_full()` = the **current** hit test
  (`:342-344`). A `MouseUp` whose hit test is empty targets the ROOT node
  (`unwrap_or(root_node)`), so only root-level `Hover(MouseUp)` callbacks run.
* `Click` (`:405-417`) is "left released AND `previous_hover_node_full() ==
  current_hover_node_full()`" — i.e. the last two entries of the hover history
  (`layout/src/managers/hover.rs:226-234`, index 0 vs 1), **not** "down target ==
  up target". A press on A, move to B, release on B yields a Click on B.
* macOS ingress: `dll/src/desktop/shell2/macos/events.rs:171-233`
  (`handle_mouse_down`), `:236-296` (`handle_mouse_up`), `:335-396`
  (`handle_mouse_move`, also used for `mouseDragged:`), `:419-441`
  (`handle_mouse_exited` pushes an EMPTY hit test). Each one snapshots the
  baseline, writes `mouse_state`, calls `update_hit_test(position)` and runs one
  pass; `process_window_events` consumes the delta afterwards
  (`common/event.rs:6877-6880`).
* A release OUTSIDE the window on macOS: AppKit keeps delivering
  `mouseDragged:`/`mouseUp:` to the view that got `mouseDown:` (standard AppKit
  mouse tracking — **UNVERIFIED in-repo, AppKit contract**). `handle_mouse_up`
  then hit-tests the outside position → empty → `MouseUp` targets root → **no
  widget's MouseUp handler runs**. The tracking area
  (`macos/mod.rs:1244-1247`) lacks `EnabledDuringMouseDrag`, so no
  `mouseExited:` arrives mid-drag either (AppKit contract, **UNVERIFIED**); the
  per-node `MouseLeave` still fires from the hover diff when the hit test goes
  empty (`event_determination.rs:558-607`).
* A release during a modal: `tfd` dialogs (`layout/src/desktop/dialogs.rs:297-318`)
  block in a child process; the release goes to the dialog. The native context
  menu / native menu bar run nested tracking loops. In all cases the window
  never sees the `mouseUp:`; the only recovery on the user's build was the next
  click (press diffs true→true → no `MouseDown`; release → `MouseUp`+`Click`).
  `ae9442beb` (master) adds release-on-blur, which covers the modal cases but
  not F9.
* Press target across a `RefreshDom`: managers are remapped by the reconcile
  (`layout/src/window.rs:14485-14493`, `hover_manager.remap_node_ids` at
  `hover.rs:256-283` drops unmounted hits). A node the rebuild did not match
  disappears from the hover history → `previous_hover_node_full()` is `None`
  → **no `Click`**, though `MouseUp` still fires on whatever is under the
  pointer in the new tree.

### F2. macOS (and X11/Wayland) never rebuild the CPU hit-tester on relayout-only paths — stale geometry after every resize  ← AzMap "+"

* The CPU hit-tester is rebuilt in exactly one place on macOS:
  `dll/src/desktop/shell2/macos/mod.rs:4793-4800`, inside
  `regenerate_layout_once` (full DOM regeneration). `grep rebuild_from_layout`
  over `macos/` finds nothing else.
* A plain window resize takes the RESIZE FAST PATH: `macos/events.rs:961-963`
  → `request_regeneration_for_resize` (`common/event.rs:2219-2228`: full only
  when a size query / breakpoint flips, else `request_resize_relayout()`) →
  `macos/mod.rs:6547-6572` `incremental_relayout_for_resize` +
  `request_relayout_only()` → `build_atomic_txn` takes the relayout-only branch
  (`:6575-6593`, "skipping regenerate_layout()"). **No hit-tester rebuild on
  that path.** The same holds for the `RegenerateLayoutIncremental` arm
  (`apply_incremental_relayout_result`, `macos/mod.rs:5359-5374`: hover/focus
  restyles and widget `set_css_property` that need layout) and for
  `process_pending_virtual_view_updates` (`:6754-6771`, the map's in-place
  tile re-render, which swaps the child DOM's node ids).
* Windows DOES rebuild after an incremental relayout
  (`windows/mod.rs:1775-1786`), headless does on its incremental tail
  (`headless/mod.rs:1664-1668`), X11 only after the VirtualView drain
  (`x11/mod.rs:4790-4802`), Wayland likewise only on the full path and the
  VirtualView drain (`wayland/mod.rs:5108-5113`, `:5458-5465`; `:7564` is the
  menu popup). None of the three rebuild after `incremental_relayout` /
  `incremental_relayout_for_resize`. So the e2e suite, which runs headless,
  structurally cannot see this.
* Consequence on macOS: after any resize, every pointer event resolves against
  the pre-resize rects until something triggers a full regeneration. The AzMap
  header is `justify-content: space-between` (`examples/azul-maps/src/lib.rs:159-162`),
  so the button row moves with the window width; "+" (`:284-293`, a
  `Hover(MouseUp)` div) at its new position hit-tests to "−"/"Recentre"/nothing.
  Its `MouseUp` is dispatched to the wrong node → `on_zoom_in` (`:479-487`)
  never runs → nothing requests a regeneration → the stale tester persists.
  Panning the map would self-heal it (`on_viewport_changed` → `RefreshDom`,
  `:492-497`), which is consistent with "does not respond ANYMORE". The window
  itself repaints fine on resize because rendering reads the new layout
  (`incremental_relayout` → `layout_and_generate_display_list`,
  `common/layout.rs:1452-1478`) — "UI is not locked, responsive on window resize".
* The same stale tester hits AzWidgets (the column reflows on resize, every
  click below the first wrapped text lands on the wrong node until a callback
  fires) and AzPaint (`point_relative_to_item` is stale, strokes offset until
  the first stroke's own `RefreshDom` rebuilds it).

### F3. Widget press-state cleared only by a MouseUp/MouseLeave delivered to the widget

* Slider: `dragging` set at `layout/src/widgets/slider.rs:473`, cleared at
  `:491` (MouseUp) and `:519-520` (MouseLeave, only when the cursor is outside
  the track rect, `:506-517`); state carried across rebuilds by
  `merge_slider_state` (`:542-557`). Robust after `dd90d4938`/`b44804467`.
* Map (the handlers that actually receive pointer events are on the
  VirtualView CONTENT grid, `layout/src/widgets/map.rs:1473-1508`, duplicated on
  the outer div `:352-391`): `drag_anchor`/`press_origin` set in
  `map_on_pointer_down` (`:740-754`), cleared only in `map_on_pointer_up`
  (`:844-877`, bound to `MouseUp` AND `MouseLeave`). Persisted across rebuilds by
  `merge_map_tile_cache` (`:610-648`, returns the OLD cache, so the anchor
  survives every `RefreshDom`). A `MouseUp` elsewhere leaves the anchor set →
  the next `MouseOver` on the grid pans without a button (`:806-837`) until the
  next `MouseLeave` of the grid. This does not block "+" (a stuck anchor only
  affects moves over the map), but it is the map's own "stuck drag".
* AzPaint canvas: `examples/azul-paint/src/lib.rs:812-817` binds `MouseDown`/
  `MouseOver`/`MouseUp` only — **no `MouseLeave`**. `on_pointer_down` opens a
  stroke (`:905-919`), `on_pointer_move` extends it whenever `current.is_some()`
  (`:921-941`), `on_pointer_up` closes it (`:943-949`). A release outside the
  window, or over the header `<p>` (`:788-792`), never reaches the canvas →
  the stroke stays open → every later hover move paints and returns
  `RefreshDom` at pointer rate. This is the concrete "cursor stuck" in AzPaint.
* Split pane: `layout/src/widgets/split_pane.rs:365-394` — same `MouseDown`/
  `MouseOver`/`MouseUp`/`MouseLeave` pattern, `is_dragging` at `:460`/`:515`.

### F4. `MouseLeave` bubbles (non-W3C), so children end their parent's drag

`dd90d4938` fixed this for the slider only ("the thumb's own leave ended the
drag"). Two widgets still bind `MouseLeave` → "pointer up" on a node with
children:

* Map grid (`map.rs:1498-1502`): every tile `<div>` and its `<p>` label are
  children; crossing a tile boundary mid-pan delivers the old tile's
  `MouseLeave`, which bubbles to the grid → `map_on_pointer_up` → anchor
  cleared → pan ends after one tile (and, within 6px of the press, fires a
  spurious `on_pin_tap`, `:860-869`).
* Split pane (`split_pane.rs:378-381`): the first drag motion leaves the
  divider for a pane → `on_split_pointer_up` → drag ends.

Propagation: `core/src/events.rs` `propagate_event` (Capture→Target→Bubble for
every `Hover` filter, `dll/src/desktop/shell2/common/event.rs:6128-6205`).

### F5. Every full/incremental relayout paints NO selection highlight, then clears the latch  ← the "flicker" and "doesn't select it"

* `layout/src/window.rs:3831-3852` — `layout_and_generate_display_list` calls
  `solver3::layout_document(..., &scroll_offsets, &BTreeMap::new() /* text_selections */, ...)`
  (`:3838`, unchanged since the April rewrite). Only
  `regenerate_display_list_for_dom` (`:12492-12583`) feeds
  `text_edit_manager.build_text_selections_map()` (`:12542`) into the
  `LayoutContext`, which is what `paint_selections` reads
  (`layout/src/solver3/display_list.rs:3212-3242`).
* `:4472-4485` then sets `text_edit_manager.display_list_dirty = false` with a
  comment claiming the selection "feeds LayoutContext above" — it does not.
* Every relayout goes through that function: `incremental_relayout`
  (`common/layout.rs:1463-1471`), the resize fast path, the pre-cascade warm
  relayout when hover/focus states moved (`common/layout.rs:497-528`), the
  full regeneration, `relayout_root_dom_in_place` after a layout-affecting
  `set_css_property` (`window.rs:4837-4862`, e.g. the slider thumb / switch knob
  `margin-left`). Each produces a display list without `SelectionRect` items;
  the next `regenerate_display_list_for_dom` (next drag move, next autoscroll
  tick — `common/event.rs:8744-8770` re-runs `process_mouse_drag_for_selection`
  at 60 Hz while `left_down`) paints it again. On the CPU backend that
  alternation is a visible flicker of the highlight.
* After the release nothing re-adds it: the first relayout of any kind (the
  demo's `bump` → `RefreshDom` on any callback,
  `examples/azul-widgets/src/lib.rs:467-475`; a resize; a layout-affecting
  restyle) leaves the subtitle permanently unhighlighted while the selection
  is still present in `text_edit_manager.cross_block` — "doesn't select it".
* The cross-block machinery itself reads correctly for the demo:
  `process_mouse_drag_for_selection` (`window.rs:13607-13702`) → pointer outside
  the anchor block → `hittest_text_position_global` (`:13714-13790`, ranks IFC
  roots by screen distance) → `set_cross_block_selection` (`:2498-2575`,
  requires flow-block siblings via `block_sibling` `:2796-2846`; heading and
  subtitle are sibling divs under the flex column, `lib.rs:431-446`, UA `div {
  display: block }` `core/src/ua_css.rs:578`). Covered by
  `layout/tests/cross_block_selection.rs:241-327`, but only by calling the
  method directly on a `LayoutWindow` — no test drives press/move/release
  through `process_window_events`, and none checks the display list after a
  relayout.

### F6. `hittest_cursor` never fails, so the single-node fallback always "extends"

`layout/src/text3/cache.rs:5143-5180` returns the closest cluster for ANY point
(only `None` when the layout is empty). In `process_mouse_drag_for_selection`
the single-node fallback (`window.rs:13676-13701`) therefore always produces a
`Range(anchor, nearest cluster)` and **clears any cross-block selection**
(`:13684`) whenever the global hit resolves to the anchor node or
`set_cross_block_selection` returns `false` (non-sibling, non-block, no text).
Selecting from a heading into text that is not its flow-block sibling (a
caption `<span>` inside `labelled()`, a button label) snaps the highlight back
to the heading instead of extending — worth knowing when reading the user's
"flickers … but doesn't select it".

### F7. Switch: the demo discards the toggle, the widget has no state carrier  ← Switch

* `examples/azul-widgets/src/lib.rs:503-505` — `on_switch(_, _, _: SwitchState)`
  ignores the state and calls `bump` → `Update::RefreshDom`. `layout()` rebuilds
  `Switch::create(s.switch_on)` with `switch_on: true` forever (`:139-147`,
  `:570`). Compare `on_slider` (`:506-515`), which stores the value — the fix
  `b44804467` made for the slider trail.
* The widget: `layout/src/widgets/switch.rs:279-315` registers ONE
  `Hover(MouseUp)` callback whose `refany` is a fresh `RefAny::new(self.switch_state)`
  per build; no `with_dataset`, no `with_merge_callback`. `transfer_states`
  (`core/src/diff.rs:999-1013`) carries nothing without a merge callback, and
  the pre-cascade fast path (`common/layout.rs:459-475`) installs the fresh
  callback list, so the toggled `checked` dies on every rebuild.
* Per click: `default_on_switch_clicked` (`switch.rs:333-394`) flips `checked`,
  runs the user callback (→ `RefreshDom`), then patches two CSS properties
  (`:379-391`). `margin-left` is layout-affecting →
  `relayout_root_dom_in_place` (`window.rs:4795-4812`) runs inside
  `apply_user_change`, then the `RefreshDom` regeneration throws the patched
  tree away and rebuilds with `checked = true`. Net: knob back where it was,
  three layout passes, interaction counter +1. No hang path found; on a big
  showcase DOM three passes per click is the only "hang" candidate.
* Bubbling is not the problem: `MouseUp` targets the deepest hovered node (the
  knob) and bubbles to the track (`common/event.rs:6183-6204`).

### F8. A click on any selectable text opens a sticky "editing session"  ← AzPaint

* `MouseDown` → `TextSelectionClick` (`core/src/events.rs:3671-3703`, unconditional
  `AddAndPass`) → `process_mouse_click_for_selection` (`window.rs:13285-13589`)
  → `initialize_editing` (`layout/src/managers/text_edit.rs:455-478`) for ANY
  text whose `user-select` is not `none` (`solver3/getters.rs:5793-5806`,
  default selectable). `has_active_editing()` is just `multi_cursor.is_some()`
  (`text_edit.rs:395-397`).
* A click that hits no text returns `None` at `window.rs:13471` **without
  clearing the session**. Focus changes keep a non-editable *range* on purpose
  (`window.rs:6691-6732`) and AzPaint has no focusable node, so the session
  lives for the rest of the window.
* Every later `MouseOver` with `left_down` becomes `TextSelectionDrag`
  (`core/src/events.rs:3706-3733`; the dll hands it `drag_start_position =
  Some(cursor)` whenever `left_down && has_active_editing()`,
  `common/event.rs:7391-7397`). In AzPaint the only IFC root is the header
  `<p>` (`examples/azul-paint/src/lib.rs:788-792`); `hittest_text_position_global`
  returns it (same node as the anchor), the fallback calls `hittest_cursor`
  (F6) → the header text is selected from the original click to the cluster
  nearest the pointer's x — a stroke on the canvas drags a selection in the
  title bar. One earlier click on the header is all it takes.
* With a latched button (F9 / pre-`ae9442beb` builds) or a latched gesture the
  60 Hz autoscroll timer keeps calling `process_mouse_drag_for_selection`
  (`common/event.rs:8744-8770`, gate is `left_down`), so the selection follows
  the pointer with no button held.

### F9. Release-on-blur does not end the gesture session; macOS native window drag swallows the release

* `macos/mod.rs:2750-2808` (`windowDidResignKey`): clears `left_down`/`right_down`/
  `middle_down` and the held keys, runs one pass — but never calls
  `record_input_sample(.., is_button_up = true)`, so
  `gesture_drag_manager.end_current_session()` (`layout/src/managers/gesture.rs:718-722`)
  is not called. `detect_drag` (`:978-1003`) does not check `session.ended`,
  and `record_input_sample_with_pen` (`:685-691`) keeps appending to an
  un-ended session on every plain mouse move. Five pixels later
  `determine_all_events` emits `DragStart` with no button
  (`event_determination.rs:891-900`) → `ActivateWindowDrag`
  (`common/event.rs:7643-7645`, `:5568-5574`) or a node drag if the press was on a
  `draggable` → `is_dragging()` stays true (no release transition → no
  `DragEnd`, `:919-927`) until Escape/blur (`:7256-7280`). Same gap on X11 /
  Wayland / Windows blur handlers by construction (the shared test
  `dll/tests/blur_releases_mouse_buttons.rs` only checks `left_down = false`).
* `handle_begin_interactive_move` (`macos/mod.rs:3204-3219`) hands the drag to
  `performWindowDragWithEvent:`; AppKit consumes the ending `mouseUp:`
  (**UNVERIFIED**, AppKit contract — the macOS twin of the USER32 move loop
  `91f0eb7a0` fixed on Windows/X11). Only AzWriter uses `-azul-app-region`
  today (`examples/azul-writer/src/editor_ui.rs`), so it does not explain the
  four reports, but it is the open macOS member of the `91f0eb7a0` family.
* The e2e `blur` op (`layout/src/e2e/full.rs:11871-11883`) only flips
  `window_focused`; it does not mirror the OS handlers' release, so the
  headless runner cannot exercise the blur-release family at all.

### F10. Smaller observations

* `acceptsFirstMouse` is not implemented (`grep` over `macos/`), so the click
  that activates an inactive azul window is swallowed by AppKit (no
  `mouseDown:`); with the diff model a lone `mouseUp:` is a no-op, so nothing
  latches — but "first click does nothing" is a plausible part of "not
  responding" when the user alt-tabs between demos.
* `process_mouse_drag_for_selection` ignores its `start_position`
  (`window.rs:13602, 13609`); the dll passes the *current* cursor as
  `start_position` anyway (`common/event.rs:7391-7397`) — harmless today, a
  trap for anyone reading the `TextSelectionDrag` payload.
* `deepest_node_across_doms` picks the highest `NodeId`, not the deepest
  node (`hover.rs:20-32`, test `:417-421` pins this). Fine for the demos, but it
  is why the map's handlers had to move onto the VirtualView grid
  (`map.rs:1473-1478`).

## Common root causes

* **RC-A — no press target.** Mouse events are pure state diffs targeted at
  whatever is under the pointer *now*. The release is not guaranteed to reach the
  widget that took the press. Every widget works around it with a `MouseLeave`
  handler (slider, map, split pane) and the one that does not (AzPaint canvas)
  sticks. (F1, F3, F4)
* **RC-B — stale CPU hit-tester on macOS/X11/Wayland.** Rebuilt only by the full
  regeneration; the resize fast path, incremental restyle relayouts and
  in-place VirtualView re-renders leave it describing the previous geometry.
  (F2)
* **RC-C — selection is painted only by the display-list-only path.** The
  layout path paints none and clears the latch. (F5)
* **RC-D — sticky selection session** + an always-succeeding cursor hit test.
  (F6, F8)
* **RC-E — non-W3C `MouseLeave` bubbling** combined with "leave = release". (F4)
* **RC-F — widget state without a dataset/merge dies on `RefreshDom`**, and two
  demo callbacks treat controlled widgets as uncontrolled (Switch; SplitPane
  has neither a dataset nor a demo that stores its ratio). (F7)
* **RC-G — blur releases the buttons but not the gesture session; macOS
  native window drag has no `WM_EXITSIZEMOVE` equivalent.** (F9)

## Per-symptom specifics

1. **Selection flicker.** State machine: press on heading → `initialize_editing`
   (session on the heading's IFC root) → moves into the subtitle →
   `set_cross_block_selection` → `regenerate_display_list_for_dom` paints both
   bands → any relayout (RC-C) repaints without them → next move/autoscroll tick
   paints them again. Release → the next relayout erases the bands for good
   while `text_edit_manager.cross_block` still holds the selection. Whether the
   per-frame relayout during the drag came from a hover restyle, the demo, or
   the resize path on the user's machine cannot be decided from code; the
   post-release loss does not depend on it.
2. **Switch.** Click → widget flips → demo bumps and refreshes → rebuild with
   `checked = true` → knob back. Not stuck; reset. If the window had been
   resized first (RC-B), the click may also have landed on the wrong node
   entirely.
3. **AzMap "+".** RC-B after a resize is the only mechanism found that matches
   all three clauses (renders correctly / resize repaints / clicks ignored).
   The map's own pan anchor can stick (RC-A) and tiles end the pan early
   (RC-E), but neither blocks the header button.
4. **AzPaint.** RC-D: one click on the header text arms a session; strokes then
   select the header. RC-A: a release outside the canvas leaves the stroke
   open and hover paints. With a pre-`ae9442beb` build a modal or focus loss
   latches `left_down` and the autoscroll timer keeps extending the selection
   without a button (F8/F9).

## Proposed fixes (ordered by payoff / certainty)

1. **Rebuild the CPU hit-tester on every path that changes geometry
   (RC-B, ~1-2 h).** Move the
   `cpu_ht.rebuild_from_layout_with_gpu(&lw.layout_results, ..)` call into a
   shared hook invoked by `incremental_relayout`, `incremental_relayout_for_resize`
   and `process_pending_virtual_view_updates` (or at the end of
   `build_atomic_txn`'s relayout-only branch on every backend), instead of each
   backend remembering it. Windows (`windows/mod.rs:1782`) and headless
   (`headless/mod.rs:1664-1668`) already do it; macOS `:4797`, X11 `:4558`,
   Wayland `:5112` do not. Add a source-level test in the style of
   `dll/tests/blur_releases_mouse_buttons.rs` until the hook is shared.
2. **Paint selections on the layout path (RC-C, ~1-2 h).** `window.rs:3838`:
   pass `&self.text_edit_manager.build_text_selections_map()` (bind it before
   the borrow of `self.layout_cache`), and only then is the
   `display_list_dirty = false` at `:4485` truthful. Same for the other
   `layout_document` callers that can carry a selection (`:9825` if it serves
   an interactive DOM).
3. **Give the release a press target (RC-A, 0.5-1 day).** Remember the
   `MouseDown` target (per button) in the hover or gesture manager; when the
   release's hit test does not contain it, dispatch `MouseUp` to the press
   target as well (W3C pointer-capture semantics for the pressed element), and
   synthesise `MouseUp` to it on blur / Escape / native-drag end. Then widgets
   no longer need "leave = release" at all. Until then, add a `MouseLeave`
   handler to the AzPaint canvas (`end_stroke`).
4. **Stop `MouseLeave` from children ending a drag (RC-E, ~2-3 h).** Either
   make `MouseLeave`/`MouseEnter` non-bubbling (W3C) with a `relatedTarget`, or
   apply the slider's rule (`slider.rs:506-517`, "cursor still inside my rect
   → ignore") to the map grid (`map.rs:1498-1502`) and the split pane
   (`split_pane.rs:378-381`) via a shared `CallbackInfo::cursor_left_hit_node()`
   helper.
5. **Selection session hygiene (RC-D, ~1-2 h).** In
   `process_mouse_click_for_selection`, a press that finds no selectable text
   must collapse/clear the session (`window.rs:13471`), like a browser's
   mousedown; do not arm `drag_start_position` for a session whose anchor is
   not under a contenteditable AND whose pointer never left the anchor block
   unless the press itself hit text. Demo: `user-select: none` on the AzPaint
   header (`lib.rs:773-774`) and the AzMap header.
6. **Switch / SplitPane state (RC-F, ~1-2 h).** Demo: `s.switch_on =
   state.checked` in `on_switch` (`examples/azul-widgets/src/lib.rs:503-505`)
   and store the split ratio in `on_splitpane` (`:555-557`). Widget: give the
   Switch a dataset + merge callback so an uncontrolled app keeps the toggled
   value (`merge_switch_state`: fresh wins only when the app's value changed
   since the last build — or simply document "controlled widget, store the
   state"), and give the SplitPane the slider's `merge_*` treatment
   (`split_pane.rs:355-394` has neither).
7. **Blur/native-drag completeness (RC-G, ~1-2 h).** In every blur handler
   (and the shared helper that should replace them) also call
   `record_input_sample(pos, NONE, false, true, None)` so
   `end_current_session()` runs, and `DeactivateDrag`. On macOS, after
   `performWindowDragWithEvent:` poll `NSEvent::pressedMouseButtons()` on the
   next event/`windowDidMove` and release `left_down` when bit 0 is clear — the
   macOS analogue of `WM_EXITSIZEMOVE` + `GetKeyState` in `91f0eb7a0`. Make the
   e2e `blur` op call the same shared helper.
8. **Optional UX.** Implement `acceptsFirstMouse:` → `YES` on both views so the
   activating click reaches the DOM (F10).

## How to verify

Headless harness (`dll/src/desktop/shell2/headless/mod.rs`, `HeadlessEvent`
`:88-117`, `step()` `:5743-5865`, `incremental_vs_full()`, `rects_by_class()` as
used by `dragging_the_slider_leaves_no_thumb_behind` `:5049-5145`). Note the
harness rebuilds its hit-tester on every path, so test 1 must be a source-level
or shared-hook test, not a headless one.

1. **Hit-tester rebuild (F2).** Source-level test listing every
   `incremental_relayout` / `incremental_relayout_for_resize` /
   `process_pending_virtual_view_updates` call site per backend and asserting a
   `rebuild_from_layout_with_gpu` follows within the arm — or, after fix 1, a
   headless test that disables the harness's own rebuild and drives
   `HeadlessEvent::Resize` then `MouseDown/Up` on a `space-between` button row,
   asserting the right callback fired.
2. **Selection across two text nodes survives a relayout (F5).** Mount the
   AzWidgets heading/subtitle (two sibling block divs in a flex column), press
   on the heading, move into the subtitle, assert the display list has
   `SelectionRect` items in both IFC roots and `incremental_vs_full == 0`; then
   trigger (a) `HeadlessEvent::Resize`, (b) a callback returning `RefreshDom`,
   (c) a `set_css_property(margin-left)` on an unrelated node, and after each
   assert the `SelectionRect` items are still present. Release and assert
   again. Today (a)(b)(c) all drop the rects.
3. **Release outside the window / over a sibling (F1/F3).** Press on a node
   with a `MouseDown` handler that latches state (the map grid, the AzPaint
   canvas layout, the slider), `MouseMove` to `(-10, -10)` (empty hit test),
   `MouseUp`; assert the widget's latched state is cleared (slider `dragging`,
   map `drag_anchor`, paint `current`). Second variant: release over a sibling
   text node.
4. **Switch toggles under RefreshDom (F7).** Headless layout that stores
   `state.checked` and returns `RefreshDom`: click the track, assert the knob
   `margin-left` moved and `SwitchState.checked` flipped; click again, assert it
   flipped back. A second test with the demo's (non-storing) callback pins the
   current behaviour so the widget-side decision is explicit.
5. **Blur ends the gesture (F9).** Press, move 3 px, emulate the OS blur
   (`window_focused=false`, buttons cleared through the shared helper), then
   `MouseMove` 20 px with no button; assert no `DragStart` was dispatched and
   `gesture_drag_manager.is_dragging() == false`. Make the e2e `blur` op route
   through the same helper and add a JSON scenario (`mouse_down`, `mouse_move`,
   `blur`, `mouse_move`, `assert_state_machines_idle`).
6. **MouseLeave from a child (F4).** Map grid: press on tile A, move into
   tile B, assert `drag_anchor.is_some()` (today: `None`). Split pane: press
   on the divider, move 10 px into a pane, assert `is_dragging`.
7. **Sticky session (F8).** Click the header text, release, press on the
   canvas, drag; assert no `Range` selection exists (today: the header is
   selected).

## Effort

Fixes 1, 2, 5, 6: about half a day together, low risk, each with a headless
or source-level test. Fix 4: 2-3 h. Fix 7: 1-2 h plus the shared blur helper.
Fix 3 (press target) is the structural one: 0.5-1 day including migrating the
slider/map/split-pane "leave = release" workarounds onto it.

## Overlaps with existing commits

* `b44804467` (slider dataset/merge + pre-cascade dataset merge): covers the
  slider only; Switch and SplitPane have no dataset (F7). The same commit's
  headless test is the template for tests 2-4.
* `dd90d4938` (thumb `MouseLeave` no longer ends the slider drag): the map grid
  and split pane still have the bubbling-leave binding (F4).
* `91f0eb7a0` (native drag releases the button on Windows/X11): macOS
  `performWindowDragWithEvent:` is the uncovered twin (F9).
* `ae9442beb` / `800c14757` (release buttons/keys on blur, all backends): buttons
  yes, gesture session no (F9); not mirrored in the e2e `blur` op.
* `705d75a4b` / `62d782952` (cross-block selection, 2026-08-04): the state side
  works; the painting side never reached the layout path (F5).
* Memory note "OPEN: capture tile repaint (NullImage after ChangeNodeImage)" is
  unrelated; "Damage/present rework" is the other half of symptom 1 only if the
  flicker persists after fix 2.

## Open / unverified

* AppKit delivery rules relied on above (mouseUp outside the window reaches the
  pressed view; no `mouseExited:` during a drag without
  `EnabledDuringMouseDrag`; `performWindowDragWithEvent:` swallows the release;
  `acceptsFirstMouse` default) are AppKit contracts, not verified in this repo.
* Which relayout trigger produced the *per-frame* flicker on the user's machine
  (vs. the certain post-release loss) needs the headless repro of test 2 or a
  log with `AZ_LOG=debug` around `[build_atomic_txn]`.
* "click hang" for the Switch: no blocking path found; three layout passes per
  click on the showcase DOM is the only candidate.

File: `scripts/BUGS_2026_08_22_input_state_stuck.md`
