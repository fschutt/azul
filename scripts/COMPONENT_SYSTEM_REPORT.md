# Component System & GUI Builder — Investigation Report (v2)

**Date:** 2025-02-21
**Scope:** Component libraries, `repr(C)` component struct, JSON definitions,
code export, debugger integration

---

## 1. Current State — What Already Exists

### 1.1 XmlComponent System (`core/src/xml.rs`, ~4463 lines)

| Concept | Location | What it does |
|---------|----------|--------------|
| `XmlComponentTrait` | `xml.rs:1119` | Trait: `get_type_id`, `get_xml_node`, `get_available_arguments`, `render_dom`, `compile_to_rust_code` |
| `XmlComponent` | `xml.rs:1368` | `{ id, renderer: Box<dyn XmlComponentTrait>, inherit_vars }` |
| `XmlComponentMap` | `xml.rs:1387` | `BTreeMap<String, XmlComponent>` — flat map, no namespacing |
| `ComponentArguments` | `xml.rs:1068` | `{ args: Vec<(Name, Type)>, accepts_text: bool }` |
| `FilteredComponentArguments` | `xml.rs:1092` | Same + concrete `values: BTreeMap<String, String>` |
| `DynamicXmlComponent` | `xml.rs:4277` | XML-defined: `<component name="..." args="a: String">` |
| `DynamicItem` / `split_dynamic_string` | `xml.rs:3272` | `{var}` interpolation (no format specifiers) |

**Builtin components:** ~50 HTML elements via `html_component!` macro (div, p, span, a,
button, table, form, icon, etc.). Each implements both `render_dom` and `compile_to_rust_code`.

**Problems with current design:**

1. **`XmlComponentTrait` is a Rust trait** — not `repr(C)`, cannot be stored or
   called across the C FFI boundary. Components defined in C/Python cannot participate.
2. **Flat namespace** — `BTreeMap<String, XmlComponent>` has no collection/library concept.
   All components share a single namespace via `normalize_casing()`.
3. **`normalize_casing` ignores `:`** — treats it as a regular character, so `shadcn:avatar`
   becomes `shadcn:avatar` (unusable as a Rust identifier).
4. **No JSON input path** — components can only come from XML (`DynamicXmlComponent`)
   or Rust code (`html_component!` macro). No `serde::Deserialize` on any component type.
5. **`compile_to_rust_code` is Rust-only** — hardcoded `Dom::create_*()` chains.
   No language parameter. No access to the DOM tree (children handled externally by caller).
   However, `DynamicXmlComponent` (XML-defined components) uses a **system-defined**
   implementation — it doesn't write its own compile logic; instead, the system walks the
   component's XML template tree and generates code via `compile_node_to_rust_code_inner`.
   This pattern should be preserved for JSON-defined components: the system provides a
   generic callback that acts on the component's structured definition.
6. **`split_dynamic_string` has no format specifiers** — `{var:?}`, `{var:.2}`,
   `{var:>10}` are NOT supported. The parser treats everything between `{` and `}` as
   the variable name literally. `{my_var:?}` would look up `"my_var:?"` and fail silently.

### 1.2 repr(C) Callback Pattern (Already Established)

Azul already has a well-established pattern for C-compatible function pointers:

```c
// dll/azul.h
typedef AzStyledDom (*AzLayoutCallbackType)(AzRefAny, AzLayoutCallbackInfo);

struct AzLayoutCallback {
    AzLayoutCallbackType cb;
    AzOptionRefAny ctx;    // FFI context for Python/Java callable objects
};
```

```rust
// core/src/callbacks.rs
#[repr(C)]
pub struct LayoutCallback {
    pub cb: LayoutCallbackType,     // extern "C" fn(RefAny, LayoutCallbackInfo) -> StyledDom
    pub ctx: OptionRefAny,          // Optional context for non-Rust callers
}
```

Every callback follows: `extern "C" fn(RefAny, *Info) -> *Return` stored in a
`#[repr(C)]` struct with `cb` + `ctx` fields. `ctx` allows FFI languages (Python, C)
to attach a callable object.

`RefAny` itself is fully `repr(C)` with runtime borrow-checking, type-ID verification,
and optional JSON serialize/deserialize function pointers (`serialize_fn`, `deserialize_fn`
stored as `usize`).

### 1.3 CSS Scoping in Components

`DynamicXmlComponent::render_dom` (`xml.rs:4331`) builds an isolated `StyledDom` subtree,
calls `dom.restyle(component_css)` on it, and only THEN returns it to the parent for
`append_child()`. This means the component's `<style>` CSS is matched only against the
component's own subtree nodes.

**CSS scoping direction (intentional):** After `append_child`, the parent's CSS rules CAN
cascade into the component subtree — this is **desired** behavior, allowing higher-level CSS
to override component-level styles. However, the component's own `<style>` CSS does NOT leak
**upward** to the parent, because it was applied to the component's isolated subtree before
being attached. This gives us the correct behavior:
- Parent CSS **can** override component CSS (higher-level wins)
- Component CSS **cannot** affect siblings or ancestors (no upward leak)

For code generation this works cleanly — each component's CSS becomes `const` inline styles
at compile time, and the parent's overrides take precedence at the integration point.

### 1.4 Code Generation (`xml.rs:2760-4270`)

| Function | Purpose |
|----------|---------|
| `compile_components_to_rust_code` | Iterates all components, produces `(name, body, args, css)` |
| `compile_components` | Wraps into `pub mod components { pub mod name { ... } }` |
| `compile_body_node_to_rust_code` | `<body>` to Rust `Dom::create_body()...` |
| `compile_node_to_rust_code_inner` | Recursive: `Dom::create_*()` + CSS matching + inline `const &str` styles |
| `compile_and_format_dynamic_items` | `{var}` to `format!("{}", var)` or `AzString::from_const_str("...")` |

The Rust codegen already handles CSS-to-inline-style conversion (matching rules against
the DOM path), `const_str` optimization for static strings, and component argument generation.

### 1.5 Debugger (`debugger.html` + `debugger.js`)

Three views: Inspector (DOM tree), Testing (E2E runner), Components (registry list).

The Components view (`debugger.js:1538-1570`) calls `get_component_registry` and
displays a flat list. `showComponentDetail()` renders the raw JSON. No metadata,
no preview, no library grouping, no "Create Component" action.

Context menu: Insert child (div/span/p/button/text), delete node. No "Create Component".

Export menu: Project JSON, E2E tests. No code export.

### 1.6 Debug Server (`debug_server.rs:3375`)

```rust
pub struct ComponentInfo {
    pub tag: String,
    pub accepts_text: bool,
    pub attributes: Vec<ComponentAttributeInfo>,  // (name, type) pairs
}
```

Universal HTML attributes (id, class, style, tabindex, aria-*, etc.) and tag-specific
attributes are manually appended in `get_universal_attributes()` and
`get_tag_specific_attributes()`. These should NOT be repeated per component — they're
a property of the rendering system, not the component.

---

## 2. Redesign: Component Libraries with `repr(C)` Struct

### 2.1 Core Type: `AzComponentDef` (replaces `XmlComponentTrait`)

The component **definition** is a `repr(C)` struct with function pointers, not a trait:

```rust
/// A component definition — can come from Rust, C, Python, or JSON.
/// This is the "class" / "template" — not an instantiation.
#[repr(C)]
pub struct AzComponentDef {
    /// Collection + name, e.g. "builtin:div", "shadcn:avatar", "mylib:card"
    pub id: AzComponentId,
    /// Human-readable display name, e.g. "Link" for "builtin:a"
    pub display_name: AzString,
    /// Markdown documentation for the component
    pub description: AzString,
    /// Parameters this component accepts (name, type, default, doc)
    pub parameters: AzComponentParamVec,
    /// Whether this component accepts text content as first arg
    pub accepts_text: bool,
    /// Child policy
    pub child_policy: AzChildPolicy,
    /// The component's own scoped CSS (applied only to its subtree)
    pub scoped_css: AzString,
    /// Example usage XML string
    pub example_xml: AzString,

    // --- Function pointers ---

    /// Render this component to a StyledDom (for live preview / runtime)
    /// fn(component_def: &AzComponentDef, args: &AzFilteredArgs,
    ///    content: AzOptionString, component_map: &AzComponentMap)
    ///    -> AzResultStyledDomError
    pub render_fn: AzComponentRenderFn,
    /// Compile this component to source code in the given language
    /// fn(component_def: &AzComponentDef, target_lang: AzString,
    ///    args: &AzFilteredArgs, content: AzOptionString,
    ///    component_map: &AzComponentMap,
    ///    dom_context: &AzCompileDomContext) -> AzResultStringError
    pub compile_fn: AzComponentCompileFn,
    /// Optional destructor for custom data
    pub custom_data: AzOptionRefAny,
}

#[repr(C)]
pub struct AzComponentId {
    pub collection: AzString,   // "builtin", "shadcn", "myproject"
    pub name: AzString,         // "div", "avatar", "card"
}

#[repr(C)]
pub struct AzComponentParam {
    pub name: AzString,         // "label"
    pub param_type: AzString,   // "String"
    pub default_value: AzOptionString,
    pub description: AzString,
}

#[repr(C)]
pub enum AzChildPolicy {
    None,                        // br, hr, img, input (void elements)
    Any,                         // div, body, section
    TextOnly,                    // p, span, h1-h6
    Specific(AzStringVec),       // ul -> ["li"], table -> ["thead","tbody","tfoot","tr"]
}
```

**Key changes from the trait-based system:**

1. **`compile_fn` takes `target_lang: AzString`** — a single function handles Rust, C, C++,
   Python. The component itself decides how to generate code for each language. New languages
   can be added without ABI breakage.
2. **`compile_fn` receives `AzCompileDomContext`** — provides access to the DOM tree,
   child nodes, parent chain. Needed for `<For>`, `<If>`, `<Map>` which analyze siblings/children.
3. **`collection:name` namespacing** — `AzComponentId` has separate `collection` and `name`
   fields, enabling library grouping.
4. **`custom_data: AzOptionRefAny`** — components can carry arbitrary extra data through the
   FFI boundary (e.g., a Python class that implements the compile logic).

### 2.2 Component Instantiation vs Definition

The **definition** (`AzComponentDef`) is the template. The **instantiation** is what
appears in the DOM tree:

```rust
/// A component instance in the DOM tree — "usage" of a component definition.
#[repr(C)]
pub struct AzComponentInstance {
    /// Which component def this instantiates (collection:name)
    pub component_id: AzComponentId,
    /// Concrete argument values: { "label": "Click me", "color": "red" }
    pub arguments: AzStringPairVec,
    /// Data binding expressions: { "counter": "{data.counter}" }
    pub bindings: AzStringPairVec,
}
```

In the DOM tree, a `<shadcn:Avatar image="{data.profile_pic}" />` becomes an
`AzComponentInstance` with `component_id = { collection: "shadcn", name: "avatar" }`,
`arguments = [("image", "{data.profile_pic}")]`.

### 2.3 `AzCompileDomContext` — Compilation Context

For structural components like `<For>`, `<If>`, `<Map>`, the compile function needs
to see the DOM context:

```rust
#[repr(C)]
pub struct AzCompileDomContext {
    /// The XmlNode being compiled (includes children, attributes)
    pub current_node: *const AzXmlNode,
    /// The full component map (to resolve nested component references)
    pub component_map: *const AzComponentMap,
    /// The current app state snapshot (for data binding resolution)
    pub app_state: *const AzJson,
    /// Indentation level for code formatting
    pub indent_level: u32,
    /// Variables currently in scope (from parent For/Map)
    pub scope_variables: AzStringPairVec,
}
```

A `<For>` component's `compile_fn` would:
1. Read its `each="{data.items}"` attribute
2. Read its `as="item"` attribute
3. Iterate its children from `current_node`
4. Determine the item type from the app state snapshot
5. Generate target-language-specific iteration code:

```rust
// compile_fn for ForComponent, target_lang = "rust":
"for item in data.items.iter() {\n    {children_code}\n}"

// target_lang = "c":
"for (size_t i = 0; i < data->items_len; i++) {\n    Item* item = &data->items[i];\n    {children_code}\n}"

// target_lang = "python":
"for item in data.items:\n    {children_code}"
```

### 2.4 Format Specifiers in `split_dynamic_string`

Extend the parser to handle `{var:spec}`:

```rust
pub enum DynamicItem {
    Var {
        name: String,
        format_spec: Option<String>,  // {counter:?} -> name="counter", spec="?"
    },
    Str(String),
}
```

Supported specifiers (matching Rust `std::fmt`):
- `{var:?}` — Debug format
- `{var:#?}` — Pretty-print debug
- `{var:.2}` — 2 decimal places
- `{var:>10}` — Right-align, width 10
- `{var:05}` — Zero-padded, width 5

During compilation, these translate directly:
- **Rust:** `format!("{var:?}", var)` (native support)
- **C:** `printf("%-10s", var)` or custom formatting
- **Python:** `f"{var:>10}"` (native support)

### 2.5 Component Libraries (Collections)

A **component library** is a named collection of `AzComponentDef` entries:

```rust
#[repr(C)]
pub struct AzComponentLibrary {
    /// Library identifier, e.g. "shadcn", "myproject"
    pub name: AzString,
    /// Version string
    pub version: AzString,
    /// Human-readable description
    pub description: AzString,
    /// The components in this library
    pub components: AzComponentDefVec,
}
```

**Sources of component libraries:**

| Source | How it works |
|--------|-------------|
| **Builtin** | `AzComponentLibrary { name: "builtin", components: [div, p, a, ...] }` — compiled into the DLL, always available |
| **JSON definition file** | `.azul-components.json` loaded at runtime, parsed into `AzComponentLibrary` |
| **Compiled plugin** | A shared library (`.so`/`.dll`) that exports `extern "C" fn get_component_library() -> AzComponentLibrary` |
| **Debugger "Create Component"** | User extracts a DOM subtree, stored as JSON component def, added to a user library |

**JSON format for component definitions:**

```json
{
  "name": "shadcn",
  "version": "0.1.0",
  "description": "Port of shadcn/ui components",
  "components": [
    {
      "name": "avatar",
      "display_name": "Avatar",
      "description": "A user avatar with image and fallback initials",
      "parameters": [
        { "name": "image", "type": "String", "default": "", "description": "Image URL" },
        { "name": "fallback", "type": "String", "default": "?", "description": "Fallback text" },
        { "name": "size", "type": "String", "default": "40px", "description": "Avatar diameter" }
      ],
      "accepts_text": false,
      "child_policy": "None",
      "scoped_css": ".avatar { border-radius: 50%; overflow: hidden; } .avatar img { width: 100%; height: 100%; object-fit: cover; }",
      "example_xml": "<shadcn:Avatar image=\"{data.profile}\" size=\"48px\" />",
      "template": "<div class=\"avatar\" style=\"width: {size}; height: {size};\"><img src=\"{image}\" /><span class=\"fallback\">{fallback}</span></div>"
    }
  ]
}
```

For JSON-defined components, `render_fn` and `compile_fn` use a **system-defined callback**
that acts on the component's current JSON definition. This is the existing "compile XML to
Rust" pattern: the system provides one generic `render_fn` / `compile_fn` implementation
that reads the component's `template` field (the XML body), resolves argument substitutions,
and either renders it to `StyledDom` (at runtime) or compiles it to source code (at export
time). The component author does NOT write their own `render_fn` / `compile_fn` — the system
defines these callbacks based on the JSON definition, analogous to how `DynamicXmlComponent`
already works today (`xml.rs:4319`: its `compile_to_rust_code` is a stub `"Dom::create_div()"`
but its `render_dom` already does full template expansion via `render_dom_from_body_node_inner`).

### 2.6 The Component Map (replaces `XmlComponentMap`)

```rust
#[repr(C)]
pub struct AzComponentMap {
    /// Libraries indexed by name. "builtin" is always present.
    pub libraries: AzComponentLibraryVec,
}

impl AzComponentMap {
    /// Qualified lookup: "shadcn:avatar" -> finds library "shadcn", component "avatar"
    pub fn get(&self, collection: &str, name: &str) -> Option<&AzComponentDef> { ... }

    /// Unqualified lookup: "div" -> searches ONLY the "builtin" library.
    /// This is the shorthand for HTML elements: "a" resolves to "builtin:a".
    /// Non-builtin components MUST be referenced with their collection prefix.
    pub fn get_unqualified(&self, name: &str) -> Option<&AzComponentDef> {
        self.get("builtin", name)
    }
}
```

**Export policy:** Builtin and compiled (DLL-provided) components are NEVER exported —
they are always available from the runtime. Only user-created libraries (JSON-defined
or debugger-created) are included in exports.

---

## 3. Debugger Integration

### 3.1 Component Tab Redesign

The sidebar shows libraries as collapsible groups:

```
> builtin (52)
    div, p, span, a, button, ...
> shadcn (12)
    Avatar, Button, Card, Dialog, ...
> myproject (3)
    UserCard, Sidebar, NavMenu
```

Each component detail panel shows:
- **Display name** + collection badge
- **Description** (markdown)
- **Parameters table** (name, type, default, description)
- **Example XML**
- **Live preview** — renders the component using the current app state snapshot
- **Scoped CSS** (editable)
- **Template** (editable for JSON-defined components; read-only for compiled)

### 3.2 "Create Component" from DOM

Right-click a node in Inspector -> "Create Component":

1. Extracts the subtree as XML template
2. Shows a dialog: component name, library target (default: "myproject"), parameters
3. Analyzes data bindings in the subtree (any `{data.*}` expressions become parameters)
4. Creates a JSON component definition
5. Registers it in the component map
6. Replaces the original subtree with `<myproject:ComponentName ... />`
7. In the DOM tree, the component's internal children render in **grey** (non-editable)

### 3.3 Context Menu Update

```
Insert child >
    builtin  >  div, span, p, button, text, ...
    shadcn   >  Avatar, Button, Card, ...
    myproject >  UserCard, Sidebar, ...
---
Create Component from selection...
---
Delete node
```

### 3.4 "Export" Menu

```
Export >
    Project as JSON
    E2E Tests (CLI format)
    ---
    Component Library (JSON)
    ---
    Code >  Rust
            C
            C++ (C++23)
            Python
```

"Export -> Code -> Rust" triggers `{ op: "export_code", language: "rust" }`.
The server compiles all components via `compile_fn(target_lang="rust")`,
generates project scaffold, returns base64 ZIP.

"Export -> Component Library" exports only user-defined (non-builtin, non-compiled)
component definitions as a `.azul-components.json` file. Builtin and DLL-provided
components are always available from the runtime and are excluded from export.

### 3.5 Component Preview with App State Snapshots

The component tab's "Live Preview" uses the existing snapshot system:

1. User saves a snapshot (`app.state.snapshots["test-data"]`)
2. In the component detail, a dropdown selects which snapshot to preview with
3. The preview calls `render_fn` with the snapshot data resolved into the bindings
4. This enables real `<For>` iteration — the app IS running, the data IS available

### 3.6 DOM Tree: Component Children in Grey

When the DOM tree encounters a node that is a component instantiation:

```
> div#root
    > <shadcn:Avatar image="{data.pic}">     [blue -- component tag]
        div.avatar                            [grey -- read-only, part of component]
          img                                 [grey]
          span.fallback                       [grey]
    <builtin:p>Hello</builtin:p>              [normal]
```

Component internal nodes are rendered with `opacity: 0.5` and clicks on them
select the parent component instance, not the internal node.

---

## 4. Persistence & Project JSON

### 4.1 What Gets Stored

The project JSON (`azul-debugger-project.json`) gains:

```json
{
  "version": 3,
  "libraries": [
    {
      "name": "myproject",
      "components": [ "..." ]
    }
  ],
  "snapshots": { },
  "tests": [ ],
  "cssOverrides": { }
}
```

Builtin components are NEVER stored — they're always available from the DLL.
Only user-created libraries are persisted.

### 4.2 Import / Export Libraries

- **Import Component Library:** Load a `.azul-components.json`, merge into project
- **Export Component Library:** Save user-defined libraries (excluding builtins)
- **Remove Component:** Right-click in component tab -> "Remove" (disabled for builtins)
- **On restart:** Libraries from the project JSON are reloaded into the component map

---

## 5. Code Generation Architecture

### 5.1 How `compile_fn` Works Per Language

Each component's `compile_fn` receives `target_lang` and produces source code.
For JSON-defined (template-based) components, a single generic implementation handles
all languages by expanding the template differently:

| Language | `Dom::create_div()` | `Dom::create_text("hello")` | `{var}` substitution |
|----------|--------------------|-----------------------------|---------------------|
| Rust | `Dom::create_div()` | `Dom::create_text(AzString::from_const_str("hello"))` | `format!("{}", var)` |
| C | `AzDom_createDiv()` | `AzDom_createText(AzString_fromConstStr("hello"))` | `snprintf(buf, ..., var)` |
| C++ | `Dom::create_div()` | `Dom::create_text(String("hello"))` | `std::to_string(var)` |
| Python | `Dom.div()` | `Dom.text("hello")` | `str(var)` or f-string |

### 5.2 Generated Project Structure

**Rust:**
```
my-app/
  Cargo.toml
  src/
    main.rs                    # App::create + layout + callbacks
    components/
      mod.rs                   # pub mod shadcn; pub mod myproject;
      shadcn/
        mod.rs                 # pub mod avatar;
        avatar.rs              # pub fn create(args) -> Dom { ... }
      myproject/
        mod.rs
        user_card.rs
  component_defs/
    myproject.json             # Check-in-able JSON definitions
```

Usage from main.rs:
```rust
use crate::components::shadcn::avatar;

fn layout(data: RefAny, _: LayoutCallbackInfo) -> StyledDom {
    let data = data.downcast_ref::<AppState>().unwrap();
    let mut body = Dom::create_body();
    body.add_child(
        avatar::create(data.profile_pic.clone())
            .dom()
    );
    body.style(Css::empty())
}
```

### 5.3 Component Data Model, Callbacks, and Backreferences

Components need to advertise not just their visual parameters, but their full **data
structure** — what data they act upon, what callbacks they accept, what backreferences
they require. This is critical because:

1. The debugger GUI builder needs to know what data/callbacks to wire up
2. Code export must generate the correct struct definitions and callback hookups
3. Components form a backreference chain (see `doc/guide/architecture.md`)

**Backreference pattern (existing architecture):** In Azul, a lower-level widget stores
a `RefAny` + `Callback` pair pointing to its parent's data. When an event fires, the
widget follows this chain: `TextInput → NumberInput → AgeInput`. Each level knows only
about its immediate parent. This decouples the State Graph from the Visual Tree.

`RefAny` is itself a recognized **type** in the component system — meaning: "here we
pass in a backreference that a user-defined callback can act upon." When a component
parameter has type `RefAny`, it signals that this slot receives a backreference from
the parent, not a static value.

The `api.json` already defines the full callback type system used for code generation
across all languages:

```json
// api.json: callback_typedef pattern
"NumberInputOnFocusLostCallbackType": {
    "callback_typedef": {
        "fn_args": [
            { "type": "RefAny" },
            { "type": "CallbackInfo" },
            { "type": "NumberInputState" }
        ],
        "returns": { "type": "Update" }
    }
}

// api.json: widget struct with callback slots
"NumberInput": {
    "struct_fields": [{
        "number_input_state": { "type": "NumberInputStateWrapper" },
        "text_input": { "type": "TextInput" },
        "style": { "type": "CssPropertyWithConditionsVec" }
    }],
    "functions": {
        "set_on_value_change": {
            "fn_args": [
                { "self": "refmut" },
                { "data": "RefAny" },
                { "callback": "NumberInputOnValueChangeCallbackType" }
            ]
        },
        "dom": {
            "fn_args": [{ "self": "value" }],
            "returns": { "type": "Dom" }
        }
    }
}
```

The component definition struct should therefore also advertise:

```rust
#[repr(C)]
pub struct AzComponentCallbackSlot {
    /// Slot name, e.g. "on_value_change", "on_focus_lost"
    pub name: AzString,
    /// The callback type name from api.json, e.g. "NumberInputOnValueChangeCallbackType"
    pub callback_type: AzString,
    /// Human-readable description
    pub description: AzString,
}

#[repr(C)]
pub struct AzComponentDataField {
    /// Field name, e.g. "number", "text"
    pub name: AzString,
    /// Type name from api.json type system, e.g. "f32", "String", "RefAny"
    /// "RefAny" signals: this is a backreference slot
    pub field_type: AzString,
    /// Default value (JSON-encoded)
    pub default_value: AzOptionString,
    /// Human-readable description
    pub description: AzString,
}
```

And `AzComponentDef` gains:

```rust
pub struct AzComponentDef {
    // ... existing fields ...

    /// The data structure this component operates on (its "state")
    /// Fields with type "RefAny" are backreference slots.
    pub data_model: AzComponentDataFieldVec,
    /// Callback slots this component exposes for parent wiring
    /// Each slot references a CallbackTypeDef from api.json
    pub callback_slots: AzComponentCallbackSlotVec,
}
```

This enables the debugger to show:
- "This Avatar component needs: `image: String`, `fallback: String`" (parameters)
- "This NumberInput accepts callbacks: `on_value_change(RefAny, CallbackInfo, NumberInputState) -> Update`"
- "This slot takes a `RefAny` — wire it to your app state" (backreference)

And code export generates the correct struct + callback wiring for each target language,
using the same `callback_typedef` patterns already in `api.json`.

### 5.4 Structural Components Code Generation

**`<For>`:**
```xml
<For each="{data.items}" as="item">
    <li>{item.name}</li>
</For>
```

Compiles to (Rust):
```rust
let mut children = Vec::new();
for item in data.items.iter() {
    children.push(
        Dom::create_node(NodeType::Li)
            .with_children(vec![
                Dom::create_text(format!("{}", item.name))
            ].into())
    );
}
dom.with_children(children.into())
```

Compiles to (C):
```c
for (size_t _i = 0; _i < data->items.length; _i++) {
    Item* item = &data->items.ptr[_i];
    AzDom li = AzDom_createNode(AzNodeType_Li);
    /* ... snprintf + AzDom_createText ... */
    AzDom_addChild(&container, li);
}
```

Compiles to (Python):
```python
for item in data.items:
    container.add_child(
        Dom.node(NodeType.Li).with_child(Dom.text(str(item.name)))
    )
```

**`<If>`:**
```xml
<If condition="{data.logged_in}">
    <span>Welcome, {data.user_name}!</span>
</If>
```

Compiles to (Rust):
```rust
if data.logged_in {
    dom.add_child(Dom::create_node(NodeType::Span)
        .with_children(vec![
            Dom::create_text(format!("Welcome, {}!", data.user_name))
        ].into()));
}
```

---

## 6. Migration Path

### Phase 1: `AzComponentDef` struct + format specifiers

1. Define `AzComponentDef`, `AzComponentId`, `AzComponentParam`, `AzChildPolicy` as `repr(C)` structs
2. Implement conversion: existing `html_component!` macro generates `AzComponentDef` with
   collection = "builtin"
3. Add `collection:name` lookup to the component map
4. Extend `split_dynamic_string` to parse format specifiers (`{var:spec}`)
5. Update `compile_and_format_dynamic_items` to emit format specifiers per language
6. Keep `XmlComponentTrait` temporarily as internal implementation detail — each builtin
   component's `render_fn`/`compile_fn` delegates to the trait (thin wrapper)

### Phase 2: JSON component definitions

1. Add `serde::Deserialize` for `AzComponentDef` (the JSON schema from S2.5)
2. Implement template-based generic `render_fn` and `compile_fn` for JSON components
3. Add `AzComponentLibrary` JSON load/save
4. Update debug server: `get_component_registry` returns libraries with metadata
5. Add `import_component_library` / `export_component_library` debug API endpoints

### Phase 3: Debugger UI

1. Redesign component sidebar: library groups, search, icons
2. Component detail panel: params table, CSS editor, template editor, live preview
3. "Create Component" from context menu
4. Grey rendering of component internals in DOM tree
5. Context menu: nested library -> component insertion
6. Snapshot-based preview in component tab

### Phase 4: Code export (Rust first)

1. Add `export_code` debug API endpoint
2. Implement Rust project scaffold generator
3. Each component's `compile_fn` handles `target_lang = "rust"`
4. ZIP packaging + base64 response
5. Debugger "Export -> Code -> Rust" menu item

### Phase 5: Multi-language code export

1. Teach builtin components' `compile_fn` to handle "c", "cpp", "python"
2. Template-based approach for JSON components: per-language node creation patterns
3. Build file templates per language
4. For/If/Map structural components: per-language iteration/conditional patterns

### Phase 6: Source-aware export

1. Track `source_file` per component — where the generated code lives
2. On re-export, detect which components changed vs unchanged
3. Preserve user-modified sections (marked with `// USER CODE START` / `// USER CODE END`)

---

## 7. Files to Modify

| File | Changes |
|------|---------|
| `core/src/xml.rs` | Add `AzComponentDef`, `AzComponentId`, `AzComponentLibrary` (repr(C)), extend `split_dynamic_string` for format specifiers, implement `ForRenderer`/`IfRenderer`/`MapRenderer`, bridge existing components to new struct |
| `core/src/callbacks.rs` | Add `AzComponentRenderFn`, `AzComponentCompileFn` type aliases |
| `dll/src/desktop/shell2/common/debug_server.rs` | Update `ComponentInfo` to derive from `AzComponentDef`, add `export_code` / `import_component_library` / `export_component_library` endpoints |
| `dll/src/desktop/shell2/common/debugger/debugger.html` | "Export -> Code" submenu, "Export -> Component Library", updated context menu with library sub-menus |
| `dll/src/desktop/shell2/common/debugger/debugger.js` | Component sidebar redesign (library groups), component detail panel, "Create Component" dialog, code export handler, library import/export, grey DOM nodes for component internals |
| `dll/src/desktop/shell2/common/debugger/debugger.css` | Component panel styles, grey opacity for component internals |
| `dll/azul.h` | Generated: `AzComponentDef`, `AzComponentId`, `AzComponentLibrary` C structs + functions |

---

## 8. Open Questions

1. **How does `<For>` determine the item type?** At runtime it uses the app state snapshot
   (the app IS running). At compile time, the user can annotate the item type explicitly
   (`<For each="{data.items}" as="item" type="Item">`) or the compiler can infer it from
   the data model struct definition.

2. **Should structural components (For/If/Map) be in the "builtin" collection or a separate
   "control" collection?** Proposal: `control:for`, `control:if`, `control:map` — keeps
   them distinct from HTML elements.

3. **CSS scoping enforcement:** Should we add a true shadow boundary (prevent parent CSS
   from leaking into components)? For code generation it doesn't matter (CSS becomes inline),
   but for live preview it affects fidelity.

4. **Component versioning:** When a component library is updated, how do existing usages
   handle breaking parameter changes? Option: store the library version in the project JSON
   and warn on mismatch.
