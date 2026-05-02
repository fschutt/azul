---
slug: hello-world/rust
title: Hello, World — Rust
language: en
canonical_slug: hello-world/rust
audience: external
maturity: mature
guide_order: 11
topic_only: false
short_desc: Installation, project layout, and the full Rust source for the counter app.
prerequisites: [hello-world]
tracked_files:
  - api.json
  - core/src/callbacks.rs
  - core/src/lib.rs
  - dll/src/lib.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# Hello, World — Rust

A complete Azul GUI in one Rust file. The example matches `examples/rust/src/hello-world.rs` in the repository.

## Cargo.toml

```toml
[package]
name = "hello-azul"
version = "0.1.0"
edition = "2021"

[dependencies]
azul = { git = "https://github.com/maps4print/azul" }
```

The `azul` crate is the public-facing wrapper around `azul-dll`. It re-exports the prelude, widgets, and platform shell. The first build compiles the framework and its WebRender fork (~10 min on a recent laptop); subsequent builds are incremental.

## Imports

```rust,no_run
use azul::prelude::*;
use azul::widgets::Button;
```

`prelude::*` brings in `App`, `AppConfig`, `Dom`, `RefAny`, `Update`, `LayoutCallbackInfo`, `CallbackInfo`, and `WindowCreateOptions`. `widgets::Button` is the built-in button used below.

## The data model

Define your application data as a plain struct.

```rust,no_run
struct DataModel {
    counter: usize,
}
```

No traits to implement, no inheritance, no `Component<…>` superclass. The framework will hold this struct inside a `RefAny` — see [architecture](../architecture.md) for the design rationale.

## The layout callback

The `layout` callback is the single entry point that turns your data into a `Dom`. It runs once at startup and again whenever a callback returns `Update::RefreshDom`.

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::Button;
# struct DataModel { counter: usize }
# extern "C" fn my_on_click(_: RefAny, _: CallbackInfo) -> Update { Update::DoNothing }
extern "C" fn my_layout_func(mut data: RefAny, _: LayoutCallbackInfo) -> Dom {
    let counter = match data.downcast_ref::<DataModel>() {
        Some(d) => format!("{}", d.counter),
        None => return Dom::create_body(),
    };

    let mut label = Dom::create_text(counter.as_str());
    label.set_inline_style("font-size: 50px");

    let mut button = Button::create("Update counter");
    button.set_on_click(data.clone(), my_on_click);
    let mut button = button.dom();
    button.set_inline_style("flex-grow: 1");

    let mut body = Dom::create_body();
    body.set_inline_style("background-color: green");

    body
        .with_child(label)
        .with_child(button)
}
```

Five things to notice.

- **`extern "C"`** — every callback crosses the FFI boundary. The signature must be `extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom`; see `core/src/callbacks.rs:113`.
- **`downcast_ref::<DataModel>()`** — the runtime cast that recovers your concrete struct from the type-erased `RefAny`. It returns `Option<&DataModel>` because, at the FFI boundary, the framework cannot statically know the type.
- **`Dom::create_text`, `Dom::create_div`, `Dom::create_body`** — the three primitive node constructors. Everything else (buttons, lists, scroll regions) builds on top of them.
- **`set_inline_style("…")`** — accepts a CSS string. Multi-property strings are valid: `"font-size: 50px; color: white;"`.
- **`data.clone()`** — `RefAny::clone` bumps the reference count, it does not deep-copy your struct. The clone is given to the button so the click handler can downcast it later.

The `_: LayoutCallbackInfo` parameter carries read-only access to the system font cache, image cache, GL context, window size, and active route — see `core/src/callbacks.rs:506`. Hello-world does not use any of it.

## The click callback

Every event callback returns an [`Update`](../events.md). `Update::DoNothing` skips re-render; `Update::RefreshDom` re-runs the layout callback.

```rust,no_run
# use azul::prelude::*;
# struct DataModel { counter: usize }
extern "C" fn my_on_click(mut data: RefAny, _: CallbackInfo) -> Update {
    let mut data = match data.downcast_mut::<DataModel>() {
        Some(s) => s,
        None => return Update::DoNothing, // error
    };

    data.counter += 1;

    Update::RefreshDom
}
```

`downcast_mut` returns `Option<RefMut<'_, DataModel>>`. The borrow is checked at runtime; if another part of the program already holds a borrow, the cast fails and you must return `Update::DoNothing`.

## main

```rust,no_run
# use azul::prelude::*;
# struct DataModel { counter: usize }
# extern "C" fn my_layout_func(_: RefAny, _: LayoutCallbackInfo) -> Dom { Dom::create_body() }
fn main() {
    let data = DataModel { counter: 0 };
    let config = AppConfig::create();
    let app = App::create(RefAny::new(data), config);
    let window = WindowCreateOptions::create(my_layout_func);
    app.run(window);
}
```

`RefAny::new(data)` wraps the struct, transferring ownership to the framework. `App::create` consumes the `RefAny` and the `AppConfig`. `WindowCreateOptions::create` takes the layout callback; further fields on `WindowCreateOptions` configure window title, size, and decorations (covered in [windowing](../windowing.md)).

`app.run(window)` blocks until the last window closes.

## Build and run

```sh
cargo run --release
```

You should see the window pictured on the [hello-world landing page](../hello-world.md). Click the button: the counter increments, the layout callback re-runs, and the new value renders.

## What just happened

1. `App::run` opened a native window and ran the layout callback once with your `RefAny`.
2. The returned `Dom` was styled, laid out, and rendered on the GPU.
3. On click, the button's event filter matched a `MouseUp` inside its hit-test bounds. The framework borrowed your `RefAny` mutably, ran `my_on_click`, observed the `Update::RefreshDom` return, and re-invoked the layout callback.
4. The new `Dom` was diffed against the previous one; only the changed text node was repainted.

## Common errors

- **`downcast_ref` returns `None`** — the `RefAny` is already mutably borrowed elsewhere, or it holds a different type. Return `Dom::create_body()` (or `Update::DoNothing`) and investigate.
- **The window opens blank** — verify your layout callback actually returns a `Dom::create_body()` with children. An empty `Dom` renders to a blank window.
- **The counter does not update** — your click callback returned `Update::DoNothing`. Change to `Update::RefreshDom`.

## Next

- [DOM and Callbacks](../dom.md) — building richer trees, `IdOrClass`, and the full callback API.
- [Events and Input](../events.md) — beyond `MouseUp`: hover, focus, keyboard.
