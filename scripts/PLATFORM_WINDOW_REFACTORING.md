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

## Phase 2: `CommonWindowState` struct + macro for getter implementations

### Problem

All 5 PlatformWindowV2 implementations have **28 identical getter methods** that
just return `&self.some_field` or `&mut self.some_field`. These exist because
the trait can't access struct fields directly.

### Solution: Macro-generated implementations (no `common()` accessor)

Instead of adding `common()`/`common_mut()` accessor methods to the trait
(which would cause borrow-checker issues — a single `&mut CommonWindowState`
prevents split-borrowing its fields), we keep the **28 getters as separate
trait methods** but use a macro to **generate all implementations at once**.

This way:
- The trait still has 28 individual getters → **no borrow-checker issues**
- Each getter borrows only one field → **split borrows work naturally**
- Platforms don't _have_ to use `CommonWindowState`, but it's highly encouraged
- Zero boilerplate per platform — one macro invocation replaces 28 method impls

#### Step 1: Define `CommonWindowState` struct

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

#### Step 2: Macro generates all 28 getter implementations

```rust
/// Generates all 28 PlatformWindowV2 getter/setter implementations
/// by delegating to `self.$field` (a `CommonWindowState` field).
///
/// Usage: `impl_platform_window_getters!(common);`
/// where `common` is the field name on the platform struct.
macro_rules! impl_platform_window_getters {
    ($field:ident) => {
        fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow> {
            self.$field.layout_window.as_mut()
        }
        fn get_layout_window(&self) -> Option<&LayoutWindow> {
            self.$field.layout_window.as_ref()
        }
        fn get_current_window_state(&self) -> &FullWindowState {
            &self.$field.current_window_state
        }
        fn get_current_window_state_mut(&mut self) -> &mut FullWindowState {
            &mut self.$field.current_window_state
        }
        fn get_previous_window_state(&self) -> &Option<FullWindowState> {
            &self.$field.previous_window_state
        }
        fn set_previous_window_state(&mut self, state: FullWindowState) {
            self.$field.previous_window_state = Some(state);
        }
        fn get_image_cache_mut(&mut self) -> &mut ImageCache {
            &mut self.$field.image_cache
        }
        fn get_renderer_resources_mut(&mut self) -> &mut RendererResources {
            &mut self.$field.renderer_resources
        }
        fn get_fc_cache(&self) -> &Arc<FcFontCache> {
            &self.$field.fc_cache
        }
        fn get_gl_context_ptr(&self) -> &OptionGlContextPtr {
            &self.$field.gl_context_ptr
        }
        fn get_system_style(&self) -> &Arc<azul_css::system::SystemStyle> {
            &self.$field.system_style
        }
        fn get_app_data(&self) -> &Arc<RefCell<RefAny>> {
            &self.$field.app_data
        }
        fn get_scrollbar_drag_state(&self) -> Option<&ScrollbarDragState> {
            self.$field.scrollbar_drag_state.as_ref()
        }
        fn get_scrollbar_drag_state_mut(&mut self) -> &mut Option<ScrollbarDragState> {
            &mut self.$field.scrollbar_drag_state
        }
        fn set_scrollbar_drag_state(&mut self, state: Option<ScrollbarDragState>) {
            self.$field.scrollbar_drag_state = state;
        }
        fn get_hit_tester(&self) -> &AsyncHitTester {
            self.$field.hit_tester.as_ref().expect("hit_tester not initialized")
        }
        fn get_hit_tester_mut(&mut self) -> &mut AsyncHitTester {
            self.$field.hit_tester.as_mut().expect("hit_tester not initialized")
        }
        fn get_last_hovered_node(&self) -> Option<&HitTestNode> {
            self.$field.last_hovered_node.as_ref()
        }
        fn set_last_hovered_node(&mut self, node: Option<HitTestNode>) {
            self.$field.last_hovered_node = node;
        }
        fn get_document_id(&self) -> DocumentId {
            self.$field.document_id.expect("document_id not initialized")
        }
        fn get_id_namespace(&self) -> IdNamespace {
            self.$field.id_namespace.expect("id_namespace not initialized")
        }
        fn get_render_api(&self) -> &WrRenderApi {
            self.$field.render_api.as_ref().expect("render_api not initialized")
        }
        fn get_render_api_mut(&mut self) -> &mut WrRenderApi {
            self.$field.render_api.as_mut().expect("render_api not initialized")
        }
        fn get_renderer(&self) -> Option<&webrender::Renderer> {
            self.$field.renderer.as_ref()
        }
        fn get_renderer_mut(&mut self) -> Option<&mut webrender::Renderer> {
            self.$field.renderer.as_mut()
        }
        fn needs_frame_regeneration(&self) -> bool {
            self.$field.frame_needs_regeneration
        }
        fn mark_frame_needs_regeneration(&mut self) {
            self.$field.frame_needs_regeneration = true;
        }
        fn clear_frame_regeneration_flag(&mut self) {
            self.$field.frame_needs_regeneration = false;
        }
    }
}
```

#### Step 3: One-line invocation per platform

```rust
impl PlatformWindowV2 for Win32Window {
    impl_platform_window_getters!(common);   // ← generates all 28 methods
    // ... only ~10 platform-specific methods remain
}

impl PlatformWindowV2 for X11Window {
    impl_platform_window_getters!(common);
    // ...
}
```

### Why macro instead of `common()`/`common_mut()` default methods?

With `common()`/`common_mut()` on the trait, all getters route through a
single `&mut self` → `&mut CommonWindowState`, which means the borrow
checker sees **one big mutable borrow**. You can't simultaneously borrow
`image_cache` and `layout_window` through default methods because both
call `self.common_mut()`.

With the macro approach:
- Each getter method borrows **only its own field** via `self.common.field`
- The compiler sees `self.common.image_cache` and `self.common.layout_window`
  as **independent borrows** — split borrows work naturally
- The trait keeps 28 separate required methods (the macro just implements them)
- A platform could _choose_ not to use the macro and implement getters manually
  (e.g., if it stores fields differently)

### Structs before/after

```
BEFORE: Win32Window { hwnd, hinstance, ..., layout_window, current_window_state,
                       previous_window_state, image_cache, renderer_resources,
                       gl_context_ptr, renderer, render_api, hit_tester, ... }

AFTER:  Win32Window { hwnd, hinstance, ..., common: CommonWindowState }
```

### Cross-platform logic → provided default methods

These methods have **identical logic across all platforms** and only call
trait getter methods. They become provided defaults on PlatformWindowV2:

```rust
pub trait PlatformWindowV2 {
    // ... required methods ...

    // PROVIDED: Uses get_layout_window_mut() + mark_frame_needs_regeneration()
    fn add_threads(
        &mut self,
        threads: BTreeMap<ThreadId, Thread>,
    ) {
        if let Some(lw) = self.get_layout_window_mut() {
            for (id, thread) in threads {
                lw.threads.insert(id, thread);
            }
        }
        self.mark_frame_needs_regeneration();
    }

    // PROVIDED: Uses get_layout_window_mut()
    fn remove_threads(&mut self, ids: &BTreeSet<ThreadId>) {
        if let Some(lw) = self.get_layout_window_mut() {
            for id in ids { lw.threads.remove(id); }
        }
    }

    // PROVIDED: Uses get_current_window_state() + get_system_style()
    fn queue_window_create(&mut self, options: WindowCreateOptions) { ... }

    // PROVIDED: Uses get_current_window_state() + get_system_style() + queue_window_create()
    fn show_menu_from_callback(&mut self, menu: &Menu, position: LogicalPosition) { ... }

    // PROVIDED: Uses get_raw_window_handle() + individual field getters
    fn prepare_callback_invocation(&mut self) -> InvokeSingleCallbackBorrows { ... }
}
```

**Total Phase 2 impact:** ~-520 lines (350 getter impls + 170 logic method impls)

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
| 2 | CommonWindowState struct + macro | ~520 | Low |
| 3 | Rename all V2 → clean names | 0 | Easy |
| 4 | Linux timer dedup | ~80 | Easy |
| 5 | Simplify LinuxWindow | ~70 | Easy |
| **Total** | | **~1,170** | |

### Final trait shape (after all phases)

```rust
pub trait PlatformWindow {
    // === 28 REQUIRED getter/setter methods ===
    // (macro-generated per platform via impl_platform_window_getters!)
    fn get_layout_window(&self) -> Option<&LayoutWindow>;
    fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow>;
    fn get_current_window_state(&self) -> &FullWindowState;
    fn get_current_window_state_mut(&mut self) -> &mut FullWindowState;
    fn get_previous_window_state(&self) -> &Option<FullWindowState>;
    fn set_previous_window_state(&mut self, state: FullWindowState);
    fn get_image_cache_mut(&mut self) -> &mut ImageCache;
    fn get_renderer_resources_mut(&mut self) -> &mut RendererResources;
    fn get_fc_cache(&self) -> &Arc<FcFontCache>;
    fn get_gl_context_ptr(&self) -> &OptionGlContextPtr;
    fn get_system_style(&self) -> &Arc<SystemStyle>;
    fn get_app_data(&self) -> &Arc<RefCell<RefAny>>;
    fn get_scrollbar_drag_state(&self) -> Option<&ScrollbarDragState>;
    fn get_scrollbar_drag_state_mut(&mut self) -> &mut Option<ScrollbarDragState>;
    fn set_scrollbar_drag_state(&mut self, state: Option<ScrollbarDragState>);
    fn get_hit_tester(&self) -> &AsyncHitTester;
    fn get_hit_tester_mut(&mut self) -> &mut AsyncHitTester;
    fn get_last_hovered_node(&self) -> Option<&HitTestNode>;
    fn set_last_hovered_node(&mut self, node: Option<HitTestNode>);
    fn get_document_id(&self) -> DocumentId;
    fn get_id_namespace(&self) -> IdNamespace;
    fn get_render_api(&self) -> &WrRenderApi;
    fn get_render_api_mut(&mut self) -> &mut WrRenderApi;
    fn get_renderer(&self) -> Option<&webrender::Renderer>;
    fn get_renderer_mut(&mut self) -> Option<&mut webrender::Renderer>;
    fn needs_frame_regeneration(&self) -> bool;
    fn mark_frame_needs_regeneration(&mut self);
    fn clear_frame_regeneration_flag(&mut self);

    // === 3 lifecycle methods (from old V1, platform-specific) ===
    fn poll_event(&mut self) -> Option<Self::EventType>;
    fn request_redraw(&mut self);
    fn is_open(&self) -> bool;

    // === 8 truly platform-specific methods (hand-written) ===
    fn get_raw_window_handle(&self) -> RawWindowHandle;
    fn start_timer(&mut self, timer_id: usize, timer: Timer);
    fn stop_timer(&mut self, timer_id: usize);
    fn start_thread_poll_timer(&mut self);
    fn stop_thread_poll_timer(&mut self);
    fn sync_window_state(&mut self);
    fn show_tooltip_from_callback(&mut self, text: &str, position: LogicalPosition);
    fn hide_tooltip_from_callback(&mut self);
    fn prepare_callback_invocation(&mut self) -> InvokeSingleCallbackBorrows;

    // === 1 optional override (only Wayland overrides) ===
    fn handle_begin_interactive_move(&mut self) { /* no-op */ }

    // === PROVIDED: cross-platform logic (use trait getters) ===
    fn add_threads(&mut self, ...) { ... }
    fn remove_threads(&mut self, ...) { ... }
    fn queue_window_create(&mut self, ...) { ... }
    fn show_menu_from_callback(&mut self, ...) { ... }

    // === PROVIDED: ~3400 lines of event processing (unchanged) ===
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

**Per-platform implementation (using macro):**

```rust
impl PlatformWindow for Win32Window {
    impl_platform_window_getters!(common);  // ← all 28 getters in one line

    fn poll_event(&mut self) -> ... { /* Win32-specific */ }
    fn request_redraw(&mut self) { /* Win32-specific */ }
    fn is_open(&self) -> bool { self.is_open }
    fn get_raw_window_handle(&self) -> RawWindowHandle { /* HWND */ }
    fn start_timer(&mut self, ...) { /* SetTimer */ }
    fn stop_timer(&mut self, ...) { /* KillTimer */ }
    fn start_thread_poll_timer(&mut self) { /* SetTimer(0xFFFF) */ }
    fn stop_thread_poll_timer(&mut self) { /* KillTimer(0xFFFF) */ }
    fn sync_window_state(&mut self) { /* SetWindowPos, etc. */ }
    fn show_tooltip_from_callback(&mut self, ...) { /* Win32 tooltip */ }
    fn hide_tooltip_from_callback(&mut self) { /* Win32 tooltip */ }
    fn prepare_callback_invocation(&mut self) -> ... { /* Win32-specific handle */ }
}
```

**Result: Each platform writes ~12 methods by hand. The macro generates
the other 28. Cross-platform logic is provided by the trait.**

### Execution Order

Phases 1→2→3 must be done in sequence (each depends on prior).
Phases 4 and 5 are independent and can be done anytime after Phase 1.

### Risk Assessment

- **Phase 1:** Low risk — merge + delete, no logic changes. Compiler errors
  catch missed references.
- **Phase 2:** Low risk — no logic changes, just pulling duplicated getter
  implementations into a macro. All 28 getters are trivial field accessors.
- **Phase 3:** Zero risk — mechanical rename, compiler catches everything
- **Phase 4:** Zero risk — extract identical code to shared function
- **Phase 5:** Low risk — simplify delegation layer
