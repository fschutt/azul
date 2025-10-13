# Architecture Change: WindowInternal → LayoutWindow Integration

**Date:** October 13, 2025
**Status:** IN PROGRESS (Step 1 complete)

---

## Problem Statement

Originally, `azul-core` had `WindowInternal` with rendering and layout state, and `azul-layout` had `LayoutWindow` with just layout caches. This created:
- **Duplication**: Both stored timers, threads, GPU cache
- **Circular dependency risk**: Core needed layout types, layout needed core types
- **Complexity**: Two sources of truth for window state

## Solution: Unified LayoutWindow

**Merge all WindowInternal fields into LayoutWindow, remove WindowInternal entirely.**

### Architecture:
```
azul-core (no_std compatible, low-level types)
    ↓
azul-layout (LayoutWindow = THE window manager)
    ↓
azul-dll (uses LayoutWindow, handles platform integration)
```

---

## Step 1: Integrate Fields ✅ COMPLETE

### Added to `layout::LayoutWindow`:
```rust
pub struct LayoutWindow {
    // Original layout fields
    pub layout_cache: Solver3LayoutCache<ParsedFont>,
    pub text_cache: TextLayoutCache<ParsedFont>,
    pub font_manager: FontManager<ParsedFont, PathLoader>,
    pub layout_results: BTreeMap<DomId, DomLayoutResult>,
    pub timers: BTreeMap<TimerId, Timer>,
    pub threads: BTreeMap<ThreadId, Thread>,
    pub gpu_value_cache: BTreeMap<DomId, GpuValueCache>,
    
    // NEW: From WindowInternal
    pub renderer_resources: RendererResources,
    pub renderer_type: Option<RendererType>,
    pub previous_window_state: Option<FullWindowState>,
    pub current_window_state: FullWindowState,
    pub document_id: DocumentId,
    pub id_namespace: IdNamespace,
    pub epoch: Epoch,
    pub gl_texture_cache: GlTextureCache,
}
```

### Added Imports:
- `azul_core::resources::{GlTextureCache, IdNamespace}`
- `azul_core::window::RendererType`

### Updated `LayoutWindow::new()`:
- Initializes all fields with sensible defaults
- `renderer_resources: RendererResources::default()`
- `renderer_type: None`
- `previous_window_state: None`
- `current_window_state: FullWindowState::default()`
- `document_id: DocumentId::new()`
- `id_namespace: IdNamespace::new()`
- `epoch: Epoch::new()`
- `gl_texture_cache: GlTextureCache::default()`

**Status**: ✅ azul-layout compiles successfully

---

## Step 2: Remove WindowInternal from Core ⏳ IN PROGRESS

### Files to modify:
1. **core/src/window.rs**:
   - Remove `pub struct WindowInternal { ... }` (lines ~734-761)
   - Remove all `impl WindowInternal` blocks:
     - `get_dpi_scale_factor()` (line ~764)
     - `new()` (lines ~873-1001) - LARGE FUNCTION, move logic to layout
     - `regenerate_styled_dom()` (lines ~1016-1139)
     - `get_current_scroll_states()` (lines ~1144-1170)
     - `get_content_size()` (lines ~1175-1186)
     - `do_quick_resize()` (lines ~1191-1207)
     - `may_have_changed_monitor()` (lines ~1212-1224)
     - `get_layout_size()` (lines ~1226-1232)
     - `get_menu_bar()` (lines ~1235-1241)
     - `get_context_menu()` (lines ~1245-1271)
     - `run_single_timer()` (lines ~1276-1399) - **IMPORTANT: Uses core::CallbackInfo**
     - `run_all_threads()` (lines ~1402-1534) - **IMPORTANT: Uses core::CallbackInfo**
     - `invoke_single_callback()` (lines ~1537-1637) - **IMPORTANT: Uses core::CallbackInfo**
     - `invoke_menu_callback()` (lines ~1640-1733) - **IMPORTANT: Uses core::CallbackInfo**

2. **core/src/lib.rs**:
   - Remove `WindowInternal` from exports (if present)

### Critical Decision: Where to move callback invocation code?

**Option A: Move to layout (RECOMMENDED)**
- `run_single_timer()`, `run_all_threads()`, `invoke_single_callback()`, `invoke_menu_callback()`
- Becomes: `impl LayoutWindow { ... }`
- Uses `layout::CallbackInfo` instead of `core::CallbackInfo`
- Pros: Clean architecture, callbacks live with layout
- Cons: Need to handle core's `old_layout_result.rs` LayoutResult references

**Option B: Move to dll**
- Keep as free functions in dll
- Pros: Keeps callback logic in integration layer
- Cons: More code in dll, harder to test

**RECOMMENDED: Option A**

---

## Step 3: Update dll to use LayoutWindow ⏸️ TODO

### Files to modify:

1. **dll/src/desktop/shell/appkit/mod.rs** (line ~103):
```rust
// OLD:
pub(crate) struct Window {
    pub(crate) internal: WindowInternal,
    // ...
}

// NEW:
pub(crate) struct Window {
    pub(crate) internal: azul_layout::LayoutWindow,
    // ...
}
```

2. **dll/src/desktop/shell/win32/mod.rs** (line ~487):
```rust
// OLD:
pub struct Window {
    pub internal: WindowInternal,
    // ...
}

// NEW:
pub struct Window {
    pub internal: azul_layout::LayoutWindow,
    // ...
}
```

3. **dll/src/desktop/shell/x11/mod.rs** (line ~1253):
```rust
// OLD:
pub struct Window {
    pub internal: WindowInternal,
    // ...
}

// NEW:
pub struct Window {
    pub internal: azul_layout::LayoutWindow,
    // ...
}
```

4. **Update all WindowInternal::new() calls** (~lines 244, 812, 1899):
```rust
// OLD:
WindowInternal::new(
    WindowInternalInit { ... },
    ...
)

// NEW:
azul_layout::LayoutWindow::new_from_options(
    window_create_options,
    ...
) // Need to add this method to LayoutWindow
```

---

## Step 4: Migrate Callback Invocation ⏸️ TODO

### Create in layout/src/window.rs:
```rust
impl LayoutWindow {
    pub fn run_single_timer(...) -> CallCallbacksResult { ... }
    pub fn run_all_threads(...) -> CallCallbacksResult { ... }
    pub fn invoke_single_callback(...) -> CallCallbacksResult { ... }
    pub fn invoke_menu_callback(...) -> CallCallbacksResult { ... }
}
```

### Update to use `layout::CallbackInfo`:
- Replace `&self.layout_results` (Vec) with `&mut self` (LayoutWindow)
- First parameter becomes `layout_window: *mut LayoutWindow`

---

## Testing Strategy

### After each step:
1. **Step 1**: ✅ `cargo build -p azul-layout --lib` - PASSED
2. **Step 2**: `cargo build -p azul-core --lib` - Should compile after WindowInternal removal
3. **Step 3**: `cargo build -p azul-dll --lib` - Should compile after dll updates
4. **Step 4**: `cargo test -p azul-layout` - Callback tests should pass
5. **Final**: `cargo build --workspace` - Everything compiles

### Integration tests:
- Run examples: `cargo run --example hello_world`
- Verify timers work
- Verify threads work
- Verify callbacks trigger correctly

---

## Benefits

### ✅ Simplification:
- **One** window manager (`LayoutWindow`) instead of two
- **No duplication** of timers/threads/GPU cache
- **Clear ownership**: Layout owns all window state

### ✅ No Circular Dependencies:
- core (low-level types) ← layout (window manager) ← dll (platform integration)
- Clean dependency flow

### ✅ Better Testing:
- All window logic in one place
- Unit tests in layout crate
- No need to mock WindowInternal

### ✅ Cleaner API:
- `layout::CallbackInfo` is the only CallbackInfo
- `layout::LayoutWindow` is the only window manager
- Less confusion for users

---

## Migration Path

### For downstream users:

**If you used `azul-core`:**
```rust
// OLD:
use azul_core::window::WindowInternal;
let window = WindowInternal::new(...);

// NEW:
use azul_layout::LayoutWindow;
let window = LayoutWindow::new(...);
```

**If you used callbacks:**
```rust
// OLD:
use azul_core::callbacks::CallbackInfo;

// NEW:
use azul_layout::callbacks::CallbackInfo;
// API is identical, just different import
```

---

## Remaining Work

- [ ] Step 2: Remove WindowInternal from core (~3-4 hours)
  - [ ] Move 14 methods to LayoutWindow
  - [ ] Update callback methods to use layout::CallbackInfo
  - [ ] Remove WindowInternal struct
  
- [ ] Step 3: Update dll (~2-3 hours)
  - [ ] Update 3 platform-specific Window structs
  - [ ] Update WindowInternal::new() call sites
  - [ ] Test compilation

- [ ] Step 4: Add helper methods (~1-2 hours)
  - [ ] `LayoutWindow::new_from_options()`
  - [ ] `LayoutWindow::get_dpi_scale_factor()`
  - [ ] Other utility methods as needed

- [ ] Testing (~2-3 hours)
  - [ ] Unit tests for LayoutWindow methods
  - [ ] Integration tests with examples
  - [ ] Verify callbacks work correctly

**Estimated total remaining: 8-12 hours**

---

## Current Status

✅ **Step 1 Complete**: LayoutWindow has all WindowInternal fields
⏳ **Step 2 In Progress**: Removing WindowInternal from core
⏸️ **Steps 3-4 Blocked**: Waiting for Step 2 completion

**Next Action**: Remove `pub struct WindowInternal` and move its methods to `LayoutWindow`
