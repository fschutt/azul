# Callback Migration Plan (Correct Approach)

**Date**: October 13, 2025  
**Status**: Planning Phase

---

## Problem

Currently:
- `azul-core` has `WindowInternal` with callback methods using `core::CallbackInfo`
- `azul-layout` has `LayoutWindow` with `layout::CallbackInfo`
- Need to migrate callback invocation to use the new `layout::CallbackInfo`

**DO NOT re-import old_layout_result.rs** - it's old code that should be removed.

---

## Strategy: Move Callback Methods to LayoutWindow

### Step 1: Identify Methods to Move

From `core/src/window.rs` `impl WindowInternal`:

1. **`run_single_timer()`** (lines ~1276-1399)
   - Invokes a single timer callback
   - Uses `core::CallbackInfo::new()`
   - Returns `CallCallbacksResult`

2. **`run_all_threads()`** (lines ~1402-1534)
   - Invokes all thread write-back callbacks
   - Uses `core::CallbackInfo::new()`
   - Returns `CallCallbacksResult`

3. **`invoke_single_callback()`** (lines ~1537-1637)
   - Invokes window create/shutdown callbacks
   - Uses `core::CallbackInfo::new()`
   - Returns `CallCallbacksResult`

4. **`invoke_menu_callback()`** (lines ~1640-1733)
   - Invokes menu item callbacks
   - Uses `core::CallbackInfo::new()`
   - Returns `CallCallbacksResult`

### Step 2: What Needs to Change

#### In `layout/src/window.rs`:

```rust
impl LayoutWindow {
    // ADD THESE METHODS (moved from core::WindowInternal)
    
    pub fn run_single_timer(
        &mut self,
        timer_id: usize,
        frame_start: Instant,
        current_window_handle: &RawWindowHandle,
        gl_context: &OptionGlContextPtr,
        image_cache: &mut ImageCache,
        system_fonts: &mut FcFontCache,
        system_callbacks: &ExternalSystemCallbacks,
    ) -> CallCallbacksResult {
        // Use layout::CallbackInfo::new(&mut self, ...)
        // instead of core::CallbackInfo::new(&self.layout_results, ...)
    }
    
    // ... similar for other 3 methods
}
```

#### Key Changes:
1. **First parameter**: Change from `&self.layout_results` (Vec) to `&mut self` (LayoutWindow)
2. **Import**: Use `use crate::callbacks::CallbackInfo` (layout version)
3. **Access**: Use `self.timers`, `self.threads`, etc. directly

### Step 3: Handle Missing Types

#### Problem: `CallCallbacksResult` is defined in core

**Solution**: Either
- **Option A**: Move `CallCallbacksResult` to layout
- **Option B**: Re-export it from layout
- **Option C**: Keep it in core and import it in layout

**Recommendation**: Option C - it's just a return type, can stay in core.

#### Problem: Methods access `self.layout_results` as `Vec<LayoutResult>`

**Solution**: 
- LayoutWindow uses `BTreeMap<DomId, DomLayoutResult>`
- Need adapter methods to provide compatibility
- OR: Rewrite methods to work with new structure

### Step 4: Clean Up Core

After moving methods to layout:

1. **Remove from `core/src/window.rs`**:
   - `impl WindowInternal { run_single_timer, run_all_threads, invoke_single_callback, invoke_menu_callback }`
   
2. **Keep in core**:
   - `pub struct WindowInternal` (may be used by dll)
   - Other WindowInternal methods that don't involve callbacks

3. **Update dll** to call `layout_window.run_single_timer()` instead of `window_internal.run_single_timer()`

---

## Implementation Steps

### ✅ Step 1: Prepare layout::CallbackInfo (DONE)
- Already has all 26 fields
- Already uses `*mut LayoutWindow`
- Ready to use

### ⏸️ Step 2: Add Missing Imports to LayoutWindow

```rust
// In layout/src/window.rs, add imports:
use azul_core::{
    callbacks::{Callback, MenuCallback, DomNodeId, RefAny, Update},
    task::{Instant, TerminateTimer, ThreadWriteBackMsg, ThreadReceiveMsg, 
           OptionThreadReceiveMsg, ThreadSendMsg},
    window::{WindowState, RawWindowHandle, OptionLogicalPosition},
    resources::{ImageCache, RendererResources, ResourceUpdate},
    gl::OptionGlContextPtr,
    // ... other needed types
};
```

### ⏸️ Step 3: Add CallCallbacksResult to Layout

**Option A**: Copy the struct
```rust
// In layout/src/callbacks.rs
#[derive(Debug)]
pub struct CallCallbacksResult {
    pub should_scroll_render: bool,
    pub callbacks_update_screen: Update,
    pub modified_window_state: Option<WindowState>,
    // ... all fields
}
```

**Option B**: Re-export from core
```rust
// In layout/src/lib.rs
pub use azul_core::window::CallCallbacksResult;
```

**Recommendation**: Option B (re-export)

### ⏸️ Step 4: Copy First Method (run_single_timer)

1. Copy the entire `run_single_timer` method from core to layout
2. Change signature: `&mut self` instead of taking external params
3. Replace `CallbackInfo::new(&self.layout_results, ...)` with `CallbackInfo::new(&mut self, ...)`
4. Use `self.timers.get_mut(...)` instead of `self.timers.get_mut(...)`
5. Test compilation

### ⏸️ Step 5: Handle layout_results Access

**Problem**: Code accesses `&self.layout_results` expecting `Vec<LayoutResult>`

**Solution**: Add adapter method to LayoutWindow:
```rust
impl LayoutWindow {
    // Temporary compatibility: get layout results as Vec
    pub fn get_layout_results_vec(&self) -> Vec<OldLayoutResult> {
        // Convert BTreeMap to Vec format if needed
        // OR: Keep a cached Vec version
        // OR: Rewrite calling code to use BTreeMap
    }
}
```

**Better Solution**: Rewrite the calling code to not need Vec access.

### ⏸️ Step 6: Repeat for Other Methods

- `run_all_threads()`
- `invoke_single_callback()`
- `invoke_menu_callback()`

### ⏸️ Step 7: Update dll Call Sites

In `dll/src/desktop/shell/*.rs` (appkit, win32, x11):

```rust
// OLD:
window.internal.run_single_timer(...)

// NEW:
window.layout_window.run_single_timer(...)
```

### ⏸️ Step 8: Remove Old Code from Core

```rust
// In core/src/window.rs
impl WindowInternal {
    // REMOVE: run_single_timer
    // REMOVE: run_all_threads
    // REMOVE: invoke_single_callback
    // REMOVE: invoke_menu_callback
}
```

---

## Current Blockers

1. **LayoutWindow doesn't have all WindowInternal fields yet**
   - Need: `renderer_resources`, `renderer_type`, `previous_window_state`, etc.
   - Solution: Either add them OR keep WindowInternal and LayoutWindow separate

2. **Missing types in layout crate**
   - `CallCallbacksResult` - can re-export from core
   - `TerminateTimer`, `ThreadWriteBackMsg`, etc. - import from core
   
3. **Different data structures**
   - WindowInternal: `Vec<LayoutResult>`
   - LayoutWindow: `BTreeMap<DomId, DomLayoutResult>`
   - Need adapter or rewrite

---

## Decision Point

### Option A: Full Integration (WindowInternal → LayoutWindow)
- Move ALL WindowInternal fields to LayoutWindow
- Move ALL WindowInternal methods to LayoutWindow
- Remove WindowInternal completely
- **Time**: 12-16 hours
- **Benefit**: Clean architecture, single source of truth

### Option B: Partial Integration (Callbacks Only)
- Keep WindowInternal in core with its fields
- Move ONLY callback methods to LayoutWindow
- LayoutWindow works with LayoutResult data passed from WindowInternal
- **Time**: 6-8 hours
- **Benefit**: Faster, less risky

### Option C: Compatibility Layer
- Keep BOTH WindowInternal and LayoutWindow
- Add conversion/sync methods between them
- Gradually migrate over time
- **Time**: 4-6 hours initial, ongoing maintenance
- **Benefit**: Fastest to get compiling

---

## Recommendation

**Choose Option B**: Partial Integration (Callbacks Only)

### Why:
1. **Focused scope**: Only callback methods move, not all window state
2. **Faster**: Can complete in one session
3. **Lower risk**: Doesn't touch WindowInternal's core functionality
4. **Clear goal**: Get `layout::CallbackInfo` working

### How:
1. Add adapter methods to LayoutWindow to provide `Vec<LayoutResult>` view if needed
2. Copy callback methods to LayoutWindow
3. Update methods to use `layout::CallbackInfo`
4. Keep WindowInternal in core for dll usage
5. Test thoroughly

---

## Next Steps

**Immediate Action** (choose one):

**Path 1: Quick Adapter**
```rust
// In layout/src/window.rs
impl LayoutWindow {
    // Temporary: provide old-style access for callbacks
    pub fn as_legacy_vec(&self) -> Vec<LegacyLayoutResult> {
        // Convert BTreeMap to Vec format
    }
}
```

**Path 2: Clean Rewrite**
```rust
// Rewrite callback methods to work with BTreeMap directly
// No Vec conversion needed
```

**Path 3: Full Integration**
```rust
// Merge all WindowInternal into LayoutWindow
// Biggest change but cleanest result
```

---

## Testing Strategy

After each method migration:
1. `cargo build -p azul-layout --lib` - must compile
2. `cargo build -p azul-core --lib` - must compile (with fewer errors)
3. `cargo build -p azul-dll --lib` - integration test
4. Run example: `cargo run --example hello_world`

---

## Current Status

- ✅ `layout::CallbackInfo` ready (all 26 fields)
- ✅ `layout::LayoutWindow` has timers, threads, GPU cache
- ⏸️ Callback methods still in `core::WindowInternal`
- ⏸️ dll still uses `core::WindowInternal`
- ❌ Cannot compile due to missing `LayoutResult` type

**BLOCKED ON**: Need to decide Option A, B, or C above

**RECOMMENDED**: Start with Option B (Partial Integration - Callbacks Only)
