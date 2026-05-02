---
slug: hello-world/cpp
title: Hello, World — C++
language: en
canonical_slug: hello-world/cpp
audience: external
maturity: wip
guide_order: 13
topic_only: false
short_desc: Using the C++ header bindings and idiomatic C++ wrappers for the counter app.
prerequisites: [hello-world]
tracked_files:
  - api.json
  - core/src/callbacks.rs
  - core/src/lib.rs
  - dll/src/lib.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# Hello, World — C++

> **WIP** — the C++ headers (`azul03.hpp`, `azul11.hpp`, `azul17.hpp`, …) are auto-generated wrappers over the C ABI and track `api.json`. Pin to a specific commit until the API is stable.

A complete Azul GUI in one C++ file. The example below tracks `examples/cpp/cpp17/hello-world.cpp` in the repository. Equivalent files for C++03, C++11, C++14, C++20, and C++23 live alongside it under `examples/cpp/`.

## Choose a language standard

The header `azul<NN>.hpp` matches the C++ standard you compile with. Each header wraps the same C ABI; differences are syntactic.

| Header | Standard | Notable |
|---|---|---|
| `azul03.hpp` | C++03 | No `auto`, no move semantics — `Dom_addChild(...)` style |
| `azul11.hpp` | C++11 | `auto`, `std::move`, lambdas |
| `azul17.hpp` | C++17 | Structured bindings, `std::optional`, `if constexpr` |
| `azul20.hpp` | C++20 | Concepts, modules-friendly |
| `azul23.hpp` | C++23 | `std::expected`-like patterns |

The C++17 example below is representative; the [C page](c.md) covers the underlying ABI in detail.

## Imports and reflection

```cpp
#include "azul17.hpp"
#include <string>

using namespace azul;

struct MyDataModel {
    uint32_t counter;
};
AZ_REFLECT(MyDataModel);
```

`AZ_REFLECT(MyDataModel)` generates `MyDataModel_upcast`, `MyDataModel_downcast_ref`, and `MyDataModel_downcast_mut`. The destructor is synthesised; for structs that own heap data, supply your own with `AZ_REFLECT_DESTRUCTOR(MyDataModel, fn)`.

`using namespace azul;` brings in the C++ wrapper types (`RefAny`, `Dom`, `App`, `String`, `WindowCreateOptions`); raw C types remain `Az*`-prefixed.

## The layout callback

```cpp
AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    auto d = MyDataModel_downcast_ref(data_wrapper);
    if (!d) return AzDom_createBody();

    Dom label = Dom::create_text(String(std::to_string(d->counter).c_str()))
        .with_inline_style(String("font-size: 50px;"));

    AzEventFilter event = AzEventFilter_hover(AzHoverEventFilter_mouseUp());
    Dom button = Dom::create_div()
        .with_inline_style(String("flex-grow: 1;"))
        .with_child(Dom::create_text(String("Increase counter")))
        .with_callback(event, data_wrapper.clone(), on_click);

    Dom body = Dom::create_body()
        .with_child(std::move(label))
        .with_child(std::move(button));

    return body.style(Css::empty()).release();
}
```

Notes:

- The callback signature must be `extern "C"`-compatible because the framework dispatches through C function pointers. The wrapper types accept the raw `Az*` arguments and convert them.
- `RefAny data_wrapper(data)` adopts the FFI handle without an additional refcount bump; copying it (`data_wrapper.clone()`) increments the count.
- `MyDataModel_downcast_ref` returns `const MyDataModel*` (or `nullptr` on failure). For mutable access, use `MyDataModel_downcast_mut`.
- `.release()` on the wrapper `Dom` yields the raw `AzDom` the framework expects to receive. Without it, the wrapper destructor would run and free the tree before the framework consumed it.

## The click callback

```cpp
AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    auto d = MyDataModel_downcast_mut(data_wrapper);
    if (!d) return AzUpdate_DoNothing;
    d->counter += 1;
    return AzUpdate_RefreshDom;
}
```

`auto d` is a smart-pointer-like guard that releases the mutable borrow on scope exit. There is no explicit `_delete` step; the wrapper does it for you.

## main

```cpp
int main() {
    MyDataModel model = {5};
    RefAny data = MyDataModel_upcast(model);

    WindowCreateOptions window = WindowCreateOptions::create(layout);

    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));

    return 0;
}
```

`AppConfig::default_()` (with the trailing underscore — `default` is a C++ keyword) constructs the default configuration. `app.run(std::move(window))` blocks until the last window closes; the destructor cleans up the framework.

## Compile and link

C++17, GCC or Clang:

```sh
g++ -std=c++17 hello-world.cpp \
    -I/path/to/azul-headers \
    -L/path/to/azul-lib -lazul \
    -Wl,-rpath,/path/to/azul-lib \
    -o hello-world
./hello-world
```

C++03 (matching `examples/cpp/cpp03/hello-world.cpp`):

```sh
g++ -std=c++03 hello-world.cpp -I/path/to/azul-headers \
    -L/path/to/azul-lib -lazul -o hello-world
```

MSVC:

```bat
cl /std:c++17 /EHsc hello-world.cpp /I path\to\azul-headers \
   /link /LIBPATH:path\to\azul-lib azul.lib
```

## Differences from the C version

- Wrapper types (`RefAny`, `Dom`, `String`, `App`) own their underlying `Az*` handle and free it on destruction.
- `.with_*` builder methods replace the `Az*_set*` / `Az*_add*` C functions; they consume `*this` and return a new value, so chain them inline.
- `std::move` is required when transferring ownership to the framework (`App::run(std::move(window))`); a copy would leave you with two handles competing to free the same memory.

## Common errors

- **Double-free at exit** — you forgot `.release()` on a `Dom` returned to the framework, or you copied a wrapper that should have been moved. Use `std::move`.
- **Linker error: `undefined reference to AzApp_create`** — the dynamic library is not linked. Add `-lazul` and confirm the rpath.
- **Counter is stuck** — the click callback returned `AzUpdate_DoNothing`. Verify the downcast succeeded (`d != nullptr`).

## Next

- [DOM and Callbacks](../dom.md) — same surface as Rust, with the C++ wrappers in place of `azul::*` types.
- [C++ Bindings](../bindings/cpp.md) — full reference for the C++ wrapper surface across language standards.
