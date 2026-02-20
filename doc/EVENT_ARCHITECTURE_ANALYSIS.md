# Event Architecture Analysis — Brittleness & Simplification

## Executive Summary

The current event architecture suffers from a **field-explosion problem**: every new
capability a callback can request requires changes in **7+ places** across the codebase,
and forgetting any one of them causes a silent bug. The root cause is that
`CallCallbacksResult` is a flat struct with 27 fields, and each field has its own
ad-hoc handling scattered across `CallbackChange` → `apply_callback_changes()` →
`CallCallbacksResult` → `needs_processing()` / `needs_redraw()` →
`process_callback_result_v2()` → 4× platform tick handlers.

This document catalogs the architectural issues and proposes simplifications.

---

## 1. The Field-Explosion Problem

### Current flow for adding a new callback capability

Say we want to add a new field `foo_changed` that a callback can set and that
should trigger a rerender. Today, this requires edits in:

| # | File | What to add |
|---|------|------------|
| 1 | `layout/src/callbacks.rs` — `CallbackChange` enum | New variant `FooChanged { ... }` |
| 2 | `layout/src/callbacks.rs` — `CallbackInfo` methods | New method `set_foo(...)` that pushes the change |
| 3 | `layout/src/timer.rs` — `TimerCallbackInfo` methods | Delegating method `set_foo(...)` → `callback_info.set_foo(...)` |
| 4 | `layout/src/window.rs` — `CallbackChangeResult` struct | New field `foo_changed` |
| 5 | `layout/src/window.rs` — `apply_callback_changes()` | Match arm for `FooChanged` → set the field |
| 6 | `layout/src/callbacks.rs` — `CallCallbacksResult` struct | New field `foo_changed` |
| 7 | `layout/src/callbacks.rs` — `CallCallbacksResult::empty()` | Initialize `foo_changed: Default` |
| 8 | `layout/src/callbacks.rs` — `needs_processing()` | Add `|| self.foo_changed` check |
| 9 | `layout/src/callbacks.rs` — `needs_redraw()` | Add `|| self.foo_changed` check (if visual) |
| 10 | `dll/src/desktop/shell2/common/event_v2.rs` — `process_callback_result_v2()` | Handle `foo_changed` |
| 11 | `api.json` | Add FFI binding for `set_foo()` |

**And** the following places have copy-pasted boilerplate that must stay in sync:

| # | File | Copy-paste |
|---|------|-----------|
| 12 | `dll/src/desktop/shell2/macos/mod.rs` — GLView `tick_timers` | Timer result → `process_callback_result_v2` → redraw |
| 13 | `dll/src/desktop/shell2/macos/mod.rs` — CPUView `tick_timers` | Same |
| 14 | `dll/src/desktop/shell2/macos/mod.rs` — `render_and_present` (threads) | Same |
| 15 | `dll/src/desktop/shell2/windows/mod.rs` — `WM_TIMER` (timers) | Same |
| 16 | `dll/src/desktop/shell2/windows/mod.rs` — `WM_TIMER` (threads 0xFFFF) | Same |
| 17 | `dll/src/desktop/shell2/linux/x11/mod.rs` — `check_timers_and_threads` | Same |
| 18 | `dll/src/desktop/shell2/linux/x11/mod.rs` — inline in event loop (threads) | Same |
| 19 | `dll/src/desktop/shell2/linux/wayland/mod.rs` — `check_timers_and_threads` | Same |
| 20 | `dll/src/desktop/shell2/linux/wayland/mod.rs` — inline in event loop (threads) | Same |

That is **up to 20 places** to change for a single new field. If the developer
forgets `needs_processing()`, the field is silently dropped for timer/thread callbacks.
If they forget `needs_redraw()`, the screen doesn't update. If they forget one of
the 8 platform copy-pastes, one platform silently ignores the result.

### Bug history proving the brittleness

Every bug fixed in this session was a missed step in this chain:

1. **`needs_processing()` was incomplete** — didn't check `update_all_image_callbacks` → timer results silently dropped
2. **`needs_redraw()` was incomplete** — didn't check `update_all_image_callbacks` → screen didn't update
3. **All platforms discarded `process_callback_result_v2` return value** — `let _ = self.process_callback_result_v2(...)` → no redraw on timer/thread results
4. **`process_callback_result_v2` invoked image callbacks redundantly** — because it was unclear that `wr_translate2` already does this
5. **macOS `render_and_present` called `invoke_expired_timers()` too** — double invocation because the boundary between "timer tick" and "render" was unclear

---

## 2. Architectural Smells

### 2.1. `CallCallbacksResult` is a God Struct

27 fields, growing with every feature. It conflates:

- **Side-effects to apply** (timers, threads, window state, focus, scroll)
- **Content changes** (text, images, CSS properties)
- **Event control** (stopPropagation, preventDefault)
- **Visual hints** (needs redraw, needs relayout)
- **Platform instructions** (open menu, show tooltip, begin interactive move)

The struct has become a bag of "everything a callback might want to do", with no
grouping, no invariants, and no way to verify completeness.

### 2.2. Dual Result Path: `CallbackChange` enum vs. `CallCallbacksResult` struct

Callbacks push `CallbackChange` variants into a `Vec`. Then `apply_callback_changes()`
converts them into `CallbackChangeResult` (intermediate). Then the caller converts
that into `CallCallbacksResult`. There are **three** representations of the same data:

```
CallbackChange (enum, 40+ variants)
    ↓ apply_callback_changes()
CallbackChangeResult (struct, ~20 fields)
    ↓ manual conversion in invoke_single_callback / run_single_timer
CallCallbacksResult (struct, 27 fields)
    ↓ needs_processing() / needs_redraw()
ProcessEventResult (enum, 7 variants)
```

`CallbackChangeResult` and `CallCallbacksResult` are nearly identical structs with
different field names. This is pure duplication.

### 2.3. Platform Boilerplate: 8× Copy-Paste of Timer/Thread Result Handling

Every platform has this exact pattern (shown once, copied 8 times):

```rust
let timer_results = self.invoke_expired_timers();
let mut needs_redraw = false;
for result in &timer_results {
    if result.needs_processing() {
        self.previous_window_state = Some(self.current_window_state.clone());
        let process_result = self.process_callback_result_v2(result);
        self.sync_window_state();
        if process_result >= ProcessEventResult::ShouldReRenderCurrentWindow {
            needs_redraw = true;
        }
    }
    if result.needs_redraw() {
        needs_redraw = true;
    }
}
if needs_redraw {
    self.frame_needs_regeneration = true;
    // platform-specific redraw trigger
}
```

This appears in macOS GLView `tick_timers`, macOS CPUView `tick_timers`,
macOS `render_and_present` (for threads), Windows `WM_TIMER` (two branches),
X11 `check_timers_and_threads`, X11 inline event loop, Wayland `check_timers_and_threads`,
Wayland inline event loop.

Any fix to this logic must be replicated 8 times.

### 2.4. `needs_processing()` vs. `needs_redraw()` is Redundant

`needs_processing()` gates whether `process_callback_result_v2()` is called.
`needs_redraw()` gates whether the platform triggers a rerender.

But `process_callback_result_v2()` already returns a `ProcessEventResult` that tells
the platform whether to rerender. So `needs_redraw()` is a **second, parallel**
determination of the same question, and they can (and did) disagree.

The correct architecture: always call `process_callback_result_v2()` (it's cheap when
there's nothing to do), and use its return value.

### 2.5. Timer Double-Counting in `invoke_expired_timers()`

`invoke_expired_timers()` applies timer/thread changes directly to `layout_window`:

```rust
borrows.layout_window.timers.insert(*timer_id, timer.clone());
```

Then the caller also calls `process_callback_result_v2()`, which calls
`self.start_timer()` / `self.stop_timer()` for the same timer IDs. This works
only because `start_timer()` was made idempotent (invalidate old NSTimer first).

This is a fragile invariant. The correct fix is to have **one** path, not two
that must agree.

### 2.6. Image Callback Invocation: Two Paths, One Correct

Image callbacks can be invoked in two places:

1. `process_callback_result_v2()` in `event_v2.rs` (was the **wrong** path — produced textures that were discarded because they were never registered with WebRender)
2. `wr_translate2::process_image_callback_updates()` (the **correct** path — registers textures with WebRender's image store)

We fixed this by removing invocation from path 1, but the design doesn't prevent
someone from re-adding it. There's no documentation or type-level guarantee that
image callbacks should only be invoked during transaction building.

### 2.7. `ProcessEventResult` Ordering is Fragile

The enum uses `max()` to combine results, and the ordering determines priority:

```
DoNothing < ReRender < UpdateDisplayList < HitTest < IncrementalRelayout < RegenerateDom
```

But `UpdateDisplayList` (2) < `IncrementalRelayout` (4) means that if one callback
returns `UpdateDisplayList` and another returns `IncrementalRelayout`, only
`IncrementalRelayout` is performed. This is correct because incremental relayout
is a superset of display list update. But this invariant is implicit and depends
on the ordering matching the subset relationship of the operations.

---

## 3. Proposed Simplifications

### 3.1. Unify `CallbackChangeResult` and `CallCallbacksResult`

Delete `CallbackChangeResult`. Have `apply_callback_changes()` return
`CallCallbacksResult` directly. The two structs have almost identical fields —
the only difference is some naming and the `Update` field.

**Impact**: Removes ~200 lines of conversion code, eliminates one representation.

### 3.2. Replace `needs_processing()` / `needs_redraw()` with Always-Process

Instead of:
```rust
if result.needs_processing() {
    let process_result = self.process_callback_result_v2(result);
    ...
}
if result.needs_redraw() {
    needs_redraw = true;
}
```

Just:
```rust
let process_result = self.process_callback_result_v2(result);
if process_result >= ProcessEventResult::ShouldReRenderCurrentWindow {
    needs_redraw = true;
}
```

`process_callback_result_v2()` already checks each field and returns `DoNothing`
when there's nothing to do. The gating functions add no value — they just duplicate
the "does this result have work?" check and can fall out of sync.

**Impact**: Removes `needs_processing()` and `needs_redraw()` entirely. Eliminates
two functions that must be manually kept in sync with `process_callback_result_v2()`.

### 3.3. Move Timer/Thread Result Handling into the Trait

The 8× copy-pasted timer/thread handling should be a default method on
`PlatformWindowV2`:

```rust
/// Process all timer and thread callback results and return whether a redraw is needed.
fn process_timer_and_thread_results(&mut self) -> bool {
    let mut needs_redraw = false;

    // Timers
    let timer_results = self.invoke_expired_timers();
    for result in &timer_results {
        self.save_previous_window_state();
        let process_result = self.process_callback_result_v2(result);
        self.sync_window_state();
        if process_result >= ProcessEventResult::ShouldReRenderCurrentWindow {
            needs_redraw = true;
        }
    }

    // Threads
    if let Some(thread_result) = self.invoke_thread_callbacks() {
        self.save_previous_window_state();
        let process_result = self.process_callback_result_v2(&thread_result);
        self.sync_window_state();
        if process_result >= ProcessEventResult::ShouldReRenderCurrentWindow {
            needs_redraw = true;
        }
    }

    needs_redraw
}
```

Each platform then just calls:
```rust
if self.process_timer_and_thread_results() {
    self.frame_needs_regeneration = true;
    // platform-specific redraw trigger
}
```

**Impact**: Reduces 8× copies to 1 implementation + 4-5 one-line call sites.
Adding new logic (e.g., a new callback type) only needs to change one place.

### 3.4. Introduce `ProcessEventResult::from(CallCallbacksResult)` 

Instead of 200+ lines in `process_callback_result_v2()` that manually map each
field to a `ProcessEventResult`, group the fields into categories with a clear
mapping:

```rust
impl CallCallbacksResult {
    /// What is the minimum ProcessEventResult this result requires?
    pub fn required_event_result(&self) -> ProcessEventResult {
        use ProcessEventResult::*;

        if self.callbacks_update_screen == Update::RefreshDomAllWindows {
            return ShouldRegenerateDomAllWindows;
        }
        if self.callbacks_update_screen == Update::RefreshDom {
            return ShouldRegenerateDomCurrentWindow;
        }
        if self.has_layout_affecting_changes() {
            return ShouldIncrementalRelayout;
        }
        if self.has_display_list_changes() {
            return ShouldUpdateDisplayListCurrentWindow;
        }
        if self.has_any_visual_change() {
            return ShouldReRenderCurrentWindow;
        }
        DoNothing
    }
}
```

The side-effects (starting timers, updating focus, etc.) still need to be applied
by `process_callback_result_v2()`, but the "what level of reprocessing do we need?"
question has a single, central answer.

### 3.5. Group `CallCallbacksResult` Fields into Sub-Structs

Instead of 27 flat fields, group them:

```rust
pub struct CallCallbacksResult {
    /// What the callbacks told us to do
    pub update_screen: Update,
    
    /// Side effects to apply to the window/platform
    pub side_effects: CallbackSideEffects,
    
    /// Content changes (text, images, CSS)
    pub content_changes: CallbackContentChanges,
    
    /// Event propagation control
    pub propagation: PropagationControl,
}

pub struct CallbackSideEffects {
    pub modified_window_state: Option<FullWindowState>,
    pub focus_change: FocusUpdateRequest,
    pub timers: TimerChanges,        // added + removed
    pub threads: ThreadChanges,      // added + removed
    pub scroll_changes: Option<...>,
    pub windows_created: Vec<...>,
    pub menus_to_open: Vec<...>,
    pub tooltips: TooltipChanges,
    pub begin_interactive_move: bool,
}

pub struct CallbackContentChanges {
    pub words_changed: Option<...>,
    pub images_changed: Option<...>,
    pub image_masks_changed: Option<...>,
    pub image_callbacks_changed: Option<...>,
    pub update_all_image_callbacks: bool,
    pub css_properties_changed: Option<...>,
}

pub struct PropagationControl {
    pub stop_propagation: bool,
    pub stop_immediate_propagation: bool,
    pub prevent_default: bool,
}
```

This makes it visually obvious what's a side-effect vs. content vs. control,
and sub-structs can have their own `is_empty()` methods.

### 3.6. Remove Dual Timer Application Path

Currently:
1. `invoke_expired_timers()` applies timer changes to `layout_window.timers` directly
2. `process_callback_result_v2()` applies them again via `start_timer()` / `stop_timer()`

Both are needed because (1) handles inter-callback ordering within one tick, and
(2) manages platform-specific timers (NSTimer, SetTimer, timerfd).

**Proposed**: Split `process_callback_result_v2()` to have a `process_timer_changes()`
method that ONLY manages platform-specific timers (create/destroy OS timer handles)
and does NOT touch `layout_window.timers`. Call it explicitly from `invoke_expired_timers()`
after applying changes to `layout_window`. This way:
- `layout_window.timers` is modified in exactly one place
- Platform timers are managed in exactly one place  
- No idempotency requirement on `start_timer()`

### 3.7. Use a Bitflag for "What Changed" Instead of Checking 27 Fields

```rust
bitflags::bitflags! {
    pub struct ChangeFlags: u32 {
        const WINDOW_STATE     = 0b0000_0001;
        const FOCUS            = 0b0000_0010;
        const TIMERS           = 0b0000_0100;
        const THREADS          = 0b0000_1000;
        const TEXT_CONTENT     = 0b0001_0000;
        const IMAGES           = 0b0010_0000;
        const IMAGE_CALLBACKS  = 0b0100_0000;
        const CSS_PROPERTIES   = 0b1000_0000;
        const SCROLL           = 0b0000_0001_0000_0000;
        const MENUS            = 0b0000_0010_0000_0000;
        const TOOLTIPS         = 0b0000_0100_0000_0000;
        // etc.
    }
}

impl CallCallbacksResult {
    pub fn change_flags(&self) -> ChangeFlags { ... }
}
```

Each `apply_callback_changes` arm sets the corresponding flag. `needs_processing()`
becomes `!change_flags.is_empty()`, `needs_redraw()` becomes
`change_flags.intersects(VISUAL_FLAGS)`. Impossible to forget.

---

## 4. Priority Ranking

| Priority | Change | Effort | Bug-prevention value |
|----------|--------|--------|---------------------|
| **P0** | 3.3: Deduplicate platform timer/thread handling | Small | High — eliminates 8× copy-paste |
| **P0** | 3.2: Remove `needs_processing()` / `needs_redraw()` | Small | High — eliminates the #1 source of bugs |
| **P1** | 3.1: Unify `CallbackChangeResult` / `CallCallbacksResult` | Medium | Medium — removes a pointless conversion step |
| **P1** | 3.6: Remove dual timer application | Medium | Medium — removes fragile idempotency requirement |
| **P2** | 3.5: Group fields into sub-structs | Medium | Medium — makes structure clearer |
| **P2** | 3.7: Bitflags for change tracking | Medium | High — makes "what changed?" machine-verifiable |
| **P3** | 3.4: `required_event_result()` method | Small | Low — nice to have, not critical |

---

## 5. What Would "Good" Look Like?

Adding a new callback capability (like `update_all_image_callbacks` was) should require
changes in exactly **3 places**:

1. Add a `CallbackChange` variant (the canonical definition of the capability)
2. Add a `CallbackInfo` method to push the change  
3. Handle the change in `apply_callback_changes()`

Everything else — needs_processing, needs_redraw, process_callback_result_v2, platform
handlers — should derive automatically from the change variant.

This means:
- `needs_processing()` → always true, or derived from change_flags
- `needs_redraw()` → derived from change_flags or ProcessEventResult return
- Platform handlers → single shared implementation via trait default method
- `process_callback_result_v2()` → driven by sub-struct grouping, not 27 if-checks

The goal is that **forgetting a step produces a compile error, not a silent bug**.
