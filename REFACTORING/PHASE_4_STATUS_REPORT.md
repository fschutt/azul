# Phase 4 Implementation Status Report

**Generated:** October 18, 2025  
**Status:** PARTIALLY COMPLETE - Compilation Errors Present  
**Test Status:** Cannot run tests due to compilation failures

---

## Executive Summary

The Phase 4 refactoring (splitting ScrollManager, IFrameManager, and GpuStateManager) has been **partially implemented** but contains **18 compilation errors** preventing compilation and testing. The three managers exist as separate files, but there are integration issues between `core`, `layout`, and the managers themselves.

---

## Implementation Status by Component

### ✅ COMPLETE: Manager Files Exist

All three manager files have been created in the `layout/src/` directory:

1. **`layout/src/scroll.rs`** (318 lines)
   - ✅ ScrollManager struct defined
   - ✅ ScrollState with animation support
   - ✅ EventSource tracking implemented
   - ✅ Smooth scroll interpolation logic
   - ✅ Frame lifecycle methods (begin_frame, end_frame)

2. **`layout/src/iframe.rs`** (250 lines)
   - ✅ IFrameManager struct defined
   - ✅ IFrameState tracking
   - ✅ PipelineId generation and tracking
   - ✅ Nested DOM ID management
   - ✅ Edge detection flags

3. **`layout/src/gpu.rs`** (157 lines)
   - ✅ GpuStateManager struct defined
   - ✅ Opacity state tracking
   - ✅ Transform key management structure
   - ✅ Fade delay/duration configuration

### ✅ COMPLETE: Core Event System

**`core/src/events.rs`** has EventSource enum:
```rust
pub enum EventSource {
    User,           // ✅ Implemented
    Programmatic,   // ✅ Implemented
    Synthetic,      // ✅ Implemented
    Lifecycle,      // ✅ Implemented
}
```

EventPhase enum also exists (Capture, Target, Bubble).

### ⚠️ PARTIAL: Tests Exist But Cannot Run

**`layout/src/window.rs`** (lines 1710-2321) contains test module with:
- ✅ 15+ tests written for ScrollManager
- ✅ Tests for IFrame edge detection
- ✅ Tests for scrollbar opacity fading
- ❌ **Cannot compile due to errors**
- ❌ **Cannot run to verify correctness**

### ❌ INCOMPLETE: Integration Issues

The managers are not properly integrated with the rest of the codebase.

---

## Compilation Errors (18 Total)

### Category 1: Import Path Errors (3 errors)

**Problem:** `layout/src/window.rs` tries to import managers from `azul_core::*` instead of local crate.

```rust
// ❌ WRONG (current code):
use azul_core::{
    gpu::{GpuStateManager, GpuValueCache},  // Error: no GpuStateManager in azul_core::gpu
    iframe::IFrameManager,                   // Error: no iframe module in azul_core
    scroll::ScrollManager,                   // Error: no scroll module in azul_core
};
```

**Fix Required:**
```rust
// ✅ CORRECT:
use crate::{
    gpu::GpuStateManager,
    iframe::IFrameManager,
    scroll::ScrollManager,
};
use azul_core::gpu::GpuValueCache;  // This stays in core
```

**Files Affected:**
- `layout/src/window.rs:20-27`

---

### Category 2: Missing Type Definitions (1 error)

**Problem:** `LayoutSize` used in `gpu.rs` but not imported.

```rust
// ❌ ERROR at layout/src/gpu.rs:96
.map(|l| LayoutSize {
    width: l.content_size.width,   // Also content_size doesn't exist
    height: l.content_size.height,
})
```

**Fix Required:**
```rust
use azul_css::props::basic::LayoutSize;
// OR use the correct field from UnifiedLayout
```

**Files Affected:**
- `layout/src/gpu.rs:96-99`

---

### Category 3: Duration API Mismatch (2 errors)

**Problem:** Calling `Duration::from_millis()` which doesn't exist.

```rust
// ❌ ERROR at layout/src/gpu.rs:34
Self::new(Duration::from_millis(500), Duration::from_millis(200))
```

**Fix Required:**
Check the actual Duration API in `core/src/task.rs` and use correct constructor:
```rust
// Likely should be:
Duration::System(SystemTimeDiff::from_millis(500))
```

**Files Affected:**
- `layout/src/gpu.rs:34`

---

### Category 4: Field Access Errors (2 errors)

**Problem:** Accessing `content_size` field on `UnifiedLayout` which doesn't exist.

```rust
// ❌ ERROR at layout/src/gpu.rs:97-98
width: l.content_size.width,   // No such field
height: l.content_size.height, // No such field
```

**Fix Required:**
Use correct fields from `UnifiedLayout`:
```rust
// Available fields are: items, bounds, overflow
width: l.bounds.size.width,
height: l.bounds.size.height,
```

**Files Affected:**
- `layout/src/gpu.rs:97-98`

---

### Category 5: EventSource Variant Error (1 error)

**Problem:** Using `EventSource::System` which doesn't exist.

```rust
// ❌ ERROR at layout/src/scroll.rs:135
if event.source == EventSource::Programmatic || event.source == EventSource::System {
```

**Fix Required:**
```rust
// ✅ CORRECT (System is not a variant):
if event.source == EventSource::Programmatic {
```

**Files Affected:**
- `layout/src/scroll.rs:135`

---

### Category 6: Missing Clone Trait (1 error)

**Problem:** `DisplayList` doesn't implement `Clone` but `LayoutWindow` is `#[derive(Clone)]`.

```rust
// ❌ ERROR at layout/src/window.rs:128
#[derive(Debug, Clone)]  // Clone fails because DisplayList doesn't impl Clone
pub struct LayoutWindow {
    pub display_list: DisplayList,  // DisplayList is not Clone
}
```

**Fix Required:**
Either:
1. Remove `Clone` from `LayoutWindow` derive
2. Add `Clone` to `DisplayList` in `solver3/display_list.rs:83`
3. Wrap `display_list` in `Arc<DisplayList>`

**Files Affected:**
- `layout/src/window.rs:117, 128`
- `layout/src/solver3/display_list.rs:83`

---

### Category 7: Missing Default Trait (1 error)

**Problem:** `ParsedFont` doesn't implement `Default` but is required by `LayoutWindow::default()`.

**Fix Required:**
Either implement `Default` for `ParsedFont` or remove it from the Default impl requirements.

---

### Category 8: Field Access Errors - gpu_value_cache (4 errors)

**Problem:** Code tries to access `self.gpu_value_cache` on `LayoutWindow` but the field is actually inside `gpu_state_manager`.

```rust
// ❌ ERROR at layout/src/window.rs:849, 854, 859, 929
self.gpu_value_cache.get(dom_id)  // No such field
```

**Fix Required:**
```rust
// ✅ CORRECT:
self.gpu_state_manager.get_or_create_cache(dom_id)
```

**Files Affected:**
- `layout/src/window.rs:849, 854, 859, 929`

---

### Category 9: Miscellaneous Type Errors (3 errors)

Various type mismatches that need investigation.

---

## Test Status

### Tests Written (Cannot Run)

**Location:** `layout/src/window.rs` (lines 1710-2321)

**Test Count:** 15+ tests

**Test Categories:**
1. ✅ **Timer Management** (4 tests)
   - `test_timer_add_remove`
   - `test_timer_get_mut`
   - `test_multiple_timers`

2. ✅ **GPU Cache Management** (2 tests)
   - `test_gpu_cache_management`
   - `test_gpu_cache_multiple_doms`

3. ✅ **ScrollManager Integration** (5 tests)
   - `test_scroll_manager_initialization`
   - `test_scroll_manager_tick_updates_activity`
   - `test_scroll_manager_programmatic_scroll`
   - `test_scroll_manager_iframe_edge_detection`
   - `test_scroll_manager_iframe_invocation_tracking`

4. ✅ **Scrollbar Opacity** (1 test)
   - `test_scrollbar_opacity_fading`

5. ✅ **IFrame Callbacks** (1 test)
   - `test_iframe_callback_reason_initial_render`

**Status:** ❌ **Cannot compile, cannot run**

---

## Missing Tests

According to the planning document (`todo/addscrolling.md`), the following tests are **NOT YET IMPLEMENTED**:

### Phase 4a Tests (IFrameManager) - MISSING

- ❌ IFrame re-invocation on edge scroll
- ❌ IFrame re-invocation on content expansion
- ❌ Prevention of duplicate InitialRender (partially covered)
- ❌ PipelineId allocation and persistence
- ❌ Nested DOM ID mapping

### Phase 4b Tests (GpuStateManager) - MISSING

- ❌ Opacity calculation from last activity time
- ❌ Fade-in transition (0.0 → 1.0 over 200ms)
- ❌ Fade-out transition (1.0 → 0.0 over 200ms)
- ❌ Fade delay (500ms of full visibility)
- ❌ Multiple simultaneous fades
- ❌ GPU property ID lifecycle

### Phase 4c Tests (Event Source) - MISSING

- ❌ User events trigger smooth scrolling
- ❌ Programmatic events instant update
- ❌ Synthetic events instant update
- ❌ Event source preserved through pipeline
- ❌ Different animation behaviors per source

### Unit Tests for Individual Managers - MISSING

**Should create:**
- `layout/src/scroll.rs` → `#[cfg(test)] mod tests`
- `layout/src/iframe.rs` → `#[cfg(test)] mod tests`
- `layout/src/gpu.rs` → `#[cfg(test)] mod tests`

---

## DLL Module Status

**Status:** ❌ **NOT INTEGRATED**

The `dll` module does not reference the new managers at all:
- No imports of ScrollManager
- No imports of IFrameManager
- No imports of GpuStateManager

**Impact:** Unknown if DLL needs to expose these managers or if integration happens at a different level.

---

## Action Plan to Fix Compilation Errors

### Priority 1: Fix Import Paths (Quick Win)

**Estimated Time:** 15 minutes

1. Fix `layout/src/window.rs:20-27`:
   ```rust
   use crate::{
       gpu::GpuStateManager,
       iframe::IFrameManager,
       scroll::ScrollManager,
   };
   use azul_core::gpu::GpuValueCache;
   ```

2. Verify that `layout/src/lib.rs` exports these modules:
   ```rust
   pub mod scroll;
   pub mod iframe;
   pub mod gpu;
   ```

---

### Priority 2: Fix Duration API Usage (Quick Fix)

**Estimated Time:** 10 minutes

1. Check `core/src/task.rs` for correct Duration constructor
2. Replace `Duration::from_millis()` with correct API in:
   - `layout/src/gpu.rs:34`
   - Possibly other locations

---

### Priority 3: Fix EventSource Variant (Quick Fix)

**Estimated Time:** 5 minutes

1. Remove `EventSource::System` from:
   - `layout/src/scroll.rs:135`

---

### Priority 4: Fix Field Access Errors (Medium)

**Estimated Time:** 30 minutes

1. Fix `gpu_value_cache` access in `layout/src/window.rs`:
   - Lines 849, 854, 859, 929
   - Should use `self.gpu_state_manager.get_or_create_cache(dom_id)`

2. Fix `content_size` access in `layout/src/gpu.rs:97-98`:
   - Use correct fields from `UnifiedLayout`

---

### Priority 5: Fix Clone/Default Traits (Medium)

**Estimated Time:** 30 minutes

**Option A:** Remove Clone from LayoutWindow
```rust
#[derive(Debug)]  // Remove Clone
pub struct LayoutWindow {
    // ...
}
```

**Option B:** Add Clone to DisplayList
```rust
#[derive(Clone)]
pub struct DisplayList {
    // ...
}
```

**Option C:** Wrap DisplayList in Arc
```rust
pub struct LayoutWindow {
    pub display_list: Arc<DisplayList>,
}
```

---

### Priority 6: Add Missing LayoutSize Import (Quick Fix)

**Estimated Time:** 5 minutes

Add to `layout/src/gpu.rs`:
```rust
use azul_css::props::basic::LayoutSize;
```

---

### Priority 7: Verify All Fixes and Compile

**Estimated Time:** 30 minutes

1. Run `cargo check --all`
2. Fix any remaining errors
3. Ensure clean compilation

---

### Priority 8: Run Tests

**Estimated Time:** 1 hour

1. Run `cargo test --package azul-layout`
2. Fix any test failures
3. Add missing tests from Phase 4a, 4b, 4c

---

## Summary by Phase

### Phase 3.5 (Event System): ✅ COMPLETE

- ✅ EventSource enum exists in core
- ✅ EventPhase enum exists in core
- ❌ Full SyntheticEvent wrapper NOT implemented (but not required for Phase 4)

### Phase 4a (IFrameManager): ⚠️ PARTIAL

- ✅ File created (`layout/src/iframe.rs`)
- ✅ Basic structure implemented
- ❌ Import path errors
- ❌ Missing dedicated unit tests
- ❌ Integration tests incomplete

### Phase 4b (GpuStateManager): ⚠️ PARTIAL

- ✅ File created (`layout/src/gpu.rs`)
- ✅ Basic structure implemented
- ❌ Import path errors
- ❌ Duration API errors
- ❌ Field access errors
- ❌ Missing dedicated unit tests
- ❌ No fade transition implementation yet

### Phase 4c (Event Source): ⚠️ PARTIAL

- ✅ EventSource enum exists
- ✅ Used in ScrollEvent struct
- ❌ EventSource::System variant error
- ❌ Missing tests for different event sources
- ❌ Animation behavior per source not fully tested

### Phase 5 (Scrollbar Transforms): ❌ NOT STARTED

- No implementation yet

### Phase 6 (WebRender Integration): ❌ NOT STARTED

- No implementation yet

---

## Estimated Time to Complete Phase 4

**Total Time:** ~4-5 hours

- Fix compilation errors: 2 hours
- Run and fix tests: 1 hour
- Add missing tests: 1-2 hours
- Documentation: 30 minutes

---

## Next Steps

1. **Immediate:** Fix the 18 compilation errors (Priority 1-6)
2. **Short-term:** Run tests and verify Phase 4 works
3. **Medium-term:** Add missing unit tests for each manager
4. **Long-term:** Begin Phase 5 (Scrollbar Transforms)

---

## Recommendations

1. **Focus on compilation first:** Cannot test until code compiles
2. **Test incrementally:** After each fix, run `cargo check`
3. **Add unit tests to manager files:** Don't rely only on integration tests in window.rs
4. **Consider DLL integration:** Unclear if managers need to be exposed to DLL layer
5. **Update documentation:** Once Phase 4 is complete, update `todo/addscrolling.md` to reflect actual implementation

---

**Document Status:** ACTIVE  
**Next Update:** After fixing compilation errors  
**Owner:** Development Team
