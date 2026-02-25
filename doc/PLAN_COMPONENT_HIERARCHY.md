# Component Hierarchy Architecture

> Analysis of how azul components relate to DOM nodes, and how the debugger
> should visualize / edit / generate code for nested component trees.

## 1. Current State

### 1.1 Rust Structures

| Struct | File | Key Fields |
|--------|------|------------|
| `NodeData` | `core/src/dom.rs:1308` | `node_type`, `dataset: OptionRefAny`, `ids_and_classes`, `attributes`, `callbacks`, `css_props`, `contenteditable`, `extra: Option<Box<NodeDataExt>>` |
| `Dom` | `core/src/dom.rs:3251` | `root: NodeData`, `children: DomVec`, `estimated_total_children` |
| `ComponentDef` | `core/src/xml.rs:1976` | `id`, `display_name`, `description`, `css`, `source`, `data_model: ComponentDataModel`, `render_fn`, `compile_fn` |
| `ComponentDataModel` | `core/src/xml.rs:1503` | `name`, `description`, `fields: ComponentDataFieldVec` |
| `ComponentDataField` | `core/src/xml.rs:1470` | `name`, `field_type: ComponentFieldType`, `default_value`, `required`, `description` |
| `ComponentFieldType` | `core/src/xml.rs:1260` | `String`, `Bool`, `I32`…`F64`, `ColorU`, `CssProperty`, `ImageRef`, `FontRef`, **`StyledDom`**, `Callback(sig)`, `RefAny(hint)`, `OptionType(box)`, `VecType(box)`, `StructRef(name)`, `EnumRef(name)` |
| `ComponentDefaultValue` | `core/src/xml.rs:1330` | …, `ComponentInstance(ComponentInstanceDefault)` |
| `ComponentInstanceDefault` | `core/src/xml.rs:1362` | `library`, `component`, `field_overrides: ComponentFieldOverrideVec` |
| `ChildPolicy` | `core/src/xml.rs:1838` | `NoChildren`, `AnyChildren`, `TextOnly` (standalone enum, NOT a field on `ComponentDef`) |
| `ComponentArguments` | `core/src/xml.rs:146` | `args`, `accepts_text: bool` — old compile pipeline only |

### 1.2 Debug API Structures

| Struct | File | Notes |
|--------|------|-------|
| `HierarchyNodeInfo` | `debug_server.rs:771` | `index`, `node_type`, `tag`, `id`, `classes`, `text`, `parent`, `children`, `events`, `rect`, `tab_index`, `contenteditable` — **no `dataset`, no `component`** |
| `ComponentInfo` | `debug_server.rs:283` | `tag`, `qualified_name`, `display_name`, `description`, `source`, `data_model`, `universal_attributes`, `callback_slots`, `css` — **no `child_policy`, `accepts_text`, `template`, `example_xml`** |

### 1.3 Debugger JS Tree

`debugger.js:506` — `renderDomTree()` / `_renderTreeNode()` renders a flat hierarchy as an
indented tree.  Each node shows `tag`, `#id`, `.classes`, event badges.  There is **no concept
of which component produced a given DOM subtree**.

---

## 2. Removals (Already Done in Rust)

These fields/concepts were removed from the Rust `ComponentDef` or never added.  They must be
removed from all plan documents and JS code:

| Field | Rationale |
|-------|-----------|
| `template` | Components are defined by `render_fn`, not raw XML template strings. |
| `example_xml` | Preview uses `render_fn` with default data model values; no separate example needed. |

**JS impact**: `showComponentDetail()` contains a "Template" section (displaying `component.template`)
and an "Example" section (displaying `component.example_xml`).  Both should be removed since the
values are always `undefined`.

---

## 3. `accepts_text` → Data Model "text" Field

### 3.1 Problem

The old `ComponentArguments.accepts_text: bool` flag (xml.rs:148) told the compile pipeline
to inject a `text: AzString` parameter.  The new `ComponentDef` uses a unified data model, so a
dedicated `accepts_text` flag is redundant.

### 3.2 Solution

A component that accepts text content simply declares a field in its data model:

```rust
ComponentDataField {
    name: "text".into(),
    field_type: ComponentFieldType::String,
    default_value: Some(ComponentDefaultValue::String("".into())),
    required: false,
    description: "Text content of the element".into(),
}
```

The compile pipeline checks: "does the data model have a field named `text` with type `String`?"
If yes, the generated render function accepts text content.

### 3.3 Migration

- `ChildPolicy::TextOnly` → component has a `text: String` field in its data model and no
  `StyledDom`-typed fields.
- `ChildPolicy::NoChildren` → component has neither `text` nor any `StyledDom`-typed field.
- `ChildPolicy::AnyChildren` → component has at least one `StyledDom`-typed field (a "slot")
  and may or may not have a `text` field.

The `ChildPolicy` enum can remain as a derived/computed classification (useful for validation
messages) but should NOT be stored as a separate field.

### 3.4 JS Impact

Remove any `component.accepts_text` badge.  The "accepts text" property is visible when the
data model contains a `text: String` field — no special UI needed.

---

## 4.  DOM Nodes vs. Component Children

### 4.1 The Problem

When a user builds a page like:

```xml
<body>
  <div class="container">
    <MyCard title="Hello">
      <MyButton on_click="handler">Click me</MyButton>
    </MyCard>
  </div>
</body>
```

The rendered DOM tree might look like:

```
body
  div.container
    div.card            ← rendered by MyCard
      h2.card-title     ← rendered by MyCard
        "Hello"
      div.card-body     ← rendered by MyCard (slot insertion point)
        button.btn      ← rendered by MyButton
          "Click me"
```

The debugger currently shows only the flat DOM tree (the right column).  There is **no way to
know** that `div.card` through `div.card-body` were produced by `MyCard`, or that `button.btn`
was produced by `MyButton`.

### 4.2 Two Trees

We need to distinguish two views:

| Tree | What it shows | Source |
|------|---------------|--------|
| **DOM Tree** | Physical `NodeData` nodes as rendered.  Tags, classes, inline styles, events, box model. | `HierarchyNodeInfo` from the debug server |
| **Component Tree** | Logical component invocations.  `<MyCard title="Hello">` containing `<MyButton on_click="handler">`. | New — requires component origin tracking |

The DOM tree is what exists today.  The Component tree is the new requirement.

---

## 5.  `NodeData.component` — Tracking Component Origin

### 5.1 Design

Add a field to `NodeData` (or to `NodeDataExt` for ABI stability):

```rust
// In NodeDataExt (dom.rs ~1388):
pub struct NodeDataExt {
    pub clip_mask: Option<ImageMask>,
    pub accessibility: Option<AccessibilityInfo>,
    pub menu_bar: Option<Menu>,
    pub context_menu: Option<Menu>,
    pub component: Option<ComponentOrigin>,  // ← NEW
}

pub struct ComponentOrigin {
    /// Qualified component name, e.g. "shadcn:card"
    pub component_id: AzString,
    /// Invoked data model snapshot (field values at render time)
    pub data_model_values: ComponentFieldNamedValueVec,
}
```

**When is it set?**  During component rendering.  When `ComponentDef.render_fn` returns a
`StyledDom`, the framework walks the returned DOM tree and stamps each top-level node
(the root of the component's output) with
`component = Some(ComponentOrigin { component_id, data_model_values })`.

Only the **root node(s)** of the component's rendered output get the stamp — inner nodes
inherit it by tree position.

### 5.2 API Extension

`HierarchyNodeInfo` gains:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub component: Option<ComponentOriginJson>,

#[derive(Debug, Clone, serde::Serialize)]
pub struct ComponentOriginJson {
    pub component_id: String,           // "shadcn:card"
    pub data_model: serde_json::Value,  // { "title": "Hello", ... }
}
```

### 5.3 JS Tree Impact

In `_renderTreeNode()`:

- If `node.component` is set, show a component badge next to the tag:
  `div.card  [MyCard]` — the badge is a colored pill linking to the component detail view.
- In the Component Tree view, group all nodes that share the same component origin into a
  single expandable node.
- Clicking the component badge navigates to the Component Editor for that component with
  the invoked field values pre-filled.

---

## 6. Component Children via `StyledDom` Slots

### 6.1 How It Already Works

A component advertises child slots via `StyledDom`-typed fields in its data model:

```rust
ComponentDataField {
    name: "children".into(),
    field_type: ComponentFieldType::StyledDom,
    default_value: None,  // typically required
    required: true,
    description: "Content to render inside the card body".into(),
}
```

The framework passes the caller-provided children as the `StyledDom` value for this field.
The component's `render_fn` inserts it at the correct position in its output DOM.

A component can have **multiple named slots**:

```rust
// Card with header slot + body slot:
fields: vec![
    ComponentDataField { name: "header", field_type: StyledDom, .. },
    ComponentDataField { name: "body",   field_type: StyledDom, .. },
]
```

### 6.2 Default Slot Values — `ComponentInstanceDefault`

The existing `ComponentInstanceDefault` struct (xml.rs:1362) already provides default
slot content:

```rust
ComponentDefaultValue::ComponentInstance(ComponentInstanceDefault {
    library: "builtin".into(),
    component: "p".into(),
    field_overrides: vec![
        ComponentFieldOverride {
            field_name: "text".into(),
            source: ComponentFieldValueSource::Literal("Default slot content".into()),
        },
    ].into(),
})
```

This means a `StyledDom` field can have a default value that is itself a component
invocation — enabling nested component trees in data models.

### 6.3 `ComponentChildVec` Pattern

For components that accept a variable number of typed children (e.g. a `<List>` that accepts
`<ListItem>` children), use:

```rust
ComponentDataField {
    name: "items".into(),
    field_type: ComponentFieldType::VecType(
        Box::new(ComponentFieldType::StructRef("ListItemData".into()))
    ),
    ..
}
```

Each `ListItemData` instance in the vec is rendered by the referenced component.  The
`ComponentDataModel` named `ListItemData` lives in `ComponentLibrary::data_models`.

For a simpler "any children" pattern, a single `StyledDom` slot suffices.

### 6.4 Debugger Visualization

In the Component Editor, `StyledDom` fields render as:

- **Slot preview**: A miniature DOM tree showing the current slot content.
- **"Edit slot"** button: Opens a sub-editor for the slot's component tree.
- **Instance list** (for `VecType(StructRef(..))`): A list of child component instances,
  each expandable into its field editor.

---

## 7. Dataset Visualization

### 7.1 Current State

`NodeData.dataset` is `OptionRefAny` — an opaque `RefAny` pointer.  The debug server
currently does **not** serialize it into `HierarchyNodeInfo`.

### 7.2 Proposal

For debugger purposes, `dataset` should be serialized as a JSON object.  This requires:

1. **Convention**: The `RefAny` stored in `dataset` should be a known type (e.g.,
   `BTreeMap<String, String>` or a component data model snapshot).

2. **Debug serialization**: When building `HierarchyNodeInfo`, attempt to extract
   `dataset` via a registered debug formatter.  If the `RefAny` has a `Debug` impl or
   a registered serializer, include it:

```rust
// In HierarchyNodeInfo:
#[serde(skip_serializing_if = "Option::is_none")]
pub dataset: Option<serde_json::Value>,
```

3. **JS display**: In `renderNodeDetail()`, add a "Dataset" section that renders the
   JSON object as a collapsible key-value tree (reusing the existing app state inspector
   pattern).

### 7.3 Fallback

If `dataset` is an opaque `RefAny` with no debug formatter, show:
`dataset: RefAny(type_id=0x..., size=N bytes)` — at minimum confirming its presence.

---

## 8. Code Generation Impact

### 8.1 Current Compile Pipeline

`compile_component()` (xml.rs:3779) generates:

```rust
pub fn render(text: AzString, arg1: Type1, arg2: Type2) -> Dom { ... }
```

The `text` parameter is conditional on `ComponentArguments.accepts_text`.  The other `args`
come from `ComponentArguments.args`.

### 8.2 New Compile Pipeline

With the unified data model, code generation uses `ComponentDef.data_model`:

```rust
// For a Card component with data model:
//   - title: String
//   - children: StyledDom
//   - show_border: bool

pub struct CardData {
    pub title: AzString,
    pub children: StyledDom,
    pub show_border: bool,
}

pub fn render(data: CardData) -> Dom { ... }
```

Steps:
1. Generate a struct from `data_model.name` + `data_model.fields`.
2. `StyledDom` fields become `StyledDom` parameters (slots).
3. `Callback` fields become callback type parameters.
4. `StructRef` / `EnumRef` fields reference their `ComponentDataModel` / `ComponentEnumModel`
   from `ComponentLibrary::data_models` / `enum_models`.
5. A `text: String` field generates the text content parameter (replaces `accepts_text`).

### 8.3 Roundtrip: Component Tree → XML → Code

```
Component Tree (debugger)
    → Serialize to XML: <Card title="Hello"><Button>Click</Button></Card>
    → Parse XML, resolve components, compile
    → Rust code with struct definitions + render functions
```

The `NodeData.component` field enables the reverse direction:
```
Rendered DOM (debug server)
    → Read component origin stamps
    → Reconstruct component invocation tree
    → Display in Component Tree view
```

---

## 9. Implementation Plan

### Phase 1: Cleanup (JS-only, no Rust changes)

| Task | Details |
|------|---------|
| Remove `template` section from `showComponentDetail()` | JS reads `component.template` which is always `undefined` |
| Remove `example_xml` section from `showComponentDetail()` | Same — always `undefined` |
| Remove `child_policy` / `accepts_text` badge rendering | Not in `ComponentInfo` API |
| Fix CSS field name: `scoped_css` → `css` | `_saveComponentCss` sends `scoped_css`, backend expects `css`. `showComponentDetail` reads `component.scoped_css`, API sends `css`. |
| Fix `field_type` string parsing in `DataModelEditor` | API sends `field_type` as string (e.g. `"String"`). Widget `FieldInput.render` dispatches on `ft.type`. Need parse layer: `"String"` → `{type:"String"}`, `"Option<String>"` → `{type:"Option", inner:{type:"String"}}` |

### Phase 2: `NodeData.component` (Rust)

| Task | Details |
|------|---------|
| Add `ComponentOrigin` struct | `component_id: AzString`, `data_model_values: ComponentFieldNamedValueVec` |
| Add `component: Option<ComponentOrigin>` to `NodeDataExt` | ABI-safe extension point |
| Stamp component origin during `render_fn` invocation | In the component rendering pipeline |
| Serialize to `HierarchyNodeInfo.component` | `ComponentOriginJson` in debug_server |
| Add `dataset` serialization to `HierarchyNodeInfo` | Best-effort JSON extraction |

### Phase 3: Component Tree View (JS)

| Task | Details |
|------|---------|
| Add component badge to `_renderTreeNode()` | `[MyCard]` pill when `node.component` is set |
| Add "Component Tree" tab next to "DOM Tree" | Groups nodes by component origin |
| Slot preview in Component Editor | For `StyledDom` fields, show miniature tree |
| Dataset section in node detail | Collapsible JSON key-value display |

### Phase 4: Code Generation Update (Rust)

| Task | Details |
|------|---------|
| New `compile_fn` signature using `ComponentDataModel` | Generate struct + render fn from unified model |
| Handle `StyledDom` slots as `StyledDom` params | Slot fields → function parameters |
| Handle `VecType(StructRef(..))` as repeated children | Generate iteration code |
| Deprecate `ComponentArguments` | Replace references with `ComponentDataModel` |

---

## 10. JS Bug Fixes Required (Phase 1 Details)

### 10.1 CSS Field Name Mismatch

**`_saveComponentCss()`** sends:
```javascript
body: JSON.stringify({ scoped_css: cssText })  // WRONG
```
Backend `DebugEvent::UpdateComponent` expects:
```rust
css: Option<String>  // field name is "css"
```

**Fix**: Change to `{ css: cssText }`.

**`showComponentDetail()`** reads:
```javascript
component.scoped_css  // WRONG — undefined
```
API `ComponentInfo` sends field as `css`.

**Fix**: Change to `component.css`.

### 10.2 field_type Parsing

`ComponentDataFieldInfo.field_type` from the API is a flat string:
- `"String"`, `"bool"`, `"i32"`, `"Option<String>"`, `"Vec<TodoItem>"`, `"StyledDom"`, etc.

`FieldInput.render()` dispatches on `fieldType.type`:
```javascript
var ft = typeof fieldType === 'object' ? fieldType : { type: fieldType };
```

**Fix**: Add a `_parseFieldType(str)` function that converts:
- `"String"` → `{ type: "String" }`
- `"Option<String>"` → `{ type: "Option", inner: { type: "String" } }`
- `"Vec<i32>"` → `{ type: "Vec", inner: { type: "i32" } }`
- `"StyledDom"` → `{ type: "StyledDom" }`
- `"Callback(ButtonOnClick)"` → `{ type: "Callback", signature: "ButtonOnClick" }`

Call this in `DataModelEditor.render()` before passing to `FieldInput`.

---

## 11. Summary of Decisions

1. **`template` and `example_xml`**: Removed.  Components render via `render_fn` with
   default data model values.
2. **`accepts_text`**: Expressed via a `text: String` field in the data model.
3. **`child_policy`**: Computed from data model shape, not stored.
4. **Component vs. DOM tree**: Two separate views.  DOM tree = physical nodes.
   Component tree = logical invocations.
5. **`NodeData.component`**: New `ComponentOrigin` in `NodeDataExt` stamps which component
   rendered each DOM subtree root.
6. **`dataset`**: Serialize to JSON in `HierarchyNodeInfo` for debugger display.
7. **`ComponentChildVec`**: Modeled as `VecType(StructRef(..))` data model fields.
   Simple child slots use `StyledDom` fields.
8. **Code generation**: Uses `ComponentDataModel` struct directly.  `ComponentArguments`
   is legacy.
