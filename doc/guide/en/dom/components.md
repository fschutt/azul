---
slug: dom/components
title: Components
language: en
canonical_slug: dom/components
audience: external
maturity: mature
guide_order: 35
topic_only: false
short_desc: Reusable UI fragments - named functions of (args) -> Dom
prerequisites: [dom]
tracked_files:
  - core/src/dom.rs
  - core/src/xml.rs
  - core/src/resources.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:53:30Z
---

# Components

## Introduction

A component is a Rust function that returns `Dom`.

There's no component trait. There's no derive macro. There's no special syntax. You compose components by calling functions. You reuse them through normal module visibility. The only value the framework actually inspects is the `Dom` you return.

```rust,no_run
use azul::prelude::*;

fn card(title: &str, body: &str) -> Dom {
    Dom::create_div()
        .with_class("card".into())
        .with_child(Dom::create_h2_with_text(title))
        .with_child(Dom::create_p_with_text(body))
}

let _ = Dom::create_body()
    .with_child(card("First", "alpha"))
    .with_child(card("Second", "beta"));
```

That's the whole model. The rest of this page covers two layers built on top.

The first layer is state. How do you thread persistent data through a component, and how do nested components forward events back up to their caller? The second layer, covered later under *Component Packs*, is registration. When you want a component visible to the XML loader or the live debugger by name, you wrap it in a `ComponentDef` and put it in a `ComponentLibrary`.

## Pure parameters

For purely visual components, pass values as parameters and return the constructed `Dom`. No `RefAny` is needed because no data has to persist across frames.

```rust,no_run
use azul::prelude::*;

pub fn badge(text: &str, kind: BadgeKind) -> Dom {
    let class = match kind {
        BadgeKind::Info  => "badge badge-info",
        BadgeKind::Warn  => "badge badge-warn",
        BadgeKind::Error => "badge badge-error",
    };
    Dom::create_span_with_text(text).with_class(class.into())
}

pub enum BadgeKind { Info, Warn, Error }
```

The caller picks the variant. The component just produces nodes. Tests are easy: call the function, walk the returned `Dom`, assert on shape.

## Owning state

When a component has internal state (an expansion flag, a counter value, a current selection), wrap it in a struct and pass it as a `RefAny` to the component's callbacks.

The state lives wherever the caller put it. The component's `dom()` method takes a clone of the `RefAny` and threads it through to each callback that needs it.

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
            .with_child(Dom::create_span_with_text(label))
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

The component doesn't own its state. The caller owns it. The component renders against it and wires callbacks back to it. That's what makes components composable. Anyone who can hand a `Counter` a `RefAny<Counter>` can build one.

For state that the *node* itself should carry (the typing buffer of a text input, a per-row checkbox, the scroll offset of a list), use `with_dataset(OptionRefAny::Some(refany))` instead. The node-attached dataset is reachable inside the callback through `info.get_dataset(info.get_hit_node())`. That's the pattern used by the built-in widgets. See [Built-in Widgets](../widgets.md).

## The backreference pattern

A component that wraps another component holds a `RefAny` to *its own* parent. When the inner component finishes its work, it follows that backreference to forward a higher-level event upward.

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

The private callback `validate` only knows about `NumberInput`. It parses the text the user typed. It updates its own state. Then it follows the backreference to the application-level callback.

The application sees a clean signature: `(parent: RefAny, info, value: i64)`. There's no string handling on the application side. There's no awareness of the inner widget's internals.

## A worked example: AgeInput over NumberInput

The application wraps `NumberInput` once more, this time to enforce a domain rule.

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

The chain `AgeInput -> NumberInput -> <input>` is a State Graph. Each layer holds one backreference, pointing at the layer above.

Events travel the chain in reverse. The `<input>` loses focus. `NumberInput::validate` runs. `AgeInput::on_age_changed` runs. Nothing leaks across layers, and nothing has to be threaded through layout.

This is the same pattern walked through in [Architecture](../architecture.md#building-a-state-graph), now in real Rust.

## Returning multiple roots

A `Dom` has a single root. To return a sequence of siblings, wrap them in a neutral container or collect into a `Dom`.

```rust,no_run
use azul::prelude::*;

pub fn breadcrumb(parts: &[&str]) -> Dom {
    parts.iter()
        .enumerate()
        .map(|(i, &p)| {
            if i == 0 {
                Dom::create_span_with_text(p)
            } else {
                Dom::create_div()
                    .with_class("crumb".into())
                    .with_child(Dom::create_span_with_text(" / "))
                    .with_child(Dom::create_span_with_text(p))
            }
        })
        .collect::<Dom>()
}
```

`collect::<Dom>` produces a `<div>` whose children are the iterator's items. There's no fragment type and no portal type. If you don't want the wrapper to affect layout, give it `display: contents`.

## Component-origin tracking

When a component's `dom()` returns, the framework can stamp the root nodes of its output with a component-origin record. That's the field the inspector populates with the qualified component id (like `"shadcn:card"`) and the JSON-serialised data model.

The origin tag has three uses. The live debugger uses it to display a Component Tree alongside the DOM Tree. The code-generation roundtrip uses it to recover the source invocation. And clicking a node to jump to the component that produced it relies on it.

The origin is set automatically when a component is registered through the component system, whether from XML or from the builder. Plain functions don't need to opt in.

# Component Packs

A *component pack* is a `ComponentLibrary` in the codebase. It's a named collection of `ComponentDef`s.

Packs are how the framework finds a component by name. `<card title="..."/>` in XML resolves through a pack. `shadcn:avatar` in the live debugger resolves through a pack. `builtin:div` resolves through a pack. They're also how a component invocation can roundtrip through the design-time tools and come back out as source code.

There's a second authoring surface in the same pipeline. A component can be declared in `.azul` markup with a typed argument list, registered into a `ComponentLibrary`, and called by name.

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

The runtime path is `Dom::create_from_parsed_xml`, introduced in [The DOM - Loading XML and XHTML](../dom.md#loading-xml-and-xhtml). It walks the parsed XML, resolves each tag against the registered component libraries, and produces the corresponding `Dom`.

Whether a component is hand-written Rust or XML-defined, the value is the same. It's a function from arguments and a `RefAny` to a `Dom`. The rest of this section is about *registering* those functions so the framework can look them up by name.

## Why packs and not just functions

A plain Rust component (`fn card(title: &str, body: &str) -> Dom`) is visible to whoever can see the symbol. That's fine for compiled-in components: your own widgets, a third-party crate's widgets.

It isn't enough when you want more.

- You want to preview a component in the live debugger without recompiling.
- You want a typed data model the debugger can edit, populate, and pass back to the render function.
- You want code generation. The design-time tool emits the equivalent Rust, C, or Python source for a tree the user laid out interactively.
- You want to load components from XML by name. `<card .../>` has to resolve to *some* function.

A `ComponentDef` (in `core/src/xml.rs`) carries everything the runtime and the design-time tools need:

```rust,ignore
pub struct ComponentDef {
    pub id: ComponentId,                 // collection + name, e.g. shadcn:card
    pub display_name: AzString,
    pub description: AzString,
    pub css: AzString,                   // ships with the component
    pub source: ComponentSource,         // where it came from (built-in, JSON, ...)
    pub data_model: ComponentDataModel,  // typed prop list - defaults double as preview values
    pub render_fn: ComponentRenderFn,    // (&def, &data, &registry) -> StyledDom
    pub compile_fn: ComponentCompileFn,  // (&def, &target_lang, &data, indent) -> source
    pub render_fn_source: OptionString,
    pub compile_fn_source: OptionString,
}
```

A `ComponentLibrary` groups defs under a name, version, and description. It also carries `exportable` and `modifiable` flags. The live editor uses those flags to decide whether the user can edit a component in place.

## Registering a pack

`AppConfig::create()` always pre-registers the `"builtin"` library. That library has 112 HTML element components (plus three control-flow builtins: `if`, `for`, `map`), so `builtin:div`, `builtin:p`, `builtin:button`, and the rest are available out of the box. Anything else is registered on top.

There are two registration shapes.

```rust,ignore
use azul::prelude::*;

let mut config = AppConfig::create();

// 1. Register a single component into a named library.
config.add_component(
    AzString::from("mylib"),
    my_register_card_fn,        // extern "C" fn() -> ComponentDef
);

// 2. Register an entire pre-built library.
config.add_component_library(
    AzString::from("shadcn"),
    register_shadcn,            // extern "C" fn() -> ComponentLibrary
);
```

Both work the same way. The registration function runs immediately at the call site. The returned `ComponentDef` or `ComponentLibrary` is moved into `config.component_libraries`. The library then becomes visible to the XML parser, to the layout callback (through `CallbackInfo`), and to the debug server.

Why the function-pointer indirection instead of a direct `ComponentDef` parameter? It's so the C and Python bindings can register libraries through their own callback shapes. The C example below shows it.

### Built-in libraries use the same registration API

The 112 built-in HTML element components are themselves registered through `add_component_library` inside `AppConfig::create()`. The render functions are the same `Dom::create_<tag>()` constructors documented in [DOM](../dom.md). Your own packs follow the same shape.

### From C

The registration callback is a `repr(C)` function pointer, so a plain function pointer is enough on the C side.

```c
extern AzComponentLibrary register_shadcn(void);

void main(void) {
    AzAppConfig config = AzAppConfig_create();
    AzAppConfig_addComponentLibrary(
        &config,
        AzString_fromCStr("shadcn"),
        register_shadcn
    );
    /* ... */
}
```

### From Python

The Python binding wraps the function pointer in a trampoline. You pass a Python callable. The binding stores it in the callback's `ctx` slot (`OptionRefAny::Some(refany)`) and dispatches through a generated trampoline.

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

`ComponentDef::render_fn` has this signature:

```rust,ignore
fn(&ComponentDef, &ComponentDataModel, &ComponentMap) -> ResultStyledDomRenderDomError
```

The render function takes a *modified* `data_model`. The design-time tool overrides default values on each field, then hands the model back. It also takes the full `ComponentMap` so the function can recursively instantiate sub-components. A `<card>` containing an `<h2>` containing a `<span>` is one render call that recurses three levels through the same registry. The return is a fully cascaded `StyledDom` ready for layout.

That's what powers the live preview. The design-time tool reads the component's data model, lets the user edit each prop, calls the render function, and shows the result inline. No recompile. No trip through the disk.

## Instantiation: from XML to DOM

When the XML parser encounters `<card title="First" body="alpha"/>`, it resolves `card` against the registered `ComponentMap`.

1. Strip the namespace if present. `<shadcn:card .../>` becomes `("shadcn", "card")`. A bare `<card .../>` falls back to `"builtin"` and is resolved like any built-in tag.
2. Look up the corresponding `ComponentDef`.
3. Take the def's `data_model` and populate each field's default value from the XML attributes. Coercion is typed.
4. Call the def's `render_fn` with the populated data model and the component map.
5. Stamp every root node of the returned `StyledDom` with a component-origin record. The qualified component id is `"shadcn:card"` and the JSON-serialised data model is the populated one. That's what lets the debugger reconstruct the invocation later.

The `ComponentMap` is what `Dom::create_from_parsed_xml` consults under the hood. The `AppConfig`'s `component_libraries` field carries the registered libraries, which are folded into a `ComponentMap` at app-create time.

## Compile: code generation roundtrip

`ComponentDef::compile_fn` is the inverse of `render_fn`. Given the same data model, it emits the source that *would* call this component as a function in the target language.

```rust,ignore
fn(&ComponentDef, &CompileTarget, &ComponentDataModel, indent: usize)
    -> ResultStringCompileError
```

That's what closes the round-trip for the design-time tool.

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

A node clicked in the inspector carries the component-origin record (the qualified component id and its data model JSON). The inspector calls `compile_fn` with that data model plus a `CompileTarget` (`Rust`, `C`, `Cpp`, or `Python`) and gets back source. From there the source can be pasted into the user's project, or handed to the codegen path that lives next to `api.json`. The round-trip is closed.

## What the data model looks like

`ComponentDataModel` is a flat list of named fields. Each field has:

- a name, like `"title"`, `"body"`, `"on_click"`, or `"children"`
- a `ComponentFieldType` (`Bool`, `F32`, `Callback`, `EnumRef`, `OptionType`, and so on)
- a `ComponentDefaultValue`. That's the initial value the inspector shows, and it's the value `render_fn` reads when nothing has overridden it.

For non-trivial types like a struct of struct of enum, `data_models` and `enum_models` on the enclosing `ComponentLibrary` carry the type definitions. References between fields use the type's name. The inspector walks those references when it builds an editor for nested data.

That's what makes user-defined types editable in the inspector. A C callback like `fn(RefAny, CallbackInfo) -> Update` shows up as a `Callback` field. The inspector lets the user pick from a list of registered callbacks instead of asking them to write Rust into a text box.


## Coming Up Next

- [Built-in Widgets](../widgets.md) — Built-in widgets and how to write your own
- [Styling with CSS](../styling.md) — Stylesheets, selectors, and the cascade
- [Layout](../layout.md) — Overview of the layout solver
