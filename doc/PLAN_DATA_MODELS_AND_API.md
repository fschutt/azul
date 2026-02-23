# Plan 1: Data Models & API Updates for the Component Type System

## Goal

Update the Rust data models in `core/src/xml.rs`, the FFI surface in `api.json`,
and the debug server API in `debug_server.rs` so that:

1. Component fields use **rich type descriptors** (`ComponentFieldType` enum)
   instead of opaque `AzString` type names.
2. The `parameters` and `callback_slots` fields on `ComponentDef` are **unified**
   into a single `data_model: ComponentDataModel`.
3. The debug server serializes **structured** field type info to the browser
   (instead of flat `"String"` / `"bool"` strings).
4. **Component previewing** works: editing any field value instantly re-renders
   the component via `render_fn` + `format_args_dynamic` + `Css::from_string`.

---

## Phase 1 — Define New Types (non-breaking, additive)

### 1.1 New types in `core/src/xml.rs`

All types must be `#[repr(C)]` or `#[repr(C, u8)]`.

| Type | Kind | Purpose |
|---|---|---|
| `ComponentFieldType` | `enum(C, u8)` | Rich type descriptor (String, Bool, I32, …, Vec, Option, Callback, StyledDom, RefAny, StructRef, EnumRef, ColorU, etc.) |
| `ComponentFieldTypeBox` | `struct { ptr: *mut ComponentFieldType }` | Heap-indirection for recursive types (e.g. `Option<String>`) |
| `ComponentCallbackSignature` | `struct` | Return type + argument list for a callback |
| `ComponentCallbackArg` | `struct` | Single callback argument (name + type) |
| `ComponentEnumModel` | `struct` | Enum definition (name + variants) |
| `ComponentEnumVariant` | `struct` | Single enum variant (name + optional fields) |
| `ComponentDefaultValue` | `enum(C, u8)` | Default: None, String, Bool, I32, …, ComponentInstance, CallbackFnPointer |
| `ComponentInstanceDefault` | `struct` | `{ library, component, field_overrides }` |
| `ComponentFieldOverride` | `struct` | `{ field_name, source }` |
| `ComponentFieldValueSource` | `enum(C, u8)` | Default / Literal / Binding |
| `ComponentFieldValue` | `enum(C, u8)` | Runtime value (String, Bool, I32, StyledDom, Callback, …) |
| `ComponentFieldNamedValue` | `struct` | `{ name: AzString, value: ComponentFieldValue }` |
| `ComponentDataField` (updated) | `struct` | `{ name, field_type: ComponentFieldType, default_value: OptionComponentDefaultValue, required, description }` |
| `ComponentDataModel` | `struct` | `{ name, description, fields: ComponentDataFieldVec }` |

### 1.2 `impl_vec!` / `impl_option!` wrappers

Each new type needs FFI-safe collection wrappers:

```rust
impl_vec!(ComponentDataField, ComponentDataFieldVec, ...);
impl_vec!(ComponentCallbackArg, ComponentCallbackArgVec, ...);
impl_vec!(ComponentEnumVariant, ComponentEnumVariantVec, ...);
impl_vec!(ComponentEnumModel, ComponentEnumModelVec, ...);
impl_vec!(ComponentFieldOverride, ComponentFieldOverrideVec, ...);
impl_vec!(ComponentFieldNamedValue, ComponentFieldNamedValueVec, ...);
impl_vec!(ComponentFieldValue, ComponentFieldValueVec, ...);
impl_option!(ComponentDefaultValue, OptionComponentDefaultValue, ...);
impl_option!(ComponentFieldType, OptionComponentFieldType, ...);
```

### 1.3 Concrete `ComponentFieldType` enum definition

```rust
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum ComponentFieldType {
    String,
    Bool,
    I32,
    I64,
    U32,
    U64,
    Usize,
    F32,
    F64,
    ColorU,
    CssProperty,
    ImageRef,
    FontRef,
    StyledDom,                                   // slot — field name = slot name
    Callback { signature: ComponentCallbackSignature },
    RefAny { type_hint: AzString },
    Option { inner: ComponentFieldTypeBox },      // recursive via Box
    Vec { inner: ComponentFieldTypeBox },         // recursive via Box
    StructRef { name: AzString },                // resolved in same library
    EnumRef { name: AzString },                  // resolved in same library
}
```

**Nesting depth limit**: restrict to depth 1 (i.e. `Option<String>` is OK,
`Option<Option<String>>` is not). This covers all practical widget field types
and avoids deep FFI pointer chains.

### 1.4 Add to `api.json`

Add all types above to a new `module "component"`. The codegen system already
handles `#[repr(C, u8)]` enums with associated data (see `CssProperty` as
precedent), so no codegen changes are needed — just new entries in `api.json`.

### 1.5 Validation

```bash
cargo run --release -p azul-doc -- codegen all
cargo build --release -p azul-dll --features "build-dll"
```

Both must pass. No existing behavior changes — all new types are additive.

---

## Phase 2 — Migrate `ComponentDataField`

### 2.1 Change field_type

**Before:**
```rust
pub struct ComponentDataField {
    pub name: AzString,
    pub field_type: AzString,           // "String", "bool", etc.
    pub default_value: OptionAzString,  // "hello", "true", etc.
    pub description: AzString,
}
```

**After:**
```rust
pub struct ComponentDataField {
    pub name: AzString,
    pub field_type: ComponentFieldType,
    pub default_value: OptionComponentDefaultValue,
    pub required: bool,
    pub description: AzString,
}
```

### 2.2 Update `builtin_data_model()` helpers

Every builtin component definition uses `data_field("href", "String", None, "...")`.
Update to use structured types:

```rust
// Before
data_field("href", "String", None, "Link URL")

// After
data_field("href", ComponentFieldType::String, None, "Link URL")
```

The `data_field()` helper signature changes from:
```rust
fn data_field(name: &str, type_str: &str, default: Option<&str>, desc: &str) -> ComponentDataField
```
to:
```rust
fn data_field(name: &str, ft: ComponentFieldType, default: Option<ComponentDefaultValue>, desc: &str) -> ComponentDataField
```

### 2.3 Update all 17 widget `builtin_component_def()` functions

Each widget file in `layout/src/widgets/` has a `builtin_component_def()` that
returns the component's type metadata. Update each to use `ComponentFieldType`
variants instead of type strings:

| Widget | Fields to update |
|---|---|
| Label | `text: String` |
| Button | `text: String`, `on_click: Callback(...)` |
| CheckBox | `checked: Bool`, `on_toggle: Callback(...)` |
| ColorInput | `color: ColorU`, `on_change: Callback(...)` |
| NumberInput | `value: F64`, `min: Option<F64>`, `max: Option<F64>`, `on_change: Callback(...)` |
| TextInput | `text: String`, `on_change: Callback(...)`, `on_submit: Callback(...)` |
| DropDown | `items: Vec<String>`, `selected: Option<Usize>`, `on_select: Callback(...)` |
| ListView | `items: Vec<StyledDom>`, `on_select: Callback(...)` |
| TreeView | `root: StructRef("TreeNode")`, `on_select: Callback(...)` |
| ProgressBar | `value: F64`, `max: F64` |
| Frame | `title: String`, `content: StyledDom` |
| TabHeader | `tabs: Vec<String>`, `active: Usize`, `on_select: Callback(...)` |
| FileInput | `path: Option<String>`, `on_select: Callback(...)` |
| Ribbon | `tabs: Vec<StructRef("RibbonTab")>` |
| NodeGraph | (complex — keep as String fields initially, refine later) |
| Titlebar | `title: String`, `icon: Option<ImageRef>` |

---

## Phase 3 — Unify `parameters` + `callback_slots` → `data_model`

### 3.1 Remove separate fields from `ComponentDef`

**Before:**
```rust
pub struct ComponentDef {
    // ...
    pub parameters: ComponentDataFieldVec,   // data fields
    pub callback_slots: ComponentCallbackSlotVec,  // callbacks
    // ...
}
```

**After:**
```rust
pub struct ComponentDef {
    // ...
    pub data_model: ComponentDataModel,  // unified: contains all fields including callbacks
    // ...
}
```

A `ComponentDataModel` is:
```rust
pub struct ComponentDataModel {
    pub name: AzString,          // e.g. "ButtonData"
    pub description: AzString,
    pub fields: ComponentDataFieldVec,  // includes Callback fields
}
```

### 3.2 Migration logic

For each existing `ComponentDef`:
1. Take all entries from `parameters` → add to `data_model.fields` as-is.
2. Take all entries from `callback_slots` → convert each to a `ComponentDataField`
   with `field_type: ComponentFieldType::Callback(ComponentCallbackSignature { ... })`.
3. Set `data_model.name = format!("{}Data", display_name)`.
4. Remove the old `parameters` and `callback_slots` fields.

### 3.3 Update all callers

Files that access `component_def.parameters` or `component_def.callback_slots`:
- `core/src/xml.rs` — template rendering, field resolution
- `dll/src/desktop/shell2/common/debug_server.rs` — `build_component_registry()`,
  `build_exported_code()`, `update_component` handler
- `layout/src/widgets/*.rs` — `builtin_component_def()` functions

---

## Phase 4 — Update `ComponentRenderFn` Signature

### 4.1 New signature

**Before:**
```rust
pub type ComponentRenderFn = fn(
    &ComponentDef,
    &XmlComponentMap,            // BTreeMap — not FFI safe
    &FilteredComponentArguments, // BTreeMap — not FFI safe
    &OptionString,
) -> Result<StyledDom, RenderDomError>;
```

**After:**
```rust
pub type ComponentRenderFn = fn(
    &ComponentDef,
    &ComponentMap,                  // FFI-safe component registry
    &ComponentFieldNamedValueVec,   // actual runtime values
    &OptionString,                  // text content
) -> Result<StyledDom, RenderDomError>;
```

### 4.2 `ComponentFieldNamedValueVec`

This carries the actual runtime values for each field in the data model:

```rust
pub struct ComponentFieldNamedValue {
    pub name: AzString,
    pub value: ComponentFieldValue,
}
```

For preview, values come from `ComponentDefaultValue` (converted to runtime
`ComponentFieldValue`). For runtime, values come from the parent's data
bindings or literal overrides.

### 4.3 Update all `builtin_render_fn` implementations

Each widget's render function currently receives `FilteredComponentArguments`
(a `BTreeMap<String, String>`) and parses values from strings:

```rust
// Before (in render fn):
let text = args.get("text").unwrap_or_default();

// After:
let text = values.find("text").as_string().unwrap_or_default();
```

Add helper methods on `ComponentFieldNamedValueVec`:
```rust
impl ComponentFieldNamedValueVec {
    fn find(&self, name: &str) -> Option<&ComponentFieldValue>;
}

impl ComponentFieldValue {
    fn as_string(&self) -> Option<&str>;
    fn as_bool(&self) -> Option<bool>;
    fn as_i32(&self) -> Option<i32>;
    fn as_f64(&self) -> Option<f64>;
    fn as_color_u(&self) -> Option<&ColorU>;
    fn as_styled_dom(&self) -> Option<&StyledDom>;
    // etc.
}
```

### 4.4 Update `ComponentCompileFn` similarly

Same signature change. Used for compile-time code generation.

---

## Phase 5 — Update Debug Server API

### 5.1 Structured `ComponentDataFieldInfo`

**Before** (server → browser JSON):
```json
{
    "name": "color",
    "field_type": "ColorU",
    "default": "#ff0000",
    "description": "Background color"
}
```

**After:**
```json
{
    "name": "color",
    "field_type": { "type": "ColorU" },
    "default": { "type": "ColorU", "value": { "r": 255, "g": 0, "b": 0, "a": 255 } },
    "required": false,
    "description": "Background color"
}
```

For complex types:
```json
{
    "name": "items",
    "field_type": { "type": "Vec", "inner": { "type": "String" } },
    "default": null,
    "required": true,
    "description": "List items"
}

{
    "name": "on_click",
    "field_type": {
        "type": "Callback",
        "signature": {
            "return_type": "Update",
            "args": [
                { "name": "button_id", "arg_type": { "type": "String" } }
            ]
        }
    },
    "default": { "type": "CallbackFnPointer", "fn_name": "my_crate::handle_click" },
    "required": false,
    "description": "Click handler"
}
```

### 5.2 Update `build_component_registry()`

Currently serializes `ComponentDataFieldInfo { field_type: String }`.
Change to serialize `ComponentFieldType` → JSON recursively:

```rust
fn serialize_field_type(ft: &ComponentFieldType) -> serde_json::Value {
    match ft {
        ComponentFieldType::String => json!({"type": "String"}),
        ComponentFieldType::Bool => json!({"type": "Bool"}),
        ComponentFieldType::I32 => json!({"type": "I32"}),
        ComponentFieldType::Option { inner } => json!({
            "type": "Option",
            "inner": serialize_field_type(inner)
        }),
        ComponentFieldType::Vec { inner } => json!({
            "type": "Vec",
            "inner": serialize_field_type(inner)
        }),
        ComponentFieldType::Callback { signature } => json!({
            "type": "Callback",
            "signature": serialize_callback_sig(signature)
        }),
        ComponentFieldType::StructRef { name } => json!({
            "type": "StructRef",
            "name": name
        }),
        ComponentFieldType::EnumRef { name } => json!({
            "type": "EnumRef",
            "name": name
        }),
        // ...
    }
}
```

### 5.3 Update `update_component` handler

Currently accepts:
```json
{
    "op": "update_component",
    "data_model": [{ "name": "...", "type": "String", "default": "...", "description": "..." }],
    "callback_slots": [{ "name": "...", "callback_type": "...", "description": "..." }]
}
```

Change to:
```json
{
    "op": "update_component",
    "library": "mylib",
    "name": "my-tag",
    "data_model": {
        "name": "MyTagData",
        "description": "...",
        "fields": [
            { "name": "href", "field_type": { "type": "String" }, "default": { "type": "String", "value": "https://..." }, "required": false, "description": "Link target" },
            { "name": "on_click", "field_type": { "type": "Callback", "signature": { "return_type": "Update", "args": [] } }, "default": null, "required": false, "description": "Click handler" }
        ]
    }
}
```

No more separate `callback_slots` array — callbacks are fields in `data_model.fields`.

### 5.4 Update `ExportedDataField` / `ExportedCallbackSlot`

Merge into a single `ExportedField` struct that carries `ComponentFieldType` info
for code generation (`generate_scaffold`, `map_type_to_rust`, `map_type_to_c`,
`map_type_to_python`).

### 5.5 Update code gen functions

`map_type_to_rust()`, `map_type_to_c()`, `map_type_to_python()` currently match
on string `field_type`. Change to match on `ComponentFieldType` enum:

```rust
fn map_type_to_rust(ft: &ComponentFieldType) -> String {
    match ft {
        ComponentFieldType::String => "String".to_string(),
        ComponentFieldType::Bool => "bool".to_string(),
        ComponentFieldType::I32 => "i32".to_string(),
        ComponentFieldType::Option { inner } => format!("Option<{}>", map_type_to_rust(inner)),
        ComponentFieldType::Vec { inner } => format!("Vec<{}>", map_type_to_rust(inner)),
        ComponentFieldType::StyledDom => "StyledDom".to_string(),
        ComponentFieldType::Callback { signature } => generate_callback_typedef(signature),
        ComponentFieldType::StructRef { name } => name.to_string(),
        ComponentFieldType::EnumRef { name } => name.to_string(),
        // ...
    }
}
```

---

## Phase 6 — Component Previewing on User Change

### 6.1 New API endpoint: `preview_component`

```json
{
    "op": "preview_component",
    "library": "mylib",
    "component": "my-tag",
    "field_values": [
        { "name": "text", "value": { "type": "String", "value": "Hello!" } },
        { "name": "color", "value": { "type": "ColorU", "value": { "r": 255, "g": 0, "b": 0, "a": 255 } } }
    ],
    "dynamic_selector_context": {
        "os": "macos",
        "theme": "dark",
        "language": "en-US"
    }
}
```

**Response:**
```json
{
    "status": "ok",
    "data": {
        "type": "preview_result",
        "value": {
            "screenshot_base64": "iVBOR...",
            "dom_tree": { ... },
            "css_expanded": ".avatar { width: 48px; ... }",
            "errors": []
        }
    }
}
```

### 6.2 Server-side preview pipeline

```
1. Receive preview_component request
2. Look up ComponentDef in ComponentMap
3. Convert JSON field_values → ComponentFieldNamedValueVec
4. Expand scoped_css template: format_args_dynamic(scoped_css, field_values)
5. Parse CSS: Css::from_string(expanded_css)
6. Call render_fn(&component_def, &component_map, &field_values, &None)
   → produces StyledDom
7. Apply scoped CSS: dom.restyle(css)
8. Apply DynamicSelectorContext override (OS/theme/lang from request)
9. Layout + render → screenshot (PNG base64)
10. Return screenshot + expanded CSS + any errors
```

**Key**: steps 4–9 are all existing operations. The only new part is
wiring them together on a single API endpoint.

### 6.3 CSS template preview

When the user edits the `scoped_css` in the browser, the browser sends
a `preview_component` request with the **edited CSS** as an override:

```json
{
    "op": "preview_component",
    "library": "mylib",
    "component": "my-tag",
    "field_values": [ ... ],
    "css_override": ".avatar { width: {size}px; border-radius: 8px; }"
}
```

The server uses `css_override` (if present) instead of the component's
stored `scoped_css`. This allows live preview without saving.

### 6.4 Debouncing

The browser debounces preview requests (e.g. 150ms after last keystroke
in the CSS editor or field value change). The server returns the latest
screenshot. If a preview is already in progress when a new request arrives,
the old one can be cancelled.

### 6.5 Incremental updates

For performance, consider two preview modes:
1. **Full screenshot** — used when the user first opens a component or
   changes OS/theme/language. Slow (~100ms).
2. **CSS-only update** — when only CSS text changed (not field values or
   template), skip `render_fn` (DOM doesn't change) and only re-parse +
   re-apply CSS. Fast (~10ms).

The browser tracks what changed and sends `preview_mode: "full" | "css_only"`.

---

## Phase 7 — Remove Old System

### 7.1 Types to remove from `core/src/xml.rs`

| Type | Reason |
|---|---|
| `XmlComponentTrait` | `dyn Trait` — not FFI safe |
| `XmlComponent` | Uses `Box<dyn XmlComponentTrait>` |
| `XmlComponentMap` | `BTreeMap` — not FFI safe |
| `FilteredComponentArguments` | `BTreeMap` — not FFI safe |
| `ComponentArguments` | Uses `BTreeMap` |
| `ComponentArgumentTypes` | Type alias `Vec<(String, String)>` — replaced by `ComponentDataFieldVec` |
| `ComponentArgumentName` | Type alias `String` — no longer needed |
| `ComponentArgumentType` | Type alias `String` — no longer needed |

### 7.2 Types to remove from `api.json`

Same types as above. Run codegen after removal to update FFI bindings.

### 7.3 Renderer structs to remove

All `*Renderer` structs (`DivRenderer`, `BodyRenderer`, `ParagraphRenderer`, etc.)
are replaced by `ComponentDef` entries in the `ComponentMap` with
`source: Builtin` and a `render_fn`.

### 7.4 `html_component!` macro

Remove — component registration is now done via `ComponentDef` structs.

---

## Phase 8 — Enum and Struct Models in Component Libraries

### 8.1 `ComponentEnumModel` storage

Each `ComponentLibrary` carries a `Vec<ComponentEnumModel>`:

```rust
pub struct ComponentLibrary {
    pub name: AzString,
    pub version: AzString,
    pub description: AzString,
    pub components: ComponentDefVec,
    pub enum_models: ComponentEnumModelVec,    // NEW
    pub data_models: ComponentDataModelVec,    // NEW (reusable struct defs)
}
```

### 8.2 API endpoints for enum/struct management

```json
// Create an enum
{ "op": "create_enum", "library": "mylib", "name": "UserRole",
  "variants": ["Admin", "Editor", "Viewer"] }

// Create a reusable struct
{ "op": "create_struct", "library": "mylib", "name": "UserProfile",
  "fields": [
      { "name": "name", "field_type": { "type": "String" } },
      { "name": "email", "field_type": { "type": "Option", "inner": { "type": "String" } } }
  ] }

// List enums/structs
{ "op": "get_library_enums", "library": "mylib" }
{ "op": "get_library_structs", "library": "mylib" }
```

These are needed for the debugger to offer `EnumRef` / `StructRef` choices
when the user creates component fields.

---

## Dependency Graph

```
Phase 1 (new types)
    │
    ▼
Phase 2 (migrate ComponentDataField)
    │
    ▼
Phase 3 (unify parameters + callback_slots)
    │
    ├───────────────────────────────┐
    ▼                               ▼
Phase 4 (ComponentRenderFn)    Phase 5 (debug server API)
    │                               │
    └───────────────────────────────┘
                    │
                    ▼
            Phase 6 (preview_component)
                    │
                    ▼
            Phase 7 (remove old system)
                    │
                    ▼
            Phase 8 (enum/struct models)
```

Phases 4 and 5 can be done in parallel. Phase 6 requires both to be complete.
Phase 7 should be done last — it's a cleanup step. Phase 8 can be done
alongside or after Phase 6.

---

## Testing Strategy

### Unit tests

1. **Type system round-trip**: `ComponentFieldType` → JSON → `ComponentFieldType`
   for all variants.
2. **Default value conversion**: `ComponentDefaultValue` → `ComponentFieldValue`
   for all types.
3. **Field type parsing**: string syntax (`Option<String>`, `fn(String) -> Update`,
   `Vec<I32>`) → `ComponentFieldType`.
4. **Code generation**: `ComponentFieldType` → Rust/C/Python type strings.

### Integration tests

1. **Preview pipeline**: create a `ComponentDef` with scoped CSS containing
   template expressions, call `preview_component`, verify the expanded CSS
   and screenshot are correct.
2. **Widget migration**: verify all 17 builtin widgets still render correctly
   after migrating from string types to `ComponentFieldType`.
3. **API round-trip**: `create_component` → `update_component` (with structured
   data model) → `get_library_components` → verify JSON shape.

### Browser tests

1. **Component detail view**: verify the browser correctly renders type-specific
   controls for each `ComponentFieldType` variant.
2. **Preview on edit**: change a field value in the browser → verify preview
   updates within 200ms.
3. **CSS template editing**: type in the CSS editor → verify live preview
   shows correct styling.

---

## Files to Modify (Summary)

| File | Changes |
|---|---|
| `core/src/xml.rs` | Add new types, update `ComponentDataField`, `ComponentDef`, `ComponentRenderFn`, remove old types |
| `api.json` | Add new types to `module "component"`, remove old aliases |
| `dll/src/desktop/shell2/common/debug_server.rs` | Update `ComponentDataFieldInfo`, `build_component_registry()`, `update_component`, add `preview_component`, update code gen functions |
| `layout/src/widgets/*.rs` (17 files) | Update `builtin_component_def()` and `builtin_render_fn()` per widget |
| `doc/codegen` (generated) | Re-run `cargo run -p azul-doc -- codegen all` |

---

## Estimated Effort

| Phase | Effort | Risk |
|---|---|---|
| Phase 1 (new types) | Small — defining types + api.json | Low |
| Phase 2 (migrate fields) | Medium — 17 widget defs + helpers | Low |
| Phase 3 (unify params/callbacks) | Medium — structural refactor | Medium |
| Phase 4 (render fn signature) | Large — all render fns change | Medium-High |
| Phase 5 (debug server API) | Medium — serialization changes | Medium |
| Phase 6 (preview) | Medium — wiring existing ops | Low-Medium |
| Phase 7 (remove old) | Small — deletion | Low |
| Phase 8 (enum/struct) | Small — additive API | Low |
