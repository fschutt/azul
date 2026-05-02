# C++ Codegen Modernization Plan

Goal: each `azul<NN>.hpp` should actually deliver the modern features its
standard makes available, not inherit the C++11 baseline. We define the
*target API* per standard first, rewrite `examples/cpp/cpp<NN>/` to use
that API, then fix up the codegen until the examples compile and link.

This plan is forward-only: define the API we want, then make it real.
No backwards-compat shims, no "macro stays as a no-op", no transitional
flags. The headers are auto-generated from `api.json`; bumping their
shape is free.

## Audit (what we have today)

Counts ripped from `target/codegen/azul<NN>.hpp` on 2026-05-02:

| Feature                              | C++03 | C++11 | C++17 | C++20 | C++23 |
|--------------------------------------|-------|-------|-------|-------|-------|
| `noexcept`                           |   0   | 1971  | 1971  | 1971  | 1971  |
| `[[nodiscard]]`                      |   0   |   0   |  635  |  635  |  635  |
| `std::optional` (Option→optional)    |   0   |   0   |   1   |   1   |   1   |
| `std::span` (Vec→span methods)       |   0   |   0   |   0   |  13   |  13   |
| `std::expected`                      |   0   |   0   |   0   |   0   | 1*    |
| `template` / `concept` / `module` / `requires` | 0 | 0 | 0 | 0 | 0   |

`*` Just a `// TODO` comment in `cpp20.rs:591–593`.

`cpp14.rs` does not exist; `get_generator(Cpp14)` aliases to
`Cpp11Generator` (`mod.rs:215`). `Cpp23Generator` is a thin wrapper on
`Cpp20Generator`. The `AZ_REFLECT(structName);` macro is the only way
to register a user type today, even in C++23.

## Target API per standard

### C++03 — macro reflection, manual moves

```cpp
struct MyDataModel { uint32_t counter; };

void MyDataModel_destructor(void* m) { }
AZ_REFLECT(MyDataModel, MyDataModel_destructor);  // macro, like in C

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    MyDataModelRef d = MyDataModelRef_create(&data);
    if (!MyDataModel_downcastRef(&data, &d)) return AzDom_createBody();
    // ...read d.ptr->counter...
    MyDataModelRef_delete(&d);
    // ...
}
```

C++03 has no templates that can hold a per-type `static const uint64_t`
without ODR pain. Stay on the macro, stay on Colvin-Gibbons move
emulation, stay on raw `Az*` types throughout. C++03 is essentially
"C with classes" for our purposes.

### C++11 — function-template reflection, RAII wrappers

```cpp
struct MyDataModel { uint32_t counter; };
// no AZ_REFLECT line.

azul::Update on_click(azul::RefAny data, azul::CallbackInfo info);

azul::Dom layout(azul::RefAny data, azul::LayoutCallbackInfo info) {
    auto* d = azul::downcast_ref<MyDataModel>(data);
    if (!d) return azul::Dom::body();

    return azul::Dom::body()
        .with_child(azul::Dom::p_with_text(std::to_string(d->counter))
            .with_inline_style("font-size: 50px;"))
        .with_child(azul::Button::create("Increase counter")
            .with_button_type(azul::ButtonType::Primary)
            .with_on_click(data.clone(), on_click)
            .dom())
        .style(azul::Css::empty());
}
```

What's new vs. today:

- **No `AZ_REFLECT` macro.** Reflection lives in `azul::upcast<T>`,
  `azul::downcast_ref<T>`, `azul::downcast_mut<T>` function templates.
  Per-type ID derived from the address of a function-local
  `static const uint64_t`.
- **Wrapper types in the public surface.** `azul::Dom`, `azul::Button`,
  `azul::RefAny`, `azul::CallbackInfo`, `azul::LayoutCallbackInfo`.
  Users do not see `Az*` types in their function signatures any more
  (the codegen emits the C-typed callback trampoline behind the scenes).
- **No `.release()` ceremony.** Returning a wrapper from a layout
  callback transparently transfers ownership. The trampoline takes
  care of releasing.
- **String literals just work.** `with_inline_style("…")` — no
  `String(...)` constructor. Implemented as an `(const char*, size_t)`
  overload.
- **`noexcept` everywhere it's safe.** Already true today; keep it.

### C++14 — `auto` deduction, generic lambdas, variable templates

Same as C++11, plus:

```cpp
auto d = azul::downcast_ref<MyDataModel>(data);    // const MyDataModel*
auto button = azul::Button::create("Click me")     // azul::Button
    .with_on_click(data.clone(), [](auto data, auto info) {
        // generic lambda - codegen wraps it in an extern "C" trampoline
        // through std::function adaption.
        return azul::Update::RefreshDom;
    });

constexpr auto kModelId = azul::type_id_v<MyDataModel>; // variable template
```

What's new vs. C++11:

- **Inline lambdas as callbacks.** Codegen emits a `with_on_click`
  overload that accepts any callable and synthesizes the
  `extern "C"` trampoline via a small `std::function` indirection.
- **Variable template `azul::type_id_v<T>`** in addition to the
  function template `azul::type_id<T>()`.
- **`auto` return types** on accessors with noisy spellings
  (`Vec<T>::span()`, `Option<T>::unwrap()`, etc.).

### C++17 — `string_view`, structured bindings, `if constexpr`

Same as C++14, plus:

```cpp
auto label = azul::Dom::p_with_text(std::string_view{"5"})
    .with_inline_style(std::string_view{"font-size: 50px;"});

// Result<Ok, Err> destructures via structured bindings:
auto result = azul::SomeFallibleCall();
if (auto [ok, err] = std::move(result); ok) {
    // ...use ok...
}

// Or via std::optional - already there:
if (std::optional<MyType> v = some_option.toStdOptional()) {
    // ...use *v...
}
```

What's new vs. C++14:

- **`std::string_view` overloads** for every `String`-taking function
  in the wrapper API. Implemented as
  `String(sv.data(), sv.size())` internally.
- **Structured bindings** for `Result<Ok, Err>`. Codegen emits
  `tuple_size<Result>`, `tuple_element<I, Result>`, `get<I>(Result&&)`
  specializations so `auto [ok, err] = result;` works.
- **`if constexpr`** inside the reflection templates to skip the heap
  `delete` for trivially destructible `T`.
- **`[[nodiscard]]`** on factory methods. Already there; keep it.

### C++20 — concepts, modules, designated initializers

Same as C++17, plus:

```cpp
import azul;  // module-aware toolchains

template<azul::ReflectableModel T>
class MyView {
    azul::RefAny data_;
public:
    explicit MyView(T model) : data_(azul::upcast(std::move(model))) {}
};

// Designated initializers for plain-data option types:
auto window = azul::WindowCreateOptions::create(layout);
window.window_state.title = "Hello";
window.window_state.size = { .width = 400.0, .height = 300.0 };
```

What's new vs. C++17:

- **`azul::ReflectableModel`** concept constrains the reflection
  templates; users get readable `requires`-clause errors instead of
  template-instantiation walls of text.
- **Module partition `azul.cppm`** (or sibling `azul20.ixx` on MSVC)
  next to `azul20.hpp`, providing `export module azul;` for toolchains
  that have C++20 modules. The `.hpp` keeps working as a fallback.
- **`std::span<T>`** for Vec types. Already there; keep it.
- **Designated initializers** in the in-header builder helpers for
  POD-like option structs (window state, app config, etc.).
- **`consteval`** for compile-time type-id derivation if we land a
  non-RTTI name extraction (`__PRETTY_FUNCTION__` slicing).

### C++23 — real `std::expected`, deducing `this`, `import std;`

Same as C++20, plus:

```cpp
import std;
import azul;

// Direct std::expected return:
std::expected<azul::Dom, azul::ParseError> dom = azul::xml::parse(src);
auto styled = std::move(dom)
    .and_then([](azul::Dom d) { return d.style(my_css); })
    .or_else([](azul::ParseError e) { return fallback_dom(e); });

// Deducing-this in builder methods - one function instead of two
// (& and && overloads). Lets users chain on r-values and l-values
// uniformly:
azul::Dom d = some_dom.with_inline_style("font-size: 50px;")
                      .with_child(child);
```

What's new vs. C++20:

- **Real `std::expected<Ok, Err>`** for every codegen-emitted
  `Result<Ok, Err>`. The IR already carries the variant payloads —
  emit a `toStdExpected() &&` member plus an implicit conversion
  operator, plus `and_then` / `or_else` member chains on the wrapper
  itself. Replaces the placeholder comment in `cpp20.rs:591–593`.
- **Deducing-`this`** on every builder method (`with_*`). Collapses
  the `const &` and `&&` overload pair to a single template function:
  `template<class Self> auto with_inline_style(this Self&& self, String s);`.
  Halves the size of the wrapper class declarations in the header.
- **`import std;`** support in the module partition.
- **`if consteval`** wherever it simplifies templated paths.

## Implementation order

Each phase ends green via:

```sh
cargo run -p azul-doc --release -- codegen all
bash scripts/build_cpp_examples.sh
```

1. **Rewrite the examples first.** For each `examples/cpp/cpp<NN>/`,
   rewrite `hello-world.cpp` as the target API spelled out above.
   These will not compile yet — that is the point. The example is the
   spec.
2. **C++11 codegen**: emit `azul::upcast<T>` / `azul::downcast_ref<T>` /
   `azul::downcast_mut<T>` function templates in `common.rs`, gated on
   `has_move_semantics()`. Promote raw `Az*` callback types in the
   public wrappers to `azul::*` types with C-typed trampolines under
   the hood. Add `(const char*, size_t)` overloads to all
   `String`-taking functions.
3. **Split `cpp14.rs`** out of the `Cpp11Generator` alias. Add `auto`
   return types, generic-lambda callback overloads, variable templates.
4. **`cpp17.rs`**: add `std::string_view` overloads. Emit
   `tuple_size`/`tuple_element`/`get` specializations for every
   `Result<Ok,Err>`. Replace the `T*` `delete` in reflection
   templates with an `if constexpr`-gated branch.
5. **`cpp20.rs`**: introduce `azul::ReflectableModel` concept and
   constrain the reflection templates. Emit a sibling `azul.cppm`
   module partition file. Add designated-initializer-friendly
   in-header helpers.
6. **Split `cpp23.rs`** out of `Cpp20Generator`. Implement the real
   `std::expected` payload conversion (the IR already has Ok/Err
   types — `cpp20.rs:592` saying otherwise is wrong). Switch every
   `.with_*` method to deducing-`this`.
7. **`scripts/build_cpp_examples.sh` is green for every standard.**

## Files involved

- `doc/src/codegen/v2/lang_cpp/common.rs` — reflection templates,
  `string_view` overloads, structured-binding adapters.
- `doc/src/codegen/v2/lang_cpp/cpp11.rs` — template reflection emission.
- `doc/src/codegen/v2/lang_cpp/cpp14.rs` — **new file**.
- `doc/src/codegen/v2/lang_cpp/cpp17.rs` — string_view, structured
  bindings, `if constexpr`.
- `doc/src/codegen/v2/lang_cpp/cpp20.rs` — concepts, module partition,
  designated initializers.
- `doc/src/codegen/v2/lang_cpp/cpp23.rs` — **new file** — real
  `std::expected`, deducing-`this`.
- `doc/src/codegen/v2/lang_cpp/mod.rs` — wire new generators, drop
  aliases.
- `examples/cpp/cpp03/hello-world.cpp` — keep macro, rewrite to spec.
- `examples/cpp/cpp11/hello-world.cpp` — template reflection, wrappers.
- `examples/cpp/cpp14/hello-world.cpp` — generic lambda + `auto`.
- `examples/cpp/cpp17/hello-world.cpp` — `string_view` and structured
  binding.
- `examples/cpp/cpp20/hello-world.cpp` — `import azul;` + concept.
- `examples/cpp/cpp23/hello-world.cpp` — `std::expected`, deducing-`this`.
- `doc/guide/en/hello-world/cpp.md` — re-advertise the now-real
  features once codegen and examples are green.

## Out of scope

- C ABI changes (none needed; everything is wrapper-side).
- Rust- and Python-side codegen.
- Reworking `AzRefAny_*` C functions. The wrappers continue to call
  them under the hood from template instantiations.
