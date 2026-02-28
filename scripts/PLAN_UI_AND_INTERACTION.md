# Plan 3: New UIs and User Interaction Plan

## Goal

Define every new screen, panel, dialog, and interaction flow needed in the
debugger HTML/CSS for the component type system. This plan covers layout,
navigation, and step-by-step user workflows — the "what does the user see
and do" counterpart to the data model (Plan 1) and JS architecture (Plan 2).

---

## Current UI Inventory

### Existing views (3 tabs in ActivityBar)

| Tab | Panel | Content |
|---|---|---|
| **Inspector** | Sidebar: DOM tree | Main: Node detail (CSS overrides) |
| **Testing** | Sidebar: Test list | Main: Test runner + results |
| **Components** | Sidebar: Library list + component list | Main: Component detail (read-only data model, CSS editor, template, placeholder preview) |

### Current Component View layout

```
┌─ MenuBar ─────────────────────────────────────────────────────┐
├─ ActivityBar ─┬─ Sidebar ────────┬─ Main Editor ──────────────┤
│               │                  │                             │
│  [Inspector]  │  Library: [▼]    │  ┌─ Left ──┬─ Right ─────┐ │
│  [Testing]    │                  │  │ Header  │ Template    │ │
│  [Components] │  ┌─────────────┐ │  │ Badges  │ (read-only) │ │
│       ↑       │  │ div         │ │  │         │             │ │
│    active     │  │ span        │ │  │ Data    │ Preview     │ │
│               │  │ button      │ │  │ Model   │ (TODO)      │ │
│               │  │ my-widget   │ │  │ (code)  │             │ │
│               │  └─────────────┘ │  │         │             │ │
│               │                  │  │ Callbk  │             │ │
│               │  [+ Library]     │  │ CSS     │             │ │
│               │  [+ Component]   │  └─────────┴─────────────┘ │
├───────────────┴──────────────────┴─────────────────────────────┤
│  Bottom Panel (collapsed by default)                           │
└────────────────────────────────────────────────────────────────┘
```

### What needs to change

The Component View evolves from a **read-only documentation viewer** into a
**visual component editor** with:
- Interactive, type-aware field editors (not code blocks)
- Live preview with screenshot rendering
- Drag & drop for StyledDom slots
- CSS template editing with autocomplete
- OS/Theme/Language preview switching
- Add/remove field dialogs
- Enum and struct model management

---

## New UI: Component Detail View (Redesigned)

### Layout

The main editor area splits into **two resizable panels** (already supported
by `app.resizer`):

```
┌─ Left Panel (60%) ───────────────────┬─ Right Panel (40%) ────────┐
│                                       │                            │
│  ┌─ Header ────────────────────────┐  │  ┌─ Preview ────────────┐  │
│  │  Avatar                         │  │  │                      │  │
│  │  builtin::avatar                │  │  │  ┌────────────────┐  │  │
│  │  A user avatar with image       │  │  │  │  [screenshot]  │  │  │
│  │  ┌──────┐ ┌────────┐ ┌──────┐   │  │  │  │               │  │  │
│  │  │source│ │children│ │text? │   │  │  │  └────────────────┘  │  │
│  │  └──────┘ └────────┘ └──────┘   │  │  │                      │  │
│  └─────────────────────────────────┘  │  │  OS: [macOS ▼]       │  │
│                                       │  │  Theme: [Light ▼]    │  │
│  ┌─ Data Model ▼ ─────────────────┐  │  │  Lang: [en-US ▼]     │  │
│  │                                 │  │  └──────────────────────┘  │
│  │  AvatarDataModel {              │  │                            │
│  │    ┌─────┐                      │  │  ┌─ Template ▶ ────────┐  │
│  │    │ Str │ alt_text: [User ___] │  │  │  (collapsed)         │  │
│  │    └─────┘                      │  │  └──────────────────────┘  │
│  │    ┌─────┐                      │  │                            │
│  │    │ i32 │ size: [48_________]  │  │  ┌─ Example XML ▶ ─────┐  │
│  │    └─────┘                      │  │  │  (collapsed)         │  │
│  │    ┌─────┐                      │  │  └──────────────────────┘  │
│  │    │ ■   │ bg_color: [#ccc___]  │  │                            │
│  │    └─────┘                      │  │                            │
│  │    ┌──────┐                     │  │                            │
│  │    │◻ Slot│ dom: builtin.a ──┐  │  │                            │
│  │    └──────┘   href: [____]   │  │  │                            │
│  │               color: system  │  │  │                            │
│  │    ┌──────┐                  │  │  │                            │
│  │    │ fn() │ on_load:         │  │  │                            │
│  │    └──────┘ <navigate>       │  │  │                            │
│  │                              │  │  │                            │
│  │  [+ Add Field]               │  │  │                            │
│  │                              │  │  │                            │
│  └──────────────────────────────┘  │  │                            │
│                                    │  │                            │
│  ┌─ Scoped CSS ▼ ─────────────┐   │  │                            │
│  │  .avatar-container {        │   │  │                            │
│  │    width: {size}px;         │   │  │                            │
│  │    background: {bg_color};  │   │  │                            │
│  │  }                          │   │  │                            │
│  │                             │   │  │                            │
│  │  [Save CSS]                 │   │  │                            │
│  └─────────────────────────────┘   │  │                            │
└────────────────────────────────────┴──┴────────────────────────────┘
```

### Section breakdown

#### S1: Component Header

- **Display name** (h3, editable for `user_defined`)
- **Qualified name** (muted, read-only: `library::tag`)
- **Description** (paragraph, editable for `user_defined`)
- **Badges**: source (`builtin` / `compiled` / `user_defined`),
  child policy (`no_children` / `any_children` / `text_only`),
  accepts text (yes/no)

**Interaction**:
- Click display name → inline edit (if `user_defined`) → blur → auto-save
- Click description → inline edit → blur → auto-save

#### S2: Data Model Editor

A `<details open>` section showing all data model fields with type-aware
inline editors (see Plan 2, `DataModelEditor` widget).

**Read-only mode** (builtin/compiled): fields shown with type badges and
current default values. Not editable, but the user can override values for
preview purposes. Changes are **transient** (reset on navigation).

**Edit mode** (user_defined): fields shown with type badges, editable values,
remove buttons, and an "Add Field" button at the bottom.

**Interaction flows**:

1. **Edit a field value (preview)**: click a field's input → type new value →
   after 150ms debounce → `preview_component` API call → preview screenshot
   updates.

2. **Add a field**: click "+ Add Field" → modal dialog (`AddFieldDialog` widget)
   → fill name, select type, optional default → click "Add" → `update_component`
   API call → component detail re-renders with new field.

3. **Remove a field**: click "×" button on a field → confirmation
   → `update_component` API call → field removed.

4. **Change field type**: not directly supported in v1. Remove and re-add.

#### S3: StyledDom Slot Fields

When a field has type `StyledDom`, it renders as a **drop zone** instead of
a text input:

```
┌──────┐
│◻ Slot│ header: builtin.div ──┐
└──────┘   id: [main-header]   │
           class: [hero]       │
           ───────────────────-┘
```

**Interaction**:

1. **Drag & drop**: user drags a component from the sidebar list → drops onto
   the slot → slot updates to show the new component's name → sub-component
   fields appear nested underneath → preview re-renders.

2. **Clear slot**: click "×" on the slot → resets to empty (or to
   `ComponentDefaultValue` if one exists).

3. **Expand/collapse sub-fields**: click the component name in the slot →
   toggle showing the sub-component's own fields (defaults shown grayed out,
   overrides shown with the actual value).

#### S4: Callback Fields

Callbacks are **read-only** in the preview view:

```
┌──────┐
│ fn() │ on_click: fn(String) → Update
└──────┘ default: <my_crate::handle_click>
```

Shows:
- Callback signature as a code badge
- Default function pointer name (if any) as a muted label
- Not editable (callbacks are Rust code, not editable in the browser)

#### S5: CSS Editor

A `<details>` section (open by default for `user_defined`) with the CSS
template editor (see Plan 2, `CssEditor` widget).

**Interaction**:

1. **Type CSS**: every 150ms after last keystroke → `preview_component` with
   `css_override` → preview updates.
2. **Template autocomplete**: type `{` → popup of data model field names
   (only CSS-compatible types: String, I32, F32, F64, Bool, ColorU) →
   select field → inserts `{field_name}`.
3. **Save**: click "Save CSS" → `update_component` with `scoped_css` →
   component persists the new CSS.
4. **Validation**: CSS parse errors shown inline below the textarea in red.

#### S6: Preview Panel

Right-side panel showing the rendered component.

**Content**:
- Screenshot image (PNG, from `preview_component` API)
- Loading spinner overlay while previewing
- OS/Theme/Language dropdown bar at the bottom

**Interaction**:

1. **Auto-preview**: on component selection, sends `preview_component` with
   default field values → displays screenshot.
2. **OS switch**: change OS dropdown → sends `preview_component` with
   `dynamic_selector_context.os = "windows"` → preview re-renders showing
   Windows-specific `@os()` styles.
3. **Theme switch**: change Theme dropdown → `dynamic_selector_context.theme
   = "dark"` → preview shows dark theme.
4. **Language switch**: change Language dropdown → `dynamic_selector_context.language
   = "de-DE"` → preview shows German locale.
5. **Resize**: the preview panel resizes with the right panel. The screenshot
   is rendered at a fixed DPI and scaled to fit.

---

## New UI: Sidebar Component List (Enhanced)

### Current

Plain text list of component names. Click to show detail.

### New

Each component in the list becomes a **draggable badge** with icon +
display name. Dragging allows dropping into StyledDom slots in the detail view.

```
┌─ Components ─────────────────┐
│                               │
│  Library: [My Widgets ▼]      │
│                               │
│  ┌─ Builtin ────────────────┐ │
│  │  ☐ div                   │ │
│  │  ☐ span                  │ │
│  │  ☐ button                │ │
│  │  ☐ a (Link)              │ │
│  │  ☐ img                   │ │
│  │  ...                     │ │
│  └──────────────────────────┘ │
│                               │
│  ┌─ User Defined ───────────┐ │
│  │  ☐ avatar                │ │
│  │  ☐ user-card             │ │
│  └──────────────────────────┘ │
│                               │
│  [+ Library]  [+ Component]   │
│                               │
└───────────────────────────────┘
```

**Interaction**:
- Each row is `draggable="true"` → can be dropped onto StyledDom slot fields
- Click → shows component detail in main panel
- Right-click → context menu: "Duplicate", "Delete" (if `user_defined`)
- Components grouped by source (builtin vs compiled vs user_defined)
- Search/filter input at the top (filters by display name or tag)

---

## New UI: Add Field Dialog

A modal dialog triggered by "+ Add Field" button.

```
┌─── Add Field ────────────────────────┐
│                                       │
│  Name:  [my_field_name___________]    │
│                                       │
│  Type:  [String              ▼]       │
│         ┌────────────────────────┐    │
│         │ String                 │    │
│         │ Bool                   │    │
│         │ I32                    │    │
│         │ F64                    │    │
│         │ ColorU                 │    │
│         │ StyledDom (slot)       │    │
│         │ Option<...>            │    │
│         │ Vec<...>               │    │
│         │ ── Enums ──            │    │
│         │ enum UserRole          │    │
│         │ ── Structs ──          │    │
│         │ struct UserProfile     │    │
│         └────────────────────────┘    │
│                                       │
│  Default: [________________]  (opt.)  │
│                                       │
│  Required: [ ]                        │
│                                       │
│  Description: [________________]      │
│                                       │
│              [Cancel]  [Add Field]    │
│                                       │
└───────────────────────────────────────┘
```

**Interaction**:

1. User types field name (validated: lowercase, underscores, no spaces).
2. Selects type from dropdown. If `Option<...>` or `Vec<...>` selected,
   a second dropdown appears for the inner type.
3. Optionally types a default value (validated against the selected type).
4. Clicks "Add Field" → API call → dialog closes → detail view refreshes.

**Validation**:
- Name must be unique within the data model
- Name must match `[a-z][a-z0-9_]*`
- If type is `EnumRef`, the enum must exist in the library
- If type is `StructRef`, the struct must exist in the library

---

## New UI: Enum Model Editor

Accessible from a new "Types" sub-tab in the sidebar (alongside the
component list), or via a button in the component detail when a field
references an `EnumRef`.

```
┌─── Enum: UserRole ───────────────────┐
│                                       │
│  Variants:                            │
│  ┌───────────────────────────────┐    │
│  │  Admin                    [×] │    │
│  │  Editor                   [×] │    │
│  │  Viewer                   [×] │    │
│  └───────────────────────────────┘    │
│                                       │
│  [+ Add Variant]                      │
│                                       │
│              [Cancel]  [Save]         │
│                                       │
└───────────────────────────────────────┘
```

**Interaction**:
1. Click "+ Add Variant" → inline text input appears → type name → Enter
2. Click "×" on a variant → removes it (with confirmation if used by components)
3. Click "Save" → `update_enum` (or `create_enum`) API call

---

## New UI: Struct Model Editor

Similar to the enum editor, but for reusable struct types.

```
┌─── Struct: UserProfile ──────────────┐
│                                       │
│  Fields:                              │
│  ┌───────────────────────────────┐    │
│  │  name: String             [×] │    │
│  │  email: Option<String>    [×] │    │
│  │  role: UserRole           [×] │    │
│  └───────────────────────────────┘    │
│                                       │
│  [+ Add Field]                        │
│                                       │
│              [Cancel]  [Save]         │
│                                       │
└───────────────────────────────────────┘
```

Uses the same `FieldEditor` widget for type selection per field.

---

## New UI: Sidebar "Types" Sub-Panel

The sidebar in the Components view gets a secondary panel (below the
component list) showing the library's custom types:

```
┌─ Components ────────────────┐
│  Library: [My Widgets ▼]    │
│                              │
│  ┌─ Components ─────────┐   │
│  │  avatar               │   │
│  │  user-card            │   │
│  └───────────────────────┘   │
│                              │
│  ┌─ Types ──────────────┐   │
│  │                       │   │
│  │  Enums:               │   │
│  │    UserRole            │   │
│  │    ThemeMode           │   │
│  │                       │   │
│  │  Structs:             │   │
│  │    UserProfile         │   │
│  │    ThemeConfig         │   │
│  │                       │   │
│  │  [+ Enum] [+ Struct]  │   │
│  └───────────────────────┘   │
│                              │
│  [+ Library] [+ Component]  │
│                              │
└──────────────────────────────┘
```

Click an enum/struct name → opens the enum/struct editor in the main panel.

---

## New UI: Create Component Dialog

Triggered by the "+ Component" button.

```
┌─── Create Component ─────────────────┐
│                                       │
│  Tag name:     [my-widget________]    │
│  Display name: [My Widget________]    │
│                                       │
│  Child policy: [Any children  ▼]      │
│  Accepts text: [ ]                    │
│                                       │
│              [Cancel]  [Create]       │
│                                       │
└───────────────────────────────────────┘
```

After creation → component appears in the sidebar list, component detail
opens with empty data model + empty template + empty CSS.

---

## New UI: Create Library Dialog

```
┌─── Create Library ───────────────────┐
│                                       │
│  Name:        [my-widgets________]    │
│  Version:     [1.0.0_____________]    │
│  Description: [__________________ ]   │
│                                       │
│              [Cancel]  [Create]       │
│                                       │
└───────────────────────────────────────┘
```

After creation → library appears in the dropdown, gets auto-selected.

---

## New UI: Export / Import Panel

Accessible from the MenuBar or a toolbar button.

```
┌─── Export Library ───────────────────┐
│                                       │
│  Library: [My Widgets ▼]             │
│                                       │
│  Format:                              │
│  (•) JSON                             │
│  ( ) Rust code                        │
│  ( ) C code                           │
│  ( ) Python code                      │
│                                       │
│  ┌─ Preview ──────────────────────┐   │
│  │  {                             │   │
│  │    "name": "my-widgets",       │   │
│  │    "version": "1.0.0",         │   │
│  │    "components": [ ... ]       │   │
│  │  }                             │   │
│  └────────────────────────────────┘   │
│                                       │
│  [Copy to Clipboard]  [Download]      │
│                                       │
└───────────────────────────────────────┘
```

**Import**: file picker or paste JSON → preview → confirm → library loaded.

---

## User Interaction Flows

### Flow 1: Create a new component from scratch

```
1. User clicks [+ Component] button in sidebar
2. → Create Component dialog opens
3. User fills tag name + display name, selects child policy
4. → Clicks [Create]
5. → API: create_component → empty component created
6. → Component appears in sidebar, detail view opens (empty)
7. User clicks [+ Add Field] in the Data Model section
8. → Add Field dialog opens
9. User types field name, selects type (e.g. String), adds default
10. → Clicks [Add]
11. → API: update_component → field added
12. → Detail view refreshes, field appears with text input
13. User types in the CSS editor (e.g. .root { color: {my_field}; })
14. → Live preview: CSS template expanded → screenshot rendered
15. → Preview updates in right panel
16. User edits the template (textarea, XML)
17. → No live preview on template edit (template change requires save + re-render)
18. User clicks [Save CSS]
19. → API: update_component → CSS persisted
```

### Flow 2: Browse and preview a builtin component

```
1. User selects "builtin" library from dropdown
2. → API: get_library_components → sidebar populates with 52+ builtins
3. User clicks "button" in sidebar
4. → showComponentDetail() renders:
     - Header: "Button", builtin::button
     - Data Model (read-only): text (String), on_click (Callback)
     - CSS (read-only): <pre> with builtin CSS
     - Template (read-only): <pre> with builtin template
5. → Auto-preview: preview_component API → screenshot appears
6. User changes "text" field value for preview purposes
   (transient — doesn't modify the builtin def)
7. → Preview updates showing button with new text
8. User switches OS dropdown to "Windows"
9. → Preview re-renders with Windows @os() styles
10. User switches Theme to "Dark"
11. → Preview re-renders with dark theme
```

### Flow 3: Drag a component into a StyledDom slot

```
1. User is editing "user-card" component (user_defined)
2. Data model shows: avatar_slot (StyledDom) → currently empty "Drop component here"
3. User sees "avatar" component in sidebar list
4. User drags "avatar" from sidebar
5. → dragstart: dataTransfer = { library: "mylib", component: "avatar" }
6. User drops onto avatar_slot drop zone
7. → drop: slot updates to show "mylib.avatar"
8. → Sub-fields of avatar appear nested underneath:
     alt_text: [User avatar]
     size: [48]
     bg_color: [#ccc]
9. → preview_component API with avatar default values → preview renders
10. User overrides avatar's size field to 64
11. → Preview re-renders showing larger avatar
```

### Flow 4: Edit CSS with template expressions

```
1. User selects user_defined component with data model:
   size (I32, default: 48), bg_color (String, default: "#ccc")
2. CSS editor shows existing scoped CSS
3. User types: .container { width: {
4. → Autocomplete popup appears: size (i32), bg_color (Str)
5. User selects "size" → inserts {size}
6. User continues typing: }px;
7. CSS is now: .container { width: {size}px; }
8. → After 150ms debounce, preview_component fires
9. → CSS expanded: .container { width: 48px; } (using default)
10. → Screenshot rendered → preview updates
11. User changes size field value to 72
12. → CSS re-expanded: .container { width: 72px; }
13. → Preview updates
```

### Flow 5: Create and use a custom enum

```
1. User clicks [+ Enum] in sidebar Types section
2. → Enum editor opens in main panel
3. User types enum name: "ButtonVariant"
4. User clicks [+ Add Variant] → types "Primary" → Enter
5. User clicks [+ Add Variant] → types "Secondary" → Enter
6. User clicks [+ Add Variant] → types "Danger" → Enter
7. User clicks [Save] → API: create_enum → enum persisted
8. User navigates to their "my-button" component
9. User clicks [+ Add Field]
10. → Add Field dialog opens
11. User types name: "variant"
12. User selects type: "enum ButtonVariant"
13. User sets default: "Primary"
14. → Clicks [Add]
15. → Field appears in data model as a <select> dropdown:
     variant: [Primary ▼]  (options: Primary, Secondary, Danger)
16. User selects "Danger" → preview re-renders with Danger variant CSS
```

### Flow 6: Export a component library as Rust code

```
1. User clicks "Export" in MenuBar
2. → Export panel opens
3. User selects library "my-widgets" from dropdown
4. User selects format "Rust code"
5. → API: export_code → returns generated Rust structs + render fns
6. → Preview area shows generated code
7. User clicks [Download] → browser downloads .rs file
8. User adds the file to their Rust project, implements callbacks
9. User recompiles → component is now "compiled" with native render_fn
```

### Flow 7: Preview OS-specific styling

```
1. User has a component with scoped CSS containing:
   @os(windows) { .btn { font-family: "Segoe UI"; border-radius: 4px; } }
   @os(macos) { .btn { font-family: "Helvetica"; border-radius: 8px; } }
2. Preview initially shows macOS style (user's current OS)
3. User switches OS dropdown to "Windows"
4. → preview_component with dynamic_selector_context.os = "windows"
5. → DynamicSelectorContext override applied:
     @os(windows) block matches → Segoe UI + 4px radius
     @os(macos) block doesn't match → skipped
6. → Screenshot shows Windows-style button
7. User switches back to "macOS"
8. → @os(macos) matches → Helvetica + 8px radius
9. → Screenshot shows macOS-style button
```

---

## CSS Changes (debugger.css)

### New CSS class inventory

All classes prefixed with `azd-` (azul-debugger):

#### Field Editor

```css
.azd-field-row          { display: flex; align-items: center; gap: 8px; padding: 4px 0; }
.azd-field-label        { min-width: 120px; font-family: monospace; font-size: 12px; }
.azd-field-label.azd-required::after { content: '*'; color: var(--error-color); }
.azd-type-badge         { font-size: 10px; padding: 1px 6px; border-radius: 3px;
                          font-family: monospace; background: var(--badge-bg); }
.azd-type-string        { color: var(--string-color); }
.azd-type-bool          { color: var(--bool-color); }
.azd-type-i32, .azd-type-f64 { color: var(--number-color); }
.azd-type-coloru        { /* inline color swatch */ }
.azd-type-styleddom     { color: var(--slot-color); font-weight: bold; }
.azd-type-callback      { color: var(--fn-color); font-style: italic; }
.azd-type-enumref       { color: var(--enum-color); }
```

#### Input Controls

```css
.azd-input              { background: var(--input-bg); border: 1px solid var(--border-color);
                          color: var(--foreground); font-size: 12px; padding: 3px 6px; }
.azd-input:focus        { border-color: var(--focus-color); outline: none; }
.azd-input-string       { width: 200px; }
.azd-input-bool         { cursor: pointer; }
.azd-input-color        { display: flex; align-items: center; gap: 4px; }
.azd-input-enum         { width: 160px; }
```

#### Slot Drop Zone

```css
.azd-input-slot         { border: 2px dashed var(--border-color); border-radius: 4px;
                          padding: 8px 12px; text-align: center; min-height: 32px;
                          transition: border-color 0.15s, background 0.15s; }
.azd-slot-empty         { color: var(--muted-color); }
.azd-slot-filled        { border-style: solid; font-family: monospace; text-align: left; }
.azd-slot-hover         { border-color: var(--focus-color);
                          background: rgba(var(--focus-color-rgb), 0.1); }
```

#### CSS Editor

```css
.azd-css-editor         { display: flex; flex-direction: column; gap: 4px; }
.azd-css-textarea       { min-height: 120px; font-family: monospace; font-size: 12px;
                          background: var(--editor-bg); color: var(--foreground);
                          border: 1px solid var(--border-color); padding: 8px;
                          resize: vertical; tab-size: 4; }
.azd-css-errors         { color: var(--error-color); font-size: 11px; }
.azd-css-error          { padding: 2px 0; }
.azd-css-error::before  { content: '⚠ '; }
```

#### Preview Panel

```css
.azd-preview-panel      { display: flex; flex-direction: column; gap: 8px; }
.azd-preview-img        { max-width: 100%; border: 1px solid var(--border-color);
                          background: var(--editor-bg); min-height: 100px; }
.azd-preview-bar        { display: flex; gap: 12px; align-items: center;
                          font-size: 12px; }
.azd-preview-dropdown   { display: flex; align-items: center; gap: 4px; }
.azd-preview-dropdown-label { color: var(--muted-color); }
.azd-preview-dropdown-select { background: var(--input-bg); color: var(--foreground);
                               border: 1px solid var(--border-color); font-size: 11px; }
.azd-loading            { position: relative; }
.azd-loading::after     { content: ''; position: absolute; inset: 0;
                          background: rgba(0,0,0,0.3); display: flex;
                          align-items: center; justify-content: center; }
```

#### Value Source Toggle

```css
.azd-source-toggle      { display: flex; border: 1px solid var(--border-color);
                          border-radius: 3px; overflow: hidden; }
.azd-source-btn         { padding: 2px 6px; font-size: 10px; border: none;
                          background: transparent; color: var(--muted-color);
                          cursor: pointer; }
.azd-source-btn.azd-active { background: var(--focus-color); color: white; }
```

#### Binding Input

```css
.azd-binding-input      { position: relative; }
.azd-input-binding      { width: 250px; font-family: monospace; }
.azd-binding-suggestions { position: absolute; top: 100%; left: 0; z-index: 100;
                           background: var(--dropdown-bg); border: 1px solid var(--border-color);
                           max-height: 200px; overflow-y: auto; list-style: none;
                           padding: 0; margin: 0; width: 100%; }
.azd-suggestion         { padding: 4px 8px; cursor: pointer; display: flex;
                          justify-content: space-between; font-size: 12px; }
.azd-suggestion:hover   { background: var(--focus-color); color: white; }
```

#### Dialog

```css
.azd-dialog-overlay     { position: fixed; inset: 0; background: rgba(0,0,0,0.5);
                          display: flex; align-items: center; justify-content: center;
                          z-index: 1000; }
.azd-dialog             { background: var(--sidebar-bg); border: 1px solid var(--border-color);
                          border-radius: 6px; padding: 20px; min-width: 400px;
                          max-width: 500px; }
.azd-dialog h3          { margin: 0 0 16px 0; }
.azd-dialog-field       { margin-bottom: 12px; }
.azd-dialog-field label { display: block; margin-bottom: 4px; font-size: 12px;
                          color: var(--muted-color); }
.azd-dialog-buttons     { display: flex; justify-content: flex-end; gap: 8px;
                          margin-top: 20px; }
```

#### Buttons

```css
.azd-btn                { padding: 6px 16px; border: 1px solid var(--border-color);
                          background: var(--input-bg); color: var(--foreground);
                          cursor: pointer; border-radius: 3px; font-size: 12px; }
.azd-btn:hover          { background: var(--focus-color); color: white; }
.azd-btn-primary        { background: var(--focus-color); color: white;
                          border-color: var(--focus-color); }
.azd-btn-small          { padding: 2px 8px; font-size: 11px; }
.azd-btn-icon           { padding: 2px 6px; border: none; background: transparent;
                          color: var(--muted-color); cursor: pointer; font-size: 14px; }
.azd-btn-icon:hover     { color: var(--foreground); }
.azd-btn-danger:hover   { color: var(--error-color); }
```

#### Data Model Editor

```css
.azd-data-model-editor  { padding: 8px 0; }
.azd-dm-header          { font-family: monospace; font-size: 13px; font-weight: bold;
                          padding: 4px 0; border-bottom: 1px solid var(--border-color);
                          margin-bottom: 8px; }
.azd-dm-field-row       { display: flex; align-items: flex-start; gap: 4px;
                          padding: 2px 0; }
```

#### Component Drag Handle

```css
.azd-component-drag     { padding: 4px 8px; cursor: grab; font-size: 12px;
                          border-radius: 3px; }
.azd-component-drag:hover { background: var(--list-hover-bg); }
.azd-component-drag:active { cursor: grabbing; opacity: 0.7; }
```

### New CSS custom properties

Add to the `:root` block in debugger.css:

```css
:root {
    /* ... existing properties ... */

    /* Type colors */
    --string-color: #ce9178;
    --bool-color: #569cd6;
    --number-color: #b5cea8;
    --slot-color: #c586c0;
    --fn-color: #dcdcaa;
    --enum-color: #4ec9b0;

    /* UI colors */
    --badge-bg: rgba(255, 255, 255, 0.06);
    --error-color: #f14c4c;
    --focus-color-rgb: 0, 122, 204;
    --dropdown-bg: var(--sidebar-bg);
    --list-hover-bg: rgba(255, 255, 255, 0.05);
}
```

---

## HTML Changes (debugger.html)

### Minimal changes

The HTML structure stays mostly the same. The main change is ensuring the
component detail area has proper containers for the widget-based rendering:

```html
<!-- Replace current component detail divs -->
<div id="component-detail-left" class="azd-detail-left"></div>
<div id="component-detail-right" class="azd-detail-right"></div>
```

No other HTML changes needed — all new UI is built dynamically via JS widgets.

---

## Accessibility Considerations

- All interactive controls have `aria-label` or visible `<label>` elements
- Dropdown selects use native `<select>` (keyboard navigable)
- Dialogs trap focus (Tab cycles within dialog, Escape closes)
- Color inputs have hex text fallback (not color-only)
- Drop zones announce state via `aria-live="polite"`
- Keyboard shortcuts: Escape closes dialogs, Enter confirms in single-field dialogs

---

## Responsive Behavior

The debugger runs in a browser window (typically 1200px+). No mobile support
needed, but the resizable panels (via `app.resizer`) handle different window
sizes gracefully:

- Below 900px width: right panel (preview) stacks below the left panel
- Below 600px width: sidebar collapses to icons only
- The CSS editor textarea is resizable via native drag handle

---

## Implementation Priority

| Priority | UI | Depends On |
|---|---|---|
| P0 | Data model field editors (interactive) | Plan 1 Phase 2, Plan 2 W1-W3 |
| P0 | Preview panel with screenshot | Plan 1 Phase 6 |
| P1 | CSS editor with template autocomplete | Plan 2 W6 |
| P1 | OS/Theme/Language preview switcher | Plan 1 Phase 6 |
| P1 | Add Field dialog | Plan 2 W9 |
| P2 | StyledDom slot drag & drop | Plan 2 W4, W10 |
| P2 | Create Component / Library dialogs | Existing API (minor updates) |
| P2 | Sidebar Types panel (enum/struct list) | Plan 1 Phase 8 |
| P3 | Enum / Struct model editors | Plan 1 Phase 8 |
| P3 | Value Source Toggle + Binding Input | Plan 2 W4, W5 |
| P3 | Export / Import panel | Existing API (no changes) |
