# Component Metadata Type System — Design Document

> **Implementation Status (2025-02-25):**
>
> Most of this design is now **implemented**. Key changes from the original proposal:
>
> | Area | Status |
> |------|--------|
> | `ComponentFieldType` enum (§3.1) | ✅ Implemented — 20 variants in `core/src/xml.rs` |
> | `ComponentCallbackSignature` (§3.2) | ✅ Implemented — `type_name` removed (not needed; `args` + `return_type` suffice) |
> | `ComponentEnumModel` / `ComponentEnumVariant` (§3.3) | ✅ Implemented |
> | `ComponentDataField` with structured `field_type` (§3.4) | ✅ Implemented |
> | `ComponentDefaultValue` (§3.5) | ✅ Partially — `Json(AzString)` variant missing |
> | `ComponentLibrary.enum_models` (§3.6) | ✅ Implemented |
> | Parameters + callback_slots merged into data_model (§5.2) | ✅ Done — `ComponentDef` has 8 fields now |
> | Old system removed (§12 Phase 6) | ✅ Done — `XmlComponentTrait`, `FilteredComponentArguments` removed |
> | `template` field on `ComponentDef` | ❌ Removed by design — source-edit-recompile instead |
> | `ChildPolicy` | ❌ Removed — child acceptance derived from data model shape |
> | `ComponentParam` / `ComponentCallbackSlot` | 🗑️ Dead types — to be removed from code |
> | Debug server structured JSON (§8.1) | ⚠️ Not done — `field_type_to_string()` still flattens to strings |
> | `parse_field_type()` / `format_field_type()` (§7) | ⚠️ Not done — debugger.js has `_parseFieldType()` client-side |
> | CRUD endpoints (§3.5 / §12) | ✅ All 5 exist + `GetComponentPreview` with CPU render |
> | Code gen uses structured types (§11) | ⚠️ Not done — still uses string matching |
>
> See `COMPONENT_SYSTEM_STATUS.md` for the full checklist.

## 1. Problem Statement

> **Status:** The old system (`XmlComponentTrait` + `XmlComponentMap`) has been
> **removed**. The new system (`ComponentDef` + `ComponentMap`) has been upgraded
> with structured `ComponentFieldType` replacing the string-based `field_type: AzString`.
> The remaining gaps are: debug server JSON serialization (still flat strings),
> code generation (still uses string matching), and some `ComponentFieldValue` variants.

The component system had two parallel implementations, both inadequate
for the long-term vision of a GUI builder with live preview and multi-language
code generation:

### Old system (`XmlComponentTrait` + `XmlComponentMap`) — ✅ REMOVED

> These types have been removed from the codebase.

- Used `BTreeMap<String, XmlComponent>` — **not FFI-safe**
- Used `Box<dyn XmlComponentTrait>` — **not FFI-safe**
- `FilteredComponentArguments` stored values as `BTreeMap<String, String>` —
  everything was stringly-typed, no Option/Callback/Dom support
- `ComponentArgumentTypes = Vec<(String, String)>` — type was just a string name
- Tightly coupled to Rust; couldn't express types for C/Python code generation

### New system (`ComponentDef` + `ComponentMap`) — ✅ UPGRADED

> `ComponentDataField.field_type` is now `ComponentFieldType` (structured enum).
> `ComponentRenderFn`/`ComponentCompileFn` no longer reference the old types.
> Remaining gap: debug server serializes field_type as flat strings, not structured JSON.

- Correctly `#[repr(C)]` with `ComponentDefVec`, `ComponentLibraryVec`, etc.
- `ComponentDataField.field_type` is now `ComponentFieldType` — a structured
  `#[repr(C, u8)]` enum with 20 variants (String, Bool, I32, Option, Vec,
  StyledDom, Callback, RefAny, StructRef, EnumRef, etc.)
- Parameters and callback_slots merged into `data_model` — single unified list
- `ComponentDef` simplified to 8 fields: id, display_name, description, css,
  source, data_model, render_fn, compile_fn
- `template` field was considered but explicitly removed (source-edit-recompile
  workflow instead)
- `ChildPolicy` removed — child acceptance derived from data model shape
  (StyledDom field → children, text: String → text, neither → no children)

### What we still need

The structured `ComponentFieldType` exists in core types but the downstream
consumers haven't caught up:

1. ~~Supports primitives, Option, Vec, nested structs, enum types~~ ✅ Done
2. ~~Distinguishes StyledDom "child slots" from data fields~~ ✅ Done
3. ~~Represents callback types with full signatures (args + return type)~~ ✅ Done
4. Carries enough metadata for multi-language code generation — ⚠️ code gen still uses string matching
5. Enables the browser debugger to render type-appropriate editing controls — ⚠️ debugger still receives flat strings
6. ~~Supports default values per-type (not just strings)~~ ✅ Done (`ComponentDefaultValue`)
7. Is JSON-serializable for import/export and debugger communication — ⚠️ `field_type_to_string()` flattens to strings

---

## 2. Current Codebase Inventory

### 2.1 Core types (core/src/xml.rs) — CURRENT STATE

```
ComponentId          { collection: AzString, name: AzString }
ComponentDataField   { name, field_type: ComponentFieldType, default_value: OptionComponentDefaultValue, description, required }
ComponentDataModel   { name, description, fields: ComponentDataFieldVec }
ComponentFieldType   { String | Bool | I32 | ... | Option(Box) | Vec(Box) | StyledDom | Callback(sig) | RefAny | StructRef | EnumRef | ... }
ComponentCallbackSignature { return_type, args: ComponentCallbackArgVec }  // no type_name
ComponentCallbackArg { name, arg_type: ComponentFieldType }
ComponentEnumModel   { name, description, variants: ComponentEnumVariantVec }
ComponentEnumVariant { name, fields: ComponentDataFieldVec }  // no description field yet
ComponentDefaultValue { String | Bool | I32 | ... | None | ComponentInstance | CallbackFnPointer }
ComponentDef         { id, display_name, description, css, source, data_model, render_fn, compile_fn }
ComponentLibrary     { name, version, description, components, exportable, modifiable, data_models, enum_models }
ComponentMap         { libraries: ComponentLibraryVec }
```

> **Removed types** (dead, still in code, to be cleaned up):
> - `ComponentParam` — merged into data_model fields
> - `ComponentCallbackSlot` — now `ComponentFieldType::Callback(sig)` fields
> - `ChildPolicy` — child acceptance derived from data model shape
>
> **Removed fields** from `ComponentDef`:
> - `parameters`, `callback_slots`, `accepts_text`, `child_policy`, `example_xml`, `template`, `node_type`

### 2.2 Callback typedefs (api.json pattern)

The api.json `callback_typedef` format already describes function signatures:

```json
"ButtonOnClickCallbackType": {
    "callback_typedef": {
        "fn_args": [
            { "type": "RefAny" },
            { "type": "CallbackInfo" }
        ],
        "returns": { "type": "Update" }
    }
}
```

Widget-specific callbacks add extra args (the widget's state):

```json
"NumberInputOnValueChangeCallbackType": {
    "callback_typedef": {
        "fn_args": [
            { "type": "RefAny" },
            { "type": "CallbackInfo" },
            { "type": "NumberInputState" }
        ],
        "returns": { "type": "Update" }
    }
}
```

### 2.3 Debug server types (debug_server.rs) — CURRENT STATE

- `ComponentInfo { tag, qualified_name, display_name, description, source, data_model, universal_attributes, callback_slots, css }`
- `ComponentDataFieldInfo  { name, field_type: String, default, description }` — ⚠️ field_type is STILL a flat string
- `ComponentCallbackSlotInfo { name, callback_type: String, description }` — ⚠️ callback_type flattened
- `field_type_to_string()` at line ~4172 — flattens `ComponentFieldType` to strings like `"String"`, `"Option<String>"`, `"Callback"` (loses args info)
- CRUD endpoints: CreateLibrary, DeleteLibrary, CreateComponent, DeleteComponent, UpdateComponent — ✅ all implemented
- `GetComponentPreview` — ✅ fully implemented with CPU render pipeline (line ~8768)
- Code gen: `map_type_to_rust()`, `map_type_to_c()` — still switches on string names

### 2.4 Debugger browser UI (debugger.js) — CURRENT STATE

Shows component data model as a structured type view via `DataModelEditor` widget.
Has `_parseFieldType()` (line ~3081) that parses string representations client-side.
Callbacks shown in a separate table with type badges.
`PreviewPanel` widget loads component preview image via `get_component_preview` API.

Remaining gap: receives flat string field_type from server, parses client-side.
Should receive structured JSON from server instead.

---

## 3. Proposed Design: `ComponentFieldType` — ✅ IMPLEMENTED

> The types described in this section have been implemented in `core/src/xml.rs`.
> Minor deviations from the proposal are noted inline.

### 3.1 Core enum — ✅ IMPLEMENTED

```rust
/// Describes the type of a component data field.
/// This is the "type system" for component metadata — rich enough for
/// code generation, debugger UI rendering, and component composition.
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum ComponentFieldType {
    // ── Primitives ──────────────────────────────────────────────
    /// UTF-8 string (AzString in Rust, AzString in C, str in Python)
    String,
    /// Boolean
    Bool,
    /// Signed 32-bit integer
    I32,
    /// Signed 64-bit integer
    I64,
    /// Unsigned 32-bit integer
    U32,
    /// Unsigned 64-bit integer
    U64,
    /// Unsigned pointer-sized integer
    Usize,
    /// 32-bit float
    F32,
    /// 64-bit float
    F64,
    /// CSS color value (RGBA)
    ColorU,

    // ── Container types ─────────────────────────────────────────
    /// Optional value of inner type. None is a valid value.
    Option(Box<ComponentFieldType>),
    /// Ordered list/vector of inner type.
    Vec(Box<ComponentFieldType>),

    // ── DOM / child slot types ──────────────────────────────────
    /// A styled DOM subtree — used for "child slot" composition.
    /// In the debugger, this becomes a drag-and-drop target.
    /// In code gen, this becomes a `StyledDom` parameter.
    ///
    /// The slot name is derived from the `ComponentDataField.name`
    /// (no separate slot name needed — the field name IS the slot name).
    StyledDom,

    // ── Callback types ──────────────────────────────────────────
    /// A callback slot with a typed signature.
    /// Contains the full function signature so the debugger can
    /// show the expected handler shape, and code gen can emit
    /// correct function pointer types.
    Callback(ComponentCallbackSignature),

    // ── Data binding ────────────────────────────────────────────
    /// A type-erased data reference (RefAny). Used when a component
    /// needs to store/pass opaque application data.
    /// The AzString is the expected type name hint (e.g. "MyAppData")
    /// — purely informational, no runtime enforcement.
    RefAny(AzString),

    // ── Structured types ────────────────────────────────────────
    /// Reference to a named struct type defined in the same library's
    /// `data_models` list. The AzString is the type name.
    /// Enables nested/composed data models.
    StructRef(AzString),

    /// Reference to a named enum type defined in the same library's
    /// `enum_models` list. The AzString is the type name.
    EnumRef(AzString),

    // ── Azul-specific types ─────────────────────────────────────
    /// CSS property value (parsed from string)
    CssProperty,
    /// Image reference (ImageRef)
    ImageRef,
    /// Font reference (FontRef)
    FontRef,
}
```

### 3.2 Callback signature type — ✅ IMPLEMENTED (without `type_name`)

> **Deviation:** `type_name` was removed — `args` + `return_type` are sufficient.
> `extra_args` renamed to `args`. `description` was also removed (field-level
> description covers it).

```rust
/// Describes the full signature of a callback.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ComponentCallbackSignature {
    /// Function arguments (the "extra" args specific to this component,
    /// beyond the implicit &mut RefAny and &mut CallbackInfo).
    pub args: ComponentCallbackArgVec,
    /// Return type. Almost always "Update".
    pub return_type: AzString,
}

/// A single argument in a callback signature (beyond RefAny + CallbackInfo).
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ComponentCallbackArg {
    /// Argument name (for documentation and code gen)
    pub name: AzString,
    /// Argument type. Can be a ComponentFieldType for full recursion,
    /// or a StructRef to reference a known type.
    pub arg_type: ComponentFieldType,
}

impl_vec!(ComponentCallbackArg, ComponentCallbackArgVec, ...);
```

### 3.3 Enum model type — ✅ IMPLEMENTED

> **Deviation:** `ComponentEnumVariant.description` field is missing in the
> current implementation. To be added.

```rust
/// Defines an enum type in the component type system.
/// Used both for component variants (component states like
/// "loading" / "error" / "loaded") and for field-level enum types.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ComponentEnumModel {
    /// Type name (e.g. "ButtonVariant", "LoadState")
    pub name: AzString,
    /// Description
    pub description: AzString,
    /// Variants in this enum
    pub variants: ComponentEnumVariantVec,
}

/// A single variant in a component enum model.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ComponentEnumVariant {
    /// Variant name (e.g. "Primary", "Outline", "Ghost" for ButtonVariant)
    pub name: AzString,
    /// Description
    pub description: AzString,
    /// Associated data fields (for struct-like enum variants).
    /// Empty for unit variants.
    pub fields: ComponentDataFieldVec,
}

impl_vec!(ComponentEnumModel, ComponentEnumModelVec, ...);
impl_vec!(ComponentEnumVariant, ComponentEnumVariantVec, ...);
```

### 3.4 Updated `ComponentDataField` — ✅ IMPLEMENTED

```rust
/// A field in the component's data model — with structured type info.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ComponentDataField {
    /// Field name, e.g. "href", "counter", "on_click"
    pub name: AzString,
    /// Structured type descriptor (replaces the old string-based field_type)
    pub field_type: ComponentFieldType,
    /// Default value (JSON-encoded), or None
    pub default_value: OptionComponentDefaultValue,
    /// Human-readable description
    pub description: AzString,
    /// Whether this field is required (no default, must be provided)
    pub required: bool,
}
```

### 3.5 Typed default values — ✅ PARTIALLY IMPLEMENTED

> **Deviation:** `ComponentDefaultValue::Json(AzString)` variant is missing.
> `ComponentFieldValueSource::Literal` uses `AzString` instead of typed `ComponentFieldValue`.

```rust
/// A typed default value for a component field.
/// Replaces the old `OptionString` default (which was always a string).
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum ComponentDefaultValue {
    /// String default
    String(AzString),
    /// Boolean default
    Bool(bool),
    /// Integer default
    I32(i32),
    I64(i64),
    U32(u32),
    U64(u64),
    Usize(usize),
    /// Float default
    F32(f32),
    F64(f64),
    /// No value (used for Option fields where default is None)
    None,
    /// JSON-encoded default for complex types (structs, enums, vecs)
    Json(AzString),
    /// Default component instantiation for StyledDom slot fields.
    /// References a component by library + name (e.g. "builtin" + "a").
    /// The component is instantiated with ITS defaults to fill the slot.
    /// Syntax in type parser / JSON: `default: builtin.a` or
    /// `default: mylib.my_card`
    ComponentInstance(ComponentInstanceDefault),
    /// Default callback implementation — a named C function pointer.
    /// For compiled components: resolved at link time or via `dladdr()`.
    /// For dynamic/user-defined components: used as a code-gen marker
    /// (the function may not exist yet — it tells the code generator
    /// which function name to emit in the generated handler stub).
    /// The string is a fully qualified path, e.g. "my_crate::handlers::on_link_click".
    /// Callbacks may live in a different crate than the UI component
    /// (to separate business logic from presentation).
    CallbackFnPointer(AzString),
}

/// Identifies a component to instantiate as a default value for a StyledDom slot.
/// Field overrides are structured: each override specifies a field name and a
/// value source (literal value, binding to parent/app state, or use-default).
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ComponentInstanceDefault {
    /// Library name (e.g. "builtin", "shadcn")
    pub library: AzString,
    /// Component name within that library (e.g. "a", "div", "card")
    pub component: AzString,
    /// Overrides for the sub-component's data model fields.
    /// Fields not listed here use the sub-component's own defaults.
    pub field_overrides: ComponentFieldOverrideVec,
}

/// A single field override in a component instantiation.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ComponentFieldOverride {
    /// Name of the field being overridden (must match a field in the
    /// sub-component's data model)
    pub field_name: AzString,
    /// Where the value comes from
    pub source: ComponentFieldValueSource,
}

/// Describes where a field value comes from in a component instantiation.
/// This is the key type for the editor's data binding system:
/// - In the "component preview" view, fields typically use `Literal` or `Default`
/// - In the "main app view", fields typically use `Binding` to connect to
///   the application's data model (RefAny state)
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum ComponentFieldValueSource {
    /// Use the sub-component's own default value for this field.
    /// Displayed as collapsed/grayed-out in the editor.
    Default,
    /// Hardcoded literal value (editable inline in the editor).
    /// Example: `color: "green"`, `count: 42`
    Literal(ComponentFieldValue),
    /// Binding path to a field in the parent/application data model.
    /// Example: `"my_data_model.current_link"`, `"app_state.user.name"`
    /// The path uses dot notation for nested field access.
    /// In the editor, this is shown as a "connected" field with a
    /// link indicator to the source field.
    Binding(AzString),
}

impl_vec!(ComponentFieldOverride, ComponentFieldOverrideVec, ...);
impl_option!(ComponentDefaultValue, OptionComponentDefaultValue, ...);
```

### 3.6 Updated `ComponentLibrary` — ✅ IMPLEMENTED

> Matches the proposal. `enum_models` added.

```rust
pub struct ComponentLibrary {
    pub name: AzString,
    pub version: AzString,
    pub description: AzString,
    pub components: ComponentDefVec,
    pub exportable: bool,
    pub modifiable: bool,
    /// Auxiliary / shared struct types that components reference via StructRef.
    /// Each component also has its OWN main data model struct in
    /// `ComponentDef.data_model` — that one is NOT duplicated here.
    /// This list is for types shared across multiple components,
    /// e.g. "UserProfile" used by both UserCard and UserList.
    pub data_models: ComponentDataModelVec,
    /// Named enum types defined by this library (NEW)
    pub enum_models: ComponentEnumModelVec,
}
```

### 3.7 Main data model vs auxiliary data models

The distinction:

- **`ComponentDef.data_model: ComponentDataModel`** — the component's own
  "main" struct. Has a name (e.g. `"ButtonData"`), which code gen uses as
  the struct name. Contains all the component's inputs: value fields,
  child slots, callbacks. Code gen emits this as the component's primary
  input type. Live preview instantiates this with default values.

- **`ComponentLibrary.data_models: ComponentDataModelVec`** — shared/auxiliary
  struct types that components reference via `ComponentFieldType::StructRef("...")`.
  These are generated as separate structs. Example: a `UserProfile` struct
  used by both `UserCard` and `UserList` components.

Code gen workflow for a component:
1. Look up `component.data_model` → emit `struct ButtonData { ... }`
2. For each field with `StructRef("Foo")` → look up `Foo` in
   `library.data_models` → emit `struct Foo { ... }`
3. For each field with `EnumRef("Bar")` → look up `Bar` in
   `library.enum_models` → emit `enum Bar { ... }`
4. For each field with `Callback(sig)` → emit callback type + stub

Live preview workflow:
1. Instantiate `component.data_model` with all fields set to defaults
2. For `StructRef` fields → recursively instantiate the referenced model
3. For `EnumRef` fields → use the default variant
4. For `StyledDom` slots → instantiate `ComponentInstance` default if set,
   otherwise empty StyledDom (or placeholder)
5. For `Callback` fields → no-op handler
6. Call `render_fn` with the instantiated data model

---

## 4. Child Slot System (StyledDom composition)

### 4.1 Concept

A component declares "slots" where parent-provided `StyledDom` subtrees can be
plugged in. This enables drag-and-drop composition in the debugger.

**Example — a Card component with 3 slots:**

```rust
ComponentDataField {
    name: "header",
    field_type: ComponentFieldType::StyledDom,
    default_value: None,   // slot is optional, renders nothing if empty
    description: "Card header area",
    required: false,
},
ComponentDataField {
    name: "content",
    field_type: ComponentFieldType::StyledDom,
    default_value: None,
    description: "Main card content",
    required: true,
},
ComponentDataField {
    name: "footer",
    field_type: ComponentFieldType::StyledDom,
    default_value: None,
    description: "Card footer area",
    required: false,
},
```

### 4.2 Template syntax — ❌ REMOVED BY DESIGN

> The `template` field and XML `<slot>` syntax were considered but explicitly removed.
> Components define their rendering in source code (Rust/C/Python), not XML templates.
> The workflow is: edit source → recompile → hot-reload.
> The `<slot>` concept is replaced by `StyledDom` fields in the data model.

Original proposal (for reference only — NOT IMPLEMENTED):

```xml
<component name="card">
    <div class="card">
        <div class="card-header"><slot name="header"/></div>
        <div class="card-body"><slot name="content"/></div>
        <div class="card-footer"><slot name="footer"/></div>
    </div>
</component>
```

### 4.3 Usage in XML

```xml
<card>
    <slot:header>
        <h2>My Card Title</h2>
    </slot:header>
    <slot:content>
        <p>Some content here</p>
    </slot:content>
</card>
```

### 4.4 Debugger UX

In the debugger, a `StyledDom` field renders as:
- A bordered drop zone with the slot name as label
- Drag components from the registry into the slot
- Or type XML directly  
- Visual indicator: filled/empty state

### 4.5 Code gen

**Rust:**
```rust
pub struct CardData {
    pub header: Option<StyledDom>,
    pub content: StyledDom,
    pub footer: Option<StyledDom>,
}
```

**C:**
```c
typedef struct {
    AzOptionStyledDom header;
    AzStyledDom content;
    AzOptionStyledDom footer;
} CardData;
```

---

## 5. Callback Type Advertising

### 5.1 Concept

Components advertise their callbacks as fully-typed slots. This replaces the
current approach where `callback_type: AzString` is just a name.

**Example — a Link component:**

```rust
ComponentDataField {
    name: "onclick",
    field_type: ComponentFieldType::Callback(ComponentCallbackSignature {
        args: vec![
            ComponentCallbackArg {
                name: "link_url".into(),
                arg_type: ComponentFieldType::String,
            },
        ].into(),
        return_type: "Update".into(),
    }),
    default_value: None,
    description: "Click handler".into(),
    required: false,
},
```

### 5.2 Merging data_model and callback_slots — ✅ DONE

The separate `parameters` and `callback_slots` fields have been removed from
`ComponentDef`. All inputs are unified in `data_model`.

**Current `ComponentDef`** (8 fields):

```rust
pub struct ComponentDef {
    pub id: ComponentId,
    pub display_name: AzString,
    pub description: AzString,
    pub css: AzString,                   // was scoped_css
    pub source: ComponentSource,
    pub data_model: ComponentDataModel,
    pub render_fn: ComponentRenderFn,
    pub compile_fn: ComponentCompileFn,
}
```

Removed fields: `parameters`, `callback_slots`, `accepts_text`, `child_policy`,
`example_xml`, `template`, `node_type`. Child acceptance is now derived from
data model shape (StyledDom field = children, text: String = text, neither = no children).

### 5.3 Debugger rendering

The debugger groups data model fields by type:

```
┌─ Link ──────────────────────────────────────────┐
│                                                  │
│  Value Fields (2)                                │
│  ┌──────────────────────────────────────────┐    │
│  │ href: String = ""     // URL link target │    │
│  │ target: String = ""   // _blank, _self   │    │
│  └──────────────────────────────────────────┘    │
│                                                  │
│  Child Slots (1)                                 │
│  ┌──────────────────────────────────────────┐    │
│  │ ┌────────────────────────────────────┐   │    │
│  │ │  content: StyledDom               │   │    │
│  │ │  [drag components here]           │   │    │
│  │ └────────────────────────────────────┘   │    │
│  └──────────────────────────────────────────┘    │
│                                                  │
│  Callbacks (1)                                   │
│  ┌──────────────────────────────────────────┐    │
│  │ onclick: fn(&mut RefAny,               │    │
│  │              &mut CallbackInfo,         │    │
│  │              link_url: String)          │    │
│  │           -> Update                     │    │
│  │                                         │    │
│  │ [Generate handler stub]                 │    │
│  └──────────────────────────────────────────┘    │
│                                                  │
└──────────────────────────────────────────────────┘
```

### 5.4 Default handler generation

Callbacks can have default implementations specified in three ways:

**1. No default (generates a TODO stub):**
```rust
extern "C" fn on_link_click_default(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    link_url: AzString,
) -> Update {
    // TODO: handle link click
    Update::DoNothing
}
```

**2. Named function pointer default (`CallbackFnPointer`):**

A component definition can specify a function name as the default handler:
```rust
ComponentDataField {
    name: "on_click",
    field_type: ComponentFieldType::Callback(ComponentCallbackSignature { .. }),
    default_value: OptionComponentDefaultValue::Some(
        ComponentDefaultValue::CallbackFnPointer(
            "my_nav_crate::handlers::navigate_to_href".into()
        ),
    ),
    ...
}
```

For **compiled components**, `dladdr()` (or equivalent) can resolve this
function name to the actual function pointer at runtime. The function is
already linked into the binary — the name is a reverse-lookup key.

For **dynamic/user-defined components**, the function may not exist yet.
The name serves as a **code generation marker**: when the user compiles
the dynamic component, the code generator emits:
```rust
use my_nav_crate::handlers::navigate_to_href;
```

This supports **separating business logic from UI**: callback implementations
live in a different crate (e.g. `my_nav_crate::handlers`) from the component
definitions (e.g. `my_ui_crate::components`). The component only knows the
function's name and signature — not its implementation.

### 5.5 Compiled vs user-defined: editability rules

Not all components are equal in the debugger:

| Source        | Data model fields | Callback signatures | Template/CSS |
|---------------|-------------------|--------------------|--------------|
| `Builtin`     | read-only         | read-only          | read-only    |
| `Compiled`    | read-only         | read-only          | read-only    |
| `UserDefined` | **editable**      | **editable**       | **editable** |

**Compiled components** (like `Button`, `TextInput`, `NumberInput`) have their
callback typedefs and data model hardcoded in Rust source. The debugger
**displays** them (so users can see e.g. that `NumberInput` fires
`on_value_change(NumberInputState) -> Update`) but they **cannot be edited**
at runtime. The signature is part of the compiled binary.

**User-defined components** (created in the debugger or imported via JSON)
have fully editable data models — users can add/remove/rename fields,
change types, add callback slots, etc. Their type definitions are stored
as `ComponentDataModel` / `ComponentEnumModel` data, not as compiled code.

The debugger UI should reflect this:
- Compiled callbacks: show signature as read-only monospace block
- User-defined callbacks: show editable form (name, args, return type)
- Compiled data model fields: show as read-only badges
- User-defined data model fields: show with edit/delete/reorder controls

---

## 6. Component Variant System (Enum States)

### 6.1 Concept

Some components have multiple "modes" or "variants" that change their rendering.
Instead of modeling this as a string enum field, we use `ComponentEnumModel` to
define the variants with full type info, enabling the debugger to show a dropdown
and the code generator to emit a proper Rust/C enum.

**Example — a Button component:**

```rust
// Define the enum in the library
ComponentEnumModel {
    name: "ButtonVariant".into(),
    description: "Visual style of the button".into(),
    variants: vec![
        ComponentEnumVariant {
            name: "Default".into(),
            description: "Standard button".into(),
            fields: vec![].into(),  // unit variant
        },
        ComponentEnumVariant {
            name: "Outline".into(),
            description: "Outlined button".into(),
            fields: vec![].into(),
        },
        ComponentEnumVariant {
            name: "Ghost".into(),
            description: "Minimal/ghost button".into(),
            fields: vec![].into(),
        },
        ComponentEnumVariant {
            name: "Destructive".into(),
            description: "Destructive action button".into(),
            fields: vec![].into(),
        },
    ].into(),
}
```

In the component's data model:
```rust
ComponentDataField {
    name: "variant",
    field_type: ComponentFieldType::EnumRef("ButtonVariant".into()),
    default_value: OptionComponentDefaultValue::Some(
        ComponentDefaultValue::String("Default".into()),
    ),
    description: "Visual style variant".into(),
    required: false,
}
```

### 6.2 Enum variants with data

For richer use cases (e.g., a data fetch component):

```rust
ComponentEnumModel {
    name: "FetchState".into(),
    description: "State of an async data fetch".into(),
    variants: vec![
        ComponentEnumVariant {
            name: "Loading".into(),
            description: "Data is being fetched".into(),
            fields: vec![].into(),
        },
        ComponentEnumVariant {
            name: "Error".into(),
            description: "Fetch failed".into(),
            fields: vec![
                ComponentDataField {
                    name: "message".into(),
                    field_type: ComponentFieldType::String,
                    default_value: None.into(),
                    description: "Error message".into(),
                    required: true,
                },
            ].into(),
        },
        ComponentEnumVariant {
            name: "Loaded".into(),
            description: "Data loaded successfully".into(),
            fields: vec![
                ComponentDataField {
                    name: "data".into(),
                    field_type: ComponentFieldType::RefAny("FetchResult".into()),
                    default_value: None.into(),
                    description: "The fetched data".into(),
                    required: true,
                },
            ].into(),
        },
    ].into(),
}
```

### 6.3 Code gen for enum variants

**Rust:**
```rust
#[derive(Debug, Clone)]
pub enum FetchState {
    Loading,
    Error { message: String },
    Loaded { data: RefAny },
}
```

**C:**
```c
typedef enum { FetchState_Loading, FetchState_Error, FetchState_Loaded } FetchStateTag;
typedef struct { AzString message; } FetchState_ErrorVariant;
typedef struct { AzRefAny data; } FetchState_LoadedVariant;
typedef struct {
    FetchStateTag tag;
    union {
        FetchState_ErrorVariant error;
        FetchState_LoadedVariant loaded;
    } payload;
} FetchState;
```

---

## 7. Type Definition String Parser

User-defined components need a way to define their data model from a string —
e.g. when typing in the debugger's "add field" dialog, or in XML `args` attributes,
or in JSON import. We need a simple parser that converts a human-readable type
string into a `ComponentFieldType`.

### 7.1 Syntax

The syntax is deliberately simpler than Rust — no lifetimes, no generics angle
brackets (use `[]` for Vec, `?` suffix for Option), no `&` references:

```
Primitive types:
    String  Bool  i32  i64  u32  u64  usize  f32  f64  ColorU

Option (nullable):
    String?          → ComponentFieldType::Option(Box(String))
    i32?             → ComponentFieldType::Option(Box(I32))
    UserProfile?     → ComponentFieldType::Option(Box(StructRef("UserProfile")))

Vec (list):
    [String]         → ComponentFieldType::Vec(Box(String))
    [i32]            → ComponentFieldType::Vec(Box(I32))
    [UserProfile]    → ComponentFieldType::Vec(Box(StructRef("UserProfile")))
    [String]?        → ComponentFieldType::Option(Box(Vec(Box(String))))

Child slots (slot name = field name, set by the ComponentDataField.name):
    slot             → ComponentFieldType::StyledDom
    slot?            → ComponentFieldType::Option(Box(StyledDom))

    Default component instance for slots:
    default: builtin.div       → ComponentDefaultValue::ComponentInstance { "builtin", "div" }
    default: builtin.a         → ComponentDefaultValue::ComponentInstance { "builtin", "a" }
    default: mylib.user_card   → ComponentDefaultValue::ComponentInstance { "mylib", "user_card" }

Callbacks:
    fn() -> Update                    → Callback with no extra args
    fn(String) -> Update              → Callback with 1 extra arg
    fn(i32, String) -> Update         → Callback with 2 extra args
    fn(NumberInputState) -> Update    → Callback referencing a struct arg

Data binding:
    RefAny                            → ComponentFieldType::RefAny("")
    RefAny(MyAppData)                 → ComponentFieldType::RefAny("MyAppData")

Struct / enum references (any unknown identifier):
    UserProfile      → ComponentFieldType::StructRef("UserProfile")
    ButtonVariant    → ComponentFieldType::EnumRef("ButtonVariant")
                       (resolved by checking library.enum_models first,
                        then library.data_models)

Azul-specific:
    CssProperty  ImageRef  FontRef
```

### 7.2 Grammar (PEG-style)

```peg
type       ← callback / container / primitive / azul_type / slot / refany / ident

callback   ← 'fn' '(' arg_list ')' '->' ident
arg_list   ← (type (',' type)*)?

container  ← '[' type ']' '?'?       # Vec, optionally nullable
           / type '?'                  # Option

primitive  ← 'String' / 'Bool' / 'bool' / 'i32' / 'i64' / 'u32' / 'u64'
           / 'usize' / 'f32' / 'f64' / 'ColorU'

azul_type  ← 'CssProperty' / 'ImageRef' / 'FontRef'

slot       ← 'slot' '?'?

refany     ← 'RefAny' ('(' ident ')')?

ident      ← [A-Z][a-zA-Z0-9_]*      # PascalCase = struct or enum ref
```

### 7.3 Parser function signature

```rust
/// Parse a type definition string into a ComponentFieldType.
///
/// Uses the library's data_models/enum_models to resolve
/// ambiguous identifiers (struct vs enum).
///
/// Returns Err for syntax errors with position info.
pub fn parse_field_type(
    input: &str,
    library: &ComponentLibrary,
) -> Result<ComponentFieldType, TypeParseError> { ... }

/// Format a ComponentFieldType back to the human-readable string syntax.
/// Round-trips with parse_field_type.
pub fn format_field_type(
    field_type: &ComponentFieldType,
) -> String { ... }

#[derive(Debug, Clone)]
pub struct TypeParseError {
    pub message: AzString,
    pub position: usize,
    pub input: AzString,
}
```

### 7.4 Usage examples

**Debugger "add field" dialog:**
```
┌─ Add Field ─────────────────────────────────────────┐
│  Name:  [on_value_change          ]                 │
│  Type:  [fn(NumberInputState) -> Update  ]          │
│         ✓ Parsed: Callback(NumberInputOnChange...)  │
│  Desc:  [Called when the value changes   ]          │
│  [Add]                                              │
└─────────────────────────────────────────────────────┘
```

**XML component definition:**
```xml
<component name="user-card" args="name: String, email: String?, avatar: ImageRef?, on_edit: fn(String) -> Update">
    <div class="card">
        <img src="{{ avatar }}" />
        <h3>{{ name }}</h3>
        <p>{{ email }}</p>
    </div>
</component>
```

**JSON import (string shorthand):**
```json
{
    "name": "email",
    "type": "String?",
    "description": "User email address"
}
```

The JSON format supports both the full structured `field_type` object
(section 8.1) and this string shorthand. The parser is used to expand
the shorthand into the full `ComponentFieldType`.

### 7.5 Compiled components bypass the parser

Compiled components (Button, TextInput, etc.) construct their
`ComponentFieldType` values directly in Rust code — they never go
through the string parser. The parser is only for:
- User-defined components in the debugger
- XML `args` attribute parsing
- JSON import with string-shorthand types

The `format_field_type()` function is used to display compiled
component types as readable strings in the debugger — but the
resulting string is **read-only** for compiled components.

---

## 8. JSON Serialization Format — ⚠️ NOT YET IMPLEMENTED

> The debug server currently uses `field_type_to_string()` which flattens
> `ComponentFieldType` to strings. The structured JSON format described here
> is the TARGET format. See ACTION_PLAN.md for implementation steps.

For the debugger API and import/export, the type system needs a JSON representation:

### 8.1 ComponentFieldType → JSON

```json
// Primitives
{ "type": "String" }
{ "type": "Bool" }
{ "type": "I32" }
{ "type": "F32" }

// Containers
{ "type": "Option", "inner": { "type": "String" } }
{ "type": "Vec", "inner": { "type": "I32" } }

// Child slot (slot name = field name)
{ "type": "StyledDom" }

// Callback
{
    "type": "Callback",
    "args": [
        { "name": "link_url", "type": { "type": "String" } }
    ],
    "return_type": "Update"
}

// Data binding
{ "type": "RefAny", "type_hint": "MyAppData" }

// Struct reference
{ "type": "StructRef", "name": "UserProfile" }

// Enum reference
{ "type": "EnumRef", "name": "ButtonVariant" }

// Azul-specific
{ "type": "ColorU" }
{ "type": "CssProperty" }
{ "type": "ImageRef" }
{ "type": "FontRef" }

// Default: component instance (for slot defaults)
{ "type": "ComponentInstance", "library": "builtin", "component": "a",
  "overrides": [
      { "field": "href", "source": "literal", "value": "https://example.com" },
      { "field": "color", "source": "default" }
  ] }

// Default: callback function pointer
{ "type": "CallbackFnPointer", "fn_name": "my_crate::handlers::on_click" }

// Field value source (used in overrides and data binding view)
{ "source": "default" }                              // use sub-component's default
{ "source": "literal", "value": "green" }             // hardcoded literal
{ "source": "binding", "path": "app_state.user.name" } // bound to app data model
```

### 8.2 ComponentDataField → JSON

```json
{
    "name": "href",
    "field_type": { "type": "Option", "inner": { "type": "String" } },
    "default": null,
    "description": "URL the link points to",
    "required": false
}
```

### 8.3 Full component example (Link)

```json
{
    "name": "link",
    "display_name": "Link",
    "description": "Clickable hyperlink element",
    "data_model": {
        "name": "LinkData",
        "description": "Input data for the Link component",
        "fields": [
            {
                "name": "href",
                "field_type": { "type": "Option", "inner": { "type": "String" } },
                "default": null,
                "description": "URL to navigate to",
                "required": false
            },
            {
                "name": "target",
                "field_type": { "type": "Option", "inner": { "type": "String" } },
                "default": null,
                "description": "Link target (_blank, _self, etc.)",
                "required": false
            },
            {
                "name": "content",
                "field_type": { "type": "StyledDom" },
                "default": { "type": "ComponentInstance", "library": "builtin", "component": "span" },
                "description": "Link content (text, icon, etc.)",
                "required": false
            },
            {
                "name": "onclick",
                "field_type": {
                    "type": "Callback",
                    "args": [
                        { "name": "link_url", "type": { "type": "String" } }
                    ],
                    "return_type": "Update"
                },
                "default": null,
                "description": "Click callback",
                "required": false
            }
        ]
    },
    "accepts_text": true,
    "source": "builtin"
}
```

---

## 9. FFI Considerations

### 9.1 `#[repr(C)]` for `ComponentFieldType`

The enum uses `#[repr(C, u8)]` and all heap allocations go through
FFI-safe wrapper types:

- `Box<ComponentFieldType>` → not directly representable in C.
  **Solution**: use a heap-allocated indirection type:

```rust
/// Heap-allocated ComponentFieldType for recursive type references.
/// Needed because C cannot have recursive enum/struct definitions
/// directly — we box the inner type.
#[repr(C)]
pub struct ComponentFieldTypeBox {
    pub ptr: *mut ComponentFieldType,
}
```

Or, alternatively, flatten the recursion via an enum + separate "inner" field
(which avoids the Box entirely):

```rust
/// Non-recursive representation of ComponentFieldType.
/// "Option" and "Vec" carry their inner type in a separate
/// heap-allocated ComponentFieldType accessed via OptionComponentFieldTypeBox.
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum ComponentFieldType {
    String,
    Bool,
    I32,
    /* ... primitives ... */
    Option { inner: ComponentFieldTypeBox },
    Vec { inner: ComponentFieldTypeBox },
    StyledDom,
    Callback { signature: ComponentCallbackSignature },
    RefAny { type_hint: AzString },
    StructRef { name: AzString },
    EnumRef { name: AzString },
    /* ... */
}
```

The `ComponentFieldTypeBox` would be managed via FFI functions:
```rust
extern "C" fn az_component_field_type_box_new(t: ComponentFieldType) -> ComponentFieldTypeBox;
extern "C" fn az_component_field_type_box_delete(b: &mut ComponentFieldTypeBox);
```

### 9.2 api.json integration

New types to add to api.json:

```
module "component":
    ComponentFieldType       — enum (C, u8) with all variants
    ComponentFieldTypeBox    — struct { ptr: *mut ComponentFieldType }
    ComponentCallbackSignature — struct
    ComponentCallbackArg     — struct
    ComponentEnumModel       — struct
    ComponentEnumVariant     — struct
    ComponentDefaultValue    — enum (C, u8)
    ComponentInstanceDefault — struct { library, component, field_overrides }
    ComponentFieldOverride   — struct { field_name, source }
    ComponentFieldValueSource — enum (C, u8): Default | Literal | Binding
    ComponentFieldValue      — enum (C, u8): runtime value type
    ComponentFieldNamedValue — struct { name, value }
```

The codegen system already handles `#[repr(C, u8)]` enums with associated
data, so all of these should work through the existing pipeline.

### 9.3 Removing old types from api.json

Once the new system is in place, remove:
- `ComponentArgumentTypes` (type alias `Vec<(String, String)>`)
- `ComponentArgumentName` (type alias `String`)
- `ComponentArgumentType` (type alias `String`)
- `FilteredComponentArguments` (struct with BTreeMap — not FFI safe)
- `XmlComponentMap` (struct with BTreeMap — not FFI safe)
- `XmlComponent` (has `Box<dyn>`)

And update `ComponentRenderFn` / `ComponentCompileFn` to no longer reference
`XmlComponentMap` or `FilteredComponentArguments`.

---

## 10. Graph-Based Composition Model

### 10.1 Concept

In a GUI builder, components form a tree (the DOM). But data flow is not
strictly tree-shaped: a component deep in the tree might need data from the
app's root state. The component type system should describe **how a component
connects to the application's data model**.

### 10.2 Three connection patterns

1. **Props down (value fields)**: Parent passes data to child via data model
   fields. The child's data model declares what it needs. This is the simple
   case — a `String` field like `href` gets its value from the parent's scope.

2. **Events up (callbacks)**: Child component fires a callback that the parent
   handles. The callback signature is declared in the component's data model
   via `ComponentFieldType::Callback(...)`.

3. **Data binding (RefAny)**: A component receives a `RefAny` that wraps the
   app's state (or a subset of it). The component can read/mutate this data.
   `ComponentFieldType::RefAny("MyAppData")` declares this binding.

### 10.3 Debugger data flow visualization

The debugger can visualize these connections:

```
┌─────────────────────────────────────────────────────────────┐
│  AppData (RefAny)                                           │
│  ├── user: UserProfile                                      │
│  └── theme: Theme                                           │
│                                                             │
│  ┌── MainLayout ─────────────────────────────────────────┐  │
│  │   data_binding: RefAny → AppData                      │  │
│  │                                                       │  │
│  │   ┌── UserCard ─────────────────────────┐             │  │
│  │   │   user: UserProfile ← AppData.user  │             │  │
│  │   │   on_edit: Callback → handler_fn    │             │  │
│  │   │                                     │             │  │
│  │   │   ┌── Avatar ─────────────┐         │             │  │
│  │   │   │   src: String         │         │             │  │
│  │   │   │   alt: String         │         │             │  │
│  │   │   └───────────────────────┘         │             │  │
│  │   └─────────────────────────────────────┘             │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### 10.4 Implementation via data model fields

No special "graph" data structure is needed. The component's data model fields
already describe the connection:

- `StructRef("UserProfile")` → "I need a UserProfile from somewhere above me"
- `RefAny("AppData")` → "I need the app's main data, type-hint: AppData"
- `Callback(...)` → "I fire this event for someone above me to handle"
- `StyledDom` field → "I accept children in this slot (slot name = field name)"

The debugger can inspect the type graph by traversing data model fields
of all instantiated components and drawing the connections.

---

## 11. Code Generation Updates

### 11.1 Current code gen flow

```
ComponentDef
  → ScaffoldComponentInfo { data_fields: Vec<(name, type_string, default_string)> }
    → map_type_to_rust(type_string) → "String" | "bool" | "i32" | ...
      → template string interpolation
```

### 11.2 New code gen flow

```
ComponentDef
  → data_model: Vec<ComponentDataField>
    → for each field:
        match field.field_type {
            String => "String" / "AzString" / "str"
            Bool => "bool"
            I32 => "i32"
            Option { inner } => format!("Option<{}>", gen(inner))
            Vec { inner } => format!("Vec<{}>", gen(inner))
            StyledDom => "StyledDom" / "AzStyledDom"
            Callback { sig } => generate_callback_typedef(sig)
            RefAny { .. } => "RefAny" / "AzRefAny"
            StructRef { name } => name  (lookup in library.data_models)
            EnumRef { name } => name    (lookup in library.enum_models)
            ColorU => "ColorU" / "AzColorU"
            CssProperty => "CssProperty" / "AzCssProperty"
            ImageRef => "ImageRef" / "AzImageRef"
            FontRef => "FontRef" / "AzFontRef"
        }
```

### 11.3 Example: Rust scaffold for a UserCard component

```rust
/// Auto-generated data model for UserCard
#[derive(Debug, Clone)]
pub struct UserCardData {
    pub name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub role: UserRole,                          // EnumRef
    pub header_slot: Option<StyledDom>,          // StyledDom slot
    pub on_edit: Option<UserCardOnEditCallback>,  // Callback
}

/// Auto-generated callback type
pub type UserCardOnEditCallbackType = extern "C" fn(
    &mut RefAny,
    &mut CallbackInfo,
    user_id: AzString,
) -> Update;

/// Callback wrapper
pub struct UserCardOnEditCallback {
    pub cb: UserCardOnEditCallbackType,
    pub ctx: OptionRefAny,
}

/// Auto-generated enum
#[derive(Debug, Clone, PartialEq)]
pub enum UserRole {
    Admin,
    Editor,
    Viewer,
}
```

### 11.4 Example: C scaffold

```c
typedef enum { UserRole_Admin, UserRole_Editor, UserRole_Viewer } UserRole;

typedef AzUpdate (*UserCardOnEditCallbackType)(
    AzRefAny* data,
    AzCallbackInfo* info,
    AzString user_id
);

typedef struct {
    UserCardOnEditCallbackType cb;
    AzOptionRefAny ctx;
} UserCardOnEditCallback;

typedef struct {
    AzString name;
    AzOptionString email;
    AzOptionString avatar_url;
    UserRole role;
    AzOptionStyledDom header_slot;
    /* Option<callback> — use NULL cb for "not set" */
    UserCardOnEditCallback on_edit;
} UserCardData;
```

---

## 12. Migration Plan

### Phase 1: Add new types (non-breaking) — ✅ DONE

1. ✅ Define `ComponentFieldType`, `ComponentCallbackSignature`, `ComponentCallbackArg`,
   `ComponentEnumModel`, `ComponentEnumVariant`, `ComponentDefaultValue`,
   `ComponentFieldTypeBox`, `ComponentInstanceDefault`, `ComponentFieldOverride`,
   `ComponentFieldValueSource`, `ComponentFieldValue`, `ComponentFieldNamedValue`
   in `core/src/xml.rs`
2. ✅ Add `impl_vec!`, `impl_option!` wrappers for all new types
3. ⚠️ Add new types to `api.json` — partially done
4. ✅ Run codegen to verify FFI safety

### Phase 2: Migrate ComponentDataField — ✅ DONE

1. Change `ComponentDataField.field_type` from `AzString` to `ComponentFieldType`
2. Change `ComponentDataField.default_value` from `OptionString` to
   `OptionComponentDefaultValue`
3. Add `required: bool` field
4. Update `builtin_data_model()` to construct `ComponentFieldType::String` etc.
   instead of `AzString::from("String")`
5. Update `data_field()` helper

### Phase 3: Unify parameters and callback_slots into data_model — ✅ DONE

1. Remove `ComponentDef.parameters` — migrate existing params into `data_model`
2. Remove `ComponentDef.callback_slots` — migrate existing callbacks into
   `data_model` as `ComponentFieldType::Callback(...)` fields
3. Update all callers

### Phase 4: Update ComponentRenderFn / ComponentCompileFn — ⚠️ PARTIAL

> `ComponentRenderFn`/`ComponentCompileFn` no longer reference old types.
> However, the signature hasn't fully migrated to `ComponentFieldNamedValueVec` yet.
> `user_defined_render_fn` and `user_defined_compile_fn` are stubs.

1. Change `ComponentRenderFn` signature:
   ```rust
   pub type ComponentRenderFn = fn(
       &ComponentDef,
       &ComponentMap,               // was &XmlComponentMap
       &ComponentFieldValueVec,     // actual field VALUES, not type defs
       &OptionString,               // text content
   ) -> Result<StyledDom, RenderDomError>;
   ```
   Note: `ComponentFieldValueVec` carries actual runtime values per field,
   not the type definitions. See section 14 (Design Analysis) for the
   `ComponentFieldValue` type.
2. Change `ComponentCompileFn` similarly
3. Update `builtin_render_fn`, `builtin_compile_fn`
4. Update `build_exported_code()` in debug_server.rs

### Phase 5: Update debug server + browser UI — ⚠️ NOT DONE

> This is the main remaining gap. `field_type_to_string()` at debug_server.rs:4172
> flattens `ComponentFieldType` to strings. The debugger receives strings and
> re-parses them client-side via `_parseFieldType()`. Should instead serialize
> as structured JSON per §8.1.

1. Change `ComponentDataFieldInfo.field_type` from `String` to structured JSON
2. Update `build_component_registry()` to serialize `ComponentFieldType` → JSON
3. Update `ExportedDataField` / `ExportedCallbackSlot` to use structured types
4. Update `showComponentDetail()` in debugger.js to render type-appropriate
   controls (dropdowns for enums, checkboxes for bools, drop zones for slots)
5. Update code gen functions (`generate_scaffold`, `map_type_to_rust`, etc.)

### Phase 6: Remove old system — ✅ MOSTLY DONE

1. ✅ Remove `XmlComponentTrait`, `XmlComponent`, `XmlComponentMap`
2. ✅ Remove `FilteredComponentArguments`, `ComponentArguments`
3. ✅ Remove `ComponentArgumentTypes`, `ComponentArgumentName`, `ComponentArgumentType`
4. ✅ Remove all `*Renderer` structs (`DivRenderer`, `BodyRenderer`, etc.)
5. ✅ Remove `html_component!` macro
6. ⚠️ Clean up api.json — partial
7. 🗑️ Dead types to remove: `ComponentParam`, `ComponentCallbackSlot`, `ChildPolicy` + module_map entries

---

## 13. Open Questions

1. **Recursive `ComponentFieldType` in FFI**: Should we use `ComponentFieldTypeBox`
   (raw pointer indirection) or limit nesting depth (e.g., no `Option<Option<T>>`,
   no `Vec<Vec<T>>`)? The `Box` approach is more general but adds FFI complexity.
   A nesting depth limit of 1-2 would cover all practical cases.

2. **Polymorphic StructRef resolution**: When a component uses `StructRef("UserProfile")`,
   should the type be resolved within the same library only, or should there be
   cross-library type references? Same-library-only is simpler but `ComponentInstance`
   defaults already use `library.component` syntax — consider aligning with
   `StructRef("library.TypeName")` for cross-library refs.

3. **Callback default handlers**: **Resolved.** Components can include
   `ComponentDefaultValue::CallbackFnPointer("crate::module::fn_name")` as the
   default value for a callback field. For compiled components, `dladdr()` resolves
   the name to a function pointer. For dynamic components, it's a code-gen marker.
   This enables separating logic from UI (different crates). See section 5.4.

---

## 14. Design Consistency Analysis

This section documents issues found during review and their resolutions.

### 14.1 Resolved: StyledDom slot name redundancy

**Problem**: Originally `ComponentFieldType::StyledDom(AzString)` carried a
separate slot name. But the `ComponentDataField.name` already serves as the
slot identifier. Two names that could diverge.

**Resolution**: Removed the `AzString` — `StyledDom` is now a unit variant.
The field name IS the slot name. Template `<slot name="header"/>` matches
the field named `"header"`.

### 14.2 Resolved: Callback description duplication

**Problem**: `ComponentCallbackSignature` had a `description` field, but
the enclosing `ComponentDataField` also has `description`. Two places to
write the same text.

**Resolution**: Removed `description` from `ComponentCallbackSignature`.
The field-level description covers it.

### 14.3 Resolved: Runtime values vs type definitions

**Problem**: The `ComponentRenderFn` signature in Phase 4 originally passed
`&ComponentDataModel` — but that's the TYPE DEFINITION (field names + types),
not the actual VALUES. The old system passed `&FilteredComponentArguments`
which had `values: BTreeMap<String, String>`. The new render function also
needs actual values.

**Resolution**: Introduced `ComponentFieldValue` (runtime value type) and
`ComponentFieldValueVec`. The render function receives actual values:

```rust
/// A runtime value for a component field — the "instance" counterpart
/// to `ComponentFieldType` (which is the "class" / type descriptor).
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum ComponentFieldValue {
    String(AzString),
    Bool(bool),
    I32(i32),
    I64(i64),
    U32(u32),
    U64(u64),
    Usize(usize),
    F32(f32),
    F64(f64),
    ColorU(ColorU),
    None,                          // for Option<T> with null value
    Some(ComponentFieldValueBox),   // for Option<T> with a value
    Vec(ComponentFieldValueVec),    // for Vec<T>
    StyledDom(StyledDom),           // actual rendered child slot content
    Callback(CallbackType),         // actual callback fn pointer
    RefAny(RefAny),                 // actual data binding
    Struct(ComponentFieldValueVec), // fields of a StructRef, in order
    Enum {
        variant: AzString,
        fields: ComponentFieldValueVec,
    },
}

/// Named field value: (field_name, value) pair.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ComponentFieldNamedValue {
    pub name: AzString,
    pub value: ComponentFieldValue,
}

impl_vec!(ComponentFieldNamedValue, ComponentFieldNamedValueVec, ...);
```

The render function signature becomes:
```rust
pub type ComponentRenderFn = fn(
    &ComponentDef,
    &ComponentMap,
    &ComponentFieldNamedValueVec,   // name → value pairs
    &OptionString,                  // text content
) -> Result<StyledDom, RenderDomError>;
```

Live preview instantiation creates `ComponentFieldNamedValueVec` from the
data model's default values. The debugger can create/modify values and
pass them to the render function for live updates.

### 14.4 Resolved: JSON data_model format

**Problem**: Section 8.3 previously showed `data_model` as a bare JSON array
of fields, but `ComponentDef.data_model` is a `ComponentDataModel` (named
struct with `name`, `description`, `fields`).

**Resolution**: JSON now shows `data_model` as an object:
```json
"data_model": {
    "name": "LinkData",
    "description": "...",
    "fields": [ ... ]
}
```

### 14.5 Resolved: StyledDom slot defaults

**Problem**: How to express "by default, put a `<span>` component here"?
The `ComponentDefaultValue` enum had no variant for instantiating a
component.

**Resolution**: Added `ComponentDefaultValue::ComponentInstance(...)` with
`library` + `component` fields. Parser syntax: `default: builtin.a`.
JSON syntax: `{ "type": "ComponentInstance", "library": "builtin", "component": "a" }`.

This also supports structured field overrides:
```json
{ "type": "ComponentInstance", "library": "builtin", "component": "a",
  "overrides": [
      { "field": "href", "source": { "type": "literal", "value": "https://example.com" } }
  ] }
```

### 14.6 Noted: Parser EnumRef/StructRef ambiguity

**Problem**: The parser resolves unknown PascalCase identifiers by checking
`library.enum_models` first, then `library.data_models`. If a library has
both a struct and an enum with the same name, the enum silently wins.

**Resolution**: The parser should error on ambiguity:
```
error: "UserRole" is ambiguous — matches both enum_models and data_models.
       Use explicit prefix: enum::UserRole or struct::UserRole
```

Parser syntax extension:
```
enum::UserRole    → always EnumRef("UserRole")
struct::UserRole  → always StructRef("UserRole")
UserRole          → resolved by lookup, error if ambiguous
```

### 14.7 Noted: Builtin data model naming

Builtin HTML elements (div, a, span, etc.) now need a `ComponentDataModel`
with a `name` field. Convention:
- Component tag `"a"` → data model name `"LinkData"` (= display_name + "Data")
- Component tag `"div"` → data model name `"DivData"`
- Component tag `"button"` → data model name `"ButtonData"`
- Component tag `"img"` → data model name `"ImageData"`

The `builtin_component_def()` helper should auto-generate:
`data_model.name = format!("{}Data", display_name)`

For most builtins with no fields (div, span, section, etc.), the data model
is empty: `DivData {}`. This is fine — code gen just emits an empty struct.

### 14.8 Overall assessment: Does the design make sense?

**Yes, with the fixes above.** The core ideas are sound:

| Concern | Status |
|---------|--------|
| Rich type descriptors instead of strings | ✅ `ComponentFieldType` enum |
| FFI safety (`#[repr(C)]`) | ✅ all types representable |
| Child slot composition | ✅ `StyledDom` variant + slot defaults |
| Callback type advertising | ✅ `Callback(ComponentCallbackSignature)` |
| Enum variants / component states | ✅ `ComponentEnumModel` |
| String parser for user input | ✅ simple syntax with `?`, `[]`, `fn()` |
| Compiled vs editable components | ✅ `ComponentSource` determines editability |
| Code generation (multi-language) | ✅ structured match on `ComponentFieldType` |
| JSON serialization | ✅ with string shorthand fallback |
| Runtime values for rendering | ✅ `ComponentFieldValue` + `ComponentFieldNamedValueVec` |
| Default slot content | ✅ `ComponentInstance` defaults |
| Named main data model | ✅ `ComponentDataModel` with name |
| Data binding (value source) | ✅ `ComponentFieldValueSource` (Literal/Binding/Default) |
| Callback default fn pointers | ✅ `CallbackFnPointer` + `dladdr` resolution |
| Dynamic → compiled pipeline | ✅ codegen from JSON → compiled component |
| Cross-crate callback separation | ✅ fn pointer names as code-gen markers |
| Recursive composition | ✅ user-defined → builtin recursion |
| Component-local CSS | ✅ `scoped_css` as template string (Section 16) |
| CSS data bindings | ✅ `{field_name}` syntax, same as XML template |
| OS-specific CSS preview | ✅ `@os()` at-rules + `DynamicSelectorContext` (already in parser) |
| CSS code generation | ✅ `format!()` / `snprintf` for compiled output |

**Remaining complexity risks:**

1. **`ComponentFieldTypeBox` for recursive types** adds FFI boilerplate.
   Recommendation: limit to depth 1 (`Option<String>` yes, `Option<Option<String>>` no).
   This covers all practical cases and avoids the pointer indirection.

2. **`ComponentFieldValue` is large.** A runtime value enum with 17 variants
   including `StyledDom` and `RefAny` is hefty. But it's only used at
   component instantiation boundaries (preview, debugger, template rendering),
   not in hot paths. Acceptable.

3. **Cross-library type references.** `StructRef` is library-local but
   `ComponentInstance` defaults are cross-library. This asymmetry is fine
   for now — struct types are implementation details, component references
   are user-facing. If cross-library struct refs are needed later, extend
   `StructRef` to `StructRef { library: OptionAzString, name: AzString }`.

---

## 15. Component Instance Editing & Data Binding

This section describes how the debugger/editor presents component instances
to the user, and how data flows between application state and component fields.

### 15.1 Two editor views

There are two fundamentally different editing contexts for a component instance:

**A) Component Preview View** — editing a component's data model in isolation,
with literal values, to see what the component looks like. This is for
component developers authoring or testing their component.

**B) Application Composition View** — editing how a component instance connects
to the application's main data model (`RefAny`). This is for application
developers wiring up components to their app state.

Both views show the same data structure (the component's `ComponentDataModel`),
but the value column means different things:

| | Preview View | Composition View |
|---|---|---|
| **Value source** | `Literal` or `Default` | `Literal`, `Binding`, or `Default` |
| **Purpose** | Quick preview / visual testing | Data flow wiring |
| **Editable?** | All fields editable | Depends on component source |
| **Shows bindings?** | No | Yes (`app_state.user.name`) |

### 15.2 Component Preview View (component definition editor)

When editing a component's definition, the editor shows the data model as
a nested tree with inline editable values. StyledDom slot fields show the
instantiated sub-component with its own fields expandable underneath:

```
AvatarDataModel {
    dom: "builtin.a" {                    ← StyledDom slot, drag & drop target
        href: null                        ← field uses Default (collapsed)
        color: "system:link"              ← field has literal default from component def
    }
    size: 48                              ← I32 field with literal default
    alt_text: "User avatar"               ← String field
    on_load: <navigate_to_href>           ← Callback with CallbackFnPointer default
}
```

**Interaction rules:**

- **StyledDom slot fields** (`dom` above): show the currently instantiated
  sub-component. Users can **drag & drop** a different component from the
  library onto this slot, which replaces the `ComponentInstanceDefault`
  (and re-renders the preview).

- **Sub-component fields** (`href`, `color` above): show underneath the
  slot, indented. By default, fields using `ComponentFieldValueSource::Default`
  are **collapsed** (grayed out, showing the default value). Click to expand
  and override.

- **Callbacks with `CallbackFnPointer` defaults**: show the function name
  as a read-only badge (e.g. `<navigate_to_href>`). The preview cannot
  execute external functions — it uses a no-op stub. But the name tells
  the user what will happen in the compiled version.

- **On edit**: changing any field value instantly re-renders the preview
  by calling `render_fn` with the updated `ComponentFieldNamedValueVec`.

### 15.3 Application Composition View (main HTML view)

When wiring up components in the application, the editor shows how each
component's fields connect to the application's main data model (the
`RefAny` state). Fields can be:
- **Bound** to an app state path → `ComponentFieldValueSource::Binding`
- **Hardcoded** to a literal → `ComponentFieldValueSource::Literal`
- **Left at default** → `ComponentFieldValueSource::Default`

```
── MyPageLayout ──────────────────────────────────────
│
│  AvatarDataModel {
│      dom: "builtin.a" {
│          href: app_state.current_link          ← Binding
│          color: "green"                        ← Literal (hardcoded)
│      }
│      size: app_state.avatar_size               ← Binding
│      alt_text: app_state.user.display_name     ← Binding
│      on_load: my_app::handlers::load_avatar    ← CallbackFnPointer
│  }
│
──────────────────────────────────────────────────────
```

**Interaction rules:**

- Users can **type a literal value** (e.g. `"green"`) or **type a binding path**
  (e.g. `app_state.user.name`). The editor distinguishes bindings from literals
  by path resolution: if the path matches a field in the app's data model, it's
  a binding; otherwise it's treated as a literal string.

- **Binding auto-complete**: the editor knows the app's `RefAny` type hint
  (e.g. `"MyAppData"`) and can offer auto-complete for paths like
  `app_state.user.` → `name`, `email`, `avatar_url`, etc. — if the
  referenced `StructRef` has its fields defined.

- **Type checking**: bindings are (soft-)validated: if `href` expects a `String`
  and `app_state.current_link` is typed as `String`, it's valid. If types
  don't match, show a warning (not a hard error — the user might know better,
  especially with `RefAny` which is type-erased).

### 15.4 The `ComponentFieldValueSource` JSON format

In the debug server API, field value sources are serialized as:

```json
// Component Preview View — all literals/defaults
{
    "component": "builtin.avatar",
    "fields": [
        { "name": "dom", "source": {
            "type": "component_instance",
            "library": "builtin",
            "component": "a",
            "overrides": [
                { "field": "href", "source": { "type": "default" } },
                { "field": "color", "source": { "type": "literal", "value": "system:link" } }
            ]
        }},
        { "name": "size", "source": { "type": "literal", "value": 48 } },
        { "name": "alt_text", "source": { "type": "literal", "value": "User avatar" } }
    ]
}

// Application Composition View — with bindings
{
    "component": "builtin.avatar",
    "fields": [
        { "name": "dom", "source": {
            "type": "component_instance",
            "library": "builtin",
            "component": "a",
            "overrides": [
                { "field": "href", "source": { "type": "binding", "path": "app_state.current_link" } },
                { "field": "color", "source": { "type": "literal", "value": "green" } }
            ]
        }},
        { "name": "size", "source": { "type": "binding", "path": "app_state.avatar_size" } },
        { "name": "alt_text", "source": { "type": "binding", "path": "app_state.user.display_name" } }
    ]
}
```

### 15.5 Recursive composition model

User-defined components must ultimately recurse to compiled (builtin) components.
This is enforced by the rendering pipeline:

```
UserDefined component "UserCard"
  └─ render_fn produces StyledDom by instantiating:
      ├─ Compiled "div" (has native render_fn)
      ├─ UserDefined "Avatar"
      │   └─ render_fn produces StyledDom by instantiating:
      │       ├─ Compiled "img" (native)
      │       └─ Compiled "div" (native)
      └─ Compiled "span" (native)
```

At each level, the component's `render_fn` receives the field values
(`ComponentFieldNamedValueVec`) and produces a `StyledDom`. For user-defined
components, the `render_fn` is the component's compiled source code that
builds the DOM using the azul API (not an XML template). For compiled
components, the `render_fn` is a native Rust function that builds the DOM
directly (with full access to the Rust type system).

> **Note:** The earlier proposal for a "generic template-expansion function"
> has been replaced with the source-edit-recompile approach. Each component's
> render_fn is actual source code, not XML template expansion.

### 15.6 Dynamic → Compiled compilation pipeline

A key goal is that user-defined (dynamic/JSON) components can be **compiled**
into native components without much interaction:

1. **Export**: the debugger exports the component definition as JSON
   (data model, template, CSS, callback signatures, default fn pointer names)

2. **Code generation**: `generate_scaffold()` emits Rust/C/Python code:
   - Data model struct (e.g. `struct AvatarData { ... }`)
   - Callback typedefs (e.g. `type AvatarOnLoadCallbackType = ...`)
   - Render function skeleton that builds the DOM from the template
   - `use` statements for callback default fn pointers
     (e.g. `use my_nav_crate::handlers::navigate_to_href;`)

3. **Compile**: user compiles the generated code → now it's a compiled
   component. Its render_fn is native Rust, its callback defaults are
   resolved via `dladdr()`, and it can be registered as a `Compiled`
   component in the `ComponentMap`.

4. **Iterate**: the compiled component can still be inspected in the debugger
   (read-only data model view), and users can create new dynamic components
   that reference it.

The `CallbackFnPointer` default is essential here: it tells the code generator
which `use` import to emit and which function to call. The function may live
in a completely separate crate (business logic separate from UI layout).

### 15.7 Callback cross-crate separation

The architecture encourages separating concerns:

```
my_app/
├── my_ui_crate/          ← component definitions (templates, CSS, data models)
│   ├── avatar.json       ← dynamic component (design-time)
│   └── avatar.rs         ← compiled component (after codegen)
│
├── my_logic_crate/       ← callback implementations (pure business logic)
│   └── handlers.rs       ← fn navigate_to_href(...), fn load_avatar(...)
│
└── my_app_crate/         ← application assembly
    └── main.rs           ← creates ComponentMap, registers libraries, runs app
```

Component definitions reference callbacks by name:
```
on_click: fn(String) -> Update  [default: my_logic_crate::handlers::navigate_to_href]
```

The debugger shows the callback signature and default function name, but
doesn't need access to the implementation. At compile time, the linker
resolves everything. This means:
- UI designers can work on component layout without writing Rust
- Backend developers can implement callbacks without knowing the UI structure
- The component JSON is the contract between the two

---

## 16. Component-Local CSS with Data Bindings

Each component carries its own **scoped CSS stylesheet** (`scoped_css: AzString`
on `ComponentDef`). This CSS is local to the component — it only affects DOM
nodes produced by that component's `render_fn`, not the global application.

This section describes how the CSS editor works, how template expressions
bind CSS values to the data model, how OS-specific previewing works, and
how this interacts with the existing `format_args_dynamic` substitution
mechanism.

### 16.1 CSS as a template string

The `scoped_css` field works exactly like the `template` field: it is a
**template string** that can contain `{field_name}` expressions. Before
the CSS is parsed into `Css` rules, the template is expanded via the
same `format_args_dynamic()` function already used for XML attribute
substitution.

**Component Preview View** — expressions resolve to literal default values:
```css
/* scoped_css template (stored in ComponentDef) */
.avatar-container {
    width: {size}px;
    height: {size}px;
    border-radius: {border_radius};
    background-color: {bg_color};
}
```

With defaults `size = 48`, `border_radius = "50%"`, `bg_color = "#ccc"`,
the preview expands this to:
```css
.avatar-container {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    background-color: #ccc;
}
```

**Application Composition View** — expressions can also be data bindings.
The user types `{app_state.theme.primary_color}` instead of `{bg_color}`.
At runtime, the binding resolves to the actual app state value before parsing.

### 16.2 The rendering pipeline for CSS templates

When rendering a component, the CSS template is processed in two steps:

```
1. scoped_css template string
      │
      ▼
2. format_args_dynamic(scoped_css, field_values)
      │  ← substitutes {field_name} → literal value
      ▼
3. Css::from_string(expanded_css)
      │  ← parses plain CSS into Css rules
      ▼
4. dom.restyle(css)
      ← applies scoped CSS to component's StyledDom
```

This already matches the existing `DynamicXmlComponent::render_dom()` code
path in `core/src/xml.rs` (line ~5100), which:
1. Finds a `<style>` child node
2. Parses its text as CSS via `Css::from_string()`
3. Calls `dom.restyle(css)`

The only change: **before step 2**, run `format_args_dynamic()` over the
CSS text to substitute template expressions. The infrastructure already
exists — the same function is used for XML attribute substitution.

### 16.3 CSS template expressions vs. CSS custom properties

Template expressions (`{field_name}`) are **not** CSS custom properties
(`var(--field-name)`). They operate at different levels:

| | Template expressions | CSS custom properties |
|---|---|---|
| **Syntax** | `{field_name}` | `var(--field-name)` |
| **Resolved** | Before CSS parsing (string level) | During CSS cascade (property level) |
| **Scope** | Component data model fields | CSS inheritance tree |
| **Can affect** | Any part of CSS text (selectors, values, properties) | Only property values |
| **Escaping** | `{{` for literal `{` | N/A |

Template expressions are more powerful because they can substitute **any**
part of the CSS string — including selectors, property names, or partial
values like `{size}px`. CSS custom properties can only replace whole values.

However, the component can also use CSS custom properties in its scoped CSS
if desired — both mechanisms coexist without conflict.

### 16.4 Editor experience for CSS editing

The editor provides a dedicated CSS panel for each component:

```
┌─ Avatar: CSS ─────────────────────────────────────┐
│                                                    │
│  .avatar-container {                               │
│      width: {size}px;           ← autocomplete     │
│      height: {size}px;            from data model  │
│      border-radius: {border_radius};               │
│      background-color: {bg_color};                 │
│  }                                                 │
│                                                    │
│  .avatar-container:hover {                         │
│      opacity: 0.8;                                 │
│  }                                                 │
│                                                    │
│  ┌─ Preview ──────────────────────────────────┐    │
│  │           ┌────────┐                       │    │
│  │           │ Avatar │                       │    │
│  │           └────────┘                       │    │
│  │                                            │    │
│  └────────────────────────────────────────────┘    │
│                                                    │
│  [OS: macOS ▼]  [Theme: Light ▼]  [Size: 1x ▼]    │
│                                                    │
└────────────────────────────────────────────────────┘
```

**Live preview**: every keystroke in the CSS editor re-expands the template
with current field values and re-renders the preview. This is cheap because
`format_args_dynamic()` + `Css::from_string()` are both fast string operations.

**Autocomplete**: when the user types `{`, the editor shows a dropdown of
available data model field names (from `ComponentDataModel.fields`). Only
fields whose types can meaningfully appear in CSS values are suggested
(String, I32, U32, F32, F64, Bool — not StyledDom, Callback, RefAny, etc.).

**Syntax highlighting**: `{field_name}` expressions are highlighted differently
from regular CSS text, making template variables visually distinct.

**Validation**: after expansion, the CSS is parsed. Parse errors are shown
inline in the editor (red underline + error message). This catches both:
- CSS syntax errors (e.g. missing semicolons)
- Template errors (e.g. `{nonexistent_field}` — no matching field in data model)

### 16.5 OS-specific preview

The CSS parser **already supports** `@os()` blocks and a comprehensive
`DynamicSelector` system (see [css/src/dynamic_selector.rs](css/src/dynamic_selector.rs)
and [css/src/parser2.rs](css/src/parser2.rs)). This is not hypothetical —
the infrastructure is fully built.

#### 16.5.1 Existing `DynamicSelector` at-rules

The parser recognizes these at-rules that wrap CSS blocks with conditions:

**`@os(...)` — Operating system targeting:**
```css
.label { font-family: sans-serif; }

@os(windows) { .label { font-family: "Segoe UI"; } }
@os(macos)   { .label { font-family: "Helvetica"; } }
@os(linux)   { .label { font-family: "DejaVu Sans"; } }
@os(ios)     { .label { font-size: 17px; } }
@os(android) { .label { font-size: 14sp; } }
@os(apple)   { /* matches both macOS + iOS */ }
@os(web)     { /* WASM target */ }
```

Parsed as `DynamicSelector::Os(OsCondition)` where `OsCondition` is:
`Any | Apple | MacOS | IOS | Linux | Windows | Android | Web`.

**`@media(...)` — Viewport / media type queries:**
```css
@media screen and (min-width: 800px) {
    .sidebar { display: flex; }
}
@media print { .no-print { display: none; } }
```

Parsed as `DynamicSelector::Media(MediaType)`, `DynamicSelector::ViewportWidth(MinMaxRange)`,
`DynamicSelector::ViewportHeight(MinMaxRange)`, `DynamicSelector::Orientation(...)`.

**`@lang(...)` — Language / locale targeting:**
```css
@lang(de) { .text { quotes: "„" """ "‚" "'"; } }
@lang(en) { .text { quotes: "\201C" "\201D" "\2018" "\2019"; } }
```

Parsed as `DynamicSelector::Language(LanguageCondition)` with BCP 47
prefix matching (e.g. `"de"` matches `"de"`, `"de-DE"`, `"de-AT"`).

#### 16.5.2 `DynamicSelector` variants (data types exist, some not yet parsed)

The full `DynamicSelector` enum has 15 variants:

| Variant | At-rule syntax | Parser status |
|---|---|---|
| `Os(OsCondition)` | `@os(windows)` | ✅ Parsed |
| `OsVersion(OsVersionCondition)` | `@os-version(>= win-10)` | Data types exist, parser TODO |
| `Media(MediaType)` | `@media screen` | ✅ Parsed |
| `ViewportWidth(MinMaxRange)` | `@media (min-width: 800px)` | ✅ Parsed |
| `ViewportHeight(MinMaxRange)` | `@media (min-height: 600px)` | ✅ Parsed |
| `ContainerWidth(MinMaxRange)` | `@container (min-width: ...)` | Data type exists, parser TODO |
| `ContainerHeight(MinMaxRange)` | `@container (min-height: ...)` | Data type exists, parser TODO |
| `ContainerName(AzString)` | `@container sidebar (...)` | Data type exists, parser TODO |
| `Theme(ThemeCondition)` | `@theme(dark)` | Data type exists, parser TODO |
| `AspectRatio(MinMaxRange)` | `@media (aspect-ratio: ...)` | Data type exists, parser TODO |
| `Orientation(OrientationType)` | `@media (orientation: portrait)` | ✅ Parsed |
| `PrefersReducedMotion(BoolCondition)` | `@media (prefers-reduced-motion)` | Data type exists, parser TODO |
| `PrefersHighContrast(BoolCondition)` | `@media (prefers-high-contrast)` | Data type exists, parser TODO |
| `PseudoState(PseudoStateType)` | `:hover`, `:active`, etc. | ✅ Parsed (as pseudo-selectors) |
| `Language(LanguageCondition)` | `@lang(de)` | ✅ Parsed |

#### 16.5.3 `OsVersion` — named version constants

`OsVersion` provides named constants for every major OS release, usable
in CSS via `@os-version(>= ...)` syntax:

- **Windows**: `win-2000`, `win-xp`, `win-vista`, `win-7`, `win-8`, `win-8.1`,
  `win-10`, `win-10-1507` through `win-10-22H2`, `win-11`, `win-11-21H2`
  through `win-11-24H2`
- **macOS**: `cheetah` (10.0) through `tahoe` (26.0) — all codenames +
  version numbers
- **iOS**: `1` through `18`
- **Android**: `cupcake` through `vanilla-ice-cream` (API level 3–35),
  also by codename, version number, or `api<N>`
- **Linux**: kernel versions like `5.4`, `6.0`
- **Linux desktop env**: `@os-version(desktop-env: gnome)` / `kde` / `xfce` etc.

```css
/* Fluent Design (Windows 10+) vs classic (older) */
@os-version(>= win-10) {
    .button { border-radius: 4px; backdrop-filter: blur(20px); }
}
@os-version(< win-10) {
    .button { border-radius: 0; border: 1px solid #999; }
}

/* macOS Sequoia+ style */
@os-version(>= sequoia) {
    .sidebar { background: rgba(255,255,255,0.6); }
}
```

#### 16.5.4 `DynamicSelectorContext` — runtime evaluation

All conditions are evaluated via `DynamicSelectorContext`, which aggregates
the current runtime state:

```rust
pub struct DynamicSelectorContext {
    pub os: OsCondition,
    pub os_version: OsVersion,
    pub desktop_env: OptionLinuxDesktopEnv,
    pub theme: ThemeCondition,       // Light / Dark / Custom
    pub media_type: MediaType,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub container_width: f32,        // NaN = no container
    pub container_height: f32,
    pub container_name: OptionString,
    pub prefers_reduced_motion: BoolCondition,
    pub prefers_high_contrast: BoolCondition,
    pub orientation: OrientationType,
    pub pseudo_state: PseudoStateFlags,
    pub language: AzString,
    pub window_focused: bool,
}
```

Created via `DynamicSelectorContext::from_system_style(&system_style)` which
auto-detects the current OS, version, theme, desktop env, accessibility prefs,
and language from the system.

#### 16.5.5 `CssPropertyWithConditions` — conditional CSS properties

Each parsed CSS property carries a `DynamicSelectorVec` of conditions:

```rust
pub struct CssPropertyWithConditions {
    pub property: CssProperty,
    pub apply_if: DynamicSelectorVec,  // ALL must match; empty = unconditional
}
```

When the parser encounters `@os(windows) { .btn { color: blue; } }`, it emits:
```rust
CssPropertyWithConditions {
    property: CssProperty::TextColor(ColorU { r: 0, g: 0, b: 255, a: 255 }),
    apply_if: vec![DynamicSelector::Os(OsCondition::Windows)].into(),
}
```

At render time, properties with unmatched conditions are simply skipped.
This is how compiled widgets already achieve OS-specific styling — e.g.
`Label` uses different `const` property arrays selected by `#[cfg(target_os)]`,
but the `DynamicSelector` system makes that work **at runtime** too.

#### 16.5.6 Preview OS switching

In the editor preview panel, the OS dropdown works by overriding the
`DynamicSelectorContext`:

```
[OS: macOS ▼]  [Theme: Light ▼]  [Lang: en-US ▼]  [Size: 1x ▼]
```

- Switching to "Windows" sets `ctx.os = OsCondition::Windows` and
  `ctx.os_version = OsVersion::WIN_11` (default latest)
- Switching to "macOS" sets `ctx.os = OsCondition::MacOS` and
  `ctx.os_version = OsVersion::MACOS_SEQUOIA`
- Theme dropdown sets `ctx.theme = ThemeCondition::Dark` etc.
- Language dropdown sets `ctx.language = "de-DE"` etc.

The component's CSS (which contains `@os()`, `@media()`, `@lang()` blocks)
is re-evaluated against the modified context. Properties whose conditions
no longer match get skipped; previously-skipped properties whose conditions
now match get applied. **No re-parsing needed** — only the condition
evaluation changes.

This means user-defined components get the same OS-specific preview
capability that compiled widgets have, with no template substitution or
`__` prefix fields needed — the CSS parser handles it natively.

### 16.6 CSS in the Application Composition View

When a component is used in the application (not editing its definition),
the CSS template expressions can reference the **application's data model**
via binding paths, just like field values in Section 15.3:

```css
/* In the application's "main HTML" editor */
.user-card {
    background-color: {app_state.theme.primary_color};
    display: {app_state.show_user_card};
}
```

Here `{app_state.theme.primary_color}` is a `ComponentFieldValueSource::Binding`
at the CSS level. The resolution pipeline:

1. Application provides `RefAny` state containing `theme.primary_color = "#3b82f6"`
2. Before CSS parsing, bindings resolve: `{app_state.theme.primary_color}` → `"#3b82f6"`
3. CSS parses normally: `background-color: #3b82f6;`

This means the **same CSS template** serves two purposes:
- In Preview View: fields resolve to literal defaults from the data model
- In Composition View: fields can resolve to app state bindings

The `ComponentFieldOverride` mechanism (Section 15.4) already supports this
at the field level. For CSS, the override targets a special pseudo-field:

```json
{
    "component": "my_lib.avatar",
    "fields": [
        { "name": "size", "source": { "type": "literal", "value": 48 } },
        { "name": "bg_color", "source": { "type": "binding", "path": "app_state.theme.primary_color" } }
    ]
}
```

No separate CSS-level binding mechanism is needed — the CSS template uses
`{bg_color}`, the component instance overrides `bg_color` with a binding,
and the runtime expands `{bg_color}` to whatever `app_state.theme.primary_color`
evaluates to. The CSS template itself never changes.

### 16.7 JSON format for scoped_css

In the component JSON export (Section 10), the CSS template is a plain string:

```json
{
    "id": "my_lib:avatar",
    "display_name": "Avatar",
    "data_model": {
        "name": "AvatarDataModel",
        "fields": [
            { "name": "size", "type": "I32", "default": 48 },
            { "name": "border_radius", "type": "String", "default": "50%" },
            { "name": "bg_color", "type": "String", "default": "#ccc" }
        ]
    },
    "scoped_css": ".avatar-container {\n    width: {size}px;\n    height: {size}px;\n    border-radius: {border_radius};\n    background-color: {bg_color};\n}",
    "template": "<div class=\"avatar-container\"><img src=\"{image_url}\" /></div>"
}
```

Both `scoped_css` and `template` are template strings with the same
`{field_name}` syntax, processed by the same `format_args_dynamic()` function.

### 16.8 Code generation for CSS

When a dynamic component is compiled (Section 15.6), the CSS template
is converted to code. The code generator handles `{field_name}` expressions
by emitting `format!()` calls:

**Rust output:**
```rust
fn avatar_css(data: &AvatarDataModel) -> String {
    format!(
        ".avatar-container {{\
            width: {}px;\
            height: {}px;\
            border-radius: {};\
            background-color: {};\
        }}",
        data.size, data.size, data.border_radius, data.bg_color,
    )
}
```

**C output:**
```c
AzString avatar_css(const AvatarDataModel* data) {
    char buf[1024];
    snprintf(buf, sizeof(buf),
        ".avatar-container { width: %dpx; height: %dpx; border-radius: %s; background-color: %s; }",
        data->size, data->size, data->border_radius, data->bg_color);
    return AzString_copyFromBytes(buf, strlen(buf));
}
```

The generated code is equivalent to the runtime template expansion, but
avoids the string parsing overhead — the `{field_name}` positions are
known at compile time.

### 16.9 Interaction with `CssPropertyWithConditionsVec`

Compiled widgets store CSS as `CssPropertyWithConditionsVec` — static arrays
of pre-parsed CSS properties (see `layout/src/widgets/label.rs` lines 90+).
User-defined components store CSS as template strings that get parsed at
render time.

When a dynamic component is compiled, the code generator can optionally
emit the CSS as `const` property arrays (like builtin widgets do) for
maximum performance. However, if the CSS uses `{field_name}` expressions
that vary per instance, it **must** remain a runtime `format!()` + parse
because the CSS values aren't known at compile time.

The rule:
- CSS with **no** template expressions → can be `const` (parsed once)
- CSS with template expressions → must be runtime (formatted per render)

### 16.10 Summary

| Aspect | How it works |
|---|---|
| **Storage** | `scoped_css: AzString` on `ComponentDef` — template string |
| **Substitution** | `format_args_dynamic()` — same as XML template |
| **Syntax** | `{field_name}` — matches data model fields |
| **Escaping** | `{{` for literal `{`, `}}` for literal `}` |
| **Parsing** | After substitution: `Css::from_string(expanded)` |
| **Application** | `dom.restyle(css)` — scoped to component DOM |
| **Preview** | Live re-render on every CSS edit |
| **OS preview** | `@os()` at-rules + `DynamicSelectorContext` override |
| **Bindings** | Works through existing field override mechanism |
| **Code gen** | `format!()` call with field references |
