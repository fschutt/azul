# Phase 5: azul-dll API Migration Guide

## Overview
azul-dll needs to be updated from the old `WindowInternal` API to the new Phase 4 `LayoutWindow` API.

## Key API Changes

### 1. LayoutWindow Construction
**Old API (10 parameters):**
```rust
LayoutWindow::new(
    internal.document_id,
    internal.layout_results,
    internal.scroll_states,
    internal.renderer_resources,
    internal.id_namespace,
    internal.gl_texture_cache,
    internal.epoch.clone(),
    internal.previous_window_state.clone(),
    internal.current_window_state.clone(),
    ud,
)
```

**New API (1 parameter, returns Result):**
```rust
let mut layout_window = LayoutWindow::new(fc_cache)?;
// Then set fields directly:
layout_window.current_window_state = window_state;
layout_window.document_id = document_id;
// etc.
```

### 2. Layout Function Signature
**Old API:**
```rust
internal.regenerate_styled_dom(
    data,
    image_cache,
    gl_context,
    &mut resource_updates,
    hidpi_factor,
    &CALLBACKS,
    fc_cache,
    azul_layout::solver2::do_the_relayout,  // Layout callback
    |window_state, scroll_states, layout_results| { ... },  // Hit test callback
    &mut debug_messages,
);
```

**New API:**
```rust
layout_window.layout_and_generate_display_list(
    styled_dom,
    &window_state,
    &renderer_resources,
    &system_callbacks,
    &mut debug_messages,
)?;
```

### 3. Scroll State Access
**Old API:**
```rust
internal.scroll_states  // Direct BTreeMap access
internal.current_window_state.process_system_scroll(...)
```

**New API:**
```rust
layout_window.scroll_states  // ScrollManager struct
// No more process_system_scroll method - handled internally
```

### 4. Result Handling
**Old API:**
```rust
let internal = WindowInternal { ... };  // Direct struct
internal.current_window_state.field
internal.document_id
```

**New API:**
```rust
let internal = LayoutWindow::new(fc_cache)?;  // Returns Result
internal.current_window_state.field  // Direct access after unwrap/? 
internal.document_id
```

### 5. Missing Types

#### AlphaType
**Old Location:** `azul_core::??`
**New Location:** Check `webrender::api::AlphaType` or equivalent

#### BorderStyleNoNone
**Old API:**
```rust
azul_css::BorderStyle::normalize_border() -> Option<BorderStyleNoNone>
```

**New API:** 
Likely removed - need to handle None variant directly in match statements

### 6. Missing Methods

#### FullWindowState::process_system_scroll
**Status:** Removed - scroll handling now internal to ScrollManager

#### FullWindowState::from_window_state  
**Status:** Removed or renamed - check FullWindowState constructors

#### FullWindowState::layout_callback_changed
**Status:** Removed - layout callback handling changed

### 7. Callback System Changes

**Old:**
```rust
MenuCallback vs CoreMenuCallback  // Two different types
config.system_callbacks  // Field existed
```

**New:**
```rust
MenuCallback  // Unified type?
// system_callbacks passed as parameter to layout functions
```

## Migration Strategy

### Step 1: Fix Module Imports
- [ ] Change all `solver2` → `solver3`
- [ ] Remove references to removed modules (dialogs?)

### Step 2: Fix LayoutWindow Construction
- [ ] Update all `LayoutWindow::new()` calls to 1-parameter version
- [ ] Handle Result<> properly with ? or unwrap()
- [ ] Set fields directly after construction

### Step 3: Update Layout Invocation
- [ ] Replace `regenerate_styled_dom` with `layout_and_generate_display_list`
- [ ] Remove layout callback parameter (now built-in)
- [ ] Update hit-test callback handling

### Step 4: Fix Access Patterns
- [ ] Add `.unwrap()` or `?` when accessing LayoutWindow fields after Result
- [ ] Update scroll state access to use ScrollManager API

### Step 5: Fix Type Mismatches
- [ ] Find correct import for AlphaType
- [ ] Handle BorderStyle without normalize_border()
- [ ] Fix MenuCallback type conflicts

### Step 6: Platform-Specific Fixes
- [ ] Fix macOS (appkit) code
- [ ] Fix Windows (win32) code
- [ ] Ensure shell/event.rs and shell/process.rs remain platform-agnostic

## Files to Update

### Core Shell Files (Platform-Agnostic)
- `dll/src/desktop/shell/event.rs` - Event handling
- `dll/src/desktop/shell/process.rs` - Event processing

### Platform-Specific Files
- `dll/src/desktop/shell/appkit/mod.rs` - macOS window creation
- `dll/src/desktop/shell/appkit/menu.rs` - macOS menus
- `dll/src/desktop/shell/win32/mod.rs` - Windows window creation

### Rendering Files
- `dll/src/desktop/wr_translate.rs` - WebRender translation layer

### Widget Files (if affected)
- Various files in `dll/src/widgets/`
