# Who owns a text box's scroll offset? The wheel vs. the caret reveal (2026-09-26)

Read-only architecture analysis. No code was changed. Base: `39a96b9bc`
(tip of `fix/input-bugs-2026-09-19`). Line numbers are from that commit.

User report (macOS, AzWidgets, trackpad/wheel), verbatim:

> there is still fighting going on over the 'last' item (key input or scroll
> event) if it's within the same multi-line text input (i.e. I type multiline
> without problems and then I want to scroll up within the multi-line
> textinput -> now the scrolling fights with the 'try to keep text cursor
> visible in selection' logic)

Follow-up question: why does selection behave differently depending on
whether the text is clipped?

---

## 1. The mechanism, stated so it can be proven wrong

**Claim.** In the shared desktop shell, EVERY event pass that is not
`prevent_default`ed and has at least one synthetic event runs a caret reveal,
whether or not any text was typed. Every wheel NSEvent, every trackpad and
momentum NSEvent, the `ScrollEnd` pass and every mouse move are such passes. So
once the user has scrolled the caret out of the TextArea's scrollport, the next
pass scrolls the TextArea back to the caret.

The path:

1. `core/src/events.rs:5300-5301`, in `post_callback_filter_system_changes`:
   `// Always apply pending text input` pushes
   `SystemChange::ApplyPendingTextInput` UNCONDITIONALLY. The only exit that
   skips it is `prevent_default`.
2. `dll/src/desktop/shell2/common/event.rs:11363-11381`, the post-callback tail
   of `process_window_events_inner`: `should_apply_text_input` is therefore
   always true. It applies `ApplyTextChangeset`, which is a no-op when nothing
   is pending (arm at `:7675-7686`), and then applies
   `ScrollCursorIntoViewAfterTextInput` **with no check that an edit
   landed**.
3. `event.rs:8168-8184`: that arm calls
   `LayoutWindow::scroll_selection_into_view(Cursor, Instant)`.
4. `layout/src/window.rs:14037`: `scroll_selection_into_view` first calls
   `scroll_manager.note_reveal_intent()`. This RE-ARMS the 722872666 arbiter
   to `ViewAction::Reveal` on every pass, and in the same call:
5. `window.rs:14187-14235` computes the minimal delta that brings the caret
   back inside the scrollport with 5 px padding, and applies it:
   - **instant** (`scroll_manager.scroll_to`, duration 0, at `:14229`) when
     the delta is at most half the scrollport, or
   - a **glide** (`ScrollInputSource::AnimateTo` pushed into the physics
     queue, `:14207-14225`) when the delta is larger than half the scrollport
     and `caret_scroll_glide` is on. It is on by default:
     `core/src/resources.rs:863`.

This is a real fight, not just a snap back, because the physics timer keeps
its own copy of where the finger is: `trackpad_raw_positions` is seeded ONCE
per gesture from the committed offset (`layout/src/scroll_timer.rs:486-506`)
and is never re-read afterwards. For the mouse wheel, `animate_targets`
accumulates the target (`:581-595`). So:

- in the wheel handler, the reveal pulls the offset back to the caret;
- on the next physics tick, the finger's raw position is written again
  (`scroll_to_unclamped`);
- on the next NSEvent, the reveal pulls the offset back again.

Frames alternate between the two owners. On the glide variant, the `AnimateTo`
arm also DELETES the finger's accumulator and its staged delta
(`scroll_timer.rs:653-654`), so the reveal wins inside the physics too.

Why it happens only "within the same multi-line text input": the reveal only
moves the caret's own scroll box, which is `find_scrollable_ancestor`,
self-inclusive (`window.rs:13921-13951`). Scrolling the page moves a different
box, which the reveal never touches.

**Macos ordering** (`dll/src/desktop/shell2/macos/events.rs`):
`handle_scroll_wheel` calls `record_scroll_from_hit_test` at `:616`, which
calls `note_user_scroll`, and then `process_window_events(0)` at `:700-701`,
which runs the tail above and calls `note_reveal_intent`. So after every wheel
event the arbiter ends up in `Reveal`. The same shared tail runs on all
backends.

**What would refute this claim:** after typing and a wheel scroll, a 1 px
`MouseMove` pass leaves the TextArea's offset where the wheel put it. The RED
test in section 8, step 1 checks exactly this.

## 2. Why 722872666 ("the last action wins") did not fix it

722872666 added a single per-window flag, `ScrollManager::last_view_action`
(`scroll_state.rs:780-802`):

- **Writers:** `note_user_scroll` at the queue chokepoints (`:829`, `:899`)
  and at thumb-drag start (`:1347`); `note_reveal_intent` in
  `scroll_node_into_view` (`window.rs:8807`) and `scroll_selection_into_view`
  (`:14037`).
- **Reader:** only the post-layout reveal, `scroll_focused_cursor_into_view`
  (`window.rs:14259`).

It is defeated in the product because of the pass described in section 1.
`scroll_selection_into_view` treats every caller as a real input, and the
per-pass tail is a caller that is not an input. So in the shell, the flag
reads `Reveal` after every pass. What actually keeps the post-layout reveal
quiet is its caret-key latch (`window.rs:14262-14302`), not the arbiter.

Why its tests were green: the headless paths do not have the defect.

- The e2e runner, `layout/src/e2e/runner.rs:1195-1218`, reveals only
  `if !changeset_result.dirty_nodes.is_empty()`. That is correct, and the
  shell does the opposite.
- The dll `HeadlessWindow`'s `Scroll` arm (`headless/mod.rs:2877-2935`)
  records the wheel but never runs `process_window_events`. The native
  backends do run it (macOS `events.rs:700`, the shared wheel arm at
  `event.rs:6627`).

This is the same "live in tests, different in the product" shape as the
earlier harness-divergence cases (memory:
`harness_cannot_reproduce_2026_08_25`, `widget_fix_wave_2026_08_25`).

## 3. Every writer of the offset

The table covers the offset of a TextArea's contenteditable (vertical) or a
TextInput's value `<p>` (horizontal).

| # | Writer | Triggered by | How it writes | Reads the arbiter? | Writes the arbiter? |
|---|---|---|---|---|---|
| W1 | Wheel, trackpad, momentum, TrackpadEnd | backend → `record_scroll_from_hit_test` (`scroll_state.rs:~880-960`) → queue (`:826-836`) | physics timer `scroll_timer.rs:334` → `ScrollTo` (`event.rs:5937-5975`) → `set_scroll_position(_unclamped)` (`scroll_state.rs:1169-1212`) | no | `note_user_scroll` |
| W2 | Physics private state: finger accumulator, `animate_targets` (wheel glide AND caret-reveal glide), velocities, rubber-band spring, momentum latch | every tick while `is_active()` (`scroll_timer.rs:236-250`) | same as W1 | no | no |
| W3 | **Per-pass reveal "after text input"** | **every non-prevented pass** (section 1) | `scroll_selection_into_view`: instant set or `AnimateTo` glide | no | **yes, every pass** |
| W4 | `CreateTextInput` reveal (IME `insertText:`, debug server, headless `TextInput`) | an edit that landed (`event.rs:6690-6711`, gated on `dirty_nodes`) | same | no | yes |
| W5 | Post-layout caret reveal `scroll_focused_cursor_into_view` | every successful layout (`window.rs:2601`) | same | **yes** (`:14259`), plus the caret-key latch | yes |
| W6 | `ScrollSelectionIntoView` (click, shift+arrow, add-cursor, cut/paste/undo/redo, select-all) | post-filter (`core/src/events.rs:5305-5323`) → `event.rs:8118-8150` | same. A RANGE reveals its bounding rect (`window.rs:14050-14058`) | no | yes |
| W7 | App `ScrollActiveCursorIntoView` | `callbacks.rs:2865` → `event.rs:6078` | same | no | yes |
| W8 | Node reveal: focus, Tab, click-to-focus, app `scroll_node_into_view`, a11y Focus | `event.rs:5419/5523/6073/7919/7995/8158`, `window.rs:16425-16470` | `window.rs:8795` → `scroll_into_view.rs:565-598`: instant set, or a 300 ms `ScrollManager` animation | no | yes (`:8807`) |
| W9 | Drag autoscroll timer | `StartAutoScrollTimer` on `TextSelectionDrag` (`core/src/events.rs:5313`) | `event.rs:377-551` → `timer_info.scroll_to` (`:543`) | no | **no** (it is the user's gesture but never claims the view) |
| W10 | Scrollbar thumb drag | `begin_thumb_drag` | direct set | no | `note_user_scroll` (`:1347`) |
| W11 | App `scroll_to` (Programmatic), a11y ScrollUp/Down/SetScrollOffset/ScrollToPoint | `window.rs:16495-16530` | `ScrollManager::scroll_to`/`scroll_by`, eased animation in `ScrollManager::tick` (`:1091-1110`) | no | no |
| W12 | Registration re-clamp | `register_scroll_nodes` after every layout (`window.rs:2585`) | clamp only (`scroll_state.rs:1287`, `:1562`) | no | no |
| W13 | VirtualView bounds | `update_virtual_scroll_bounds` (`:1302-1325`) | clamp only | no | no |

Three separate motion engines write one number:

1. instant sets;
2. `ScrollManager::animation`, which is eased and ticked by
   `ScrollManager::tick`;
3. the physics timer's springs, `AnimateTo` seeks and the private finger
   accumulator.

Each engine carries its own idea of where the view should be. None of them
knows about the others, except that a physics commit clears (2):
`set_scroll_position_unclamped` sets `animation = None`.

## 4. Secondary mechanisms (real, but not the reported fight)

**M2. A finger does not cancel an in-flight caret-reveal glide.**

- The `TrackpadContinuous`/`TrackpadMomentum` arm removes the node's
  `node_velocities` entry (`scroll_timer.rs:558-560`) but leaves its
  `animate_targets` entry.
- The seek loop only visits nodes that have a velocity entry (`:752`,
  `:780`), so the glide goes DORMANT instead of dying, and `is_active()`
  (`:237`) keeps the timer ticking.
- The glide wakes up as soon as anything re-creates a velocity entry for the
  node: the momentum edge hand-off (`:515-545`), a `TrackpadEnd` over an edge
  (`:690-705`), or the stale-gesture arm (`:386-405`). The view then glides
  back to a caret target from before the user scrolled.
- `WheelDiscrete` on a MouseWheel device bases its new target on the existing
  `animate_targets` entry (`:581-584`). A wheel click during a reveal glide
  therefore extends the REVEAL's destination, not the current view.

The reveal glide is recognisable by `ScrollInputDevice::Keyboard`
(`window.rs:14223`).

**M3. A range reveal whose rect is larger than the scrollport scrolls to the
START of the selection.** `calculate_instant_scroll_delta`
(`window.rs:14325-14353`) tests the left edge before the right, and the top
edge before the bottom. W6 uses the whole selection's bounding rect for
ranges. So Shift+Right past the right edge of a TextInput, or Shift+Down past
the bottom of a TextArea, reveals the selection's start, while the end being
extended leaves the view. W5 then re-reveals the focus end in the same frame
(the caret key changed), so the two reveals disagree within one frame. If the
first one was a glide (more than half the scrollport), the physics spends the
next ~400 ms pulling toward the start while W5 has already placed the end.
Browsers reveal the selection's FOCUS.

**M4. W5's "did the caret change?" latch is a heuristic keyed on content.**
The key is `(contenteditable_key, cursor, document_text_revision)`. Every
overlay text write bumps `document_text_revision` (`window.rs:17751`),
including edits the local user did not make, such as a peer's edit that
shifts the local caret. This works today only because W3 has already defeated
the arbiter it was meant to back up.

## 5. Candidates checked and refuted

- **Caret/text tweens and `reveal_pending`.** `apply_text_tweens`
  (`window.rs:9165-9655`) patches display-list items only. `reveal_pending`
  (`text_edit.rs:243`, set at `window.rs:17190`) clips glyphs under a gliding
  caret; it is not a scroll. It reads `scroll_manager` once, for the
  focus-ring frame, and never writes it.
- **Blink and tween timers.** Both return `Update::DoNothing`
  (`window.rs:202`, `:375`). A layout they cause runs W5, but the caret key is
  unchanged, so it does nothing.
- **Momentum treated as a reveal.** It is not: `is_user_scroll()` includes
  `TrackpadMomentum` and `TrackpadEnd` (`scroll_state.rs:~120-135`). The
  momentum tail matters only because each of its events is one more pass for
  W3.
- **Relayout re-registration.** It only clamps (W12), which never moves a
  mid-range offset.

## 6. "Selection behaves differently when the text is clipped"

Another agent is tracing the horizontal TextInput selection path in detail.
Reconcile the following with that trace; they are this analysis's hypotheses.

All of the following can only have an effect when the box can scroll. An
unclipped box has `max_scroll = 0`, so every writer clamps to 0 and the
pointer-to-glyph mapping cannot move under the user. That is why clipping
changes the behaviour.

1. **W3 runs on every drag `MouseMove` pass, AFTER `TextSelectionDrag`.**
   The pre-filter changes are applied at `event.rs:10997`; the tail runs at
   `:11374`. It reveals the primary cursor, which for a range is `r.end`, the
   end under the pointer (`core/src/selection.rs:702-706`). So:
   - The selection end is resolved against offset A
     (`window_point_to_ifc_local`, `window.rs:20121-20190`). W3 then moves the
     offset to B in the same pass, and the frame paints with B. The painted
     selection end is no longer under the pointer.
   - Within 5 px of an edge, the next pass maps the same pointer to a glyph
     further along, and W3 scrolls again. The result is a runaway
     "micro-autoscroll" whose speed depends on the mouse event rate, stacked
     on the timer autoscroll W9.
2. **The press.** A click within about 5 px (plus the caret width) of the
   scrollport edge is followed in the same pass by W6 and W3. They shift the
   text under a pointer that has not moved, so the drag that follows starts
   from a different glyph than the one pressed.
3. **Keyboard extension past the edge** jumps to the selection start (M3).
4. **W9 (drag autoscroll) never claims the view**, and nothing orders it
   against W3/W5/W6. During a drag, three writers move the same box, each
   against its own notion of the target.

Diagnosis: this is the same problem as the TextArea fight. It is an OWNERSHIP
problem that shows up as a COORDINATE problem. A pointer point is resolved
against a scroll offset that another writer changes later in the same pass. The
2026-08-25 fix (the `azul_core::spaces` newtypes) made the conversion correct,
but no type can make it stable while several writers can move the offset in
between.

## 7. Architecture diagnosis

- **There is no single arbiter.** There are five ad-hoc checks:
  1. `last_view_action`: one bit per window, read by W5 only;
  2. the caret-key latch (W5);
  3. the e2e runner's `dirty_nodes` gate, which the shell does not have;
  4. the physics' finger-overrides-spring rule, which does not cover
     `animate_targets`;
  5. the momentum latch, which covers rubber-band ownership only.
- **Intent is derived, not carried.** A reveal is re-derived from state (the
  caret exists; the caret key changed; this pass had events) instead of being
  a one-shot value created by the input that caused it and consumed once.
  This is defect 4 of `text_scroll_architecture_2026_08_25` ("no conservation
  of intent") in its reveal form. The per-pass tail W3 is the extreme case: a
  reveal with no cause at all.
- **Writing the intent is conflated with performing the reveal.**
  `scroll_selection_into_view` both claims the view (`:14037`) and performs
  the reveal. So any caller, including non-inputs, becomes the "last action".
- **Three motion engines, no cancellation protocol.** A user gesture does not
  retire the engine's own pending motion (M2), and a reveal does not know
  whether a gesture is in progress (W3 during momentum, W3 during a drag).
- **The harnesses diverge from the shell exactly at the defect.** This is
  runner `:1207` vs. shell `:11370-11377`, and the headless `Scroll` arm that
  never runs a pass.

## 8. Is another architecture change needed?

Yes, but a small one, done in steps, with step 1 shipping first on its own.
The target shape is **one view-intent arbiter per window with explicit
owners, and a single reveal request that is consumed once**:

- `ScrollManager` holds `pending_reveal: Option<RevealRequest { target:
  Caret | Selection(Focus) | Node(DomNodeId, options), cause: InputSeq }>`
  and `last_user_scroll: Option<(InputSeq, container)>`.
- Only the input sites issue requests: an edit that landed, a caret or
  selection op, a click, a focus change, or an app/a11y request. Nothing else
  may issue one. `scroll_selection_into_view` becomes a pure "compute and
  apply" that neither reads nor writes the arbiter.
- The request is consumed ONCE, after layout (publish before consume, as
  `register_scroll_nodes` already does at `window.rs:2585`), against fresh
  geometry, and then cleared.
- A user scroll (W1, W10, and W9, which counts as the user's drag) drops the
  pending request **and retires any engine-owned `AnimateTo` target on that
  container**.
- This deletes `last_view_action`, `last_revealed_caret_key` and
  `last_revealed_caret_rect`, and W3 as a separate path.

### Fix plan, RED test first for each step

**Step 1 (P0, the reported bug). Reveal only when an edit landed, through one
shared function.**

- Fix: the shell tail at `event.rs:11370-11377` reveals only if
  `ApplyTextChangeset` produced `dirty_nodes`. Put "apply the pending text,
  then reveal if it landed" into ONE `LayoutWindow` method and call it from
  both the shell and `runner.rs:1195-1218`, so the two cannot drift again.
  Apply the same gate to the `InsertLineBreakAtCursor` path
  (`event.rs:11669-11673`).
- Harness parity: the headless `Scroll` arm must run `process_window_events`
  after recording, as macOS does.
- RED test (dll, `headless/mod.rs` `mod tests`): TextArea with
  `height: 80px`. Click into it, then type 15 lines through
  `apply_user_change(CreateTextInput{"line\n"})` and `regenerate_layout()`.
  Record `y_typed` as the offset of `find_scrollable_ancestor(focus)`; it must
  be greater than 0.
- Then: `record_scroll_from_hit_test(0, -30, WheelDiscrete, MouseWheel)`,
  commit the physics result with `set_scroll_position(y_typed - 30)`, run
  `process_window_events(0)` as `macos/events.rs:700` does, and step a 1 px
  `MouseMove`.
  - Assert the offset is `y_typed - 30`. **Today the offset returns to
    `y_typed`**, the caret-at-bottom reveal position. 30 px is below half the
    scrollport, so the snap is instant even with glide on.
  - Also assert `!reveal_may_move_view()`; **today it is true**.
  - Also assert that the queue holds no `AnimateTo`, which covers the glide
    variant.
- Control, which must pass both before and after: one more keystroke reveals
  the caret again. That is the "key input wins" half of the rule.
- Note: the test `step()` helper ignores `Scroll`/`TextInput`
  (`headless/mod.rs:~8865`), so call the APIs directly.

**Step 2 (P1). A user gesture retires the engine's reveal glide (M2).**

- Fix: in the finger and momentum arms, and in the wheel arm, remove any
  `animate_targets` entry for the node whose device is `Keyboard`, i.e. an
  engine reveal. Base a wheel target on `current_offset` when the existing
  target is engine-owned.
- RED test (`scroll_timer.rs` tests, next to the `AnimateTo` tests at
  `:2765-2990`): queue `AnimateTo(0, 400)` with device `Keyboard`, tick once,
  queue `TrackpadContinuous(0, -30)`, tick. Assert
  `!st.animate_targets.contains_key(&key)`; **today it is present**.
- Second assert: a later `TrackpadEnd` past the top edge, followed by ticks,
  never moves the offset toward 400.

**Step 3 (P1). A range reveal targets the selection's focus, not its bounding
rect (M3).**

- Fix: `SelectionScrollType::Selection` reveals the focus-end caret rect, or
  the bounding rect only when it fits, anchored to the focus side.
- RED test (layout lib test): a TextInput holding 200 characters and a range
  from 0 to 150, with the focus at 150 and a scrollport narrower than the
  range. Call `scroll_selection_into_view(Selection, Instant)` and assert the
  focus caret is inside the visible area. **Today the view goes to the range
  start**, offset = `start.x - 5` clamped to 0, and the focus is off-screen.

**Step 4 (P2). Structural change: the one-shot `RevealRequest` arbiter
(section 8 shape).**

- Remove `note_reveal_intent` from `scroll_selection_into_view`, W5's latch,
  and `last_view_action`. Make W9 claim the view.
- RED first is impossible for an API that does not exist yet. Use a
  **negative control** instead: with step 4 in place, revert step 1's
  `dirty_nodes` gate. The step 1 test must STAY green. That proves the
  arbiter owns the decision, not an incidental gate.
- Plus contract tests on the new API:
  1. a request is consumed exactly once;
  2. a user scroll after a request drops it;
  3. a request issued after a user scroll wins;
  4. a peer's or program's text change issues no request.

**Step 5 (P2). Drag selection with a single owner.**

- During an armed `text_selection_drag_anchor`, W9 is the only writer of the
  anchor block's scroll box. Resolve the selection end AFTER that pass's
  scroll writes (or re-resolve it when the offset changes), so hit test and
  paint use the same offset.
- RED test (dll headless): a horizontally overflowing TextInput scrolled to
  the middle. Press on a glyph 3 px inside the right scrollport edge and drag
  1 px; the selection end stays within one glyph of the pointer. **Today W3
  moves the text by about 5 px or more under a still pointer.** The test must
  hold still for N more 1 px moves: the offset changes only by W9's
  time-based amount, not once per event.
- Fold this into the other agent's findings before writing it.

## 9. Verify before acting

- Confirm on device with the one-variable instrument: log
  `reveal_may_move_view()` and the offset around `event.rs:11374`, then type,
  scroll, and move the mouse. The claim predicts a snap on the first pointer
  move after scrolling the caret out of view, even with the wheel idle.
- After step 1, confirm that the momentum tail no longer re-reveals. It
  should not, since its passes will not land an edit.
- The glide threshold (half the scrollport) decides whether the fight looks
  like an instant snap or a spring tug-of-war. Both are the same bug.
