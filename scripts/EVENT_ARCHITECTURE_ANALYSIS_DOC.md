# Event Architecture Analysis — Brittleness & Simplification

## Executive Summary

The event architecture has two independent sources of state changes:

1. **User changes** (`CallbackChange`): Things the application callback explicitly
   requested — add timer, modify window state, insert text, etc.
2. **System changes**: Things the framework itself determined — focus changed due to
   mouse click, scrollbar thumb dragged, text selection extended, drag pseudo-state
   activated, window resized by OS, cursor blink timer needed, etc.

Today, user changes are processed through a brittle pipeline that converts an exhaustive
enum (`CallbackChange`, 40+ variants) into flat structs (`CallbackChangeResult` →
`CallCallbacksResult`, 27 fields each), losing compile-time safety. System changes are
processed via ad-hoc inline code scattered across `process_window_events_recursive_v2()`
with no shared representation at all.

The fix: **two enum types** (`CallbackChange` for user, `SystemChange` for framework),
both processed through exhaustive `match` loops, wrapped in a newtype that forces
the platform layer to handle both. Adding a new variant to either enum causes a compile
error everywhere it needs to be handled.

---

## 1. Current Architecture: Four Representations + Ad-Hoc System Processing

### User changes: four representations of the same data

```
User callback runs, calls CallbackInfo methods
    ↓ pushes to Arc<Mutex<Vec<CallbackChange>>>
Vec<CallbackChange>                          ← Enum, 40+ variants, exhaustive match
    ↓ apply_callback_changes() in LayoutWindow
CallbackChangeResult                         ← Struct, ~25 fields, NO exhaustive check
    ↓ merge_into() + resolve_focus_into()
CallCallbacksResult                          ← Struct, 27 fields, NO exhaustive check
    ↓ needs_processing() — manual field enumeration
    ↓ needs_redraw()     — manual field enumeration (subset!)
    ↓ process_callback_result_v2() — manual if-block per field
ProcessEventResult                           ← Enum, 7 ordered variants
    ↓ platform tick handler (8× copy-paste)
Platform redraw trigger                      ← setNeedsDisplay / InvalidateRect / etc
```

Four representations. Three conversion steps. Zero compile-time safety after step 1.

### System changes: no representation at all

System changes are processed inline in `process_window_events_recursive_v2()` with
code like:

```rust
// Focus changed? → restyle, start blink timer, scroll into view
if focus_changed {
    layout_window.apply_focus_restyle();
    if needs_cursor_init {
        layout_window.finalize_pending_focus_changes();
        self.start_timer(CURSOR_BLINK_TIMER_ID, blink_timer);
    }
    result = result.max(ShouldReRenderCurrentWindow);
}

// Drag started? → set :dragging pseudo-state, activate GPU transform
if drag_started && has_draggable_node {
    layout_window.gesture_drag_manager.activate_node_drag(...);
    result = result.max(ShouldReRenderCurrentWindow);
}
```

~30 different system changes, processed across ~500 lines of interleaved if-blocks
and match statements. No central enum. No exhaustive check. Adding a new system
behavior = finding the right spot in a 700-line function and hoping you didn't miss
anything.

### What gets lost at each conversion (user changes)

| Step | What's lost |
|------|------------|
| `Vec<CallbackChange>` → `CallbackChangeResult` | **Exhaustive match IS used** here (good). But output is a flat struct, "was this set?" requires `is_some()` / `is_empty()` |
| `CallbackChangeResult` → `CallCallbacksResult` | `merge_into()` manually forwards ~20 fields. Fields can fall out of sync. |
| `CallCallbacksResult` → `needs_processing()` | Manual enumeration of 20+ conditions. Missing one → silent bug. **This is where `update_all_image_callbacks` was missed.** |
| `CallCallbacksResult` → `needs_redraw()` | Separate manual enumeration with DIFFERENT conditions — parallel determination that can (and did) disagree. |
| `CallCallbacksResult` → `process_callback_result_v2()` | 200+ lines of if-blocks. No compile-time guarantee all fields are handled. |

### Adding a new callback capability today: 20 places

| # | File | What |
|---|------|------|
| 1 | `layout/src/callbacks.rs` — `CallbackChange` | New variant |
| 2 | `layout/src/callbacks.rs` — `CallbackInfo` | New method pushing the Change |
| 3 | `layout/src/timer.rs` — `TimerCallbackInfo` | Delegating method |
| 4 | `layout/src/window.rs` — `CallbackChangeResult` | New field |
| 5 | `layout/src/window.rs` — `CallbackChangeResult::Default` | Initialize field |
| 6 | `layout/src/window.rs` — `apply_callback_changes()` | Match arm |
| 7 | `layout/src/window.rs` — `merge_into()` | Forward field |
| 8 | `layout/src/callbacks.rs` — `CallCallbacksResult` | New field |
| 9 | `layout/src/callbacks.rs` — `CallCallbacksResult::empty()` | Initialize field |
| 10 | `layout/src/callbacks.rs` — `needs_processing()` | Check field |
| 11 | `layout/src/callbacks.rs` — `needs_redraw()` | Check field (if visual) |
| 12 | `event_v2.rs` — `process_callback_result_v2()` | Handle field |
| 13 | `api.json` | FFI binding |
| 14–20 | 4 platforms × 2 (timer+thread) tick handlers | Copy-paste boilerplate |

**Bug history**: every bug in this session was a missed step in this chain.

---

## 2. Catalog of Current NOTE / IDEMPOTENT / TODO Comments

These comments are symptoms of the architectural brittleness. Each one marks a place
where the developer had to leave a warning because the type system couldn't enforce
the invariant.

### Timer dual-application path (IDEMPOTENT)

**File**: `macos/mod.rs:1870`
```rust
// IDEMPOTENT: If an NSTimer already exists for this timer_id, invalidate
// it before creating a new one. This prevents duplicate NSTimers when
// invoke_expired_timers() already inserted the timer into layout_window
// and then process_callback_result_v2 calls start_timer() again.
```

**Root cause**: Timer changes are applied in TWO places — once in `invoke_expired_timers()`
(to `layout_window.timers`) and again in `process_callback_result_v2()` → `start_timer()`
(platform timer). The `IDEMPOTENT` comment exists because the second application must
silently tolerate the timer already existing.

**With two-enum architecture**: `CallbackChange::AddTimer` is processed once in
`apply_deferred_changes()`. No dual path, no idempotency needed.

### Timer/thread double-processing (NOTE)

**File**: `event_v2.rs:3636-3640`
```rust
// NOTE: These are ALSO processed by process_callback_result_v2 (called
// by the platform tick_timers handler), which calls start_timer()/
// stop_timer() to manage platform-specific timers (e.g. NSTimer).
// start_timer() is idempotent — it invalidates any existing NSTimer
// before creating a new one. stop_timer() is also idempotent.
```

**Root cause**: Same as above. The developer had to leave a NOTE explaining why
the same timer changes are applied twice, and why this is "safe" (idempotency).

### Image callback invocation path (NOTE)

**File**: `event_v2.rs:3084-3093`
```rust
// NOTE: We do NOT invoke the image callbacks here. The actual invocation
// happens in wr_translate2::process_image_callback_updates() during the
// WebRender transaction building phase. Invoking them here would cause:
//   1. Double callback invocation (once here, once during transaction build)
//   2. Double state mutation (e.g. rotation += 1 incremented twice per frame)
//   3. Wasted work (the textures produced here are never registered with
//      WebRender and are discarded)
```

**Root cause**: `process_callback_result_v2()` had code to invoke image callbacks
that was wrong — the correct invocation path is during WebRender transaction
building. The NOTE warns future developers not to re-add it.

**With two-enum architecture**: `CallbackChange::UpdateAllImageCallbacks` processes
in `apply_deferred_changes()` by setting `result = ShouldReRenderCurrentWindow`.
The rendering path handles actual invocation. No place to accidentally add wrong
invocation because the match arm is explicit about what it does.

### Window state save ordering (NOTE)

**File**: `event_v2.rs:2824`
```rust
// NOTE: We must save previous state BEFORE modifying current state
// so that process_window_events_recursive_v2 can detect the change
```

**Root cause**: `process_callback_result_v2()` modifies `current_window_state`
but the recursive event processor needs to compare old vs new state. The ordering
dependency is documented but not enforced.

**With two-enum architecture**: `apply_deferred_changes()` is called AFTER
`save_previous_window_state()` in the trait default method. The ordering is
structural, not documented.

### Platform tick handler boilerplate  (8× copy-paste)

**File**: `macos/mod.rs:443-465`, `windows/mod.rs:2870-2910`, `x11/mod.rs:1385-1420`,
`x11/mod.rs:2830-2870`

All platforms have identical code:
```rust
let timer_results = macos_window.invoke_expired_timers();
let mut needs_redraw = false;
for result in &timer_results {
    if result.needs_processing() {
        macos_window.previous_window_state = Some(macos_window.current_window_state.clone());
        let process_result = macos_window.process_callback_result_v2(result);
        macos_window.sync_window_state();
        if process_result >= ProcessEventResult::ShouldReRenderCurrentWindow {
            needs_redraw = true;
        }
    }
    if result.needs_redraw() {
        needs_redraw = true;
    }
}
```

**Root cause**: No trait default method. Each platform implements the same logic.

### Dead fields in `CallCallbacksResult`

`should_scroll_render` (bool): Initialized in `empty()`, never set by any callback
processing path. The V2 system handles scroll rendering through `nodes_scrolled_in_callbacks`.

`cursor_changed` (bool): Set by callbacks but never read in `process_callback_result_v2()`.

**Root cause**: No way to verify that every field is both written and read. Flat
structs don't have "unused field" warnings.

---

## 3. Catalog of System Changes (Not From User Callbacks)

These ~30 system changes currently have NO shared enum representation. They are
processed as ad-hoc code in `process_window_events_recursive_v2()` and platform
event handlers.

### Category 1: Pre-Callback System Events

Processed BEFORE user callbacks are dispatched. Currently modeled as
`PreCallbackSystemEvent` enum in `core/src/events.rs` (good!), but processed
inline rather than through a unified loop.

| System Change | Data | Where Processed |
|---|---|---|
| `TextClick` | target, position, click_count | `process_mouse_click_for_selection()` |
| `TextDragSelection` | target, start/current position | `process_mouse_drag_for_selection()` |
| `ArrowKeyNavigation` | target, direction, word_jump | TODO — not implemented |
| `KeyboardShortcut` | target, Copy/Cut/Paste/SelectAll/Undo/Redo | Direct clipboard/undo |
| `DeleteSelection` | target, forward | `delete_selection()` |

### Category 2: Post-Callback System Events

Processed AFTER user callbacks. Currently modeled as `PostCallbackSystemEvent` enum
(good!), but only used for "should we do X?" flags.

| System Change | Platform Action Needed? | Where Processed |
|---|---|---|
| `FocusChanged` | Yes — start/stop cursor blink timer | Inline in `process_window_events_recursive_v2` |
| `ApplyTextInput` | No | `apply_text_changeset()` |
| `ScrollIntoView` | No | `scroll_selection_into_view()` |
| `StartAutoScrollTimer` | Yes — `start_timer()` | Inline — creates 60Hz timer |
| `CancelAutoScrollTimer` | Yes — `stop_timer()` | Inline — removes timer |

### Category 3: Drag & Drop System Processing

Processed after callbacks, triggered by gesture detection (not user API calls).

| System Change | Data | Platform Action? |
|---|---|---|
| Auto-activate node drag | DragStart + `draggable=true` → `activate_node_drag()` | No |
| Auto-activate window drag | DragStart without draggable → `activate_window_drag()` | Yes — `begin_interactive_move()` |
| Set `:dragging` pseudo-state | `styled_node_state.dragging = true` | No |
| Set `:drag-over` pseudo-state | DragEnter/Leave on target nodes | No |
| GPU transform update | Delta → `ComputedTransform3D::new_translation()` | No |
| Auto-deactivate drag | DragEnd → clear states, `end_drag()` | No |

### Category 4: Focus / Cursor / Selection

| System Change | Platform Action? |
|---|---|
| Mouse click-to-focus (W3C default) | Possible — start blink timer |
| Tab/Shift+Tab navigation | Possible — start blink timer |
| Escape → clear focus | Possible — stop blink timer |
| Enter/Space → synthetic click | No (dispatches more events) |
| Focus restyle (`:focus` CSS) | No |
| Start cursor blink timer | Yes — `start_timer()` |
| Stop cursor blink timer | Yes — `stop_timer()` |
| Clear selection on focus change | No |
| Synthetic Focus/Blur events | No (dispatches more events) |

### Category 5: Scrollbar Interaction

| System Change | Platform Action? |
|---|---|
| Scrollbar hit test | No |
| Scrollbar thumb click → drag | No |
| Scrollbar track click → jump | No |
| Scrollbar thumb drag → scroll | No |
| GPU scroll update | No |

### Category 6: Platform-Level Events

| System Change | Where |
|---|---|
| Window resize → update viewport, DPI | Platform event handler |
| HiDPI change (monitor switch) | Platform event handler |
| Scroll input recording | Platform `handle_scroll_wheel()` |
| Hit test update (mouse move) | `update_hit_test_at()` |
| Gesture recording (mouse down/move/up) | `record_input_sample()` |
| File drop | Platform `performDragOperation` |

### Category 7: Layout Engine

| System Change | Where |
|---|---|
| Register scroll nodes after layout | `layout_v2.rs` |
| Calculate scrollbar states (visibility, thumb position) | `layout_v2.rs` |
| Scrollbar opacity sync → GPU | `layout_v2.rs` |
| Apply runtime states (`:focus`, `:hover`, `:active`, etc.) | `layout_v2.rs` |
| State migration (DOM reconciliation) | `layout_v2.rs` |
| Manager NodeId remapping after DOM rebuild | `layout_v2.rs` |
| DOM unchanged detection (layout skip) | `layout_v2.rs` |

### Key observation

**Categories 1-2** already have enum representations (`PreCallbackSystemEvent`,
`PostCallbackSystemEvent`) — these are the embryo of `SystemChange`.

**Categories 3-5** are processed as inline code with no enum — they should be.

**Categories 6-7** happen at different lifecycle stages (platform input recording,
post-layout fixup). These are not "changes" in the same sense — they're lifecycle
hooks. They should stay where they are.

---

## 4. Proposed Architecture: Two Enums + Newtype Wrapper

### The two change types

```rust
/// Changes requested by user callbacks.
/// This enum is already mostly correct — just keep it through processing.
pub enum CallbackChange {
    // ~40 variants: AddTimer, RemoveTimer, ModifyWindowState,
    // InsertText, MoveCursorLeft, ChangeNodeText, ...
}

/// Changes determined by the framework/layout engine.
/// These are things the system decided to do, not things the user asked for.
pub enum SystemChange {
    // --- Focus ---
    FocusNode { target: DomNodeId },
    ClearFocus,
    StartCursorBlinkTimer,
    StopCursorBlinkTimer,
    RestyleForFocusChange { old: Option<DomNodeId>, new: Option<DomNodeId> },

    // --- Text ---
    ApplyTextChangeset,
    ScrollCursorIntoView,

    // --- Drag ---
    ActivateNodeDrag { target: DomNodeId, start_position: LogicalPosition },
    ActivateWindowDrag,
    SetDraggingPseudoState { target: NodeId, active: bool },
    SetDragOverPseudoState { target: NodeId, active: bool },
    UpdateDragGpuTransform { target: NodeId, delta: LogicalPosition },
    DeactivateDrag,

    // --- Auto-Scroll ---
    StartAutoScrollTimer,
    CancelAutoScrollTimer,

    // --- Scrollbar ---
    ScrollbarTrackClick { dom_id: DomId, node_id: NodeId, click_ratio: f32 },
    ScrollbarThumbDragStart { hit_id: ScrollbarHitId, mouse_pos: LogicalPosition },
    ScrollbarThumbDragUpdate { delta: LogicalPosition },

    // --- Hover/Restyle ---
    RestyleNodes { changes: Vec<(NodeId, Vec<ChangedCssProperty>)> },
    DispatchSyntheticEvents { events: Vec<SyntheticEvent> },
}
```

### The newtype wrapper: `FrameChanges`

The key design: user and system changes are BUNDLED in a newtype that forces
the platform to process BOTH:

```rust
/// All changes from one event processing cycle.
/// The platform layer MUST process both user and system changes.
///
/// Note: This is NOT a struct with two Vec fields (which would let you
/// forget one). The `process()` method is the ONLY way to consume this.
pub struct FrameChanges {
    user_changes: Vec<CallbackChange>,
    system_changes: Vec<SystemChange>,
    update_screen: Update,
}

impl FrameChanges {
    pub fn is_empty(&self) -> bool {
        self.user_changes.is_empty()
            && self.system_changes.is_empty()
            && self.update_screen == Update::DoNothing
    }

    /// Process all changes. Returns the required redraw level.
    ///
    /// This is the ONLY public method. You can't extract user_changes or
    /// system_changes separately — you must process them together.
    pub fn process(self, window: &mut dyn PlatformWindowV2) -> ProcessEventResult {
        let mut result = ProcessEventResult::DoNothing;

        // Process user changes (exhaustive match)
        for change in &self.user_changes {
            let r = window.apply_user_change(change);
            result = result.max(r);
        }

        // Process system changes (exhaustive match)
        for change in &self.system_changes {
            let r = window.apply_system_change(change);
            result = result.max(r);
        }

        // Apply callback return value
        match self.update_screen {
            Update::RefreshDomAllWindows => {
                result = result.max(ProcessEventResult::ShouldRegenerateDomAllWindows);
            }
            Update::RefreshDom => {
                result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
            }
            Update::DoNothing => {}
        }

        result
    }
}
```

**Why a newtype?** A plain `(Vec<CallbackChange>, Vec<SystemChange>)` allows the
caller to destructure and process only one. The newtype's `process()` method
guarantees both are handled. You literally cannot access the inner Vecs — the only
way to consume a `FrameChanges` is `process()`, which handles everything.

### Processing methods on PlatformWindowV2

```rust
trait PlatformWindowV2 {
    /// Process a single user-initiated change.
    /// Adding a new CallbackChange variant → compile error here.
    fn apply_user_change(&mut self, change: &CallbackChange) -> ProcessEventResult {
        match change {
            // === LayoutWindow-only (apply directly) ===
            CallbackChange::InsertText { .. } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.text_input_manager.insert_text(..);
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }
            CallbackChange::MoveCursorLeft { .. } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.cursor_manager.move_left(..);
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }
            // ... all immediate changes ...

            // === Platform-level (need OS APIs) ===
            CallbackChange::AddTimer { timer_id, timer } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.timers.insert(*timer_id, timer.clone());
                }
                self.start_timer(timer_id.id, timer.clone());
                ProcessEventResult::DoNothing
            }
            CallbackChange::ModifyWindowState { state } => {
                self.apply_window_state_modification(state);
                ProcessEventResult::ShouldReRenderCurrentWindow
            }
            CallbackChange::OpenMenu { menu, position } => {
                self.show_menu_from_callback(menu, *position);
                ProcessEventResult::DoNothing
            }
            // ... all platform changes ...

            // === Content changes ===
            CallbackChange::ChangeNodeText { .. } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.apply_text_change(..);
                }
                ProcessEventResult::ShouldIncrementalRelayout
            }
            CallbackChange::UpdateAllImageCallbacks => {
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            // === Propagation control (consumed by dispatch loop, no-op here) ===
            CallbackChange::StopPropagation
            | CallbackChange::StopImmediatePropagation
            | CallbackChange::PreventDefault => ProcessEventResult::DoNothing,
        }
    }

    /// Process a single system-initiated change.
    /// Adding a new SystemChange variant → compile error here.
    fn apply_system_change(&mut self, change: &SystemChange) -> ProcessEventResult {
        match change {
            SystemChange::FocusNode { target } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.focus_manager.set_focus(*target);
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }
            SystemChange::StartCursorBlinkTimer => {
                if let Some(lw) = self.get_layout_window() {
                    let timer = lw.create_cursor_blink_timer(self.get_current_window_state());
                    self.start_timer(CURSOR_BLINK_TIMER_ID.id, timer);
                }
                ProcessEventResult::DoNothing
            }
            SystemChange::StopCursorBlinkTimer => {
                self.stop_timer(CURSOR_BLINK_TIMER_ID.id);
                ProcessEventResult::DoNothing
            }
            SystemChange::ActivateWindowDrag => {
                self.handle_begin_interactive_move();
                ProcessEventResult::DoNothing
            }
            SystemChange::StartAutoScrollTimer => {
                let timer = create_autoscroll_timer();
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.add_timer(DRAG_AUTOSCROLL_TIMER_ID, timer.clone());
                }
                self.start_timer(DRAG_AUTOSCROLL_TIMER_ID.id, timer);
                ProcessEventResult::ShouldReRenderCurrentWindow
            }
            SystemChange::RestyleNodes { changes } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.apply_restyle(changes);
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }
            // ... exhaustive ...
        }
    }
}
```

**Both methods use exhaustive `match`**. Adding a variant to either enum → compile
error in the corresponding method.

### The event processing pipeline

```rust
/// In process_window_events_recursive_v2:
fn process_window_events_recursive_v2(&mut self, depth: u32) -> ProcessEventResult {
    let mut frame_changes = FrameChanges::empty();

    // 1. Pre-callback system events (text selection, etc.)
    let pre_events = self.compute_pre_callback_system_events(&synthetic_events);
    frame_changes.add_system_changes(pre_events);

    // 2. Dispatch user callbacks
    let (user_changes, prevent_default) = self.dispatch_callbacks(&events);
    frame_changes.add_user_changes(user_changes);

    // 3. Post-callback system events (focus, drag, auto-scroll)
    let post_events = self.compute_post_callback_system_events(prevent_default, ..);
    frame_changes.add_system_changes(post_events);

    // 4. Process everything — COMPILER ENFORCED
    let result = frame_changes.process(self);

    // 5. Recurse if needed (synthetic events from window state changes)
    if needs_recursion {
        let recursive_result = self.process_window_events_recursive_v2(depth + 1);
        result = result.max(recursive_result);
    }

    result
}
```

### Timer/thread tick handler: single trait default method

```rust
trait PlatformWindowV2 {
    fn process_timers_and_threads(&mut self) -> ProcessEventResult {
        let mut result = ProcessEventResult::DoNothing;

        // Timers produce user changes (from timer callbacks)
        let timer_frame_changes = self.invoke_expired_timers(); // returns Vec<FrameChanges>
        for fc in timer_frame_changes {
            self.save_previous_window_state();
            let r = fc.process(self);
            self.sync_window_state();
            result = result.max(r);
        }

        // Threads produce user changes (from writeback callbacks)
        if let Some(thread_fc) = self.invoke_thread_callbacks() {
            self.save_previous_window_state();
            let r = thread_fc.process(self);
            self.sync_window_state();
            result = result.max(r);
        }

        result
    }
}
```

Each platform:
```rust
// macOS tick_timers, Windows WM_TIMER, X11 select timeout, Wayland timerfd
let result = self.process_timers_and_threads();
if result >= ProcessEventResult::ShouldReRenderCurrentWindow {
    self.frame_needs_regeneration = true;
    self.trigger_platform_redraw();
}
```

8× copy-paste → 1 implementation + 4 one-line call sites.

---

## 5. How NOTE/IDEMPOTENT/TODO Comments Disappear

| Comment | Location | Root cause | How it disappears |
|---------|---------|------------|-------------------|
| `IDEMPOTENT: If an NSTimer already exists...` | `macos/mod.rs:1870` | Timer changes applied in two places | Single `apply_user_change(AddTimer)` — one path, no idempotency needed |
| `NOTE: These are ALSO processed by process_callback_result_v2...` | `event_v2.rs:3636` | `invoke_expired_timers()` applies timers to `layout_window`, then `process_callback_result_v2` applies them again via `start_timer()` | `invoke_expired_timers()` returns `FrameChanges`. The `process()` method applies both logical and platform timer in one match arm. |
| `NOTE: We do NOT invoke the image callbacks here...` | `event_v2.rs:3084` | `process_callback_result_v2` used to call image callbacks (wrong — should only happen during WebRender transaction). Developer left 7-line warning. | `apply_user_change(UpdateAllImageCallbacks)` returns `ShouldReRenderCurrentWindow`. The match arm is 1 line. No place to accidentally add wrong invocation. |
| `NOTE: We must save previous state BEFORE modifying...` | `event_v2.rs:2824` | Ordering dependency between `previous_window_state` save and `current_window_state` modification | `process_timers_and_threads()` calls `save_previous_window_state()` → `fc.process()` → `sync_window_state()` in structural order. |
| `IMPORTANT: Hit tests must already be done by platform layer!` | `event_v2.rs:1423` | `process_window_events_recursive_v2` assumes hit tests are fresh, but no type enforces this | `SystemChange` doesn't include hit test updates — those are lifecycle hooks (category 6). Separation makes the boundary explicit. |
| 8× platform tick handler boilerplate | macOS, Windows, X11, Wayland | No trait default method | `process_timers_and_threads()` default method. Each platform calls it + triggers redraw. |
| Dead fields `should_scroll_render`, `cursor_changed` | `CallCallbacksResult` | No way to detect unused struct fields | These structs are deleted. No fields to forget. |

---

## 6. What Is NOT a `SystemChange`

Categories 6 and 7 from the catalog (platform events, layout engine) are NOT
`SystemChange` variants. They are **lifecycle hooks** that happen at fixed points
in the frame cycle:

- **Platform input recording** (scroll, mouse, keyboard) → happens BEFORE event
  processing, not as a "change" to process
- **Hit test update** → happens at a fixed point in the platform event handler
- **Layout engine scroll registration** → happens AFTER layout, not during
  event processing
- **DOM reconciliation / state migration** → happens during DOM rebuild, separate
  lifecycle
- **Window resize** → platform event that triggers the whole cycle, not a change
  within it

The distinction: `CallbackChange` and `SystemChange` are **things to DO** that were
determined during one event processing cycle. Lifecycle hooks are **when things
happen** — they're structural, not data.

---

## 7. Why Two Enums Instead of One?

Option A: Merge system changes into `CallbackChange`
```rust
enum Change {
    // User changes
    AddTimer { .. },
    ModifyWindowState { .. },
    // System changes
    FocusNode { .. },
    StartCursorBlinkTimer,
    ActivateWindowDrag,
}
```

**Problems**:
1. User callbacks could push `FocusNode` or `StartCursorBlinkTimer` — these are
   internal framework actions that shouldn't be exposed via `CallbackInfo`.
2. The `CallbackInfo` API methods map 1:1 to `CallbackChange` variants. Mixing in
   system variants breaks this clean correspondence.
3. `CallbackChange` goes through `api.json` → C FFI → language bindings. System
   changes are internal-only and shouldn't be in the public API.
4. System changes are determined by the *framework analysis* of events (hit tests,
   gesture detection, focus rules). They have different lifetimes and trust levels
   than user changes.

Option B: Two separate enums ← **This is the right answer.**
- `CallbackChange` = public API surface, pushed by user code via `CallbackInfo`
- `SystemChange` = internal, created by framework event analysis
- Both processed through `FrameChanges::process()`, both with exhaustive `match`

---

## 8. Migration Path

### Phase 1: Create `SystemChange` enum (low risk)

1. Define `SystemChange` enum in `core/src/events.rs` (next to `ProcessEventResult`)
2. The existing `PostCallbackSystemEvent` and `PreCallbackSystemEvent` enums are
   precursors — migrate their variants into `SystemChange`
3. Add `apply_system_change()` default method to `PlatformWindowV2`
4. Convert inline system processing in `process_window_events_recursive_v2` to
   push `SystemChange` variants, then process via match

### Phase 2: Convert user changes to `FrameChanges` (medium risk)

1. Create `FrameChanges` newtype
2. Convert `invoke_single_callback()` to return `FrameChanges` instead of
   `CallCallbacksResult`
3. Change `apply_callback_changes()` to return `Vec<CallbackChange>` (deferred only)
   instead of `CallbackChangeResult`
4. Delete `CallbackChangeResult`, `CallCallbacksResult`, `merge_into()`,
   `needs_processing()`, `needs_redraw()`
5. Add `apply_user_change()` default method to `PlatformWindowV2`
6. Delete `process_callback_result_v2()`

### Phase 3: Unify platform tick handlers (low risk)

1. Add `process_timers_and_threads()` default method to `PlatformWindowV2`
2. Replace 8× platform boilerplate with one-liner calling the default method
3. Each platform just adds the platform-specific redraw trigger

### Phase 4: Clean up (low risk)

1. Remove dead fields (`should_scroll_render`, `cursor_changed`)
2. Remove NOTE/IDEMPOTENT comments that are no longer relevant
3. Update `api.json` to expose `CallbackChange`-based API (if needed)

---

## 9. What "Adding a Feature" Looks Like After

### Adding a new user callback capability (e.g., `update_all_image_callbacks`)

| # | File | What | Compiler enforced? |
|---|------|------|---|
| 1 | `CallbackChange` enum | Add `UpdateAllImageCallbacks` variant | — |
| 2 | `CallbackInfo` | Add `update_all_image_callbacks()` method that pushes it | — |
| 3 | `TimerCallbackInfo` | Add delegating method | — |
| 4 | `apply_user_change()` | Add match arm → return `ShouldReRenderCurrentWindow` | **YES — compile error if missing** |
| 5 | `api.json` | FFI binding | — |

**5 places** instead of 20. Step 4 is compiler-enforced.

### Adding a new system behavior (e.g., start cursor blink on focus)

| # | File | What | Compiler enforced? |
|---|------|------|---|
| 1 | `SystemChange` enum | Add `StartCursorBlinkTimer` variant | — |
| 2 | Event analysis code | Push it when focus changes to contenteditable | — |
| 3 | `apply_system_change()` | Add match arm → call `self.start_timer()` | **YES — compile error if missing** |

**3 places**. Step 3 is compiler-enforced.

### What's eliminated entirely

- No `CallbackChangeResult` field to add
- No `CallCallbacksResult` field to add
- No `CallCallbacksResult::empty()` to update
- No `merge_into()` to update
- No `needs_processing()` to update
- No `needs_redraw()` to update
- No 8× platform tick handlers to update
- No NOTE comments explaining "don't forget to also do X"

---

## 10. Summary

| Aspect | Current | Proposed |
|--------|---------|----------|
| User change representations | 4 (enum → struct → struct → if-blocks) | 1 (enum → exhaustive match) |
| System change representation | None (ad-hoc inline code) | 1 (enum → exhaustive match) |
| Compile-time safety | Only in `apply_callback_changes()` | In `apply_user_change()` AND `apply_system_change()` |
| Places to change for new user feature | 20 | 5 |
| Places to change for new system behavior | ~3 (find right spot in 700-line function) | 3 (all with exhaustive match) |
| Platform tick handler copies | 8 | 1 (trait default method) |
| "Can you forget to handle a change?" | Yes — silent bug | No — compile error |
| NOTE/IDEMPOTENT comments needed | ~10 | 0 |
| Dead fields | 2+ | 0 (no flat structs) |

The architecture becomes: **two exhaustive enums, processed in a loop, wrapped in a
newtype that makes it impossible to skip either one**.
