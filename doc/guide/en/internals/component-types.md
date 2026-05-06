---
slug: component-types
title: Component Type System
language: en
canonical_slug: component-types
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: How typed component IDs resolve at the layout layer
prerequisites: []
tracked_files:
  - core/src/xml.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

> **WIP** — `ComponentFieldType` and the related runtime types are in place, but the debug-server JSON serializer and the multi-language code generator still flatten field types to strings. The render-function signature also still passes `&ComponentDataModel` (with default values doubling as current values) instead of the eventual `ComponentFieldNamedValueVec`.

A component is the unit of azul's GUI builder: a typed bundle of (data model, render function, compile function, scoped CSS, source kind). The type system that describes a component's inputs lives in [`core/src/xml.rs`](https://github.com/maps4print/azul/blob/master/core/src/xml.rs), a single `ComponentFieldType` enum that the debugger renders, the code generator emits, and the parser/serializer round-trips. All of it is `#[repr(C)]` so libraries can be authored, exported, and re-imported across the FFI boundary.

## File map

- **`ComponentId`.** A `collection:name` qualified key.
  - `core/src/xml.rs::ComponentId`
- **`ComponentFieldType`.** Per-field type descriptor.
  - `core/src/xml.rs::ComponentFieldType`
- **`ComponentCallbackSignature`.** Return type plus args.
  - `core/src/xml.rs::ComponentCallbackSignature`
- **`ComponentEnumModel` / `ComponentEnumVariant`.** Enum model type.
  - `core/src/xml.rs::ComponentEnumModel`
- **`ComponentDefaultValue`.** Typed defaults.
  - `core/src/xml.rs::ComponentDefaultValue`
- **`ComponentInstanceDefault`.** Slot-default sub-component.
  - `core/src/xml.rs::ComponentInstanceDefault`
- **`ComponentFieldValueSource`.** Default, Literal, or Binding.
  - `core/src/xml.rs::ComponentFieldValueSource`
- **`ComponentFieldValue`.** Runtime value enum.
  - `core/src/xml.rs::ComponentFieldValue`
- **`ComponentDataField`.** One field of a model.
  - `core/src/xml.rs::ComponentDataField`
- **`ComponentDataModel`.** Named struct of fields.
  - `core/src/xml.rs::ComponentDataModel`
- **`ComponentSource`.** Builtin, Compiled, or UserDefined.
  - `core/src/xml.rs::ComponentSource`
- **`ComponentRenderFn` / `ComponentCompileFn`.** Function pointer types.
  - `core/src/xml.rs::ComponentRenderFn`
- **`ComponentDef`.** The component itself.
  - `core/src/xml.rs::ComponentDef`
- **`ComponentLibrary`.** A library of components.
  - `core/src/xml.rs::ComponentLibrary`
- **`ComponentMap`.** Registry of libraries.
  - `core/src/xml.rs::ComponentMap`
- **`tag_to_node_type`.** Maps a builtin tag to `NodeType`.
  - `core/src/xml.rs::tag_to_node_type`
- **`builtin_render_fn`.** Render function for builtin HTML elements.
  - `core/src/xml.rs::builtin_render_fn`
- **`user_defined_render_fn`.** Render function for user-defined components.
  - `core/src/xml.rs::user_defined_render_fn`

## ComponentDef

```rust,ignore
#[repr(C)]
pub struct ComponentDef {
    pub id: ComponentId,
    pub display_name: AzString,
    pub description: AzString,
    pub css: AzString,
    pub source: ComponentSource,
    pub data_model: ComponentDataModel,
    pub render_fn: ComponentRenderFn,
    pub compile_fn: ComponentCompileFn,
    pub render_fn_source: OptionString,
    pub compile_fn_source: OptionString,
}
```

(`core/src/xml.rs::ComponentDef`)

`id = ComponentId { collection, name }` qualifies the component as `"collection:name"`: `"builtin:div"`, `"shadcn:avatar"`, `"myproject:user_card"`. `data_model` is the typed input struct; `render_fn` and `compile_fn` are the only behavioural attachments. `render_fn_source` / `compile_fn_source` carry the originating source code for `UserDefined` components so the debugger can show and re-export it.

The earlier proposal had separate `parameters`, `callback_slots`, `accepts_text`, `child_policy`, and `template` fields; all were folded into `data_model`. A field of type `StyledDom` is a child slot, a field of type `Callback(...)` is a callback slot, and child acceptance is derived from the model's shape. The `template` was dropped in favour of source-edit-recompile.

## ComponentFieldType

```rust,ignore
#[repr(C, u8)]
pub enum ComponentFieldType {
    String, Bool,
    I32, I64, U32, U64, Usize,
    F32, F64,
    ColorU,
    CssProperty,
    ImageRef, FontRef,
    /// StyledDom slot — field name = slot name
    StyledDom,
    /// Callback with typed signature
    Callback(ComponentCallbackSignature),
    /// RefAny data binding with type hint
    RefAny(AzString),
    /// Optional value (recursive via Box)
    OptionType(ComponentFieldTypeBox),
    /// Vec of values (recursive via Box)
    VecType(ComponentFieldTypeBox),
    /// Reference to a struct defined in the same library
    StructRef(AzString),
    /// Reference to an enum defined in the same library
    EnumRef(AzString),
}
```rust

(`core/src/xml.rs::ComponentFieldType`)

The variants split into five groups:

- **Primitives.** `String`, `Bool`, the eight numeric widths, and `ColorU`. They map straight to a Rust/C/Python primitive in code generation.
- **Azul-specific.** `CssProperty`, `ImageRef`, and `FontRef`. Pre-declared azul types the editor can offer pickers for.
- **Slot.** `StyledDom`. The field name *is* the slot name; there is no separate string. The debugger renders this as a drop-zone, and `render_fn` reads the embedded child subtree.
- **Callback.** `Callback(ComponentCallbackSignature)` carries `return_type: AzString` and `args: ComponentCallbackArgVec` (`core/src/xml.rs::ComponentCallbackSignature`). The `&mut RefAny` and `&mut CallbackInfo` arguments are implicit and not stored in `args`.
- **References.** `RefAny(type_hint)`, `StructRef(name)`, and `EnumRef(name)` resolve names against `ComponentLibrary::data_models` and `ComponentLibrary::enum_models`.
- **Containers.** `OptionType(Box)` and `VecType(Box)` recurse via `ComponentFieldTypeBox`. The names are `OptionType`/`VecType` in code (not bare `Option`/`Vec`) to avoid clashing with `core::option::Option` and `alloc::vec::Vec`.

### ComponentFieldTypeBox — recursive types across FFI

```rust,ignore
#[repr(C)]
pub struct ComponentFieldTypeBox {
    pub ptr: *mut ComponentFieldType,
}
```

(`core/src/xml.rs::ComponentFieldTypeBox`)

A `Box<ComponentFieldType>` doesn't survive the C ABI, so the recursion is broken with a raw pointer plus a hand-rolled `Drop`/`Clone`/`Hash`. `ComponentFieldType::parse` and the serde implementation (`core/src/xml.rs::parse` and `core/src/xml.rs::serde`) are the only producers; both go through `ComponentFieldTypeBox::new`, which `Box::into_raw`s the inner value.

`ComponentFieldValueBox` (`core/src/xml.rs::ComponentFieldValueBox`) uses the same pattern for the runtime-value enum.

## ComponentDataModel and ComponentDataField

```rust,ignore
#[repr(C)]
pub struct ComponentDataModel {
    pub name: AzString,
    pub description: AzString,
    pub fields: ComponentDataFieldVec,
}

#[repr(C)]
pub struct ComponentDataField {
    pub name: AzString,
    pub field_type: ComponentFieldType,
    pub default_value: OptionComponentDefaultValue,
    pub required: bool,
    pub description: AzString,
}
```rust

(`core/src/xml.rs::ComponentDataModel`, `core/src/xml.rs::ComponentDataField`)

The model's `name` is what the code generator emits as the input struct name (e.g. `"AvatarData"`). Two convenience helpers exist on `ComponentDataModel`: `get_field(name) -> Option<&ComponentDataField>` and `with_default(name, value) -> Self` for builder-style override.

A library can also expose **shared** struct types in `ComponentLibrary::data_models`. Components reference them via `StructRef(name)`. Each component still has its own *main* `data_model` on `ComponentDef`; the library list is for types reused across multiple components (e.g. `UserProfile`).

## ComponentDefaultValue

```rust,ignore
#[repr(C, u8)]
pub enum ComponentDefaultValue {
    None,
    String(AzString),
    Bool(bool),
    I32(i32), I64(i64), U32(u32), U64(u64), Usize(usize),
    F32(f32), F64(f64),
    ColorU(ColorU),
    /// Default is an instance of another component
    ComponentInstance(ComponentInstanceDefault),
    /// Default callback function pointer name
    CallbackFnPointer(AzString),
    /// JSON string representing a complex default value
    Json(AzString),
}
```rust

(`core/src/xml.rs::ComponentDefaultValue`)

The variants line up with `ComponentFieldType` for primitives. `ComponentInstance` carries a `library`, `component`, and a list of `ComponentFieldOverride { field_name, source }` so a `StyledDom` slot can default to a sub-component already configured. `CallbackFnPointer` is a fully-qualified function name (`"my_app::handlers::on_click"`); compiled components resolve it via `dladdr`, dynamic components emit it as a code-gen `use` import. `Json` is the escape hatch for complex defaults that don't fit any other variant. The serializer at `core/src/xml.rs::serde_json_default` re-parses and re-emits the JSON in place.

## ComponentFieldValueSource — Literal vs Binding

```rust,ignore
#[repr(C, u8)]
pub enum ComponentFieldValueSource {
    Default,
    Literal(AzString),
    Binding(AzString),
}
```rust

(`core/src/xml.rs::ComponentFieldValueSource`)

This is the per-instance counterpart of `ComponentDefaultValue`. The debugger has two views:

- **Component preview.** Fields use `Default` or `Literal`. Hardcoded values, instantiated for visual testing.
- **Application composition.** Fields can also use `Binding("app_state.user.name")`, a dotted path resolved against the application's `RefAny` state. Auto-complete uses the type-hint on the bound `RefAny` plus any `StructRef`-defined fields.

`ComponentFieldOverride { field_name, source }` carries one of these for each overridden field of a default component instance.

## ComponentFieldValue — the runtime value enum

```rust,ignore
#[repr(C, u8)]
pub enum ComponentFieldValue {
    String(AzString), Bool(bool),
    I32(i32), /* …same numeric set as ComponentDefaultValue … */
    ColorU(ColorU),
    None,
    Some(ComponentFieldValueBox),
    Vec(ComponentFieldValueVec),
    StyledDom(StyledDom),
    Struct(ComponentFieldNamedValueVec),
    Enum { variant: AzString, fields: ComponentFieldNamedValueVec },
    Callback(AzString),
    RefAny(RefAny),
}
```rust

(`core/src/xml.rs::ComponentFieldValue`)

Where `ComponentFieldType` is the *class*, `ComponentFieldValue` is the *instance*. It carries actual `StyledDom` subtrees, `RefAny`s, and resolved literals. The intended render-function signature passes a `ComponentFieldNamedValueVec` of these; the current signature still passes a `&ComponentDataModel` with `default_value` doubling as "current value". See the divergence note at the end of the page.

## ComponentSource

```rust,ignore
#[repr(C)]
pub enum ComponentSource {
    Builtin,
    Compiled,
    UserDefined,
}
```

(`core/src/xml.rs::ComponentSource`)

Drives editability in the debugger: `Builtin` (the HTML elements baked into the DLL) and `Compiled` (Rust widgets like `Button`, `TextInput`) are read-only. Their data model is generated from compiled code, and the type cannot be edited. `UserDefined` components are JSON/XML-imported or built in the debugger and are fully editable. The `exportable` flag on `ComponentLibrary` follows: `builtin` and compiled libraries are not exportable; user-defined libraries are.

## ComponentRenderFn and ComponentCompileFn

```rust,ignore
pub type ComponentRenderFn = fn(
    &ComponentDef,
    &ComponentDataModel,
    &ComponentMap,
) -> ResultStyledDomRenderDomError;

pub type ComponentCompileFn = fn(
    &ComponentDef,
    &CompileTarget,
    &ComponentDataModel,
    indent: usize,
) -> ResultStringCompileError;
```rust

(`core/src/xml.rs::ComponentRenderFn`, `core/src/xml.rs::ComponentCompileFn`)

`render_fn` is the live-rendering path; `compile_fn` emits source code for one of `CompileTarget::{Rust, C, Cpp, Python}`. Both are bare `fn` pointers (not closures), which keeps `ComponentDef` `#[repr(C)]` and FFI-safe. The `&ComponentMap` argument lets the function recursively look up sub-components.

For `Builtin` HTML elements, `builtin_render_fn` (`core/src/xml.rs::builtin_render_fn`) calls `tag_to_node_type(def.id.name)` to map `"div"`, `"a"`, `"button"`, and so on to `NodeType`, builds a `Dom`, and styles with the per-component `def.css`. `builtin_compile_fn` emits `Dom::create_node(NodeType::Div)` (Rust), `AzDom_createDiv()` (C), `Dom::create_div()` (C++), or `Dom.div()` (Python).

For user-defined components, `user_defined_render_fn` (`core/src/xml.rs::user_defined_render_fn`) is a generic walker: it iterates `data.fields`, dispatches on each `default_value` variant, and for `ComponentInstance` looks up the sub-component in the `ComponentMap` and recurses. The `def.css` string is parsed via `Css::from_string(...)` and applied to the wrapper.

## ComponentLibrary and ComponentMap

```rust,ignore
#[repr(C)]
pub struct ComponentLibrary {
    pub name: AzString,
    pub version: AzString,
    pub description: AzString,
    pub components: ComponentDefVec,
    pub exportable: bool,
    pub modifiable: bool,
    pub data_models: ComponentDataModelVec,
    pub enum_models: ComponentEnumModelVec,
}

#[repr(C)]
pub struct ComponentMap {
    pub libraries: ComponentLibraryVec,
}
```

(`core/src/xml.rs::ComponentLibrary`, `core/src/xml.rs::ComponentMap`)

Lookup helpers:

- The `ComponentMap::get(collection, name)` method is qualified; it searches the named library only.
- The `ComponentMap::get_unqualified(name)` method always searches `"builtin"`.
- The `ComponentMap::get_by_qualified_name("a:b")` method splits at `:` and falls back to unqualified.
- The `ComponentMap::get_exportable_libraries()` method filters for `exportable == true`.
- The `ComponentMap::all_components()` method returns a flat-mapped iterator over every library.

The `"builtin"` library is built once (`core/src/xml.rs::build_builtin_library`) and registers every HTML element via `builtin_component_def(tag, display_name, default_text, css)` (`core/src/xml.rs::builtin_component_def`), plus three control-flow components (`if`, `for`, `map`).

## Type-string parser and serde format

`ComponentFieldType::parse(s)` (`core/src/xml.rs::parse`) accepts a small grammar: `String`, `Option<Bool>`, `Vec<I32>`, `Callback(fn(...) -> Update)`, `RefAny(MyAppData)`, `EnumRef(ButtonVariant)`, `StructRef(UserProfile)`. It's the entry point for XML `args="..."` attributes and the debugger "add field" dialog. The inverse `field_type_to_string()` (`core/src/xml.rs::field_type_to_string`, serde-only) flattens back to the same syntax for export.

When the `serde-json` feature is enabled, `ComponentDataModel`, `ComponentDataField`, `ComponentFieldType`, and `ComponentDefaultValue` round-trip through JSON via the implementations in `core/src/xml.rs`. Field types currently serialize as strings (`"Option<String>"`); the planned structured-JSON format from the design doc is not implemented.

## Adding a new built-in HTML element

1. Add a variant to `NodeType` (`core/src/dom.rs::NodeType`).
2. Add the tag arm to `tag_to_node_type` (`core/src/xml.rs::tag_to_node_type`) and the corresponding `tag_to_nodetypetag` in the same file.
3. Add a `builtin_component_def("tag", "Display Name", default_text, css)` line to the builtin-library construction at `core/src/xml.rs::build_builtin_library`.
4. If the tag has a UA-CSS default, add an arm in `apply_ua_css_to_compact`. See [Cascade, Inheritance, Restyle](cascade.md).
5. Update the XML parser's tag table (`layout/src/xml/mod.rs`) so XML / XHTML round-trips.

## Adding a new field-type variant

1. Add a variant to `ComponentFieldType` (`core/src/xml.rs::ComponentFieldType`). Keep `#[repr(C, u8)]` discipline. No `Box<T>`, use `ComponentFieldTypeBox` for recursion.
2. Add the parse arm in `ComponentFieldType::parse` (`core/src/xml.rs::parse`) and the inverse in `field_type_to_string` (`core/src/xml.rs::field_type_to_string`).
3. Add a matching `ComponentDefaultValue` variant if the new type has a literal default form, and a `ComponentFieldValue` runtime variant.
4. Update `user_defined_render_fn` (`core/src/xml.rs::user_defined_render_fn`) and `user_defined_compile_fn` (`core/src/xml.rs::user_defined_compile_fn`) so the generic preview/compile paths know how to project it.
5. Regenerate `api.json` so the new variant crosses the FFI boundary.

## Divergences from the design doc

The design intent in [`scripts/COMPONENT_TYPE_SYSTEM_DESIGN.md`](https://github.com/maps4print/azul/blob/master/scripts/COMPONENT_TYPE_SYSTEM_DESIGN.md) is mostly implemented. Three deltas to be aware of:

1. **Render-function signature.** The doc proposes `&ComponentFieldNamedValueVec` (actual runtime values). The implementation still passes `&ComponentDataModel` and treats `default_value` on each field as the current value. The runtime-value types exist (`ComponentFieldValue`, `ComponentFieldNamedValue`) but are not wired through `render_fn` yet.
2. **Variant names.** The doc says `Option(Box<...>)` and `Vec(Box<...>)`; the code uses `OptionType` and `VecType` to avoid colliding with `core::option::Option` / `alloc::vec::Vec` in scope.
3. **Debug-server JSON.** The doc specifies structured JSON for field types; the current `field_type_to_string()` flattens to `"Option<String>"`-style strings, and the debugger re-parses them client-side. The serde implementation in `core/src/xml.rs::serde` matches the legacy string form.

Code that constructs `ComponentDef` values must build the data model with the *current* value pre-baked into `default_value`. The `ComponentDataModel::with_default(name, value)` builder at `core/src/xml.rs::with_default` is the supported way to do it.

## See also

- [DOM Internals](dom.md) — the `NodeType` enum that builtin component tags map onto.
- [CSS Parser](css-parser.md) — `def.css` is parsed via `Css::from_string` before each render.
- [Cascade, Inheritance, Restyle](cascade.md) — per-component CSS scoping interacts with the deferred-cascade design.

## Coming Up Next

- [DOM Internals](dom.md) — How the public `Dom` type is built and stored
- [Cascade, Inheritance, Restyle](cascade.md) — Selector matching, specificity, and computed values
- [Code Organization](code-organization.md) — Top-level crate map and where each piece lives
