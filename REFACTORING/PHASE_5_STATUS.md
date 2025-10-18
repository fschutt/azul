# Phase 5 Status: azul-dll Migration

## Date: 18. Oktober 2025

## Summary
Started Phase 5 to update azul-dll for macOS and Windows. azul-dll has **148 compilation errors** that need to be fixed. This is a significantly larger effort than the Phase 4 core/layout refactoring.

## Completed
- ✅ Changed all `solver2` → `solver3` references (find/replace in dll/src)
- ✅ Added missing imports: `StyleBorderRadius`, `ImageMask`  
- ✅ Commented out unused/obsolete functions: `wr_translate_layout_side_offsets`, `wr_translate_image_mask`
- ✅ Created API migration guide: `REFACTORING/PHASE_5_DLL_API_MIGRATION.md`
- ✅ Analyzed error patterns and categorized by issue type

## Current Errors (148 total)

### Category 1: Missing Modules (3 errors)
**Files affected:** `dll/src/widgets/color_input.rs`, `dll/src/widgets/file_input.rs`

```rust
error[E0432]: unresolved import `crate::dialogs`
```

**Fix needed:** 
- Create `dll/src/dialogs.rs` module OR
- Remove dialog functionality from widgets

### Category 2: Missing CALLBACKS constant (~8 errors)
**Files affected:** `shell/event.rs`, `shell/process.rs`, `shell/appkit/mod.rs`

```rust
error[E0425]: cannot find value `CALLBACKS` in module `crate::desktop::app`
```

**Fix needed:**
- Define `CALLBACKS` in `dll/src/desktop/app.rs` as `ExternalSystemCallbacks`
- Or pass callbacks as parameter instead of global

### Category 3: Obsolete Layout API (~12 errors)
**Files affected:** `shell/event.rs`, `shell/process.rs`, `shell/appkit/mod.rs`, `shell/win32/mod.rs`

```rust
error[E0425]: cannot find value `do_the_relayout` in module `azul_layout::solver3`
internal.regenerate_styled_dom(..., solver3::do_the_relayout, ...)
```

**Fix needed:**
- Replace `regenerate_styled_dom()` calls with `layout_and_generate_display_list()`
- Remove layout callback parameter (now built-in)
- Update to new LayoutWindow API

### Category 4: LayoutWindow Construction (~15 errors)
**Files affected:** `shell/appkit/mod.rs`, `shell/win32/mod.rs`

```rust
error[E0422]: cannot find struct `LayoutWindowInit`
error[E0061]: LayoutWindow::new takes 1 argument but 10 arguments were supplied
```

**Fix needed:**
```rust
// OLD:
let internal = LayoutWindow::new(
    document_id, layout_results, scroll_states, renderer_resources,
    id_namespace, gl_texture_cache, epoch,
    previous_window_state, current_window_state, ud
);

// NEW:
let mut internal = LayoutWindow::new(fc_cache)?;
internal.document_id = document_id;
internal.current_window_state = current_window_state;
// etc.
```

### Category 5: Result<> Handling (~25 errors)
**Files affected:** `shell/appkit/mod.rs`

```rust
error[E0599]: no method `get_content_size` found for enum `Result<T, E>`
error[E0609]: no field `current_window_state` on type `Result<LayoutWindow, ...>`
```

**Fix needed:**
- Add `.unwrap()` or `?` after `LayoutWindow::new()`
- Or use `match`/`if let Ok()` pattern

### Category 6: Scroll API Changes (~10 errors)
**Files affected:** `shell/process.rs`

```rust
error[E0599]: no method named `process_system_scroll` found for struct `FullWindowState`
error[E0599]: no function named `from_window_state` found
error[E0599]: no method named `layout_callback_changed` found
```

**Fix needed:**
- ScrollManager now handles scrolling internally
- Remove calls to removed methods
- Update to new ScrollManager API

### Category 7: Type Annotation Issues (~60 errors)
**Files affected:** `wr_translate.rs`

```rust
error[E0282]: type annotations needed
  --> dll/src/desktop/wr_translate.rs:914:20
    |
914 | .and_then(|tl| tl.get_property_or_default())
    |         ^^     ----------------------- type must be known at this point
```

**Fix needed:**
- Add explicit type annotations to closures
- Or refactor to make types inferrable

### Category 8: BorderStyle API Changes (~10 errors)
**Files affected:** `wr_translate.rs`

```rust
error[E0599]: no method named `normalize_border` found for enum `azul_css::props::style::BorderStyle`
error[E0433]: use of undeclared type `BorderStyleNoNone`
```

**Fix needed:**
- `BorderStyle::normalize_border()` method removed
- Handle `None` variant directly in match statements
- Remove references to `BorderStyleNoNone` type

### Category 9: Missing Functions (~2 errors)
**Files affected:** `wr_translate.rs`

```rust
error[E0425]: cannot find function `push_display_list_content` in this scope
```

**Fix needed:**
- Function renamed or removed
- Check solver3 display list API

### Category 10: Menu Callback Type Mismatch (~2 errors)
**Files affected:** `shell/appkit/menu.rs`

```rust
error[E0308]: expected `MenuCallback`, found `CoreMenuCallback`
```

**Fix needed:**
- Unify callback types or add conversion
- Check if types were merged in Phase 4

### Category 11: Miscellaneous (~3 errors)
- AlphaType enum location changed
- Various struct field access issues

## Architecture Analysis

### Platform-Agnostic Shell Code
✅ **Design Goal Confirmed:** `shell/event.rs` and `shell/process.rs` are designed to be platform-independent and reused across macOS/Windows/Linux.

**Current structure:**
```
dll/src/desktop/shell/
├── event.rs          # Platform-agnostic event handling
├── process.rs        # Platform-agnostic event processing  
├── mod.rs
├── appkit/           # macOS-specific (uses shell/event.rs, shell/process.rs)
├── win32/            # Windows-specific (uses shell/event.rs, shell/process.rs)
└── x11/              # Linux-specific (uses shell/event.rs, shell/process.rs)
```

### Key Changes from Phase 4

1. **LayoutWindow is now Result<>**
   - Old: Direct struct construction
   - New: `LayoutWindow::new(fc_cache)?` returns `Result<LayoutWindow, LayoutError>`

2. **Layout API simplified**
   - Old: `regenerate_styled_dom()` with callback parameters
   - New: `layout_and_generate_display_list()` - layout logic is built-in

3. **Manager Architecture**
   - Old: Direct `scroll_states: BTreeMap<>`
   - New: `scroll_states: ScrollManager` with encapsulated logic

4. **No more solver2**
   - `solver3` is the current layout solver
   - `do_the_relayout` function doesn't exist - layout is automatic

## Next Steps (Recommended Order)

### Phase 5A: Minimal Compilation (Priority: HIGH)
**Goal:** Get azul-dll to compile, even with stubs

1. Create stub `dialogs` module (2 hours)
2. Define `CALLBACKS` constant in `app.rs` (1 hour)
3. Comment out broken `az_regenerate_dom` functions temporarily (1 hour)
4. Fix LayoutWindow::new() calls with Result<> handling (3 hours)
5. Add type annotations to closures in wr_translate.rs (4 hours)

**Estimated:** 11 hours
**Result:** DLL compiles but missing functionality

### Phase 5B: Restore Core Functionality (Priority: MEDIUM)
**Goal:** Restore layout and rendering

1. Reimplement `az_regenerate_dom` with new LayoutWindow API (8 hours)
2. Update `az_redo_hit_test` for new API (4 hours)
3. Fix ScrollManager integration (3 hours)
4. Restore display list generation (4 hours)

**Estimated:** 19 hours
**Result:** Basic layout and rendering work

### Phase 5C: Platform-Specific Fixes (Priority: MEDIUM)
**Goal:** macOS and Windows both work

1. Fix macOS window creation (appkit/mod.rs) (6 hours)
2. Fix Windows window creation (win32/mod.rs) (6 hours)
3. Fix menu handling for both platforms (4 hours)
4. Test cross-platform builds (2 hours)

**Estimated:** 18 hours
**Result:** Both platforms functional

### Phase 5D: Widget & Polish (Priority: LOW)
**Goal:** Widgets and dialogs work

1. Restore dialog functionality (8 hours)
2. Fix widget implementations (6 hours)
3. End-to-end testing (4 hours)
4. Documentation updates (2 hours)

**Estimated:** 20 hours
**Result:** Full feature parity

## Total Estimated Effort
- **Phase 5A (Compilation):** 11 hours
- **Phase 5B (Core Functionality):** 19 hours
- **Phase 5C (Cross-Platform):** 18 hours  
- **Phase 5D (Polish):** 20 hours
- **TOTAL:** ~68 hours (~9 working days)

## Risk Assessment

**HIGH RISK:**
- `az_regenerate_dom` is core to the rendering pipeline
- Incorrect LayoutWindow migration could cause crashes
- Scroll handling changes affect all UI interactions

**MEDIUM RISK:**
- Type inference issues in wr_translate.rs might cascade
- Menu callback type mismatches could break menus
- BorderStyle changes might affect all border rendering

**LOW RISK:**
- Dialog module can be stubbed/removed
- Widgets can be fixed incrementally
- Platform-specific code is isolated

## Recommendation

Given the scope (148 errors, ~68 hours), I recommend:

1. **Short-term:** Complete Phase 5A to get compilation working
2. **Medium-term:** Complete Phase 5B to restore core functionality
3. **Long-term:** Complete Phase 5C-D for full feature parity

**Alternative:** If time is limited, consider keeping azul-dll on hold and using azul-core/azul-layout directly from Rust code until DLL migration can be completed properly.

## Files Modified So Far
- ✅ `dll/src/desktop/wr_translate.rs` - Added imports, commented obsolete functions
- ✅ All `dll/src/**/*.rs` - Changed solver2 → solver3

## Files That Need Major Work
- ❌ `dll/src/desktop/shell/event.rs` - Needs complete rewrite of az_regenerate_dom
- ❌ `dll/src/desktop/shell/process.rs` - Needs callback system update
- ❌ `dll/src/desktop/shell/appkit/mod.rs` - Needs LayoutWindow::new() fixes
- ❌ `dll/src/desktop/shell/win32/mod.rs` - Same as appkit
- ❌ `dll/src/desktop/app.rs` - Needs CALLBACKS definition
- ❌ `dll/src/widgets/*.rs` - Need dialog module or removal
- ❌ `dll/src/desktop/wr_translate.rs` - Needs type annotations

## Conclusion

azul-dll migration is a **large undertaking** requiring careful API translation from the old WindowInternal system to the new LayoutWindow system. The good news is that azul-core and azul-layout are working perfectly cross-platform, providing a solid foundation.

The DLL layer is essentially a C-API wrapper, so while tedious to fix, each error is mechanical rather than architectural. With ~68 hours of focused work, azul-dll can be fully updated for both macOS and Windows.
