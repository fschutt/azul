---
slug: dom/components
title: Components
language: en
canonical_slug: dom/components
audience: external
maturity: mature
guide_order: 35
topic_only: false
short_desc: Reusable UI fragments - functions of (args) -> Dom, plus the named-pack registration the debugger uses
prerequisites: [dom]
tracked_files:
  - core/src/dom.rs
  - core/src/xml.rs
  - core/src/resources.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:53:30Z
---

# Components and Component Packs

A component is a Rust function whose return type is `Dom`. There is no
component trait, no derive macro, no special syntax. Composition is plain
function calls; reuse is module visibility. The framework only sees the final
`Dom` value.

```rust,no_run
# use azul::prelude::*;
fn card(title: &str, body: &str) -> Dom {
    Dom::create_div()
        .with_class("card".into())
        .with_child(Dom::h2(title))
        .with_child(Dom::p_with_text(body))
}

let _ = Dom::create_body()
    .with_child(card("First", "alpha"))
    .with_child(card("Second", "beta"));
```

That's everything to know about the model. The rest of this page is two
layers on top: how to thread state through components, and — at the end —
how to register a *named* component (or a whole library of them) so the
XML loader, the live debugger, and the design-time codegen tools can find
them by name.

## Pure parameters

For purely visual components — content, sizing, variants — pass values by
parameter and return the constructed `Dom`. No `RefAny` is involved because
no data must persist across frames.

```rust,no_run
# use azul::prelude::*;
pub fn badge(text: &str, kind: BadgeKind) -> Dom {
    let class = match kind {
        BadgeKind::Info  => "badge badge-info",
        BadgeKind::Warn  => "badge badge-warn",
        BadgeKind::Error => "badge badge-error",
    };
    Dom::span(text).with_class(class.into())
}

pub enum BadgeKind { Info, Warn, Error }
```

The caller decides the variant; the component only produces nodes. Tests are
trivial: call the function, walk the returned `Dom`, assert on its shape.

## Owning state

When a component has internal state — an expansion flag, a counter, a
selection — wrap it in a struct and pass it as a `RefAny` to the component's
callbacks. The state lives wherever the caller put it; the component's
`dom()` method takes a clone of the `RefAny` and threads it through.

```rust,ignore
use azul::prelude::*;

pub struct Counter { value: i64 }

impl Counter {
    pub fn dom(state: RefAny) -> Dom {
        let label = match state.downcast_ref::<Counter>() {
            Some(c) => format!("{}", c.value),
            None => return Dom::create_div(),
        };
        Dom::create_div()
            .with_child(Dom::span(label))
            .with_child(
                Dom::create_button_no_a11y("+1".into())
                    .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), state, increment)
            )
    }
}

extern "C" fn increment(mut data: RefAny, _info: CallbackInfo) -> Update {
    let mut c = match data.downcast_mut::<Counter>() { Some(c) => c, None => return Update::DoNothing };
    c.value += 1;
    Update::RefreshDom
}
```

The component does not own its state. The caller owns it; the component
just renders against it and wires callbacks back to it. This is what makes
components composable — anyone can construct a `Counter` because anyone can
hand it a `RefAny<Counter>`.

For state that the *node* itself should carry — the typing buffer of a
text input, a per-row checkbox, the scroll offset of a list — use
`with_dataset(OptionRefAny::Some(refany))`. The node-attached dataset is
reachable inside the callback via `info.get_dataset(info.get_hit_node())`.
This is the pattern the built-in widgets use; see
[Built-in Widgets](../widgets.md).

## The backreference pattern

A component that wraps another component holds a `RefAny` to *its own*
parent. When the inner component finishes its work, it follows the
backreference to forward a higher-level event upward.

```rust,ignore
use azul::prelude::*;

pub type OnNumberChange = extern "C" fn(RefAny, CallbackInfo, i64) -> Update;

pub struct NumberInput {
    value: i64,
    on_change: Option<(RefAny, OnNumberChange)>,
}

impl NumberInput {
    pub fn new(value: i64) -> Self { Self { value, on_change: None } }

    pub fn set_on_change(&mut self, parent: RefAny, cb: OnNumberChange) {
        self.on_change = Some((parent, cb));
    }

    pub fn dom(self) -> Dom {
        let label = format!("{}", self.value);
        let state = RefAny::new(self);
        Dom::create_input_no_a11y("number".into(), "n".into(), label.clone().into())
            .with_attribute(AttributeType::Value(label.into()))
            .with_dataset(OptionRefAny::Some(state))
            .with_callback(EventFilter::Focus(FocusEventFilter::FocusLost), RefAny::new(()), validate)
    }
}

extern "C" fn validate(_unused: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let typed = match info.get_string_contents(hit) {
        Some(s) => s, None => return Update::DoNothing,
    };
    let parsed: i64 = match typed.as_str().parse() { Ok(n) => n, Err(_) => return Update::DoNothing };

    let mut ds = match info.get_dataset(hit) { Some(d) => d, None => return Update::DoNothing };
    let on_change = {
        let mut me = match ds.downcast_mut::<NumberInput>() { Some(m) => m, None => return Update::DoNothing };
        me.value = parsed;
        me.on_change.clone()
    };
    if let Some((parent, cb)) = on_change {
        return cb(parent, info, parsed);
    }
    Update::RefreshDom
}
```

The inner private callback (`validate`) speaks only to `NumberInput`. It
parses the text the user typed, updates the component's own state, then
follows the backreference to the application-level callback. The application
sees a clean `(parent: RefAny, info, value: i64)` — no string handling, no
inner-widget concerns.

## A worked example: `AgeInput` over `NumberInput`

The application wraps `NumberInput` once more to enforce a domain rule:

```rust,ignore
use azul::prelude::*;

pub struct AgeInput { age: i64 }

extern "C" fn layout(mut data: RefAny, _: LayoutCallbackInfo) -> Dom {
    let age = match data.downcast_ref::<AgeInput>() { Some(a) => a.age, None => return Dom::create_body() };
    let mut input = NumberInput::new(age);
    input.set_on_change(data.clone(), on_age_changed);
    Dom::create_body().with_child(input.dom())
}

extern "C" fn on_age_changed(mut data: RefAny, _info: CallbackInfo, new_age: i64) -> Update {
    let mut a = match data.downcast_mut::<AgeInput>() { Some(a) => a, None => return Update::DoNothing };
    if new_age < 0 || new_age > 150 { return Update::DoNothing; }
    a.age = new_age;
    Update::RefreshDom
}
```

The chain `AgeInput → NumberInput → <input>` is a State Graph. Each layer
holds exactly one backreference — to the layer above. Events follow the
chain in reverse: `<input>` focus-lost ▸ `NumberInput::validate` ▸
`AgeInput::on_age_changed`. Nothing leaks between layers; nothing has to be
threaded through layout.

This is the same pattern walked through in
[Architecture](../architecture.md#building-a-state-graph), now in real Rust.

## Returning multiple roots

A `Dom` has a single root. To return a sequence of siblings, wrap them in a
neutral container or collect into a `Dom`:

```rust,no_run
# use azul::prelude::*;
pub fn breadcrumb(parts: &[&str]) -> Dom {
    parts.iter()
        .enumerate()
        .map(|(i, &p)| {
            if i == 0 {
                Dom::span(p)
            } else {
                Dom::create_div()
                    .with_class("crumb".into())
                    .with_child(Dom::span(" / "))
                    .with_child(Dom::span(p))
            }
        })
        .collect::<Dom>()
}
```

`collect::<Dom>` produces a div whose children are the iterator's items.
There is no fragment / portal type; "no real wrapper" is not a goal — a
`<div>` with `display: contents` reaches the same layout effect.

## Component-origin tracking

When a component's `dom()` returns, the framework can stamp the root nodes
of its output with a `ComponentOrigin` (`core/src/dom.rs:1588`):

```rust,ignore
pub struct ComponentOrigin {
    pub component_id: AzString,        // e.g. "shadcn:card"
    pub data_model_json: crate::json::Json,
}
```

The origin tag is what the live debugger uses to display a Component Tree
alongside the DOM Tree, what the code-generation roundtrip uses to recover
the source invocation, and what makes "click a node, jump to the component
that produced it" possible. It is set automatically when a component is
registered through the component system (XML or builder); plain functions do
not need to opt in.

# Component Packs

A **component pack** (a `ComponentLibrary` in the codebase) is a named
collection of `ComponentDef`s. Packs are how the framework finds a component
by name — `<card title="…"/>` in XML, `shadcn:avatar` in the live debugger,
`builtin:div` for the 52 stock HTML elements — and how it round-trips a
component invocation through the design-time tools to source code.

A second authoring surface, in the same pipeline, is XML: a component is
declared in `.azul` markup with a typed argument list, registered into a
`ComponentLibrary`, and called by name (`<card title="…"/>`). The runtime
path is `xml::str_to_dom_unstyled(root_nodes, &component_map)` introduced in
[The DOM — Loading XML and XHTML](../dom.md#loading-xml-and-xhtml); the
ahead-of-time path is `xml::str_to_rust_code(root_nodes, imports,
&component_map)` which emits the equivalent Rust source for compile-time
inclusion.

```xml
<component name="card" args="title: String, body: String">
    <div class="card">
        <h2>{title}</h2>
        <p>{body}</p>
    </div>
</component>

<app>
    <card title="First" body="alpha"/>
    <card title="Second" body="beta"/>
</app>
```

Whether a component is hand-written Rust or XML-defined, the result is the
same value: a function from arguments and a `RefAny` to a `Dom`. The
remainder of this section is *registering* those functions so the framework
can look them up by name.

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

- `core/src/dom.rs:1588` — `ComponentOrigin`
- `core/src/xml.rs:1090` — `ComponentId`, registry types
- `core/src/xml.rs:2033` — `ComponentRenderFn` signature
- `core/src/xml.rs:2040` — `ComponentCompileFn` signature
- `core/src/xml.rs:2049` — `RegisterComponentFnType`
- `core/src/xml.rs:2070` — `RegisterComponentLibraryFnType`
- `core/src/xml.rs:2094` — `ComponentDef`
- `core/src/xml.rs:2141` — `ComponentLibrary`
- `core/src/xml.rs:2172` — `ComponentMap`
- `core/src/xml.rs:3510` — `register_builtin_components` (the dogfood library)
- `core/src/xml.rs:4314` — `str_to_dom_unstyled` runtime entry
- `core/src/xml.rs:4362` — `str_to_rust_code` AOT entry
- `core/src/resources.rs:523` — `AppConfig::add_component`
- `core/src/resources.rs:565` — `AppConfig::add_component_library`
