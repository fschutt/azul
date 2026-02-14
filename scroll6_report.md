# scroll6 Report: Status vs SCROLL_ARCHITECTURE.md

Commit: `d3b6372f` (scroll5: complete scroll architecture cleanup)

---

## 1. Scroll Event Flow: Recording vs Actually Scrolling

### Architecture Goal (SCROLL_ARCHITECTURE §5.1)

```
Platform Event → record input → physics timer → ScrollTo → set_scroll_position → repaint
```

### Current Implementation: ✅ FULLY IMPLEMENTED

The two-phase flow is now clean and consistent across all 4 platforms:

**Phase 1: Recording (platform event handler)**
- macOS: `handle_scroll_wheel()` → `record_scroll_from_hit_test()` → `ScrollInputQueue.push()`
- Windows: `WM_MOUSEWHEEL` handler → same
- X11: `handle_scroll()` → same
- Wayland: `handle_pointer_axis()` → same

All four call `record_scroll_from_hit_test()` which does a hit-test lookup,
creates a `ScrollInput { dom_id, node_id, delta, source, timestamp }`, and
pushes it into the `Arc<Mutex<Vec<ScrollInput>>>` queue. If this is the first
input (queue was empty), the handler starts `SCROLL_MOMENTUM_TIMER_ID`.

**Phase 2: Physics + Application (timer callback)**
- `scroll_physics_timer_callback()` fires every 16ms
- Drains `queue.take_all()`
- For each input: TrackpadContinuous → direct position, WheelDiscrete → velocity impulse
- Integrates velocity decay, clamping
- Pushes `CallbackChange::ScrollTo { dom_id, node_id, position }` via `timer_info.scroll_to()`

**Phase 3: Processing (event_v2.rs)**
- `process_callback_result_v2()` handles `nodes_scrolled_in_callbacks`
- For each `(dom_id, node_id, position)`:
  - Calls `scroll_manager.scroll_to()` (instant, duration=0)
  - Calls `iframe_manager.check_reinvoke()` (transparent IFrame support)
  - If IFrame needs re-invocation → `ShouldRegenerateDomCurrentWindow`
  - Otherwise → `ShouldReRenderCurrentWindow`

**What was removed:**
- `EventProvider` impl for ScrollManager (scroll events no longer come through synthetic event system)
- `begin_frame()` / `end_frame()` / `record_sample()` / `process_scroll_event()` (dead code from old frame-based approach)
- `ScrollEvent` struct (was only used as a bag of fields in `gpu_scroll()`)
- `had_scroll_activity` / `had_programmatic_scroll` / `had_new_doms` flags
- `previous_offset` from `AnimatedScrollState`

---

## 2. The Three Sizes

### Architecture Goal (SCROLL_ARCHITECTURE §1.1)

| Term | Definition |
|------|-----------|
| **Scroll clip size** | Container's inner box (what the user sees) |
| **Content size** | Total children extent (what can be scrolled through) |
| **Virtual scroll size** | Logical total for lazy/infinite scroll (may be >> content size) |

### Current Implementation

**Regular scroll: ✅ CORRECT**
- `container_rect` = scroll clip size (set by `register_or_update_scroll_node()` from layout)
- `content_rect` = total children extent (from `overflow_content_size`)
- `clamp()` computes `max_scroll = content_rect.size - container_rect.size`
- `get_scroll_node_info()` returns both rects + computed max_scroll_x/y

**Virtual scroll (IFrame): ✅ STORED, ⚠️ PARTIALLY USED**

`AnimatedScrollState` now has:
```rust
pub virtual_scroll_size: Option<LogicalSize>,
pub virtual_scroll_offset: Option<LogicalPosition>,
```

These are propagated from IFrame callbacks via `update_virtual_scroll_bounds()`,
called at both `update_iframe_info()` sites in `window.rs`.

- `clamp()`: ✅ uses `virtual_scroll_size` when available
- `get_scroll_node_info()`: ✅ uses `virtual_scroll_size` for max_scroll computation

### ⚠️ BUG: `calculate_scrollbar_states()` ignores virtual_scroll_size

The scrollbar geometry calculation (`calculate_scrollbar_states()` at line 747)
still uses `content_rect.size` directly for both the **visibility filter** and the
**thumb ratio** calculation:

```rust
// Filter: only uses content_rect, not virtual size
.filter(|(_, s)| s.content_rect.size.height > s.container_rect.size.height)

// Thumb ratio: uses content_rect, not virtual size
let thumb_size_ratio = (container_height / content_height).min(1.0);
let max_scroll = (content_height - container_height).max(0.0);
```

For virtual scrolling, the scrollbar should reflect the **virtual total** not the
**rendered subset**. Example: 100,000 rows × 20px = 2,000,000px virtual, but only
50 rows are rendered = 1,000px content. Scrollbar should show thumb_size = clip/2M.

**Fix needed:** `calculate_vertical_scrollbar_static()` and
`calculate_horizontal_scrollbar_static()` should read `virtual_scroll_size`
(when set) instead of `content_rect.size`.

### ⚠️ BUG: `is_node_scrollable()` ignores virtual_scroll_size

```rust
fn is_node_scrollable(&self, dom_id: DomId, node_id: NodeId) -> bool {
    self.states.get(&(dom_id, node_id)).map_or(false, |state| {
        let has_horizontal = state.content_rect.size.width > state.container_rect.size.width;
        let has_vertical = state.content_rect.size.height > state.container_rect.size.height;
        has_horizontal || has_vertical
    })
}
```

Should also check `virtual_scroll_size` — a node with virtual_size 2M but
content_rect 500px (less than container 600px) would incorrectly be marked
non-scrollable.

---

## 3. Body Scroll Bug (SCROLL_ARCHITECTURE §3)

### Architecture Goal (§3.4)

> `apply_content_based_height()` should not expand the node if it has
> `overflow: scroll` or `overflow: auto`.

### Current Implementation: ✅ FIXED

In `cache.rs` line 1705:
```rust
let is_scroll_container = dom_id.map_or(false, |id| {
    let ov_x = get_overflow_x(...);
    let ov_y = get_overflow_y(...);
    matches!(ov_x, ... Scroll | Auto) || matches!(ov_y, ... Scroll | Auto)
});

if should_use_content_height(&css_height) {
    let skip_expansion = is_scroll_container
        && containing_block_size.height.is_finite()
        && containing_block_size.height > 0.0;
    if !skip_expansion {
        final_used_size = apply_content_based_height(...);
    }
}
```

This correctly prevents `<body>` (or any `overflow:scroll` node) from expanding
to content height, so the scrollbar sees `container < content` and works.

---

## 4. Physics and Rubber-Banding

### Architecture Goal

Smooth scroll with velocity decay for mouse wheel, direct passthrough for trackpad.

### Current Implementation: ✅ IMPLEMENTED (basic physics, no rubber-banding)

**Velocity-based physics** (for `WheelDiscrete`):
- Impulse: `velocity += delta * WHEEL_IMPULSE_MULTIPLIER * 60.0`
- Decay: `velocity *= exp(-FRICTION_DECAY_RATE * dt * 60.0)` where `FRICTION_DECAY_RATE = 0.05`
- Threshold: snaps to zero when `|velocity| < 0.5 px/s`
- Edge kill: velocity zeroed when clamped at boundary

**Direct passthrough** (for `TrackpadContinuous`):
- Position = current + delta (OS handles momentum)
- Kills any existing velocity for the node

**Rubber-banding**: ❌ NOT IMPLEMENTED
- Scroll position is hard-clamped to `[0, max_scroll]`
- No overscroll bounce effect
- This is acceptable for V1 — rubber-banding can be added later by allowing
  temporary overshoot in `clamp()` and adding a spring-back animation

---

## 5. Timer Lifecycle

### One Timer Per Window (not per node): ✅ CORRECT

- Timer ID: `SCROLL_MOMENTUM_TIMER_ID = TimerId { id: 0x0002 }` (single constant)
- One timer per window, multiplexes all nodes via `ScrollPhysicsState.node_velocities: BTreeMap<(DomId, NodeId), NodeScrollPhysics>`
- The `ScrollInputQueue` is shared (`Arc<Mutex>`) between the platform handler and the timer

### Self-Termination: ✅ CORRECT

```rust
if physics.is_active() || any_changes {
    TimerCallbackReturn { should_update: ..., should_terminate: Continue }
} else {
    TimerCallbackReturn::terminate_unchanged()
}
```

`is_active()` checks:
1. `input_queue.has_pending()` — new inputs from platform
2. Any node with `|velocity| > VELOCITY_STOP_THRESHOLD`
3. Any `pending_positions` (trackpad/programmatic that haven't been pushed yet)

When all three are false and there were no changes this tick → `Terminate`.

### Timer Restart: ✅ CORRECT

`record_scroll_from_hit_test()` returns `should_start_timer = true` only when
the queue was previously empty. The platform handler checks this and calls
`self.start_timer(SCROLL_MOMENTUM_TIMER_ID.id, timer)` — if the timer is
already running (queue wasn't empty), it just adds to the queue without
restarting.

### Auto-Scroll Timer (for drag-selection): ✅ SEPARATE TIMER

`DRAG_AUTOSCROLL_TIMER_ID = TimerId { id: 0x0003 }` — completely separate from
scroll momentum. Uses `find_scroll_parent()` + `get_scroll_node_info()` to find
the container, calculates delta from mouse proximity to edges, pushes `ScrollTo`.
Does NOT self-terminate (runs as long as drag-selection is active).

---

## 6. IFrame Re-Invocation (SCROLL_ARCHITECTURE §4)

### Architecture Goal

Timer pushes ScrollTo → processing code checks IFrame → re-invokes if needed.
Timer knows nothing about IFrames.

### Current Implementation: ✅ WIRED UP

In `process_callback_result_v2()` (event_v2.rs line 2993):
```rust
// After setting scroll position:
if let Some(_reason) = layout_window.iframe_manager.check_reinvoke(
    *dom_id, node_id, &layout_window.scroll_manager, layout_bounds,
) {
    needs_iframe_reinvoke = true;
}
```

And virtual bounds propagation in `window.rs`:
```rust
self.scroll_manager.update_virtual_scroll_bounds(
    parent_dom_id, node_id,
    callback_return.virtual_scroll_size,
    Some(callback_return.scroll_offset),
);
```

---

## 7. Summary

| Component | SCROLL_ARCHITECTURE Goal | Status |
|-----------|-------------------------|--------|
| Recording vs scrolling separation | Two-phase: record → timer → ScrollTo | ✅ Done (all 4 platforms) |
| Scroll clip size (container_rect) | From layout, not expanded for overflow:scroll | ✅ Done (§3.4 fix in cache.rs) |
| Content size (content_rect) | Total children extent | ✅ Done |
| Virtual scroll size | Optional, from IFrame callback | ✅ Stored + used in clamp/get_info |
| Scrollbar uses virtual size | thumb_ratio = clip / virtual_total | ⚠️ BUG: still uses content_rect |
| is_node_scrollable checks virtual | Should be scrollable if virtual > container | ⚠️ BUG: still uses content_rect |
| Physics (velocity decay) | Mouse wheel momentum | ✅ Exponential decay |
| Rubber-banding (overscroll bounce) | Bounce at edges | ❌ Not implemented (hard clamp) |
| Timer: one per window | Single SCROLL_MOMENTUM_TIMER_ID | ✅ Correct |
| Timer: self-terminating | Terminates when all velocities zero + no inputs | ✅ Correct |
| Timer: restart on new input | Only starts when queue was empty | ✅ Correct |
| IFrame re-invocation | Transparent in ScrollTo processing | ✅ Wired up |
| Auto-scroll timer | Separate DRAG_AUTOSCROLL_TIMER_ID | ✅ Correct |
| CPU renderer scroll support | Not implemented (TODO in SCROLL_ARCHITECTURE §5) | ❌ Known gap |

### Remaining Work (2 bugs + 1 future enhancement)

1. **BUG: `calculate_scrollbar_states()` ignores `virtual_scroll_size`** — scrollbar
   thumb ratio and visibility check use `content_rect` instead of virtual bounds.
   Fix: check `virtual_scroll_size.unwrap_or(content_rect.size)` in the static
   helper methods and filter predicates.

2. **BUG: `is_node_scrollable()` ignores `virtual_scroll_size`** — same issue.
   Fix: check `virtual_scroll_size.unwrap_or(content_rect.size)` for comparison.

3. **Enhancement: Rubber-banding** — Currently hard-clamped. Add overscroll bounce
   by allowing temporary overshoot in `clamp()` + spring-back animation. Low priority.
