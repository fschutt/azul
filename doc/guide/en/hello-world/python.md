---
slug: hello-world/python
title: Hello, World — Python
language: en
canonical_slug: hello-world/python
audience: external
maturity: wip
guide_order: 14
topic_only: false
short_desc: Installing the Python wheel and writing the counter app in Python.
prerequisites: [hello-world]
tracked_files:
  - api.json
  - core/src/callbacks.rs
  - core/src/lib.rs
  - dll/src/lib.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# Hello, World — Python

> **WIP** — the Python extension is generated from `api.json` and currently targets Python 3.10+. The high-level shape is stable; some methods may still rename as the C ABI settles.

A complete Azul GUI in one Python file. The example matches `examples/python/hello-world.py` in the repository.

## Build the extension

The Python module is the same library as the C dynamic library, compiled with the `python-extension` feature. Build it once from a checkout:

```sh
cargo build -p azul-dll --release \
    --no-default-features --features python-extension
```

The resulting library is `target/release/libazul_dll.{so,dylib,pyd}`. Python imports it as `azul`, so rename or symlink it:

```sh
# macOS
cp target/release/libazul_dll.dylib target/release/azul.so
# Linux
cp target/release/libazul_dll.so target/release/azul.so
# Windows
copy target\release\azul_dll.dll target\release\azul.pyd
```

Then either run Python from the directory containing the file, or prepend that path to `sys.path` (see `examples/python/hello_world.py` for the boilerplate that does this for the in-repo example).

## The whole program

```python
from azul import *

class DataModel:
    def __init__(self, counter):
        self.counter = counter

def layout(data, info):
    label = Dom.text(str(data.counter))
    label.set_inline_style("font-size:50px;")

    button = Dom.div()
    button.set_inline_style("flex-grow:1;")
    button.add_child(Dom.text("Increase counter"))
    button.set_callback(On.MouseUp, data, on_click)

    body = Dom.body()
    body.add_child(label)
    body.add_child(button)

    return body.style(Css.empty())

def on_click(data, info):
    data.counter += 1
    return Update.RefreshDom

model = DataModel(5)
window = WindowCreateOptions.create(layout)

app = App.create(model, AppConfig.create())
app.run(window)
```

Read top-down; each block is one concept.

## What changes from Rust or C

Three observations.

- **No upcast / downcast boilerplate.** Pass a regular Python instance into `App.create` or `set_callback`; the binding layer wraps it in a `RefAny` automatically and gives it back to your callback as the same instance. The framework holds a strong reference until you drop the `App`.
- **No `extern "C"` qualifier.** Python callbacks are stored alongside the `RefAny` in the `ctx: OptionRefAny` field of `LayoutCallback` — see `core/src/callbacks.rs:128`. A small Rust trampoline extracts the Python callable and dispatches.
- **Strings are plain Python `str`.** The `Az*` string handling vanishes; the binding converts UTF-8 at the boundary.

## Event filters

Where Rust uses `Button::set_on_click`, Python uses `Dom.set_callback(On.MouseUp, data, callback)`. The `On.*` enum mirrors the Rust `EventFilter`:

| Python | Rust equivalent |
|---|---|
| `On.MouseUp` | `EventFilter::Hover(HoverEventFilter::MouseUp)` |
| `On.MouseDown` | `EventFilter::Hover(HoverEventFilter::LeftMouseDown)` |
| `On.MouseEnter` | `EventFilter::Hover(HoverEventFilter::MouseEnter)` |
| `On.FocusGained` | `EventFilter::Focus(FocusEventFilter::FocusReceived)` |
| `On.TextInput` | `EventFilter::Focus(FocusEventFilter::TextInput)` |

The full list lives in `core/src/events.rs`; the binding generator walks it.

## Run it

```sh
cd target/release
python3 path/to/hello-world.py
```

You should see the same window pictured on the [hello-world landing page](../hello-world.md). Click increments the counter; the layout function re-runs.

## Common errors

- **`ImportError: dynamic module does not define module export function (PyInit_azul)`** — the library was built without `--features python-extension`. Rebuild.
- **`ModuleNotFoundError: No module named 'azul'`** — the renamed `azul.so` / `azul.pyd` is not on `sys.path`. Either `cd` into its directory or prepend the path:

  ```python
  import sys, os
  sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'target', 'release'))
  import azul
  ```

- **Counter does not advance** — the click callback returned `Update.DoNothing` (or implicitly `None`). Always return `Update.RefreshDom` from a handler that mutates the model.
- **`TypeError: layout() takes 0 positional arguments but 2 were given`** — the layout callable must accept exactly `(data, info)`.

## Next

- [DOM and Callbacks](../dom.md) — the same DOM surface, written with `Dom.text`, `Dom.div`, `Dom.body` instead of the Rust constructors.
- [Python Bindings](../bindings/python.md) — full reference for the Python module surface, including type stubs and packaging.
