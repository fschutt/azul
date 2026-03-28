# Session 8 Plan — IME Integration + Accessibility Audit

**Date**: 2026-03-28
**Branch**: `layout-debug-clean`
**Status**: Investigation complete, implementation plan ready

---

## 1. IME (Input Method Editor) Status per Platform

### 1.1 macOS — `NSTextInputClient` ✅ Mostly Implemented

**What works:**
- `GLView` and `CPUView` both implement `NSTextInputClient` protocol
- `insertText:replacementRange:` receives composed text after IME confirmation
- `setMarkedText:selectedRange:replacementRange:` handles preedit/composition display
- `hasMarkedText`, `markedRange`, `selectedRange`, `unmarkText` implemented
- `interpretKeyEvents:` called on keyDown → routes through macOS text input system
- `firstRectForCharacterRange:actualRange:` positions the IME candidate window

**What's missing:**
- `firstRectForCharacterRange` returns approximate position from `ime_position` field
  but doesn't read the actual cursor rect from the text layout (uses window-level
  `ImePosition::Initialized(rect)` which is updated after layout via
  `update_ime_position_from_cursor()` → `get_focused_cursor_rect_viewport()`)
- Preedit text (composition string in `setMarkedText`) is stored but not visually
  rendered inline in the contenteditable area — it only appears in the floating
  macOS IME window. For proper CJK input, the composition text should be shown
  inline with an underline in the text field.

**To test:**
1. Add Japanese input source: System Settings → Keyboard → Input Sources → + → Japanese
2. Switch: Ctrl+Space or Globe key
3. Run `./tests/e2e/contenteditable_test`
4. Click the contenteditable region
5. Type "nihongo" → should show にほんご composition → Space to convert → Enter to confirm

### 1.2 Windows — `WM_IME_*` + `ImmSetCompositionWindow` ✅ Mostly Implemented

**What works:**
- `WM_IME_STARTCOMPOSITION`: calls `sync_ime_position_to_os()` to set candidate window position
- `WM_IME_COMPOSITION`: reads composition string via `ImmGetCompositionStringW` with
  `GCS_COMPSTR` (preedit) and `GCS_RESULTSTR` (confirmed)
- `WM_CHAR` / `WM_IME_CHAR`: receives confirmed characters as UTF-16
- `ImmSetCompositionWindow`: positions candidate window at cursor location
- High surrogate pair handling for UTF-16 supplementary plane characters
- `ime_composition` field stores active composition string

**What's missing:**
- Same as macOS: preedit text not rendered inline in contenteditable
- `ime_composition` string is stored but no visual indicator (underline/highlight)
  is drawn in the text field during composition
- After text input, `sync_ime_position_to_os()` is called to update position

### 1.3 X11 — XIM/XIC ✅ Mostly Implemented

**What works:**
- `ImeManager` struct manages XIM (X Input Method) + XIC (X Input Context)
- `XOpenIM` / `XCreateIC` initialization with `XIMPreeditNothing | XIMStatusNothing` style
- `XFilterEvent` called before event dispatch to let XIM handle key events
- `Xutf8LookupString` for proper UTF-8 character lookup (with IME)
- `XSetICFocus` / `XUnsetICFocus` called on FocusIn/FocusOut events
- GTK IM integration via `Gtk3Im` for IBus/Fcitx support

**What's missing:**
- Preedit callback support: using `XIMPreeditNothing` (system renders preedit)
  instead of `XIMPreeditCallbacks` (app renders preedit inline)
- Cursor rectangle not communicated to XIM for candidate window positioning
  (would need `XIMPreeditPosition` style + spot location)
- Same inline preedit gap as other platforms

### 1.4 Wayland — `zwp_text_input_v3` ⚠️ Partially Implemented

**What works:**
- `zwp_text_input_manager_v3` and `zwp_text_input_v3` types defined
- `text_input_manager` and `text_input` fields on WaylandWindow
- XKB keymap-based keyboard handling (correct for non-IME input)

**What's missing:**
- `zwp_text_input_v3` protocol NOT wired up:
  - `enable` / `disable` not called on focus changes
  - `set_surrounding_text` not called (IME needs context around cursor)
  - `set_cursor_rectangle` not called (candidate window positioning)
  - `set_content_type` not called (tells IME this is text input)
  - `commit` not called after setting state
  - `commit_string` / `preedit_string` / `done` event listeners not registered
- This means Wayland IME (IBus, Fcitx5) will NOT work for Japanese input

---

## 2. IME → ContentEditable Integration Gap

The critical missing piece across ALL platforms:

**Problem**: When a contenteditable region gains focus, the OS needs to be told
"this is a text input area, enable IME here." Currently:

1. Focus change sets `focus_manager.focused_node` ✅
2. `handle_focus_change_for_cursor_blink()` starts cursor blink ✅
3. `apply_focus_restyle()` applies `:focus` CSS ✅
4. `scroll_node_into_view()` scrolls to focused element ✅
5. **MISSING**: Tell the OS "IME should be active at this text input location"

**What needs to happen on focus→contenteditable:**

| Platform | Action needed |
|----------|--------------|
| macOS | `inputContext.activate()` (already automatic via NSTextInputClient) |
| Windows | `ImmAssociateContextEx(hwnd, 0, IACE_DEFAULT)` to enable IME |
| X11 | `XSetICFocus(xic)` (already called on FocusIn) |
| Wayland | `zwp_text_input_v3_enable()` + `set_content_type(TEXT_INPUT)` + `commit()` |

macOS and X11 are mostly fine — IME is always active when the window has focus.
Windows may need explicit `ImmAssociateContextEx` if IME was previously disabled.
Wayland is the biggest gap — the text-input protocol must be actively enabled.

---

## 3. Inline Preedit Rendering (All Platforms)

Currently, composition text (preedit) is shown in the OS's floating candidate window.
For proper CJK input UX, the composition should be rendered INLINE in the text:

```
Before composition:  Hello|
During composition:  Hello にほんご|   (underlined, unconfirmed)
After confirmation:  Hello 日本語|     (confirmed, no underline)
```

**Implementation plan:**
1. Add `preedit_text: Option<String>` and `preedit_cursor: Option<usize>` to `CursorManager`
2. In `setMarkedText` (macOS) / `WM_IME_COMPOSITION GCS_COMPSTR` (Windows) /
   XIM preedit callback (X11) / `preedit_string` event (Wayland):
   - Store the preedit string in `cursor_manager.preedit_text`
3. In `paint_cursor()` / new `paint_preedit()`:
   - If `preedit_text.is_some()`, render the composition text with underline decoration
4. In `insertText` (macOS) / `GCS_RESULTSTR` (Windows) / commit events:
   - Clear `preedit_text`
   - Insert the confirmed text via `record_text_input()`

---

## 4. Accessibility Audit

### 4.1 Framework: AccessKit (all platforms)

Azul uses [AccessKit](https://github.com/AccessKit/accesskit) which provides:
- **macOS**: NSAccessibility via `accesskit_macos::SubclassingAdapter`
- **Windows**: UI Automation (UIA) via `accesskit_windows::SubclassingAdapter`
- **Linux (X11)**: AT-SPI via `accesskit_unix::Adapter` (gated behind `a11y` feature)
- **Wayland**: No separate adapter (uses AT-SPI like X11)

### 4.2 A11y Tree Construction

The a11y tree is built from the DOM/layout tree. Key code:
- `layout/src/window.rs:706`: `a11y_manager` field on LayoutWindow
- `layout/src/window.rs:4990-5035`: `process_accessibility_action()` handles a11y events
- `layout/src/window.rs:5326`: `edit_text_node()` for a11y-driven text editing
- Each platform's `accessibility.rs` bridges AccessKit ↔ platform API

### 4.3 Widget Role Mapping

| Widget | Expected A11y Role | Current Status |
|--------|-------------------|----------------|
| `<div>` | `Role::GenericContainer` | ⚠️ Need to verify |
| `<button>` / OnClick | `Role::Button` | ⚠️ Need to verify |
| `<p>` / Text | `Role::StaticText` | ⚠️ Need to verify |
| `contenteditable` | `Role::TextInput` or `Role::MultilineTextInput` | ⚠️ Need to verify |
| `<input type="text">` | `Role::TextInput` | ⚠️ Need to verify |
| `<label>` | `Role::Label` | ⚠️ Need to verify |
| `<img>` / Image | `Role::Image` | ⚠️ Need to verify |
| Checkbox | `Role::CheckBox` | ⚠️ Need to verify |
| Scrollable container | `Role::ScrollView` | ⚠️ Need to verify |

### 4.4 ContentEditable + A11y Gap Analysis

For a blind user to type Japanese in a contenteditable:

1. **Tab navigation to contenteditable** → Focus system already handles Tab
   - `TabIndex::Auto` on contenteditable enables tab-focus ✅
   - Focus change fires a11y focus event ✅ (via `update_tree()`)

2. **Screen reader announces "editable text"** → Needs correct Role
   - ContentEditable nodes need `Role::TextInput` or `Role::MultilineTextInput`
   - The `value` property must contain the current text
   - **VERIFY**: Does the a11y tree builder check `is_contenteditable()` and
     set the correct role?

3. **IME activation** → Platform-specific (see §2)
   - macOS: VoiceOver + IME works if NSTextInputClient is correct ✅
   - Windows: NVDA/JAWS + IME works if UIA role is `Edit` ✅
   - Linux: Orca + IBus works if AT-SPI role is correct ⚠️

4. **Text input feedback** → A11y tree must be updated after text changes
   - After `apply_text_changeset()`, the a11y tree node's `value` must update
   - **VERIFY**: Is `a11y_dirty` set after text changes?
   - The `text_selection` property must reflect cursor position for
     screen reader cursor tracking

5. **Selection announcement** → Selection changes should be reported
   - A11y `text_selection` property tracks cursor/selection for screen readers

---

## 5. Implementation Priority

### P1: Wayland `zwp_text_input_v3` Wiring (HIGH — blocks Wayland IME entirely)

1. In `WaylandWindow::new()`, bind `zwp_text_input_manager_v3` from registry
2. Create `zwp_text_input_v3` instance via `get_text_input(seat)`
3. Register event listeners: `enter`, `leave`, `preedit_string`, `commit_string`, `done`
4. On contenteditable focus: call `enable()`, `set_content_type(TEXT)`, `set_cursor_rectangle()`, `commit()`
5. On contenteditable blur: call `disable()`, `commit()`
6. In `preedit_string` handler: store in `cursor_manager.preedit_text`
7. In `commit_string` handler: call `record_text_input()`

### P2: Inline Preedit Rendering (MEDIUM — improves CJK UX on all platforms)

1. Add `preedit_text: Option<String>` to `CursorManager`
2. macOS `setMarkedText` → store preedit text
3. Windows `WM_IME_COMPOSITION GCS_COMPSTR` → store preedit text
4. X11: Switch to `XIMPreeditCallbacks` or `XIMPreeditPosition` for spot location
5. `paint_cursor()` / new `paint_preedit()` → render preedit text with underline
6. On `insertText` / `GCS_RESULTSTR` / `commit_string` → clear preedit, insert text

### P3: A11y Role Verification (MEDIUM — required for screen reader users)

1. Verify `contenteditable` nodes get `Role::TextInput` / `Role::MultilineTextInput`
2. Verify `value` property contains current text content
3. Verify `text_selection` property tracks cursor position
4. Verify `a11y_dirty` is set after text changes → tree update sent
5. Test with VoiceOver (macOS) and NVDA (Windows)

### P4: IME Position Accuracy (LOW — cosmetic improvement)

1. macOS `firstRectForCharacterRange`: use actual glyph position from layout
2. Windows `ImmSetCompositionWindow`: verify cursor rect is in screen coordinates
3. X11: Set XIM spot location for candidate window positioning
4. Wayland: `set_cursor_rectangle` in buffer coordinates

### P5: A11y Action Handling (LOW — for assistive technology control)

1. Handle `Action::SetTextSelection` → move cursor
2. Handle `Action::ReplaceSelectedText` → insert text
3. Handle `Action::SetValue` → replace all text
4. Test with AT tools: VoiceOver (macOS), NVDA (Windows), Orca (Linux)

---

## 6. Test Plan

### Manual Testing

| Test | macOS | Windows | X11 | Wayland |
|------|-------|---------|-----|---------|
| ASCII typing in contenteditable | `./contenteditable_test` | Same | Same | Same |
| Japanese IME (romaji→kana→kanji) | Ctrl+Space → type "nihongo" | Alt+~ | IBus | IBus/Fcitx5 |
| Candidate window position | Near cursor? | Near cursor? | Near cursor? | Near cursor? |
| Preedit text shown inline | ❌ (floating) | ❌ (floating) | ❌ (floating) | ❌ (no IME) |
| VoiceOver/NVDA announces role | Check | Check | N/A | N/A |
| Tab to contenteditable | Focus ring? | Focus ring? | Focus ring? | Focus ring? |

### Automated Tests

- `contenteditable_e2e.rs`: Already tests cursor rendering, text input, damage ✅
- Add: `test_preedit_text_stored()` — verify preedit is captured
- Add: `test_a11y_role_contenteditable()` — verify a11y Role assignment

---

## 7. Architecture: IME Data Flow

```
┌────────────────────────────────────────────────────────────┐
│  OS IME (macOS/Windows/X11/Wayland)                        │
│                                                            │
│  User types: "n-i-h-o-n-g-o" → にほんご → [Space] → 日本語│
└─────────┬──────────────────────────┬───────────────────────┘
          │ setMarkedText /          │ insertText /
          │ WM_IME_COMPOSITION /     │ GCS_RESULTSTR /
          │ preedit_string           │ commit_string
          ▼                          ▼
┌─────────────────────┐    ┌───────────────────────────┐
│  CursorManager      │    │  record_text_input()      │
│  .preedit_text =    │    │  → TextInputManager       │
│   "にほんご"          │    │  → apply_text_changeset() │
│  (intermediate)     │    │  → update display list    │
└─────────┬───────────┘    └───────────────────────────┘
          │
          ▼
┌─────────────────────┐
│  paint_preedit()    │
│  → render underlined│
│    composition text │
└─────────────────────┘
```
