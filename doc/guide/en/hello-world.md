---
slug: hello-world
title: Hello, World
language: en
canonical_slug: hello-world
audience: external
maturity: mature
guide_order: 10
topic_only: false
short_desc: Introduction — a simple counter app showing window setup, state in a RefAny, layout callback, and a click handler.
prerequisites: []
tracked_files:
  - api.json
  - core/src/callbacks.rs
  - core/src/lib.rs
  - dll/src/lib.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# Hello, World

A 50-line program produces a window with a counter label and a button. Clicking the button increments the counter and re-runs the layout callback.

```azul-render screenshot=hello-world width=400 height=240 subtitle="The minimum viable Azul window — counter label plus a button."
<body style="background-color: green; flex-direction: column;">
  <p style="font-size: 50px; color: white; padding: 8px;">5</p>
  <div style="flex-grow: 1; padding: 16px; background-color: #2563eb; color: white; font-size: 18px; text-align: center;">Increase counter</div>
</body>
```

## Pick your language

Each guide is self-contained. Pick the one that matches your build environment; you do not need to read the others.

| Language | Page | Status |
|---|---|---|
| Rust | [hello-world/rust](hello-world/rust.md) | mature |
| C (C99+) | [hello-world/c](hello-world/c.md) | wip |
| C++ (03 / 11 / 17 / 20 / 23) | [hello-world/cpp](hello-world/cpp.md) | wip |
| Python (3.10+) | [hello-world/python](hello-world/python.md) | wip |

## What every page covers

Each per-language page walks the same five-step path:

1. Install or link the Azul library.
2. Define a data model.
3. Write a `layout` callback that returns a `Dom`.
4. Attach a click callback that mutates the model and returns `Update::RefreshDom`.
5. Build, run, see a window.

The end result is identical across languages: the same window, the same counter, the same click behaviour. Differences are limited to syntax and to how the host language obtains a `RefAny` from a struct — see [Architecture — Data Access](architecture.md#data-access-format-and-locality) for the design rationale.

## Three concepts the program uses

These appear in every per-language version. Skim the definitions; the per-language page repeats them in context.

- **`RefAny`** — a type-erased, reference-counted handle to your data. The framework holds it on your behalf; your callbacks downcast it back to your concrete type.
- **`Dom`** — the tree your `layout` callback returns. Built by composing nodes (`Dom::create_body`, `Dom::create_text`, `Dom::create_div`) and adding inline CSS or callbacks.
- **`Update`** — what every event callback returns. `Update::DoNothing` skips re-render; `Update::RefreshDom` re-runs the `layout` callback for the next frame.

## How a callback travels across the FFI boundary

The same program runs from Rust, C, C++, and Python because every callback funnels through one C-ABI signature: `extern "C" fn(RefAny, CallbackInfo) -> Update`. Native code stores its function pointer in `LayoutCallback::cb`; FFI bindings (Python, future scripted languages) store a small Rust trampoline in `cb` and put the foreign callable in the `ctx: OptionRefAny` field — see `core/src/callbacks.rs:128`. The trampoline extracts the foreign function and dispatches.

You will not see this layer directly when writing app code, but it is why the four per-language pages all describe the same lifecycle.

## What is not covered yet

The hello-world is a single window with one click handler and no styling beyond inline CSS. Multi-window apps, external stylesheets, the widget library, animations, scrolling, and IME-aware text input are introduced on later pages of this guide.
