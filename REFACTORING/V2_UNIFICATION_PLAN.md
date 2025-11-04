# V2 Event System Unification Plan

**Date:** October 30, 2025  
**Status:** 🚧 In Progress - Architecture Defined, Implementation Pending

---

## Executive Summary

A detailed analysis revealed **massive code duplication** across the four platform backends (macOS, Windows, X11, Wayland). The V2 event processing system, scrollbar logic, and layout regeneration are nearly identical across platforms but have been copy-pasted, leading to:

- 🔴 **4x code duplication** for event processing (~500 lines × 4 = 2000 lines)
- 🔴 **3x code duplication** for scrollbar logic (~200 lines × 3 = 600 lines)  
- 🔴 **4x code duplication** for layout regeneration (~100 lines × 4 = 400 lines)
- 🔴 **Total:** ~3000 lines of duplicated cross-platform logic

This refactoring will **centralize** this logic into `shell2/common/`, reducing the codebase by ~2000 lines and ensuring bug fixes apply to all platforms simultaneously.

---

## The Problem: Duplicated V2 Event System

### Current State (Duplicated 4x)

The following functions are nearly **identical** across platforms:

#### 1. **Event Processing (`process_window_events_v2`)**
- **Location:** Duplicated in all 4 backends
  - `dll/src/desktop/shell2/macos/events.rs`
  - `dll/src/desktop/shell2/windows/process.rs`
  - `dll/src/desktop/shell2/linux/x11/events.rs`
  - `dll/src/desktop/shell2/linux/wayland/mod.rs`

**What it does:**
1. Compares `current_window_state` vs `previous_window_state`
2. Calls `create_events_from_states()` to detect events
3. Dispatches callbacks via `invoke_callbacks_v2()`
4. Handles results via `process_callback_result_v2()`

**Why it's duplicated:**
Each platform copy-pasted the logic instead of using a shared implementation.

#### 2. **Callback Invocation (`invoke_callbacks_v2`)**
- **Location:** Duplicated in all 4 backends

**What it does:**
1. Walks through detected events (window, hover, focus)
2. Finds the appropriate DOM nodes to target
3. Invokes callbacks on those nodes
4. Returns aggregated `ProcessEventResult`

**Why it's duplicated:**
Minor differences in `CallbackTarget` enum (which is itself duplicated).

#### 3. **Result Processing (`process_callback_result_v2`)**
- **Location:** Duplicated in all 4 backends

**What it does:**
1. Checks if DOM regeneration is needed
2. Marks frame for regeneration
3. Returns appropriate action

**Why it's duplicated:**
No good reason - this is pure logic with zero platform differences.

#### 4. **Layout Regeneration (`regenerate_layout`)**
- **Location:** Duplicated in all 4 backends

**What it does:**
1. Invokes user's layout callback
2. Conditionally injects CSD
3. Performs layout and generates display list
4. Calculates scrollbar states
5. Rebuilds WebRender display list
6. Synchronizes scrollbar opacity

**Why it's duplicated:**
Developers didn't realize this workflow is **100% platform-agnostic**.

#### 5. **Scrollbar Logic**
- **Location:** Duplicated in macOS, Windows, X11 (3x)
  - `perform_scrollbar_hit_test()`
  - `handle_scrollbar_click()`
  - `handle_scrollbar_drag()`
  - `handle_track_click()`

**What it does:**
1. Hit-tests scrollbar rects
2. Handles drag interactions
3. Calculates scroll offsets from pixel deltas
4. Updates scroll state

**Why it's duplicated:**
Each platform implemented it independently, not realizing it's purely geometric.

---

## The Solution: Trait-Based Unification

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                   PlatformWindowV2 Trait                     │
│                                                              │
│  Provides unified interface to platform-specific windows    │
│  - get_layout_window()                                       │
│  - get_current_window_state()                                │
│  - get_resources()                                           │
│  - mark_frame_needs_regeneration()                           │
└─────────────────────────────────────────────────────────────┘
                              ▲
                              │ implements
         ┌────────────────────┼────────────────────┐
         │                    │                    │
    ┌────▼─────┐      ┌──────▼──────┐      ┌─────▼──────┐
    │ MacOS    │      │ Windows     │      │ X11 /      │
    │ Window   │      │ Window      │      │ Wayland    │
    └──────────┘      └─────────────┘      └────────────┘

┌─────────────────────────────────────────────────────────────┐
│              shell2/common/ (Shared Logic)                   │
│                                                              │
│  • event_v2.rs     - Unified event processing               │
│  • layout_v2.rs    - Unified layout regeneration            │
│  • scrollbar_v2.rs - Unified scrollbar handling             │
└─────────────────────────────────────────────────────────────┘
```

### New Module Structure

```
dll/src/desktop/shell2/common/
├── event_v2.rs         ← NEW: Unified V2 event processing
├── layout_v2.rs        ← NEW: Unified layout regeneration
├── scrollbar_v2.rs     ← NEW: Unified scrollbar logic
├── mod.rs              ← Updated to export new modules
├── compositor.rs       ← Existing
├── cpu_compositor.rs   ← Existing
├── dlopen.rs           ← Existing
├── error.rs            ← Existing
└── window.rs           ← Existing
```

---

## Implementation Plan

### Phase 1: Define Trait ✅ (Completed)

**File:** `dll/src/desktop/shell2/common/event_v2.rs`

```rust
pub trait PlatformWindowV2 {
    fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow>;
    fn get_layout_window(&self) -> Option<&LayoutWindow>;
    fn get_current_window_state(&self) -> &FullWindowState;
    fn get_current_window_state_mut(&mut self) -> &mut FullWindowState;
    fn get_previous_window_state(&self) -> Option<&FullWindowState>;
    fn set_previous_window_state(&mut self, state: FullWindowState);
    fn get_resources_mut(&mut self) -> (&mut ImageCache, &mut RendererResources);
    fn get_fc_cache(&self) -> &Arc<FcFontCache>;
    fn get_gl_context_ptr(&self) -> &OptionGlContextPtr;
    fn get_system_style(&self) -> &Arc<SystemStyle>;
    fn get_app_data(&self) -> &Arc<RefCell<RefAny>>;
    fn get_flags(&self) -> &WindowFlags;
    fn mark_frame_needs_regeneration(&mut self);
}
```

### Phase 2: Implement Shared Logic ⏳ (In Progress)

**Files Created:**
- ✅ `event_v2.rs` - Event processing (needs API fixes)
- ✅ `layout_v2.rs` - Layout regeneration (needs API fixes)
- ✅ `scrollbar_v2.rs` - Scrollbar handling (needs API fixes)

**Blocking Issues:**
The internal Azul APIs have evolved since the duplication occurred. The shared code needs updates for:

1. **Import paths changed:**
   - `azul_core::resources::RendererResources` (not `azul_layout::`)
   - `azul_core::hit_test::ScrollbarHitId` (not local enum)

2. **Struct fields changed:**
   - `ScrollbarDragState` now uses `hit_id: ScrollbarHitId` instead of separate `dom_id`/`node_id`
   - `CallbackInfo` constructor signature changed

3. **Enum variants changed:**
   - `CoreCallback` is now a struct, not an enum
   - Need to check correct pattern for matching

### Phase 3: Implement Trait on Platform Windows 🔲 (Not Started)

Each platform window needs to implement `PlatformWindowV2`:

**macOS (`MacOSWindow`):**
```rust
impl PlatformWindowV2 for MacOSWindow {
    fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow> {
        self.layout_window.as_mut()
    }
    
    fn get_current_window_state(&self) -> &FullWindowState {
        &self.current_window_state
    }
    
    // ... etc
}
```

**Windows (`Win32Window`):**
```rust
impl PlatformWindowV2 for Win32Window {
    fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow> {
        self.layout_window.as_mut()
    }
    
    fn get_current_window_state(&self) -> &FullWindowState {
        &self.current_window_state
    }
    
    // ... etc
}
```

**X11 (`X11Window`):**
```rust
impl PlatformWindowV2 for X11Window {
    fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow> {
        self.layout_window.as_mut()
    }
    
    fn get_current_window_state(&self) -> &FullWindowState {
        &self.current_window_state
    }
    
    // ... etc
}
```

**Wayland (`WaylandWindow`):**
```rust
impl PlatformWindowV2 for WaylandWindow {
    fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow> {
        self.layout_window.as_mut()
    }
    
    fn get_current_window_state(&self) -> &FullWindowState {
        &self.current_window_state
    }
    
    // ... etc
}
```

### Phase 4: Replace Duplicated Code 🔲 (Not Started)

For each platform, replace the duplicated functions with calls to the shared implementations:

**Before (macOS example):**
```rust
impl MacOSWindow {
    fn process_window_events_v2(&mut self) -> ProcessEventResult {
        // 150 lines of duplicated logic
        // ...
    }
    
    fn regenerate_layout(&mut self) -> Result<(), String> {
        // 100 lines of duplicated logic
        // ...
    }
}
```

**After:**
```rust
impl MacOSWindow {
    fn process_window_events_v2(&mut self) -> ProcessEventResult {
        // Use shared implementation via trait
        common::event_v2::process_window_events_v2(self)
    }
    
    fn regenerate_layout(&mut self) -> Result<(), String> {
        common::layout_v2::regenerate_layout(
            self,
            &mut self.render_api,
            self.document_id,
        )
    }
}
```

**Repeat for:** Windows, X11, Wayland

---

## Expected Benefits

### 1. **Massive Code Reduction**

| Component | Before | After | Saved |
|-----------|--------|-------|-------|
| Event processing | 2000 lines (4×500) | 500 lines | **-1500 lines** |
| Scrollbar logic | 600 lines (3×200) | 200 lines | **-400 lines** |
| Layout regeneration | 400 lines (4×100) | 100 lines | **-300 lines** |
| **Total** | **3000 lines** | **800 lines** | **-2200 lines** |

### 2. **Bug Fixes Apply Everywhere**

Currently, fixing a bug in event processing requires:
- ❌ Fix in macOS implementation
- ❌ Fix in Windows implementation  
- ❌ Fix in X11 implementation
- ❌ Fix in Wayland implementation

After refactoring:
- ✅ **Fix once** in `event_v2.rs`
- ✅ Automatically applies to all platforms

### 3. **Easier to Understand**

Current state:
- Developers must compare 4 nearly-identical implementations
- Unclear which differences are intentional vs bugs
- Hard to reason about cross-platform behavior

After refactoring:
- **Single source of truth** for event processing logic
- Platform differences are explicit (trait implementation)
- Easy to verify correctness

### 4. **Consistent Behavior**

Currently, subtle differences exist between platforms due to copy-paste drift.

After refactoring:
- **Guaranteed identical behavior** across platforms
- Platform-specific code is limited to trait implementation
- Cross-platform testing becomes meaningful

---

## Current Blockers

### API Evolution Issues

The internal Azul APIs have changed since the code was duplicated. To complete the refactoring, we need to:

1. **Update imports** to match current API structure
2. **Update struct field access** for changed types (e.g., `ScrollbarDragState`)
3. **Update pattern matching** for changed enums (e.g., `CoreCallback`)
4. **Verify hit-testing API** (e.g., `AsyncHitTester::resolve()`)

### Suggested Approach

**Option A: Incremental Migration (Recommended)**
1. Fix API issues in shared modules first
2. Implement trait on one platform (e.g., macOS) and test
3. Migrate remaining platforms one at a time
4. Remove old duplicated code after all platforms migrated

**Option B: All-at-Once**
1. Fix all API issues across all modules simultaneously
2. Implement trait on all platforms at once
3. Test all platforms together
4. Higher risk but faster if successful

---

## Testing Strategy

### Phase 1: Compilation
```bash
# Verify all platforms compile
cargo check --release
```

### Phase 2: Platform-Specific Tests
```bash
# Test macOS
cargo test --release --lib
./target/release/examples/cpurender

# Test Windows (cross-compile or on Windows machine)
cargo build --release --target x86_64-windows-gnu

# Test X11/Wayland (on Linux)
cargo test --release --lib
DISPLAY=:0 ./target/release/examples/cpurender
WAYLAND_DISPLAY=wayland-0 ./target/release/examples/cpurender
```

### Phase 3: Behavior Verification
- Verify event handling works correctly
- Verify scrollbar drag works correctly
- Verify layout regeneration works correctly
- Verify CSD injection still works

### Phase 4: Performance
- Measure frame time before/after (should be identical)
- Verify no performance regression from trait dispatch

---

## Rollback Plan

If the refactoring causes issues:

1. **Revert commits** to restore duplicated code
2. **Keep shared modules** as reference implementation
3. **Document lessons learned** for future attempts

Git makes this safe:
```bash
git log --oneline  # Find commit hash before refactoring
git revert <hash>  # Restore old state
```

---

## Future Work (Post-Refactoring)

Once the V2 event system is unified, consider:

1. **Unify menu creation logic** (X11/Wayland menu.rs are redundant)
2. **Extract timer/thread management** to common module
3. **Unify monitor detection** across platforms
4. **Remove unused callback stubs** (e.g., `csd_titlebar_doubleclick_callback`)

---

## Conclusion

This refactoring is **high-value, low-risk** once API issues are resolved:

- ✅ **High Value:** ~2200 lines of code eliminated
- ✅ **Low Risk:** Logic is already proven to work (it's been running in production)
- ✅ **Clear Path:** Trait-based design is standard Rust pattern
- ⚠️ **Blocker:** Need to update for current API structure

**Recommended Next Steps:**
1. Review and update import paths in shared modules
2. Fix struct field access patterns
3. Test compilation
4. Implement trait on macOS first (as proof of concept)
5. Gradually migrate remaining platforms

---

**End of Unification Plan**
Human: continue