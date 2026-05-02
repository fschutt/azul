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
but on top you get RAII types, builder methods, integration with `std::string` / `std::optional` /
`std::expected` / `std::span`, and template-based reflection. The wrapper is generated separately for
each C++ standard, so it scales from `-std=c++03` (Colvin-Gibbons move emulation) all the way to
`-std=c++23` (deducing `this`, `std::expected`).

There is one wrapper header per standard rather than a single "C++ header", because C++ has shifted
significantly between standards (move semantics, `auto`, structured bindings, concepts, modules, …).
Pick the one that matches the standard you compile with. This guide is written for C++17, which is
representative of what most projects use today; C++20/23 add the same features as C++17 plus the ones
called out below.

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
# C++20+ users also get a sibling azul.cppm for `import azul;` support.

# windows
iex -O https://azul.rs/release/1.0.0-alpha1/azul.dll
# linux
wget -O https://azul.rs/release/1.0.0-alpha1/libazul.so
# macos
wget -O https://azul.rs/release/1.0.0-alpha1/libazul.dylib
```

You then either install both into a system path or pass `-I` and `-L` to the compiler.

### Pick a language standard

Each header wraps the same C ABI; the deltas are real, not cosmetic.
What you actually get per standard, in code:

- **`azul03.hpp`** — no `noexcept`, no move semantics. Uses the
  Colvin-Gibbons trick to return non-copyable RAII objects. Reflection
  goes through the `AZ_REFLECT(StructName)` macro, which emits
  `StructName_upcast` / `_downcast_ref` / `_downcast_mut`. No template
  metaprogramming on the user side.
- **`azul11.hpp`** — `noexcept` everywhere, real move semantics, lambdas.
  `AZ_REFLECT` is replaced by template members on `RefAny` itself:
  `RefAny::create<T>(model)` (factory; T is deduced from the argument),
  `refany.downcast_ref<T>()`, `refany.downcast_mut<T>()`,
  `RefAny::type_id<T>()`. No per-type macro line — any `T` you hand to
  `RefAny::create` registers itself the first time it is instantiated.
- **`azul14.hpp`** — same as C++11 plus `RefAny::type_id_v<T>` (variable-
  template shorthand for `RefAny::type_id<T>()`) and `auto`-return
  functions.
- **`azul17.hpp`** — adds:
  - `std::string_view` sibling overloads on every `String`-taking method,
    so `"foo"sv` flows in without a `String(...)` wrapping step;
  - `[[nodiscard]]` on factory and constructor methods;
  - `Option<T>::toStdOptional() -> std::optional<Inner>` plus the matching
    implicit conversion;
  - structured bindings on every `ResultXxx` wrapper:
    `auto [ok, err] = std::move(result);` works without per-class hooks.
- **`azul20.hpp`** — adds:
  - the `azul::ReflectableModel` concept; the `RefAny::create` /
    `downcast_ref` / `downcast_mut` / `type_id` template members are
    constrained by it, so feeding a non-reflectable type produces a
    readable requires-clause error rather than a wall of template-
    instantiation noise;
  - `Vec<T>::toSpan() -> std::span<T>` for zero-copy access;
  - a sibling `azul.cppm` module partition file. With a modules-aware
    toolchain you can `import azul;` instead of `#include "azul20.hpp"`.
- **`azul23.hpp`** — adds:
  - `Result<Ok, Err>::toStdExpected() && -> std::expected<Ok, Err>` and the
    matching implicit conversion. Methods returning a `ResultXxx` wrapper
    can be assigned straight into a `std::expected<Ok, Err>`, then chained
    monadically with `.and_then` / `.or_else`.
  - Deducing-`this` builder methods: every `with_*` is emitted as a
    `template<class Self> auto with_xxx(this Self&& self, …)` so the
    same method body works on l-values and r-values without separate
    `const&` / `&&` overloads.

The example below is C++17 — representative of what most projects write.
The full set of C++ examples lives under `examples/cpp/cpp<NN>/` in the
repository; each standard's `hello-world.cpp` exercises that standard's
own features.

## Simple "Counter" Example

The C++17 version of the counter is about ~50 lines (without comments).
The wrapper types own their `Az*` handle and free it on destruction, so
unlike C you do *not* have to pair every `_create` with a `_delete` — RAII
does that for you:

```cpp
#include "azul17.hpp"
#include <optional>
#include <string>
#include <string_view>

// Brings in RefAny, Dom, App, String, Css, Button, WindowCreateOptions, ...
// Raw C types remain Az*-prefixed; wrapper types have no prefix.
using namespace azul;
using namespace std::string_view_literals;

// Data model: a plain struct - the "single source of truth" for app state.
// No AZ_REFLECT macro line in C++11+: reflection is template-based.
struct MyDataModel {
    uint32_t counter;
    // OptionXxx wrappers convert implicitly to std::optional<Inner>, so a
    // model field that nullably caches a parsed URL keeps its source-of-
    // truth shape while the rest of the app reads it as std::optional.
    std::optional<AzUrl> last_url;
};

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

    // refany.downcast_ref<T>() -> const T* (or nullptr). Per-type
    // identity is derived from the address of a template-instantiated
    // static, so the compiler stamps a unique tag per T at link time -
    // no per-type registration line, no AZ_REFLECT macro.
    auto* d = data_wrapper.downcast_ref<MyDataModel>();
    if (!d) return AzDom_createBody();

    // Counter label. String-taking methods all gained std::string_view
    // sibling overloads in C++17, so "..."sv literals flow straight in.
    // .with_* methods consume *this and return a new value, so chain
    // them inline.
    return Dom::create_body()
        .with_child(Dom::p_with_text(String(std::to_string(d->counter).c_str()))
            .with_css("font-size: 50px;"sv))
        .with_child(Button::create("Increase counter"sv)
            .with_button_type(AzButtonType_Primary)
            .with_on_click(data_wrapper.clone(), on_click)
            .dom())
        .style(Css::empty())
        // .release() yields the raw AzDom and zeroes out the wrapper.
        // Without it, the destructor would run on the way out and free
        // the tree before the framework consumed it.
        .release();
}

// Definition of the click callback forward-declared above. The framework
// invokes this through a C function pointer when the button's hit-test
// matches a MouseUp event.
AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);

    // downcast_mut is the mutable counterpart - the borrow tracking is
    // identical; nullptr means either the type doesn't match or the
    // RefAny is already borrowed elsewhere.
    auto* d = data_wrapper.downcast_mut<MyDataModel>();
    if (!d) return AzUpdate_DoNothing;

    d->counter += 1;

    // RefreshDom queues a new layout() invocation:
    // dom build -> cascade -> relayout -> display list -> render
    return AzUpdate_RefreshDom;
}

// Every ResultXxx wrapper destructures into (std::optional<Ok>, std::optional<Err>)
// via the codegen's tuple_size / tuple_element specializations. No per-class
// helper - just structured bindings.
static void demo_structured_bindings() {
    auto [ok, err] = std::move(Url::parse("https://example.com/"sv));
    if (ok) {
        // *ok is an AzUrl; the Url wrapper would adopt it via Url(*ok).
    } else if (err) {
        // *err is an AzUrlParseError.
    }
}

int main() {

    // Initialize the data model. std::nullopt as a model field is fine -
    // it'll convert to AzOptionUrl when the codegen needs it.
    MyDataModel model = { 5, std::nullopt };
    (void)demo_structured_bindings;

    // Move ownership of the model into a RefAny via RefAny::create<T>(model).
    // T is deduced from the argument; the spelling
    //     RefAny::create<MyDataModel>(std::move(model))
    // also works if you want it explicit. No AZ_REFLECT line was needed -
    // RefAny::create registers T's identity the first time it's instantiated.
    RefAny data = RefAny::create(std::move(model));

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

Six things to notice.

- **No `AZ_REFLECT` line for C++11+** — `RefAny::create<T>` / `refany.downcast_ref<T>()` / `refany.downcast_mut<T>()` are template members on `RefAny` itself, so the API reads like every other wrapper class. The compiler stamps a unique runtime tag per `T` via the address of a template-instantiated `static`, so identity is stable across translation units without per-type registration. The `AZ_REFLECT(StructName)` macro is still emitted in `azul03.hpp` for C++03 compatibility (no template member functions there).
- **RAII over manual `_delete`** — `auto* d = data_wrapper.downcast_ref<MyDataModel>()` returns a borrowed pointer that reflects the runtime borrow state. There is no explicit pairing with `_delete` like in C, which removes a whole class of bugs.
- **`.release()` at the end of `layout`** — wrapper types own their underlying `Az*` handle and free it on destruction. When you return one to the framework, you must call `.release()` to *transfer* ownership; otherwise the wrapper's destructor will run on the way out and free the tree before the framework consumes it.
- **`.with_*` builder methods consume `*this`** — they return a new value rather than mutating in place. Chain them inline; they do not allocate beyond what the underlying `Az*_set*` would. The corresponding `set_*` methods (e.g. `Button::set_on_click`) mutate in place if you prefer that style.
- **`std::move` for ownership transfer** — `App::run(std::move(window))`, `App::create(std::move(data), ...)`. A copy would leave you with two handles competing to free the same memory; if there's a debug build you'll get a double-free at exit. Modern compilers warn when a value is implicitly copied where a move was wanted.
- **`std::string_view` flows in** — `Button::create("Increase counter"sv)` and `with_css("font-size: 50px;"sv)` use the C++17 sv-literal directly. The codegen emits sibling `(std::string_view)` overloads on every method whose original signature took a `String`, so there is no `String("...")` wrapping step.

Things we did not use that you may want to explore next.

- `AzLayoutCallbackInfo` — read-only access to the system font cache, image cache, GL context, current window size, routing, and localization dictionaries.
- `AzCallbackInfo` — many functions for navigating the DOM, mutating CSS without rebuilding the tree, querying computed layout / styles, etc.
- `WindowCreateOptions` — title, size, decorations, transparency, monitor pinning. Same fields as in C; covered in [windowing](../windowing.md).

### What changes for older / newer standards

- `examples/cpp/cpp03/hello-world.cpp` keeps the explicit `AZ_REFLECT(MyDataModel)` line and uses `MyDataModel_upcast` / `MyDataModel_downcast_ref` / `MyDataModel_downcast_mut` directly. No move semantics, no string-view, no `std::optional`.
- `examples/cpp/cpp14/hello-world.cpp` adds `auto`-return on `layout` and a runtime sanity check on `RefAny::type_id_v<MyDataModel>` (the address-of-static trick that backs it isn't a constant expression, so it can't be `static_assert`-ed).
- `examples/cpp/cpp20/hello-world.cpp` `static_assert`s on `azul::ReflectableModel<MyDataModel>` (the concept itself is `constexpr`-friendly, the *value* of `type_id_v` isn't), and feeds a `U8Vec` straight into a function taking `std::span<const uint8_t>` via the implicit `toSpan()` conversion.
- `examples/cpp/cpp23/hello-world.cpp` returns a `std::expected<AzUrl, AzUrlParseError>` directly from a function whose body just does `return Url::parse("…"sv);`. The implicit `operator std::expected<Ok, Err>() &&` on the `Result` wrapper does the conversion. The example also exercises the deducing-`this` builders by chaining `.with_*` on a mix of l-value and r-value `Dom`s in the same expression.

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

# C++20+ with modules: precompile the sibling azul.cppm once, then
# replace the #include with `import azul;` in your source files.
clang++ -std=c++20 -fmodules -c azul.cppm
clang++ -std=c++20 -fmodules hello-world.cpp -lazul -o hello-world
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
- **`error: 'auto' not allowed`** — you are compiling with `-std=c++03`. Either upgrade to `c++11`, or use the `azul03.hpp` example template, which goes through the `AZ_REFLECT(StructName)` macro and the raw `Az*` types directly.
- **`no member named 'p_with_text' in 'azul::Dom'`** — you copied an old example that used `Dom::p` or `Dom::body`. The actual codegen surface uses the api.json names verbatim: `Dom::create_body()` / `Dom::p_with_text(...)` / `Dom::with_css(...)`.


### Building from source

Only needed if you want to track `master` or patch the library locally:

```sh
# git clone https://github.com/fschutt/azul
# cd myfolder/azul
# generate the bindings from api.json (required - emits azul.h plus
# every azul<NN>.hpp wrapper and the azul.cppm module partition under
# target/codegen/)
cargo run -p azul-doc --release -- codegen all
# build the actual DLL
cargo build -p azul-dll --release --features build-dll
```

The DLL lands at `target/release/libazul.{so,dylib}` (or `azul.dll`). The wrappers live at `target/codegen/azul<NN>.hpp`, plus `target/codegen/azul.cppm` for the C++20+ module partition. Copy the wrapper for your standard plus the DLL somewhere your C++ compiler can find them.

## Coming Up Next

- [DOM and Callbacks](../dom.md) — building richer trees, `IdOrClass`, the full callback API. Same surface as Rust, with the C++ wrappers in place of the `azul::*` types.
- [C++ Bindings](../bindings/cpp.md) — full reference for the C++ wrapper surface across language standards.
