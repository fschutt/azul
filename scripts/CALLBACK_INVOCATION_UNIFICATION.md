# Callback Invocation Architecture — Unification Analysis

## 1. Current Architecture

There are **4 callback invocation paths** in `layout/src/window.rs` on `LayoutWindow`,
plus **1 event-dispatch path** in `dll/src/desktop/shell2/common/event_v2.rs`:

| # | Method | Location | Called by |
|---|--------|----------|-----------|
| A | `run_single_timer()` | `window.rs:3395` | `invoke_expired_timers()` in `event_v2.rs:3559` |
| B | `run_all_threads()` | `window.rs:3625` | `invoke_thread_callbacks()` in `event_v2.rs:3680` |
| C | `invoke_single_callback()` | `window.rs:3910` | `dispatch_events_propagated()` in `event_v2.rs:1075` |
| D | `invoke_menu_callback()` | `window.rs:4095` | macOS/Windows/Linux menu item dispatch |
| E | `dispatch_events_propagated()` | `event_v2.rs:832` | `process_window_events_recursive_v2()` |

All 4 paths (A-D) follow the **same 6-step pattern**:

```
1. Create CallbackInfo (from CallbackInfoRefData + callback_changes Arc<Mutex>)
2. Invoke the callback (timer/thread/single/menu — different fn signatures)
3. Extract callback_changes from Arc<Mutex>
4. Call self.apply_callback_changes(collected_changes, ...) → CallbackChangeResult
5. Merge CallbackChangeResult into CallCallbacksResult
6. Return CallCallbacksResult
```

Path E wraps path C in a W3C propagation loop and processes results via
`process_callback_result_v2()`.

## 2. The Problem: ~150 Lines of Copy-Paste Per Path

### 2.1 CallCallbacksResult Construction (28 fields, copy-pasted 4×)

Every path constructs `CallCallbacksResult` with 28 named fields:

```rust
let mut ret = CallCallbacksResult {
    should_scroll_render: false,
    callbacks_update_screen: Update::DoNothing,
    modified_window_state: None,
    // ... 25 more fields ...
};
```

This literal appears at lines **3418, 3641, 3922, 4111** — identical each time.

### 2.2 Intermediate Accumulators (copy-pasted 3×)

Paths B/C/D (which may merge results from multiple callbacks) declare 10 intermediate
`ret_*` accumulator variables:

```rust
let mut ret_modified_window_state = current_window_state.clone();
let mut ret_timers = FastHashMap::new();
let mut ret_timers_removed = FastBTreeSet::new();
let mut ret_words_changed = BTreeMap::new();
// ... 6 more ...
```

### 2.3 CallbackChangeResult → CallCallbacksResult Merging (~50 lines, duplicated 4×)

The transfer from `CallbackChangeResult` to `CallCallbacksResult` is repeated in
each path with minor variations:

- **Path A (timer)**: Uses `if !x.is_empty() { ret.x = Some(x); }` directly
  (handles `image_callbacks_changed`, `update_all_image_callbacks` ✔)
- **Path B (thread)**: Uses `for (dom_id, nodes) in` loops into accumulators,
  then `if !ret_x.is_empty() { ret.x = Some(ret_x); }`
  (MISSING `image_callbacks_changed`, `update_all_image_callbacks` ✘)
- **Path C (single)**: Uses `.extend()` into accumulators
  (MISSING `image_callbacks_changed`, `update_all_image_callbacks` ✘)
- **Path D (menu)**: Same as C
  (MISSING `image_callbacks_changed`, `update_all_image_callbacks` ✘)

**Every time a new field is added to `CallbackChangeResult` or `CallCallbacksResult`,
all 4 paths must be updated.** This is the root cause of the bugs where
`image_callbacks_changed` and `update_all_image_callbacks` were only forwarded in
the timer path.

### 2.4 CallbackInfoRefData Construction (~15 lines, copy-pasted 4×)

```rust
let ref_data = crate::callbacks::CallbackInfoRefData {
    layout_window: self,
    renderer_resources,
    previous_window_state,
    current_window_state,
    gl_context,
    current_scroll_manager: &current_scroll_states,
    current_window_handle,
    system_callbacks,
    system_style,
    monitors: self.monitors.clone(),
    #[cfg(feature = "icu")]
    icu_localizer: self.icu_localizer.clone(),
    ctx: ...,  // only this varies
};
```

## 3. Why It Matters for Future Extensibility

Adding a new callback source (notification callback, tray menu callback,
file-watcher callback, clipboard callback, etc.) currently requires:

1. Write a new `invoke_xxx_callback()` method (~120 lines)
2. Copy the `CallCallbacksResult` construction (28 fields)
3. Copy the `CallbackInfoRefData` construction (12 fields)
4. Copy the `CallbackChangeResult → CallCallbacksResult` merging (~50 lines)
5. Copy the accumulator finalization (~25 lines)
6. Remember to handle ALL fields (easy to miss)

This is ~240 lines of boilerplate per new callback type, and each new field
on `CallCallbacksResult` or `CallbackChangeResult` requires updating every path.

## 4. Proposed Unification

### 4.1 `CallbackChangeResult` gets a `merge_into(ret: &mut CallCallbacksResult)` method

Move the transfer logic out of each callback path and into a method on
`CallbackChangeResult` itself. This is the single most impactful change:

```rust
impl CallbackChangeResult {
    /// Merge this result into a CallCallbacksResult accumulator.
    /// Handles all fields uniformly — new fields only need to be added here.
    pub fn merge_into(self, ret: &mut CallCallbacksResult) {
        ret.stop_propagation = ret.stop_propagation || self.stop_propagation;
        ret.prevent_default = ret.prevent_default || self.prevent_default;
        ret.tooltips_to_show.extend(self.tooltips_to_show);
        ret.hide_tooltip = ret.hide_tooltip || self.hide_tooltip;
        ret.begin_interactive_move = ret.begin_interactive_move || self.begin_interactive_move;

        if self.hit_test_update_requested.is_some() {
            ret.hit_test_update_requested = self.hit_test_update_requested;
        }

        // Timers/threads: merge into Option<HashMap>
        merge_option_map(&mut ret.timers, self.timers);
        merge_option_map(&mut ret.threads, self.threads);
        merge_option_set(&mut ret.timers_removed, self.timers_removed);
        merge_option_set(&mut ret.threads_removed, self.threads_removed);

        // DOM changes: merge BTreeMap<DomId, BTreeMap<NodeId, T>>
        merge_nested_map(&mut ret.words_changed, self.words_changed);
        merge_nested_map(&mut ret.images_changed, self.images_changed);
        merge_nested_map(&mut ret.image_masks_changed, self.image_masks_changed);
        merge_nested_map(&mut ret.css_properties_changed, self.css_properties_changed);
        merge_nested_set(&mut ret.image_callbacks_changed, self.image_callbacks_changed);
        merge_nested_map(&mut ret.nodes_scrolled_in_callbacks, self.nodes_scrolled);

        ret.update_all_image_callbacks = ret.update_all_image_callbacks
            || self.update_all_image_callbacks;

        if self.modified_window_state != ret.modified_window_state
                .as_ref()
                .unwrap_or(&FullWindowState::default()) // compare to current
        {
            ret.modified_window_state = Some(self.modified_window_state);
        }

        if let Some(ft) = self.focus_target {
            // Resolve immediately while we have layout_results access,
            // or store raw and resolve later
            ret.pending_focus_target = Some(ft);
        }

        if !self.queued_window_states.is_empty() {
            ret.queued_window_states.extend(self.queued_window_states);
        }
        if !self.text_input_triggered.is_empty() {
            ret.text_input_triggered.extend(self.text_input_triggered);
        }
    }
}
```

**Impact**: Eliminates ~200 lines of duplicated merging code across 4 paths.
When a new field is added, it only needs to be handled in ONE place.

### 4.2 `CallCallbacksResult::default()` replaces literal construction

Replace the 28-field literal with `CallCallbacksResult::default()`:

```rust
// Before (28 lines, repeated 4×):
let mut ret = CallCallbacksResult {
    should_scroll_render: false,
    callbacks_update_screen: Update::DoNothing,
    ...
};

// After (1 line):
let mut ret = CallCallbacksResult::default();
```

The existing `impl Default for CallCallbacksResult` block already does this
(at `callbacks.rs:3886`), but it's never used!

**Impact**: -108 lines  (4 × 27 lines of field initialization).

### 4.3 Unified `invoke_callback_generic()` helper

The core 6-step pattern can be extracted into a single helper:

```rust
impl LayoutWindow {
    /// Generic callback invocation: create CallbackInfo, invoke callback,
    /// collect changes, apply them, merge into result.
    fn invoke_and_collect<F>(
        &mut self,
        hit_dom_node: DomNodeId,
        cursor_relative_to_item: OptionLogicalPosition,
        cursor_in_viewport: OptionLogicalPosition,
        ctx: OptionRefAny,
        // Common params (could be a struct)
        current_window_handle: &RawWindowHandle,
        gl_context: &OptionGlContextPtr,
        image_cache: &mut ImageCache,
        system_fonts: &mut FcFontCache,
        system_style: Arc<SystemStyle>,
        system_callbacks: &ExternalSystemCallbacks,
        previous_window_state: &Option<FullWindowState>,
        current_window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        // The actual callback invocation (closure)
        invoke_fn: F,
    ) -> (Update, CallbackChangeResult)
    where
        F: FnOnce(CallbackInfo) -> Update,
    {
        let scroll_states = self.get_nested_scroll_states(DomId::ROOT_ID);
        let callback_changes = Arc::new(Mutex::new(Vec::new()));

        let ref_data = CallbackInfoRefData {
            layout_window: self,
            renderer_resources,
            previous_window_state,
            current_window_state,
            gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle,
            system_callbacks,
            system_style,
            monitors: self.monitors.clone(),
            #[cfg(feature = "icu")]
            icu_localizer: self.icu_localizer.clone(),
            ctx,
        };

        let callback_info = CallbackInfo::new(
            &ref_data, &callback_changes,
            hit_dom_node, cursor_relative_to_item, cursor_in_viewport,
        );

        let update = invoke_fn(callback_info);

        let collected = callback_changes.lock()
            .map(|mut g| core::mem::take(&mut *g))
            .unwrap_or_default();

        let change_result = self.apply_callback_changes(
            collected, current_window_state, image_cache, system_fonts,
        );

        self.queue_iframe_updates(change_result.iframes_to_update.clone());

        (update, change_result)
    }
}
```

Then each path becomes ~15 lines instead of ~120:

```rust
// Timer path:
let (update, change_result) = self.invoke_and_collect(
    hit_dom_node, cursor_relative_to_item, cursor_in_viewport,
    timer_ctx,
    /* common params */,
    |callback_info| {
        let timer = self.timers.get_mut(&TimerId { id: timer_id }).unwrap();
        let tcr = timer.invoke(&callback_info, &system_callbacks.get_system_time_fn);
        should_terminate = tcr.should_terminate;
        tcr.should_update
    },
);
ret.callbacks_update_screen = update;
change_result.merge_into(&mut ret);
```

**Impact**: Each new callback type is ~15 lines + its specific invocation logic,
instead of ~120 lines with error-prone field forwarding.

### 4.4 Common Parameters as a Struct

The 9 "environment" parameters that every callback path takes can be grouped:

```rust
/// Environment available during callback invocation.
/// Shared across timer, thread, single-callback, and menu paths.
pub struct CallbackEnv<'a> {
    pub current_window_handle: &'a RawWindowHandle,
    pub gl_context: &'a OptionGlContextPtr,
    pub image_cache: &'a mut ImageCache,
    pub system_fonts: &'a mut FcFontCache,
    pub system_style: Arc<SystemStyle>,
    pub system_callbacks: &'a ExternalSystemCallbacks,
    pub previous_window_state: &'a Option<FullWindowState>,
    pub current_window_state: &'a FullWindowState,
    pub renderer_resources: &'a RendererResources,
}
```

This would reduce each function signature from 11 parameters to 2
(`&mut self, env: &mut CallbackEnv`).

## 5. Future Callback Types — What They'd Look Like

With the unified architecture, adding a new callback source is trivial:

### 5.1 Notification Callback

```rust
pub fn invoke_notification_callback(
    &mut self,
    notification_id: NotificationId,
    action: NotificationAction, // Clicked, Dismissed, ActionButton(idx)
    env: &mut CallbackEnv,
) -> CallCallbacksResult {
    let mut ret = CallCallbacksResult::default();
    let hit_dom_node = DomNodeId::root(); // Notifications aren't DOM-attached

    let (update, change_result) = self.invoke_and_collect(
        hit_dom_node, OptionLogicalPosition::None, OptionLogicalPosition::None,
        OptionRefAny::None, env,
        |callback_info| {
            let cb = self.notification_callbacks.get(&notification_id)?;
            (cb.callback.cb)(cb.refany.clone(), notification_id, action, callback_info)
        },
    );
    ret.callbacks_update_screen = update;
    change_result.merge_into(&mut ret);
    ret
}
```

### 5.2 Tray Menu Callback

```rust
pub fn invoke_tray_menu_callback(
    &mut self,
    menu_item_id: TrayMenuItemId,
    env: &mut CallbackEnv,
) -> CallCallbacksResult {
    let mut ret = CallCallbacksResult::default();

    let (update, change_result) = self.invoke_and_collect(
        DomNodeId::root(), OptionLogicalPosition::None, OptionLogicalPosition::None,
        OptionRefAny::None, env,
        |callback_info| {
            let cb = self.tray_callbacks.get(&menu_item_id)?;
            (cb.callback.cb)(cb.refany.clone(), callback_info)
        },
    );
    ret.callbacks_update_screen = update;
    change_result.merge_into(&mut ret);
    ret
}
```

### 5.3 File Watcher Callback

```rust
pub fn invoke_file_watcher_callback(
    &mut self,
    path: &Path,
    change_type: FileChangeType,
    env: &mut CallbackEnv,
) -> CallCallbacksResult { ... } // same ~10 line pattern
```

## 6. Migration Plan

### Phase 1: Zero-Risk Mechanical Refactors (no behavior change)

1. **Add `CallCallbacksResult::default()` usage** — replace all 4 literal constructions
   with `CallCallbacksResult::default()`. The existing `impl Default` already exists
   but is unused. Just add `update_all_image_callbacks: false` to it (already done).

2. **Add `CallbackChangeResult::merge_into()`** — implement the method and replace
   the forwarding code in all 4 paths. This fixes the missing-field bugs (B/C/D don't
   forward `image_callbacks_changed` / `update_all_image_callbacks`) automatically.

3. **Add `CallbackEnv` struct** — replace 9 parameters with 1 struct on all 4 methods.

### Phase 2: Extract `invoke_and_collect()` (minor signature change)

4. Extract the common `CallbackInfoRefData` + `CallbackInfo::new()` + `apply_callback_changes()`
   sequence into a generic helper. Closures handle the specific invocation.

### Phase 3: Add New Callback Types

5. Each new callback type (notification, tray, file watcher, clipboard, etc.)
   is ~15 lines of code using the unified infrastructure.

## 7. Line Count Impact

| Component | Before | After | Saved |
|-----------|--------|-------|-------|
| `CallCallbacksResult` construction (×4) | 112 | 4 | **108** |
| Accumulator declarations (×3) | 33 | 0 | **33** |
| `CallbackChangeResult` → `CallCallbacksResult` merging (×4) | 200 | 4 | **196** |
| Accumulator finalization (×3) | 75 | 0 | **75** |
| `CallbackInfoRefData` construction (×4) | 60 | 4 | **56** |
| **Total** | **480** | **12** | **~468** |

Plus: each future callback type goes from **~120 lines** to **~15 lines**.

## 8. Field Consistency Audit (Current Bugs)

Fields forwarded from `CallbackChangeResult` → `CallCallbacksResult`:

| Field | Timer (A) | Thread (B) | Single (C) | Menu (D) |
|-------|-----------|------------|------------|----------|
| `stop_propagation` | ✔ | ✔ | ✔ | ✔ |
| `prevent_default` | ✔ | ✔ | ✔ | ✔ |
| `tooltips_to_show` | ✔ | ✔ | ✔ | ✔ |
| `hide_tooltip` | ✔ | ✔ | ✔ | ✔ |
| `begin_interactive_move` | ✔ | ✔ | ✔ | ✔ |
| `hit_test_update_requested` | ✔ | ✔ | ✔ | ✔ |
| `timers` | ✔ | ✔ | ✔ | ✔ |
| `threads` | ✔ | ✔ | ✔ | ✔ |
| `timers_removed` | ✔ | ✔ | ✔ | ✔ |
| `threads_removed` | ✔ | ✔ | ✔ | ✔ |
| `modified_window_state` | ✔ | ✔ | ✔ | ✔ |
| `words_changed` | ✔ | ✔ | ✔ | ✔ |
| `images_changed` | ✔ | ✔ | ✔ | ✔ |
| `image_masks_changed` | ✔ | ✔ | ✔ | ✔ |
| `css_properties_changed` | ✔ | ✔ | ✔ | ✔ |
| `nodes_scrolled` | ✔ | ✔ | ✔ | ✔ |
| `focus_target` | ✔ | ✔ | ✔ | ✔ |
| `queued_window_states` | ✔ | ✘ | ✘ | ✘ |
| `text_input_triggered` | ✔ | ✘ | ✘ | ✘ |
| **`image_callbacks_changed`** | **✔** | **✘** | **✘** | **✘** |
| **`update_all_image_callbacks`** | **✔** | **✘** | **✘** | **✘** |
| `windows_created` | ✘ | ✘ | ✘ | ✘ |
| `menus_to_open` | ✘ | ✘ | ✘ | ✘ |
| `iframes_to_update` | ✔ (via queue) | ✔ (via queue) | ✔ (via queue) | ✔ (via queue) |

**Bugs found: 6 fields not forwarded in paths B/C/D.**

The `merge_into()` approach fixes all of these automatically since the method
handles every field in one place.

## 9. Scroll Physics Timer — Architecture & Hookup Analysis

### 9.1 Intended Architecture

```text
Platform Event Handler (macOS/Windows/X11/Wayland)
  → ScrollManager.record_scroll_from_hit_test(delta, source, ...)
  → ScrollInputQueue.push(ScrollInput)
  → start SCROLL_MOMENTUM_TIMER_ID timer (if not already running)

Timer fires (every ~16ms):
  → scroll_physics_timer_callback (layout/src/scroll_timer.rs)
    1. queue.take_recent(100)  — consume pending inputs
    2. Physics integration (velocity, friction, rubber-banding)
    3. push_change(CallbackChange::ScrollTo) for each updated node
    4. Return Update::RefreshDom + Continue (or TerminateTimer)

Timer result flows back:
  → run_single_timer()         → CallCallbacksResult { nodes_scrolled_in_callbacks, ... }
  → invoke_expired_timers()    → Vec<CallCallbacksResult>
  → Platform tick_timers()     → process_callback_result_v2(result)
                                  → scroll_manager.scroll_to() for each node
                                  → generate_frame()
                                  → scroll_all_nodes(txn) sends offsets to WebRender
```

### 9.2 Per-Platform Timer Result Processing

| Platform | Checks `nodes_scrolled_in_callbacks`? | Calls `process_callback_result_v2`? | Scroll Works? |
|----------|--------------------------------------|-------------------------------------|---------------|
| macOS GLView (`tick_timers` line 435) | ✔ `has_scroll_changes` | ✔ conditionally | ✔ |
| macOS CPUView (`tick_timers` line 982) | ✔ `has_scroll_changes` | ✔ conditionally | ✔ |
| Windows (`WM_TIMER` line 2881) | ✘ **MISSING** | ✔ only for window_state/queued/text | ✘ **BROKEN** |
| X11 epoll path (line 1386) | ✘ **MISSING** | ✔ only for window_state/queued/text | ✘ **BROKEN** |
| X11 `check_timers_and_threads` (line 2826) | ✘ **MISSING** | ✘ not called at all | ✘ **BROKEN** |
| Wayland epoll path (line 1545) | ✘ **MISSING** | ✔ only for window_state/queued/text | ✘ **BROKEN** |

**Bug**: On Windows, X11, and Wayland, the `needs_processing` check is:
```rust
let needs_processing = result.modified_window_state.is_some()
    || !result.queued_window_states.is_empty()
    || !result.text_input_triggered.is_empty();
// MISSING: || result.nodes_scrolled_in_callbacks.as_ref().map(|s| !s.is_empty()).unwrap_or(false)
```

This means `process_callback_result_v2()` is NOT called when only scroll changes
are present. The `nodes_scrolled_in_callbacks` is silently dropped, so
`scroll_manager.scroll_to()` is never called, `scroll_all_nodes()` sends stale
offsets to WebRender, and **scrolling does not work on these platforms**.

macOS has the correct check (added separately):
```rust
let has_scroll_changes = result.nodes_scrolled_in_callbacks.as_ref()
    .map(|s| !s.is_empty()).unwrap_or(false);
if has_window_changes || has_scroll_changes || has_text_input { ... }
```

### 9.3 X11 `check_timers_and_threads` — Additional Bug

The X11 path at `linux/x11/mod.rs:2826` is a separate simplified timer check that
doesn't call `process_callback_result_v2` at all:

```rust
fn check_timers_and_threads(&mut self) {
    let timer_results = self.invoke_expired_timers();
    if !timer_results.is_empty() {
        self.frame_needs_regeneration = true;  // blindly set, no result processing!
    }
    ...
}
```

This means scroll changes, window state changes, focus changes, etc. from timer
callbacks are all silently dropped on this code path.

### 9.4 Over-Rendering: `Update::RefreshDom` Instead of Scroll-Only Update

The scroll physics timer returns `Update::RefreshDom` when scroll positions change
(scroll_timer.rs line 350):

```rust
should_update: if any_changes {
    Update::RefreshDom       // <-- Forces FULL DOM regeneration!
} else {
    Update::DoNothing
},
```

In `invoke_expired_timers()` this triggers `mark_frame_needs_regeneration()`, which
leads to `generate_frame_if_needed()` → `generate_frame(display_list_was_rebuilt=true)`.
This causes:
- Full font/image resource re-collection
- Full display list rebuild for all DOMs
- Full WebRender scene builder pass

**All of this is unnecessary for scrolling.** WebRender natively supports scroll
frame transforms — only `scroll_all_nodes()` + `txn.generate_frame()` is needed.
The existing code in `generate_frame()` already handles this when
`display_list_was_rebuilt=false`:

```rust
} else {
    txn.skip_scene_builder();
}
// ... later, always runs:
scroll_all_nodes(layout_window, txn);
synchronize_gpu_values(layout_window, txn);
txn.generate_frame(0, WrRenderReasons::empty());
```

So the rendering infrastructure already supports scroll-only updates. The problem is:
1. The scroll timer returns `Update::RefreshDom` (too heavy)
2. `invoke_expired_timers` maps `RefreshDom` → `mark_frame_needs_regeneration`
3. `generate_frame_if_needed` always passes `display_list_was_rebuilt=true`

### 9.5 The Double-Path Problem: ScrollTo via `apply_callback_changes` vs `process_callback_result_v2`

The `CallbackChange::ScrollTo` flow has a confusing two-step handoff:

1. **`apply_callback_changes()`** (window.rs:1997): Collects `ScrollTo` changes
   into `CallbackChangeResult.nodes_scrolled` — does NOT touch `ScrollManager`.

2. **`process_callback_result_v2()`** (event_v2.rs:2994): Takes `nodes_scrolled_in_callbacks`
   from `CallCallbacksResult` and calls `scroll_manager.scroll_to()` for each node.

This means the actual scroll position update requires BOTH steps to complete. If
`process_callback_result_v2` is not called (as on Windows/X11/Wayland), the
ScrollManager never receives the new positions.

The `nodes_scrolled` field uses `NodeHierarchyItemId` (public API type),
which is converted to `NodeId` in `process_callback_result_v2`. This conversion
is the only reason the split exists — `apply_callback_changes` operates on
`NodeHierarchyItemId` from the C API, but `ScrollManager` needs `NodeId`.

### 9.6 Proposed Fix

#### Fix 1: Add `has_scroll_changes` check on all platforms

Add the missing check to Windows, X11 epoll, X11 check_timers_and_threads,
and Wayland timer handlers:

```rust
// All platforms must check:
let has_scroll_changes = result.nodes_scrolled_in_callbacks.as_ref()
    .map(|s| !s.is_empty()).unwrap_or(false);
let needs_processing = result.modified_window_state.is_some()
    || !result.queued_window_states.is_empty()
    || !result.text_input_triggered.is_empty()
    || has_scroll_changes;  // <-- Add this
```

For X11 `check_timers_and_threads`, also call `process_callback_result_v2`:
```rust
fn check_timers_and_threads(&mut self) {
    let timer_results = self.invoke_expired_timers();
    for result in &timer_results {
        let needs_processing = result.modified_window_state.is_some()
            || !result.queued_window_states.is_empty()
            || !result.text_input_triggered.is_empty()
            || result.nodes_scrolled_in_callbacks.as_ref()
                .map(|s| !s.is_empty()).unwrap_or(false);
        if needs_processing {
            self.previous_window_state = Some(self.current_window_state.clone());
            let _ = self.process_callback_result_v2(result);
        }
        if matches!(result.callbacks_update_screen,
            Update::RefreshDom | Update::RefreshDomAllWindows) {
            self.frame_needs_regeneration = true;
        }
    }
}
```

#### Fix 2: Scroll-only frame generation (no display list rebuild)

Option A: **New `Update` variant** — Add `Update::ScrollOnly` (or reuse
`ShouldReRenderCurrentWindow`):
- `scroll_physics_timer_callback` returns `Update::ScrollOnly` instead of `RefreshDom`
- Platform timer handlers check for `ScrollOnly` → call
  `generate_frame(display_list_was_rebuilt=false)` instead of full rebuild

Option B: **Separate scroll flag** — Check `nodes_scrolled_in_callbacks` on
`CallCallbacksResult` directly (already available):
- If `needs_redraw && only_scroll_changes`, call
  `generate_frame(display_list_was_rebuilt=false)` 
- If `needs_redraw && dom_changes`, call
  `generate_frame(display_list_was_rebuilt=true)`

Option C: **Remove `RefreshDom` from scroll timer entirely** — The scroll timer
uses `push_change(CallbackChange::ScrollTo)`, which flows through
`process_callback_result_v2 → scroll_manager.scroll_to()`. The timer can return
`Update::DoNothing` (no DOM regeneration needed), and the platform handler checks
`has_scroll_changes` to decide on a scroll-only re-render. This is the cleanest
approach because:
- No new `Update` variant needed
- `process_callback_result_v2` already sets `ShouldReRenderCurrentWindow` for
  scroll changes
- Platform code just needs to generate a frame with scroll updates

```rust
// scroll_timer.rs — proposed change:
should_update: Update::DoNothing,  // Don't trigger DOM refresh
should_terminate: TerminateTimer::Continue,
// The ScrollTo changes flow through nodes_scrolled_in_callbacks
// and are processed by process_callback_result_v2, which already
// returns ShouldReRenderCurrentWindow.
```

Platform handlers would then need:
```rust
// After processing timer results:
if has_scroll_changes && !needs_dom_rebuild {
    // Scroll-only: update scroll positions + repaint, skip display list rebuild
    generate_frame(display_list_was_rebuilt=false);
}
```

#### Fix 3: Unify timer result processing across platforms

This is the same `merge_into()` / `invoke_and_collect()` unification from
Section 4. Once timer result processing lives in a single cross-platform
method (e.g., `process_timer_results()` on `PlatformWindowV2`), the
`has_scroll_changes` check only needs to be written once.

### 9.7 Scroll Flow — Consistency Audit

| Step | macOS | Windows | X11 (epoll) | X11 (poll) | Wayland |
|------|-------|---------|-------------|------------|---------|
| Platform records scroll input | ✔ | ✔ | ✔ | ✔ | ✔ |
| Starts SCROLL_MOMENTUM_TIMER | ✔ | ✔ | ✔ | ✔ | ✔ |
| Timer fires, physics runs | ✔ | ✔ | ✔ | ✔ | ✔ |
| `push_change(ScrollTo)` | ✔ | ✔ | ✔ | ✔ | ✔ |
| `apply_callback_changes` → `nodes_scrolled` | ✔ | ✔ | ✔ | ✔ | ✔ |
| `nodes_scrolled` → `CallCallbacksResult` | ✔ | ✔ | ✔ | ✔ | ✔ |
| `invoke_expired_timers` returns result | ✔ | ✔ | ✔ | ✔ | ✔ |
| `process_callback_result_v2` called with scroll | ✔ | ✘ | ✘ | ✘ | ✘ |
| `scroll_manager.scroll_to()` applied | ✔ | ✘ | ✘ | ✘ | ✘ |
| IFrame re-invocation check | ✔ | ✘ | ✘ | ✘ | ✘ |
| `generate_frame` with correct `display_list_was_rebuilt` | ✘ (always true) | ✘ | ✘ | ✘ | ✘ |
| `scroll_all_nodes` sends offsets to WebRender | ✔ (but stale on non-macOS) | ✘ | ✘ | ✘ | ✘ |
