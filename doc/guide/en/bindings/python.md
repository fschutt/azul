---
slug: bindings/python
title: Python Bindings
language: en
canonical_slug: bindings/python
audience: external
maturity: wip
guide_order: 340
topic_only: false
short_desc: Installing the Python wheel, the `azul.*` module layout, and how Rust callbacks bridge to Python functions.
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

> **WIP** — the Python extension is built from the same `api.json` as the other bindings, via PyO3. The class names and method signatures are stable; the GIL semantics around long-running callbacks are still being refined. Test on your target Python version before shipping.

The Python binding ships as a single CPython extension module: `azul.so` on macOS, `azul.cpython.so` on Linux, `azul.pyd` on Windows. Drop the file next to your script (or anywhere on `sys.path`) and `import azul` works. CPython 3.10+ is supported via PyO3's `abi3` ABI; the same `azul.so` works against every minor 3.10+ release.

## Install — release archive

Download the matching artifact from `azul.rs/release/<version>/` and rename it to the import name your platform expects:

| platform | release filename | rename to |
|---|---|---|
| Linux | `azul.cpython.so` | `azul.so` |
| macOS | `azul.so` | `azul.so` |
| Windows | `azul.pyd` | `azul.pyd` |

Place the file in your project directory, then:

```python
from azul import *
```

The module exports every public class declared in `api.json` — `App`, `Dom`, `RefAny`, `Css`, `Update`, `WindowCreateOptions`, the widget set, etc. — as PyO3 `#[pyclass]` types.

## Build from source

```sh
cargo build -p azul-dll --release --no-default-features --features python-extension
```

The `python-extension` feature enables PyO3 with `abi3` and produces a CPython-compatible cdylib. The build script (`dll/build.rs`) adds `-undefined dynamic_lookup` on macOS so the extension can defer Python symbol resolution to the host interpreter at load time.

After the build:

| platform | source | rename to |
|---|---|---|
| Linux | `target/release/libazul_dll.so` | `azul.so` |
| macOS | `target/release/libazul_dll.dylib` | `azul.so` |
| Windows | `target/release/libazul_dll.dll` | `azul.pyd` |

The release pipeline (`doc/src/dllgen/build.rs`) runs the same build with `--features python-extension` once per platform and publishes the renamed file.

## Verifying the import

```sh
python3 -c "import azul; print(azul.__doc__)"
```

A successful import prints the auto-generated module docstring. If Python complains `dynamic module does not define module export function`, the file is in the wrong format for the running interpreter — check that you renamed `azul.cpython.so` to `azul.so` on Linux.

## Python is `abi3`

The wheel works against any CPython ≥ 3.10 without rebuilding per minor version. PyO3's `abi3` flag restricts the extension to the stable subset of the CPython C-API. There are no separate `cp310-…`, `cp311-…`, `cp312-…` artifacts — one file covers all of them.

## Class layout

Every C-ABI type that crosses the Python boundary becomes a `#[pyclass]` wrapper. The Python view is:

- Constructors are class methods: `Dom.body()`, `Dom.text("hello")`, `App.create(model, AppConfig.create())`.
- Setters are instance methods: `body.add_child(...)`, `body.set_inline_style("...")`.
- Enums are accessed as attributes of a class: `Update.RefreshDom`, `Update.DoNothing`.
- Callbacks are plain Python callables; the framework manages their lifetime through `RefAny`.

The Python extension is **not** a thin wrapper over `azul.h` — it is generated separately by `lang_python.rs`. Some types that exist in C (e.g. `VecRef` slice types) are skipped because they have no idiomatic Python equivalent, while others (e.g. callback wrappers) gain pure-Python convenience layers.

## Project layout

```
my-app/
├── azul.so             ← renamed from the release archive
└── hello-world.py
```

No `setup.py`, no `pyproject.toml`, no virtualenv requirement. The extension is a self-contained native module. To package for distribution, use the standard wheel tooling:

```sh
pip wheel . --wheel-dir dist/
```

with a minimal `pyproject.toml` that pulls `azul.so` in via `package_data`. PyPI distribution would ship one wheel per `(platform, arch)` cell; abi3 keeps the Python-version axis collapsed.

## Logging

The Python build enables PyO3's `pyo3-log` integration. Rust `log!` macros inside azul are forwarded to Python's standard `logging` module:

```python
import logging
logging.basicConfig(level=logging.DEBUG)
import azul
# azul's internal log messages now appear via Python logging
```

This is the only way to see framework-level diagnostics from Python; there is no `azul.set_log_level(...)` Python API.

## GIL behaviour

Layout and event callbacks run on the framework's thread, which acquires the GIL before invoking Python code. Background threads spawned via `azul.Thread` (covered in [Background Tasks](../background-tasks.md)) run *without* the GIL held; the worker function must re-acquire it before touching `RefAny`-wrapped Python objects.

For typical interactive apps the GIL is uncontested — layout and one click callback per frame is well below the threshold where contention matters. Long-running computations should move into a Rust thread or use `asyncio` with the framework's event loop hook (covered in the [Background Tasks](../background-tasks.md) page once that integration lands).

## Reading the example program

The Python hello-world is at [Hello, World — Python](../hello-world/python.md), referenced from `api.json`'s `examples[].code.python` field as `python/hello-world.py`. The full source:

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

No reflection boilerplate, no manual `RefAny.upcast` — passing a Python object to `App.create` wraps it in a `RefAny` automatically; callbacks receive the original Python instance back.

## Next

- [Hello, World — Python](../hello-world/python.md) — full program walkthrough.
- [Code Generation](../code-generation.md) — how `azul.so` / `azul.pyd` is produced.
- [Rust Bindings](rust.md), [C Bindings](c.md), [C++ Bindings](cpp.md) — the same API in other host languages.
