---
slug: bindings/python
title: Python Bindings
language: en
canonical_slug: bindings/python
audience: external
maturity: wip
guide_order: 340
topic_only: false
short_desc: Installing the Python wheel and the `azul.*` module layout
prerequisites: [hello-world, code-generation]
tracked_files:
  - api.json
  - dll/build.rs
  - doc/src/dllgen/build.rs
  - doc/src/dllgen/deploy.rs
  - doc/src/dllgen/license.rs
  - doc/src/dllgen/mod.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:50:43Z
---

# Python Bindings

> **WIP** — class names and method signatures are stable. GIL semantics around long-running callbacks are still being refined. Test on your target Python version before shipping.

The Python binding is a single CPython extension module. CPython 3.10+ is supported via PyO3's `abi3` ABI, so one file works against every minor release from 3.10 onwards.

## Install

```sh
pip install azul
```

The wheel bundles the prebuilt native library. There are no system dependencies.

If a wheel is not yet published for your platform, download the artifact from `azul.rs/release/<version>/`:

| platform | release filename | rename to |
|---|---|---|
| Linux | `azul.cpython.so` | `azul.so` |
| macOS | `azul.so` | `azul.so` |
| Windows | `azul.pyd` | `azul.pyd` |

Place the file in your project directory or anywhere on `sys.path`, then:

```python
from azul import *
```

## What you get

The module exports every public class as a PyO3-backed type: `App`, `AppConfig`, `Dom`, `RefAny`, `Css`, `Update`, `WindowCreateOptions`, the widget set, etc.

- Constructors are class methods: `Dom.create_body()`, `Dom.create_div()`, `App.create(model, AppConfig.create())`.
- Setters are instance methods: `body.add_child(...)`, `body.set_css("...")`.
- Builder forms consume self and return a new value: `dom.with_child(...)`, `dom.with_css("...")`.
- Enums are accessed as attributes: `Update.RefreshDom`, `Update.DoNothing`.
- Callbacks are plain Python callables. The framework manages their lifetime through `RefAny`.

## Verifying the import

```sh
python3 -c "import azul; print(azul.__doc__)"
```

A successful import prints the module docstring. If Python complains `dynamic module does not define module export function`, the file is in the wrong format for the running interpreter — check that you renamed `azul.cpython.so` to `azul.so` on Linux.

## abi3

The wheel works against any CPython 3.10 or newer without rebuilding per minor version. PyO3's `abi3` flag restricts the extension to the stable subset of the CPython C-API. There are no separate `cp310-...`, `cp311-...`, `cp312-...` artifacts. One file covers all of them.

## Project layout

```
my-app/
├── azul.so
└── hello-world.py
```

No `setup.py`, no `pyproject.toml`, no virtualenv requirement. The extension is a self-contained native module.

## Logging

The Python build forwards Rust `log!` macros through PyO3's `pyo3-log` integration to Python's standard `logging`:

```python
import logging
logging.basicConfig(level=logging.DEBUG)
import azul
```

This is the only way to see framework-level diagnostics from Python.

## GIL behaviour

Layout and event callbacks run on the framework's thread, which acquires the GIL before invoking Python code. Background workers spawned via `Thread` (covered in [Background Tasks](../background-tasks.md)) run without the GIL held; the worker function must re-acquire it before touching `RefAny`-wrapped Python objects.

For typical interactive apps the GIL is uncontested. Long-running computations should move into a Rust thread or use `asyncio` with the framework's event-loop hook.

## Reading the example program

The Python hello-world is at [Hello, World — Python](../hello-world/python.md). The full source:

```python
from azul import *

class DataModel:
    def __init__(self, counter):
        self.counter = counter

def layout(data, info):
    label = (Dom.p_with_text(str(data.counter))
             .with_css("font-size: 50px;"))

    button = Button.create("Increase counter")
    button.set_on_click(data, on_click)

    return (Dom.create_body()
            .with_child(label)
            .with_child(button.dom()))

def on_click(data, info):
    data.counter += 1
    return Update.RefreshDom()

if __name__ == "__main__":
    model = DataModel(5)
    window = WindowCreateOptions.create(layout)
    app = App.create(model, AppConfig.create())
    app.run(window)
```

No reflection boilerplate, no manual `RefAny.upcast`. Passing a Python object to `App.create` wraps it in a `RefAny` automatically; callbacks receive the original Python instance back.

## Next

- [Hello, World — Python](../hello-world/python.md) — full program walkthrough.
- [Rust Bindings](rust.md), [C Bindings](c.md), [C++ Bindings](cpp.md) — the same API in other host languages.
