---
slug: internals/dom/component-types
title: Component Type System
language: en
canonical_slug: internals/dom/component-types
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

# Component Type System

## Overview

*WIP.* The component type system is the unit of azul's GUI builder: a typed bundle of (data model, render function, compile function, scoped CSS, source kind), described by `ComponentDef` and friends in `core/src/xml.rs`. Every type is `#[repr(C)]`, so libraries can be authored, exported, and re-imported across the FFI boundary. The runtime types are stable and used today by the live preview, the compile-to-source pipeline, and the multi-language code generator. The debug-server JSON serialiser and the render-function call signature still flatten field types to strings and pass `&ComponentDataModel` (with default values doubling as current values) instead of the eventual `ComponentFieldNamedValueVec`. Both are mechanical changes pending consolidation.

The type system does three jobs at once. It describes the *class* of input a component accepts (what `ComponentFieldType` does), the *literal default* for each field (`ComponentDefaultValue`), and the *runtime instance* the component is rendered with (`ComponentFieldValue`). Together they let the same `ComponentDef` drive the in-app preview, the JSON / XML round-trip, the codegen emitter that produces standalone Rust / C / C++ / Python source, and the debug server's introspection UI.

This page describes the shape of those types and how they interact, then lists the procedures for adding a new built-in HTML element or a new field-type variant.

## ComponentDef

`ComponentDef` is the component itself:

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

`id = ComponentId { collection, name }` qualifies the component as `"collection:name"`: `"builtin:div"`, `"shadcn:avatar"`, `"myproject:user_card"`. `data_model` is the typed input struct; `render_fn` and `compile_fn` are the only behavioural attachments. `render_fn_source` / `compile_fn_source` carry the originating source code for `UserDefined` components so the debugger can show and re-export them.

The earlier proposal had separate `parameters`, `callback_slots`, `accepts_text`, `child_policy`, and `template` fields; all were folded into `data_model`. A field of type `StyledDom` is a child slot, a field of type `Callback(...)` is a callback slot, and child acceptance is derived from the model's shape. The `template` was dropped in favour of source-edit-recompile.

## ComponentFieldType

`ComponentFieldType` is the per-field type descriptor:

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
```

The variants split into five groups:

- **Primitives.** `String`, `Bool`, the eight numeric widths, and `ColorU`. They map straight to a Rust / C / Python primitive in code generation.
- **Azul-specific.** `CssProperty`, `ImageRef`, and `FontRef`. Pre-declared azul types the editor can offer pickers for.
- **Slot.** `StyledDom`. The field name *is* the slot name; there is no separate string. The debugger renders this as a drop-zone, and `render_fn` reads the embedded child subtree.
- **Callback.** `Callback(ComponentCallbackSignature)` carries a `return_type: AzString` and `args: ComponentCallbackArgVec`. The `&mut RefAny` and `&mut CallbackInfo` arguments are implicit and not stored in `args`.
- **References.** `RefAny(type_hint)`, `StructRef(name)`, and `EnumRef(name)` resolve names against `ComponentLibrary::data_models` and `ComponentLibrary::enum_models`.
- **Containers.** `OptionType(Box)` and `VecType(Box)` recurse via `ComponentFieldTypeBox`. The names are `OptionType` / `VecType` in code (not bare `Option` / `Vec`) to avoid clashing with `core::option::Option` and `alloc::vec::Vec`.

A `Box<ComponentFieldType>` doesn't survive the C ABI, so the recursion is broken with a raw pointer plus a hand-rolled `Drop` / `Clone` / `Hash`:

```rust,ignore
#[repr(C)]
pub struct ComponentFieldTypeBox {
    pub ptr: *mut ComponentFieldType,
}
```

`ComponentFieldType::parse` and the serde implementation are the only producers; both go through `ComponentFieldTypeBox::new`, which `Box::into_raw`s the inner value. `ComponentFieldValueBox` uses the same pattern for the runtime-value enum.

## Data model and fields

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
```

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
```

The variants line up with `ComponentFieldType` for primitives. `ComponentInstance` carries a `library`, `component`, and a list of `ComponentFieldOverride { field_name, source }` so a `StyledDom` slot can default to a sub-component already configured. `CallbackFnPointer` is a fully-qualified function name (`"my_app::handlers::on_click"`); compiled components resolve it via `dladdr`, dynamic components emit it as a code-gen `use` import. `Json` is the escape hatch for complex defaults that don't fit any other variant. The serializer re-parses and re-emits the JSON in place via `serde_json_default`.

## ComponentFieldValueSource: Literal vs Binding

```rust,ignore
#[repr(C, u8)]
pub enum ComponentFieldValueSource {
    Default,
    Literal(AzString),
    Binding(AzString),
}
```

This is the per-instance counterpart of `ComponentDefaultValue`. The debugger has two views:

- **Component preview.** Fields use `Default` or `Literal`. Hardcoded values, instantiated for visual testing.
- **Application composition.** Fields can also use `Binding("app_state.user.name")`, a dotted path resolved against the application's `RefAny` state. Auto-complete uses the type-hint on the bound `RefAny` plus any `StructRef`-defined fields.

`ComponentFieldOverride { field_name, source }` carries one of these for each overridden field of a default component instance.

## ComponentFieldValue: the runtime value enum

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
```

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

Drives editability in the debugger. `Builtin` (the HTML elements baked into the DLL) and `Compiled` (Rust widgets like `Button`, `TextInput`) are read-only. Their data model is generated from compiled code, and the type cannot be edited. `UserDefined` components are JSON / XML-imported or built in the debugger and are fully editable. The `exportable` flag on `ComponentLibrary` follows: `builtin` and compiled libraries are not exportable; user-defined libraries are.

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
```

`render_fn` is the live-rendering path; `compile_fn` emits source code for one of `CompileTarget::{Rust, C, Cpp, Python}`. Both are bare `fn` pointers (not closures), which keeps `ComponentDef` `#[repr(C)]` and FFI-safe. The `&ComponentMap` argument lets the function recursively look up sub-components.

For `Builtin` HTML elements, `builtin_render_fn` calls `tag_to_node_type(def.id.name)` to map `"div"`, `"a"`, `"button"`, and so on to `NodeType`, builds a `Dom`, and styles it with the per-component `def.css`. `builtin_compile_fn` emits `Dom::create_node(NodeType::Div)` (Rust), `AzDom_createDiv()` (C), `Dom::create_div()` (C++), or `Dom.div()` (Python).

For user-defined components, `user_defined_render_fn` is a generic walker: it iterates `data.fields`, dispatches on each `default_value` variant, and for `ComponentInstance` looks up the sub-component in the `ComponentMap` and recurses. The `def.css` string is parsed via `Css::from_string(...)` and applied to the wrapper.

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

The map provides several lookup strategies. `ComponentMap::get(collection, name)` is fully qualified and searches the named library only. `get_unqualified(name)` always searches `"builtin"`. `get_by_qualified_name("a:b")` splits at `:` and falls back to unqualified. `get_exportable_libraries()` filters for `exportable == true`. `all_components()` returns a flat-mapped iterator over every library.

The `"builtin"` library is built once by `build_builtin_library` and registers every HTML element via `builtin_component_def(tag, display_name, default_text, css)`, plus three control-flow components (`if`, `for`, `map`).

## Type-string parser and serde format

`ComponentFieldType::parse(s)` accepts a small grammar: `String`, `Option<Bool>`, `Vec<I32>`, `Callback(fn(...) -> Update)`, `RefAny(MyAppData)`, `EnumRef(ButtonVariant)`, `StructRef(UserProfile)`. It's the entry point for XML `args="..."` attributes and the debugger "add field" dialog. The inverse `field_type_to_string()` (serde-only) flattens back to the same syntax for export.

When the `serde-json` feature is enabled, `ComponentDataModel`, `ComponentDataField`, `ComponentFieldType`, and `ComponentDefaultValue` round-trip through JSON. Field types currently serialise as strings (`"Option<String>"`); the planned structured-JSON format from the design doc is not implemented.

## Adding a new built-in HTML element

1. Add a variant to `NodeType`.
2. Add the tag arm to `tag_to_node_type` and the corresponding `tag_to_nodetypetag`.
3. Add a `builtin_component_def("tag", "Display Name", default_text, css)` line to `build_builtin_library`.
4. If the tag has a UA-CSS default, add an arm in `apply_ua_css_to_compact`. See [Cascade, Inheritance, Restyle](../styling/cascade.md).
5. Update the XML parser's tag table in `layout/src/xml/mod.rs` so XML / XHTML round-trips.

## Adding a new field-type variant

1. Add a variant to `ComponentFieldType`. Keep `#[repr(C, u8)]` discipline. No `Box<T>` — use `ComponentFieldTypeBox` for recursion.
2. Add the parse arm in `ComponentFieldType::parse` and the inverse in `field_type_to_string`.
3. Add a matching `ComponentDefaultValue` variant if the new type has a literal default form, and a `ComponentFieldValue` runtime variant.
4. Update `user_defined_render_fn` and `user_defined_compile_fn` so the generic preview / compile paths know how to project it.
5. Regenerate `api.json` so the new variant crosses the FFI boundary.

## Divergences from the design doc

The design intent in `scripts/COMPONENT_TYPE_SYSTEM_DESIGN.md` is mostly implemented. Three deltas to be aware of:

1. **Render-function signature.** The doc proposes `&ComponentFieldNamedValueVec` (actual runtime values). The implementation still passes `&ComponentDataModel` and treats `default_value` on each field as the current value. The runtime-value types exist (`ComponentFieldValue`, `ComponentFieldNamedValue`) but are not wired through `render_fn` yet.
2. **Variant names.** The doc says `Option(Box<...>)` and `Vec(Box<...>)`; the code uses `OptionType` and `VecType` to avoid colliding with `core::option::Option` / `alloc::vec::Vec` in scope.
3. **Debug-server JSON.** The doc specifies structured JSON for field types; the current `field_type_to_string()` flattens to `"Option<String>"`-style strings, and the debugger re-parses them client-side. The serde implementation matches the legacy string form.

Code that constructs `ComponentDef` values must build the data model with the *current* value pre-baked into `default_value`. The `ComponentDataModel::with_default(name, value)` builder is the supported way to do it.

## See also

- [DOM Internals](../dom.md) — the `NodeType` enum that builtin component tags map onto.
- [CSS Parser](../styling/css-parser.md) — `def.css` is parsed via `Css::from_string` before each render.
- [Cascade, Inheritance, Restyle](../styling/cascade.md) — per-component CSS scoping interacts with the deferred-cascade design.

## Coming Up Next

- [DOM Internals](../dom.md) — How the public `Dom` type is built and stored
- [Cascade, Inheritance, Restyle](../styling/cascade.md) — Selector matching, specificity, and computed values
- [Code Organization](../code-organization.md) — Top-level crate map and where each piece lives
