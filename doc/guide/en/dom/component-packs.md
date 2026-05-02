---
slug: dom/component-packs
title: Component Packs
language: en
canonical_slug: dom/component-packs
audience: external
maturity: mature
guide_order: 32
topic_only: false
short_desc: Registering libraries of named components — how the framework discovers them, previews them in the debugger, and round-trips them through code generation.
prerequisites: [dom, dom/components]
tracked_files:
  - core/src/xml.rs
  - core/src/resources.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:53:30Z
---

# Component Packs

A **component pack** (a `ComponentLibrary` in the codebase) is a named
collection of `ComponentDef`s. Packs are how the framework finds a component
by name — `<card title="…"/>` in XML, `shadcn:avatar` in the live debugger,
`builtin:div` for the 52 stock HTML elements — and how it round-trips a
component invocation through the design-time tools to source code.

This page covers the registration, preview, and instantiation surface. The
hand-written-Rust component model is in [Components](components.md); this
page is one level down: how those components register so the framework can
*find* them by name.

## Why packs and not just functions

A plain Rust component (`fn card(title: &str, body: &str) -> Dom`) is
visible to whoever has the symbol. That works for compiled-in components —
your own widgets, a third-party crate's widgets — but it doesn't help when:

- You want to **preview** a component in the live debugger without
  recompiling.
- You want a **typed data model** the debugger can edit, populate, and pass
  back to the render function.
- You want **code generation** — the design-time tool emits the equivalent
  Rust / C / Python source for a tree you laid out interactively.
- You want to **load components from XML** by name — `<card …/>` has to
  resolve to *some* function.

A `ComponentDef` (`core/src/xml.rs:2094`) carries everything the runtime
plus the design-time tools need:

```rust,ignore
pub struct ComponentDef {
    pub id: ComponentId,                 // collection + name, e.g. shadcn:card
    pub display_name: AzString,
    pub description: AzString,
    pub css: AzString,                   // ships with the component
    pub source: ComponentSource,         // where it came from (built-in, JSON, …)
    pub data_model: ComponentDataModel,  // typed prop list — defaults double as preview values
    pub render_fn: ComponentRenderFn,    // (&def, &data, &registry) -> StyledDom
    pub compile_fn: ComponentCompileFn,  // (&def, &target_lang, &data, indent) -> source
    pub render_fn_source: OptionString,
    pub compile_fn_source: OptionString,
}
```

A `ComponentLibrary` (`core/src/xml.rs:2141`) groups defs under a name,
version, description, and an `exportable` / `modifiable` pair of flags that
the live editor uses to decide whether the user is allowed to edit
in-place.

## Registering a pack

`AppConfig::create()` always pre-registers the `"builtin"` library — the 52
HTML element components — so you start with `builtin:div`, `builtin:p`,
`builtin:button`, … available out of the box. Anything else you register on
top.

Two registration shapes:

```rust,ignore
use azul::prelude::*;

let mut config = AppConfig::create();

// 1. Register a single component into a named library.
config.add_component(
    AzString::from_const_str("mylib"),
    my_register_card_fn,        // extern "C" fn() -> ComponentDef
);

// 2. Register an entire pre-built library.
config.add_component_library(
    AzString::from_const_str("shadcn"),
    register_shadcn,            // extern "C" fn() -> ComponentLibrary
);
```

Both work the same way: the registration function runs immediately at the
call site, the returned `ComponentDef` / `ComponentLibrary` is moved into
`config.component_libraries`, and the library becomes visible to the XML
parser, the layout callback (via `CallbackInfo`), and the debug server. The
function-pointer indirection (rather than a direct `ComponentDef`
parameter) is what lets the C and Python bindings register libraries
through their respective callback shapes — see the C example below.

### Built-in libraries are dogfood

The 52 built-in HTML elements are themselves registered through
`add_component_library`:

```rust,ignore
// core/src/resources.rs:484, inside AppConfig::create()
s.add_component_library(
    AzString::from_const_str("builtin"),
    crate::xml::register_builtin_components as extern "C" fn() -> ComponentLibrary,
);
```

`register_builtin_components` (`core/src/xml.rs:3510`) returns a fully
populated `ComponentLibrary` whose render functions are the same
`Dom::create_<tag>()` constructors documented in [DOM](../dom.md). Your own
packs use the same shape.

### From C

The `RegisterComponentLibraryFn` callback type is `repr(C)`, so a plain
function pointer suffices on the C side:

```c
extern AzComponentLibrary register_shadcn(void);

void main(void) {
    AzAppConfig config = AzAppConfig_create();
    AzAppConfig_addComponentLibrary(
        &config,
        AzString_fromConstStr("shadcn"),
        register_shadcn          // converts to AzRegisterComponentLibraryFn implicitly
    );
    /* ... */
}
```

### From Python

The Python binding wraps the function pointer in a trampoline. Pass a
Python callable; the binding stores it in the callback's `ctx` slot
(`OptionRefAny::Some(refany)`) and dispatches through a
generated trampoline:

```python
from azul import *

def register_shadcn():
    # Build a ComponentLibrary using the typed builders the binding exposes.
    return ComponentLibrary.create("shadcn", "1.0.0", "shadcn-style components", [
        # ... ComponentDefs ...
    ], exportable=True, modifiable=False)

config = AppConfig.create()
config.add_component_library("shadcn", register_shadcn)
```

## Render: live preview

`ComponentDef::render_fn` has signature

```rust,ignore
fn(&ComponentDef, &ComponentDataModel, &ComponentMap) -> ResultStyledDomRenderDomError
```

The render function takes a *modified* `data_model` (the design-time tool
overrides the default values on each field, then hands it back) plus the
full `ComponentMap` so the function can recursively instantiate
sub-components — `<card>` containing a `<h2>` containing a `<span>` is one
render call that recurses three levels through the same registry. The
return is a fully cascaded `StyledDom` ready for layout.

This is what powers the live preview: the design-time tool reads the
component's data model, lets the user edit each prop, calls the render
function, and shows the result inline. No recompile, no trip through the
disk.

## Instantiation: from XML to DOM

When the XML parser encounters `<card title="First" body="alpha"/>`, it
resolves `card` against the `ComponentMap`:

1. Strip the namespace if present: `<shadcn:card …/>` →
   `("shadcn", "card")`. Bare `<card …/>` falls back to `"builtin"` and is
   resolved like a built-in tag.
2. Look up the corresponding `ComponentDef` via `ComponentMap::get`.
3. Take the def's `data_model`, populate each field's `default_value` from
   the XML attributes (typed coercion based on `ComponentDataModelField`).
4. Call `render_fn(&def, &populated_data_model, &component_map)`.
5. Stamp every root node of the returned `StyledDom` with a
   `ComponentOrigin` — `component_id: "shadcn:card"`, `data_model_json` —
   so the debugger can reconstruct the invocation.

The `ComponentMap` is what `str_to_dom_unstyled` and `str_to_dom` take as
their second argument; the `AppConfig`'s `component_libraries` are folded
into a `ComponentMap` at app-create time via
`ComponentMap::from_libraries` (`core/src/xml.rs:3248`).

## Compile: code generation roundtrip

`ComponentDef::compile_fn` is the inverse of `render_fn`: given the same
data model, emit the source that *would* call this component as a function
in the target language.

```rust,ignore
fn(&ComponentDef, &CompileTarget, &ComponentDataModel, indent: usize)
    -> ResultStringCompileError
```

This is what lets the design-time tool finish the round-trip:

```text
              ┌──────────────────┐                           ┌────────────┐
   user edits │ ComponentDataModel│ ─── render_fn  ──────────▶│  StyledDom │
              └──────────────────┘                           └────────────┘
                       ▲                                            │
                       │                                            │ inspector
                       └────────────────── compile_fn ◀─────────────┘ (data → source)
                                            │
                                            ▼
                          fn card(title: &str, body: &str) -> Dom { … }
```

A node clicked in the inspector carries `ComponentOrigin { component_id,
data_model_json }`. The inspector calls `compile_fn` with that data model
plus a target language (`Rust`, `C`, `Python`), gets back source, and
either pastes it into the user's project or hands it to the codegen path
that lives next to `api.json`. Round-trip closed.

## What the data model looks like

`ComponentDataModel` is a flat list of named fields. Each field has:

- a name (`"title"`, `"body"`, `"on_click"`, `"children"`),
- a `field_type` — one of `Value(ComponentValueType)`, `Callback(callback
  signature)`, `Children`, `StructRef("OtherType")`, or
  `EnumRef("OtherEnum")`, plus a few framework-specific cases,
- a `default_value: ComponentValue` — the initial value the inspector
  shows, and the value `render_fn` reads when nothing has overridden it.

For non-trivial types — a struct of struct of enum — `data_models` and
`enum_models` on the enclosing `ComponentLibrary` carry the type
definitions. References between fields use the type's name; the inspector
walks the references when it builds an editor for nested data.

This is what makes user-defined types editable in the inspector. A C
callback like `fn(RefAny, CallbackInfo) -> Update` shows up as a
`Callback` field; the inspector lets the user pick from a list of
registered callbacks instead of asking them to write Rust into a text box.

## Where to read the source

- `core/src/xml.rs:2094` — `ComponentDef`
- `core/src/xml.rs:2141` — `ComponentLibrary`
- `core/src/xml.rs:2172` — `ComponentMap`
- `core/src/xml.rs:2033` — `ComponentRenderFn` signature
- `core/src/xml.rs:2040` — `ComponentCompileFn` signature
- `core/src/xml.rs:2049` — `RegisterComponentFnType`
- `core/src/xml.rs:2070` — `RegisterComponentLibraryFnType`
- `core/src/xml.rs:3510` — `register_builtin_components` (the dogfood library)
- `core/src/resources.rs:523` — `AppConfig::add_component`
- `core/src/resources.rs:565` — `AppConfig::add_component_library`
