---
slug: dom/components
title: Components
language: en
canonical_slug: dom/components
audience: external
maturity: mature
guide_order: 31
topic_only: false
short_desc: Reusable UI fragments as functions returning Dom — how composition and props work without a virtual-DOM diff.
prerequisites: [dom]
tracked_files:
  - core/src/dom.rs
  - core/src/xml.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:53:30Z
---

# Components

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
[Built-in Widgets](widgets.md).

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
[Architecture](architecture.md#building-a-state-graph), now in real Rust.

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

## XML components and named registration

A second authoring surface is XML: a component is declared in `.azul`
markup with a typed argument list, registered into a `ComponentLibrary`,
and called by name (`<card title="…"/>`). The runtime path is
`xml::str_to_dom_unstyled(root_nodes, &component_map)` introduced in
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
mechanics of *registering* a component (or a whole library of components)
so the framework can resolve `<card …/>` by name — plus the live-preview
and code-generation roundtrip the design-time tools rely on — are covered
in [Component Packs](component-packs.md).

## Where to read the source

- `core/src/dom.rs:1588` — `ComponentOrigin`
- `core/src/xml.rs:1090` — `ComponentId`, registry types
- `core/src/xml.rs:2094` — `ComponentDef` (live-preview + compile shape)
- `core/src/xml.rs:4314` — `str_to_dom_unstyled` runtime entry
- `core/src/xml.rs:4362` — `str_to_rust_code` AOT entry
