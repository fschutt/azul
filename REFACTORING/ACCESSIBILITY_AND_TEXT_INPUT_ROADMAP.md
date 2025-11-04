# Accessibility and Text Input Integration Roadmap

**Date**: November 3, 2025  
**Goal**: Complete accessibility integration with accesskit and proper text input with IME support

---

## Phase 1: Terminology Cleanup ✅ (In Progress)

### 1.1 Rename "absolute_positions" to "calculated_positions"
- **Rationale**: These positions are already in logical units and have nothing to do with HiDPI
- **Files to update**:
  - `layout/src/window.rs` - Main struct fields and methods
  - `layout/src/solver3/cache.rs` - LayoutCache struct
  - `layout/src/solver3/mod.rs` - Layout computation
  - `layout/src/solver3/positioning.rs` - Position calculations
  - `layout/src/solver3/display_list.rs` - Display list generation
  - `layout/src/solver3/tests.rs` - Test fixtures
- **Impact**: ~76 occurrences across 7 files
- **Breaking**: Internal API only, no public API changes

**Implementation Steps**:
1. Replace struct field names: `absolute_positions` → `calculated_positions`
2. Update local variable names: `abs_pos` → `calc_pos`
3. Update comments and documentation
4. Verify compilation

---

## Phase 2: Windows (Win32) Platform Integration

### 2.1 Text Input / IME Integration
**Location**: `dll/src/desktop/shell2/windows/`

**Tasks**:
- [ ] Review current text input handling in `mod.rs`
- [ ] Integrate Windows IME composition events
  - [ ] `WM_IME_STARTCOMPOSITION` - Start IME session
  - [ ] `WM_IME_COMPOSITION` - Update composition string
  - [ ] `WM_IME_ENDCOMPOSITION` - Commit composition
  - [ ] `WM_IME_CHAR` - Direct character input
- [ ] Map Windows IME events to `TextInputManager`
- [ ] Test with:
  - [ ] Japanese IME (Hiragana → Kanji conversion)
  - [ ] Chinese IME (Pinyin → Characters)
  - [ ] Korean IME (Hangul composition)
- [ ] Handle composition window positioning (cursor rect)

**Key Windows APIs**:
```rust
// In windows/mod.rs
WM_IME_STARTCOMPOSITION
WM_IME_COMPOSITION  
WM_IME_ENDCOMPOSITION
WM_IME_SETCONTEXT
WM_IME_NOTIFY
```

### 2.2 Accessibility Integration (accesskit)
**Location**: `dll/src/desktop/shell2/windows/`

**Tasks**:
- [ ] Add `accesskit` dependency for Windows
- [ ] Create Windows accessibility adapter
  - [ ] Implement `accesskit::ActionHandler` trait
  - [ ] Map accesskit actions to internal events
- [ ] Hook up adapter to window creation
- [ ] Implement tree update mechanism:
  - [ ] Convert Azul accessibility tree to accesskit tree
  - [ ] Submit updates via `accesskit::TreeUpdate`
- [ ] Test with:
  - [ ] Windows Narrator
  - [ ] NVDA screen reader
  - [ ] JAWS screen reader
- [ ] Verify standard keyboard navigation (Tab, Shift+Tab, arrows)

**Key Files to Create/Modify**:
- `dll/src/desktop/shell2/windows/accessibility.rs` (new)
- `dll/src/desktop/shell2/windows/mod.rs` (integrate adapter)

---

## Phase 3: Linux (X11) Platform Integration

### 3.1 Text Input / IME Integration  
**Location**: `dll/src/desktop/shell2/linux/x11/`

**Tasks**:
- [ ] Review current text input handling in `events.rs`
- [ ] Integrate X11 Input Method (XIM/ibus/fcitx)
  - [ ] Handle `KeyPress` events with XIM context
  - [ ] Process pre-edit (composition) strings
  - [ ] Handle commit strings
- [ ] Map X11 IME events to `TextInputManager`
- [ ] Test with:
  - [ ] ibus (common on Ubuntu/Fedora)
  - [ ] fcitx (common on Arch/Manjaro)
  - [ ] Japanese/Chinese/Korean input
- [ ] Handle pre-edit window positioning

**Key X11 Mechanisms**:
```rust
// In linux/x11/events.rs
XKeyEvent with XLookupString
XIM context for composition
KeyPress with XFilterEvent
```

### 3.2 Accessibility Integration (accesskit)
**Location**: `dll/src/desktop/shell2/linux/x11/`

**Tasks**:
- [ ] Add `accesskit_unix` dependency for Linux
- [ ] Create X11/Wayland accessibility adapter
  - [ ] Implement AT-SPI bridge via accesskit_unix
  - [ ] Handle accessibility tree updates
- [ ] Hook up adapter to window creation
- [ ] Test with:
  - [ ] Orca screen reader (GNOME)
  - [ ] BRLTTY (Braille display support)
  - [ ] accerciser (accessibility explorer)
- [ ] Verify keyboard navigation works

**Key Files to Create/Modify**:
- `dll/src/desktop/shell2/linux/x11/accessibility.rs` (new)
- `dll/src/desktop/shell2/linux/x11/mod.rs` (integrate adapter)

---

## Phase 4: Accessibility Tree Architecture Review

### 4.1 Tree Update Mechanism
**Location**: `layout/src/managers/a11y.rs`

**Current State**:
```rust
// In window.rs::layout_and_generate_display_list()
#[cfg(feature = "accessibility")]
if result.is_ok() {
    let _tree_update = crate::managers::a11y::A11yManager::update_tree(
        self.a11y_manager.root_id,
        &self.layout_results,
        &self.current_window_state.title,
        self.current_window_state.size.dimensions,
    );
    // TODO: Pass tree_update to platform adapter
}
```

**Tasks**:
- [ ] Verify `A11yManager::update_tree()` generates correct accesskit tree
- [ ] Implement tree diffing for incremental updates (performance)
- [ ] Add platform adapter callback to submit tree updates
- [ ] Test tree update on:
  - [ ] Initial render
  - [ ] DOM changes (add/remove nodes)
  - [ ] Property changes (text, role, states)
  - [ ] Focus changes

### 4.2 External Tree Querying
**Tasks**:
- [ ] Implement platform-specific queries from screen readers
- [ ] Handle focus requests from assistive tech
- [ ] Handle action requests (click, type, etc.)
- [ ] Verify coordinate mapping (logical → screen coordinates)

### 4.3 Initial A11y Tree Render
**Tasks**:
- [ ] Ensure tree is built on first layout
- [ ] Verify root node has correct window properties
- [ ] Test empty DOM edge case
- [ ] Profile performance impact of tree generation

---

## Phase 5: Text Editing Feature Completion

### 5.1 Mouse Click → Cursor Position
**Location**: `layout/src/managers/focus_cursor.rs`, event processing

**Tasks**:
- [ ] Implement hit-test to text cursor conversion
  - [ ] Click position → DOM node
  - [ ] Click position → text offset within node
  - [ ] Text offset → TextCursor (cluster_id + affinity)
- [ ] Create helper: `fn screen_pos_to_cursor(pos: LogicalPosition, layout_window: &LayoutWindow) -> Option<TextCursor>`
- [ ] Integrate into mouse click event handler
- [ ] Test with:
  - [ ] Single-line text
  - [ ] Multi-line text  
  - [ ] RTL text (Arabic, Hebrew)
  - [ ] Ligatures and complex scripts

**Key Functions Needed**:
```rust
// In layout/src/text3/cache.rs or new module
pub fn hit_test_to_cursor(
    inline_layout: &UnifiedLayout,
    local_pos: LogicalPosition,
) -> Option<TextCursor>;

// Maps logical position to cluster index + affinity
```

### 5.2 Text Input Without Callbacks
**Current Flow**:
1. User types → `TextInputManager::record_input()`
2. Events generated → `dispatch_synthetic_events()`
3. Callbacks invoked → may call `preventDefault()`
4. **If !preventDefault**: Apply text changes internally
5. Relayout → update text cache
6. ✅ **Already implemented**: Scroll cursor into view

**Tasks**:
- [x] Text input recording (TextInputManager)
- [x] Event generation (determine_all_events)
- [x] Event dispatch (dispatch_synthetic_events)
- [x] Callback invocation (process_callback_result_v2)
- [x] Text post-processing (apply_text_changeset)
- [x] Cursor scroll-into-view (get_focused_cursor_rect + scroll logic)
- [ ] Test contenteditable nodes work end-to-end:
  - [ ] Type in input → text appears
  - [ ] Cursor visible and positioned correctly
  - [ ] Backspace/Delete work
  - [ ] Arrow keys work
  - [ ] Selection works (Shift+arrows, mouse drag)

### 5.3 Automatic Scroll Frame Creation
**Location**: Layout/display list generation

**Current Issue**: When cursor goes out of bounds, need to convert clip frame to scroll frame

**Tasks**:
- [ ] Detect when content overflows (text longer than container)
- [ ] Check if cursor is outside visible area
- [ ] Dynamically add scrollbar_info to layout node
- [ ] Generate scroll frame in display list instead of clip frame
- [ ] Update ScrollManager with new scrollable node
- [ ] Test:
  - [ ] Type long text in small input → auto-scrolls
  - [ ] Type newlines in textarea → vertical scroll appears
  - [ ] Both horizontal + vertical scrollbars work

**Architecture Decision**:
- Option A: Detect in layout pass, add scrollbar_info to node
- Option B: Detect in post-layout, mark node as needing scroll
- **Recommended**: Option A (cleaner, one-pass)

---

## Phase 6: Testing and Validation

### 6.1 Platform-Specific Testing
**Windows**:
- [ ] Windows Narrator announces UI correctly
- [ ] IME composition works (Japanese, Chinese, Korean)
- [ ] Keyboard navigation works
- [ ] Text selection with mouse/keyboard
- [ ] Copy/paste (prepare hooks, full implementation later)

**Linux**:
- [ ] Orca screen reader works
- [ ] ibus/fcitx IME works
- [ ] Keyboard navigation
- [ ] Text selection
- [ ] Copy/paste hooks

**macOS** (already implemented):
- [x] VoiceOver works
- [x] IME works
- [x] Keyboard navigation
- [ ] Verify recent changes didn't break anything

### 6.2 Cross-Platform Testing
- [ ] Create test app with:
  - [ ] Text inputs (single-line, multi-line)
  - [ ] Buttons with labels
  - [ ] Lists with screen reader navigation
  - [ ] Complex forms
- [ ] Verify behavior is consistent across platforms
- [ ] Document any platform-specific quirks

### 6.3 Performance Testing
- [ ] Profile accessibility tree generation cost
- [ ] Profile text input latency
- [ ] Profile cursor scroll-into-view cost
- [ ] Ensure < 16ms frame time for 60 FPS

---

## Implementation Order

### Week 1: Terminology + Windows
1. ✅ Rename absolute_positions → calculated_positions
2. Implement Windows IME integration
3. Implement Windows accesskit adapter
4. Test with Narrator + Japanese IME

### Week 2: Linux + A11y Review
1. Implement Linux IME integration (X11)
2. Implement Linux accesskit adapter
3. Review and fix accessibility tree update mechanism
4. Test with Orca + ibus

### Week 3: Text Editing Features
1. Implement mouse click → cursor position
2. Test contenteditable end-to-end on all platforms
3. Implement automatic scroll frame creation
4. Test cursor scroll-into-view thoroughly

### Week 4: Testing + Polish
1. Cross-platform testing with real screen readers
2. Performance profiling and optimization
3. Documentation updates
4. Bug fixes and edge cases

---

## Success Criteria

- ✅ Code compiles on all platforms (Windows, Linux, macOS)
- ✅ Windows Narrator can navigate UI and read text
- ✅ Linux Orca can navigate UI and read text  
- ✅ Japanese/Chinese IME works on all platforms
- ✅ Click in text → cursor appears at correct position
- ✅ Type in contenteditable without callback → text updates
- ✅ Cursor stays visible when typing (auto-scroll)
- ✅ Long text auto-creates scroll frame
- ✅ < 16ms frame time maintained

---

## Dependencies to Add

### Cargo.toml updates needed:

```toml
# For Windows accessibility
[target.'cfg(target_os = "windows")'.dependencies]
accesskit = "0.12"
accesskit_windows = "0.16"

# For Linux accessibility  
[target.'cfg(target_os = "linux")'.dependencies]
accesskit = "0.12"
accesskit_unix = "0.7"

# Already have for macOS
[target.'cfg(target_os = "macos")'.dependencies]
accesskit = "0.12"
accesskit_macos = "0.11"
```

---

## Notes

- **IME Complexity**: Different platforms handle IME very differently
  - Windows: Composition window, candidate window
  - Linux: Pre-edit string in-place or separate window
  - macOS: Marked text with replacement range
  
- **Accessibility Differences**:
  - Windows: UI Automation (UIA) via accesskit
  - Linux: AT-SPI via DBus (accesskit_unix)
  - macOS: NSAccessibility (already implemented)

- **Copy/Paste**: Not in this phase, but architecture should support:
  - Clipboard manager abstraction
  - Platform-specific clipboard access
  - Rich text format conversion

---

## Current Status

- ✅ Scroll-into-view implemented (Phase 5.2 partial)
- ✅ scroll_states → scroll_manager renamed
- ✅ Tests written for cursor scroll calculations
- ✅ absolute_positions → calculated_positions (Phase 1 complete)
- ✅ Windows IME integration (Phase 2.1 complete)
- ✅ Windows accessibility (Phase 2.2 complete)
- ✅ Linux IME integration (Phase 3.1 - already implemented via XIM)
- ✅ Linux accessibility (Phase 3.2 complete)
- ✅ A11y tree review (Phase 4 - infrastructure exists and functional)
- ✅ Mouse click → cursor (Phase 5.1 - hit_test_to_cursor() implemented)
- ⏳ Text editing features (Phase 5.2 - needs end-to-end testing)
- ⏳ Auto scroll frame creation (Phase 5.3 - needs implementation)

---

## References

- accesskit documentation: https://docs.rs/accesskit
- Windows UIA: https://docs.microsoft.com/en-us/windows/win32/winauto
- AT-SPI specification: https://www.freedesktop.org/wiki/Accessibility/AT-SPI2/
- IME best practices: https://learn.microsoft.com/en-us/windows/apps/design/input/input-method-editors
