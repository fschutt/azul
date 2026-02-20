# Event Architecture Analysis — Brittleness & Simplification

## Executive Summary

The current event architecture suffers from a **representation explosion**: callbacks
produce `Vec<CallbackChange>` (an enum with exhaustive `match` support), but this
is immediately flattened into `CallbackChangeResult` (struct, ~25 fields), then
converted again into `CallCallbacksResult` (struct, 27 fields), then checked by
`needs_processing()` and `needs_redraw()` (manual field enumeration), then processed
by `process_callback_result_v2()` (200+ lines of if-blocks), and finally the
platform-specific tick handlers (8× copy-paste) decide whether to redraw.

The key insight: **`CallbackChange` is already the canonical, exhaustive representation**.
Rust's `match` statement guarantees that adding a new variant causes a compile error
everywhere that processes changes. But all that compile-time safety is thrown away the
moment we convert into flat structs — from that point forward, every consumer must
manually enumerate fields, and forgetting one is a silent bug.

The fix: **keep `Vec<CallbackChange>` as the primary data structure all the way through
processing**. Delete `CallbackChangeResult`, `CallCallbacksResult`, `needs_processing()`,
`needs_redraw()`, and `process_callback_result_v2()`. Replace them with a single
`match`-based processing loop.

---

## 1. Current Architecture: Four Representations of the Same Data

### The pipeline today

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

### What gets lost at each conversion

| Step | What's lost |
|------|------------|
| `Vec<CallbackChange>` → `CallbackChangeResult` | **Exhaustive match IS used** here (good). But the output is a flat struct where "was this field set?" must be checked with `is_some()` / `is_empty()` |
| `CallbackChangeResult` → `CallCallbacksResult` | `merge_into()` manually forwards ~20 fields. `CallbackChangeResult` has `iframes_to_update` that isn't in `CallCallbacksResult` (handled separately). Fields can fall out of sync. |
| `CallCallbacksResult` → `needs_processing()` | Manual enumeration of 20+ conditions. Adding a field to `CallCallbacksResult` without adding it here → silent bug. **This is where `update_all_image_callbacks` was missed.** |
| `CallCallbacksResult` → `needs_redraw()` | Separate manual enumeration with DIFFERENT conditions. Must stay in sync with `needs_processing()` AND `process_callback_result_v2()`. |
| `CallCallbacksResult` → `process_callback_result_v2()` | 200+ lines of independent if-blocks. Each field handled separately. No compile-time guarantee that all fields are handled. |

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

## 2. The Core Insight: `CallbackChange` Already Has What We Need

`CallbackChange` is an enum with 40+ variants. Rust's exhaustive `match` means adding
a new variant produces a **compile error** everywhere that pattern-matches on it.

The entire chain of `CallbackChangeResult` → `CallCallbacksResult` →
`needs_processing()` / `needs_redraw()` → `process_callback_result_v2()` exists only
because the code converts the enum into flat structs too early. Once you're in
struct-land, the compiler can't help you anymore.

### What `apply_callback_changes()` actually does

Looking at the current `apply_callback_changes()` match statement, the 40+ variants
fall into **two categories**:

1. **Immediate side-effects on LayoutWindow** (~25 variants):
   Applied directly in the match arm. Examples: `InsertText` (modifies text_input_manager),
   `MoveCursorLeft` (modifies cursor_manager), `SetCursorVisibility` (toggles blink),
   `ScrollIntoView` (modifies scroll_manager), `AddImageToCache` (modifies image_cache),
   `ReloadSystemFonts` (rebuilds font cache), `SetDragData` / `AcceptDrop` (modifies
   gesture_drag_manager). These don't need to be forwarded anywhere — they're done.

2. **Deferred changes for the platform layer** (~15 variants):
   Stored in `CallbackChangeResult` fields for later processing by the shell layer.
   Examples: `AddTimer` → needs platform `start_timer()`, `ModifyWindowState` → needs
   platform window update, `OpenMenu` → needs platform menu API, `ShowTooltip` → needs
   platform tooltip, `CreateNewWindow` → needs platform window creation.

The bug-prone path is category (2): these changes must survive three struct conversions
and be checked in five manual enumeration functions.

### Why category (2) exists

The deferred changes can't be applied in `apply_callback_changes()` because that
function only has `&mut LayoutWindow` — it doesn't have access to platform APIs
(NSTimer, HWND, wl_surface, etc.). So the changes must be forwarded to
`process_callback_result_v2()` which has `&mut self` on `PlatformWindowV2`.

---

## 3. Proposed Architecture: Keep `Vec<CallbackChange>` Through to Processing

### The key change

Instead of converting `Vec<CallbackChange>` → struct → struct → if-blocks, keep the
`Vec<CallbackChange>` as-is and process it directly with a `match` in the
`PlatformWindowV2` trait:

```
User callback runs
    ↓ pushes to Arc<Mutex<Vec<CallbackChange>>>
Vec<CallbackChange>
    ↓ Phase 1: apply_immediate_changes() on LayoutWindow
    │   applies category (1) changes directly
    │   returns remaining Vec<CallbackChange> (category 2 only)
    ↓ Phase 2: apply_deferred_changes() on PlatformWindowV2  ← EXHAUSTIVE MATCH
ProcessEventResult
    ↓ platform tick handler (single trait default method)
Platform redraw trigger
```

Two steps. One representation. Compile-time exhaustiveness at every step.

### Phase 1: `apply_immediate_changes()` — on LayoutWindow

This is the existing `apply_callback_changes()` but it **only handles immediate
side-effects** (category 1). Changes that need the platform layer are left in the
Vec for Phase 2.

```rust
impl LayoutWindow {
    /// Apply changes that can be resolved with only LayoutWindow access.
    /// Returns the remaining changes that need platform-level processing.
    pub fn apply_immediate_changes(
        &mut self,
        changes: Vec<CallbackChange>,
        current_window_state: &FullWindowState,
        image_cache: &mut ImageCache,
        system_fonts: &mut FcFontCache,
    ) -> Vec<CallbackChange> {
        let mut deferred = Vec::new();

        for change in changes {
            match change {
                // === Immediate: resolved here ===
                CallbackChange::InsertText { .. } => { /* apply to text_input_manager */ }
                CallbackChange::MoveCursorLeft { .. } => { /* apply to cursor_manager */ }
                CallbackChange::ScrollIntoView { .. } => { /* apply to scroll_manager */ }
                CallbackChange::SetCursorVisibility { .. } => { /* toggle blink */ }
                CallbackChange::AddImageToCache { .. } => { /* apply to image_cache */ }
                CallbackChange::ReloadSystemFonts => { /* rebuild font cache */ }
                CallbackChange::SetDragData { .. } => { /* apply to drag_manager */ }
                // ... all other immediate changes ...

                // === Propagation control: extract but don't defer ===
                CallbackChange::StopPropagation => { /* handled by dispatch loop */ }
                CallbackChange::PreventDefault => { /* handled by dispatch loop */ }

                // === Deferred: needs platform access ===
                other @ CallbackChange::AddTimer { .. }
                | other @ CallbackChange::RemoveTimer { .. }
                | other @ CallbackChange::AddThread { .. }
                | other @ CallbackChange::RemoveThread { .. }
                | other @ CallbackChange::ModifyWindowState { .. }
                | other @ CallbackChange::CreateNewWindow { .. }
                | other @ CallbackChange::OpenMenu { .. }
                | other @ CallbackChange::ShowTooltip { .. }
                | other @ CallbackChange::HideTooltip
                | other @ CallbackChange::BeginInteractiveMove
                // Content changes that need display list / layout update:
                | other @ CallbackChange::ChangeNodeText { .. }
                | other @ CallbackChange::ChangeNodeImage { .. }
                | other @ CallbackChange::UpdateImageCallback { .. }
                | other @ CallbackChange::UpdateAllImageCallbacks
                | other @ CallbackChange::ChangeNodeCssProperties { .. }
                | other @ CallbackChange::ScrollTo { .. }
                // ... etc
                => {
                    deferred.push(other);
                }
            }
        }

        deferred
    }
}
```

**Compile-time guarantee**: Adding a new `CallbackChange` variant forces you to
handle it in this match — either as immediate or deferred. You can't forget.

### Phase 2: `apply_deferred_changes()` — on PlatformWindowV2

A new default method on `PlatformWindowV2` that processes the remaining changes.
This replaces `process_callback_result_v2()`:

```rust
trait PlatformWindowV2 {
    /// Process deferred callback changes that need platform access.
    /// Returns the visual impact level.
    fn apply_deferred_changes(
        &mut self,
        changes: &[CallbackChange],
        update_screen: Update,   // from callback return value
    ) -> ProcessEventResult {
        let mut result = ProcessEventResult::DoNothing;

        for change in changes {
            match change {
                CallbackChange::AddTimer { timer_id, timer } => {
                    if let Some(lw) = self.get_layout_window_mut() {
                        lw.timers.insert(*timer_id, timer.clone());
                    }
                    self.start_timer(timer_id.id, timer.clone());
                }
                CallbackChange::RemoveTimer { timer_id } => {
                    if let Some(lw) = self.get_layout_window_mut() {
                        lw.timers.remove(timer_id);
                    }
                    self.stop_timer(timer_id.id);
                }
                CallbackChange::ModifyWindowState { state } => {
                    // apply to current_window_state, detect mouse/keyboard changes
                    // trigger synthetic events if needed
                    result = result.max(ShouldReRenderCurrentWindow);
                }
                CallbackChange::UpdateAllImageCallbacks => {
                    result = result.max(ShouldReRenderCurrentWindow);
                }
                CallbackChange::ChangeNodeText { .. } => {
                    // apply text to StyledDom cache
                    result = result.max(ShouldIncrementalRelayout);
                }
                CallbackChange::ChangeNodeCssProperties { .. } => {
                    // apply CSS to StyledDom cache
                    // check if layout-affecting or paint-only
                    result = result.max(ShouldIncrementalRelayout);
                }
                CallbackChange::OpenMenu { menu, position } => {
                    self.show_menu_from_callback(menu, *position);
                    result = result.max(ShouldReRenderCurrentWindow);
                }
                // ... exhaustive match over all deferred variants
                _ => {
                    // Immediate changes were already handled in Phase 1
                    // This arm catches them safely (no-op)
                }
            }
        }

        // Apply the callback's Update return value
        match update_screen {
            Update::RefreshDomAllWindows => {
                self.mark_frame_needs_regeneration();
                result = result.max(ShouldRegenerateDomAllWindows);
            }
            Update::RefreshDom => {
                self.mark_frame_needs_regeneration();
                result = result.max(ShouldRegenerateDomCurrentWindow);
            }
            Update::DoNothing => {}
        }

        result
    }
}
```

**Compile-time guarantee**: Adding a new `CallbackChange` variant produces a
"non-exhaustive patterns" error here too (unless caught by `_ =>`). The developer
must consciously decide whether the new variant needs platform processing.

### What gets deleted

| Deleted | Reason |
|---------|--------|
| `CallbackChangeResult` struct (25 fields) | Replaced by `Vec<CallbackChange>` |
| `CallbackChangeResult::merge_into()` | No more struct-to-struct conversion |
| `CallbackChangeResult::resolve_focus_into()` | Focus handled inline |
| `CallCallbacksResult` struct (27 fields) | Replaced by `Vec<CallbackChange>` + `Update` |
| `CallCallbacksResult::empty()` | No more struct |
| `CallCallbacksResult::needs_processing()` | Always process; exhaustive match handles it |
| `CallCallbacksResult::needs_redraw()` | Return value of `apply_deferred_changes()` |
| `process_callback_result_v2()` (~250 lines) | Replaced by `apply_deferred_changes()` |
| 8× platform tick boilerplate | Replaced by single `process_timer_and_thread_results()` |

### What replaces `CallCallbacksResult`

The single struct that flows between callback invocation and processing:

```rust
/// Output of invoking one or more callbacks.
/// This is the ONLY intermediate representation between callback execution
/// and platform-level processing.
pub struct CallbackOutput {
    /// The callback's return value (DoNothing / RefreshDom / RefreshDomAllWindows)
    pub update_screen: Update,
    /// Deferred changes that need platform access (timers, menus, window state, etc.)
    /// These have already been filtered by apply_immediate_changes().
    pub deferred_changes: Vec<CallbackChange>,
    /// Event propagation control (extracted during dispatch, consumed by dispatch loop)
    pub stop_propagation: bool,
    pub stop_immediate_propagation: bool,
    pub prevent_default: bool,
}
```

3 fields + a Vec, vs. 27 flat fields. The Vec preserves the original enum
representation, so processing uses exhaustive `match`.

### Platform tick handler: single trait method

```rust
trait PlatformWindowV2 {
    /// Process all expired timers and pending threads.
    /// Returns whether a redraw is needed.
    fn process_timers_and_threads(&mut self) -> ProcessEventResult {
        let mut result = ProcessEventResult::DoNothing;

        // Timers
        let timer_outputs = self.invoke_expired_timers(); // returns Vec<CallbackOutput>
        for output in &timer_outputs {
            self.save_previous_window_state();
            let r = self.apply_deferred_changes(&output.deferred_changes, output.update_screen);
            self.sync_window_state();
            result = result.max(r);
        }

        // Threads
        if let Some(thread_output) = self.invoke_thread_callbacks() {
            self.save_previous_window_state();
            let r = self.apply_deferred_changes(&thread_output.deferred_changes, thread_output.update_screen);
            self.sync_window_state();
            result = result.max(r);
        }

        result
    }
}
```

Each platform just calls:
```rust
let result = self.process_timers_and_threads();
if result >= ProcessEventResult::ShouldReRenderCurrentWindow {
    self.frame_needs_regeneration = true;
    self.trigger_platform_redraw(); // setNeedsDisplay / InvalidateRect / etc
}
```

8× copy-paste → 1 implementation + 4 one-line call sites.

---

## 4. Handling Merging: Multiple Callbacks in One Event Cycle

Currently `dispatch_events_propagated()` invokes N callbacks and returns
`Vec<CallCallbacksResult>`. Each result was built from merged changes.

With the new architecture, each callback produces a `CallbackOutput`. The dispatch
function can simply concatenate the deferred_changes and max the update_screen:

```rust
fn dispatch_events_propagated(&mut self, events: &[SyntheticEvent])
    -> (CallbackOutput, bool /* prevent_default */)
{
    let mut combined = CallbackOutput::default();
    let mut any_prevent_default = false;

    for planned in planned_callbacks {
        let output = self.invoke_single_callback(planned);

        any_prevent_default |= output.prevent_default;
        combined.update_screen = combined.update_screen.max(output.update_screen);
        combined.deferred_changes.extend(output.deferred_changes);

        if output.stop_immediate_propagation { break; }
        if output.stop_propagation { /* skip to next event */ }
    }

    (combined, any_prevent_default)
}
```

This eliminates the per-callback `merge_into()` step entirely. The Vec of
CallbackChange variants is naturally mergeable (just `extend`).

---

## 5. Why Not `CallbackChange` All The Way? (The Split Justification)

Some `CallbackChange` variants must run inside `apply_callback_changes()` with
`&mut LayoutWindow` because they need layout data (cursor positions, inline layouts,
scroll states). These can't wait for `apply_deferred_changes()` because:

1. **Order matters**: `InsertText` followed by `MoveCursorRight` must see the
   text that was just inserted. Both modify LayoutWindow state.
2. **Layout data access**: `ScrollIntoView` needs layout rectangles to compute
   scroll deltas. This data is in `LayoutWindow`.
3. **LayoutWindow is behind `self.get_layout_window_mut()`** in the platform
   trait, which can't be held while also calling platform methods.

So the two-phase split is architecturally necessary. The key improvement is that
**both phases use `match` on the same enum**, so adding a variant requires handling
it in one or both phases, enforced by the compiler.

---

## 6. Impact on the Timer Double-Application Problem

Currently `invoke_expired_timers()` applies timer changes to `layout_window.timers`
directly AND then `process_callback_result_v2()` calls `start_timer()`/`stop_timer()`
again. This works only because `start_timer()` was made idempotent.

With the new architecture, timer changes flow as `CallbackChange::AddTimer` in the
deferred Vec. `apply_deferred_changes()` handles it once:

```rust
CallbackChange::AddTimer { timer_id, timer } => {
    // Update LayoutWindow (logical timer state)
    if let Some(lw) = self.get_layout_window_mut() {
        lw.timers.insert(*timer_id, timer.clone());
    }
    // Update platform (OS timer handle)
    self.start_timer(timer_id.id, timer.clone());
}
```

**One path. One place. No idempotency requirement.**

The only subtlety: within `invoke_expired_timers()`, if timer A's callback adds
timer B, and then timer B expires in the same tick, we need timer B to be in
`layout_window.timers` before we check expiration. This is handled by applying
`AddTimer` changes immediately during the timer loop (in phase 1), before the
platform-level `start_timer()` call happens (in phase 2 after the loop).

---

## 7. What "Adding a Feature" Looks Like After

Adding a new callback capability (e.g., `update_all_image_callbacks`):

| # | File | What |
|---|------|------|
| 1 | `CallbackChange` enum | Add `UpdateAllImageCallbacks` variant |
| 2 | `CallbackInfo` | Add `update_all_image_callbacks()` method that pushes it |
| 3 | `TimerCallbackInfo` | Add delegating method |
| 4 | `apply_immediate_changes()` | Add match arm (immediate or forward to deferred) |
| 5 | `apply_deferred_changes()` | Add match arm (what ProcessEventResult?) |
| 6 | `api.json` | FFI binding |

**6 places** instead of 20. Steps 4 and 5 are **compile-error enforced** by
`match`. Steps 1–3 are inherent (you're defining a new API). Step 6 is inherent
(FFI). Nothing is optional or forgettable.

Steps that are **eliminated entirely**:
- No `CallbackChangeResult` field to add
- No `CallCallbacksResult` field to add
- No `CallCallbacksResult::empty()` to update
- No `merge_into()` to update
- No `needs_processing()` to update
- No `needs_redraw()` to update
- No 8× platform tick handlers to update

---

## 8. Additional Simplifications

### 8.1. `ProcessEventResult` Ordering

The enum uses `max()` to combine results. The ordering encodes a subset relationship:

```
DoNothing ⊂ ReRender ⊂ UpdateDisplayList ⊂ HitTest ⊂ IncrementalRelayout ⊂ RegenerateDom
```

This is correct but implicit. With the new architecture, `apply_deferred_changes()`
directly returns the correct level based on match arms, so the ordering is less
critical — but it should be documented.

### 8.2. `should_scroll_render` and `cursor_changed` on `CallCallbacksResult`

These fields exist on `CallCallbacksResult` but are never set by
`apply_callback_changes()` — they're set by other code paths (scroll processing,
cursor style changes). With `CallCallbacksResult` deleted, these become local
variables in their respective processing paths, which is cleaner.

### 8.3. Propagation Control

`StopPropagation`, `StopImmediatePropagation`, and `PreventDefault` are consumed
during callback dispatch (in `dispatch_events_propagated`), not during result
processing. They should be extracted from the `Vec<CallbackChange>` during
dispatch and stored as booleans on `CallbackOutput`. The match arm in
`apply_deferred_changes` is a no-op for these.

---

## 9. Priority Ranking

| Priority | Change | Effort | Compile-safety value |
|----------|--------|--------|---------------------|
| **P0** | Replace `process_callback_result_v2()` with `apply_deferred_changes()` using `match` | Large | **Critical** — every future field gets compile-time safety |
| **P0** | Move timer/thread handling into trait default method | Small | High — eliminates 8× copy-paste |
| **P0** | Delete `CallCallbacksResult`, `needs_processing()`, `needs_redraw()` | Medium | High — eliminates 3 manual-sync points |
| **P1** | Delete `CallbackChangeResult`, merge into two-phase model | Medium | Medium — removes one data representation |
| **P1** | Introduce `CallbackOutput` as sole intermediate | Medium | Medium — clean, minimal API surface |
| **P2** | Split `apply_callback_changes()` into phase 1 + deferred Vec | Medium | Medium — clear separation of concerns |

---

## 10. Summary

The current architecture converts `CallbackChange` (compiler-enforced exhaustive enum)
into flat structs (no compiler enforcement) too early, then manually re-enumerates
fields in 5 different places that must stay in sync. Every recent bug was caused by
a forgotten field in one of these manual enumerations.

The fix: **stop converting**. Keep `Vec<CallbackChange>` as the data structure, process
it with `match` in two phases (LayoutWindow-level and Platform-level), and let Rust's
exhaustive pattern matching guarantee that new variants can't be silently ignored.
