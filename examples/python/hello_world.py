#!/usr/bin/env python3
"""
Azul "counter" hello-world — Python port of examples/rust/src/hello-world.rs.

A DataModel { counter } is wrapped as the app's RefAny, a layout callback builds
a DOM with a text label showing the counter and a "Increase counter" button whose
on-click handler does `counter += 1` and returns Update.RefreshDom, then
`App.create(data, AppConfig.create())` + `WindowCreateOptions.create(layout)` +
`app.run(window)`.

To run (documented flow):
    # From the azul directory, after building the python extension:
    #   cargo build -r -p azul-dll --no-default-features --features python-extension
    #   cp target/release/libazul.dylib target/release/azul.so   # macOS
    #   cp target/release/libazul.so    target/release/azul.so    # Linux
    cd target/release
    python3 ../../examples/python/hello_world.py

The headless E2E runner (CI) drives it like:
    AZ_E2E="$PWD/tests/e2e/hello_world_counter.json" AZ_BACKEND=headless \
        PYTHONPATH=target/release python3 examples/python/hello_world.py

============================================================================
Verified 2026-05-28: runs headless under AZ_E2E and prints `test result: ok`.
The layout callback and the button's on-click handler both bridge to Python
through the same pyo3 ctx trampoline that every other binding callback uses —
there is no layout-specific special-casing.
============================================================================
"""

import os
import sys

# Find azul.so both from CI (PYTHONPATH=target/release) and from the documented
# `cd target/release` flow. Prepend target/release relative to the repo root.
sys.path.insert(
    0, os.path.join(os.path.dirname(__file__), "..", "..", "target", "release")
)

import azul


class DataModel:
    """Application state — held alive inside the app's RefAny."""

    def __init__(self, counter):
        self.counter = counter


def my_on_click(data, info):
    # `data` is the DataModel passed through App.create / set_on_click.
    data.counter += 1
    return azul.Update.RefreshDom


def my_layout_func(data, info):
    # Label showing the current counter value.
    label = azul.Dom.create_text(str(data.counter)).with_css("font-size: 50px")

    # Button labelled exactly "Increase counter" (the E2E runner clicks this
    # text and checks the counter increments).
    button = (
        azul.Button.create("Increase counter")
        .with_on_click(data, my_on_click)
        .dom()
        .with_css("flex-grow: 1")
    )

    body = (
        azul.Dom.create_body()
        .with_css("background-color: green")
        .with_child(label)
        .with_child(button)
    )
    return body


def main():
    data = DataModel(0)
    app = azul.App.create(data, azul.AppConfig.create())
    window = azul.WindowCreateOptions.create(my_layout_func)
    app.run(window)


if __name__ == "__main__":
    main()
