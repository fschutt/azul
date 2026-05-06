---
slug: hello-world/python
title: Hello World [Python]
language: en
canonical_slug: hello-world/python
audience: external
maturity: wip
guide_order: 14
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - core/src/callbacks.rs
  - core/src/lib.rs
  - dll/src/lib.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# Hello World [Python]

Python is the easiest way to use Azul. You can write idiomatic Python — plain classes, 
plain `str`, plain method calls — and the binding (uses Rusts `pyo3`) takes care of the rest.

## Installation

```sh
pip install azul
```

The wheel bundles the prebuilt native library, so there are no system dependencies 
to worry about. Targets **Python 3.10+** - make sure you have the right version.

> ![NOTE]
> If `pip install` does not yet have a wheel for your platform, 
> see "[Building the extension](#building-the-extension)" below for the 
> manual route.

## Simple "Counter" Example

```python
from azul import *

# Plain Python class - "single source of truth" for app state
class DataModel:
    def __init__(self, counter):
        self.counter = counter

# Layout callback: f(DataModel) -> Dom. Runs once on startup and again
# after every callback that returns Update.RefreshDom.
def layout(data, layoutcallbackinfo):

    # Rendered counter label. p_with_text wraps the text node in a <p>;
    # .with_css(...) is the builder counterpart of set_css(...) - it
    # consumes self and returns a new Dom, so we can chain inline.
    label_dom = (Dom.p_with_text(str(data.counter))
                 .with_css("font-size: 50px;"))

    # Button widget: custom widget from the "azul.widgets" module
    button = Button.create("Increase counter")
    button.set_on_click(data, on_click)
    button_dom = button.dom()

    # Final wrapup - Dom.create_body builds the root, then .with_child(...)
    # appends children. Mutating set_/add_ methods are also available; the
    # builder form just chains nicer.
    return (Dom.create_body()
            .with_child(label_dom)
            .with_child(button_dom))

# Click callback: f(DataModel) -> Update. 'data' is the same Python
# instance you passed to App.create, it is mutated in place (thread safe).
# Update variants in Python are constructor calls, hence the trailing ().
def on_click(data, info):
    data.counter += 1
    return Update.RefreshDom()

# main function
if __name__ == "__main__":

    # Initialize the data model (here we set counter=5 on startup)
    model = DataModel(5)

    # Configure the window. layout is the "/" default route; SPA-style
    # routing is done later by swapping the layout callback.
    window = WindowCreateOptions.create(layout)
    window.window_state.title = "Hello World!"
    window.window_state.size.dimensions.width = 400.0
    window.window_state.size.dimensions.height = 300.0

    # AppConfig discovers system-native styling, monitor layout, etc.
    # App.run blocks until the last window closes.
    app = App.create(model, AppConfig.create())
    app.run(window)
```

Three things to notice.

- **Pass plain Python objects.** No upcast, no downcast, no reflection macro. The binding wraps your `DataModel` instance for you and hands the *same* instance back to your callbacks. The framework holds a strong reference until you drop the `App`, so the GC will not eat it under your feet.
- **Strings are `str`, styles are CSS strings.** No `AzString`, no `String(...)` wrapper, no `AZ_CONST_STR` macro. Pass UTF-8 Python strings; the binding converts at the boundary.
- **Callbacks are regular functions** with the signature `(data, info) -> Update` (or `-> Dom` for layout). No `extern "C"`, no boxing, no decorators — just `def`.

Things we did not use that you may want to explore next.

- The `info` argument — read-only access to the system font cache, image cache, GL context, current window size, routing, and localization dictionaries in `layout`; lots of mutation helpers in `on_click` (DOM navigation, CSS overrides without rebuilding, computed-layout queries).
- `WindowCreateOptions` — title, size, decorations, transparency, monitor pinning. Covered in [windowing](../windowing.md).

## Run it

```sh
python3 hello-world.py
```

You should see the window pictured on the [hello-world landing page](../hello-world.md). Click the button: the counter increments, the layout callback re-runs, and the new value renders.

1. `app.run(window)` opened a native window and ran `layout()` once with your `DataModel` on startup.
2. The returned `Dom` was styled, laid out, and rendered.
3. On click, the framework matched the button's event filter, called `on_click(data, info)`, observed the `Update.RefreshDom` return, and re-invoked `layout()`.
4. The new `Dom` was diffed against the previous one; only the changed text node was repainted.

## Building the extension

Only needed if `pip install azul` does not yet have a wheel for your platform, or if you want to track `master`. From a checkout:

```sh
# git clone https://github.com/fschutt/azul
# cd myfolder/azul
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

Then either run Python from the directory containing the file, or prepend that path to `sys.path`:

```python
import sys, os
sys.path.insert(0,
    os.path.join(os.path.dirname(__file__), 'target', 'release'))
import azul
```

## Common errors

- **`ModuleNotFoundError: No module named 'azul'`** — `pip install azul` either failed silently or got installed into a different interpreter than the one you're running. Verify that `which python3` and `pip --version` point at the same Python install.
- **Counter does not advance** — the click callback returned `Update.DoNothing`, or it implicitly returned `None` (which the binding treats as `DoNothing`). Always end a mutating handler with `return Update.RefreshDom`.
- **`TypeError: layout() takes 0 positional arguments but 2 were given`** — your callback signature is wrong. `layout` and click handlers must accept exactly `(data, info)`.
- **Mutation isn't sticking** — you mutated a *copy* of the model instead of the instance bound to the framework. The binding always passes the same instance back; check that you are not shadowing `data` with a fresh `DataModel(...)` somewhere inside the callback.

## Coming Up Next

- [Application Architecture](../architecture.md) — Explains the concepts of architecting a larger Azul application - "What makes Azul special?"
- [Document Object Model](../dom.md) — The Dom tree - node types, hierarchy, and CSS
- [Hello World [Rust]](rust.md)
