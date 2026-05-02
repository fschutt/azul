---
slug: hello-world/cpp
title: Hello World [C++]
language: en
canonical_slug: hello-world/cpp
audience: external
maturity: wip
guide_order: 13
topic_only: false
short_desc: Hello World example in C++ - covers installation, project layout, and simple "counter" app
prerequisites: [hello-world]
tracked_files:
  - api.json
  - core/src/callbacks.rs
  - core/src/lib.rs
  - dll/src/lib.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# Hello World [C++]

The C++ binding is a thin, header-only wrapper over the [C ABI](./c.md): same DLL, same `azul.h` underneath,
however, in difference to C you get RAII types and builder methods on top, as well as integration with 
`std::string` and modern C++, depending on which standard you have available (`libazul` supports everything 
from C++03 to C++23).

There is one wrapper header per C++ standard rather than a single "C++ header", because C++ has 
shifted significantly between standards (move semantics, `auto`, structured bindings, concepts, ...). 
Pick the one that matches the standard you compile with. This guide is written for C++17, if you use an
older version, you might benefit more from reading the [C guide](./c.md).

## Installation

### Pre-built DLL (recommended)

Same as the [C installation](./c.md#installation), the ideal 
installation uses your system package manager:

```sh
# windows
choco install libazul
# linux - debian-like
apt install libazul
# linux - arch-like
yum install libazul
# macos
brew install libazul
```

This installs `libazul.{so,dylib,dll}` plus the family of `azul<NN>.hpp` wrappers (and the underlying C `azul.h`) into the standard system locations, so a plain `g++ hello-world.cpp -lazul` will pick everything up. Alternatively, download the wrapper(s) and the DLL manually from the [/releases](/releases) page (or in your CI):

```sh
# wrapper for the C++ standard you target
wget -O azul17.hpp https://azul.rs/release/1.0.0-alpha1/azul17.hpp
# also: azul03.hpp, azul11.hpp, azul14.hpp, azul20.hpp, azul23.hpp

# windows
iex -O https://azul.rs/release/1.0.0-alpha1/azul.dll
# linux
wget -O https://azul.rs/release/1.0.0-alpha1/libazul.so
# macos
wget -O https://azul.rs/release/1.0.0-alpha1/libazul.dylib
```

You then either install both into a system path or pass `-I` and `-L` to the compiler.

### Pick a language standard

Each header wraps the same C ABI. The deltas between standards are
small today — most of the modern-C++ niceties we'd like to expose
(template-based reflection, concepts, `import azul;`, real
`std::expected`, deducing `this`) are tracked in
[`scripts/CPP_CODEGEN_MODERNIZATION.md`](https://github.com/fschutt/azul/blob/master/scripts/CPP_CODEGEN_MODERNIZATION.md)
and are not in the headers yet.

What you actually get today:

- `azul03.hpp`: no `noexcept`, no move semantics — Colvin-Gibbons move emulation, copy-only types.
- `azul11.hpp`: `noexcept` everywhere, RAII move semantics, lambdas. Same generator backs `azul14.hpp`.
- `azul17.hpp`: adds `[[nodiscard]]` on factory methods and `Option<T>::toStdOptional() -> std::optional<T>`.
- `azul20.hpp`: adds `Vec<T>::toSpan() -> std::span<T>` for zero-copy access.
- `azul23.hpp`: same generator as `azul20.hpp` for now; `std::expected` integration is a placeholder comment, not real yet.

The example below is C++17 — representative of what you'll write 90%
of the time. The full set of C++ examples lives under
`examples/cpp/cpp<NN>/` in the repository.

## Simple "Counter" Example

The C++17 version of the counter is about ~50 lines (without comments). 
The wrapper types own their `Az*` handle and free it on destruction, so 
unlike C you do *not* have to pair every `_create` with a `_delete` — RAII 
does that for you:

```cpp
#include "azul17.hpp"
#include <string>

// Brings in RefAny, Dom, App, String, Css, Button, WindowCreateOptions, ...
// Raw C types remain Az*-prefixed; wrapper types have no prefix.
using namespace azul;

// Data model: Plain old struct - the "single source of truth" for app state.
struct MyDataModel {
    uint32_t counter;
};

// AZ_REFLECT generates:
//
//   MyDataModel_upcast(struct)         -> RefAny
//   MyDataModel_downcast_ref(refany)   -> const MyDataModel*  (or nullptr)
//   MyDataModel_downcast_mut(refany)   -> MyDataModel*        (or nullptr)
//
// It stores a compiler-generated tag in the RefAny, so that the framework
// can verify type-safety casts at runtime. The destructor is synthesised
// from the C++ type; if your struct owns heap data, supply your own with
// AZ_REFLECT_DESTRUCTOR(MyDataModel, fn).
AZ_REFLECT(MyDataModel);

// Forward-declare on_click so layout() can pass it to the button.
// All UI callbacks share this signature, and they MUST use the raw C
// types because the framework dispatches through C function pointers.
AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

// f(DataModel) -> Dom. Runs once on startup and again after every
// callback that returns Update::RefreshDom.
AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {

    // Adopt the FFI handle into a RAII wrapper. RefAny does NOT bump
    // the refcount on construction (it adopts the reference the
    // framework handed us); .clone() bumps the count.
    RefAny data_wrapper(data);

    // downcast_ref returns const MyDataModel* (or nullptr on failure).
    // The borrow is released automatically when 'd' goes out of scope.
    auto d = MyDataModel_downcast_ref(data_wrapper);
    if (!d) return AzDom_createBody();

    // Counter label - builder-style API. Each .with_* consumes *this
    // and returns a new value, so chain them inline.
    Dom label = Dom::create_p_with_text(String(std::to_string(d->counter).c_str()))
        .with_inline_style(String("font-size: 50px;"));

    // Button widget - has its own helper API on top of Dom.
    // .clone() bumps the refcount on the RefAny; the clone is moved
    // into the button so the framework can hand it back to on_click.
    Button button = Button::create(String("Increase counter"))
        .with_button_type(AzButtonType_Primary)
        .with_on_click(data_wrapper.clone(), on_click);

    // Final wrapup. .style() applies a CSS sheet; Css::empty() = no
    // stylesheet (we used inline styles above).
    //
    // .release() yields the raw AzDom and zeroes out the wrapper.
    // Without it, the wrapper's destructor would run on the way out
    // and free the tree before the framework consumed it.
    return Dom::create_body()
        .with_child(std::move(label))
        .with_child(button.dom())
        .style(Css::empty())
        .release();
}

// Definition of the click callback forward-declared above. The framework
// invokes this through a C function pointer when the button's hit-test
// matches a MouseUp event.
AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);

    // downcast_mut is the mutable counterpart. The borrow is released
    // automatically when 'd' goes out of scope - no explicit _delete.
    auto d = MyDataModel_downcast_mut(data_wrapper);
    if (!d) return AzUpdate_DoNothing;

    d->counter += 1;

    // RefreshDom queues a new layout() invocation:
    // dom build -> cascade -> relayout -> display list -> render
    return AzUpdate_RefreshDom;
}

int main() {

    // Initialize the data model
    MyDataModel model = { 5 };

    // Move ownership of the model into a RefAny.
    RefAny data = MyDataModel_upcast(model);

    // Configure the window(s) to spawn on startup. layout() is the
    // "/" default route; SPA-style routing is done later by swapping
    // the layout callback on a window.
    WindowCreateOptions window = WindowCreateOptions::create(layout);

    // 'default' is a C++ keyword - the wrapper exposes it as
    // default_() with a trailing underscore.
    //
    // AppConfig discovers system-native styling, monitor layout, etc.
    App app = App::create(std::move(data), AppConfig::default_());

    // Blocks until the last window closes; the destructor cleans
    // up the framework instance on scope exit.
    app.run(std::move(window));
    return 0;
}
```

Five things to notice.

- **`AZ_REFLECT(MyDataModel)`** — shorter than the C `AZ_REFLECT(MyDataModel, destructor)` because the C++ wrapper synthesises the destructor for you. If your struct owns heap data, supply your own with `AZ_REFLECT_DESTRUCTOR(MyDataModel, fn)`. The macro emits `MyDataModel_upcast`, `MyDataModel_downcast_ref`, and `MyDataModel_downcast_mut` plus a runtime type tag.
- **RAII over manual `_delete`** — `auto d = MyDataModel_downcast_ref(...)` returns a smart-pointer-like guard that releases the runtime borrow on scope exit. There is no explicit pairing with `_delete` like in C, which removes a whole class of bugs.
- **`.release()` at the end of `layout`** — wrapper types own their underlying `Az*` handle and free it on destruction. When you return one to the framework, you must call `.release()` to *transfer* ownership; otherwise the wrapper's destructor will run on the way out and free the tree before the framework consumes it.
- **`.with_*` builder methods consume `*this`** — they return a new value rather than mutating in place. Chain them inline; they do not allocate beyond what the underlying `Az*_set*` would. The corresponding `set_*` methods (e.g. `Button::set_on_click`) mutate in place if you prefer that style.
- **`std::move` for ownership transfer** — `App::run(std::move(window))`, `App::create(std::move(data), ...)`. A copy would leave you with two handles competing to free the same memory; if there's a debug build you'll get a double-free at exit. Modern compilers warn when a value is implicitly copied where a move was wanted.

Things we did not use that you may want to explore next.

- `AzLayoutCallbackInfo` — read-only access to the system font cache, image cache, GL context, current window size, routing, and localization dictionaries.
- `AzCallbackInfo` — many functions for navigating the DOM, mutating CSS without rebuilding the tree, querying computed layout / styles, etc.
- `WindowCreateOptions` — title, size, decorations, transparency, monitor pinning. Same fields as in C; covered in [windowing](../windowing.md).

## Build and run

If you installed `libazul` through your system package manager, the
header and the shared library live in standard locations and the
compiler will find them on its own — one line is enough:

```sh
g++ -std=c++17 hello-world.cpp -lazul -o hello-world
./hello-world
```

(On Windows with `chocolatey` / `vcpkg`, the equivalent is
`cl /std:c++17 /EHsc hello-world.cpp azul.lib` once `azul.lib` is on
the linker search path.)

If you downloaded the wrappers and DLL manually (or built from source),
you have to point the compiler at them explicitly. `-I` / `-L` add
include and link search paths; `-Wl,-rpath` tells the dynamic loader
where to find `libazul.{so,dylib}` at runtime so you do not have to
set `LD_LIBRARY_PATH` (Linux) or `DYLD_LIBRARY_PATH` (macOS) every
time you run the binary.

```sh
# Linux
g++ -std=c++17 hello-world.cpp \
    -I/path/to/azul-headers \
    -L/path/to/azul-lib -lazul \
    -Wl,-rpath,/path/to/azul-lib \
    -o hello-world

# macOS — @executable_path resolves relative to the binary, so you can
# ship the .dylib next to the .bin and the loader will pick it up
g++ -std=c++17 hello-world.cpp \
    -I/path/to/azul-headers \
    -L/path/to/azul-lib -lazul \
    -Wl,-rpath,@executable_path/. \
    -o hello-world

# Windows (MSVC) — drop azul.dll next to the .exe at run time
cl /std:c++17 /EHsc hello-world.cpp /I path\to\azul-headers ^
   /link /LIBPATH:path\to\azul-lib azul.lib

# C++03 - same DLL, different wrapper
g++ -std=c++03 hello-world.cpp -I/path/to/azul-headers \
    -L/path/to/azul-lib -lazul -o hello-world
```

You should see the window pictured on the [hello-world landing page](../hello-world.md). Click the button: the counter increments, the layout callback re-runs, and the new value renders.

1. `app.run(std::move(window))` opened a native window and ran `layout()` once with your `RefAny` on startup.
2. The returned `AzDom` was styled, laid out, and rendered (default: CPU-rendered; can be GPU-rendered if needed).
3. On click, the button's event filter matched a `MouseUp` inside its hit-test bounds. The framework borrowed the `RefAny` mutably, ran `on_click`, observed the `AzUpdate_RefreshDom` return, and re-invoked `layout()`.
4. The new `AzDom` was diffed against the previous one; only the changed text node was repainted.

## Common errors

- **Double-free at exit** — you forgot `.release()` on a wrapper returned to the framework, or you copied a wrapper that should have been moved. Use `std::move`, and check that every `Dom` / `RefAny` / `WindowCreateOptions` / `Button` you hand back to the framework is `release`'d or `std::move`'d.
- **Linker error: `undefined reference to AzApp_create`** — the dynamic library is not linked. Add `-lazul` and confirm the rpath (`-Wl,-rpath,/path/to/azul-lib` on Linux, `@executable_path/.` on macOS, place `azul.dll` next to the `.exe` on Windows).
- **Counter does not update on click** — the click callback returned `AzUpdate_DoNothing`, or the downcast silently returned `nullptr`. Verify with an `assert(d != nullptr)` or a print before the increment.
- **The window opens blank** — the layout callback returned an empty body, or you forgot a `.with_child(...)` somewhere in the chain.
- **`error: 'auto' not allowed`** — you are compiling with `-std=c++03`. Either upgrade to `c++11` or use the explicit form (`MyDataModelRef d = MyDataModel_downcast_ref(...);`).


### Building from source

Only needed if you want to track `master` or patch the library locally:

```sh
# git clone https://github.com/fschutt/azul
# cd myfolder/azul
# generate the bindings from api.json (required - emits azul.h plus
# every azul<NN>.hpp wrapper under target/codegen/)
cargo run -p azul-doc --release -- codegen all
# build the actual DLL
cargo build -p azul-dll --release --features build-dll
```

The DLL lands at `target/release/libazul.{so,dylib}` (or `azul.dll`). The wrappers live at `target/codegen/azul<NN>.hpp`. Copy the wrapper for your standard plus the DLL somewhere your C++ compiler can find them.

## Coming Up Next

- [DOM and Callbacks](../dom.md) — building richer trees, `IdOrClass`, the full callback API. Same surface as Rust, with the C++ wrappers in place of the `azul::*` types.
- [C++ Bindings](../bindings/cpp.md) — full reference for the C++ wrapper surface across language standards.
