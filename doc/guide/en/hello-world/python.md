---
slug: hello-world/python
title: Hello World [Python]
language: en
canonical_slug: hello-world/python
audience: external
maturity: mature
guide_order: 14
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - core/src/callbacks.rs
  - core/src/lib.rs
  - dll/src/lib.rs
last_generated_rev: 2660b0c45c9ea401ad6777a203f468755167e62e
generated_at: 2026-09-16T00:00:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Css
  - RefAny
  - WindowCreateOptions
  - LayoutCallbackInfo
  - CallbackInfo
  - Update
---

# Hello World [Python]

## Introduction

For Python, Azul offers a custom python extension (using the `pyo3` binding library), so you can write idiomatic Python - plain classes, 
plain `str`, plain method calls - and the binding takes care of the rest. The extension is packaged as a `.whl`, so you can easily install it with PyPI or `uv`.

Azul is not (yet) on the public pypi.org index, so you can either use the self-hosted pip index on azul.rs, or a manual download.

### Self-hosted pip index

Pointing `--index-url` at the self-hosted PEP 503 index on azul.rs makes pip (or uv, poetry, pdm) resolve `azul` from azul.rs instead of pypi.org:

```sh
pip install azul --index-url https://azul.rs/ui
```

### Manual download

If you want to install the Python extension manually, download it for your platform from the
[GitHub release](https://github.com/fschutt/azul/releases/tag/$VERSION) and put it
next to your script, so Python can find it:

```sh
# macOS
curl -L -o azul.so https://github.com/fschutt/azul/releases/download/$VERSION/azul.so
# linux
curl -L -o azul.so https://github.com/fschutt/azul/releases/download/$VERSION/azul.cpython.so
# windows
curl.exe -L -O https://github.com/fschutt/azul/releases/download/$VERSION/azul.pyd
```

The abi3 wheel targets **Python 3.10+** (pyo3 is
`abi3-py310`) - make sure you have a recent version. If there is no prebuilt module for your platform or architecture, see "[Building the extension](#building-the-extension)" below for the manual route.

## Building the extension

You can build the Azul Python extension manually with:

```sh
# git clone https://github.com/fschutt/azul
# cd myfolder/azul
cargo build -p azul-dll --release \
    --no-default-features --features python-extension
```

The resulting library is `target/release/libazul.{so,dylib}` (`azul.dll` on Windows). Python imports it as `azul`, so rename or symlink it:

```sh
# macOS
cp target/release/libazul.dylib target/release/azul.so
# Linux
cp target/release/libazul.so target/release/azul.so
# Windows
copy target\release\azul.dll target\release\azul.pyd
```

Then either run Python from the directory containing the file, or prepend that path to `sys.path`:

```python
import sys, os
sys.path.insert(0,
    os.path.join(os.path.dirname(__file__), 'target', 'release'))
import azul
```

## Simple "Counter" Example

```python
from azul import *

class DataModel:
    def __init__(self, counter):
        self.counter = counter

def layout(data, info):
    label = (Dom.create_p_with_text(str(data.counter))
             .with_css("font-size: 32px; margin: 0;"))

    button = (Button.create("Increase counter")
              .with_on_click(data, on_click)
              .dom()
              .with_css("flex-grow: 1;"))

    return (Dom.create_body()
            .with_child(label)
            .with_child(button))

def on_click(data, info):
    data.counter += 1
    return Update.RefreshDom

if __name__ == "__main__":
    model = DataModel(5)
    window = WindowCreateOptions.create(layout)
    app = App.create(model, AppConfig.create())
    app.run(window)
```

The Python extension allows you to natively work with Python functions without worrying about lifetime or reference count handling. Function callbacks and conversions between `PyObject` and Azul's `RefAny` are transparently handled for you. The binding wraps your `DataModel` instance and hands the same instance back to every callback, which you mutate in place.

In the Python extension, there are no submodules, so you can import `azul.App` instead of `azul.app.App`. Enum variants are plain class attributes, so returning `Update.RefreshDom` takes no parentheses. Styles are CSS strings, where `with_css("...")` also accepts `:hover { }`, `@media ... { }` and `@os(...)` queries inline. Builder methods such as `.with_css(...)` and `.with_child(...)` consume `self` and return a new `Dom`, so you can keep chaining the result.

The `info` argument, which this example ignores, carries read-only access to the system font cache, image cache, GL context, window size, routing and localization dictionaries into the `layout` function, plus the mutation helpers (DOM navigation, CSS overrides without rebuilding, computed-layout queries) into `on_click`. You can customize the window by changing the fields of the `WindowCreateOptions` (also see the API search box on the API documentation page).

## Run it

```sh
python3 hello-world.py
```

You should see the window pictured on the [hello-world landing page](../hello-world.md). Click the button: the counter should increment, the layout callback then re-runs, and the new value renders.

1. `app.run(window)` opens a native window and runs the layout callback once with your `DataModel` instance.
2. The returned DOM is styled, laid out, and rendered.
3. The framework then continuously queries whether anything matches the event filter set up in the DOM. On a click event, the framework takes your data model instance, runs the click callback on it, observes the `Update.RefreshDom` return, and re-invokes the layout callback.
4. The framework determines the diff between the previous frame's DOM and the current one, and only re-updates and re-paints the counter, not the entire window.

Congratulations - once you've got the hello-world example running, you've already mastered 80% of the framework. As you might have guessed, more complex UI and styling are only composing more Dom objects together and working with the various event filters. To make this more streamlined, you can now start reading about the [architecture patterns](../architecture.md) or explore what [methods the `Dom` has to offer](../dom.md). See you in the next tutorial!
