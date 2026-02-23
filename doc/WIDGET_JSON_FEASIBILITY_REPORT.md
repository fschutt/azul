# Widget JSON Definability Report

**Question:** Given the builtin types (div, span, text, etc.), can every widget
in `layout/src/widgets/` be defined via pure JSON using the proposed
`ComponentFieldType` system — with callbacks as stubs?

**Short answer:** 13 of 17 widgets: **yes, fully definable via JSON.**
4 widgets have caveats that require design accommodations but are still
feasible with minor extensions.

---

## 1. Widget-by-Widget Analysis

### Legend

| Symbol | Meaning |
|--------|---------|
| ✅ | Fully definable via JSON data model + XML template |
| ⚠️ | Definable but needs a noted workaround or type system extension |
| ❌ | Cannot be expressed in the proposed type system |

---

### 1.1 Label ✅

**State struct:**
```rust
Label { string: AzString, label_style: CssPropertyWithConditionsVec }
```

**JSON data model:**
```json
{
    "name": "LabelData",
    "fields": [
        { "name": "string", "type": "String", "required": true },
        { "name": "label_style", "type": "CssProperty", "required": false }
    ]
}
```

**Template:** `<div class="__azul-native-label">{{ string }}</div>`

**Callbacks:** None.

**Verdict:** Trivially expressible. Just a text node with styling. No
impediments whatsoever.

---

### 1.2 Button ✅

**State struct:**
```rust
Button {
    label: AzString,
    image: OptionImageRef,
    button_type: ButtonType,      // enum: Default|Primary|Secondary|Success|Danger|Warning|Info|Link
    container_style / label_style / image_style: CssPropertyWithConditionsVec,
    on_click: OptionButtonOnClick,
}
```

**JSON data model:**
```json
{
    "name": "ButtonData",
    "fields": [
        { "name": "label", "type": "String", "required": true },
        { "name": "image", "type": "ImageRef?", "required": false },
        { "name": "button_type", "type": "ButtonType", "default": "Default" },
        { "name": "on_click", "type": "fn() -> Update", "required": false }
    ]
}
```

**Enum:** `ButtonType { Default, Primary, Secondary, Success, Danger, Warning, Info, Link }`

**Template:**
```xml
<button class="__azul-native-button {{ button_type.class_name }}">
    <img if="{{ image }}" src="{{ image }}" />
    <text>{{ label }}</text>
</button>
```

**Verdict:** Fully expressible. The `ButtonType` enum maps to `ComponentEnumModel`.
The platform-specific styling (Windows/Linux/Mac gradients) is a
`CssPropertyWithConditionsVec` concern — in JSON it would either:
- Be a single platform-agnostic CSS (the simplest path), or
- Use the `button_type` field to pick from pre-defined style presets

The internal `build_button_container_style()` function dynamically constructs
CSS based on `ButtonType` — this logic can't be expressed in static JSON/CSS.
**But** that's a rendering concern, not a data model concern. A JSON-defined
button would use simpler CSS and still be fully functional. The compiled
version can always override with the sophisticated per-platform styling.

---

### 1.3 CheckBox ✅

**State:**
```rust
CheckBoxState { checked: bool }
on_toggle: fn(CheckBoxState) -> Update
```

**JSON data model:**
```json
{
    "name": "CheckBoxData",
    "fields": [
        { "name": "checked", "type": "Bool", "default": false },
        { "name": "on_toggle", "type": "fn(CheckBoxState) -> Update", "required": false }
    ]
}
```

**Template:**
```xml
<div class="__azul-native-checkbox-container" tabindex="auto">
    <div class="__azul-native-checkbox-content" />
</div>
```

**Callback behavior:** The `default_on_checkbox_clicked` handler toggles
`checked` and changes CSS opacity of the inner div. This is an internal
event handler pattern: read state → mutate → update CSS.

**Verdict:** Fully expressible. The toggle behavior is a generic
"click toggles boolean + updates CSS" pattern that a template engine could
support natively. For JSON definition, the `on_toggle` callback would be a
stub. The actual toggle logic could be a builtin behavior annotation like
`"behavior": "toggle_bool"` or just shipped as the default compiled handler.

---

### 1.4 ColorInput ✅

**State:**
```rust
ColorInputState { color: ColorU }
on_value_change: fn(ColorInputState) -> Update
title: AzString  // dialog title
```

**JSON data model:**
```json
{
    "name": "ColorInputData",
    "fields": [
        { "name": "color", "type": "ColorU", "default": "#FFFFFF" },
        { "name": "title", "type": "String", "default": "Pick color" },
        { "name": "on_value_change", "type": "fn(ColorInputState) -> Update", "required": false }
    ]
}
```

**Template:**
```xml
<div class="__azul_native_color_input" style="background-color: {{ color }}" />
```

**Verdict:** Fully expressible. The click handler opens a color dialog — that's
a platform action, not something a JSON component needs to implement. The
callback `on_value_change` is a stub. The actual dialog-opening is a compiled
behavior.

---

### 1.5 FileInput ✅

**State:**
```rust
FileInputState { path: OptionString }
on_path_change: fn(FileInputState) -> Update
default_text: AzString
file_dialog_title: AzString
```

**JSON data model:**
```json
{
    "name": "FileInputData",
    "fields": [
        { "name": "path", "type": "String?", "default": null },
        { "name": "default_text", "type": "String", "default": "Select File..." },
        { "name": "file_dialog_title", "type": "String", "default": "Select File" },
        { "name": "on_path_change", "type": "fn(FileInputState) -> Update", "required": false }
    ]
}
```

**Template:** Delegates to `Button.dom()` — i.e. it's a composed component:
```xml
<builtin.button label="{{ path ?? default_text }}" on_click="{{ fileinput_on_click }}" />
```

**Verdict:** Fully expressible as a component that instantiates `builtin.button`
in a `StyledDom` slot. This is exactly the recursive composition model from
section 15.5 of the design doc. The file dialog opening is a platform action.

---

### 1.6 NumberInput ✅

**State:**
```rust
NumberInputState { previous: f32, number: f32, min: f32, max: f32 }
on_value_change: fn(NumberInputState) -> Update
on_focus_lost: fn(NumberInputState) -> Update
+ delegates to TextInput internally
```

**JSON data model:**
```json
{
    "name": "NumberInputData",
    "fields": [
        { "name": "number", "type": "f32", "default": 0.0 },
        { "name": "min", "type": "f32", "default": 0.0 },
        { "name": "max", "type": "f32", "default": 3.4028235e+38 },
        { "name": "on_value_change", "type": "fn(NumberInputState) -> Update", "required": false },
        { "name": "on_focus_lost", "type": "fn(NumberInputState) -> Update", "required": false }
    ]
}
```

**Template:** Wraps `builtin.text_input`:
```xml
<builtin.text_input text="{{ number }}" on_text_input="{{ validate_text_input }}" />
```

**Verdict:** Fully expressible. It's a TextInput with validation logic. The
`validate_text_input` function (parses string → f32, clamps to min/max) is the
callback logic that would be a stub in JSON / `CallbackFnPointer` default.

---

### 1.7 ProgressBar ✅

**State:**
```rust
ProgressBarState { percent_done: f32, display_percentage: bool }
// No callbacks!
```

**JSON data model:**
```json
{
    "name": "ProgressBarData",
    "fields": [
        { "name": "percent_done", "type": "f32", "default": 0.0, "required": true },
        { "name": "display_percentage", "type": "Bool", "default": false }
    ]
}
```

**Template:**
```xml
<div class="__azul-native-progress-bar-container">
    <div class="__azul-native-progress-bar-bar" style="width: {{ percent_done }}%" />
    <div class="__azul-native-progress-bar-remaining" style="width: {{ 100 - percent_done }}%" />
</div>
```

**Verdict:** Fully expressible. No callbacks. Pure presentation. The only
complexity is the computed `width: {{ percent_done }}%` — the template engine
needs to support simple expressions (which is a reasonable requirement for
any template system).

---

### 1.8 Frame ✅

**State:**
```rust
Frame { title: AzString, flex_grow: f32, content: Dom }
```

**JSON data model:**
```json
{
    "name": "FrameData",
    "fields": [
        { "name": "title", "type": "String", "required": true },
        { "name": "flex_grow", "type": "f32", "default": 0.0 },
        { "name": "content", "type": "slot", "required": true }
    ]
}
```

**Template:**
```xml
<div class="__azul-native-frame">
    <div class="__azul-native-frame-header">
        <div class="__azul-native-frame-header-before"><div/></div>
        <p>{{ title }}</p>
        <div class="__azul-native-frame-header-after"><div/></div>
    </div>
    <div class="__azul-native-frame-content">
        <slot name="content" />
    </div>
</div>
```

**Verdict:** Fully expressible. The `content` is a `StyledDom` slot — exactly
what the design was made for. No callbacks.

---

### 1.9 DropDown ✅

**State:**
```rust
DropDown { choices: StringVec, selected: usize, on_choice_change: fn(usize) -> Update }
```

**JSON data model:**
```json
{
    "name": "DropDownData",
    "fields": [
        { "name": "choices", "type": "[String]", "required": true },
        { "name": "selected", "type": "usize", "default": 0 },
        { "name": "on_choice_change", "type": "fn(usize) -> Update", "required": false }
    ]
}
```

**Template (conceptual):**
```xml
<div class="__azul-native-dropdown">
    <div class="__azul-native-dropdown-wrapper" tabindex="auto">
        <div class="__azul-native-dropdown-focused-text">
            {{ choices[selected] }}
        </div>
        <div class="__azul-native-dropdown-arrow">▼</div>
    </div>
</div>
```

**Verdict:** Definable. The actual DropDown has a complex popup rendering
system (creates a floating menu on focus). The popup itself would need
a "popup/overlay" template concept — but the data model is simple.
The popup behavior could be a builtin behavior annotation. The `choices`
field uses `[String]` (Vec of String), which the type system supports.

---

### 1.10 TabHeader + TabContent ✅

**State:**
```rust
TabHeader { tabs: StringVec, active_tab: usize, on_click: fn(TabHeaderState) -> Update }
TabContent { content: Dom, has_padding: bool }
```

**JSON data models:**
```json
{
    "name": "TabHeaderData",
    "fields": [
        { "name": "tabs", "type": "[String]", "required": true },
        { "name": "active_tab", "type": "usize", "default": 0 },
        { "name": "on_click", "type": "fn(TabHeaderState) -> Update", "required": false }
    ]
}
```
```json
{
    "name": "TabContentData",
    "fields": [
        { "name": "content", "type": "slot", "required": true },
        { "name": "has_padding", "type": "Bool", "default": true }
    ]
}
```

**Verdict:** Fully definable. Tabs are two components (header + content panel)
that work together. The tab switching is a click callback that sets `active_tab`.

---

### 1.11 TextInput ⚠️

**State:**
```rust
TextInputState {
    text: U32Vec,                    // Vec<u32> — characters as u32 codepoints
    placeholder: OptionString,
    max_len: usize,
    selection: OptionTextInputSelection,
    cursor_pos: usize,
}
TextInputOnTextInputCallbackType → fn(TextInputState) -> OnTextInputReturn
TextInputOnVirtualKeyDownCallbackType → fn(TextInputState) -> OnTextInputReturn
TextInputOnFocusLostCallbackType → fn(TextInputState) -> Update
```

**JSON data model:**
```json
{
    "name": "TextInputData",
    "fields": [
        { "name": "text", "type": "String", "default": "" },
        { "name": "placeholder", "type": "String?", "default": null },
        { "name": "max_len", "type": "usize", "default": 4294967295 },
        { "name": "on_text_input", "type": "fn(TextInputState) -> OnTextInputReturn", "required": false },
        { "name": "on_virtual_key_down", "type": "fn(TextInputState) -> OnTextInputReturn", "required": false },
        { "name": "on_focus_lost", "type": "fn(TextInputState) -> Update", "required": false }
    ]
}
```

**Caveat 1: `U32Vec` (codepoints as u32 array)**
The internal representation uses `Vec<u32>` for characters — not `AzString`.
In JSON, this would be exposed as `String` (the natural representation).
The compiled render function converts between String ↔ U32Vec internally.
**This is fine for JSON definition** — the data model advertises `String`,
and the compiled handler does the conversion.

**Caveat 2: `OnTextInputReturn` (custom return type)**
The text input callbacks return `OnTextInputReturn { update: Update, valid: TextInputValid }`
— not just `Update`. This is a struct-typed return value. The type system
supports this via `StructRef("OnTextInputReturn")`, but it's a custom return
type not present in the base `callback_typedef` pattern (which always returns
`Update`).

**Caveat 3: Complex internal behavior**
The 500+ lines of input handling code (cursor positioning, selection,
keyboard events, cursor blinking animation via TimerId) are all in the
compiled render/event handlers. The JSON data model only describes
the interface — the behavior is compiled. This is exactly the intended
design: JSON defines the data model, compiled code provides the behavior.

**Verdict:** Definable with the noted type system extension (custom callback
return types). The `OnTextInputReturn` struct needs to be defined as an
auxiliary data model in the library's `data_models` list.

---

### 1.12 ListView ⚠️

**State:**
```rust
ListView {
    columns: StringVec,
    rows: ListViewRowVec,         // Vec<ListViewRow>
    sorted_by: OptionUsize,
    scroll_offset: PixelValueNoPercent,
    content_height: OptionPixelValueNoPercent,
    column_context_menu: OptionMenu,
    on_lazy_load_scroll: fn(ListViewState) -> Update,
    on_column_click: fn(ListViewState, usize) -> Update,
    on_row_click: fn(ListViewState, usize) -> Update,
}
ListViewRow { cells: DomVec, height: OptionPixelValueNoPercent }
```

**JSON data model:**
```json
{
    "name": "ListViewData",
    "fields": [
        { "name": "columns", "type": "[String]", "required": true },
        { "name": "rows", "type": "[ListViewRow]" },
        { "name": "sorted_by", "type": "usize?", "default": null },
        { "name": "on_lazy_load_scroll", "type": "fn(ListViewState) -> Update", "required": false },
        { "name": "on_column_click", "type": "fn(ListViewState, usize) -> Update", "required": false },
        { "name": "on_row_click", "type": "fn(ListViewState, usize) -> Update", "required": false }
    ]
}
```

**Caveat 1: `ListViewRow.cells: DomVec`**
Each row cell is an arbitrary `Dom` — not a string. This means each cell
is a `StyledDom` slot. In the type system: `[slot]` (Vec of StyledDom).
**This is already supported** by `ComponentFieldType::Vec(Box(StyledDom))`.

**Caveat 2: `OptionMenu` (context menu)**
The `column_context_menu` is an `OptionMenu` — an OS-level context menu.
This type doesn't exist in the `ComponentFieldType` enum. Options:
- Add `Menu` as a new variant to `ComponentFieldType`, or
- Model it as a `StructRef("Menu")` referencing a builtin type, or
- Leave context menus for compiled components only

**Caveat 3: Lazy-loading scroll**
The lazy-load callback receives `ListViewState` which includes
`current_scroll_position: LogicalPosition` and `current_content_height: LogicalSize`.
These are pixel-level layout metrics. The callback signature is expressible,
but the data types (`LogicalPosition`, `LogicalSize`, `PixelValueNoPercent`)
need to exist as `StructRef`s.

**Verdict:** Definable with auxiliary struct definitions for `ListViewRow`,
`ListViewState`, and the layout metric types. The `OptionMenu` field
would be best handled as an opaque `StructRef("Menu")`.

---

### 1.13 TreeView ✅

**State:**
```rust
TreeView { root: AzString }
```

**JSON data model:**
```json
{
    "name": "TreeViewData",
    "fields": [
        { "name": "root", "type": "String", "required": true }
    ]
}
```

**Verdict:** Trivially expressible. The current implementation is a static
layout demo (hardcoded tree structure in `dom()`). A real tree view would need
recursive node definitions — but even that is expressible via
`StructRef("TreeNode")` with `TreeNode { label: String, children: [TreeNode] }`.

---

### 1.14 Titlebar ✅

**State:**
```rust
Titlebar { title: AzString, height: f32, font_size: f32, padding_left: f32,
           padding_right: f32, title_color: ColorU }
```

**JSON data model:**
```json
{
    "name": "TitlebarData",
    "fields": [
        { "name": "title", "type": "String", "required": true },
        { "name": "height", "type": "f32", "default": 30.0 },
        { "name": "font_size", "type": "f32", "default": 13.0 },
        { "name": "padding_left", "type": "f32", "default": 69.0 },
        { "name": "padding_right", "type": "f32", "default": 69.0 },
        { "name": "title_color", "type": "ColorU", "default": "#333333" }
    ]
}
```

**Template:**
```xml
<div class="__azul-titlebar" style="height: {{ height }}px">
    <text style="color: {{ title_color }}; font-size: {{ font_size }}px">
        {{ title }}
    </text>
</div>
```

**Callbacks:** None in the data model. Window control buttons (minimize,
maximize, close) use builtin window control events, not component callbacks.

**Verdict:** Fully expressible. Platform-specific metrics
(`from_system_style()`) are a compiled concern, not a data model issue.

---

### 1.15 Ribbon ⚠️

**State:**
```rust
Ribbon { tab_active: i32 }
RibbonOnTabClickedCallbackType = fn(i32) -> Update
```

**JSON data model:**
```json
{
    "name": "RibbonData",
    "fields": [
        { "name": "tab_active", "type": "i32", "default": 0 },
        { "name": "on_tab_clicked", "type": "fn(i32) -> Update", "required": false }
    ]
}
```

**Caveat:** The current Ribbon implementation (2789 lines!) is 99% hardcoded
CSS styling for the Microsoft Office-style ribbon. The actual data model is
trivial (`tab_active: i32`). The heavyweight part is the static tab layout
with hardcoded tab names ("FILE", "HOME", "INSERT", etc.) and ~2300 lines
of CSS constants.

For a JSON-defined ribbon, the tab labels and ribbon sections would need to
be dynamic data, not hardcoded. This would require something like:
```json
{ "name": "tabs", "type": "[RibbonTab]" }
```
with `RibbonTab { label: String, sections: [RibbonSection] }`.

**Verdict:** The data model is trivially expressible. The problem is that
the current implementation is a static layout rather than a data-driven one.
A proper JSON-defined ribbon would need a richer data model, but that's a
widget redesign issue, not a type system limitation.

---

### 1.16 NodeGraph ⚠️

**State:**
```rust
NodeGraph {
    node_types: NodeTypeIdInfoMapVec,
    input_output_types: InputOutputTypeIdInfoMapVec,
    nodes: NodeIdNodeMapVec,
    allow_multiple_root_nodes: bool,
    offset: LogicalPosition,
    style: NodeGraphStyle,
    callbacks: NodeGraphCallbacks,   // 8 optional callbacks!
    add_node_str: AzString,
    scale_factor: f32,
}

Node { node_type: NodeTypeId, position, fields: NodeTypeFieldVec,
       connect_in: InputConnectionVec, connect_out: OutputConnectionVec }

NodeTypeFieldValue = enum { TextInput(String), NumberInput(f32),
                            CheckBox(bool), ColorInput(ColorU),
                            FileInput(OptionString) }
```

**JSON data model:**
```json
{
    "name": "NodeGraphData",
    "fields": [
        { "name": "node_types", "type": "[NodeTypeIdInfoMap]" },
        { "name": "input_output_types", "type": "[InputOutputTypeIdInfoMap]" },
        { "name": "nodes", "type": "[NodeIdNodeMap]" },
        { "name": "allow_multiple_root_nodes", "type": "Bool", "default": false },
        { "name": "scale_factor", "type": "f32", "default": 1.0 },
        { "name": "on_node_added", "type": "fn(NodeTypeId, NodeGraphNodeId, NodeGraphNodePosition) -> Update" },
        { "name": "on_node_removed", "type": "fn(NodeGraphNodeId) -> Update" },
        { "name": "on_node_dragged", "type": "fn(NodeGraphNodeId, NodeDragAmount) -> Update" },
        { "name": "on_node_connected", "type": "fn(NodeGraphNodeId, usize, NodeGraphNodeId, usize) -> Update" },
        { "name": "on_node_input_disconnected", "type": "fn(NodeGraphNodeId, usize) -> Update" },
        { "name": "on_node_output_disconnected", "type": "fn(NodeGraphNodeId, usize) -> Update" },
        { "name": "on_node_field_edited", "type": "fn(NodeGraphNodeId, usize, NodeTypeId, NodeTypeFieldValue) -> Update" },
        { "name": "on_node_graph_dragged", "type": "fn(GraphDragAmount) -> Update" }
    ]
}
```

**Caveat 1: Deep struct graph**
NodeGraph requires ~12 auxiliary struct definitions:
`NodeTypeId`, `NodeGraphNodeId`, `InputOutputTypeId`, `Node`,
`NodeTypeField`, `NodeTypeFieldValue`, `InputConnection`,
`OutputConnection`, `OutputNodeAndIndex`, `NodeTypeIdInfoMap`,
`InputOutputTypeIdInfoMap`, `NodeIdNodeMap`, `NodeGraphNodePosition`,
`NodeDragAmount`, `GraphDragAmount`, `NodeTypeInfo`, `InputOutputInfo`.

All of these are expressible as `StructRef`/`EnumRef` in the type system.

**Caveat 2: Complex rendering logic**
The NodeGraph's `dom()` function (3700+ lines) implements custom canvas-like
rendering with positioned nodes, bezier-curve connections, drag-and-drop,
and field editors inside each node. This is fundamentally a compiled component
— the rendering logic cannot be expressed as a simple XML template.

**Caveat 3: Callback arity**
`OnNodeConnectedCallbackType` takes 4 extra arguments beyond `RefAny` + `CallbackInfo`.
`OnNodeFieldEditedCallbackType` takes 4 extra args including a `NodeTypeFieldValue` enum.
The type system's `ComponentCallbackSignature.extra_args` supports this —
no arity limitation.

**Verdict:** The **data model** is fully expressible (it's just structs, enums,
and callbacks). But the **template/rendering** is not — NodeGraph is inherently
a compiled component. Its data model would be defined in JSON for documentation
and debugger display, but the rendering must remain in Rust. This is exactly
the `Compiled` component source type described in section 5.5 of the design doc.

---

## 2. Summary Table

| # | Widget | Lines | Fields | Callbacks | Slots | Enums | Aux Structs | JSON? |
|---|--------|-------|--------|-----------|-------|-------|-------------|-------|
| 1 | Label | 133 | 1 | 0 | 0 | 0 | 0 | ✅ |
| 2 | Button | 1031 | 4 | 1 | 0 | 1 | 0 | ✅ |
| 3 | CheckBox | 310 | 1 | 1 | 0 | 0 | 1 | ✅ |
| 4 | ColorInput | 185 | 2 | 1 | 0 | 0 | 1 | ✅ |
| 5 | FileInput | 212 | 3 | 1 | 0 | 0 | 1 | ✅ |
| 6 | NumberInput | 310 | 3 | 2 | 0 | 0 | 1 | ✅ |
| 7 | ProgressBar | 617 | 2 | 0 | 0 | 0 | 0 | ✅ |
| 8 | Frame | 447 | 2 | 0 | 1 | 0 | 0 | ✅ |
| 9 | DropDown | 1057 | 2 | 1 | 0 | 0 | 0 | ✅ |
| 10 | TabHeader | 1463 | 2 | 1 | 0 | 0 | 1 | ✅ |
| 11 | TabContent | (in tabs) | 1 | 0 | 1 | 0 | 0 | ✅ |
| 12 | TextInput | 1072 | 3 | 3 | 0 | 2 | 3 | ⚠️ custom return type |
| 13 | ListView | 1684 | 3 | 3 | 0 | 0 | 4 | ⚠️ DomVec cells, Menu |
| 14 | TreeView | 1935 | 1 | 0 | 0 | 0 | 0 | ✅ |
| 15 | Titlebar | 618 | 5 | 0 | 0 | 0 | 0 | ✅ |
| 16 | Ribbon | 2789 | 1 | 1 | 0 | 0 | 0 | ⚠️ static not data-driven |
| 17 | NodeGraph | 3764 | 5 | 8 | 0 | 2 | 12+ | ⚠️ compiled rendering |

---

## 3. Gap Analysis: What the Type System Handles Well

### 3.1 Primitive fields ✅
Every widget's simple fields (`String`, `bool`, `f32`, `i32`, `usize`,
`ColorU`) map directly to `ComponentFieldType` variants.

### 3.2 Optional fields ✅
`OptionString`, `OptionImageRef`, `OptionUsize` → `ComponentFieldType::Option(...)`.

### 3.3 Vec/list fields ✅
`StringVec` (tabs, dropdown choices), `ListViewRowVec` →
`ComponentFieldType::Vec(...)`.

### 3.4 Enum fields ✅
`ButtonType` (8 variants), `NodeGraphStyle`, `TextInputValid`,
`TextInputSelection`, `NodeTypeFieldValue` →
`ComponentEnumModel` + `ComponentFieldType::EnumRef(...)`.

### 3.5 Child slots ✅
`Frame.content: Dom`, `TabContent.content: Dom` →
`ComponentFieldType::StyledDom`.

### 3.6 Callbacks ✅
All 22 distinct callback types across all widgets are expressible as
`ComponentFieldType::Callback(ComponentCallbackSignature)`.
Extra args (up to 4) are supported via `extra_args`.

### 3.7 Platform-specific styling ✅ (irrelevant to data model)
The massive CSS constant arrays (Windows/Linux/Mac button styles, etc.)
are a compiled concern. The JSON data model doesn't care about styling
implementation — it declares the *interface*, not the *implementation*.

---

## 4. Gap Analysis: What Needs Minor Extensions

### 4.1 Custom callback return types (TextInput) — EASY FIX

`OnTextInputReturn { update: Update, valid: TextInputValid }` is not the
standard `Update` return. Fix: allow `ComponentCallbackSignature.return_type`
to be a `StructRef("OnTextInputReturn")` that references an auxiliary struct,
not just the string `"Update"`.

**Current design:** `return_type: AzString` — already supports this, just set
it to `"OnTextInputReturn"` and define the struct in `data_models`.

### 4.2 `DomVec` as a field type (ListView rows) — ALREADY SUPPORTED

`ListViewRow.cells: DomVec` = `[StyledDom]` = `Vec(Box(StyledDom))`.
Already works with the current type system.

### 4.3 OS-level types: `Menu`, `LogicalPosition`, `LogicalSize` — STRUCT_REF

These are Azul core types that exist in the binary but aren't in the
component type system. Solution: reference them as `StructRef("Menu")`,
`StructRef("LogicalPosition")`, etc. The code generator knows these types
exist in `azul_core` and emits the correct import.

### 4.4 Callbacks that accept multiple extra arguments

`OnNodeConnectedCallbackType(input: NodeGraphNodeId, input_index: usize,
output: NodeGraphNodeId, output_index: usize)` — 4 extra args.
Already supported: `ComponentCallbackSignature.extra_args` is a Vec.

---

## 5. Conclusions

### 5.1 The type system works

All 17 widgets can have their **data models** expressed as JSON using the
proposed `ComponentFieldType` system. The proposed design handles every
pattern found in the real codebase:
- Primitives, Options, Vecs ✅
- Enum types (ButtonType, NodeTypeFieldValue) ✅
- StyledDom slots (Frame, TabContent) ✅
- Callbacks with rich signatures (0-4 extra args) ✅
- Nested struct references (Node → NodeTypeField → NodeTypeFieldValue) ✅
- Optional callbacks (every widget has optional callbacks) ✅

### 5.2 Data model ≠ implementation

The key insight from this analysis: **the data model is always simple, the
complexity lives in the rendering and event handling code.** Even the 3700-line
NodeGraph has a data model that's just ~20 fields across ~15 structs. The
thousands of lines are CSS constants and DOM-building logic.

This validates the design's split between:
- **Data model** (JSON-definable): what the component expects as input
- **Render function** (compiled): how it turns those inputs into DOM

### 5.3 No "impossible" widgets

There is no widget that fundamentally cannot be described. The most complex
case (NodeGraph) has a fully expressible data model — it just can't have its
*rendering* expressed as a simple XML template. But that's by design: it's
a `Compiled` component. Its data model is still useful for:
- Debugger UI (inspect/edit NodeGraph state)
- Code generation (emit `NodeGraphData` struct)
- Documentation (advertise the component's interface)

### 5.4 Recommended minor additions to the design

1. **`StructRef` for core Azul types**: Pre-register `Menu`, `LogicalPosition`,
   `LogicalSize`, `PixelValue`, `ImageRef`, `FontRef` as known `StructRef`
   names that don't need to be defined in the library's `data_models` — they're
   part of the core framework. (Note: `ImageRef` and `FontRef` already have
   dedicated `ComponentFieldType` variants.)

2. **Custom callback return types**: The design already supports this via
   `return_type: AzString` referencing an auxiliary struct. Just document
   that `"Update"` is the default but not the only option.

3. **Template expression support**: For computed CSS values like
   `width: {{ percent_done }}%`, the template engine needs basic expressions.
   This is a template engine concern, not a type system concern, but worth
   noting for implementation planning.
