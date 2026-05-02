---
slug: bindings/cpp
title: C++ Bindings
language: en
canonical_slug: bindings/cpp
audience: external
maturity: wip
guide_order: 330
topic_only: false
prerequisites: [hello-world, code-generation]
tracked_files:
  - api.json
  - dll/build.rs
  - doc/src/dllgen/build.rs
  - doc/src/dllgen/deploy.rs
  - doc/src/dllgen/license.rs
  - doc/src/dllgen/mod.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T20:40:52Z
---

# C++ Bindings

> **WIP** — six C++ headers (`azul03.hpp` through `azul23.hpp`) are auto-generated. The C++03 and C++11 variants are conservative wrappers; the C++17+ variants use `std::optional`, `std::variant`, and `[[nodiscard]]`. API names are stable; ergonomic helpers are still being added.

The C++ binding is a header-only wrapper over `azul.h`. One header per supported standard ships in every release archive: pick the one matching your `-std=` flag.

| header | C++ standard | language features used |
|---|---|---|
| `azul03.hpp` | C++03 | manual move via swap, no `noexcept` |
| `azul11.hpp` | C++11 | move semantics, `noexcept`, `enum class`, `std::function` |
| `azul14.hpp` | C++14 | C++11 features + `auto` return type deduction |
| `azul17.hpp` | C++17 | `std::optional`, `std::variant`, `[[nodiscard]]`, `std::string_view` |
| `azul20.hpp` | C++20 | C++17 features + `std::span` |
| `azul23.hpp` | C++23 | C++20 features + `std::expected` |

All six headers wrap the same C ABI. They differ only in which standard-library types are used in the public surface. The release pipeline emits all six in one pass (`doc/src/codegen/v2/lang_cpp/`).

## Get the artifacts

From a checkout:

```sh
cargo run --release -p azul-doc -- codegen all
cargo build -p azul-dll --release --no-default-features --features build-dll
```

The headers land at `target/codegen/azul03.hpp` … `target/codegen/azul23.hpp`. The shared library is `target/release/libazul.{so,dylib,dll}`.

For release archives: download `azul{NN}.hpp` plus the platform tarball from `azul.rs/release/<version>/`.

## Linux — g++

```sh
g++ -std=c++17 -I. hello-world.cpp -L. -lazul -Wl,-rpath,'$ORIGIN' -o hello-world
./hello-world
```

The `-std=` flag must match the header you include. Mixing `-std=c++11` with `azul17.hpp` fails to compile because `std::optional` is C++17.

## macOS — clang++

```sh
clang++ -std=c++17 -I. hello-world.cpp -L. -lazul -o hello-world
./hello-world
```

The dylib already has `@executable_path/libazul.dylib` as its install name, so a sibling `libazul.dylib` is found at run time without `DYLD_LIBRARY_PATH`.

## Windows — MinGW

```sh
g++ -std=c++17 -I. hello-world.cpp -L. -lazul -o hello-world.exe
hello-world.exe
```

Drop `azul.dll` next to the `.exe`.

## Windows — MSVC

Link against the import library, not the static archive:

```bat
cl /std:c++17 /I. /EHsc hello-world.cpp azul.dll.lib /Fehello-world.exe
```

## Picking a C++ version

Newer headers track the C ABI symbol-for-symbol but expose more ergonomic types. Practical guidance:

- **C++17** is the recommended default. `std::optional<T>` for nullable returns, `std::variant<…>` for tagged unions, `[[nodiscard]]` on builders. The example file is `examples/cpp/cpp17/hello-world.cpp`.
- **C++11** drops `std::optional`/`std::variant` and uses raw enums with payload accessors. Use this when toolchain restrictions preclude C++17.
- **C++03** uses RAII via destructors only; no move semantics, no `noexcept`. Reserved for legacy embedded toolchains.
- **C++20 / C++23** add `std::span` for slice arguments and `std::expected` for `Result`-style returns. Useful when the calling code already uses those types.

The release pipeline ships all six in every archive so you can pick the highest standard your project supports.

## Header conventions

Each header opens with this pattern:

```cpp,ignore
namespace azul {
    using Dom = AzDom;       // alias
    class String { /* RAII over AzString */ };
    class App { /* RAII over AzApp */ };
    // ...
}
```

C-ABI types remain accessible under their `Az` prefix. Wrapper classes manage the lifecycle (constructor calls `Az<Type>_create`, destructor calls `Az<Type>_delete`). `release()` extracts the underlying C struct without running the destructor — needed when handing a value back across the C-ABI boundary, e.g. returning from a layout callback:

```cpp,ignore
AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    azul::Dom body = azul::Dom::create_body();
    return body.style(azul::Css::empty()).release();
}
```

The `AZ_REFLECT(MyType)` macro (defined per-header) generates the boilerplate for `RefAny` reflection: `MyType_upcast`, `MyType_downcast_ref`, `MyType_downcast_mut`, JSON conversion. See [Hello, World — C++](../hello-world/cpp.md) for the full pattern.

## Mixing C and C++ in one TU

A `.cpp` file can include `azul.h` and the corresponding `azulNN.hpp`. The C header is `extern "C"`-guarded internally; the C++ wrapper sits on top and shares the same ABI. This is how callbacks declared in the layout function (which must use C signatures for FFI) coexist with C++ types in the rest of the file.

```cpp,ignore
#include "azul17.hpp"  // pulls in azul.h transparently
```

A separate `extern "C"` declaration is not required.

## Example layout

```
my-app/
├── azul17.hpp          ← copy from target/codegen/
├── libazul.so          ← copy from target/release/
└── hello-world.cpp
```

`azul.h` is implicitly included from the wrapper header. You only need the dialect-specific header file plus the shared library.

## Reading the example program

The hello-world walkthrough for C++17 is at [Hello, World — C++](../hello-world/cpp.md). The repository includes one example per C++ standard under `examples/cpp/cpp03/` … `examples/cpp/cpp23/`; pick the one matching your `-std=` flag. They are referenced from `api.json`'s `examples[].code.cpp03` … `cpp23` fields.

## Next

- [Hello, World — C++](../hello-world/cpp.md) — full program walkthrough (C++17).
- [C Bindings](c.md) — the underlying ABI.
- [Code Generation](../code-generation.md) — how the six headers are produced.
