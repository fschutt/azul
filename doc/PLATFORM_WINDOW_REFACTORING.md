# Platform Window Refactoring Plan

## Problem Statement

The `dll/src/desktop/shell2/` directory (67 files, 46,296 lines) has accumulated
layered abstractions from multiple refactoring rounds:

- **`PlatformWindow` (V1)** — lifecycle trait (poll/present/close)
- **`PlatformWindowV2`** — event processing trait (37 required methods + ~3400 lines of default methods)
- **`event_v2.rs`**, **`layout_v2.rs`** — "V2" names but no V1 equivalents exist
- **`GnomeMenuManager` (V1)** — dead code, replaced by V2
- **`WindowProperties`** — only used as V1 trait argument, never called externally
- **28 trivial getter methods** duplicated across 5 platform implementations

### Current State

```
                     PlatformWindow (V1)          PlatformWindowV2
                     10 methods                   37 required + 8 default methods
                     ─────────────────            ───────────────────────────
Implements:          Win32, macOS, X11,           Win32, macOS, X11,
                     Wayland, iOS, Linux,         Wayland, iOS
                     Stub

Used generically?    NO (never dyn/T:)            NO (never dyn/T:)
Used in run.rs?      poll_event, request_redraw   Indirectly (via event handlers)
                     is_open (only Linux)
```

### Note: Compositor / CpuCompositor

The `Compositor` trait and `CpuCompositor` in `common/compositor.rs` /
`common/cpu_compositor.rs` are **kept intentionally**. They are currently
unused but reserved for a future **anti-grain geometry (AGG) CPU rendering
backend** as an alternative to the WebRender GPU path. Do not delete.

---

## Phase 1: Merge PlatformWindow V1 into PlatformWindowV2

### Problem

`PlatformWindow` (V1) and `PlatformWindowV2` coexist but serve different purposes:
- V1: window lifecycle (`poll_event`, `present`, `request_redraw`, `is_open`, `close`)
- V2: event processing, callbacks, state management

V1 is **never used generically** — it's only called on concrete types. The only
place that uses the trait import is `run.rs` and `linux/registry.rs`.

### 1a. Move useful V1 methods into PlatformWindowV2

Only 3 V1 methods are actually called in event loops:

| Method | Called from | Action |
|--------|-----------|--------|
| `poll_event()` | run.rs (macOS, Linux) | → Add to PlatformWindowV2 as required |
| `request_redraw()` | run.rs + platform event handlers | → Add to PlatformWindowV2 as required |
| `is_open()` | run.rs (Linux only) | → Add to PlatformWindowV2 as required |

Methods NOT called from run.rs or event processing (delete or make inherent):

| Method | Status |
|--------|--------|
| `new()` | Constructor — should be inherent, not on trait |
| `get_state()` | Never called from run.rs — dead on trait |
| `set_properties()` | Never called externally |
| `get_render_context()` | Never called from run.rs |
| `present()` | Called internally by platforms (make inherent method) |
| `close()` | Only `linux/registry.rs` L127 uses trait-qualified call |
| `sync_clipboard()` | Never called from run.rs (handled internally) |

### 1b. Delete PlatformWindow V1 trait

After moving `poll_event()`, `request_redraw()`, `is_open()` to PlatformWindowV2:
- Delete `pub trait PlatformWindow` from `common/window.rs`
- Remove `WindowProperties` struct (only existed as argument for `set_properties()`)
- Remove all `impl PlatformWindow for XxxWindow` blocks (7 total)
- Make `present()`, `close()` inherent methods on each platform
- Fix `linux/registry.rs` line 127 to use inherent method instead of trait call

### 1c. Clean up dead imports

- `PlatformWindow` import in `wayland/events.rs` line 17 (unused)
- `CompositorMode` import in `macos/mod.rs` line 72 (imported but never used)

### 1d. Delete `GnomeMenuManager` V1

**File:** `linux/gnome_menu/mod.rs` — the old `GnomeMenuManager` struct

- Only `GnomeMenuManagerV2` is referenced from X11Window and WaylandWindow
- V1 manager struct + methods are dead code

**Impact:** ~100-200 lines

**Total Phase 1 impact:** ~-500 lines (V1 trait impls + WindowProperties + dead code)

---

## Phase 2: Extract `CommonWindowState` struct + macro (~28 getters → 2)

### Problem

All 5 PlatformWindowV2 implementations have **28 identical getter methods** that
just return `&self.some_field` or `&mut self.some_field`. These exist because
the trait can't access struct fields directly.

### Solution

Extract shared state into a `CommonWindowState` struct:

```rust
pub struct CommonWindowState {
    // Layout
    pub layout_window: Option<LayoutWindow>,

    // Window state
    pub current_window_state: FullWindowState,
    pub previous_window_state: Option<FullWindowState>,

    // Resources
    pub image_cache: ImageCache,
    pub renderer_resources: RendererResources,
    pub gl_context_ptr: OptionGlContextPtr,
    pub fc_cache: Arc<FcFontCache>,
    pub app_data: Arc<RefCell<RefAny>>,
    pub system_style: Arc<SystemStyle>,

    // WebRender
    pub renderer: Option<WrRenderer>,
    pub render_api: Option<WrRenderApi>,
    pub hit_tester: Option<AsyncHitTester>,
    pub document_id: Option<DocumentId>,
    pub id_namespace: Option<IdNamespace>,
    pub new_frame_ready: Arc<(Mutex<bool>, Condvar)>,

    // UI state
    pub scrollbar_drag_state: Option<ScrollbarDragState>,
    pub last_hovered_node: Option<HitTestNode>,
    pub frame_needs_regeneration: bool,
    pub pending_window_creates: Vec<WindowCreateOptions>,
    pub dynamic_selector_context: DynamicSelectorContext,
    pub tooltip: Option<TooltipWindow>,
}
```

Add only 2 required methods to the trait:

```rust
pub trait PlatformWindowV2 {
    fn common(&self) -> &CommonWindowState;
    fn common_mut(&mut self) -> &mut CommonWindowState;
    // ... only truly platform-specific methods remain required
}
```

Use a macro to implement these 2 methods per platform (assumes each
platform struct has `common: CommonWindowState` field):

```rust
macro_rules! impl_common_window_state {
    () => {
        fn common(&self) -> &CommonWindowState { &self.common }
        fn common_mut(&mut self) -> &mut CommonWindowState { &mut self.common }
    }
}

// In each platform:
impl PlatformWindowV2 for Win32Window {
    impl_common_window_state!();
    // ... only ~10 platform-specific methods
}
```

All 28 getters become default methods on the trait:

```rust
// These 28 methods become zero-boilerplate defaults:
fn get_layout_window(&self) -> Option<&LayoutWindow> {
    self.common().layout_window.as_ref()
}
fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow> {
    self.common_mut().layout_window.as_mut()
}
// ... etc for all 28
```

### Structs before/after

```
BEFORE: Win32Window { hwnd, hinstance, ..., layout_window, current_window_state,
                       previous_window_state, image_cache, renderer_resources,
                       gl_context_ptr, renderer, render_api, hit_tester, ... }

AFTER:  Win32Window { hwnd, hinstance, ..., common: CommonWindowState }
```

### Methods that become defaults (using `common()`)

These all move from "required" → "provided default":

- `get_layout_window` / `get_layout_window_mut`
- `get_current_window_state` / `get_current_window_state_mut`
- `get_previous_window_state` / `set_previous_window_state`
- `get_image_cache_mut`
- `get_renderer_resources_mut`
- `get_fc_cache`
- `get_gl_context_ptr`
- `get_system_style`
- `get_app_data`
- `get_scrollbar_drag_state` / `get_scrollbar_drag_state_mut` / `set_scrollbar_drag_state`
- `get_hit_tester` / `get_hit_tester_mut`
- `get_last_hovered_node` / `set_last_hovered_node`
- `get_document_id` / `get_id_namespace`
- `get_render_api` / `get_render_api_mut`
- `get_renderer` / `get_renderer_mut`
- `needs_frame_regeneration` / `mark_frame_needs_regeneration` / `clear_frame_regeneration_flag`

### Methods that also become defaults (cross-platform logic)

These have identical logic across all platforms:

| Method | Impact |
|--------|--------|
| `add_threads` | -10 lines × 3 |
| `remove_threads` | -8 lines × 3 |
| `queue_window_create` | -3 lines × 3 |
| `show_menu_from_callback` | -20 lines × 3 |
| `prepare_callback_invocation` | -15 lines × 3 |

**Total Phase 2 impact:** ~-520 lines (350 getters + 170 logic methods)

### Borrow-Checker Note

The reason the trait originally has individual getters instead of a single
`&mut self` → `&mut CommonWindowState` is to allow **split borrows**:
borrowing `image_cache` and `layout_window` simultaneously from
`&mut self`. With a single `common_mut()`, the borrow checker sees
one `&mut CommonWindowState` and won't allow simultaneous mutable borrows
of its fields.

**Mitigation options:**
1. **Temporarily reborrow**: `let c = self.common_mut(); let (a, b) = (&mut c.field_a, &mut c.field_b);`
   — Rust allows splitting borrows on a struct if done in the same expression
2. **Keep `prepare_callback_invocation` as the split-borrow point** — it already
   returns a struct of individually borrowed fields, so the common getters can
   still go through `common()`/`common_mut()` for all other cases
3. **Add targeted splitter methods**: `fn common_split_layout_and_resources(&mut self) -> (&mut Option<LayoutWindow>, &mut RendererResources, ...)`

---

## Phase 3: Rename all V2 → clean names

After Phase 1 (V1 trait deleted) and Phase 2 (struct extracted), the "V2"
suffixes are meaningless — there's no V1 anymore. Rename everything:

| Current | Rename to |
|---------|-----------|
| `event_v2.rs` | `event.rs` |
| `layout_v2.rs` | `layout.rs` |
| `PlatformWindowV2` | `PlatformWindow` |
| `process_window_events_recursive_v2` | `process_window_events` |
| `GnomeMenuManagerV2` | `GnomeMenuManager` |

This is a pure mechanical rename (find-and-replace + `git mv`).
The compiler catches any missed references.

**Total Phase 3 impact:** 0 net lines, but cleaner naming throughout

---

## Phase 4: Deduplicate X11/Wayland timer code

X11 and Wayland have **identical** `start_timer`/`stop_timer` implementations
using `timerfd_create`/`timerfd_settime`/`close`. Also identical
`start_thread_poll_timer`/`stop_thread_poll_timer`.

Extract to `linux/common/timer.rs`:

```rust
pub fn start_timerfd(timer_fds: &mut BTreeMap<usize, i32>, timer_id: usize, interval_ms: u64) {
    // timerfd_create + timerfd_settime (currently duplicated in x11 + wayland)
}

pub fn stop_timerfd(timer_fds: &mut BTreeMap<usize, i32>, timer_id: usize) {
    // close(fd)
}
```

**Impact:** ~-80 lines

---

## Phase 5: Simplify `LinuxWindow` enum wrapper

`LinuxWindow` is a simple enum delegating to X11Window/WaylandWindow:

```rust
pub enum LinuxWindow {
    X11(X11Window),
    Wayland(WaylandWindow),
}
```

After Phase 1 (V1 trait deleted), the V1 delegation layer vanishes. Consider
whether `LinuxWindow` should implement the merged `PlatformWindow` trait by
delegating, or whether run.rs should match on the enum variant directly.

**Impact:** -50-100 lines from `linux/mod.rs` (currently 334 lines)

---

## Summary

| Phase | Description | Lines Removed | Difficulty |
|-------|-------------|:------------:|:----------:|
| 1 | Merge V1→V2, delete V1 trait + dead code | ~500 | Medium |
| 2 | CommonWindowState struct + macro | ~520 | Medium |
| 3 | Rename all V2 → clean names | 0 | Easy |
| 4 | Linux timer dedup | ~80 | Easy |
| 5 | Simplify LinuxWindow | ~70 | Easy |
| **Total** | | **~1,170** | |

### Final trait shape (after all phases)

```rust
pub trait PlatformWindow {
    // 2 struct access methods (macro-generated per platform)
    fn common(&self) -> &CommonWindowState;
    fn common_mut(&mut self) -> &mut CommonWindowState;

    // 3 lifecycle methods (from old V1, platform-specific)
    fn poll_event(&mut self) -> Option<Self::EventType>;
    fn request_redraw(&mut self);
    fn is_open(&self) -> bool;

    // 8 truly platform-specific methods
    fn get_raw_window_handle(&self) -> RawWindowHandle;
    fn start_timer(&mut self, timer_id: usize, timer: Timer);
    fn stop_timer(&mut self, timer_id: usize);
    fn start_thread_poll_timer(&mut self);
    fn stop_thread_poll_timer(&mut self);
    fn sync_window_state(&mut self);
    fn show_tooltip_from_callback(&mut self, text: &str, position: LogicalPosition);
    fn hide_tooltip_from_callback(&mut self);

    // 1 optional override (only Wayland overrides)
    fn handle_begin_interactive_move(&mut self) { /* no-op */ }

    // ~28 default getter methods (auto-derived from common())
    fn get_layout_window(&self) -> Option<&LayoutWindow> { ... }
    fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow> { ... }
    // ... all delegating to common()/common_mut()

    // ~5 default cross-platform logic methods
    fn add_threads(&mut self, ...) { ... }
    fn remove_threads(&mut self, ...) { ... }
    fn queue_window_create(&mut self, ...) { ... }
    fn show_menu_from_callback(&mut self, ...) { ... }
    fn prepare_callback_invocation(&mut self) -> InvokeSingleCallbackBorrows { ... }

    // ~3400 lines of provided event processing (unchanged)
    fn apply_user_change(&mut self, ...) -> ProcessEventResult { ... }
    fn apply_system_change(&mut self, ...) -> ProcessEventResult { ... }
    fn dispatch_events_propagated(&mut self, ...) -> (...) { ... }
    fn process_window_events(&mut self, depth: usize) -> ProcessEventResult { ... }
    fn process_timers_and_threads(&mut self) -> bool { ... }
    fn invoke_expired_timers(&mut self) -> (...) { ... }
    fn invoke_thread_callbacks(&mut self) -> (...) { ... }
    fn update_hit_test_at(&mut self, ...) { ... }
}
```

**Result: Each platform goes from ~47 required methods (V1+V2) to ~13 required methods.**

### Execution Order

Phases 1→2→3 must be done in sequence (each depends on prior).
Phases 4 and 5 are independent and can be done anytime after Phase 1.

### Risk Assessment

- **Phase 1:** Low risk — merge + delete, no logic changes. Compiler errors
  catch missed references.
- **Phase 2:** Medium risk — restructuring struct fields across 5 platforms.
  All getters are trivial, but `prepare_callback_invocation` has borrow-checker
  implications. Mitigation: split-borrow patterns (see note above).
- **Phase 3:** Zero risk — mechanical rename, compiler catches everything
- **Phase 4:** Zero risk — extract identical code to shared function
- **Phase 5:** Low risk — simplify delegation layer
