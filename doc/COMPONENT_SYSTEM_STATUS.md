# Component System — Status & Updated Requirements

**Date:** 2025-02-21
**Baseline:** `doc/COMPONENT_SYSTEM_REPORT.md` (v2)

---

## 1. Progress Against Original Report

### Phase 1: `AzComponentDef` struct — DONE

| Item | Status | Notes |
|------|--------|-------|
| `ComponentDef` repr(C) struct | ✅ Done | `core/src/xml.rs:1327` — 15 fields including `render_fn`, `compile_fn`, `node_type` |
| `ComponentId` (collection:name) | ✅ Done | `core/src/xml.rs:1127` — `collection` + `name` fields, `qualified_name()`, `builtin()`, `new()` |
| `ComponentParam` | ✅ Done | `core/src/xml.rs:1160` — name, param_type, default_value, description |
| `ComponentCallbackSlot` | ✅ Done | `core/src/xml.rs:1180` — name, callback_type, description |
| `ComponentDataField` | ✅ Done | `core/src/xml.rs:1200` — name, field_type, default_value, description |
| `ChildPolicy` enum | ✅ Done | `NoChildren`, `AnyChildren`, `TextOnly` (no `Specific(StringVec)` variant yet) |
| `ComponentSource` enum | ✅ Done | `Builtin`, `Compiled`, `UserDefined` |
| `CompileTarget` enum | ✅ Done | `Rust`, `C`, `Cpp`, `Python` |
| `ComponentRenderFn` / `ComponentCompileFn` type aliases | ✅ Done | Function pointer types in xml.rs |
| `RegisterComponentFn` / `RegisterComponentLibraryFn` | ✅ Done | repr(C) callback structs with `cb` + `ctx` for FFI |
| `ComponentLibrary` | ✅ Done | name, version, description, components, exportable |
| `ComponentMap` with qualified lookup | ✅ Done | `get(collection, name)`, `get_unqualified()`, `get_by_qualified_name()`, `get_exportable_libraries()` |
| `impl_vec!` / `impl_option!` for all types | ✅ Done | Full FFI-compatible vector/option types |
| 52 builtin components registered via `register_builtin_components()` | ✅ Done | `core/src/xml.rs:1623` — using `builtin_component_def()` helper |
| `builtin_render_fn` / `builtin_compile_fn` | ✅ Done | NodeType-based rendering + multi-language codegen |
| `user_defined_render_fn` / `user_defined_compile_fn` | ✅ Done | Placeholder for JSON-imported components |
| Extend `split_dynamic_string` for format specifiers | ❌ Not started | `{var:?}`, `{var:.2}` etc. not yet parsed |
| `ChildPolicy::Specific(StringVec)` | ❌ Not started | `ul -> ["li"]`, `table -> ["thead","tbody","tr"]` |

### Phase 2: JSON component definitions — PARTIALLY DONE

| Item | Status | Notes |
|------|--------|-------|
| `ExportedLibraryResponse` with serde Serialize+Deserialize | ✅ Done | `debug_server.rs:373` — JSON-serializable library format |
| `ExportedComponentDef` / `ExportedComponentParam` / `ExportedDataField` / `ExportedCallbackSlot` | ✅ Done | Full JSON round-trip types |
| `ImportComponentLibrary` debug API endpoint | ✅ Done | JSON → ComponentDef conversion, inserts into ComponentMap |
| `ExportComponentLibrary` debug API endpoint | ✅ Done | ComponentDef → JSON export for user-defined libraries |
| `get_component_registry` / `get_libraries` / `get_library_components` endpoints | ✅ Done | Full REST-style API |
| Template-based generic `render_fn` for JSON components | ❌ Stub only | `user_defined_render_fn` just creates a div — doesn't expand XML template |
| Template-based generic `compile_fn` for JSON components | ❌ Stub only | `user_defined_compile_fn` just creates `Dom::div()` — doesn't expand template |

### Phase 3: Debugger UI — PARTIALLY DONE

| Item | Status | Notes |
|------|--------|-------|
| Component sidebar: library list | ✅ Done | Shows libraries with counts, selectable |
| Component sidebar: component list per library | ✅ Done | Shows display_name + tag |
| Component detail panel: params, data model, callbacks, CSS, example XML | ✅ Done | Full detail rendering in `showComponentDetail()` |
| Import Component Library menu item | ✅ Done | Import > Component Library... (file picker) |
| Export Component Library menu item | ✅ Done | Export > Component Library (JSON) |
| Export Code (Rust/C/C++/Python) menu items | ✅ Done | Export > Code (Rust/C/C++/Python) |
| Library dropdown instead of list | ❌ Not done | Currently a list, should be a dropdown selector |
| Component filter/search | ❌ Not done | No filter input |
| "Create Component" from context menu | ❌ Not done | |
| Component tree editor (second column) | ❌ Not done | |
| Drag-and-drop components into DOM tree | ❌ Not done | |
| Grey rendering of component internals | ❌ Not done | |
| Live preview with CPU render | ❌ Not done | |
| Context menu: nested library → component insertion | ❌ Not done | |

### Phase 4: Code export — PARTIALLY DONE

| Item | Status | Notes |
|------|--------|-------|
| `export_code` debug API endpoint | ✅ Done | Returns ExportedCodeResponse with files map |
| Rust scaffold: Cargo.toml + src/main.rs | ✅ Done | DataModel struct, layout callback, callback stubs |
| C scaffold: main.c with struct + layout | ✅ Done | |
| C++ scaffold: main.cpp with struct + layout | ✅ Done | |
| Python scaffold: main.py with class + layout | ✅ Done | |
| Helper functions (to_pascal_case, map_type_to_rust, etc.) | ✅ Done | |
| ZIP packaging + base64 response | ❌ Not done | Currently returns files as JSON map |
| Component module structure (components/mod.rs) | ❌ Not done | All code in single main file |

### Phase 5: Multi-language code export — PARTIALLY DONE

| Item | Status | Notes |
|------|--------|-------|
| Builtin `compile_fn` handles all 4 languages | ✅ Done | `builtin_compile_fn` handles Rust/C/C++/Python |
| `For` / `If` / `Map` structural components | ❌ Not done | |
| Per-language iteration/conditional patterns | ❌ Not done | |

### Phase 6: Source-aware export — NOT STARTED

| Item | Status |
|------|--------|
| `source_file` tracking per component | ❌ |
| Change detection on re-export | ❌ |
| User code preservation markers | ❌ |

---

## 2. New Requirements (from user feedback on debugger screenshot)

### 2.1 UI Redesign: Library Selector + Component List

**Current:** Two-section sidebar — "COMPONENT LIBRARIES" (list of libraries) + "COMPONENTS" (list of components in selected library). Each component shows `<> DisplayName <tag>` with an icon.

**Required:**

- **Heading:** "Components" (not "COMPONENT LIBRARIES")
- **Library selector:** A dropdown `Library: <builtin> ▾` — not a clickable list. Picking a library from the dropdown loads its components below.
- **Below dropdown:** Two buttons: `+ Library` and `+ Component`
  - `+ Library` creates a new empty user-defined library (name prompt)
  - `+ Component` creates a new empty component in the current library
  - Both buttons should be **hidden** (or disabled) if the library advertises `readonly: true` (i.e., builtin/compiled libraries cannot be modified)
- **Component list:** No `<>` icon prefix, no `<tag>` suffix — just the display name. Clean list.
- **Filter input:** Text field above the component list to filter by name (client-side fuzzy match).

### 2.2 Library Mutability Flag

The `ComponentLibrary` struct needs a field to indicate whether the library accepts user modifications (add/remove/edit components):

```rust
pub struct ComponentLibrary {
    // ... existing fields ...
    /// Whether this library can be modified by the user (add/remove/edit components)
    /// False for builtin and compiled libraries.
    pub modifiable: bool,
}
```

This is **separate** from `exportable`:
- `modifiable = false, exportable = false` → builtin (can't change, can't export)
- `modifiable = false, exportable = true` → compiled plugin (can't change, can export the JSON definition)
- `modifiable = true, exportable = true` → user-created (full control)

The debug server should return this field in `LibrarySummary` and `ComponentLibraryInfo`.

### 2.3 Component Detail: Two-Column Layout

The main editor area, when a component is selected, should show **two columns**:

**Left column — Component Properties:**
- Component name + library badge
- Description (editable for user-defined)
- Data model fields table (the component's data structure — see §2.5)
- Callback slots table
- Component-level CSS (editable for user-defined, read-only for builtin)
  - If CSS is edited, triggers re-render of the preview (right column)
- Component-specific parameters separated from universal HTML attributes
  - Universal HTML attributes (id, class, style, tabindex, aria-*, contenteditable, draggable, hidden, lang, dir, title, role, data-*) should be in a **collapsed** `<details>` section labeled "Universal HTML Attributes"
  - Component-specific attributes (e.g., `href` for Link, `src` for Image) should be shown **first**, above the collapsed universal section

**Right column — Component Tree / Preview:**
- A mini DOM tree showing the **internal structure** of this component (its template)
- For user-defined components: **editable** — drag-and-drop other components from the library list to build the template
- For builtin components: read-only, showing that it maps to a single NodeType
- Below the tree: a **preview image** rendered via the CPU renderer (see §2.7)

### 2.4 Component Tree Builder (Drag-and-Drop)

The right column of the component detail is where users **build** custom components:

1. The library's component list (left sidebar) acts as a **palette**
2. User drags a component (e.g., "Div") from the palette into the component tree (right column)
3. Dropping inserts the component as a child (or sibling, depending on drop position)
4. The tree shows the component's internal structure with indentation
5. Each node in the tree can be:
   - Selected (highlights, shows properties)
   - Deleted (right-click → delete)
   - Reordered (drag within the tree)
6. Text content can be added by selecting a text-accepting node and typing in a text field

**This is the core "GUI builder" feature:** users create component templates by composing
other components visually, then export the result as code.

### 2.5 Data Models as Structured Types (not flat attributes)

**Current:** `ComponentDataField` is a flat list of `(name, field_type, default_value, description)` where `field_type` is a string like `"String"`, `"f32"`, `"RefAny"`.

**Required:** Data models should support **nested custom types**. A component's data model is not just "a list of attributes" — it's a type definition that can reference other type definitions.

Example: A `UserCard` component might have:

```
UserCardDataModel {
    user: UserProfile,        // nested custom type
    show_avatar: bool,
    on_click: RefAny,         // backreference slot
}

UserProfile {
    name: String,
    avatar_url: String,
    bio: String,
}
```

This means `ComponentDataField.field_type` can be:
- **Primitive:** `"String"`, `"bool"`, `"i32"`, `"f32"`, `"u32"`, `"usize"`
- **Built-in complex:** `"RefAny"` (backreference slot), `"OptionString"`, etc.
- **User-defined struct:** `"UserProfile"` — references another data model definition
- **Callback type:** `"ButtonOnClickCallbackType"` — from api.json

For code generation, nested types produce nested `struct` definitions. For the debugger,
nested types show as expandable trees in the data model inspector.

**Where are custom data model definitions stored?**

Each `ComponentLibrary` should have a `data_models` field — a registry of named struct
definitions that components in that library can reference:

```rust
pub struct ComponentLibrary {
    // ... existing fields ...
    /// Named data model types defined by this library
    /// Components reference these by name in their field_type
    pub data_models: ComponentDataModelVec,
}

/// A named data model (struct definition) for code generation
pub struct ComponentDataModel {
    /// Type name, e.g. "UserProfile"
    pub name: AzString,
    /// Description
    pub description: AzString,
    /// Fields in this struct
    pub fields: ComponentDataFieldVec,
}
```

**Default instantiation:** Each component should be able to produce a "default" instance
of its data model — all fields filled with their `default_value`. This is used:
- In the debugger preview (render with default data)
- As a starting point when the user drags the component into a layout
- For validation when wiring up data bindings

### 2.6 Same Component List in Inspector View

**Requirement:** The DOM tree in the Inspector view should also have access to the component
palette. Users should be able to drag components from the palette and drop them **between**
existing DOM tree nodes to insert them.

**Implementation approach:**
- The component list (currently only in the Components sidebar view) should also be
  accessible in the Inspector view — either as a collapsible section below the DOM tree,
  or as a persistent palette panel.
- Drop targets appear between DOM tree nodes on drag hover
- Dropping a component invokes `insert_node` with the component's tag and default attributes

### 2.7 Component Preview via CPU Renderer

**New API endpoint:** `get_component_preview`

```json
{
    "op": "get_component_preview",
    "component": "builtin:div",
    "config": {
        "width": 300,
        "height": 200,
        "theme": "dark",
        "os": "macos",
        "data": { }
    }
}
```

Returns a base64-encoded PNG image of the component rendered via the CPU renderer
(same path as `take_screenshot`, but rendering an isolated component subtree).

**Preview updates when:**
- The component's internal tree structure changes (nodes added/removed/reordered)
- The component's data model defaults change (different preview data)
- The component's scoped CSS changes (different styling)

**Preview configuration:**
- `width` / `height` — viewport size (null = auto-fit)
- `theme` — "light" | "dark" (applies UA stylesheet variant)
- `os` — "macos" | "windows" | "linux" (for `@media` queries and platform-specific rendering)
- `data` — JSON object to fill the data model fields (overrides defaults)

**Implementation:** The server receives the component definition, constructs a minimal
DOM tree using `render_fn`, applies `scoped_css`, renders via the existing CPU screenshot
path, and returns the base64 image. This does **not** require a window — it uses the
headless/software renderer.

### 2.8 Removing Visual Noise

- **No `<>` icon** in the component list items
- **No `<tag>` suffix** after the display name in the list
- Just clean display names: "Link", "Div", "Button", "Avatar"
- The tag name is shown **in the detail panel** when a component is selected (as `builtin:a`)

---

## 3. Data Architecture Changes

### 3.1 `ComponentLibrary` Updates

```rust
pub struct ComponentLibrary {
    pub name: AzString,
    pub version: AzString,
    pub description: AzString,
    pub components: ComponentDefVec,
    pub exportable: bool,
    pub modifiable: bool,                    // NEW: can user add/remove/edit components?
    pub data_models: ComponentDataModelVec,  // NEW: library-level type definitions
}
```

### 3.2 `ComponentDataModel` (New Type)

```rust
/// A named struct definition used as a data model type.
/// Components reference these by name in ComponentDataField.field_type.
#[repr(C)]
pub struct ComponentDataModel {
    /// Type name, e.g. "UserProfile", "TodoItem"
    pub name: AzString,
    /// Human-readable description
    pub description: AzString,
    /// Fields in this struct
    pub fields: ComponentDataFieldVec,
}
```

This allows nesting: a field with `field_type = "UserProfile"` references a
`ComponentDataModel` with `name = "UserProfile"` in the same library.

### 3.3 `ComponentDef` Updates

```rust
pub struct ComponentDef {
    // ... existing fields unchanged ...

    /// XML/HTML template body for user-defined components.
    /// Used by the template-based render_fn/compile_fn.
    /// Empty for builtin components (they render via node_type).
    pub template: AzString,  // NEW: the component's XML template body
}
```

The `template` field stores the component's internal DOM structure as XML. For example:
```xml
<div class="avatar" style="width: {size}; height: {size};">
    <img src="{image}" />
    <span class="fallback">{fallback}</span>
</div>
```

This is what gets rendered in the component tree builder, and what the template-based
`render_fn` / `compile_fn` expand.

### 3.4 Debug Server Response Updates

`ComponentLibraryInfo` gains:
```rust
pub modifiable: bool,
pub data_models: Vec<DataModelInfo>,
```

`ComponentInfo` gains:
```rust
pub template: String,
```

`LibrarySummary` gains:
```rust
pub modifiable: bool,
```

New response type:
```rust
pub struct ComponentPreviewResponse {
    /// Base64-encoded PNG image data
    pub image: String,
    /// Width of the rendered image
    pub width: u32,
    /// Height of the rendered image
    pub height: u32,
}
```

### 3.5 New Debug API Endpoints

| Endpoint | Purpose |
|----------|---------|
| `get_component_preview` | Render component preview image (CPU) |
| `create_library` | Create a new empty user-defined library |
| `delete_library` | Delete a user-defined library |
| `create_component` | Create a new empty component in a library |
| `delete_component` | Delete a component from a library |
| `update_component` | Update a component's template, CSS, data model, etc. |
| `update_component_tree` | Update a component's internal DOM tree (drag-and-drop result) |

---

## 4. Implementation Plan (Ordered)

### Step 1: Core Type Updates

Files: `core/src/xml.rs`

1. Add `modifiable: bool` field to `ComponentLibrary`
2. Add `ComponentDataModel` struct + `ComponentDataModelVec` (impl_vec!, impl_option!, etc.)
3. Add `data_models: ComponentDataModelVec` field to `ComponentLibrary`
4. Add `template: AzString` field to `ComponentDef`
5. Update `register_builtin_components()`: set `modifiable: false`, `data_models: empty`, `template: empty` for builtins
6. Update `builtin_component_def()`: add `template: AzString::from_const_str("")`
7. Update `user_defined_render_fn` / `user_defined_compile_fn` to use the `template` field when non-empty (parse XML, expand variables, render/compile — leveraging existing `render_dom_from_body_node_inner` / `compile_node_to_rust_code_inner`)

### Step 2: Debug Server Updates

Files: `dll/src/desktop/shell2/common/debug_server.rs`

1. Update `ComponentLibraryInfo`, `LibrarySummary` to include `modifiable` + `data_models`
2. Update `ComponentInfo` to include `template`
3. Update `build_component_registry()` to populate new fields
4. Separate component-specific attributes from universal HTML attributes in the response
   (add `universal_attributes` and `specific_attributes` fields, or mark attributes with `is_universal: bool`)
5. Add new debug event handlers:
   - `CreateLibrary { name, description }`
   - `DeleteLibrary { name }`
   - `CreateComponent { library, name, display_name }`
   - `DeleteComponent { library, component }`
   - `UpdateComponent { library, component, template?, scoped_css?, data_model?, ... }`
   - `GetComponentPreview { component, config }`
6. Implement `GetComponentPreview`: construct DOM from render_fn, apply CSS, use CPU renderer, return base64 PNG
7. Update `ExportedLibraryResponse` / `ExportedComponentDef` to include `template`, `data_models`

### Step 3: Debugger HTML Restructure

Files: `dll/src/desktop/shell2/common/debugger/debugger.html`

1. Replace `#component-registry-container` + `#component-list-container` with:
   - Heading: "Components"
   - Dropdown: `Library: <select id="library-selector">...</select>`
   - Buttons: `+ Library` | `+ Component` (conditionally shown based on `modifiable`)
   - Filter: `<input type="text" id="component-filter" placeholder="Filter...">`
   - Component list: `<div id="component-list-container">...</div>` (clean names, no icons)
2. Update `#view-components` to two-column layout:
   - Left: component properties (data model, callbacks, CSS, attributes with universal collapsed)
   - Right: component tree editor + preview image

### Step 4: Debugger JS — Library Selector & Filter

Files: `dll/src/desktop/shell2/common/debugger/debugger.js`

1. Replace `loadLibraries()` → populates a `<select>` dropdown instead of a list
2. Add `onLibraryChange()` handler for dropdown selection
3. Add `filterComponents()` handler for the filter input (client-side fuzzy match)
4. Update `_renderComponentList()`:
   - No `<>` icon prefix
   - No `<tag>` suffix
   - Just display name, clean list items
5. Add `createLibrary()` handler — prompts for name, posts `create_library`
6. Add `createComponent()` handler — prompts for name, posts `create_component`
7. Hide/disable `+ Library` / `+ Component` buttons when `modifiable = false`

### Step 5: Debugger JS — Two-Column Component Detail

Files: `dll/src/desktop/shell2/common/debugger/debugger.js`, `debugger.css`

1. Rewrite `showComponentDetail()` to render two-column layout:
   - Left: properties panel with collapsible sections
   - Right: component tree + preview
2. Split attributes into "Component Attributes" (shown open) + "Universal HTML Attributes" (collapsed `<details>`)
3. Show data model as structured type tree (with nested type expansion)
4. For user-defined components: make CSS editable (textarea), on change → post `update_component`
5. For user-defined components: make description editable
6. Add preview image container (loads via `get_component_preview`)

### Step 6: Component Tree Editor

Files: `dll/src/desktop/shell2/common/debugger/debugger.js`, `debugger.css`

1. Render the component's `template` as a mini DOM tree in the right column
2. For user-defined components: make tree editable:
   - Drag from component palette → drop into tree
   - Right-click → delete node
   - Drag within tree → reorder
   - Select node → show inline property editor
3. On tree change → post `update_component_tree` → re-render preview
4. For builtin components: show read-only tree (just the NodeType mapping)

### Step 7: Component Preview Rendering

Files: `dll/src/desktop/shell2/common/debug_server.rs`, `debugger.js`

1. Implement `GetComponentPreview` handler:
   - Call component's `render_fn` with default data model values
   - Apply `scoped_css`
   - Use CPU renderer to produce image
   - Return base64 PNG
2. In debugger JS: call `get_component_preview` when component is selected, on template change, on CSS change, on data model change
3. Show preview image below the component tree

### Step 8: Drag-and-Drop in Inspector View

Files: `dll/src/desktop/shell2/common/debugger/debugger.js`, `debugger.html`, `debugger.css`

1. Add component palette to Inspector view (collapsible panel or persistent sidebar section)
2. Implement drag start on component list items
3. Implement drop zones between DOM tree nodes
4. On drop → invoke `insert_node` API with component tag and defaults
5. Refresh DOM tree after insertion

---

## 5. Type System Summary

### Primitive Types (JSON-level)

These are the base types available for `ComponentDataField.field_type`:

| Type | Rust | C | Python | JSON |
|------|------|---|--------|------|
| `String` | `String` | `AzString` | `str` | `"hello"` |
| `bool` | `bool` | `bool` | `bool` | `true` |
| `i32` | `i32` | `int32_t` | `int` | `42` |
| `f32` | `f32` | `float` | `float` | `3.14` |
| `u32` | `u32` | `uint32_t` | `int` | `42` |
| `usize` | `usize` | `size_t` | `int` | `42` |
| `RefAny` | `RefAny` | `AzRefAny` | `RefAny` | (backreference) |
| `OptionString` | `Option<String>` | `AzOptionString` | `Optional[str]` | `null` or `"..."` |

### User-Defined Types

User types are defined as `ComponentDataModel` entries in the library's `data_models` vec.
They are referenced by name in `ComponentDataField.field_type`.

Code export resolves these to struct definitions in the target language.

### Callback Types

Callback type names (e.g., `"ButtonOnClickCallbackType"`) reference definitions in
`api.json`. The debugger shows the full signature. Code export generates the correct
callback typedef + wiring.

---

## 6. Requirements Checklist (Nothing Dropped)

From original `COMPONENT_SYSTEM_REPORT.md`:

- [x] repr(C) ComponentDef with function pointers (§2.1)
- [x] ComponentId with collection:name namespacing (§2.1)
- [x] ComponentParam, ComponentCallbackSlot, ComponentDataField (§2.1, §5.3)
- [x] ChildPolicy enum (§2.1) — missing `Specific(StringVec)` variant
- [x] ComponentLibrary with version, description, exportable (§2.5)
- [x] ComponentMap with qualified lookup (§2.6)
- [x] RegisterComponentFn / RegisterComponentLibraryFn for C FFI (§1.2)
- [x] Debug server: get_component_registry, get_libraries, get_library_components (§3.1)
- [x] Debug server: import_component_library, export_component_library (§3.4, §4.2)
- [x] Debug server: export_code with multi-language scaffold (§3.4, §5.2)
- [x] Debugger: component sidebar with library grouping (§3.1)
- [x] Debugger: component detail panel with params, data model, callbacks (§3.1)
- [x] Debugger: Import/Export menus (§3.4)
- [ ] Format specifiers in `split_dynamic_string` (§2.4)
- [ ] `AzCompileDomContext` for structural components (§2.3)
- [ ] `ComponentInstance` struct for DOM tree (§2.2)
- [ ] Template-based render_fn/compile_fn for JSON components (§2.5)
- [ ] "Create Component" from DOM subtree (§3.2)
- [ ] Grey rendering of component internals in DOM tree (§3.6)
- [ ] Context menu: nested library → component insertion (§3.3)
- [ ] Snapshot-based live preview (§3.5)
- [ ] For/If/Map structural components (§5.4)
- [ ] ZIP packaging for code export (§5.2)
- [ ] `source_file` tracking and user code preservation (Phase 6)

From new user requirements (this session):

- [ ] Library dropdown selector (not list) (§2.1)
- [ ] `+ Library` / `+ Component` buttons with modifiable check (§2.1)
- [ ] `modifiable` field on ComponentLibrary (§2.2)
- [ ] Component filter input (§2.1)
- [ ] Clean component list (no `<>` icon, no `<tag>` suffix) (§2.8)
- [ ] Two-column component detail: properties + tree/preview (§2.3)
- [ ] Universal HTML attributes collapsed separately (§2.3)
- [ ] Component-level CSS display, editable for user components (§2.3)
- [ ] Component tree builder via drag-and-drop (§2.4)
- [ ] Nested/structured data models with `ComponentDataModel` (§2.5)
- [ ] Default data model instantiation for preview (§2.5)
- [ ] Component palette in Inspector view + drag-drop into DOM tree (§2.6)
- [ ] `get_component_preview` API (CPU render) (§2.7)
- [ ] Preview auto-updates on structure/CSS/data change (§2.7)
- [ ] `template` field on ComponentDef (§3.3)
- [ ] `data_models` field on ComponentLibrary (§3.1)
- [ ] CRUD endpoints: create_library, delete_library, create_component, delete_component, update_component (§3.5)
