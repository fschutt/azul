# C++ Codegen Modernization Plan

Goal: each `azul<NN>.hpp` should actually deliver the modern features its
standard makes available, not inherit the C++11 baseline. The headers are
auto-generated from `api.json` via the IR in `doc/src/codegen/v2/ir.rs`;
bumping their shape is free.

This plan is **type-driven**: every phase dispatches on a `TypeCategory`, on
an IR shape (`callback_wrapper_info`, `EnumVariantKind`, `FieldRefKind`), or
on a `method_name` text pattern that covers a whole category. **No
hard-coded `if class_name == "Dom"` branches anywhere in the codegen** —
they're brittle to renames and force per-class drift.

Forward-only: no backwards-compat shims, no "macro stays as a no-op", no
transitional flags. C ABI is frozen — wrapper-side changes only.

## Audit (what we have today)

Counts ripped from `target/codegen/azul<NN>.hpp` on 2026-05-02 (pre-plan):

| Feature                                            | C++03 | C++11 | C++17 | C++20 | C++23 |
|----------------------------------------------------|-------|-------|-------|-------|-------|
| `noexcept`                                         |   0   | 1971  | 1971  | 1971  | 1971  |
| `[[nodiscard]]`                                    |   0   |   0   |  635  |  635  |  635  |
| `std::optional` (Option→optional)                  |   0   |   0   |   1   |   1   |   1   |
| `std::span` (Vec→span methods)                     |   0   |   0   |   0   |  13   |  13   |
| `std::expected` (Result→expected)                  |   0   |   0   |   0   |   0   | 1*    |
| `template` / `concept` / `module` / `requires`     |   0   |   0   |   0   |   0   |   0   |

`*` Just a `// TODO` comment in `cpp20.rs:591-593`. The IR already carries
the Ok/Err payload types as `EnumVariantKind::Tuple` on the sibling enum —
the comment claiming otherwise is wrong, this is a lookup, not new IR work.

`cpp14.rs` does not exist; `get_generator(Cpp14)` aliases to `Cpp11Generator`
(`mod.rs:215`). `Cpp23Generator` is a thin wrapper on `Cpp20Generator`. The
`AZ_REFLECT(structName)` macro is the only way to register a user type today
in C++11+ as well as C++03.

## Foundation (already landed)

These commits set up the rest of the plan and are not redone in any phase
below.

- **C++11+ template reflection** (`2fdd140cd`). `azul::upcast<T>`,
  `azul::downcast_ref<T>`, `azul::downcast_mut<T>`, `azul::type_id<T>()`
  emitted inside `namespace azul`. C++14+ also gets `azul::type_id_v<T>`.
  Lives in `common.rs::generate_template_reflection`, called from cpp11/17/20
  generators.
- **Three pre-existing codegen blockers fixed** (`17e2bdcb8`):
  1. `type_has_wrapper` diverged from `should_skip_class` — `Recursive` and
     `DestructorOrClone` were skipped from emission but still claimed to
     have wrappers. Now both predicates agree.
  2. Wrapper class constructors were unconditionally `explicit`, breaking
     methods that returned a Copy wrapper from a C-returning function (e.g.
     `Void Void::sleep_ms`). `explicit` now applied to non-Copy types only.
  3. `should_substitute_callbacks` only matched `FunctionKind::Method`,
     missing `MethodMut` and `StaticMethod`. Setters like
     `NodeData::set_merge_callback` (a `MethodMut`) didn't substitute the
     callback wrapper for its raw fn-pointer typedef. Now all four kinds
     substitute.

## Examples are the spec

The six `examples/cpp/cpp<NN>/hello-world.cpp` files (commit `1ef4232eb`)
are the per-standard target API. They drive what the codegen needs to emit.
Callback signatures stay on raw `Az*` types because the framework dispatches
through C function pointers — the wrappers are adopted at the top of each
callback body via `RefAny data_wrapper(data);`.

Wrapper-typed callbacks (`Update on_click(RefAny, CallbackInfo)`) need a
per-call-site templated extern "C" trampoline; that's deferred indefinitely
unless someone designs a clean implementation.

## Explicitly dropped from earlier drafts

- **No `Dom::body()` alias** for `Dom::create_body()`. The earlier draft
  hand-rolled this in the codegen; the user pushed back. Examples write
  whatever the codegen naturally emits from api.json.
- **No runtime CSS-parsing `with_inline_style` helper.** It's just a
  normal `with_*` method that takes a `String` — same as any other
  `String`-taking method, picked up by the string_view phase below.
- **No "alias map driven from api.json metadata".** Renames go in the
  api.json key directly if needed.
- **No `if class_name == "Dom"` branches** in the codegen. Every dispatch
  in this plan is on `TypeCategory`, on `callback_wrapper_info.is_some()`,
  on `FieldRefKind`, on the `EnumVariantKind` shape of a sibling enum, or
  on a `method_name` text pattern.

## Phases

Each phase ends green via:

```sh
cargo run -p azul-doc --release -- codegen all
bash scripts/build_cpp_examples.sh
```

### Phase 0 — Promote `Option` and `Result` to first-class `TypeCategory`s

**Trigger:** Adds `TypeCategory::Option` and `TypeCategory::Result` to
`ir.rs:721`. Detection in `ir_builder.rs::classify_struct_type` (~line
2132): a struct is `Option` if `name.starts_with("Option")` AND a sibling
enum of the same name has `Some`/`None` variants; `Result` if
`name.starts_with("Result")` AND sibling enum has `Ok`/`Err` variants. The
prefix is the cheap pre-filter; the variant names are the real signal.

**Codegen slot:** Replace `is_option_type` / `is_result_type` in
`common.rs:227-234` (today both are name-prefix checks — already a soft
class-name hack) with `matches!(struct_def.category, TypeCategory::Option)`
etc. All four generators already call those helpers — no other touch needed.

**Why first:** Every later phase that touches Option/Result depends on
this dispatch.

### Phase 1 — Confirm template reflection placement (already landed)

Already in `common.rs::generate_template_reflection`, gated on
`standard.has_move_semantics()`. Called from cpp11/17/20 generators inside
`namespace azul` after `class RefAny` declares. No change.

### Phase 2 — `std::string_view` overloads, category-driven

**Trigger:** Two distinct insertion points, both type-driven:

(a) The `String` wrapper itself (`category == TypeCategory::String`) gets
its `(std::string_view)` constructor in `generate_string_methods` for
C++17+. Already there — confirm and keep.

(b) Every wrapper method whose argument list contains a `String`-typed
`FunctionArg` gets a sibling overload that takes `std::string_view` and
forwards by constructing `String(sv.data(), sv.size())`. The detection is
`arg.type_name == "String"` in `generate_args_signature_ex` at
`common.rs:471`. Skip when `should_substitute_callbacks(func)` is false
(trait-generated functions don't get sv overloads). This naturally picks up
`Dom::with_inline_style`, `Dom::p_with_text`, `Css::parse`, `App::*`, etc.
— no class-name hacks; the `String` argument shape is the trigger.

**Codegen slot:** New helper `generate_method_declaration_with_sv_overload`
invoked from `generate_method_declarations` and
`generate_constructor_declarations` in cpp17.rs:534-619. C++14/C++11
generators do not emit the sv overload (no `<string_view>`). Implementation
is one-line forward.

### Phase 3 — `std::optional` and `std::expected` from `Option` / `Result`

**Trigger:** `category == TypeCategory::Option` →
`toStdOptional()` and `operator std::optional<Inner>()` (Option already has
`toStdOptional`; keep). `category == TypeCategory::Result` →
`toStdExpected() &&` and `operator std::expected<Ok, Err>() &&`, plus
`and_then(F)` / `or_else(F)` member templates that delegate to
`std::expected`'s.

The Ok/Err types come from looking up the sibling `EnumDef` (matching by
name) in `ir.enums`, finding variants `Ok` and `Err`, and reading the
single tuple element of each `EnumVariantKind::Tuple`. Generic for every
`Result*` enum.

**Codegen slot:**
- `cpp17.rs::generate_option_methods` — keep.
- `cpp17.rs::generate_result_methods` (lines 502-520) only emits
  `isOk`/`isErr` today; extend it. Factor the sibling-enum lookup into
  `common.rs::get_result_payload_types(struct_def, ir) -> Option<(String, String)>`
  so cpp20/cpp23 can call it.
- `cpp20.rs::generate_result_methods` and the C++23 path at
  `cpp20.rs:582-604`: emit the real `std::expected<Ok, Err> toStdExpected() &&`
  branching on the tag, plus `operator std::expected<...>()`. C++23
  additionally emits `and_then` / `or_else` member templates. The `// TODO`
  at `cpp20.rs:601-603` deletes.

**Example change:** C++23 example reads `std::expected<Css, CssParseError>
sheet = Css::parse(R"(...)")` directly — only the comment "Result<Ok, Err>
converts implicitly" becomes accurate.

### Phase 4 — Structured bindings on every `Result`

**Trigger:** Every `category == TypeCategory::Result` struct.

**Codegen slot:** New `common.rs::generate_structured_binding_specs(ir)`
called from cpp17+ generators **after** the closing `} // namespace azul`,
because the specializations live in `namespace std`:

```cpp
template<> struct std::tuple_size<azul::ResultFooBar> : std::integral_constant<size_t, 2> {};
template<> struct std::tuple_element<0, azul::ResultFooBar> { using type = std::optional<Foo>; };
template<> struct std::tuple_element<1, azul::ResultFooBar> { using type = std::optional<Bar>; };
template<size_t I> auto get(azul::ResultFooBar&& r);
```

Enables `auto [ok, err] = std::move(result);` for any `Result*` wrapper
without per-class hacks.

### Phase 5 — `azul::ReflectableModel` concept (C++20+)

**Trigger:** Template reflection consumers in
`common.rs::generate_template_reflection`. Concept is purely structural:
`std::is_object_v<T> && std::is_destructible_v<T> && !std::is_same_v<T, RefAny>`.
No per-class enumeration.

**Codegen slot:** Make `generate_template_reflection` standard-aware (it
already takes `CppStandard`): when `standard >= Cpp20`, emit
`template<class T> concept ReflectableModel = …;` at the top of the block,
then change every `template<class T>` to `template<ReflectableModel T>`
in `upcast` / `downcast_ref` / `downcast_mut` / `type_id`. Pre-C++20 path
unchanged.

### Phase 6 — `azul.cppm` module partition (C++20+)

**Trigger:** Sibling-file emission, not per-type.

**Codegen slot:** New method `generate_module_partition(ir, config) ->
String` on `Cpp20Generator` / `Cpp23Generator`. Writes one extra file
alongside `azul20.hpp` / `azul23.hpp`:

```cpp
module;
#include "azul20.hpp"
export module azul;
export namespace azul { /* re-export every wrapper class name from ir.structs */ }
```

The class-name list comes from iterating `ir.structs` and filtering with
`should_skip_class` and `renders_as_type_alias` — same predicates as
forward declarations, same loop. Call site: extend `CppDialect::generate`
return shape (or add a sibling method `generate_module(...)`) so
`generate_all_cpp_headers` collects the extra file.

The `__has_include(<azul.cppm>) ? import azul : #include "azul20.hpp"`
shape in the example covers both module-aware and pre-modules toolchains.

### Phase 7 — Designated-init eligible POD audit

**Trigger:** A new predicate, *not* a TypeCategory variant:

```rust
fn is_designated_init_eligible(s: &StructDef) -> bool {
    s.traits.is_copy
      && !needs_destructor(s)
      && s.fields.iter().all(|f|
          f.ref_kind == FieldRefKind::Owned
          && (is_primitive(&f.type_name) || enum_or_pod_field(&f.type_name, ir)))
}
```

Catches `WindowState`, `LayoutSize`, `LogicalPosition`, `AppConfig`-shaped
shells. Rejects `Dom`, `Css`, `Button` (heap-managed fields). No
class-name match.

**Codegen slot:** Largely a documentation + verification phase. The Bug 2
fix (non-`explicit` ctor on Copy types) already lets users write
`AzLogicalSize{ .width = 400, .height = 300 }` and have it convert to the
wrapper. So in practice this phase audits the C struct layout for each
predicate hit and confirms the example compiles. If any candidate proves
non-POD, the predicate is wrong; fix the predicate, don't fake the
example.

### Phase 8 — Deducing-`this` on builder methods (C++23)

**Trigger:** `func_has_self(func) && matches!(func.kind, Method | MethodMut)
&& (method_name.starts_with("with_") || method_name.contains("_with_"))`.
Catches `with_child`, `with_inline_style`, `with_button_type`,
`with_on_click`, `th_with_scope`, … across every wrapper class. No
class-name dispatch. Static factories like `p_with_text` are excluded by
the `func_has_self` gate even though their name pattern matches.

**Codegen slot:** New `cpp23.rs` (split from `cpp20.rs`). In the
equivalent of `generate_method_implementations_shared` (`cpp20.rs:734`):
when the rule holds, emit one
`template<class Self> auto with_xxx(this Self&& self, /* args */)` instead
of the current `&`-qualified form. Body forwards `std::forward<Self>(self).inner_`
into the C call and constructs the wrapper from the result.

The user-visible win is chaining on r-values without temporaries-of-temporaries
warnings; collapses no overloads today (we only have `&` forms) but
positions us cleanly when we want both.

### Phase 9 — Drop `AZ_REFLECT` for C++11+; wire `cpp14` and `cpp23`

**Trigger:** `!standard.has_move_semantics()` for AZ_REFLECT gating —
template reflection has fully replaced the macro for C++11+.

**Codegen slot:**
- Gate `generate_reflect_macro` calls in `cpp11.rs:40`, `cpp17.rs:39`,
  `cpp20.rs:49` on `!standard.has_move_semantics()`. C++03 keeps the
  macro path in `cpp03.rs:40` (no template reflection there).
- Wire `Cpp14Generator` and `Cpp23Generator` into
  `mod.rs::get_generator` (drop the Cpp14→Cpp11 alias and the
  Cpp23→Cpp20 thin wrapper). Each gets its own struct + impl block;
  bodies can still delegate to the previous standard's helpers for
  shared logic.

**Verification:** Full sweep — `codegen all && build_cpp_examples.sh`
green for all six standards.

## Files involved

- `doc/src/codegen/v2/ir.rs` — `TypeCategory::Option` / `TypeCategory::Result`.
- `doc/src/codegen/v2/ir_builder.rs` — `classify_struct_type` extension.
- `doc/src/codegen/v2/lang_cpp/common.rs` — string_view overload helper,
  `get_result_payload_types`, structured-binding spec emission,
  `generate_template_reflection` becomes standard-aware (concept gate).
- `doc/src/codegen/v2/lang_cpp/cpp11.rs` — drop AZ_REFLECT path.
- `doc/src/codegen/v2/lang_cpp/cpp14.rs` — **new file**.
- `doc/src/codegen/v2/lang_cpp/cpp17.rs` — string_view overloads,
  Result→optional sibling pairs, structured-binding emission.
- `doc/src/codegen/v2/lang_cpp/cpp20.rs` — concept gate, module partition,
  real `std::expected` payload conversion.
- `doc/src/codegen/v2/lang_cpp/cpp23.rs` — **new file** — deducing-this on
  builder methods.
- `doc/src/codegen/v2/lang_cpp/mod.rs` — wire new generators, drop aliases.
- `examples/cpp/cpp<NN>/hello-world.cpp` — already aligned (commit `1ef4232eb`).
  Adjust if a phase reveals a better shape.
- `doc/guide/en/hello-world/cpp.md` — re-advertise the now-real features
  once codegen and examples are green (final phase).

## Out of scope

- C ABI changes (none needed; everything is wrapper-side).
- Rust- and Python-side codegen.
- Reworking `AzRefAny_*` C functions. The wrappers continue to call them
  under the hood from template instantiations.
- Wrapper-typed callbacks (`Update on_click(RefAny, CallbackInfo)`).
  Deferred indefinitely until someone designs a clean
  per-call-site templated extern "C" trampoline that doesn't need
  `static thread_local` state. Examples keep raw-`Az*` callback signatures.
