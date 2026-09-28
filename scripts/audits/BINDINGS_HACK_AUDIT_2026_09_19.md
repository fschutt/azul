# Bindings hack audit: shipped-language codegen

**Date:** 2026-09-19. **Branch:** `fix/docs-update-human` (HEAD 70fe66c4a). The audit was read-only: no files edited, no commits, no cargo or builds.

**Scope:**
- The shipped tier from `SHIPPED_LANGUAGES` (doc/src/docgen/mod.rs:151): c, cpp, rust, csharp, java, scala, kotlin, lua, ruby, node, ocaml, zig, go, fortran, haskell, python, d, crystal, swift.
- The shared codegen infrastructure (doc/src/codegen/v2/{generator, ir, ir_builder, managed_host_invoker, managed_lang_helpers, transmute_helpers, module_plan, config, mod}.rs).
- Pascal and every non-shipped language are excluded.
- Two follow-up asks from the user during the audit are also covered:
  - Derive coverage: do Debug, PartialEq, PartialOrd, Ord, Hash, Clone and Default reach each language through the C functions?
  - Module misclassification: are api.json types filed under the right module?

**Severity tags**

| Tag | Meaning |
|---|---|
| `HELLO-WORLD-ONLY` | Exists to make the hello-world work or read nicely |
| `NAME-KEYED SPECIAL CASE` | Behaviour decided by a literal API name or a hand-kept name list |
| `SILENT SKIP` | API surface dropped or degraded without a diagnostic |
| `MINOR` | Cosmetic, latent, or a duplicated predicate |

**CERTAIN** means the code was read and the name-keying or skip is unambiguous. **SUSPECT** means the maintainer has to judge. Most SUSPECT items are structurally derived conveniences whose only current match is the hello-world's entry point.

## Caveats

- **Other sessions were editing emitters during the audit.** Uncommitted changes exist in lang_d, lang_fortran, lang_haskell and lang_java/managed.rs, plus examples/{d,fortran,haskell} (and pascal, which is out of scope).
  - Findings marked `[WIP]` exist only in those uncommitted edits.
  - Line numbers in those directories refer to the working tree at about 15:40. Where the Haskell and HEAD lines differ, both are given.
- **`target/codegen` was wiped and regenerated three or four times** by a concurrent `codegen all` (about 15:28, 15:32 and 15:35). Every output number was re-taken from one consistent regeneration (15:35-15:36). Spot-checked output line numbers were unchanged across regenerations.
- **Corrections to figures quoted in this audit's own working notes:**
  - `HOST_INVOKER_KINDS` has **62** entries, not 63. A regex count also matched `"C"` in an `extern "C"` comment.
  - The azul.h `_clone` count is **1,114** Clone functions, not 1,115. `AzBoxDecorationBreak_clone(void)` is the constructor for the enum variant `BoxDecorationBreak::Clone` (see S14).

## Executive summary

**Almost no emitter branches on a hello-world API name.** There is no `if name == "Button"` / `"Dom"` / `"App"` / `"WindowCreateOptions"` anywhere in the shipped emitters. What we found instead are four kinds of problem.

**1. Hand-kept name lists in the shared IR.** They decide type categories and callback support, and every emitter inherits them. The most damaging:
- `TypeCategory::Vec` = {U8Vec, StringVec, GLuintVec, GLintVec} plus the misfiled InstantPtr and StringMenuItem, **out of 127 Vec types** (S1, S2). Category-gated Vec helpers therefore reach only those 4 Vecs (3 in Zig) in C, C++, Java, C#, Kotlin, Ruby, Lua and Zig. They also emit broken methods on InstantPtr (C, Lua, Ruby).
- The 62-name `HOST_INVOKER_KINDS` (S5) leaves **15 callback kinds (Timer, WriteBack, RenderImage, IconResolver, DbMerge, ...) unusable or broken** in most managed bindings.

**2. One api.json shape the bindings only worked around for the hello-world (S17).** Seven functions take a raw callback typedef, so a managed closure loses its ctx at the C boundary. Every managed binding carries a structurally derived `WindowCreateOptions.create(layoutFn)` factory to get around this for the hello-world's entry point. The generic `Dom.with_callback` / `Dom.add_callback` / `NodeData.add_callback` did not get that treatment:
- **Lua:** the closure silently never fires.
- **Ruby:** the call raises.
- **Node:** throws a TypeError.
- **Haskell:** the functions are dropped.
- **Go, Fortran, Java, C#, Kotlin, OCaml:** raw function pointers only.

**3. A few genuinely name-keyed special cases:**
- **C#:** `func.c_name == "AzApp_run"`.
- **OCaml:** helpers keyed on `"with_child"`, `"with_css"`, `create_*_with_text` and a method named `"dom"`. They produce exactly the hello-world's `Dom.p ~css` / `Dom.body ~children` / `Button.create → dom`.
- **Go:** smart setters keyed on the prefix `set_on_`, and a hand-typed `Log(AppLogLevel, *String)` interface.
- **Fortran:** a Makefile hard-coded to `EXE := hello_world`.
- **Python:** several contradictory name lists, which drop about 14% of methods and all bytes/string-list APIs.
- **Rust:** a hand-curated prelude with 4 stale names.
- **Lua:** a `"ThreadCallback"` literal.
- **Ruby:** an invented `create_with_layout` name that the hello-world calls.
- **Kotlin and Java:** a zero-argument `WindowCreateOptions.create()` added so the hello-world compiles.

**4. Silent skips and real bugs found along the way:**
- Node calls a nonexistent `lib.Az{T}_deepCopy` (221 names, 767 sites).
- OCaml lays out all 196 type aliases as 8-byte opaque pointers (280 ABI-mismatched bindings).
- Python threads never run (stub trampoline).
- C++ exposes `delete_()`, which double-frees (911 classes).
- Fortran has no `assignment(=)`, so copies double-free.
- Swift drops 1,347 functions on natively mapped types and filters them out of its own diagnostic list.
- 14 of the 19 shipped bindings drop all 1,436 api.json constants. Only C, and through azul.h C++ and Swift, plus Zig carry them (S18).

**Derives:**
- The FFI layer of every language declares nearly every derive function.
- All seven capabilities surface idiomatically in Rust, Python, Go, D, Crystal, Swift, OCaml and Zig (in C they are the plain functions).
- C++ and Fortran expose all seven only as named methods: no operators, and in Fortran only on struct classes.
- C#, Java/Scala, Kotlin, Lua, Ruby, Node and Haskell never surface ordering (`_partialCmp`/`_cmp`). Lua, Node and Haskell also never surface `_hash`.
- Java, Kotlin, C# and Ruby pair C-backed equality with identity hashing on about 260 classes, which breaks the equals/hash contract.
- Shared defect: 122 generic aliases get no derive functions at all, because alias trait inheritance is not implemented (S15).

**Modules:** the autofix module classifier (doc/src/autofix/module_map.rs) matches the longest keyword substring, and that splits families:
- Widget `*Callback` wrappers are in `dom`, their `*CallbackType` in `callbacks`, and the widget in `widgets`.
- 118 `*VecSlice` types are spread over 24 modules.
- CSS parse errors are filed under `xml`.
- `WindowPosition`, `StyledDom` and `EventFilter` are filed under `css`.

## Top findings, ranked

| # | Finding | Where | Severity |
|---|---|---|---|
| 1 | `TypeCategory::Vec` comes from a 4-name list (api.json never sets `vec_element_type`). 4 of 127 Vecs get the category-gated helpers. InstantPtr/StringMenuItem are misfiled as Vec and get broken helpers (C `AzInstantPtr_empty` does not compile; Lua/Ruby `to_a` raise). | ir_builder.rs:2256, 2264-2281, 2350, 2379 | NAME-KEYED + SILENT SKIP (all langs gating on it) |
| 2 | Raw-typedef callback args (`WindowCreateOptions.create`, `Dom.with_callback`/`add_callback`, `NodeData.add_callback`, `StringMenuItem.with_callback`, `Callback/LayoutCallback.create`) drop the host-invoker ctx. Only the hello-world's WindowCreateOptions got a workaround (a structural smart factory in every managed binding). | api.json + managed_host_invoker.rs:200-238; per-language | HELLO-WORLD-ONLY workaround / SILENT SKIP |
| 3 | 15 of 77 callback kinds are outside `HOST_INVOKER_KINDS` (Timer, WriteBack, RenderImage, IconResolver, DbMerge, DatasetMerge, GetSystemTime, ...). They are unusable in Java/Kotlin/C# (non-public ctors), Go and OCaml (no ctor), Lua (ABI mismatch), Ruby/Node (raw pass-through). Haskell has one global slot per kind. Python's Thread/Timer trampolines are stubs. | managed_host_invoker.rs:51-131 | NAME-KEYED LIST / SILENT SKIP |
| 4 | C# `let is_app_run = func.c_name == "AzApp_run";` drives `__AzAppLoopState` and a leak branch in every `Dispose`, and misses `run_tray_only`. | lang_csharp/wrappers.rs:1522 | NAME-KEYED |
| 5 | OCaml tag helpers keyed on `"with_child"`, `"with_css"` and `create_*_with_text`; `create` returns a `dom` for any class with a method named `"dom"`. Together they produce exactly the hello-world's call shapes. | lang_ocaml/wrappers.rs:1173-1194, 842-855 | NAME-KEYED / HELLO-WORLD-ONLY |
| 6 | OCaml emits all 196 type aliases as 8-byte `unit ptr` (280 by-value bindings ABI-wrong; `AzCssProperty` laid out as 16 bytes instead of ≥88). 56 VecRef by-value args are passed as 8-byte pointers. | lang_ocaml/types.rs:170-176, 648-649, 107-122 | SILENT SKIP (ABI) |
| 7 | Python drops every `&mut self` method on unsendable classes: 521 of 3,812 methods (13.7%) disappear, including `Dom.add_child`, `Button.set_on_click` and `App.add_window`. Only the `with_*` builders the hello-world uses survive. | lang_python.rs:2815-2818 | SILENT SKIP |
| 8 | Python name lists: `CAPI_DIRECT` drops every U8Vec/StringVec/GLuintVec method; `return_type == "ImageRef"` drops the OpenGL callback; `ends_with("Value")` drops 11 real types; Thread/Timer trampolines are stubs that never run the Python function. | lang_python.rs:3090-3103, 677-682, 3160-3171, 712-733 | NAME-KEYED / SILENT SKIP |
| 9 | Node calls `lib.Az{T}_deepCopy` (the real export is `_clone`): 221 names at 767 call sites, so every Option unwrap or Vec iteration of cloneable structs throws. | lang_node/wrappers.rs:799, 1050 | SILENT SKIP (runtime TypeError) |
| 10 | Go: an idiomatic closure can be passed only through `set_on_*` setters (prefix-keyed) and the WindowCreateOptions factory. 31 callback-taking functions (all async `ResumeCallback` APIs, `Thread.create`, `AppConfig.add_route`, ...) have no idiomatic path. The logger interface is hand-typed `Log(AppLogLevel, *String)`. | lang_go/managed.rs:725, 895, 547-561; wrappers.rs:673-676 | HELLO-WORLD-ONLY / NAME-KEYED |
| 11 | Haskell silently drops 13 functions with raw-typedef callback args (incl. `domAddCallback`/`domWithCallback`). The raw path for the 15 non-host-invoker kinds uses one global C slot per kind (the newest registration wins) and returns an uninitialised struct. | lang_haskell/wrappers.rs:1149, 1291-1297; cshim.rs:339-369 | SILENT SKIP |
| 12 | Fortran's shipped Makefile hard-codes `EXE := hello_world` / `hello_world.o`. The String constructor is found by the method name `copy_from_bytes`. There is no `assignment(=)`, so `b = a` double-frees. | lang_fortran/makefile.rs:17-18, 120, 134-138; wrappers.rs:437-445 | HELLO-WORLD-ONLY / NAME-KEYED |
| 13 | Generic aliases don't inherit derives (the comment says they do): 122 of 180 get no Debug/Eq/Ord/Hash/Clone functions in libazul. Variant constructors share names with trait functions (`AzHttpMethod_delete(void)`, `AzBoxDecorationBreak_clone(void)`); only `"Default"` is guarded, and that guard drops 2 union variant constructors. | ir_builder.rs:1257-1266, 1704-1707, 1720-1731 | SILENT SKIP / NAME-KEYED |
| 14 | Derive surfacing: ordering is never exposed in C#/Java/Scala/Kotlin/Lua/Ruby/Node/Haskell, and hash never in Lua/Node/Haskell. About 260 classes each in Java/Kotlin/C#/Ruby pair value `equals` with identity hash. C++ public `delete_()` double-frees (911 classes). | per language | SILENT SKIP / correctness |
| 15 | `ir.constants` is consumed only by lang_c.rs and lang_zig. All 1,436 api.json constants (GL enums etc.) are missing from Rust, Python, C#, Java/Scala, Kotlin, Lua, Ruby, Node, OCaml, Go, Fortran, Haskell, D and Crystal. | all emitters except lang_c.rs / lang_zig | SILENT SKIP |
| 16 | Rust prelude is a hand-curated ~70-name list; 4 stale names (`On`, `OptionStyledDom`, `WindowState`, `Label`) are dropped with only a comment. VecRef helpers pick element types from a name table, so 7 VecRef types get none. | lang_reexports.rs:247-353; lang_rust.rs:1854-1899 | NAME-KEYED / SILENT SKIP |
| 17 | Hello-world-shaped API additions: Kotlin/Java zero-arg `WindowCreateOptions.create()` plus the `App.create(data, layoutFn)` splice; Ruby's invented `create_with_layout`; Python `__call__` on unit enums for `Update.RefreshDom()`. | lang_kotlin/wrappers.rs:720-753, 798-948; lang_java/wrappers.rs:328-360, 603-813; lang_ruby/wrappers.rs:240; lang_python.rs:1751-1764 | SUSPECT (HELLO-WORLD-ONLY) |
| 18 | Module misfits from the keyword classifier: callback families split dom/callbacks/widgets; VecSlice scattered; CSS parse errors in `xml`; `WindowPosition`/`StyledDom`/`EventFilter` in `css`; `MimeTypeHint`/`DetectedPinch` in `window` via 3-letter substrings. | doc/src/autofix/module_map.rs:213, 244, 323, 636-643, 733 | misfit classification |

## Finding counts per language

| Language | CERTAIN | SUSPECT | Headline |
|---|---|---|---|
| shared infra | 15 (S1-S10, S14-S18) | 3 (S11-S13) | Vec/VecRef/C-API-direct name lists; host-invoker list; S17; constants |
| modules | 8 (M1-M8) | 1 | keyword-length classifier |
| C | 5 | 2 | Vec macros for 4 Vecs; name list decides callback signature shape |
| C++ | 2 (+1 correctness) | 2 | iterators on 4 Vecs; `delete_()` double free |
| Rust | 5 | 3 | prelude list; VecRef name table |
| Python | 14 | 2 | `&mut self` drop; 5+ name lists; stub thread trampolines |
| C# | 10 | 2 | `AzApp_run`; 15 kinds; no ordering |
| Java | 9 | 5 | 15 kinds package-private; unpinned raw callbacks; `[WIP]` log keyed on `"log"`/`"Error"` |
| Scala | 0 (inherits Java) | 2 | example uses lowest tiers only |
| Kotlin | 7 | 3 | constants dropped; zero-arg `create()`; identity hash |
| Lua | 9 | 1 | typedef-form callbacks never fire; constants and boxed types dropped |
| Ruby | 8 | 2 | `create_with_layout`; broken `String#to_s`; no ordering |
| Node | 6 | 3 | `_deepCopy`; silent 15 kinds; unions leak `toString` |
| OCaml | 7 | 4 | name-keyed Dom/Button helpers; aliases as 8-byte ptrs |
| Zig | 3 | 3 | slice→Vec on 3 Vecs; Byref predicate re-derived |
| Go | 6 | 3 | callbacks only in hello-world shapes; hand-typed logger |
| Fortran | 5 (+2 derive) | 4 | hello_world Makefile; no `assignment(=)` |
| Haskell | 6 | 5 | 13 fns dropped; global callback slots; no Ord/Hash |
| D | 4 | 3 | GL alias names; hand-spelled runtime symbols |
| Crystal | 3 | 3 | fieldless-enum derives re-implemented |
| Swift | 6 | 3 | 1,347 fns dropped silently; pointer/callback fields unreachable |

## 0. Shared infrastructure (a hack here reaches every language)

Files audited: generator.rs, ir.rs, ir_builder.rs, managed_host_invoker.rs, managed_lang_helpers.rs, transmute_helpers.rs, module_plan.rs, config.rs, mod.rs (all under doc/src/codegen/v2/).

### CERTAIN

- **S1 [NAME-KEYED SPECIAL CASE]** doc/src/codegen/v2/ir_builder.rs:2256 and :2350: `const VEC_TYPE_NAMES: &[&str] = &["U8Vec", "StringVec", "GLuintVec", "GLintVec"];` / `if class_data.vec_element_type.is_some() || VEC_TYPE_NAMES.contains(&name) {`
  api.json sets `vec_element_type` on **0 of 2,459 classes**. The field exists (doc/src/api.rs:1227), but autofix never writes it into api.json. So `TypeCategory::Vec` comes purely from this 4-name list. The other **123 `*Vec` structs** (DomVec, StringPairVec, ListViewRowVec, ...) are classified `Regular`. Every emitter that gates its generic "Vec to host list" helper on the category therefore gives it to 4 Vec types only: Java (lang_java/types.rs:753), Kotlin (lang_kotlin/mod.rs:782), C# (lang_csharp/types.rs:504), Ruby (lang_ruby/types.rs:449), Lua (lang_lua/wrappers.rs:270), Zig (lang_zig/wrappers.rs:926), the C empty-Vec macros (lang_c.rs:1502) and C++ `is_vec_type` (lang_cpp/common.rs:263). OCaml (lang_ocaml/wrappers.rs:1539) and C++ `has_vec_layout` (lang_cpp/common.rs:271) already work around it by checking the struct layout.
  Fix: classify Vec structurally (fields ptr/len/cap/destructor, exactly like `has_vec_layout`), or have autofix write `vec_element_type` for every Vec. Then delete the list.

- **S2 [NAME-KEYED SPECIAL CASE]** ir_builder.rs:2264-2281 and :2377-2379: `CAPI_DIRECT_TYPES` contains `"InstantPtr"` and `"StringMenuItem"`, and both fall through to `return TypeCategory::Vec; // Vec is used as "uses C-API directly" marker`.
  Two non-Vec structs therefore receive Vec helpers. Verified in the output: `target/codegen/azul.lua:98422` `function InstantPtr_methods:to_lua_array()` reads `self.len`, but InstantPtr has no `len` field (it has ptr/clone_fn/destructor/run_destructor). `azul.lua:107230` `StringMenuItem_methods:to_lua_array()` indexes `self.ptr[i]` as AzString, yet StringMenuItem's fields are label/accelerator/callback/...
  Fix: drop the two names, or give "uses the C-API directly" its own category. Never overload `Vec`.

- **S3 [NAME-KEYED LIST / SILENT SKIP]** ir_builder.rs:2222-2248: `const VECREF_TYPE_NAMES: &[&str] = &[ "GLuintVecRef", ... "F32VecRefMut" ];` (23 names)
  api.json has no `vec_ref_element_type` anywhere either, so the `VecRef` category is name-list-only. **10 of the 23 names do not exist in api.json**: GLintVecRef, U16VecRef, U32VecRef, Refstr, TessellatedColoredSvgNodeVecRef, OptionI16VecRef, OptionI32VecRef, OptionF32VecRef, OptionFloatVecRef, F32VecRefMut. A new VecRef not added here silently becomes `Regular`. There is a dead duplicate at config.rs:799-823 (`PythonConfig.vecref_types` is never read; `skip_types` is empty, and `callback_types` is empty and never populated).
  Fix: derive the category from the `VecRef`/`VecRefMut` suffix plus the ptr/len layout, then delete both lists.

- **S4 [SILENT SKIP]** ir_builder.rs:1704-1707: `if variant.name == "Default" { continue; }` in `build_enum_variant_constructors`.
  10 enums have a variant named `Default`. For the two **data-carrying unions `AccessibilityAction` and `ComponentFieldValueSource`** (neither derives `Default`), no binding can construct the `Default` variant through the generated variant constructors. The collision the comment fears (`_createDefault`) cannot occur for them.
  Fix: emit the constructor under a non-colliding name (for example `defaultVariant`). Or skip it only when the type also has `_createDefault` and the language-level method names actually collide.

- **S5 [NAME-KEYED LIST, test-guarded]** managed_host_invoker.rs:51-131: `pub const HOST_INVOKER_KINDS: &[&str] = &[ "Callback", "LayoutCallback", ... "ThreadCallback" ];` (62 names)
  This is category (b), callback wrapping. The `matches_every_engine_thunk` test (managed_host_invoker.rs:814) keeps it equal to the engine's `impl_managed_callback!` sites, so it cannot drift. But it is still a hand-kept list rather than api.json data.
  **15 of the 77 non-destructor callback typedefs are outside it**: CaretTweenCallbackType, ComponentCompileFn, ComponentRenderFn, CustomE2eOpCallbackType, DatasetMergeCallbackType, DbMergeCallbackType, GetSystemTimeCallbackType, IconResolverCallbackType, MarginBoxCallbackType, MeasureDomFn, RegisterComponentLibraryFnType, RenderImageCallbackType, SelectionTweenCallbackType, **TimerCallbackType**, **WriteBackCallbackType**.
  The file's own doc (managed_host_invoker.rs:48-50) says these get "the legacy `pin_callback` path ... which compiles fine but won't fire on libffi-style hosts". So the bindings that route closures through the host invoker (C#, Java/Scala, Kotlin, Lua, Ruby, Node, OCaml, Go, Fortran, Haskell) cannot implement timers, thread write-backs, OpenGL render-image callbacks or icon resolvers with a host closure. D, Crystal and Swift escape this with their own per-typedef trampolines, and Python with in-process trampolines (7 of which are stubs). Per-language details are in table 2a.
  Fix: apply `impl_managed_callback!` engine-side to these kinds, and let autofix mark them in api.json (e.g. `managed_callback: true`) so the list is derived.

- **S6 [NAME-KEYED SPECIAL CASE]** managed_host_invoker.rs:502-517 (`return_c_size`): `"AzDom" => 280, "AzVirtualViewReturn" => 320, "AzOnTextInputReturn" => 8, "AzUpdate" => 4, ... _ => 4,`
  These are hard-coded C struct sizes keyed by type name. The comment at :499-501 records that they already drifted once (Dom 240 -> 280), and any unknown aggregate return silently falls back to 4 bytes. The only consumer is Perl (lang_perl/managed.rs:263, out of scope), so no shipped language is affected today.
  Fix: compute the size from the IR (lang_fortran/layout.rs already implements C layout).

- **S7 [SILENT SKIP]** ir_builder.rs:2104-2127 (`detect_callback_arg_info`): `let (typedef_name, wrapper_name) = if type_name.ends_with("CallbackType") { ... } else { if !type_name.ends_with("Callback") { return None; } ...`
  Callback arguments are recognised only by the naming convention. `AppConfig.add_component_library(register_fn: RegisterComponentLibraryFnType)` (a `*FnType`) gets no `callback_info`, so managed bindings cannot pass a closure there. The shared IR also carries a Python-specific trampoline name: `format!("invoke_py_{}", ...)` at :2133.
  Fix: detect a callback argument via `class_data.callback_typedef.is_some()`, which api.json already records, instead of the suffix.

- **S8 [MINOR]** ir.rs:110: `if name.ends_with("CallbackType") || name.ends_with("FnType") { return false; }` in `is_value_aggregate`. It tests the suffix before consulting `ir.callback_typedefs`. Correct today, but it re-derives a category from the name.
  Fix: look the name up in `ir.callback_typedefs`.

- **S9 [MINOR]** config.rs:653-666: `for gl_type in &[ "GLuint", "GLint", ... "GLintptr" ] { type_exclude.insert(...) }`. The legacy `rust_public_api` excludes 12 GL aliases by name. This only affects `target/codegen/azul.rs`, which generator.rs:166 labels "legacy, may be removed".

- **S10 [MINOR, stale docs]** Two doc comments that no longer match reality:
  - managed_host_invoker.rs:413 says "today only Button matches the wrapper-struct shape". False: `smart_callback_setter_info` matches **58 `with_*` methods over 43 classes** (verified against api.json). So the smart setter is generic, not Button-only.
  - :266 says "Today only `WindowCreateOptions` matches". That one is still true.

- **S14 [NAME COLLISION]** ir_builder.rs:1720-1731: variant constructors are named `format!("Az{}_{}", enum_name, method_name)` with `method_name` = lowerCamel(variant). That is the same namespace as the trait functions (`_delete`, `_clone`, `_hash`, ...), and only the `Default` spelling is guarded (S4). As a result azul.h declares **constructors** named like trait functions:
  - `AzPhysicalKey_delete(void)` (:64090)
  - `AzHttpMethod_delete(void)` (:64166)
  - `AzVirtualKeyCode_delete(void)` (:65322)
  - `AzBoxDecorationBreak_clone(void)` (:66233)
  All four enums are Copy, so no real `_delete`/`_clone` exists and no C symbol clashes today. But any code that recognises a destructor or clone by its suffix rather than by `FunctionKind` misreads them (even this audit's first derive count took `AzBoxDecorationBreak_clone` for a Clone function; the true Clone count is 1,114, not 1,115). Found during the C/C++ pass and verified directly.
  Fix: give variant constructors a reserved-name escape derived from `FunctionKind::c_suffix()` (for example `Az{Enum}_{variant}Variant`) instead of special-casing `"Default"`.

- **S15 [SILENT SKIP]** ir_builder.rs:1257-1266: the comment says `// For type aliases, inherit traits from the target type if not explicitly set`, but the code is `let derives = class_data.derive.clone().unwrap_or_default();`. It reads only the alias's own `derive`. **122 of 180 generic aliases** (the `CssPropertyValue<T>` monomorphs such as `CaretColorValue`) declare no derive of their own. They therefore get no `_toDbgString`/`_partialEq`/`_partialCmp`/`_cmp`/`_hash`/`_clone`/`_createDefault` in libazul or in any binding, even though `CssPropertyValue` derives Debug, Clone, PartialEq, PartialOrd, Ord, Hash, Copy and Eq. Verified: azul.h exports only `AzCaretColorValue_delete` (:89228). They also lose `Copy`, so they get a `_delete` they should not need.
  Fix: when an alias has no `derive`, take the target type's derive/custom_impls, as the comment says.

- **S16 [SILENT SKIP, category-keyed]** The managed bindings (C#, Java, Kotlin, Node, OCaml, Fortran, Haskell) bind every derive function in azul.h **except 14 types**: the 12 `VecRef`/`VecRefMut` types (their category comes from the S3 name list) and `InstantPtrCloneCallback`/`InstantPtrDestructorCallback` (the `DestructorOrClone` category). Verified by diffing `Az*_toDbgString` between azul.h and target/codegen/java. Lua, Ruby, Zig, Go, D and Crystal bind all of them. Swift declares all of them through `import CAzul`, but its wrappers call fewer (table 2b).

- **S17 [HELLO-WORLD-ONLY workaround for an api.json shape; SILENT SKIP elsewhere]**
  - **The shape.** 7 api.json functions take the **raw typedef** of a host-invoker kind instead of its wrapper struct:
    - `WindowCreateOptions.create(layout_callback: LayoutCallbackType)`
    - `LayoutCallback.create(cb: LayoutCallbackType)` and `Callback.create(cb: CallbackType)`
    - `Dom.with_callback`, `Dom.add_callback`, `NodeData.add_callback` and `StringMenuItem.with_callback` (all `callback: CallbackType`)
  - **Why the closure never runs.** The DLL exports only the bare-pointer form, e.g. `AzDom_withCallback(AzDom, AzEventFilter, AzRefAny, AzCallbackType)` (azul.h:54324). Its body is `Callback::create(callback)`, which sets ctx = None (layout/src/callbacks.rs:1110-1113). The host-invoker thunk returns `$default` when ctx is None (core/src/host_invoker.rs:442-450). `managed_c_symbol`/`has_callback_wrapper_arg` (managed_host_invoker.rs:200-238) add `…Struct`/`WithCtx` twins only for **wrapper-struct** args, so no ctx-preserving twin exists.
  - **What the bindings do.** Every managed binding papers over this with the structural `layout_callback_factory_info` smart factory (S11). It builds `_createDefault()` and byte-splices the registered wrapper into `window_state.layout_callback`. That works, but only for the hello-world's entry point.
  - **Consequence.** The four generic DOM event-registration functions get no such fix. The outcome varies by binding (table 2a):
    - **Lua:** the closure is registered, its handle leaks, and the event silently returns `DoNothing` (azul.lua `Dom:with_callback` → `callback = _callback_cb.cb`).
    - **Ruby:** passes the registered `AzCallback` struct to a parameter declared `:az_callback_type`, so the call raises.
    - **Node:** throws a descriptive TypeError.
    - **Haskell:** drops the functions.
    - **Go, Fortran, Java, C#, Kotlin, OCaml:** expose only a raw function pointer or delegate.
    - **D, Crystal, Swift, Python:** work, by carrying the closure in the `data` RefAny or building the full wrapper in-process.
  - **Fix.** Change these api.json args to the wrapper types (`Callback`, `LayoutCallback`), so the generic triple/twin machinery applies. Then delete the WindowCreateOptions factory special cases. (Found during the Kotlin/Lua pass and verified directly.)

- **S18 [SILENT SKIP, cross-language]** `ir.constants` has only two consumers: `rg 'ir\.constants'` over the shipped emitters finds lang_c.rs and lang_zig and nothing else.
  - **Where the constants reach.** api.json defines **1,436 constants** (e.g. `GlContextPtr::ACCUM_ALPHA_BITS`). They exist only as azul.h `#define AzGlContextPtr_ACCUM_ALPHA_BITS 0x0D5B` (azul.h:50798) and in azul.zig. C++ and Swift see them only transitively through azul.h (for Swift, via `import CAzul`, not `import Azul`).
  - **Where they are missing.** Every other shipped binding (Rust, Python, C#, Java/Scala, Kotlin, Lua, Ruby, Node, OCaml, Go, Fortran, Haskell, D, Crystal) has 0 of them, with no diagnostic. Verified by grepping each output for `ACCUM_ALPHA_BITS`. The shipped Rust binding (dll_api_external.rs, reexports.rs) has none either; they exist only in core/src/glconst.rs. Lua's cdef.rs:21-23 even claims that `#define`s "are turned into `enum { X = Y };`"; no such code exists (cdef.rs:279-283 drops every `#` line).
  - **Fix.** Emit `ir.constants` generically in every binding, as static constants on the owning class/module (`<Class>.<NAME>`).

### SUSPECT (maintainer's judgement)

- **S11 [HELLO-WORLD-shaped, but structurally derived]** managed_host_invoker.rs:696-801 (`app_factory_info`) and :257-356 (`layout_callback_factory_info`). No type names are used. Matching is by shape: a 2-arg `(RefAny, Config-with-_default)` constructor, methods taking a struct with a nested layout-callback field, and a `_default` + 1-callback-arg factory. They exist so the managed guides can write `App.create(model, layoutFn)` + `app.run(opts)` and `WindowCreateOptions.create(layoutFn)`, and exactly one class matches each (App, WindowCreateOptions). They are allowed under the "generic" rule, but they are ergonomics built for the hello-world.
- **S12 [MINOR]** ir_builder.rs:1450-1476 (`link_callback_wrappers`): a struct counts as a callback wrapper only if its name ends in `Callback` and it has a field named exactly `ctx` or `callable` of type `OptionRefAny`. All current wrappers comply (checked: no struct with a callback-typedef field plus an OptionRefAny field is left unlinked), but this is convention-keyed.
- **S13 [MINOR]** transmute_helpers.rs:405-414 detects "builder pattern" methods by the substrings `.with_` / `.set_` / `.add_` in the api.json `fn_body`, to decide whether to clone self. It is a heuristic, not keyed on API names.

### Acceptable helpers (not hacks)
- managed_host_invoker.rs:563-608 `emit_cdef_block`: host-handle releaser, RefAny host handles, per-kind invoker setters and `createFromHostHandle(Byref)`. This is callback and RefAny glue.
- managed_lang_helpers.rs: every predicate is category-based (`is_refany_type` uses `TypeCategory::RefAny`; `has_wrapper_class` and `has_delete_function` use FunctionKind). Clean.
- module_plan.rs: a pure type-dependency graph (SCC plus Kahn). The only literal is `CONTAINER_MODULES = &["vec", "option"]` (:51), which is api.json's container convention.
- generator.rs and mod.rs: dispatch only.
- ir_builder.rs:112-190 `STANDALONE_NON_FFI_TYPES`: a validation deny-list (the build fails with a message). It does not skip anything silently.

## 0b. Classification: TypeCategory misfits (IR level)
(The per-type category is the "classification" every emitter trusts.)
- `TypeCategory::Vec` = {U8Vec, StringVec, GLuintVec, GLintVec} plus the misfiled {InstantPtr, StringMenuItem}. 123 real Vecs are `Regular` (S1, S2).
- `TypeCategory::VecRef` = a name list with 10 stale entries (S3).
- `TypeCategory::DestructorOrClone` is decided by name suffixes (ir_builder.rs:2399-2406, :2429-2434: `ends_with("Destructor")`, `"DestructorType"`, `"CloneCallbackType"`, `"CloneCallback"`, `"DestructorCallback"`). This is consistent with api.json today, but it is a naming convention, not data.
- `TypeCategory::Option`/`Result` require the name prefix `Option`/`Result` **and** Some/None or Ok/Err variants (ir_builder.rs:2287-2329). That is correct and structural. Compare the module classifier below, which uses a bare substring.

## 1. Module classification misfits (api.json modules), a user follow-up

**Where it comes from.** api.json modules are assigned by the autofix classifier `doc/src/autofix/module_map.rs`:
- `determine_module` (:627) first applies structural rules. Option prefix. Vec suffixes (:636-643). Error suffix / Result prefix (:646-648). Then the hand-kept table `DIFFICULT_TYPE_MODULES` (:545). Then the **longest substring** among per-module keyword lists (`get_module_keywords`, :59-478; ties go to the module-name match, then to `MODULES` order).
- `get_correct_module_with_path` (:718) uses the Rust source path only when it *confirms* the current module (:766) or when keywords fall through to `misc`. A confident keyword hit therefore overrides the source path.
- Of the non-structural types, **313 sit in a module that differs from their source-path module**. Some of that is deliberate (e.g. `component`, `time` are grouped by concern). The ones below are clearly keyword collisions.

Each misfit is user-visible in every binding that exposes modules. For example, target/codegen/reexports.rs gives Rust users `azul::callbacks::ButtonOnClickCallbackType` (:155), `azul::dom::ButtonOnClickCallback` (:2250) and `azul::widgets::Button` (:5297). It also exports `azul::css::StyledDom` (:765) and `azul::css::WindowPosition` (:1149). Per-module units (Haskell, OCaml, Fortran through module_plan.rs, the docs pages) inherit the same split.

### CERTAIN

- **M1 [misfit: callback families split across 3 modules]**
  - Cause: `dom` owns the bare keyword `"callback"` (module_map.rs:213, 8 chars), while `callbacks` owns `"callbacktype"` (:244, 12 chars).
  - Result for widgets:
    - All 56 widget `*CallbackType` typedefs are in `callbacks`.
    - **47 widget `*Callback` wrapper structs are in `dom`**.
    - The widget itself is in `widgets`.
    - 9 wrappers whose widget keyword beats 8 characters are in `widgets` instead: `textinput`/`numberinput`/`colorinput`/`fileinput`/`mapviewport`/`nodegraph` (e.g. TextInputOnTextInputCallback, NumberInputOnValueChangeCallback, OnNodeGraphDraggedCallback).
  - The core family is split the same way:
    - `Callback`, `CoreCallback`, `CoreCallbackData`, `VirtualViewCallback`, `RenderImageCallback`, `CaretTweenCallback`, `SelectionTweenCallback` are in `dom`.
    - `CallbackInfo`, `CallbackType`, `VirtualViewCallbackInfo`, `RenderImageCallbackInfo` are in `callbacks`.
    - `LayoutCallback`/`TimerCallback`/`ThreadCallback` are fully in `callbacks`.
  - The InstantPtr family spans three modules: `InstantPtr` in `time`, `InstantPtrCloneCallback`/`InstantPtrDestructorCallback` in `dom`, and their `*Type` typedefs in `callbacks`.
  - Widget state types are in `dom` for the same reason (`"selection"`/`"virtualkey"` dom keywords): `TextInputSelection`, `TextInputSelectionRange`, `TextInputOnVirtualKeyDown`, `TextAreaOnVirtualKeyDown`.
  - Fix: keep a callback family (typedef, wrapper, `*CallbackInfo`, `Option*Callback`) with its owner, derived from the source path (`azul_layout::widgets::*` maps to `widgets`, `azul_core::callbacks::*` to `callbacks`). The family rule should outrank keywords.

- **M2 [misfit: `*VecSlice` escapes the structural vec rule]** The 118 `*VecSlice` types are spread over 24 modules (dom 30, css 30, widgets 14, window 8, component 8, gl 4, svg 4, db 4, ...), while every sibling `*Vec` is in `vec`.
  - Cause: `is_structural` (module_map.rs:733) includes `ends_with("vecslice")`, but `determine_module`'s vec rule (:637-641) does not. The "structural" answer it trusts is therefore the keyword guess.
  - Fix: add `ends_with("vecslice")` to Priority 2.

- **M3 [misfit: CSS parse-error family split]** `*ParseErrorOwned` does not end in `error`, so it escapes the structural error rule (:647).
  - Cause: xml's keyword `"parse"` (:323) ties with shorter CSS keywords and wins on `MODULES` order.
  - Result: **18 CSS parse errors are in `xml`** (CssParseErrorOwned, FlexGrowParseErrorOwned, FlexWrapParseErrorOwned, GridParseErrorOwned, OverscrollBehaviorParseErrorOwned, ...), 123 are in `css`, and one each is in `window` (CursorParseErrorOwned), `image` (CssImageParseErrorOwned) and `time` (DurationParseErrorOwned).
  - `ParseIntErrorWithInput`/`ParseFloatErrorWithInput` land in `dom`: dom's `"input"` ties with `"parse"`, and dom comes first.
  - Fix: treat `*ErrorOwned` / `*ErrorWithInput` as structural errors, or follow the source path.

- **M4 [misfit: non-Option structs in the Option container module]** `SvgParseOptions`, `SvgRenderOptions`, `IconStyleOptions` and `ScrollIntoViewOptions` are in `option`, because the keyword `"option"` (:69) matches as a substring. module_plan.rs then treats them as containers (CONTAINER_MODULES) and relocates them by dependency.
  - Fix: use the same test as `ir_builder::is_option_shaped`: `Option` prefix plus Some/None variants.

- **M5 [misfit: short-substring collisions]**
  - `MimeTypeHint` is in `window`: window keyword `"ime"` (:186) matches inside "m**ime**".
  - `DetectedPinch` (a gesture) is in `window`: `"dpi"` (:176) matches inside "detecte**dpi**nch".
  - `MapPinTap` (map widget event) is in `app`: the module name "app" matches inside "m**app**intap".
  - `CssColorComponent` is in `component` (module name "component").
  - `TitlebarButtons`/`TitlebarButtonSide` (azul_css::system) are in `widgets`, via `"button"` (:452).
  - `FrameConsumer`/`ConsumerFrame`/`ZombieFrame` are in `widgets`, via `"frame"` (:467).
  - `UpdateImageType` (image resources) is in `callbacks`, via `"update"` (:254).
  - `DeleteResult`/`SelectAllResult` (text-edit results, not errors) are in `error`, via `"result"` (:72).

- **M6 [misfit: CSS keywords reaching into other domains]**
  - Window types in `css`: `WindowPosition`, `WindowDecorations`, `WindowBackgroundMaterial`, `CursorPosition`, `ImePosition`, `LinuxDecorationsState`. `"position"` (:96), `"decoration"` (:126) and `"background"` (:86) are longer than `"window"`.
  - DOM types in `css`: **`StyledDom`**, `StyledNode`, `StyledNodeState` (`"style"` beats module name `dom`). StyledDom is what layout callbacks return.
  - Event filters in `css`: `EventFilter`, `HoverEventFilter`, `FocusEventFilter`, `ApplicationEventFilter`, `ExternalEventFilter`, because the CSS `"filter"` (:100) beats dom's `"event"`.
  - Also in `css`: `FocusDirection`/`GestureDirection` (`"direction"`, :115), `ListType` (ICU list format) via `"list"` (:135), and `XmlTextPos` via `"text"` (:117).
  - CSS selector types in `svg`: `CssPath`, `CssPathSelector`, `CssPathPseudoSelector`, because svg's `"path"` (:296) beats module name `css`.
  - The CSS cursor property in `window`: `StyleCursor`/`StyleCursorValue`, via window's `"cursor"` (:184).

- **M7 [misfit: other cross-domain keyword wins]**
  - `XmlNode`, `XmlNodeChild`, `XmlAttributeMap` are in `dom` (`"node"`/`"attribute"`, :206-207).
  - `ParsedSvg` is in `xml`.
  - `VideoDecoder`, `VideoEncoder`, `DecodedVideo`, `VideoDecodeResult`, `VideoEncodeCheck` are in `image` (`"decode"`/`"encode"`, :329).
  - `MapTheme`/`UiTheme` (widgets) are in `window` (`"theme"`, :182).
  - `IcuTime` is in `time`, `IcuLocalizerHandle` in `fluent`, and `StringSet`/`StringSetValue` (the CSS `string-set` property) in `str`.

- **M8 [misfit: `misc` leftovers that split families]**
  - `MediaControlKind`, `MediaControlRequest` and `PlaybackState` sit in `misc`, while `MediaPlaybackState`/`NowPlayingInfo` are forced into `audio` by DIFFICULT_TYPE_MODULES.
  - `PaginationSnapshot` sits in `misc`, while the whole pagination family is forced into `pdf`.
  - Also in `misc`: `InstallKind`/`ReleaseInfo` (updater), `Capability`/`PermissionState`/`PermissionQuality`, `Transient*`, `DocOp*`, `EditResumePoint`.

### SUSPECT
- **M9 [MINOR]** module_map.rs:545-618 `DIFFICULT_TYPE_MODULES` is itself a hand-kept name-to-module table. Its comments document four word-boundary traps it had to patch (Tablet/Table, Dial/Dialog, Hid/Hidpi, Media/MediaType).
  - Fix: invert the priority. `module_from_external_path` already maps most crate paths, so make the source path the primary signal, and use keywords only for types without a path or for concern-organized modules.
- Input-state types (`KeyboardState`, `MouseState`, `TouchState`, `DebugState`, `ScanCode`, `VirtualKeyCode`) are in `dom` although their path is `azul_core::window`. This looks deliberate: they are listed as explicit dom keywords (:214-220).

## 2. Cross-language matrices

### 2a. Callback support: which callback shapes work idiomatically

The columns:
- **Host-invoker kinds:** the 62 kinds in `HOST_INVOKER_KINDS`.
- **15 other kinds:** Timer, WriteBack, RenderImage, IconResolver, DbMerge, DatasetMerge, GetSystemTime, CaretTween, SelectionTween, MarginBox, CustomE2eOp, MeasureDomFn, ComponentRender/Compile, RegisterComponentLibrary (S5).
- **Raw-typedef args (S17):** `Dom.with_callback`, `Dom.add_callback`, `NodeData.add_callback`, `StringMenuItem.with_callback`.
- **WCO factory:** the structural `WindowCreateOptions.create(layoutFn)`. Only WindowCreateOptions matches `layout_callback_factory_info`, and it is the hello-world's entry point.

| Language | Host-invoker kinds | 15 other kinds | Raw-typedef args (S17) | WCO factory |
|---|---|---|---|---|
| C | native fn ptr | usable, but as a `{cb,ctx}` struct: the name list decides the signature shape (lang_c.rs:910-914, 971-972) | native fn ptr | n/a |
| C++ | uniform via `az_detail_wrap_cb` | uniform | native | n/a |
| Rust | bare `extern "C" fn` | wrapper struct built by hand (e.g. examples/rust/src/anim.rs:66-68) | native | n/a |
| Python | own IR-driven trampolines | 7 kinds are **stubs that return a default** (Thread, Timer, SelectionTween, CaretTween, DbMerge, MarginBox, CustomE2eOp); 8 kinds have none | works (full wrapper built in-process) | generic |
| C# | typed `WithData<T>` for all 62 | `internal` ctor only; unusable | raw delegate (pinned) | `Create<T>` |
| Java / Scala | typed SAMs for all 62 | package-private ctor; unusable outside `com.azul`; raw callbacks unpinned (GC hazard) | raw JNA `Callback`, unpinned | `create(fn)` plus zero-arg `create()` |
| Kotlin | typed for all 62 | `internal` ctor only | bare JNA interface | ×2 plus zero-arg `create()` |
| Lua | `_register_callback` | `pin_callback` ABI mismatch, runtime error | **closure never fires, handle leaks (silent)** | special case 2 |
| Ruby | `_register_callback` | raw pass-through, TypeError; callback typedefs ABI-wrong | **struct passed to a fn-ptr param, raises** | `create_with_layout` (invented name) |
| Node | registerCallback | silent raw pass-through | explicit TypeError (loud) | `create(fn)` |
| OCaml | `HostFn` for all 62 | no ctor, so `Timer.create` is uncallable | raw `unit ptr` only | `?layout` |
| Zig | `_asCallback` | the wrapper struct must be hand-built (gate is inherited from the managed list) | `_asCallback` works | n/a |
| Go | only 58 `On<X>` setters plus the WCO factory; **31 callback-taking fns have no idiomatic path** | unusable (no ctor) | raw `AzCallbackType` | `WindowCreateOptionsCreate` |
| Fortran | typed procedure for every arg | `bind(C)` fn plus `c_funloc` by hand | raw `c_funptr` | `window_create_options_create` |
| Haskell | closures for all 62 | **one global C slot per kind**; uninitialised return when unset | **13 fns dropped (only a `-- SKIPPED` comment)** | `windowCreateOptionsCreate` |
| D | own trampoline per typedef (152 typed members) | Timer typed via the data RefAny; DatasetMerge/IconResolver/RegisterComponentLibrary/Thread raw | typed via the data RefAny | factory, falling back to the guessed field name `"ctx"` |
| Crystal | 55 per-typedef trampolines | Timer/WriteBack/DbMerge typed; Thread/IconResolver raw; field-only kinds as raw setters | typed `Proc` | factory with `"ctx"` fallback |
| Swift | same design as Crystal | same as Crystal, but field-only kinds are unreachable | typed closure | factory with `"ctx"` fallback |

### 2b. Derive surfacing

Numbers are distinct `Az*_<cap>` symbols the idiomatic layer calls, out of the azul.h totals: Debug 2018, PartialEq 1563, PartialOrd 970, Ord 843, Hash 871, Clone 1114, Default 506. Nearly every capability is declared in each binding's FFI layer (except the S16 gap). The table shows what reaches the idiomatic API.

| Language | Debug | PartialEq | PartialOrd / Ord | Hash | Clone | Default | Notable defects |
|---|---|---|---|---|---|---|---|
| C | fn (100%) | fn | fn | fn | fn | fn | ground truth |
| C++ | `toDbgString()` (100%) | `partialEq()`, no `operator==` | `partialCmp()`/`cmp()`, no `operator<` | `hash()`, no `std::hash` | `clone()`, but the copy ctor is deleted | `createDefault()` | public `delete_()` double-frees (911 classes) |
| Rust | `Debug` (100%) | `PartialEq` | `PartialOrd`/`Ord` | `Hash` | `Clone` | `Default` | complete, via the C calls |
| Python | `__repr__` | `__eq__` | `__lt__`… | `__hash__` | `__copy__` | `default()` | gaps only where no pyclass exists (InstantPtr and StringMenuItem, via the name list; VecRef); enum `__eq__`/`__hash__` re-implemented as a 1-byte transmute |
| C# | 661 | 535 | **0** | 267, plus a `ValueType` hash fallback on 268 | 640 | 173 | unions and value structs get nothing |
| Java / Scala | 663 | 535 | **0** | 267, plus identity fallback on 268 | 640 (`clone_()`) | 173 | JNA `Structure` equals by pointer |
| Kotlin | 663 | 535 | **0** | 267, plus identity fallback on 268 | 640 | 173 | 1,341 payload variants emitted only as `// SKIPPED` comments |
| Lua | 1057 | 811 | **0** | **0** | 1099 | 370 | unions have no `__eq`/`__tostring` |
| Ruby | 648 | 514 | **0** | 253, plus address-hash fallback on 261 | 668 | 158 | only struct classes that own a `_delete` |
| Node | 1642 (586 union `toString()` return a raw, leaked AzString) | 811 (structs only) | **0** | **0** | 1094 (plus the broken `_deepCopy` calls) | 370 | |
| OCaml | 1873 | 1559 | 528 / 839 | 867 | 1101 | 506 | no `pp` |
| Zig | 1890 (no `format()`) | 1562 | 969 / 842 (raw `u8`) | 870 | 1114 | 506 | |
| Go | all | all | raw `uint8` | all | all | all | complete |
| Fortran | 1000 | 751 | 461 / 375 (raw int8) | 387 | 577 | 331 | no operators; **no `assignment(=)`, so double frees** |
| Haskell | 820 | 701 | **0** | **0** | 640 | 333 | native `deriving (Show)` elsewhere; bitwise copies of non-Copy unions |
| D | 1349 | 960 | 72 / 459 (cmp preferred) | 472 | 756 | 504 | implicit `==`/`toHash` where Rust has none |
| Crystal | 2018 | 1337 | 126 / 665 (cmp preferred) | 681 | 1113 | 506 | fieldless-enum derives re-implemented natively |
| Swift | 1576 | 1186 | 73 / 637 (cmp preferred) | 662 | 756 | 504 | native `[T]`/`T?`/`String` use Swift semantics |

**Shared causes behind the derive gaps:**
- 122 generic aliases never get derive functions (S15).
- 14 VecRef/DestructorOrClone types are undeclared in C#, Java, Kotlin, Node, OCaml, Fortran and Haskell (S16).

### 2c. Impact of the shared name-list issues

The columns:
- **Vec category (S1/S2):** the Vec helpers that end up gated on the 4-name list.
- **`Default`-variant skip (S4):** whether `AccessibilityAction::Default` / `ComponentFieldValueSource::Default` can be constructed.
- **Constants (S18):** whether api.json constants are exposed.

| Language | Vec category (S1/S2) | `Default`-variant skip (S4) | Constants (S18) |
|---|---|---|---|
| C | `_empty` macros for 4 Vecs; `AzInstantPtr_empty` does not compile | designated initializer only | yes |
| C++ | begin/end/size/`toStdVector`/span on 4 Vecs (`from_std_vector` on all 127) | holder lacks `default_()` | via azul.h |
| Rust | none | none | **missing** |
| Python | InstantPtr/StringMenuItem get no pyclass, so `MenuItem::String` cannot be built | none | **missing** |
| C# | FFI `ToArray` on 4 Vecs | none | **missing** |
| Java / Scala | FFI `toList` on 4 Vecs | none | **missing** |
| Kotlin | FFI `toList` on 4 Vecs; wrappers structural (75/127) | none | **missing** |
| Lua | `to_lua_array` on 6 types, 2 of them broken | no `default` ctor | **missing** |
| Ruby | Native `to_a` on 5 types (InstantPtr raises; struct elements return garbage) | n/a (no enum ctors at all) | **missing** |
| Node | none (layout-based) | no `default` factory | **missing** |
| OCaml | none (layout-based) | no ctor | **missing** |
| Zig | slice to Vec only for U8Vec/StringVec/GLuintVec (88 params of 39 Vec types excluded) | no ctor | yes |
| Go | none | none | **missing** |
| Fortran | none | **unconstructible** | **missing** |
| Haskell | none | none | **missing** |
| D | none | none | **missing** |
| Crystal | none | none | **missing** |
| Swift | none | none | via `import CAzul` only |

### 2d. Patterns that recur across bindings

These are per-binding copies of one missing shared helper.

- **The "failure logger" keyed on a method named `log` and a variant named `Error`.**
  - Instances: OCaml (committed, lang_ocaml/managed.rs:714-736); Go (hand-typed `Log(AppLogLevel, *String)`, lang_go/managed.rs:547-561); Java, Fortran and Haskell (`[WIP]`, uncommitted: lang_java/managed.rs:925-968, lang_fortran/managed.rs:429-466, lang_haskell/wrappers.rs WIP 1035-1072).
  - Failure mode: if the variant is renamed, Fortran falls back to `AppLogLevel::Off` and silently mutes every report.
  - Fix: one shared `failure_logger_info(ir)` in managed_host_invoker.rs, driven by an api.json marker.
- **String/RefAny identified by literal name** (`== "String"`, `== "RefAny"`) instead of `TypeCategory`/`is_refany_type`: C++ common.rs:1640; Ruby wrappers.rs:1089; Kotlin wrappers.rs:1126; Lua wrappers.rs:256, 369; C# wrappers.rs:1245, 1252; Java wrappers.rs:1365; OCaml wrappers.rs:393, 898, 902, 1175, 1187 and managed.rs:111; Haskell wrappers.rs:283, 665, 936, 938, 1177; D model.rs:626-657; Crystal model.rs:440, 443; Swift model.rs:666-699.
- **The String constructor found by a literal method or C name**, and the names disagree between bindings: Fortran and Haskell use `"copy_from_bytes"`; Ruby, Node, Go, C#, Java, D and Crystal use `"from_utf8"` / `AzString_fromUtf8`. Several also hard-code the `AzString.vec.len` offset (Ruby `get_uint64(8)`, Java `getLong(8)`, C# `ReadInt64(sPtr, IntPtr.Size)`).
  - Fix: one shared `string_ctor(ir)` helper that finds the `(bytes, len) -> String` constructor by signature.
- **Layout-factory ctx field guessed as `"ctx"`** when `callback_ctx_field` returns `None`, with `AzOptionRefAny_delete` hard-coded: D wrappers.rs:1989-1995, Crystal wrappers.rs:1265-1276, Swift wrappers.rs:1955-1963.
- **Hard-coded package versions** despite ir.rs:48-51 ("MUST use `api_version`"): Kotlin gradle.rs:29 `"0.2.0"`, C# csproj.rs:50 `1.0.0`, Java pom.rs:29-31 `com.azul:azul:1.0.0` (the published coordinates are `rs.azul:azul`).
- **Payload-carrying union variants emitted as `// SKIPPED` comments:** 1,341 of them in C#, Java and Kotlin. The `Az<Enum>_<variant>(payload)` constructors are declared but never wrapped.


---

# Per-language findings


---

<!-- c_cpp.md -->

**Source partial:** C and C++ bindings: hack audit (partial report)

Scope: `doc/src/codegen/v2/lang_c.rs`, `doc/src/codegen/v2/lang_cpp/{mod,common,cpp03,cpp11,cpp14,cpp17,cpp20}.rs`.
Generated output checked: `target/codegen/azul.h`, `azul03.hpp` … `azul23.hpp`, `azul.cppm`. The directory was wiped and regenerated at 15:34–15:35 while this audit ran; every generated-output line number below was re-checked against the regenerated files.
Examples: `examples/c/hello-world.c`, `examples/cpp/cpp{03,11,14,17,20,23}/hello-world.cpp`.

Overall: neither emitter contains a branch keyed on a specific API type or function name (no `"Dom"`, `"Button"`, `"App"`, `"WindowCreateOptions"` and so on). Every `Az<Name>` literal in the emitters sits in a comment, except the RefAny, String and `OptionRefAny` helper symbols. The hello-world examples use only generically generated API. The findings below come from shared name lists that decide behaviour by category (Vec, callback kind), from silent kind-level skips, and from dead "for now" branches.

---

## C

### CERTAIN

- **[SILENT SKIP]** `doc/src/codegen/v2/lang_c.rs:1483-1504`: the doc comment says `/// Generate empty Vec initializer macros for all Vec types`, but the loop is gated by `if !matches!(struct_def.category, TypeCategory::Vec) { continue; }` (line 1502).
  - Why it is a hack: `TypeCategory::Vec` is assigned only by the shared name lists `VEC_TYPE_NAMES` and `CAPI_DIRECT_TYPES` (ir_builder.rs:2256, 2264-2281; api.json never sets `vec_element_type`).
  - Result in azul.h: 5 macros. The 4 listed Vecs get one (`AzU8Vec_empty`, `AzGLuintVec_empty`, `AzGLintVec_empty`, `AzStringVec_empty`), and **none of the other 123 `*Vec` structs** do.
  - One of the 5 is broken: `#define AzInstantPtr_empty { .ptr = 0, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzInstantPtrDestructorCallback_Tag_NoDestructor } } }` (azul.h:115498-115503). `AzInstantPtr` has no `.len`/`.cap`, and `AzInstantPtrDestructorCallback` is a plain `{ cb }` struct with no `_Tag_NoDestructor`, so any use fails to compile. (`StringMenuItem`, also misfiled as Vec, escapes only because it has no field named `destructor`.)
  - Fix: gate on the structural Vec layout (`lang_cpp/common.rs::has_vec_layout`: `ptr`/`len`/`cap`/`destructor`) and take the NoDestructor tag from the destructor enum's IR variants.

- **[NAME-KEYED SPECIAL CASE]** `doc/src/codegen/v2/lang_c.rs:910-914` (`!is_self && super::managed_host_invoker::is_callback_wrapper(&a.type_name)`) and `971-972`: the hand-maintained 63-name `HOST_INVOKER_KINDS` list decides the C signature shape of every callback-taking function.
  - Kinds in the list get the raw fn-pointer triple plus `WithCtx` and `Struct` variants, e.g. `AzButton_setOnClick(AzButton* button, AzRefAny data, AzButtonOnClickCallbackType on_click)` (azul.h:56047).
  - Kinds outside it keep the wrapper struct by value: `AzTimer_create(AzRefAny refany, AzTimerCallback callback, AzGetSystemTimeCallback get_system_time_fn)` (azul.h:62893) and `AzThreadWriteBackMsg_create(AzWriteBackCallback callback, AzRefAny data)` (azul.h:62938). `AzTimerCallback` and `AzButtonOnClickCallback` have the same `{cb, ctx/callable: AzOptionRefAny}` layout, so a C user passes a bare function for buttons but must build a struct for timers.
  - The structural data is computed and then thrown away: `generate_functions` builds `callback_wrappers` from `callback_wrapper_info` (lines 288-298), and `generate_function_declaration` discards it with `let _ = callback_wrappers;` (line 877).
  - The shape must match the DLL export in lang_rust.rs, so it is consistent, just keyed on a name list.
  - Fix: decide the triple from `StructDef::callback_wrapper_info`, in lang_c.rs and lang_rust.rs together.

- **[SILENT SKIP]** `doc/src/codegen/v2/lang_c.rs:1348` (`for enum_def in &ir.enums {` in `generate_enum_variant_checkers`) and `1399` (same loop in `generate_union_match_helpers`): the `Az*_is<Variant>` macros and the `Az*_matchRef<V>`/`matchMut<V>` helpers are generated only for `ir.enums`.
  - The 178 monomorphized tagged unions in `ir.type_aliases` (every CSS `*Value = CssPropertyValue<T>` and every `BoxOrStatic*`) get none: azul.h has 895 unions and only 717 get helpers.
  - The shared IR also builds variant constructors only for `ir.enums` (ir_builder.rs:1693-1697). Together this means C has no function to construct or inspect e.g. `AzLayoutWidthValue`; the only way is a hand-written designated initializer. Of those 178 types, 121 have only a `_delete` and 57 have no function at all.
  - The skip is by kind, not by name.
  - Fix: also walk `type_aliases` whose `monomorphized_def` is `MonomorphizedKind::TaggedUnion { variants }` (the variant data is already there).

- **[MINOR]** `doc/src/codegen/v2/lang_c.rs:1417` (`EnumVariantKind::Struct(_) => continue, // Skip struct variants for now`), `1425-1428` (`// Only handle single-element tuple variants for now` / `if payload_types.len() != 1 { continue; }`) and `1432-1435` (array payload skip).
  - These are unreachable today. `ir_builder.rs:1152-1175` (`build_variant_kind`) only produces `Unit` or a single-element `Tuple` ("The current api.json structure doesn't support multi-element tuples or struct variants"), and the validator rejects array types.
  - They are dead "for now" branches that would silently drop helpers if the schema ever grows.
  - Fix: delete them, or make them a hard error.

- **[MINOR]** `doc/src/codegen/v2/lang_c.rs:1508` (`let has_run_destructor = false; // run_destructor field has been removed…`) leaves the `if has_run_destructor { … }` branch at lines 1515-1528 dead; its comment references "e.g., FmtArgVec".
  - Fix: delete the branch.

### SUSPECT

- **[MINOR]** `doc/src/codegen/v2/lang_c.rs:1577-1587`: the hand-written `#define AzString_fromConstStr(s) { .vec = { .ptr = (const uint8_t*)(s), .len = sizeof(s) - 1, .cap = sizeof(s) - 1, .destructor = { .NoDestructor = { .tag = AzU8VecDestructor_Tag_NoDestructor } }, } }` hard-codes the field layout of `AzString`/`AzU8Vec` and the destructor tag name.
  - Its purpose is acceptable (AZ_REFLECT's type-name string).
  - But the emitter's own note (lines 1551-1553) says every other initializer macro was removed because such macros "can easily get out of sync with the actual Rust struct definitions".
  - Maintainer's call. Fix: emit the initializer from the IR's `String` → `U8Vec` field list.

- **[MINOR]** `doc/src/codegen/v2/lang_c.rs:17-117` (C keyword list, 96 entries) vs `lang_cpp/common.rs:12` (C++ list, 102 entries).
  - The two copies have drifted: only the C++ list escapes the Windows macro names `far`, `max`, `min`, `near`, `small`.
  - api.json has 23 fields and arguments named `min`/`max` (`SliderState`, `GridMinMax`, `MeterAriaInfo`, `NumberInputState`, …). They are escaped in the C++ wrapper signatures but not in the `azul.h` structs the wrapper includes, so a `<windows.h>` without `NOMINMAX` included before `azul.h` still collides.
  - Escaping itself is acceptable; the duplication is the problem.
  - Fix: one shared list.

### Acceptable helpers (not hacks)

- `AZ_REFLECT` / `AZ_REFLECT_JSON` / `AZ_REFLECT_FULL`, `lang_c.rs:1619-1846`: RefAny upcast/downcast (`<T>_upcast`, `_downcastRef/Mut`, `Ref/RefMut_delete`).
- `AZ_STR`, `lang_c.rs:1609-1611`: generic C-string → `AzString` conversion.
- Portability preamble (restrict, ssize_t, `DLLIMPORT`, `AZ_ALIGNOF`), `lang_c.rs:323-406`.
- `Byref` twins, `lang_c.rs:1107-1153`: uniform, driven by `CodegenIR::is_value_aggregate`.
- The `"OptionRefAny"` ctx type literal in the `WithCtx` twin, lines 965 and 1059: host-invoker glue. `callback_ctx_field()` could derive it, but every wrapper uses it.
- `_Force8Bit` sentinels and u8-repr tag widths; keyword escaping via `escape_cpp_keyword_for_c`.

### Per-language impact of shared issues

- **Vec category name list:** see the first CERTAIN finding: `_empty` macros for 4 of 127 Vecs, plus the broken `AzInstantPtr_empty`.
- **`VECREF_TYPE_NAMES`:** no effect in C.
- **Enum `Default`-variant skip** (ir_builder.rs:1705): `AzAccessibilityAction_default` and `AzComponentFieldValueSource_default` do not exist. For these two tagged unions the `Default` variant can only be built with a designated initializer. For the 8 unit enums it is harmless, because the C enumerator (e.g. `AzButtonType_Default`) exists.
- **Enum-variant constructor name collisions (new, shared, same family as the `Default` skip).** Variant constructors are named `Az{Enum}_{lowerCamel(variant)}`, the namespace trait entry points also use. azul.h declares:
  - `AzPhysicalKey AzPhysicalKey_delete(void);` (64090)
  - `AzHttpMethod AzHttpMethod_delete(void);` (64166)
  - `AzVirtualKeyCode AzVirtualKeyCode_delete(void);` (65322)
  - `AzBoxDecorationBreak AzBoxDecorationBreak_clone(void);` (66233)

  These are value constructors spelled like the Drop and Clone entry points. There is no symbol clash today only because all four enums are `Copy`, so no real `_delete`/`_clone` exists. The `Default` skip patches just one name of this class.
  - Side effect: the audit's ground-truth list `derive_symbols_azul_h.txt` counts `AzBoxDecorationBreak_clone` as a Clone capability, so the true Clone count is **1114**, not 1115.
  - Fix (shared, ir_builder): give variant constructors a disjoint namespace, e.g. `Az{Enum}_variant{Name}`, or suffix the colliding ones.
- **`HOST_INVOKER_KINDS`:** decides the C signature shape (second CERTAIN finding). The 15 kinds outside it (Timer, WriteBack, RenderImage, IconResolver, …) are still fully usable from C, via the struct with `.cb`/`.ctx`. `RegisterComponentLibraryFnType` is a plain fn pointer in C (`AzAppConfig_addComponentLibrary(AzAppConfig*, AzString, AzRegisterComponentLibraryFnType)`, azul.h:62851), which is fine for C.
- **`InstantPtr`/`StringMenuItem` misfiled as Vec:** produces the broken `AzInstantPtr_empty` (above).

### Derive coverage (C)

azul.h is itself the ground truth. Every trait entry point the IR builds is declared generically: `generate_functions` walks all of `ir.functions` with no category filter (lang_c.rs:300-305). C has no operators, so the surface is the C function itself: `Az{T}_partialEq(const T*, const T*)` and so on. That gives 7/7 capabilities, 100%.

The only C-side gaps come from shared code:
- **Monomorphized aliases have no derives.** The 178 CSS `*Value`/`BoxOrStatic*` aliases export no Debug, PartialEq, PartialOrd, Ord, Hash, Clone or Default at all; 121 have only `_delete`.
  - `ir_builder.rs:1257-1266` says "For type aliases, inherit traits from the target type if not explicitly set", but the code only reads the alias's own `derive`. For example, `CaretColorValue` has no `derive`, while `CssPropertyValue` derives Debug/Clone/PartialEq/PartialOrd/Ord/Hash/Copy/Eq.
  - Fix (shared): merge the target's `derive` into the alias's traits.
- **The `_clone` false positive** described above.

---

## C++

### CERTAIN

- **[SILENT SKIP]** Iterator support is gated on the name-list-driven Vec category.
  - Gate: `if is_vec_type(struct_def) { gen.generate_vec_methods(code, struct_def, config); }` at `lang_cpp/cpp11.rs:741`, `lang_cpp/cpp17.rs:253`, `lang_cpp/cpp20.rs:718` and `lang_cpp/cpp03.rs:216`, where `is_vec_type` is `matches!(struct_def.category, TypeCategory::Vec)` (`lang_cpp/common.rs:262-264`).
  - What it gates: `begin()`/`end()`/`size()`/`empty()`/`operator[]`/`toStdVector()`, plus `toSpan()`/`operator std::span` on C++20/23 (cpp20.rs:252-315).
  - Result in azul17.hpp: exactly 4 classes get this API (`U8Vec`, `StringVec`, `GLuintVec`, `GLintVec`), while `from_std_vector` exists on all 127 Vec classes. For example `class DomVec` and `class StringPairVec` have `len()` and `from_std_vector` but no `begin()`/`size()`/`toStdVector()`.
  - The file itself documents the problem: `has_vec_layout` (common.rs:266-275) exists because "the `Vec` category is assigned from an api.json `vec_element_type` marker that only some Vecs carry … `DomVec` or `RibbonTabVec` are categorized `Regular` yet are Vecs all the same". It is used for `from_std_vector` (common.rs:787) but not for the iterator API.
  - Fix: gate all four sites on `has_vec_layout`.

- **[MINOR]** `lang_cpp/common.rs:1407-1408` claims `// Note: VecRef types ARE included in C++ - they become simple wrapper classes // that expose ptr/len as std::span (C++20+) or raw pointers (earlier)`. No such code exists.
  - `is_vecref_type` (common.rs:1414-1416) has no caller anywhere.
  - Generated `class U8VecRef` in azul20.hpp has no `span`, `begin` or `size`, only `inner()`/`ptr()` and the trait methods.
  - The VecRef category itself comes only from the shared `VECREF_TYPE_NAMES` list.
  - Fix: implement the view from the structural `ptr`+`len` fields, or delete the claim and the dead helper.

### SUSPECT

- **[MINOR]** `lang_cpp/common.rs:1640, 1661, 1689`: `arg.type_name == "String" && matches!(arg.ref_kind, ArgRefKind::Owned)` selects the arguments that get `std::string_view` overloads.
  - It keys on the literal type name rather than `TypeCategory::String` (`is_string_type`, common.rs:278). This is equivalent today because there is exactly one String type.
  - Fix: use the category.
- **[MINOR]** The duplicated and drifted C/C++ keyword lists; see the matching C item above.

### Acceptable helpers (not hacks)

- **RefAny:**
  - `RefAny::create<T>` / `type_id<T>` / `downcast_ref<T>` / `downcast_mut<T>` (common.rs:2182-2289)
  - free `azul::downcast_ref/mut` (2315-2355)
  - `detail::type_id_holder` / `type_destructor` and the `ReflectableModel` concept (2121-2172)
  - C++ `AZ_REFLECT*` shims (1986-2109) and `az_string_from_literal` (1967-1983)
- **Callback wrapping:** `template<class W, class F> inline W az_detail_wrap_cb(F f) { W w = W(); w.cb = f; return w; }` (common.rs:1956-1959). Every callback wrapper is exposed as its raw `…CallbackType` through the structural `callback_wrapper_info` (common.rs:1300-1304, 1488-1497).
- **Generic per-kind helpers:**
  - String: `c_str`/`length`/`std::string`/`string_view` (cpp11.rs:439-464, cpp20.rs:317-352)
  - Option/Result: `isSome`/`unwrap`/`value`/`toStdOptional`/`toStdExpected`, plus structured bindings (common.rs:887-998, 1151-1245; cpp20.rs:745-832)
  - `from_std_vector` via `has_vec_layout` plus the `copy_from_ptr` naming convention (common.rs:782-875)
  - unit-enum constant holders and tagged-union `Tag`/constructor holders (common.rs:337-448, 614-772)
  - `namespace ffi` aliases, callback typedef aliases, the `azul.cppm` module partition
- **Other:** C++23 deducing-`this` for methods named `with_*`/`*_with_*` (common.rs:1462-1470, a uniform naming convention). Keyword and Windows-macro escaping (common.rs:12-137).

### Per-language impact of shared issues

- **Vec category:** see the first CERTAIN finding.
- **`InstantPtr`/`StringMenuItem` misfiled as Vec:** harmless in C++. `get_vec_element_type` returns `None` (the `ptr` of `InstantPtr` is `c_void`; `StringMenuItem` has no `ptr`), so no bogus methods are emitted.
- **`Default`-variant skip:** the C++17 holder `namespace AccessibilityAction` has `Tag::Default` but no `default_()` constructor, while every other variant has one (`focus()`, `blur()`, …). `ComponentFieldValueSource` is the same.
- **`HOST_INVOKER_KINDS`:**
  - `lang_cpp/common.rs:1737-1749`: `if super::super::managed_host_invoker::is_callback_wrapper(&arg.type_name) { result.push(escaped_name); } else { … az_detail_wrap_cb<…> }`. C++ consults the name list only to mirror lang_c's name-keyed triple.
  - For users the C++ API is uniform. `Timer::create(RefAny refany, TimerCallbackType callback, …)` forwards `az_detail_wrap_cb<AzTimerCallback>(callback)` (azul17.hpp:101163-101165). No callback kind is degraded or skipped.
  - It is a silent coupling point: if lang_c and the list drift, the header stops compiling.
- **Monomorphized aliases:** the 178 `*Value` unions are reachable only as raw `ffi::X` C unions. They have no holder, no constructors and no derive functions, which are the shared gaps described under C.

### Derive coverage (C++)

**(a) How each capability surfaces**

Wrapper classes (structs, plus the synthesized Option/Result classes) get **named member functions**, because the instance-method loops include trait kinds; they filter only `!is_constructor_or_default` (cpp11.rs:228, cpp17.rs:383, cpp20.rs:1060, cpp03.rs:319):

| Capability | Member |
|---|---|
| PartialEq | `bool partialEq(const T& b) const` |
| PartialOrd | `uint8_t partialCmp(const T& b) const` |
| Ord | `uint8_t cmp(const T& b) const` |
| Hash | `uint64_t hash() const` |
| Debug | `String toDbgString() const` |
| Clone | `T clone() const` |
| Default | `static T createDefault()`, via the constructor loop's `is_constructor_or_default` |

Types without a wrapper class (unit enums, tagged unions, empty-Copy structs rendered as `using X = AzX;`) get **free functions** in `namespace azul`: `partialEq(a,b)`, `partialCmp`, `cmp`, `hash(a)`, `toDbgString(a)`, `clone(a)` and `defaultOf<AzT>()` (common.rs:2396-2554). This split is by kind, not by name, and it is computed from the same `should_skip_class`/`renders_as_type_alias` predicates, so the two paths never overlap.

**(b) Coverage numbers**

Distinct `Az*_<cap>` symbols referenced by each header vs azul.h:

| capability | azul.h | 03 | 11 | 14 | 17 | 20 | 23 |
|---|---|---|---|---|---|---|---|
| toDbgString | 2018 | 2018 | 2018 | 2018 | 2018 | 2018 | 2018 |
| partialEq | 1563 | 1563 | 1563 | 1563 | 1563 | 1563 | 1563 |
| partialCmp | 970 | 970 | 970 | 970 | 970 | 970 | 970 |
| cmp | 843 | 843 | 843 | 843 | 843 | 843 | 843 |
| hash | 871 | 871 | 871 | 871 | 871 | 871 | 871 |
| clone | 1115 | 1114 | 1114 | 1114 | 1114 | 1114 | 1114 |
| createDefault | 506 | 506 | 506 | 506 | 506 | 506 | 506 |

The single missing `_clone` is `AzBoxDecorationBreak_clone(void)`, the constructor of the unit variant `BoxDecorationBreak::Clone`, not a Clone capability. Effective coverage is 100% in all 6 dialects.

**(c) Declared but not surfaced idiomatically (by kind, not by name; SUSPECT, maintainer's call)**

- No `operator==`/`!=` anywhere; there are 0 in all `azul*.hpp`. This is deliberate for the enum free functions (the reasoning is documented at common.rs:2375-2388), but wrapper classes have none either.
- No `std::hash<T>` specialization (0 in all headers), so no wrapper can be a key in `std::unordered_map`/`unordered_set` without a user-written hasher.
- No `operator<`/`<=>`, so there is no `std::map`/`std::sort` without a user-written comparator.
- No `operator<<`.
- **Clone never backs copy construction.**
  - C++11+ deletes the copy constructor of every non-Copy class: `"    {}(const {}&) = delete;"` at cpp11.rs:677, cpp17.rs:177 and cpp20.rs:649. So `T b = a;` fails to compile even when `T` derives Clone and has `clone()`.
  - C++03's copy constructor is destructive, an auto_ptr-style move (`memset(const_cast<…>(&other.inner_), 0, …)`, cpp03.rs:437).
- Default is only a static `createDefault()`, never `T()`.

**(d) Host-side re-implementations**

None: every capability forwards to its `Az{T}_<cap>` function.

However, the **Drop** entry point is exposed incorrectly, which is a **CERTAIN correctness defect, not a hack**.
- The same instance-method loops emit a public `void delete_();` on **911 classes in every dialect**, e.g. `inline void String::delete_() { AzString_delete(&inner_); }` (azul17.hpp:95007-95009).
- It neither clears `owned_` (C++11+) nor zeroes `inner_` (C++03).
- The destructor `~String() { if (owned_) AzString_delete(&inner_); }` (azul17.hpp:28543) therefore frees the same object again, so calling `x.delete_()` guarantees a double free.
- Fix: skip `FunctionKind::Delete` in the four method loops (RAII already covers it), or emit `if (owned_) { X_delete(&inner_); owned_ = false; }`.


---

<!-- rust_python.md -->

**Source partial:** Rust + Python: codegen hack audit (partial)

Scope checked (read-only). Shipped Rust binding = `target/codegen/dll_api_external.rs` (`CodegenConfig::dll_dynamic`, UsingCAPI) + `reexports.rs` (`lang_reexports.rs`). That is exactly what `doc/src/dllgen/bundles.rs:156-185,307-318` packs into the `azul` crate. `dll_api_internal.rs` (`dll_internal`) is what `dll/src/lib.rs:213-217` includes for link-static. It was covered where it shares emitter code.
NOT shipped: `azul.rs` (`rust_public_api`, generator.rs:166 "legacy, may be removed"; its only other consumer is a lint) and `doc/src/codegen/v2/rust/` (`RustDynamicGenerator`/`RustStaticGenerator` are only re-exported at mod.rs:120 and never called). Both are dead.
Python = `lang_python.rs` → `target/codegen/python_api.rs` (PyO3, `dll/src/lib.rs:269-275`). Output was verified against the 15:35 regeneration.

---

## Rust

### CERTAIN
- **[NAME-KEYED SPECIAL CASE]** doc/src/codegen/v2/lang_reexports.rs:247-328: `let prelude_types: &[&str] = &[ "App", "AppConfig", "Dom", … "Label", "TextInput", … ];`.
  - Why: this is a hand-curated list of ~70 type names for `azul::prelude`, not derived from api.json. It has already drifted. `On`, `OptionStyledDom`, `WindowState` and `Label` no longer exist, and all four are dropped silently: lines 348-353 emit only `// WARNING: Type '{}' not found in any module` (reexports.rs:5562, 5623, 5642, 5645).
  - Fix: add a per-class `prelude: true` flag in api.json (or a module list) and make a missing entry a codegen error.
- **[SILENT SKIP]** doc/src/codegen/v2/lang_rust.rs:1854-1899: `let inner_type = match struct_def.name.strip_suffix("VecRef")` … `match inner_type.as_str() { "U8" => Some("u8"), … "GLuint" => Some("GLuint"), "GLint" => Some("GLint"), "GLfloat" => Some("GLfloat"), _ => None }`.
  - Why: the VecRef `as_slice`/`len`/`From<&[T]>`/`From<&Vec>`/`AsRef` helpers take the element type from the type NAME plus a 15-entry table, not from the `ptr` field.
  - Affected: `GLenumVecRef` (no "GLenum" entry) and `RefstrVecRef` (no `Refstr` type in the IR) are silently skipped. `ends_with("VecRef")` excludes all six `*VecRefMut` types.
  - Verified: dll_api_external.rs has no `as_slice`/`From<&[T]>` for `AzGLenumVecRef`, `AzRefstrVecRef`, `AzU8VecRefMut`, `AzGLintVecRefMut`, `AzGLint64VecRefMut`, `AzGLbooleanVecRefMut` or `AzGLfloatVecRefMut`.
  - Fix: read the element type from the `ptr` field (`FieldRefKind::Ptr/PtrMut` + `type_name`), as `generate_vec_convenience_methods` (1536-1560) already does, and emit `as_mut_slice` for `PtrMut`.
- **[MINOR]** doc/src/codegen/v2/lang_rust.rs:2223-2224: `let is_ref_type = arg.type_name.ends_with("Ref") || arg.type_name.ends_with("RefMut");`.
  - Why: a name-suffix test meant for slice refs also removes the `Into<T>` ergonomics from owned handle types: `ImageRef`, `FontRef`, `OptionImageRef`, `OptionFontRef`, `SystemColorRef`, `SystemMetricRef`, `BoxOrStaticImageRef` (e.g. dll_api_external.rs:83093 `create_with_image(image: AzImageRef)`).
  - Fix: test `TypeCategory::VecRef` (or `vec_ref_element_type`) instead of the name.
- **[MINOR]** doc/src/codegen/v2/lang_reexports.rs:209-233: `fn is_primitive_alias(name) … matches!(name, "GLuint" | "GLint" | "GLenum" | … | "c_void" | "c_char")`.
  - Why: a hard-coded alias skip list.
  - Fix: skip aliases whose `target` is a primitive (IR data).
- **[SILENT SKIP / MINOR]** doc/src/codegen/v2/lang_reexports.rs:139-140: `// Skip generic types for now (they need special handling)`.
  - Why: generic templates are never re-exported under unprefixed names, and nothing reports it.
  - Fix: re-export them (a `pub use … as Name` works for generics) or emit a diagnostic.

### SUSPECT
- **[NAME-KEYED SPECIAL CASE]** doc/src/codegen/v2/lang_rust.rs:2176-2185: `if rewrite_cb_to_fnptr && … && super::managed_host_invoker::is_callback_wrapper(&arg.type_name)`, together with the pair prologue at 4811.
  - Why: public Rust methods accept a bare `extern "C" fn` only for the 63 wrappers in `HOST_INVOKER_KINDS`. For `TimerCallback`, `WriteBackCallback`, `RenderImageCallback`, `IconResolverCallback` and the rest, the user must build the wrapper struct by hand. Example: `AzTimer::create(refany, callback: AzTimerCallback, …)` at dll_api_external.rs:96640, whose `impl AzTimerCallback` has only `clone`. examples/rust/src/anim.rs:66-68 writes `TimerCallback { cb: tick, … }`.
  - Why suspect: API ergonomics depend on a hand-maintained name list, not on the api.json shape. Every `{cb, ctx}` wrapper has the same shape.
  - Also: the comment at 2174 is stale ("e.g. OnVideoFrameCallback … have no raw variant"); `OnVideoFrameCallback` is in the list now.
  - Fix: emit the raw/WithCtx/Struct triple for every IR callback wrapper (`callback_wrapper_info.is_some()`).
- **[MINOR]** doc/src/codegen/v2/lang_rust.rs:960-1040 (`new_serde`): `{}Json::parse(s.as_str())`, `{}ResultJsonJsonParseError::Ok(json)`, `{}Json::null()`, `{}ResultRefAnyString::Ok(refany)`.
  - Why: a RefAny helper (acceptable category) that hard-codes four specific API names. It is emitted for internal bindings only; it returns early for link-dynamic at 927-932.
  - Fix: resolve these names from the IR and fail codegen if they are absent.
- **[MINOR]** Dead code: `doc/src/codegen/v2/rust/` (shared.rs:13-37 hand-writes the GL aliases) and `config.rs:650-667` (`rust_public_api` GL `type_exclude` list) only feed the unshipped `azul.rs`.
  - Fix: delete them, or mark them unshipped so they are not maintained.

### Acceptable helpers (not hacks)
- RefAny upcast/downcast glue:
  - lang_rust.rs:410-660: `Ref`/`RefMut` guards, `RefAny::new<T>`, `downcast_ref`/`downcast_mut` (via `new_c`/`is_type`/`get_data_ptr` for link-dynamic, transmute for link-static).
  - lang_rust.rs:536-548: `new_c(GlVoidPtrConst{..}, size, align, type_id, st, destructor, 0, 0)`. Positional and brittle, but it is upcast glue.
- The one String kind: `From<&str>`, `as_str`, `Display`, `PartialEq<str>`, `Deref` (lang_rust.rs:670-890).
- Vec convenience (lang_rust.rs:1536-1760): structural (`*Vec` with ptr/len/cap), so it covers all 127 Vec types.
- Option/Result helpers: structural on the Some/None and Ok/Err variants.
- Callback glue:
  - raw/WithCtx/Struct triple, Byref twins and pair prologue (lang_rust.rs:4200-4265, 4800-4950);
  - the prologue hard-codes the fn-pointer field `cb`, but all 63 host-invoker wrappers do have a `cb` field (checked against api.json).
- `transmute_helpers.rs`: generic textual rewrites of api.json `fn_body` (`self.`/`object.`/legacy lowercase receiver names, `Self::`). Fragile, but not keyed on any API name.

### Per-language impact of shared issues
- **`TypeCategory::Vec` / `CAPI_DIRECT_TYPES`:** no impact. lang_rust.rs and lang_reexports.rs never read `TypeCategory` (rg finds 0 uses). Vec helpers are structural, so `InstantPtr`/`StringMenuItem` get no bogus Vec methods.
- **Enum `Default`-variant constructor skip:** no impact. Rust users write `AccessibilityAction::Default` and `ComponentFieldValueSource::Default` directly.
- **`HOST_INVOKER_KINDS`:** decides which callback args accept a bare fn (see the SUSPECT finding above).
- **`RegisterComponentLibraryFnType`:** passed as the raw fn type; fine for Rust.

### Derive coverage
- **(a) How each derive surfaces:**
  - Shipped link-dynamic binding: `generate_abi_derived_trait_impls` (lang_rust.rs:3642-3845) implements every trait by CALLING the C entry point:
    - `Debug` → `Az{T}_toDbgString` (writes `s.as_str()`);
    - `PartialEq` → `_partialEq` (or `_partialCmp == Equal`);
    - `PartialOrd` → `_partialCmp` (or `_cmp`);
    - `Ord` → `_cmp`;
    - `Hash` → `_hash`;
    - `Default` → `_createDefault` (line 3836);
    - `Clone` (non-Copy) → `_clone` (lang_rust.rs:3868-3880);
    - `Drop` → `_delete`, for leaf types only (3887-3900).
  - Link-static binding (`dll_api_internal.rs`): delegates to the real type through a pointer cast (3525-3639).
  - Copy types use `#[derive(Copy, Clone)]`.
- **(b) Coverage in dll_api_external.rs:** 100% on all seven capabilities. Every symbol in azul.h is both declared and called:

  | Symbol | Called / in azul.h |
  |---|---|
  | `toDbgString` | 2018/2018 |
  | `partialEq` | 1563/1563 |
  | `partialCmp` | 970/970 |
  | `cmp` | 843/843 |
  | `hash` | 871/871 |
  | `clone` | 1115/1115 |
  | `createDefault` | 506/506 |

- **(c) Filter:** none beyond generic templates, which have no C symbols anyway. The filter is kind-based (IR `TypeTraits`), not name-keyed.
- **(d) Re-implementation:** none. Documented limitation: link-dynamic `Debug` is always the `{:#?}` string. The hand-written `PartialEq<str>`/`Display` on `AzString` are additions, not replacements.

---

## Python

### CERTAIN
- **[SILENT SKIP / NAME-KEYED SPECIAL CASE]** doc/src/codegen/v2/lang_python.rs:3090-3103: `const CAPI_DIRECT: &[&str] = &["U8Vec","StringVec","GLuintVec","GLintVec","RefAny",…,"InstantPtr","StringMenuItem"]; if CAPI_DIRECT.contains(&type_name) { return false; }`.
  - Why: this contradicts the comment at 3178 ("U8Vec, StringVec … ARE Python-compatible"). It makes the hand-written `FromPyObject`/`IntoPyObject` for `AzU8Vec`/`AzStringVec` (420-503) and helpers (317-354) dead: no pymethod takes or returns `AzU8Vec`/`AzStringVec`.
  - Effect: every method using U8Vec (19 fns), StringVec (15) or GLuintVec is silently absent. Verified absent: `DropDown.create`, `ListView.create`/`with_columns`, `Segmented.create`, `RawImage.decode_image_bytes`, `NamedFont.create`, `HttpRequestConfig.http_post`, `FileDialog.save_bytes`, `CallbackInfo.get_dropped_files`/`get_drag_types`, `ImageRef.get_bytes`.
  - Fix: remove the Vec names (the conversions already exist) and generate list↔Vec conversion per Vec kind.
- **[SILENT SKIP]** doc/src/codegen/v2/lang_python.rs:2815-2818: `if func.kind == FunctionKind::MethodMut && self.class_needs_unsendable(&func.class_name, ir) { return true; }`.
  - Why: every `&mut self` method of a class holding raw pointers is dropped, and the code states no reason (pyo3 `unsendable` classes can still take `PyRefMut`).
  - Effect: `Dom.add_child`/`set_css`/`add_class`/`add_callback`/`set_children`, `Button.set_on_click`/`set_button_type`/`set_icon`, `App.add_window`/`set_tray`/`set_app_icon`, `TextInput.set_text`/`set_on_text_input`, `AppConfig.add_route`/`add_component_library` are all missing. Only the `with_*` builder forms exist, which are exactly what examples/python/hello-world.py uses.
  - Total: 521 of the 3812 api.json methods on the 1902 pyclasses (13.7%) are absent with no trace in python_api.rs. The 330 `// fn … - skipped: no fn_body` comments are separate.
  - Fix: allow `&mut self` on unsendable pyclasses, and emit a skip comment and a count for anything still dropped.
- **[NAME-KEYED SPECIAL CASE / SILENT SKIP]** doc/src/codegen/v2/lang_python.rs:677-682 (`if return_type == "ImageRef" { continue; }`) and 2947-2950 (the same test in `callback_arg_is_bridgeable`).
  - Why: `RenderImageCallbackType` (custom OpenGL/image render) gets no trampoline, and every method taking it is dropped (e.g. `ImageRef.callback`). `ImageRef` does have a pyclass, so `result.extract::<AzImageRef>` would work.
  - Corroboration: examples/python/opengl.py contains no GL at all, only a CSS-rotated div.
  - Fix: delete the special case and use the generic extract path.
- **[NAME-KEYED SPECIAL CASE]** doc/src/codegen/v2/lang_python.rs:767-776: `"Update" => format!("{}::DoNothing", …), "OnTextInputReturn" => format!("{} {{ update: azul_core::callbacks::Update::DoNothing, valid: azul_layout::widgets::text_input::TextInputValid::Yes }}", …), _ => …::default()`.
  - Why: the default return value for two callback return types is hard-coded by name, including crate paths.
  - Fix: `Default` impls (or a `callback_default` annotation) in api.json, then use `::default()` uniformly.
- **[SILENT SKIP]** doc/src/codegen/v2/lang_python.rs:712-733 + 790-802 vs 2956-2962: the trampoline needs an arg type that defines `get_ctx`; without one it emits `// … ABI-completeness stub: returns the default. return default;`.
  - Why: `callback_arg_is_bridgeable` only checks for "a non-RefAny, non-primitive arg". Seven kinds are therefore stubs (`Timer`, `Thread`, `CustomE2eOp`, `SelectionTween`, `CaretTween`, `DbMerge`, `MarginBox`), yet `Thread.create(thread_initialize_data, writeback_data, callback)` and `ThreadPool.create_thread(…)` are still emitted.
  - Effect: they wire the Python callable into `invoke_py_thread_callback` (python_api.rs:70261-70271, body `return default;`). The Python thread function never runs, with no error.
  - Fix: use the same `get_ctx` predicate in `callback_arg_is_bridgeable` (drop the method with a diagnostic), or give `ThreadSender`/`TimerCallbackInfo` a `get_ctx` in api.json.
- **[NAME-KEYED SPECIAL CASE / SILENT SKIP]** doc/src/codegen/v2/lang_python.rs:3160-3171: `if type_name.ends_with("Value") && !["PixelValue","PixelValueNoPercent","FloatValue","PercentageValue","AngleValue"].contains(&type_name) { return false; }`.
  - Why: this suffix rule excludes 11 real (non-alias) types: `DbValue`, `JsonKeyValue`, `FmtValue`, `NodeTypeFieldValue`, `AttributeNameValue`, `AspectRatioValue`, `ComponentDefaultValue`, `OptionPixelValue`, `OptionDbValue`, `OptionJsonKeyValue`, `OptionComponentDefaultValue`. Every method, field and variant using them is dropped.
  - Concrete damage: `invoke_py_on_node_field_edited_callback` receives `arg5: NodeTypeFieldValue` but calls Python with only `(data, info, arg2, arg3, arg4)` (python_api.rs:73211-73248). The edited value is silently dropped by the `else { continue; }` at 921-923.
  - Fix: delete the rule; the structural generic-alias check at 3149-3155 already covers `CssPropertyValue<T>` aliases.
- **[NAME-KEYED SPECIAL CASE]** doc/src/codegen/v2/lang_python.rs:3107-3120: `POINTER_TYPE_ALIASES = ["HwndHandle","X11Visual","XWindowType","XConnection","WaylandHandle","IOSHandle","MacOSHandle","AndroidHandle"]`.
  - Why: presented as c_void aliases, but `WaylandHandle`, `IOSHandle`, `MacOSHandle` and `AndroidHandle` are real structs and `XWindowType` is an enum. `HwndHandle` and `XConnection` do not exist in api.json. Methods using the real types are wrongly dropped.
  - Fix: test in the IR for an alias whose target is `c_void` behind a pointer.
- **[NAME-KEYED SPECIAL CASE]** doc/src/codegen/v2/lang_python.rs, several hand lists that contradict each other:
  - Send-safety lists:
    - 213-228 (`PYTHON_SEND_SAFE_TYPES`, 14 names);
    - 2497-2520 (20 names);
    - 2619-2666 (44 names);
    - 2567-2575 (`PYTHON_FORCE_UNSENDABLE_ENUMS`, 7 names).
  - Contradiction: `RawWindowHandle` is both "send-safe" and "force unsendable".
  - Stale entries: `CheckThreadFinishedCallback`, `LibrarySendThreadMsgCallback`, `ThreadSenderInner`, `ThreadReceiverInner`, `ThreadInner` do not exist.
  - `CAPI_TYPE_ALIASES` (50-62): a third copy of ir_builder's `CAPI_DIRECT_TYPES`, plus a nonexistent `ParsedSvgXmlNode`.
  - Fix: one IR predicate from api.json (`is_send_safe` already exists in the IR) and one C-API-direct predicate.
- **[SILENT SKIP]** doc/src/codegen/v2/lang_python.rs:1667-1699 (`} else if is_callback_wrapper_type(ty, ir) { … builder.line("    // TODO: callback type conversion"); … unimplemented!("Option<{}> not yet supported in Python") …`).
  - Why: this stub is unreachable, because `is_python_compatible_type` (3080) rejects callback wrappers first and 1669-1671 `continue`. Tuple variants carrying a callback wrapper therefore get no constructor: `AzOptionCallback` exposes `None()` but no `Some(...)`. Struct variants are also skipped (1710-1712).
  - Fix: build the wrapper from a `Py<PyAny>` through the trampoline, as `generate_pymethod` already does for callback args (2133-2218).
- **[SILENT SKIP]** doc/src/codegen/v2/lang_python.rs:651-662 + 669-675 + 2935-2946: no trampoline when the first arg is not `RefAny`, when any arg is a pointer, or when no ctx-carrying arg exists.
  - Kinds with no trampoline (8): `IconResolverCallbackType`, `GetSystemTimeCallbackType`, `DatasetMergeCallbackType`, `RenderImageCallbackType`, `MeasureDomFn`, `ComponentRenderFn`, `ComponentCompileFn`, `RegisterComponentLibraryFnType`.
  - The dependent methods vanish with no diagnostic: `IconProviderHandle.with_resolver`/`set_resolver`/`register_*` (9), `Dom.with_merge_callback`/`set_merge_callback`.
  - Fix: emit a skip comment and a count, and model these kinds in the IR.
- **[MINOR]** doc/src/codegen/v2/lang_python.rs:3330-3348: `DIRECT_FFI_TYPES.contains(&type_name) || type_name.ends_with("Vec")`.
  - Why: the list is redundant. The suffix rule removes getters/setters for every Vec-typed field (1473-1476), every `as_<variant>()` on a Vec payload (1607), and would drop Vec extra args of callbacks (914-923). No list↔Vec conversion exists for any Vec kind.
  - Fix: generate per-kind Vec↔list conversion.
- **[MINOR]** doc/src/codegen/v2/lang_python.rs:3124-3140: `GENERIC_TYPE_ALIASES` (12 names), redundant with the structural check at 3149-3155 ("Generalized version of GENERIC_TYPE_ALIASES").
  - Fix: delete the list.
- **[MINOR]** doc/src/codegen/v2/lang_python.rs:3020-3034: `has_callback_pattern` is keyed on arg names `"data"`/`"callback"`. Its result, computed at 1825, is never read (dead).
  - Fix: delete it.
- **[MINOR]** doc/src/codegen/v2/lang_python.rs:697-700: trampoline name = `to_snake_case(&callback.name.replace("Type", ""))`, while the IR's `trampoline_name` uses `strip_suffix("Type")` (ir_builder.rs:2110-2135).
  - Why: a typedef with "Type" in mid-name would reference a nonexistent fn. This is latent; no current typedef hits it.
  - Fix: use `arg.callback_info.trampoline_name` in both places.

### SUSPECT
- **[MINOR]** doc/src/codegen/v2/lang_python.rs:1751-1764: `fn __call__(&self) -> Self` on every unit-enum instance, "docs and examples have historically also used the constructor-call spelling (`Update.RefreshDom()`)".
  - Why suspect: generic per kind, but the API surface exists to paper over example/doc drift.
- **[MINOR]** doc/src/codegen/v2/lang_python.rs:532-628: the RefAny JSON trampolines hard-code `azul_layout::json::Json`, `Json::parse`, `Json::null()`, `ResultRefAnyString`. The category (RefAny wrapping) is acceptable, but the helper is tied to specific API names.

### Acceptable helpers (not hacks)
- `PyDataWrapper`/`PyCallableWrapper`/`PyObjectWrapper` (514-530): RefAny wrapping.
- Per-typedef `invoke_py_*` trampolines driven by the IR (634-988).
- Wrapping a callable into the IR-discovered wrapper fields (2133-2218, looks up the `OptionRefAny` field).
- `AzString` `FromPyObject`/`IntoPyObject` (the single String kind).
- Keyword lists (1410-1422).
- `#[new]` for ctor `new`.
- `__str__`/`__repr__` via `Debug`.

### Per-language impact of shared issues
- **`TypeCategory::Vec` / `CAPI_DIRECT_TYPES`** (ir_builder.rs:2264-2281):
  - `uses_capi_directly()` (lang_python.rs:2435) means no pyclass is emitted for `String`, `U8Vec`, `StringVec`, `GLuintVec`, `GLintVec`, `RefAny`, `InstantPtr`, `StringMenuItem`. That is fine for `String` and `RefAny`, which become native Python objects.
  - Combined with `CAPI_DIRECT` above, `InstantPtr` and `StringMenuItem` are totally unusable from Python. `AzMenuItem` exposes only `Separator()` and `BreakLine()`; the label-carrying `String(StringMenuItem)` variant is missing, so Python cannot build a text menu entry.
- **`skip_in_python`** (ir.rs:895-904): all VecRef (23-name list), GenericTemplate, DestructorOrClone and CallbackTypedef types get no pyclass.
- **Enum `Default`-variant ctor skip:** no impact. Python emits variants straight from `EnumDef` (e.g. `fn Default()`, see the 1716-1722 comment).
- **`HOST_INVOKER_KINDS`:** not used. Python has its own trampolines: 69 are emitted, of which 7 are no-op stubs, and 8 kinds have none (listed above).
- **`RegisterComponentLibraryFnType`:** has no `callback_info` and no trampoline. `AppConfig.add_component_library` is absent; it is also `&mut self`.

### Derive coverage
- **(a) How each derive surfaces.** Python calls the REAL Rust trait impls in-process: the pyclass `inner` is the `dll_types_only` mirror, whose UsingTransmute impls delegate to the azul_core types (lang_rust.rs:2906-3040). It does not call the `Az*_` C functions, which is appropriate for a PyO3 extension compiled into libazul.
  - `Debug` → `__str__`/`__repr__` on every class (1540-1546, 1766-1772).
  - `PartialEq` → `__eq__`/`__ne__` (1333-1345).
  - `PartialOrd`/`Ord` → `__lt__`/`__le__`/`__gt__`/`__ge__` (1347-1359).
  - `Hash` → `__hash__` via `DefaultHasher` over the real `Hash` (1361-1374).
  - `Clone` → `__copy__`/`__deepcopy__` (1376-1390).
  - `Default` → `default()` staticmethod (1392-1398).
- **(b) Coverage (pyclasses with the dunder / types in azul.h):**

  | Symbol | Dunder / in azul.h |
  |---|---|
  | `toDbgString` | 1868/2018 |
  | `partialEq` | 1551/1563 |
  | `partialCmp` | 958/970 |
  | `cmp` | 831/843 |
  | `hash` | 859/871 |
  | `clone` | 1095/1115 |
  | `createDefault` | 505/506 |

  Every pyclass whose type declares the derive has the dunder (0 misses).
- **(c) Gaps.** Every missing type is a type with no pyclass at all:
  - the 150 `*VecDestructor`/destructor types (kind-based, fine);
  - `String`/`U8Vec`/`StringVec`/`GLuintVec`/`GLintVec`/`RefAny`, which become native Python values (acceptable);
  - `InstantPtr`, `InstantPtrCloneCallback`, `InstantPtrDestructorCallback`, `StringMenuItem`: excluded by the NAME-KEYED `CAPI_DIRECT_TYPES`/`CAPI_DIRECT` lists (CERTAIN, SILENT SKIP);
  - `PhysicalSizeU32` (a generic alias) and the VecRef types.
- **(d) Re-implementation:**
  - lang_python.rs:1131-1155: every non-union enum wrapper re-implements `PartialEq`/`Eq`/`Hash` as `transmute_copy::<_, u8>` of the discriminant, regardless of api.json derives. It reads 1 byte of a repr(C) discriminant, which is only correct on little-endian with fewer than 256 variants (MINOR).
  - 34 pyclasses without a `Debug` derive get a name-only `f.debug_struct("X").finish()` for `__repr__` (lang_rust.rs:2919-2923; kind-based, MINOR).


---

<!-- csharp_java_scala.md -->

**Source partial:** Bindings hack audit: C#, Java, Scala (partial)

Scope: `doc/src/codegen/v2/lang_csharp/`, `doc/src/codegen/v2/lang_java/`, `examples/{csharp,java,scala}/`, output in `target/codegen/Azul.cs` and `target/codegen/java/`. Read-only, no cargo.

Context notes:
- **Concurrent edits.** `lang_java/managed.rs` picked up an uncommitted edit from another session while this audit ran (it adds `failure_logger`; see Java SUSPECT 1). Line numbers below are for the working tree as of about 15:40. `target/codegen` was regenerated at least four times during the audit. All output numbers come from a complete regeneration, and that regeneration already includes the work-in-progress change.
- **Correction to the shared-infra count.** `HOST_INVOKER_KINDS` has **62** entries, not 63. The regex count also matched `"C"` from the `extern "C"` comment inside the list. All 62 kinds exist in the IR, and both bindings emit all 62.

---

## C#

### CERTAIN

- **[NAME-KEYED SPECIAL CASE]** `doc/src/codegen/v2/lang_csharp/wrappers.rs:1522`: `let is_app_run = func.c_name == "AzApp_run";`.
  - This one C name switches on `__AzAppLoopState.Running = true;` / `= false;` (`:1535`, `:1596`) around a single method.
  - The emitted runtime class `internal static class __AzAppLoopState` (`:379`) exists only for this check. So does the "leak with warning" branch in every owning wrapper's `Dispose(bool)` (`:1114`): `if (!disposing && __AzAppLoopState.Running)`. The comments call that branch a workaround whose proper fix is a "documented follow-up".
  - The check misses the second blocking event loop in api.json: `App::run_tray_only` (`AzApp_runTrayOnly`). While it runs, finalizer-thread deletes are not guarded.
  - Fix: mark event-loop entry points in api.json (for example `"blocks": true`) and key the flag on that attribute, not on a C symbol.
- **[SILENT SKIP]** Callback kinds outside `HOST_INVOKER_KINDS` get no managed path.
  - `managed.rs:292,343,420,451` iterate only `host_invoker_kinds(ir)`. So TimerCallback, WriteBackCallback, DbMergeCallback, GetSystemTimeCallback, CaretTweenCallback, SelectionTweenCallback, MarginBoxCallback and CustomE2eOpCallback get no `Register<Kind>`, no `<Kind>WithData<T>` and no smart builder.
  - Their wrapper class exposes only `internal TimerCallback(AzTimerCallback inner)` plus `Clone()` (output: `Azul.cs`, class `TimerCallback`).
  - As a result, `public static Timer Create(RefAny refany, TimerCallback callback, GetSystemTimeCallback get_system_time_fn)` can only be fed by hand-building the raw FFI struct with a raw delegate: no RefAny host handle, no typed delegate. Nothing reports this.
  - Kinds whose api.json has a raw-typedef factory (`RenderImageCallback.Create(AzRenderImageCallbackType)`, `DatasetMergeCallback.From`, `IconProviderHandle.WithResolver`, `AppConfig.AddComponentLibrary(string, AzRegisterComponentLibraryFnType)`) are usable in raw form. Their delegates are at least pinned (`wrappers.rs:1322-1323`: `HostInvoker.__Pin(...)`).
  - Fix: give these kinds `impl_managed_callback!` in the engine so the shared list picks them up, or emit a generic `Create(<Kind>Type fn, RefAny ctx)` for every `CallbackDataPair` struct.
- **[SILENT SKIP]** `wrappers.rs:1749-1760`: `// SKIPPED: variant {}.{} has payload — set fields directly on the FFI struct.`
  - 1341 payload-carrying union variants get no helper. Only unit variants get `public static AzX Variant()`.
  - The IR's `EnumVariantConstructor` functions are declared in the public `NativeMethods` (for example `AzOptionString_some`) but are never surfaced.
  - Fix: emit one helper per payload variant that forwards to `Az<Enum>_<variant>(payload)`.
- **[SILENT SKIP]** `functions.rs:117-179`, `should_emit_function`, drops every function on `VecRef`, `DestructorOrClone` and `GenericTemplate` structs.
  - The "declared capability" rescue at `:134-145` only re-admits `DestructorOrClone` **enums** and `Recursive` types.
  - So 14 `_toDbgString` and 12 `_clone` symbols on VecRef types, plus `_partialEq`, `_partialCmp`, `_cmp` and `_hash` on `InstantPtrCloneCallback`, `InstantPtrDestructorCallback` and `U8VecRef`, are in azul.h but missing from NativeMethods.
  - VecRef membership comes from the hard-coded `VECREF_TYPE_NAMES` list in ir_builder.rs.
  - Fix: extend the rescue to VecRef structs and DestructorOrClone structs.
- **[MINOR]** There are two disjoint Vec-to-host-array mechanisms, and neither covers all Vecs.
  - `types.rs:504` `if s.category == TypeCategory::Vec { emit_vec_to_list_cs(...) }` emits the FFI-level `ToArray()` only on `AzU8Vec`, `AzGLuintVec`, `AzGLintVec` and `AzStringVec` (the four name-listed Vecs). `InstantPtr` and `StringMenuItem` escape bogus methods only because of the `find(|f| f.name == "ptr")` guard (`:514-521`).
  - `wrappers.rs:971` `"u8" | "i8" | "bool" => ("byte", "byte", "ToByteArray")` is the wrapper-level `ToXxxArray()`. It matches literal Rust primitive names without resolving aliases, so `GLuintVec` and `GLintVec` (elements `GLuint`/`GLint`) get none. Only U8Vec, U16Vec, U32Vec and F32Vec get one.
  - Struct-element Vecs are covered structurally through `IEnumerable<T>` (67 classes).
  - Fix: use one structural Vec detector and resolve `ir.find_type_alias` before the primitive match.
- **[MINOR]** The String type is detected by name, and its layout and C names are hard-coded.
  - `wrappers.rs:1245` `a.type_name.trim() == "String"` and `:1252` `!= "String"`, while the rest of the file keys on `TypeCategory::String`.
  - `:937-939` `Marshal.ReadInt64(sPtr, System.IntPtr.Size)` hard-codes the offset of `AzString.vec.len`.
  - `:946` `NativeMethods.AzString_delete(sPtr)` and `:1349` `NativeMethods.AzString_fromUtf8(...)` hard-code the C names.
  - This is generic per String kind, but it is literal. Fix: find the struct by `TypeCategory::String`, then derive its C names and offsets from the IR.
- **[MINOR]** Literal fallbacks:
  - `wrappers.rs:750` `.unwrap_or("WindowCreateOptions")` in doc text. It is dead code, because `window_methods` is non-empty by construction.
  - `:740` `.unwrap_or_else(|| "Create".to_string())`.
  - Fix: drop both.
- **[MINOR]** `csproj.rs:50` `<Version>1.0.0</Version>` is hard-coded. ir.rs:48-51 says version-bearing emitters MUST use `ir.api_version` (0.2.0). Fix: pass `ir.api_version` to `generate_csproj()`.

### SUSPECT

- **[HELLO-WORLD-ONLY, structural]** Two factories exist to make the hello-world read nicely:
  - `wrappers.rs:721-765` `public static App Create<T>(T data, AppConfig config)` (from `app_factory_info`).
  - `wrappers.rs:643-688` `public static WindowCreateOptions Create<T>(HostInvoker.LayoutCallbackWithData<T> fn)` (from `layout_callback_factory_info`).
  - Neither names a class, but exactly one class matches each today. They exist so `examples/csharp/hello-world.cs` can write `App.Create(_model, AppConfig.Create())` and `WindowCreateOptions.Create<MyDataModel>(Layout)`.
  - The same shared detector produces a different API in Java (`App.create(T, layoutFn)` plus a hidden splice). The two bindings diverge.
  - This needs the maintainer's judgement.
- **[MINOR]** `mod.rs:230-234`: `map_type_to_csharp` returns `"IntPtr"` for any type name it cannot resolve. That is a silent, type-losing fallback, which `managed.rs:139-147` then has to re-detect. Fix: emit a diagnostic or error on unknown names.

### Acceptable helpers (not hacks)

- `managed.rs`, host-invoker glue:
  - The `NativeMethodsManaged` imports: `AzApp_setHostHandleReleaser`, `AzRefAny_newHostHandle`, `AzRefAny_getHostHandle`, and per kind `AzApp_set<Kind>Invoker` / `Az<Kind>_createFromHostHandle`.
  - The `HostInvoker` handle table and `RefanyCreate` / `RefanyGet` / `RefanyWrap`.
  - The typed `<Kind>WithData<T>` delegates, derived from each callback's IR signature. All 62 kinds are covered (62 `InvokerDelegate` and 62 `WithData<T>` in the output).
- `functions.rs:44-101`: `_ResolveAzul` DllImport resolver (library loading).
- Smart `On<Event><T>(T, <Kind>WithData<T>)` builders for every `with_on_*` from the shared detector. The output has 58 builders, plus the one layout factory that uses the same `Register<Kind><T>(fn)` line.
- Keyword `@` escaping, `new`→`Create`, the `new` modifier on System.Object shadowing, and Option/Result `AsNullable()` / `Unwrap()` from the None/Some/Ok/Err shape.

### Per-language impact of shared issues

- **`TypeCategory::Vec` name list:** FFI-level `ToArray()` exists only on the four listed Vecs (see CERTAIN). There are no bogus methods on `InstantPtr` or `StringMenuItem`.
- **Enum `Default`-variant constructor skip:** no impact. The union helper sets the tag directly, so `AzAccessibilityAction Default()` and `AzComponentFieldValueSource Default()` exist even though `AzAccessibilityAction_default` is not declared.
- **The 15 kinds outside `HOST_INVOKER_KINDS`:** raw-only or unconstructible (see CERTAIN 2).
- **`RegisterComponentLibraryFnType`:** raw only, `AddComponentLibrary(string name, AzRegisterComponentLibraryFnType register_fn)`, pinned.

### Derive coverage

**(a) Where each capability surfaces.** Everything is on wrapper classes only, because the emitters live in `emit_wrapper_class`.

| Capability | C# surface | Emitter lines |
|---|---|---|
| Debug | `public override string ToString()` via `Az{T}_toDbgString` | `wrappers.rs:889-957`; skipped for the String category and when a user method maps to `ToString` |
| PartialEq | `public override bool Equals(object)` via `_partialEq` | `:800-853` |
| Hash | `public override int GetHashCode()` via `_hash` | `:855-880`; falls back to `=> _inner.GetHashCode()` (`:883`) |
| PartialOrd / Ord | not surfaced: no `IComparable`. `is_trait_function()` skips them at `:692` and no emitter exists | — |
| Clone | instance method `Clone()` (not `ICloneable`) | via `emit_wrapper_method` |
| Default | `static CreateDefault()` | via `emit_wrapper_method` |

**(b) Coverage numbers:** distinct symbols in azul.h / declared in NativeMethods / used by the idiomatic layer.

| Capability | azul.h | Declared | Used |
|---|---:|---:|---:|
| toDbgString | 2018 | 2004 | 661 |
| partialEq | 1563 | 1560 | 535 |
| partialCmp | 970 | 967 | **0** |
| cmp | 843 | 840 | **0** |
| hash | 871 | 868 | 267 |
| clone | 1115 | 1103 | 640 |
| createDefault | 506 | 506 | 173 |

The gap between azul.h and "declared" is the category-skip gap (CERTAIN 4).

**(c) Declared but never surfaced.** The filter is **kind-based** (`has_wrapper_class`), not name-keyed.
- **[CERTAIN, SILENT SKIP]** `_partialCmp` and `_cmp` are never surfaced for any type.
- Tagged unions never get the capabilities: toDbgString 713, partialEq 519, hash 227, clone 461, createDefault 37.
- Plain value structs without a wrapper class never get them either: 413, 282, 186, 12 and 160 respectively.
- Unit enums use the C# enum built-ins. That is equivalent, except that 136 `_createDefault` are unreachable, so the default variant is not exposed.

**(d) Host re-implementations instead of the C call.**
- **[CERTAIN, MINOR]** 268 wrapper classes pair a deep `Equals` (`_partialEq`) with `GetHashCode() => _inner.GetHashCode()`. That is CLR `ValueType` hashing of the raw FFI struct, pointer fields included. Two values that are equal but have distinct heap buffers hash differently, which breaks `Dictionary` and `HashSet`. Fix: emit `Equals` only with `Hash`, or hash through `_toDbgString`.
- FFI structs (`[StructLayout] public struct AzX`: value types and tagged unions) inherit CLR `ValueType.Equals`, `GetHashCode` and `ToString` (shallow and bitwise, or the type name).
- `types.rs:457`: `Result.Unwrap()` builds its message from `Err.payload.ToString()` (the CLR default), not from `_toDbgString`.

---

## Java

### CERTAIN

- **[SILENT SKIP]** Callback kinds outside `HOST_INVOKER_KINDS` cannot be constructed outside `package com.azul`.
  - `lang_java/managed.rs:75,168,179,276,294` iterate only `host_invoker_kinds(ir)`.
  - In the output, the wrappers `TimerCallback`, `WriteBackCallback`, `DbMergeCallback`, `GetSystemTimeCallback`, `CaretTweenCallback`, `SelectionTweenCallback` and `MarginBoxCallback` have only the package-private `X(Pointer ptr)` constructor and **no public static factory**. `CustomE2eOpCallback` has only `createDefault()`.
  - So `Timer.create(RefAny, TimerCallback, GetSystemTimeCallback)`, `ThreadWriteBackMsg.create(WriteBackCallback)` and `Db.setOnConflict(DbMergeCallback)` are unusable outside `package com.azul`, with no diagnostic.
  - The raw-typedef paths are also unpinned. `RenderImageCallback.create(AzRenderImageCallbackType cb)`, `DatasetMergeCallback.from(...)`, `AppConfig.addComponentLibrary(String, AzRegisterComponentLibraryFnType)` and `WindowCreateOptions.create(AzLayoutCallbackType)` hand a JNA `Callback` to native code without keeping a reference. Compare `wrappers.rs:1444`, "Callback args: do NOT auto-substitute", with C#'s `HostInvoker.__Pin`. The native side stores the pointer, so a GC can free the trampoline while it is still in use.
  - Fix: add the engine thunks for these kinds, or emit a generic public factory plus `livePins.add(cb)` for raw callback args.
- **[SILENT SKIP]** `wrappers.rs:1948-1957`: `// SKIPPED: variant {}.{} carries a payload — set the variant`. The 1341 payload variants get no helper. The variant constructors are declared in the public `AzulNative<Module>` classes (for example `AzulNativeOption.AzOptionString_some`) but are never surfaced. Fix: same as C#.
- **[SILENT SKIP]** `functions.rs:150-211`: the same category skip and incomplete rescue as C#, with identical missing counts: 14 toDbgString and 12 clone on VecRef types, plus 3 each of partialEq, partialCmp, cmp and hash.
- **[MINOR]** Disjoint Vec helpers, as in C#:
  - `types.rs:753` `if s.category == TypeCategory::Vec { emit_vec_to_list_java(...) }` puts `toList` / `toXxxArray` only on `AzU8Vec`, `AzGLuintVec`, `AzGLintVec` and `AzStringVec`.
  - `wrappers.rs:1070` `let (arr_ty, getter, method_name) = match elem_rust.trim() { "u8" | "i8" | "bool" => ... }` does not resolve aliases. Wrapper-level `toXxxArray()` exists only on U8Vec, U16Vec, U32Vec and F32Vec, not on GLuintVec or GLintVec.
  - Fix: same as C#.
- **[MINOR]** Hard-coded String module class:
  - `wrappers.rs:926` `builder.line("AzulNativeStr.INSTANCE.AzString_delete(__sp);");` and `:1482` `AzulNativeStr.INSTANCE.AzString_fromUtf8(...)` bypass `native_class_for_class` (`functions.rs:63`), which every other call site uses.
  - `:1365` detects the String type by name: `a.type_name.trim() == "String"`.
  - `:225` and `:918` use fixed offsets (`ptr.getLong(8)`).
  - Fix: derive all of these from the `TypeCategory::String` struct.
- **[MINOR]** Wrong-type handlers are dropped silently.
  - `managed.rs:192` `public static Az{w}.ByValue register{w}(Object fn)` accepts any `Object`.
  - The per-kind invoker (`managed.rs:348-356`) does `if (fn instanceof AzulNativeManaged.<Kind>InvokerCallback) { ... }` with no else branch, next to a leftover `let _ = wrapper; // future: refine dispatch` (`:355`).
  - A handler of the wrong type compiles and then never fires, with no message. C# logs the same mismatch (`lang_csharp/managed.rs:534-538`).
  - Fix: type the parameter as `<Kind>InvokerCallback`, or log in an else branch.
- **[MINOR]** `pom.rs:29-31` hard-codes `<groupId>com.azul</groupId>`, `<artifactId>azul</artifactId>` and `<version>1.0.0</version>`. The published coordinates are `rs.azul:azul:$VERSION` (guides, `examples/java/pom.xml`), and ir.rs requires `ir.api_version`. Fix: pass `ir.api_version` and the real groupId.

### SUSPECT

- **[NAME-KEYED SPECIAL CASE, uncommitted work in progress by another session]** `managed.rs:925-968` `fn failure_logger`.
  - It looks for a wrapper argument whose type has a method keyed on name: `f.method_name == "log"` (`:937`), `f.args[2].type_name.trim() == "String"` (`:940`).
  - It picks the level variant `.find(|v| v.name == "Error")` (`:953`).
  - It is already in the regenerated output: 118 calls of the form `__arg1.log(AppLogLevel.Error.value, "azul: Callback raised " + __e);` in `AzulHostInvoker.java`.
  - It is a shape probe, but the method and variant names are literals. Fix: mark the logging method and level in api.json, or document it as a convention.
- **[HELLO-WORLD-ONLY, structural]** `wrappers.rs:603-813` `fn emit_app_factory` and `:328-360` zero-arg `create()`.
  - `emit_app_factory` emits `public static <T> App create(T data, AzulHostInvoker.LayoutCallbackWithData<T> layout)`, a hidden `__layoutCallback` field, and `private void __spliceLayoutCallback(...)`, which is called inside `run` and `addWindow` (output `App.java:44,62,90,105`).
  - The zero-arg `public static WindowCreateOptions create()` exists because the layout callback now arrives through the App.
  - api.json has neither signature. They exist so `examples/java/HelloWorld.java` can write `App.create(new Counter(), HelloWorld::layout)` and `WindowCreateOptions.create()`.
  - No names are involved (only App matches today), but it adds non-trivial hidden semantics. This needs the maintainer's judgement.
- **[MINOR]** There are three overlapping callback tiers per kind:
  - raw `AzulNativeManaged.<Kind>InvokerCallback` (`managed.rs:75-107`),
  - typed-return `AzulHostInvoker.<Kind>` (`managed.rs:405-527`, emitted only when the return type has a wrapper class, `:424`), which exists for 4 kinds (LayoutCallback, VirtualViewCallback, MapMountCallback, VideoMountCallback),
  - `<Kind>WithData<T>` (all 62).
  - The middle tier's only in-repo users are the Scala example and guide and the `WindowCreateOptions.create(AzulHostInvoker.LayoutCallback)` overload (`wrappers.rs:277-278`).
- **[MINOR]** `types.rs:315,602,647` `if (tag == 0) ...` hard-codes None and Ok as tag 0 instead of emitting the IR variant index. It holds for all 317 Option-shaped and 25 Result-shaped enums today, but it is latent.
- **[MINOR]** Unit-enum arguments surface as `int` in wrapper methods (`mod.rs:265-272` → `public Button withButtonType(int button_type)`), so callers write `ButtonType.Primary.value`. This is uniform per kind: an ergonomics gap, not a name-keyed hack.

### Acceptable helpers (not hacks)

- `AzulNativeManaged`: the releaser, RefAny host handles, and per-kind `AzApp_set<Kind>Invoker` / `createFromHostHandle`.
- `AzulHostInvoker`: the handle table, `refanyCreate` / `refanyGet` / `refanyWrap` (`managed.rs:219-263`), per-kind `register`, and 62 `WithData<T>` SAMs.
- Per-module `Native.register("azul")` classes (library loading).
- Collision renames: `String`→`AzulString` (`wrappers.rs:1978`), `close`→`closeInner` (`:2008`), and `toString`/`clone`/… → `_` suffix (`:2012`).
- Smart setters from the shared detector: 57 raw `on<Event>(Object, <Kind>InvokerCallback)` and 58 typed `<T> with<Event>(T, <Kind>WithData<T>)`.

### Per-language impact of shared issues

- **`TypeCategory::Vec` name list:** see CERTAIN (four FFI-level helpers). There are no bogus methods on `InstantPtr` or `StringMenuItem`: the `ptr` element maps to `void`, or the struct has no `ptr` field.
- **Enum `Default`-variant skip:** no impact. The union helper emits `default_()` by setting the tag.
- **The 15 kinds outside `HOST_INVOKER_KINDS`:** see CERTAIN 1.
- **`RegisterComponentLibraryFnType`:** raw and unpinned (`addComponentLibrary(java.lang.String, AzRegisterComponentLibraryFnType)`).

### Derive coverage

**(a) Where each capability surfaces.** Wrapper classes only.

| Capability | Java surface | Emitter lines |
|---|---|---|
| Debug | `toString()` via `_toDbgString` | `wrappers.rs:889-931`; the String category uses a direct vec decode instead |
| PartialEq | `equals(Object)` | `:820-855` |
| Hash | `hashCode()` | `:857-870`; falls back to identity, `return ptr == null ? 0 : ptr.hashCode();` (`:878`) |
| PartialOrd / Ord | not surfaced: no `Comparable`. `is_trait_function()` skips them at `:373` | — |
| Clone | `clone_()`, renamed at `:2012`; no `Cloneable` | 640 classes |
| Default | `static createDefault()` | 173 classes |

**(b) Coverage numbers:** azul.h / declared in `AzulNative*` / used by the idiomatic layer.

| Capability | azul.h | Declared | Used |
|---|---:|---:|---:|
| toDbgString | 2018 | 2004 | 663 |
| partialEq | 1563 | 1560 | 535 |
| partialCmp | 970 | 967 | **0** |
| cmp | 843 | 840 | **0** |
| hash | 871 | 868 | 267 |
| clone | 1115 | 1103 | 640 |
| createDefault | 506 | 506 | 173 |

**(c)** The same kind-based gaps as C#. `_partialCmp` and `_cmp` are never surfaced; tagged unions and wrapper-less value structs get none of the capabilities.

**(d) Host re-implementations instead of the C call.**
- **[CERTAIN, MINOR]** 268 wrapper classes pair a deep `equals` with the identity `ptr.hashCode()` fallback. That violates the equals/hashCode contract.
- The JNA `Structure` subclasses (`AzX`, `types.rs:716-760`, with no overrides) inherit JNA 5.14's `Structure.equals` and `hashCode`. Verified with `javap`: they compare class plus `getPointer()` identity, not values. `toString()` is JNA's reflective dump.
- `types.rs:666`: `unwrap()` reports `java.lang.String.valueOf(__err.payload)`, not `_toDbgString`.

---

## Scala

### Verification

- **There is no Scala emitter.** There is no `lang_scala` in `doc/src/codegen/v2/`, and "scala" never appears in the codegen. `doc/src` mentions it only in `docgen/mod.rs:153` (`SHIPPED_LANGUAGES`) and `dllgen/deploy.rs:1402-1420` (copies `scala/HelloWorld.scala`).
- api.json installs it with scala-cli and `--dep rs.azul:azul:$VERSION`, which is the Java binding.
- `examples/scala/build.sh` compiles against `../java/target/classes` plus JNA. CI lane `scripting-a` (`rust.yml:2885`).

**Trace of the example's calls:**

| Call in `HelloWorld.scala` | Java emitter source | Kind |
|---|---|---|
| `AzulNativeManaged.ButtonOnClickCallbackInvokerCallback` | `lang_java/managed.rs:75-107` | generic per kind |
| `AzulHostInvoker.refanyGet` / `refanyWrap` | `managed.rs:235-263` | RefAny helpers |
| `AzulHostInvoker.LayoutCallback` | `managed.rs:405-527` `emit_typed_invoker_sam` | typed-return tier, 4 kinds |
| `Button.onClick(m, ON_CLICK)` | `wrappers.rs:492-522` | raw-SAM sibling of `withOnClick`, shared detector |
| `WindowCreateOptions.create(LAYOUT)` | `wrappers.rs:270-326` | typed overload from `layout_callback_factory_info` |
| `App.create(RefAny, AppConfig)` | api.json constructor | literal |
| `withButtonType(ButtonType.Primary.value)` | int-typed enum argument | uniform per kind |

Nothing is Scala-specific or name-keyed. The example compiles against APIs that are generic per kind.

### CERTAIN

No findings in Scala itself. It inherits every Java finding unchanged.

### SUSPECT

- **[MINOR]** The example deliberately uses the lowest tiers. It implements the raw invoker SAM, calls `refanyGet` by hand with a `match`, and writes `outPtr.setInt(0, Update.RefreshDom.value)` itself. The Java example uses the typed `withOnClick(data, fn)` and `App.create(T, layoutFn)` path.
- **[MINOR]** `doc/guide/en/hello-world/scala.md:140` says the program must live in `package com.azul` "because some of the raw-pointer plumbing in the generated sources is package-private".
  - The calls in the example itself are all public, so that claim looks stale for this example.
  - It is true, however, for the callback kinds outside `HOST_INVOKER_KINDS` (package-private constructors, Java CERTAIN 1).
  - Sharing the binding's package is therefore the only way Java or Scala code can reach those APIs.

### Acceptable helpers (not hacks)

See Java.

### Per-language impact of shared issues

Identical to Java.

### Derive coverage

- Scala `==` / `##` / `toString` call the Java overrides. So `_partialEq`, `_hash` and `_toDbgString` reach Scala for the wrapper classes (535, 267 and 663 respectively).
- Scala inherits the identity-hash fallback and the pointer-identity `equals` of JNA `Structure` values.
- There is no `Ordering` or `Comparable`, because `_partialCmp` and `_cmp` are never surfaced.


---

<!-- kotlin_lua.md -->

**Source partial:** Bindings hack audit: Kotlin and Lua (partial)

Scope: `doc/src/codegen/v2/lang_kotlin/`, `doc/src/codegen/v2/lang_lua/`. Output checked:
`target/codegen/azul.lua` and `target/codegen/kotlin/Azul.kt` from the 15:36 codegen run. Their
sizes match the 15:29 run; codegen was re-run several times during the audit. Output line numbers
refer to that run. Examples: `examples/lua/hello-world.lua`, `examples/kotlin/HelloWorld.kt`.

Summary: neither emitter has `if name == "Button"`-style special cases. The API-name hits
(Lua 71, Kotlin 43) are almost all comments, RefAny/host-invoker glue or IR-structural detectors.
The real problems are:

- **Callback shapes.** Callback support works for the two shapes the hello-world uses: the
  `with_on_*` wrapper-struct setters and `WindowCreateOptions.create`. It degrades for the rest.
- **Silent category skips.** Whole categories are dropped without a diagnostic: constants, and in
  Lua the `is_boxed_object` types.
- **Shared Vec-category issue.** Some helpers depend on `TypeCategory::Vec`, which is assigned from
  a name list (see the shared findings).

---

## Lua

### CERTAIN

- **[SILENT SKIP]** `doc/src/codegen/v2/lang_lua/wrappers.rs:1355-1367`: typedef-form callback args
  of host-invoker kinds pass only the fn pointer:
  `"{indent}{n} = _{n}_cb.cb\n"`
  - **What happens.** The emitter's own comment (1339-1343) says:
    "ANY ctx is dropped at the C boundary … Functions in this shape need a special-case fixup
    elsewhere". Only `WindowCreateOptions.create` (special case 2) and `*Callback.create`
    (special case 1) got a fixup.
  - **Affected API.** `Dom:with_callback`, `Dom:add_callback`, `NodeData:add_callback` and
    `StringMenuItem:with_callback`: azul.lua:108810, 108990, 106960, 107219, e.g.
    `local _callback_cb = azul._register_callback('Callback', callback); callback = _callback_cb.cb`.
  - **The C side.** These call `AzDom_withCallback(…, AzCallbackType callback)`. api.json's
    `fn_body` is `Callback::create(callback)`, which is "just a function pointer (ctx = None)"
    (layout/src/callbacks.rs:1101-1113).
  - **Result.** The engine thunk does `let ctx = info.get_ctx(); … _ => return $default`
    (core/src/host_invoker.rs:442-450). The Lua closure never runs; the event silently returns
    `DoNothing`. The registered host handle (the wrapper's ctx RefAny) also leaks in `_lua_handles`.
  - **Why it is a hack.** Only the shapes the hello-world uses work (`with_on_click` takes the
    wrapper struct; `WindowCreateOptions.create` is special-cased). The generic DOM event-callback
    API is dead in Lua.
  - **Likely cross-language.** The root cause is shared: api.json declares these 4 args as raw
    `CallbackType`, and `managed_c_symbol` only adds `…Struct` twins for wrapper-struct args.
  - Fix: have the dll export `WithCtx`/`Struct` twins for typedef-form args of host-invoker kinds
    too, or change the 4 api.json args to `Callback`; then drop both special cases.
- **[SILENT SKIP]** `doc/src/codegen/v2/lang_lua/wrappers.rs:1368-1382`: every callback kind
  outside `HOST_INVOKER_KINDS` goes through
  `"{indent}{n} = azul.pin_callback('{ty}', {n})\n"`.
  - **ABI mismatch.** The branch does not check `abi_takes_wrapper`. Example: azul.lua:106577
    `callback = azul.pin_callback('AzTimerCallbackType', callback)` feeds
    `AzTimer_create(AzRefAny, AzTimerCallback callback, AzGetSystemTimeCallback)` (azul.h:62893),
    so a function pointer is passed where a `{cb, ctx}` struct is expected by value.
  - **Also unbuildable.** `pin_callback` itself cannot build these callbacks under LuaJIT: their
    typedefs pass RefAny/CallbackInfo by value (managed.rs:389-395).
  - **Same shape elsewhere.** `Db:set_on_conflict` (azul.lua:95412, `AzDbMergeCallback`),
    `Dom`/`NodeData` merge callbacks, `ThreadWriteBackMsg.create`.
  - **Net effect.** Timer, WriteBack, DbMerge, DatasetMerge, RenderImage, IconResolver, CaretTween,
    SelectionTween, MarginBox, CustomE2eOp and GetSystemTime callbacks are unusable from Lua (runtime
    error). There is no generation-time diagnostic.
  - Fix: derive "host-invoker capable" from IR/api.json metadata instead of the shared name list.
    For wrapper-struct ABIs, build `{cb = pinned, ctx = none}` or emit a generation-time warning.
- **[SILENT SKIP]** `doc/src/codegen/v2/lang_lua/wrappers.rs:100-110`: `should_emit_struct`
  excludes `TypeCategory::Boxed`. The module doc (line 34) calls these "internal heap wrappers".
  - **What is actually skipped.** The 7 `is_boxed_object` classes include public API:
    `GlContextPtr` (225 api fns), `ImageRef` (20), `Texture` (11), `FontRef` (5), `Svg` (2).
    That is 263 functions.
  - **Output.** azul.lua has no `azul.ImageRef/FontRef/Svg/GlContextPtr/Texture` table, methods
    table or metatype. The only `C.AzImageRef_*` reference is an Option unwrap at azul.lua:112954.
    Kotlin emits wrappers for all five.
  - Fix: drop `Boxed` from the Lua exclusion list (Kotlin shows it is not needed), or restrict it to
    the truly opaque `GLsyncPtr`/`GlVoidPtrConst`.
- **[SILENT SKIP]** Constants: `lang_lua/` never reads `ir.constants`.
  - The C header emits all 1,430 of them as `#define AzX_NAME value` (lang_c.rs:1328-1331).
  - `cdef.rs:279-283` drops every `#` line: "Drop other preprocessor lines unconditionally".
  - The module doc at `cdef.rs:21-23` claims simple `#define`s "are turned into `enum { X = Y };`".
    No such code exists.
  - `rg ACCUM_ALPHA_BITS azul.lua` finds nothing.
  - Fix: emit `azul.<Class>.<NAME> = <value>` (or a cdef `enum {}`) from `ir.constants`, and fix the
    doc.
- **[NAME-KEYED SPECIAL CASE]** `doc/src/codegen/v2/lang_lua/wrappers.rs:608-621` and `1081-1094`:
  `.map(|c| c.callback_wrapper_name == "ThreadCallback")` replaces `Thread.create` and
  `ThreadPool:create_thread` with `error('ThreadCallback from Lua is unsupported; use the writeback
  pattern', 2)`.
  - The rationale (worker thread vs single-threaded VM) is valid. But thread affinity is encoded as
    one hard-coded kind name. It lives in a comment in `managed_host_invoker.rs:121-130`, not in the
    IR.
  - Fix: carry `fires_on_worker_thread` in api.json/IR (or a shared const next to
    `HOST_INVOKER_KINDS`) and key on it.
- **[MINOR]** `doc/src/codegen/v2/lang_lua/wrappers.rs:256` `if class == "String" {` (adds
  `to_lua_string`) and `:369` `… && s.name != "String"` (no `__tostring`). These key on the name.
  The same file already uses `TypeCategory::String` at 884-889.
  Fix: `s.category == TypeCategory::String`.
- **[MINOR]** `doc/src/codegen/v2/lang_lua/wrappers.rs:651-653`:
  `Some(rt) if rt.starts_with("Option") => Some(":to_opt()")` / `starts_with("Result")`.
  - Auto-unwrap keys on a name prefix, while the methods it calls are emitted from variant shape
    (475-523): two predicates for one concept.
  - No mismatch in the current output (732 call sites, all resolve).
  - Fix: use `TypeCategory::Option`/`Result`, or the same variant-shape probe.
- **[MINOR]** `doc/src/codegen/v2/lang_lua/wrappers.rs:1324`:
  `let abi_takes_wrapper = !a.type_name.ends_with("Type");` is a name-suffix heuristic.
  Fix: `ir.find_struct(ty)` versus `ir.callback_typedefs` lookup.
- **[MINOR]** `doc/src/codegen/v2/lang_lua/cdef.rs:225` / `:233`: name-keyed macro filters
  (`#define AZ_REFLECT`, `#define AzString_fromConstStr`). These are redundant with the generic
  "`#define … \` continuation" rule at 241. Also, the comment-prefix list at 302-310
  (`"/* Empty Vec "`, `"/* Full reflection"`, …) couples to lang_c.rs comment text.
  Fix: delete the two name filters; strip comments generically.

### SUSPECT

- `doc/src/codegen/v2/lang_lua/wrappers.rs:1206-1262` (special case 2, via
  `layout_callback_factory_info`).
  - Emits `azul.WindowCreateOptions.create(fn)` as
    `_register_callback('LayoutCallback', fn)` + `C.AzWindowCreateOptions_createDefault()` +
    `_opts.window_state.layout_callback = _cb` (azul.lua:111809-111815). api.json's
    `AzWindowCreateOptions_create(layout_callback)` is never called.
  - It is structurally derived (no names), but WindowCreateOptions is the only match, and it is the
    hello-world's entry point.
  - It exists because of the same ctx-drop defect as the first CERTAIN item. If typedef-form args
    get ctx-carrying twins, this branch and special case 1 (1165-1204) become unnecessary.
    Maintainer's call.

### Acceptable helpers (not hacks)

- **Library init.** FFI-module probe (mod.rs:352-386); lazy cdef + memoizing `ffi.load('azul')`
  proxy + Byref routing (mod.rs:406-482, IR `is_value_aggregate`).
- **Host-invoker glue** (managed.rs, all from `host_invoker_kinds(ir)`): per-kind `ffi.cast`
  invokers, `_register_callback`, releaser, `pin_callback`.
- **RefAny helpers.** `refany_create/refany_get/_refany_unwrap/_refany_arg`, keyed on
  `is_refany_type` (IR category).
- **Generic per-kind conversions.** `_az_string`, `String.from_lua`, `_consume`.
- **Generic idioms.** `:with(opts)`/`_apply_opts` (generic cdata recursion) and CC-6
  "void mutator returns self" (wrappers.rs:674-684, uniform).
- **Smart-setter aliases.** `Button_methods:on_click(data, fn)` → `self:with_on_click(...)`
  (wrappers.rs:215-233, from `smart_callback_setter_info`; 121 matches, generic).
- **Special case 1.** `Callback.create`/`LayoutCallback.create` → `_register_callback` passthrough
  (wrappers.rs:1165-1204).
- **Other.** Lua reserved-word escaping (1390-1400); rockspec uses `ir.api_version`.
- **Hello-world.** examples/lua/hello-world.lua uses only these generic paths plus the WCO factory
  above.

### Per-language impact of shared issues

- **`TypeCategory::Vec` name list.** `to_lua_array` (wrappers.rs:270) exists for exactly 6 types:
  InstantPtr, U8Vec, GLuintVec, GLintVec, StringVec, StringMenuItem.
  - Two are bogus:
    - `InstantPtr_methods:to_lua_array` (azul.lua:98422) reads the nonexistent `self.len`.
    - `StringMenuItem_methods:to_lua_array` (azul.lua:107230) treats the `label: String` field as an
      element array.
  - The same file probes Vecs structurally for `__len` (wrappers.rs:389-393), and that probe hits
    127 types. So 123 real Vecs get `#v` but no table conversion.
  - Fix: use the structural probe for both.
- **Enum `Default`-variant skip** (ir_builder.rs:1705). `azul.AccessibilityAction` (azul.lua:115996)
  lists `Tag.Default` but has no `default` constructor next to its 26 others. The same applies to
  `ComponentFieldValueSource`.
- **15 non-host-invoker kinds.** See the second CERTAIN item (emitted, but broken at runtime).
  Typed support: all 62 `HOST_INVOKER_KINDS` get `_register_callback`. The shared list has 62
  names; the "63" figure came from a regex that also matched a stray `"C"`.
- **`RegisterComponentLibraryFnType`** (no callback_info): passed as a raw varargs value; no
  pinning.

### Derive coverage

Counts are distinct symbols (surfaced in the idiomatic layer / present in azul.h). The cdef layer
declares all of them lazily in `__az_fn_decls`, so coverage below is only about the idiomatic
surface.

| Capability | Lua surface | Surfaced / azul.h | Missing by kind |
|---|---|---|---|
| Debug `_toDbgString` | `:toString()` + `__tostring` (wrappers.rs:194-207, 366-383) | 1057 / 2018 | union 713, unit enum 227, struct 20, alias 1 |
| PartialEq `_partialEq` | `__eq` (wrappers.rs:344-364) | 811 / 1563 | union 519, unit enum 226, struct 6, alias 1 |
| PartialOrd `_partialCmp` | none (no `__lt`/`__le`) | 0 / 970 | all |
| Ord `_cmp` | none | 0 / 843 | all |
| Hash `_hash` | none (no `:hash()`) | 0 / 871 | all |
| Clone `_clone` | `:clone()` for structs + unions | 1099 / 1115 | struct 14, unit enum 2 |
| Default `_createDefault` | static `createDefault` for structs + unions | 370 / 506 | unit enum 136 |

**(c) Why the gaps exist.** The filters are kind-based, not name-keyed:
- `emit_data_enum_wrapper` (wrappers.rs:435-570) binds only Method/MethodMut/DeepCopy. The 713
  tagged unions therefore get no `__eq`/`__tostring`: 469 union metatypes are `{ __index, __gc }`
  only.
- Unit enums are plain number tables. Native equality is fine, but there is no `tostring` or
  default accessor.
- The 20 structs are the VecRef/DestructorOrClone/Boxed exclusions (FontRef, ImageRef, Svg,
  GlContextPtr, …).
- `partialCmp`/`cmp`/`hash` are never surfaced anywhere.

**(d) Host-side re-implementations of derives:** none found. Every surfaced capability calls the C
symbol.

Gaps:
- **[SILENT SKIP]** No `__lt`/`__le` (partialCmp/cmp) and no `hash`. Fix: emit
  `__lt/__le = C.Az{T}_partialCmp` in the struct and union metatypes, and a `:hash()` method.
- **[MINOR]** Unions miss `__eq`/`__tostring`. Fix: share the struct metatype-clause code with
  `emit_data_enum_wrapper`.

---

## Kotlin

### CERTAIN

- **[SILENT SKIP]** Constants: `lang_kotlin/` (and `lang_java/`, whose helpers it reuses) never
  reads `ir.constants`. 0 of azul.h's 1,430 constants appear in Azul.kt
  (`rg ACCUM_ALPHA_BITS` finds nothing).
  Fix: emit `object <Class>Constants { const val NAME = … }` from `ir.constants`.
- **[SILENT SKIP]** Callback kinds outside `HOST_INVOKER_KINDS` have no idiomatic entry point.
  - Example: `@JvmStatic fun create(refany: RefAny, callback: TimerCallback, get_system_time_fn:
    GetSystemTimeCallback): Timer` (Azul.kt:161056). It takes a `TimerCallback` wrapper whose only
    constructor is `class TimerCallback internal constructor(internal val ptr: Pointer, …)`
    (Azul.kt:135675).
  - There is no `AzulHostInvoker.registerTimerCallback`, and `AzTimerCallback.cb` is a bare
    `Pointer?` (Azul.kt:82258; callback-typedef fields collapse to `Pointer?` at
    lang_kotlin/mod.rs:1037-1043).
  - Users must hand-build a JNA function pointer. The Gradle example sidesteps `internal` only
    because it compiles Azul.kt into the same module.
  - Same for WriteBack (`ThreadWriteBackMsg.create`), DbMerge, DatasetMerge, RenderImage,
    IconResolver, CaretTween, SelectionTween, MarginBox, CustomE2eOp, GetSystemTime.
  - Fix: generic "raw-callback" factory per callback wrapper (JNA callback → `CallbackReference`
    pointer + `ctx = None`), or extend host-invoker coverage from IR metadata.
- **[MINOR]** `doc/src/codegen/v2/lang_kotlin/gradle.rs:29`:
  `pub const DEFAULT_AZUL_VERSION: &str = "0.2.0";`
  - This is a hard-coded release number, while ir.rs:48-51 says version-bearing emitters "MUST use
    [`api_version`] instead of hardcoding a release number".
  - generator.rs:445 calls `generate_build_gradle_kts()` without the IR.
  - Fix: `generate_build_gradle_kts(&ir.api_version)`, like rockspec/gemspec/package.json.
- **[MINOR]** `doc/src/codegen/v2/lang_kotlin/wrappers.rs:1126-1128`:
  `a.type_name.trim() == "String" && matches!(a.ref_kind, ArgRefKind::Owned)`.
  - Auto-string conversion keys on the name, while the same file uses `TypeCategory::String` at
    140, 155 and 504.
  - Fix: `ir.find_struct(..).category == TypeCategory::String`.
- **[MINOR]** `doc/src/codegen/v2/lang_kotlin/wrappers.rs:1251-1257`: when a type has `partialEq`
  but no `hash`, it emits `override fun hashCode(): Int = ptr.hashCode()`.
  - That pairs value equality (`AzX_partialEq`) with identity hashing. On 268 classes, two `equals`
    objects hash differently, which breaks the equals/hashCode contract (HashMap/HashSet).
  - This is a host-side substitute for a missing derive.
  - Fix: constant hash (e.g. `0`), or hash `toDbgString`, when only PartialEq exists.
- **[SILENT SKIP / MINOR]** `doc/src/codegen/v2/lang_kotlin/wrappers.rs:1903-1913`: union payload
  variants get only
  `"// SKIPPED: variant {}.{} carries a payload — set the variant"` (1,341 variants in Azul.kt).
  - The C constructors `Az<Enum>_<variant>(payload)` are declared in the JNA objects but never
    wrapped.
  - Unit variants are built by poking the tag byte plus `u.setType(...)` (1880-1901, 723 helpers)
    instead of calling the exported C constructors.
  - Fix: wrap the `EnumVariantConstructor` functions like any static factory.
- **[MINOR]** Dead code with hard-coded arity: `doc/src/codegen/v2/lang_kotlin/mod.rs:1085-1088`
  `return format!("{} {{ _, _, _, _ -> }}", base);`. This 4-arg lambda default for callback
  typedefs is unreachable, because `ref_kind_kt_field` maps those fields to `Pointer?` first
  (1041-1043). Fix: delete it.

### SUSPECT

- **`emit_kt_default_options_factory`** (`doc/src/codegen/v2/lang_kotlin/wrappers.rs:720-753`)
  emits a zero-arg `@JvmStatic fun create(): WindowCreateOptions`, documented "Equivalent to
  `createDefault()`".
  - api.json's WindowCreateOptions has only `create(layout_callback: LayoutCallbackType)`.
  - This is API surface added so the hello-world's `WindowCreateOptions.create()`
    (examples/kotlin/HelloWorld.kt) compiles alongside `App.create(model, ::layout)`.
  - It is structurally gated (`app_factory_info`), but the flavour is HELLO-WORLD-ONLY.
  - Suggest: have the example call `createDefault()` and drop the duplicate.
- **App factory** (`doc/src/codegen/v2/lang_kotlin/wrappers.rs:798-948`):
  `App.create(data: T, fn: LayoutCallbackWithData<T>)` plus
  `__spliceIntoWindowCreateOptions` (Azul.kt:105399ff).
  - At `run`/`addWindow` it compares `opts.window_state.layout_callback.cb` with the cached
    engine-default `cb` (`if (__slot.cb != __WindowCreateOptionsDefaultCb) return`), then
    byte-splices a freshly registered callback.
  - It is built only from allowed pieces (RefAny wrap + callback registration) and derived
    structurally; no names.
  - But it adds a behavioural policy that api.json does not have, and the only match is
    `App`/`WindowCreateOptions`. Maintainer's call.
- **WindowCreateOptions smart factories** (`doc/src/codegen/v2/lang_kotlin/wrappers.rs:663-713`):
  `WindowCreateOptions.create(fn)` ×2 (raw SAM + typed `AzulHostInvoker.LayoutCallback`) via
  `layout_callback_factory_info`.
  - They bypass `AzWindowCreateOptions_create`. Structural; WindowCreateOptions is the only match.
  - Same root cause as the Lua special case (typedef-form arg drops ctx).
- **Note, not a hack.** Typed callback coverage is generic: 62/62 host-invoker kinds get
  `<Kind>InvokerCallback` and `<Kind>WithData<T>` (managed.rs:419-473). 4 kinds get typed-return
  SAMs. There are 59 typed + 58 raw smart setters.
- **Note, not a hack.** Typedef-form callback args (`Dom.withCallback(event, data, callback:
  AzCallbackType)`, Azul.kt:172231) surface the bare JNA interface `fun callback(arg0:
  AzRefAny.ByValue, arg1: AzCallbackInfo.ByValue): Int` (Azul.kt:88610). This works as a direct
  C callback but is untyped. It is the same api.json root as the first Lua CERTAIN item, with no
  silent failure here.

### Acceptable helpers (not hacks)

- **Library init.** `Native.register(<object>::class.java, "azul")` per module
  (mod.rs:274-293).
- **Host-invoker glue.** `AzulNativeManaged` + `AzulHostInvoker`: handles map, releaser,
  per-kind invokers/`register<Kind>`, typed SAM bridges (managed.rs, all from
  `host_invoker_kinds(ir)`).
- **RefAny helpers.** `refanyCreate`/`refanyGet`/`refanyWrap`.
- **Generic per-kind conversions.** Auto-String, auto-wrapper-class `.ByValue` overlays,
  Option→nullable / Result→unwrap by variant shape (wrappers.rs:69-131), Vec
  iterator/`toXxxArray` by structural probe (wrappers.rs:370-390, 592-598), Cleaner-based
  ownership.
- **Renames and escaping.** Keyword/collision renames (`new`→`create`, `close`→`closeInner`,
  `toString_`, `String`→`AzulString`; wrappers.rs:1922-1967, mod.rs:1134-1139) and
  keyword escaping.

### Per-language impact of shared issues

- **`TypeCategory::Vec` name list.** The FFI-struct accessors (`mod.rs:782`
  `if s.category == TypeCategory::Vec { emit_vec_to_list_kt(...) }`) exist on exactly 4 classes:
  AzU8Vec, AzStringVec, AzGLuintVec, AzGLintVec.
  - The InstantPtr/StringMenuItem misfiles are harmless here (no `ptr` field / void element).
  - The wrapper classes use a structural probe instead: 75 of 127 Vec wrappers get
    `iterator()`/`toXxxArray()`.
  - The other 52 get nothing. That includes GLintVec/GLuintVec, because the primitive table at
    wrappers.rs:1299-1307 does not resolve the `GLint`/`GLuint` aliases. It also includes every
    enum/union/POD-element Vec (VirtualKeyCodeVec, CssPropertyVec, DomIdVec, …). This filter is
    kind-based.
- **Enum `Default`-variant skip.** Not affected: union unit variants are built from tags by
  `XHelpers`.
- **15 non-host-invoker kinds.** See the second CERTAIN item.

### Derive coverage

| Capability | Kotlin surface | Surfaced / azul.h (JNA-declared) | Missing by kind |
|---|---|---|---|
| Debug | `override fun toString()` via `_toDbgString` (wrappers.rs:1262-1293) | 663 / 2018 (2004) | struct 414, union 713, unit enum 227, alias 1 |
| PartialEq | `override fun equals` (wrappers.rs:1208-1238) | 535 / 1563 (1560) | struct 282, union 519, unit enum 226 |
| PartialOrd / Ord | none (no `Comparable<T>`) | 0 / 970 and 0 / 843 | all |
| Hash | `hashCode()` via `_hash` (1240-1250); else identity (see CERTAIN) | 267 / 871 (868) | struct 186, union 227, unit enum 190 |
| Clone | `fun clone()` on wrapper classes | 640 / 1115 (1103) | union 461, struct 12, unit enum 2 |
| Default | companion `createDefault()` | 173 / 506 | struct 160, union 37, unit enum 136 |

**(b) JNA layer.** It declares nearly all symbols. The small gaps (for example 14 `toDbgString`,
12 `clone`) are VecRef-category functions dropped by `should_emit_function`
(mod.rs:205-215).

**(c) Why the gaps exist.** The filters are kind-based, not name-keyed:
- Only `has_wrapper_class` structs surface derives. Of the 414 structs missing `toString`, 374 are
  `_delete`-less POD with no API functions, 27 have only static functions, and 13 are VecRef or
  String (String decodes bytes on purpose).
- Tagged unions never get a wrapper class, so they never surface `toString`/`equals`/`hashCode`/
  `clone`/`createDefault`.
- Unit enums are Kotlin `enum class` (native `equals`/`hashCode`/`toString`/`compareTo` by
  ordinal). These are semantically equivalent host re-implementations: acceptable.

**(d) Host-side re-implementations.**
- The identity `hashCode()` fallback (CERTAIN above).
- The union unit-variant helpers, which write the tag instead of calling the C constructors.

Gaps:
- **[SILENT SKIP]** `partialCmp`/`cmp` never surfaced. Fix: implement `Comparable<X>` via
  `_cmp` (or `_partialCmp`) when `traits.is_ord` / `is_partial_ord`.
- **[MINOR]** Unions have no idiomatic wrapper. Fix: emit a union wrapper class that reuses the
  struct wrapper's derive emitters.


---

<!-- ruby_node.md -->

**Source partial:** Ruby + Node: bindings hack audit (partial)

Scope: `doc/src/codegen/v2/lang_ruby/` (3,160 lines, read in full) and `doc/src/codegen/v2/lang_node/` (3,702 lines, read in full).

Generated output: `target/codegen/azul.rb` and `target/codegen/node/azul.js`. Something re-ran `codegen all` three times while I worked. Every count below comes from a byte-identical snapshot taken at 15:36, kept in `scratchpad/partials/ruby_node_snapshot/`.

Examples: `examples/ruby/hello-world.rb`, `examples/node/hello-world.js`.

**Headline.** Neither emitter has a branch keyed on a hello-world API name: no `"Button"`, `"Dom"`, `"App"` or `"WindowCreateOptions"` compare anywhere. Everything the two hello-worlds call is either generic or structurally derived:
- `Dom.create_p_with_text` / `with_css` / `with_child`
- `Button#on_click` / `withOnClick`
- `WindowCreateOptions.create(_with_layout)`
- `App.create` / `run`
- `RefAny.wrap` / `refanyCreate`

The real problems are of three kinds:
- hand-written layout or symbol shortcuts that are wrong;
- callback support that stops at the 62 host-invoker kinds and says nothing about the other 15;
- derive capabilities that are bound but not surfaced, or surfaced broken.

---

## Ruby

### CERTAIN

- **[SILENT SKIP]** `lang_ruby/types.rs:449`: `if s.category == TypeCategory::Vec { emit_vec_to_a_ruby(builder, s); }`
  - **Why:** `Native::Az*#to_a` is gated on the IR category. ir_builder assigns that category only by name (the `U8Vec/StringVec/GLuintVec/GLintVec` list, plus the misfiled `CAPI_DIRECT_TYPES`).
  - **Result:** azul.rb has exactly 5 `def to_a`: `AzInstantPtr`, `AzU8Vec`, `AzGLuintVec`, `AzGLintVec`, `AzStringVec`. The other ~123 `*Vec` Native structs get none.
  - **InstantPtr:** its `to_a` does `return [] if self[:len].zero? …` on a struct whose layout is `:ptr, :clone_fn, :destructor, :run_destructor`. There is no `:len`, so ruby-ffi raises.
  - **StringMenuItem:** escapes only because it has no `ptr` field (`emit_vec_to_a_ruby` returns early at `types.rs:458`).
  - **Fix:** gate on the layout detector already used for `each` (`wrappers.rs:440` `detect_vec_elem_type`), or delete the Native-level `to_a`. The wrapper classes already `include Enumerable` on all 127 Vecs.

- **[SILENT SKIP]** `lang_ruby/types.rs:484-486`: `"size = self.class.const_get(:LAYOUT).fields.first.size rescue 0"` then `"(0...self[:len]).map { |i| self[:ptr] + i * size }"`
  - **Why:** this is a stub fallback for struct elements, and it returns garbage.
    - `FFI::Struct` has no `LAYOUT` constant. Checked against installed ruby-ffi 1.15.5, the gemspec's `~> 1.15`: `FFI::Struct.const_defined?(:LAYOUT) == false`. So `size` is always 0.
    - `AzStringVec#to_a` therefore returns `len` copies of the same raw pointer.
    - Even if the constant existed, it would read the size of the first field (`ptr`), not the element size.
  - **Fix:** use `Native::Az<Elem>.size` plus `_clone`, as `emit_rb_each_if_vec` does (`wrappers.rs:407-416`), or drop the method.

- **[SILENT SKIP]** callback kinds outside `HOST_INVOKER_KINDS` get no wrapping and no diagnostic.
  - **Code:**
    - `lang_ruby/wrappers.rs:800`: `if super::super::managed_host_invoker::HOST_INVOKER_KINDS.contains(&wrapper) {` (no else branch).
    - `lang_ruby/wrappers.rs:847-849`: `if a.callback_info.is_some() { return name.to_string(); }`
  - **Result:** the user value goes raw into a by-value struct slot. Generated `Timer.create`:
    ```ruby
    Native.az_timer_create(Native.az_ref_any_clone(refany.ptr), callback, get_system_time_fn)
    ```
    against:
    ```ruby
    attach_function :az_timer_create, :AzTimer_create, [AzRefAny.by_value, AzTimerCallback.by_value, AzGetSystemTimeCallback.by_value], …
    ```
    - A Proc is rejected with a bare ruby-ffi TypeError.
    - A wrapper instance is not unwrapped either (no `.ptr`).
  - **Affected API:** `Timer.create` / `Timer.invoke` (TimerCallback, GetSystemTimeCallback), `ThreadWriteBackMsg.create` (WriteBackCallback), `RenderImageCallback.create`, `IconProviderHandle.with_resolver` / `set_resolver`, `Dom` / `NodeData.with_merge_callback` / `set_merge_callback` (DatasetMergeCallback), `Db#set_on_conflict` (DbMergeCallback), and `AppConfig#add_component_library` (`RegisterComponentLibraryFnType`, which ir_builder doesn't even flag as a callback).
  - **No raw fallback either:** the typedefs are emitted with every non-primitive mapped to `:pointer` (`types.rs:675-684` `arg_callback_ffi_type`).
    - Emitted: `callback :az_timer_callback_type, [:pointer, :pointer], :pointer`
    - api.json: `fn(RefAny, TimerCallbackInfo) -> TimerCallbackReturn`, all by value.
    - Emitted: `callback :az_register_component_library_fn_type, [], :pointer`
    - api.json: that type returns a `ComponentLibrary` struct by value.
    - So a hand-built `FFI::Function` would also be ABI-wrong.
  - **Scope:** 62 callback kinds get idiomatic support. The other 15 of the 77 non-destructor callback typedefs get none.
  - **Fix:** engine `impl_managed_callback!` for the remaining kinds. Until then, emit `raise ArgumentError, "<Kind> callbacks are not supported from Ruby"` when a callable is passed.

- **[MINOR]**, real bug: `lang_ruby/wrappers.rs:169-180`, the String-category `to_s`: `builder.line("vec_ptr = @ptr.get_pointer(0)"); builder.line("vec_len = @ptr.get_uint64(8)");`
  - **Why:** hand-written C layout offsets (the comment says "read offset 0 (vec.ptr) and offset 8 (vec.len)"), called on the wrong receiver type.
  - `@ptr` is the `AzString.by_value` `FFI::Struct`. azul.rb has 28 `String.new(_ret)` sites, e.g. `IconProviderHandle#debug_lookup`, `VideoEncoder.backend_name`.
  - `FFI::Struct` has no `get_pointer` / `get_uint64` (checked on ruby-ffi 1.15.5), so `Azul::String#to_s` raises NoMethodError.
  - **Fix:** read the fields through the IR names, as `emit_rb_to_s_if_supported` already does (`wrappers.rs:489-490`: `az_str[:vec][:ptr]`, `[:vec][:len]`), or take the layout from the IR like Node's `string_layout` (`lang_node/wrappers.rs:261-268`).

- **[MINOR]** `lang_ruby/wrappers.rs:1089`: `a.type_name.trim() == "String" && matches!(a.ref_kind, ArgRefKind::Owned)`
  - **Why:** the auto-AzString rule is keyed on the literal type name. Every other check in the file, and Node's twin (`lang_node/wrappers.rs:1446-1451`), uses `TypeCategory::String`. Same behaviour today.
  - **Fix:** `ir.find_struct(..).category == TypeCategory::String`.

- **[MINOR]** `lang_ruby/managed.rs:192-195`: `&& f.method_name == "from_utf8"` … `.unwrap_or_else(|| ruby_attach_name(&format!("{}_fromUtf8", config.apply_prefix("String"))))`
  - **Why:** if the String constructor is renamed in api.json, the generator silently guesses a symbol instead of failing.
  - **Fix:** make a missing `from_utf8` a codegen error.

### SUSPECT

- **[HELLO-WORLD-ONLY]**, flavoured naming: `lang_ruby/wrappers.rs:240`: `builder.line("def self.create_with_layout(layout_fn = nil, &block)");`
  - **Why:** the factory is structural (`layout_callback_factory_info`, any host-invoker kind), but its name hard-codes "layout".
  - The api.json `create` is aliased to it (`wrappers.rs:302-319` → `alias_method :create, :create_with_layout`).
  - The hello-world calls the invented name `create_with_layout`; Node's hello-world uses the api.json name `create`.
  - **Fix:** keep only the api.json name `create`, or derive the suffix from `info.callback_wrapper`. Change the example to `WindowCreateOptions.create(layout)`.

- **[SILENT SKIP]** tagged-union enums and POD structs get no wrapper class at all.
  - **Code:** `lang_ruby/wrappers.rs:66-81`: `emit_wrappers` iterates `ir.structs` only, and skips any struct without `_delete` (`"# (no wrapper for {} — no _delete; use Native::{} directly)"`).
  - **Result:** no method of a tagged union (e.g. the 22 `AccessibilityAction` variant constructors) is reachable idiomatically, and neither is any derive of a Copy struct. See Derive coverage.
  - This is kind-based, not name-keyed, so it may be a deliberate scope decision.
  - **Fix:** emit wrapper classes for data-carrying enums and value types (without a finalizer when there's no `_delete`), the way Node does.

### Acceptable helpers (not hacks)

- **Host-invoker glue** (`managed.rs`): `_register_callback` dispatch generated from `host_invoker_kinds(ir)` (62 kinds), per-kind `FFI::Function` invokers, the releaser, and `RefAny.wrap` / `unwrap` (RefAny found by `TypeCategory::RefAny`, not by name).
  - `managed.rs:85` `config.apply_prefix("App")` is a literal, but it names the glue owner of `AzApp_setHostHandleReleaser` / `AzApp_set<Kind>Invoker`, which live in `core/src/host_invoker.rs`, not api.json.
- **Library loading:** `mod.rs:81-93` (`ffi_lib` candidates, `AZ_LIB_DIR`).
- **Uniform per-kind helpers:**
  - `functions.rs`: every attach uses `managed_c_symbol` and `blocking: true`.
  - `managed.rs`: `_consume` and `_apply_opts` (the `with(opts)` builder).
  - `wrappers.rs:188-211`: smart `on_*(data, fn)` setters from `smart_callback_setter_info`. 58 generated, covering every `with_on_*` wrapper-struct method.
  - `each` / Enumerable on all 127 Vec wrappers, detected by layout (`wrappers.rs:349-463`).
  - `to_opt` / `unwrap` for Option/Result, detected by variant shape.
  - `RUBY_RESERVED` escaping.
- **Packaging:** `gemspec.rs` takes its version from `ir.api_version`.

### Per-language impact of shared issues

- **`TypeCategory::Vec` name list:** only the 5 Native `to_a` above: 4 real Vecs plus the broken `InstantPtr` one. The idiomatic `each` does not use the category.
- **Enum `Default`-variant skip:** no idiomatic impact. Ruby exposes no enum constructors. `Native::AzAccessibilityAction_Tag::Default = 0` exists, so a user can set `[:tag]` by hand.
- **`HOST_INVOKER_KINDS`:** all 62 real entries are registered. The 15 missing kinds are covered in CERTAIN #3.

### Derive coverage

**(a) How each capability is bound and surfaced.** Every capability is attached in `functions.rs:50-56` (all functions of emitted classes). Idiomatic surfacing happens only on struct wrapper classes that own a `_delete`:

| Capability | Idiomatic surface | Emitter |
|---|---|---|
| Debug | `to_s` + `alias inspect` (decodes, then frees via String `_delete`) | `wrappers.rs:468-503` |
| PartialEq | `==` + `eql?` | `wrappers.rs:521-535` |
| Hash | `hash` | `wrappers.rs:537-545` |
| Clone | `clone` + `alias dup`; also used inside `to_opt`/`unwrap` and `each` | `wrappers.rs:625-628`, `:739-741` |
| Default | `def self.create_default` | `wrappers.rs:564-575` does not exclude it |
| PartialOrd / Ord | none: no `<=>`, no `Comparable` | `wrappers.rs:564-575` hides them and nothing re-surfaces them |

**(b) Coverage numbers** (distinct symbols):

| Capability | azul.h | attached | called from idiomatic code |
|---|---|---|---|
| toDbgString | 2018 | 2018 | 648 |
| partialEq | 1563 | 1563 | 514 |
| partialCmp | 970 | 970 | **0** |
| cmp | 843 | 843 | **0** |
| hash | 871 | 871 | 253 |
| clone | 1115 | 1115 | 668 |
| createDefault | 506 | 506 | 158 |

**(c) Bound but not surfaced.** The filter is kind-based (no wrapper class for tagged unions, unit enums, or structs without `_delete`), not name-keyed. Symbols bound but not surfaced:

| Capability | Tagged unions | POD structs | Unit enums |
|---|---|---|---|
| toDbgString | 713 | 429 | 227 |
| partialEq | 519 | 304 | 226 |
| hash | 227 | 201 | 190 |
| clone | 445 | — | — |
| createDefault | 37 | 175 | 136 |

- Unit enums are Integer constants in Ruby, so their `==`/`hash` gap is harmless.
- **[SILENT SKIP] CERTAIN:** `partialCmp` / `cmp` are never surfaced for any kind.
- **Fix:** define `<=>` via `_cmp` (falling back to `_partialCmp`) and `include Comparable` when `traits.is_ord` / `is_partial_ord`.

**(d) Host-side re-implementation.**
- **[MINOR] CERTAIN:** `wrappers.rs:546-555`: `} else if has_eq { … "@ptr.nil? ? 0 : @ptr.address.hash"`
  - **Why:** when a type derives PartialEq but not Hash, Ruby fabricates an identity hash. 261 wrapper classes get this.
  - It breaks the `eql?` ⇒ equal-`hash` contract, because `eql?` is aliased to the C `_partialEq`. Equal values hash differently, so Hash keys and `uniq` misbehave.
  - **Fix:** without a `_hash` export, don't alias `eql?` to `==` (keep identity `eql?`/`hash`), or leave `hash` undefined.

---

## Node

### CERTAIN

- **[SILENT SKIP]**, generator emits calls to an unbound symbol: `lang_node/wrappers.rs:799` `"const __cloned = lib.Az{}_deepCopy(buf[i]);"` and `:1050` `"const __cloned = lib.Az{}_deepCopy({});"`
  - **Why:** the Clone export is `Az{T}_clone` (`ir_builder.rs:1903` `c_name: format!("Az{}_clone", type_name)`), and `functions.rs` binds `lib.<c_name>`. No `lib.Az*_deepCopy` exists.
  - **Scale in azul.js:** 221 distinct unbound names at 767 call sites: 83 Vec `[Symbol.iterator]` bodies and 684 Option/Result unwraps.
  - **Examples:**
    - `DomVec.get()` does `const __cloned = lib.AzDom_deepCopy(_ret.Some.payload);`, while the real binding is `lib.AzDom_clone = azulFFI.func({ name: 'AzDom_clone', … })`.
    - Same for `CallbackInfo.getDomSubtree()` and every iteration over a Vec of cloneable structs.
  - All of these throw `TypeError: lib.AzDom_deepCopy is not a function` whenever there is a value. The hello-world never exercises them.
  - Node gets this from convention-built symbol strings; `format!("Az{}_toDbgString"/"_partialEq", …)` at `:493`, `:820`, `:850` happen to match today.
  - **Fix:** always spell calls from the IR `FunctionDef.c_name` of the right `FunctionKind`, as Ruby's `class_fn_rb` does.

- **[SILENT SKIP]** `lang_node/wrappers.rs:1371-1373`: `if !HOST_INVOKER_KINDS.contains(&wrapper) { continue; }`
  - **Why:** callback args of the 15 non-host-invoker kinds get no registration and no error. The same function throws a descriptive TypeError for the raw-typedef case (`:1382-1395`).
  - **Result:** generated `Timer.create(refany, callback, getSystemTimeFn)` passes `(callback && callback._ptr !== undefined ? callback._ptr : callback)` into `decl: 'AzTimer AzTimer_create(AzRefAny, AzTimerCallback, AzGetSystemTimeCallback)'`.
  - No proto exists to build a C callback by hand either: `types.rs:120-125` registers every fn-pointer typedef as `azulFFI.alias('<Name>', 'void *')`.
  - **Affected API:** same list as Ruby (Timer, ThreadWriteBackMsg, RenderImageCallback, IconProviderHandle resolver, Dom/NodeData merge callback, `Db.setOnConflict`, `AppConfig.addComponentLibrary`).
  - **Fix:** throw a descriptive TypeError for unsupported kinds (reuse the `:1382` message) and add engine thunks.

- **[MINOR]** `lang_node/wrappers.rs:138`: `b.line("return lib.AzString_fromUtf8(buf, buf.length);");`
  - **Why:** the generic string helper hard-codes the C symbol. Ruby derives it from the String category plus the `from_utf8` method. A rename in api.json breaks every string argument at runtime with no codegen error.
  - **Fix:** look up the String-category constructor in the IR.

- **[MINOR]** `lang_node/functions.rs:62-68`: `if f.kind.is_declared_capability() && ir.find_enum(&f.class_name).is_some_and(|e| e.category == TypeCategory::DestructorOrClone) { return true; }`
  - **Why:** the declared-derive escape covers DestructorOrClone *enums* only. `:70-83` then drops VecRef and DestructorOrClone *structs*, and the VecRef category comes purely from ir_builder's `VECREF_TYPE_NAMES` list.
  - **Unbound:** 14 `_toDbgString`, 3 `_partialEq`, 12 `_clone`, 3 each of `_partialCmp` / `_cmp` / `_hash` (e.g. `AzU8VecRef_toDbgString`, `AzInstantPtrCloneCallback_partialEq`).
  - **Fix:** apply the same escape to `find_struct`, as the comment at `:54-61` already intends.

### SUSPECT

- **[SILENT SKIP]** the `bun:ffi` fallback loader cannot marshal the by-value API, and is chosen silently.
  - **Selection:** `lang_node/mod.rs:759-761`: `const azulFFI = isDeno ? loadDeno() : (isNode || (isBun && _koffiResolves())) ? loadNodeKoffi() : loadBun();`
  - **No struct marshalling:** `toBunType` maps every struct to `FFIType.ptr` (`mod.rs:690` `default: return FFIType.ptr;`, `:708-710`).
  - **Missing `encodeInto`:** `loadBun()` (`mod.rs:695-742`) never defines it, yet `managed.rs:308-311` calls `azulFFI.encodeInto(outPtr, 'AzDom', _raw)` for struct-returning callbacks. The TypeError is swallowed by the invoker catch (`managed.rs:331-336`). The struct catch-default is koffi-only (`managed.rs:406`). Result: layout returns are dropped.
  - **Fix:** throw at load time ("Bun needs `bun add koffi`"), or port the Deno by-value layer.

- **[SILENT SKIP]**, derive policy: `lang_node/wrappers.rs:517-519`: `// SKIPPED: PartialEq/Cmp/Hash are surfaced via azul.__ffi.lib … they are not idiomatic JS.`
  - **Why:** 868 `_hash`, 967 `_partialCmp` and 840 `_cmp` are bound but no `hash()` / `compare()` method exists, although `arr.sort((a, b) => a.compare(b))` is idiomatic.
  - This is a uniform decision, so it is the maintainer's call.

- **[MINOR]** `lang_node/mod.rs:814-816`: `pub fn ffi_type_name(name: &str) -> String { format!("Az{}", name) }`
  - **Why:** it ignores `config.type_prefix`; `generate_azul_js(ir, _config)` never reads the config.
  - **Fix:** use `config.apply_prefix`.

### Acceptable helpers (not hacks)

- **Library loading** (`mod.rs:485-763`): `_resolveDllPath` (`AZ_LIB`, script directory, `AZ_LIB_DIR`, cwd, loader path) and the koffi / Bun / Deno adapters, including the Deno by-value layer (`DENO_ADAPTER_JS`).
- **Host-invoker glue** (`managed.rs`):
  - Literal glue symbols `AzApp_setHostHandleReleaser`, `AzRefAny_newHostHandle`, `AzRefAny_getHostHandle`; these are core exports, not api.json.
  - 62 per-kind invokers and `registerCallback` cases from `host_invoker_kinds(ir)`.
  - `refanyCreate` / `refanyGet` / `_refanyFromPtr`, with RefAny recognised by the IR field names.
  - Catch-default writes (enum → 0, struct → the IR `Default` factory).
- **Uniform per-kind helpers:**
  - `wrappers.rs`: `_consume`, `_applyOpts` (`with(opts)`), and `_azStringDecode`, whose layout comes from the IR (`:238-268`).
  - Smart `<event>(data, fn)` setters from the shared detector (`:449-471`, 58 generated).
  - `WindowCreateOptions.create(layoutFn)` from `layout_callback_factory_info` (`:1279-1300`, structural; the hello-world uses the api.json name).
  - `Callback.create(fn)`-style constructors for host-invoker kinds (`:1244-1253`, `:1305-1317`).
  - Vec `[Symbol.iterator]` by layout on all 127 Vecs, and Option/Result unwrapping by variant shape.
  - `is_js_reserved` escaping.
  - `functions.rs`: `managed_c_symbol` and `_Inout_` for `&mut` aggregates.
- **Packaging:** `package_json.rs` takes its version from `ir.api_version`.

### Per-language impact of shared issues

- **`TypeCategory::Vec` name list:** no impact; the iterator is layout-based (`wrappers.rs:733-742`).
- **VecRef name list:** these structs get no wrapper class, and their declared derives are unbound (CERTAIN #4).
- **Enum `Default`-variant skip, confirmed:** the `AccessibilityAction` class has static factories for 22 variants (`focus()` … `customAction(payload)`) but none for `Default`, which is tag 0. There is no `lib.AzAccessibilityAction_default` binding. The same applies to `ComponentFieldValueSource`. Users must hand-build the koffi union.
- **`HOST_INVOKER_KINDS`:** 62/62 registered. The 15 unsupported kinds are covered in CERTAIN #2.

### Derive coverage

**(a) How each capability is bound and surfaced.** Every capability is bound as `lib.<c_name>` (`functions.rs:104-141`).

| Capability | Idiomatic surface | Emitter |
|---|---|---|
| Debug | structs: `toString()`, decodes and frees via String `_delete` | `wrappers.rs:816-844`, gated at `:493-512` |
| Debug | tagged unions: raw alias | `:684-687` → `emit_instance_alias(b, f, "toString", …)` |
| PartialEq | `equals(other)`, structs only | `:531`, `:849-870` |
| Clone | `clone()` alias | `:503-505` (structs), `:680-682` (enums) |
| Clone | internal uses: the broken `_deepCopy` sites | `:799`, `:1050` |
| Default | `static createDefault()` | `:513-516` |
| Hash / PartialOrd / Ord | not surfaced | `:517-519` |

**(b) Coverage numbers** (distinct symbols):

| Capability | azul.h | bound | called from idiomatic code |
|---|---|---|---|
| toDbgString | 2018 | 2004 | 1642 (586 of them the raw enum alias) |
| partialEq | 1563 | 1560 | 811 |
| partialCmp | 970 | 967 | **0** |
| cmp | 843 | 840 | **0** |
| hash | 871 | 868 | **0** |
| clone | 1115 | 1103 | 1094 |
| createDefault | 506 | 506 | 370 |

**(c) Filtering and broken surfaces:**
- **[SILENT SKIP] CERTAIN, Debug on tagged unions.** The enum wrapper's `toString()` returns the raw koffi `AzString` object and never frees it: `return lib.AzAccessibilityAction_toDbgString(this._ptr);`. That is 586 classes, versus 1056 struct wrappers that decode.
  - Kind-based (struct vs enum).
  - **Fix:** call `emit_node_to_string_if_supported` from `emit_enum_wrapper`.
- **[SILENT SKIP] CERTAIN, PartialEq on tagged unions.** `emit_node_equals_if_supported` is only called from `emit_struct_wrapper` (`:531`). 519 union-enum `_partialEq` exports are bound but get no `equals()`.
  - **Fix:** call it from `emit_enum_wrapper` as well.
- Hash / PartialOrd / Ord: see SUSPECT #2.
- Unit enums are frozen number tables, so their 226–227 unsurfaced symbols per capability are harmless.

**(d) Host-side re-implementation:** none found. `equals()` only adds a null check; unit enums compare as JS numbers.


---

<!-- ocaml_zig.md -->

**Source partial:** Bindings hack audit: OCaml + Zig (partial)

Scope: `doc/src/codegen/v2/lang_ocaml/` (working tree = HEAD incl. 70fe66c4a, no uncommitted changes), `doc/src/codegen/v2/lang_zig/`.
Output checked: `target/codegen/ocaml/` (279 files), `target/codegen/azul.zig`, `target/codegen/azul.h`. target/codegen was wiped and regenerated three times during the audit (15:28, 15:32, 15:36) by someone else's `codegen all`. All output numbers come from one consistent copy taken at 15:36 (same byte sizes as the 15:29 and 15:33 generations). Output line numbers refer to that copy.
Examples: `examples/ocaml/hello_world.ml`, `examples/zig/hello-world.zig`.

---

## OCaml

Hello-world entry points, and where each comes from:

| Call in `hello_world.ml` | Generator path |
|---|---|
| `Azul.Dom.p ~css:… text` | Tag helper keyed on `with_css` and `create_*_with_text` (CERTAIN #1) |
| `Azul.Dom.body ~children:[…]` | Tag helper keyed on `with_child` (CERTAIN #1) |
| `Azul.Button.create "…" ~button_type ~on_click` returns a `dom` | Keyed on a method named `dom` (CERTAIN #2). `~button_type` and `~on_click` come from the generic `with_<x>` → `?<x>` rule. |
| `Azul.WindowCreateOptions.create ~layout ()` | Structural layout factory, which matches exactly one class (SUSPECT #1) |
| `Azul.App.create ~data ~app_config ()`, `App.run`, `AppConfig.create ()` | Generic smart constructor / method |
| `Azul.RefAny.key / upcast / bind / lift` | RefAny glue (acceptable) |

### CERTAIN

1. **[NAME-KEYED SPECIAL CASE]** `doc/src/codegen/v2/lang_ocaml/wrappers.rs:1173-1176`
   - Code: `builder_named(s, ir, "with_child", |a| a.type_name.trim() == s.name)` and `builder_named(s, ir, "with_css", |a| a.type_name.trim() == "String")`, plus `:1191-1194` `.strip_prefix("create_").and_then(|r| r.strip_suffix("_with_text"))`.
   - Why it is a hack: the tag helpers are keyed on three literal api.json method names, and only `Dom` matches. That yields 94 helpers (54 `val <tag> : children:t list -> t`, 40 `val <tag> : ?css:string -> string -> t`; `azul_api_dom.mli:2582` `val p`, `:2584` `val body`), which are exactly the hello-world's `Dom.p ~css` / `Dom.body ~children`. The generic smart constructor already turns every `with_<x>` into `?<x>`. The dedicated `?css` parameter exists only because of the `"with_css"` literal.
   - Fix: derive "container constructor + child builder" and "text constructor" from the argument and return types (or api.json metadata), not from method-name literals.

2. **[NAME-KEYED SPECIAL CASE]** `doc/src/codegen/v2/lang_ocaml/wrappers.rs:842-855` (`dom_conversion`, used at `:939-943`)
   - Code: `f.method_name == "dom" && … f.args.len() == 1 …`
   - Why it is a hack: every class with a method literally named `dom` gets a smart `create` that returns the DOM instead of the widget. That is 46 modules in `azul_api_widgets.mli`, e.g. `Button.create : ?on_click:(ref_any * (…)) -> ?button_type:ButtonType.t -> … -> string -> dom`. The widget itself is then reachable only through `create_raw`. This is what lets the hello-world drop `Button.create …` straight into `~children`.
   - Fix: keep `create : … -> t` and let users call `Button.dom`, or mark conversion methods in api.json.

3. **[MINOR]** Literal type-name compares where the IR already has a category
   - Code: `lang_ocaml/wrappers.rs:393` `if t == "String"`, `:898` and `:902` `a.type_name.trim() == "RefAny"`, `:1175` and `:1187` `… == "String"`, `lang_ocaml/managed.rs:111` `if t == "RefAny" {`.
   - Why it is a hack: `managed_lang_helpers.rs:125-128` says bindings must not compare against the literal `"RefAny"`. The same file imports `is_refany_type` and uses it at `wrappers.rs:395`.
   - Fix: use `TypeCategory::String` and `is_refany_type`.

4. **[SILENT SKIP]** Type aliases become 8-byte opaque pointers: `doc/src/codegen/v2/lang_ocaml/types.rs:170-176` and `:648-649`
   - Code: `for ta in ir.type_aliases… builder.line(&format!("type {} = unit ptr", ffi)); builder.line(&format!("let ({} : {} typ) = ptr void", ffi, ffi));` and `// Callback function pointers, opaque types, unknown — pointer-sized. (8, 8)`.
   - Why it is a hack: nothing in `lang_ocaml` reads `monomorphized_def` or asks `find_type_alias` for a size. So all 196 aliases turn into 8-byte opaque pointers without any diagnostic. 180 of them are real tagged unions or structs; `GLuint`, for example, is a `u32`. Evidence in the output:
     - `type az_gluint = unit ptr` (`azul_types_gl.ml:63`) and `type az_caret_color_value = unit ptr` (`azul_types_css.ml:724`).
     - `AzCssProperty` is sealed as 2×`uint64_t` = 16 bytes (`azul_types_css_3.ml:1640-1641`). The real union contains `AzCssPropertyVariant_BoxShadowLeft { uint8_t tag; AzStyleBoxShadowValue payload; }` (`azul.h:47200`), and `AzStyleBoxShadow` alone is 4×16-byte `AzPixelValueNoPercent` + 8 bytes. The real union is therefore at least 88 bytes.
     - 280 `foreign` bindings take an alias by value and 23 return one through the 8-byte placeholder. Examples: `AzNodeType_text(BoxOrStaticString)`, `AzCssProperty_animation(StyleAnimationVecValue)`, and every GL entry taking `GLuint` or `GLenum`.
     - The hello-world path never touches these types.
   - Fix: emit aliases from `monomorphized_def` (Zig already does this at `lang_zig/c_decls.rs:368-421`), map plain aliases to their target, and add an alias branch to `c_size_of_type`.

5. **[SILENT SKIP]** VecRef passed by value as an 8-byte pointer: `doc/src/codegen/v2/lang_ocaml/types.rs:107-122` (`:118` `type {} = unit ptr` for `Recursive | VecRef | DestructorOrClone`) together with `functions.rs:89-104`, which filters only by the owning class's category
   - Why it is a hack: the 23-name VecRef list (`ir_builder.rs:2222-2248`) becomes `unit ptr`. Yet 56 `foreign` signatures of other classes pass a VecRef by value, where C expects a 16-byte `{ptr,len}` struct. Example: `azul_ffi_gl.ml:70` `foreign "AzGlContextPtr_deleteBuffers" ((ptr az_gl_context_ptr) @-> az_gluint_vec_ref @-> returning void)`.
   - Fix: emit VecRefs as their real 2-field structures, or skip such functions with a diagnostic.

6. **[SILENT SKIP]** Raw callback-typedef arguments have no closure path: `doc/src/codegen/v2/lang_ocaml/wrappers.rs:386-416`
   - Why it is a hack: an argument becomes `HostFn` only `if is_callback_wrapper(t)`. A typedef-typed argument falls through to `map_type_to_ocaml_typ` and ends up as `az_<typedef>` = `unit ptr` (`types.rs:101-104`). Output: `azul_api_dom.mli:2253` `val add_callback : t -> az_event_filter Ctypes.structure -> ref_any -> az_callback_type -> unit` and `:2501` `val with_callback : … -> az_callback_type -> t` (NodeData has the same).
   - Why a raw pointer can't be made to work: the engine thunk finds the host closure through `info.get_ctx()` (`core/src/host_invoker.rs:442`), and that ctx is `None` on the raw-typedef path. So the generic "attach an event handler to any DOM node" API cannot take an OCaml closure.
   - Scope: api.json has 12 functions that take a raw typedef: `Dom.with_callback/add_callback`, `NodeData.add_callback`, `StringMenuItem.with_callback`, `IconProviderHandle.with/set_resolver`, `Callback.create`, `LayoutCallback.create`, `RenderImageCallback.create`, `DatasetMergeCallback.from`, `AppConfig.add_component_library`, `WindowCreateOptions.create`. Only `WindowCreateOptions.create` got a bespoke factory (SUSPECT #1). The hello-world avoids the gap by using `Button.with_on_click`, which takes the wrapper struct.
   - Fix: move these 12 api.json signatures to the wrapper-struct form (or have the dll emit the `WithCtx` twin for typedef arguments) so the generic `HostFn` path covers them.

7. **[MINOR]** Dead tagged-union "views": `doc/src/codegen/v2/lang_ocaml/wrappers.rs:1942-1946`
   - Code: `// Payload-bearing variants are surfaced as opaque ints …` and ``format!("`{} of int", lit)``.
   - Why it is a hack: 590 `type az_*_view = [ … ]` declarations (295 unions × .ml/.mli) are emitted, and no generated function produces or consumes them (rg finds no reference outside the declarations). They are stub types, even though `mod.rs:50-51` presents them as the tagged-union surface.
   - Fix: emit real `to_view`/`of_view` converters, or drop the declarations.

### SUSPECT

1. **[HELLO-WORLD-ONLY?]** Layout smart factory: `lang_ocaml/managed.rs:457-551` (`azul_<class>_with_layout`) and `lang_ocaml/wrappers.rs:869-892` (`BaseCtor::Layout`)
   - It is driven by the structural `layout_callback_factory_info`, but exactly one class matches. The result is `WindowCreateOptions.create : ?layout:(ref_any -> az_layout_callback_info Ctypes.structure -> dom) -> unit -> t` (`azul_api_window.mli:756`).
   - It exists because `AzWindowCreateOptions_create(LayoutCallbackType)` drops the ctx, which is the same root cause as CERTAIN #6.
   - Judgement call: acceptable callback wrapping, or should api.json's constructor take `LayoutCallback` so the generic `HostFn` path covers it?

2. **[MINOR]** Downcast-failure logger found by API name: `lang_ocaml/managed.rs:714-736`
   - Code: `f.method_name == "log" && f.args.len() == 3 && … f.args[2].type_name.trim() == "String"` and `ir.find_enum(level)?.variants.iter().find(|v| v.name == "Error")?`.
   - This sits inside the callback glue, which is allowed, but it is keyed on a method name and a variant name. Renaming either silently switches all 59 kinds to stderr.
   - Fix: mark the log entry point in api.json, or accept the stderr path everywhere.

3. **[MINOR]** ThreadCallback registered like any other kind
   - `lang_ocaml/managed.rs:278-283` says "`~runtime_lock` stays false … Callbacks are therefore main-thread only".
   - Yet the output contains `azul_register_thread_callback` (`azul_managed.ml:1888`) and `Thread.create ~callback:(ref_any -> thread_sender -> thread_receiver -> unit)` (`azul_api_task.mli:55`).
   - `managed_host_invoker.rs:121-130` says thunks for this kind MUST take the VM lock. Meanwhile `WriteBackCallback`, the documented alternative, has no constructor (see the shared-issues section).
   - Fix: acquire the runtime lock for thread-affine kinds, or withhold ThreadCallback with a diagnostic.

4. **[MINOR]** Stale hello-world-driven comment: `lang_ocaml/mod.rs:639-642`
   - Code: `// … doesn't match the hand-written hello-world's `RefAny.wrap` calls`.
   - The module-naming choice was justified by an old hello-world, and no `RefAny.wrap` exists any more. Fix: update the comment.

### Acceptable helpers (not hacks)

- Library loading: `azul_loader.ml` `Dl.dlopen` with an `AZ_DYLIB` override (`mod.rs:333-379`).
- Host-invoker prelude, generic over all 63 `host_invoker_kinds`: handle table, pinned releaser, typed per-kind invokers, `azul_register_<kind>_callback` (`managed.rs:262-751`).
- RefAny glue from 70fe66c4a: `RefAny.key/upcast/downcast/lift/bind` (`wrappers.rs:1085-1127`), including `Obj` introspection used only for the error message.
- `azul_az_string` and the AzString decoders.
- Generic smart constructor: `?<x>` per self-consuming `with_<x>`, `~data` for a lone RefAny constructor argument, and `?on_<event>:(ref_any * fn)` for all 121 wrapper-struct setters via `smart_callback_setter_info`.
- Vec `to_list`/`to_array` by structural layout probe (`wrappers.rs:1538-1553`, `:1644-1657`).
- Record finalisers and keyword escaping (`mod.rs:973-1066`).
- Packaging: `dune.rs` / `mod.rs:262` ship `hello_world.ml` (`include_str!` of the example) and a `hello_world` executable stanza inside the generated project. The binding's project doubles as the example project; that's packaging, not API.

### Per-language impact of shared issues

- **`TypeCategory::Vec` 4-name list, and InstantPtr/StringMenuItem filed as Vec:** no impact. OCaml never reads the category and probes the layout (`wrappers.rs:1539-1553`, whose comment names the IR bug).
- **VecRef name list:** CERTAIN #5 (56 by-value signatures bound as 8-byte pointers).
- **`Default`-variant constructor skip:** `AccessibilityAction` and `ComponentFieldValueSource` get no `ffi_az_*_default` binding at all. OCaml surfaces union variant constructors only as raw `ffi_az_<enum>_<variant>` values (the union enum modules carry capabilities only), so `…::Default` can only be built by writing the tag bytes by hand.
- **The 15 kinds outside HOST_INVOKER_KINDS — [SILENT SKIP]:** `managed.rs:312-314`, `:433-435`, `:557-582` iterate `host_invoker_kinds(ir)` only, so nothing is emitted for the other kinds.
  - `module TimerCallback : sig type t = timer_callback val clone … val equal … val hash … val to_string … val compare end` (`azul_api_callbacks.mli:592-603`) has no constructor. The same holds for WriteBack, GetSystemTime, MarginBox, DbMerge, SelectionTween and CaretTween.
  - As a result `Timer.create … ~callback:timer_callback ~get_system_time_fn:get_system_time_callback` (`azul_api_task.mli:192`) cannot be called.
  - RenderImageCallback and DatasetMergeCallback take a raw `az_*_callback_type` (`unit ptr`).
  - No `Foreign.funptr` view exists for these typedefs (`type az_timer_callback_type = unit ptr`, `azul_types_time.ml:45`).
- **`RegisterComponentLibraryFnType`:** `AppConfig.add_component_library` takes a raw `unit ptr`.

### Derive coverage

**(a) How each capability is surfaced**

| Derive | OCaml surface | Emitter lines |
|---|---|---|
| Debug | `to_string : t -> string`; decodes, then `AzString_delete`s. No `pp`/Format printer. | `wrappers.rs:1804-1852`; enums `:2092`, `:2191` |
| PartialEq | `equal` | `:1856-1896`, `:2084`, `:2180` |
| Ord | `compare : t -> t -> int`, mapping 0/1/2 to -1/0/1 | `:2253-2278` |
| PartialOrd | `partial_compare : t -> t -> int option` (255 → `None`); on structs only when there is no `_cmp` | `:2311-2351` |
| Hash | `hash : t -> int` via `UInt64.to_int` | — |
| Clone | `clone : t -> t`; DeepCopy is an ordinary method | `:2052` |
| Default | `create_default : unit -> t` on struct modules, `default ()` in enum modules | `:2051`, `:2223` |

**(b) Distinct symbols covered** (idiomatic = referenced from `azul_api_*`/`azul_enums_*`/`azul_records_*`/`azul_managed`; mapped back to C names through the `foreign` table)

| cap | azul.h | OCaml `foreign` | OCaml idiomatic |
|---|---|---|---|
| toDbgString | 2018 | 2004 | 1873 |
| partialEq | 1563 | 1560 | 1559 |
| partialCmp | 970 | 967 | 528 |
| cmp | 843 | 840 | 839 |
| hash | 871 | 868 | 867 |
| clone | 1115* | 1103 | 1101 |
| createDefault | 506 | 506 | 506 |

\*One of the 1115 `_clone` symbols is `AzBoxDecorationBreak_clone(void)` (`azul.h:66233`). It is the constructor of the unit variant `BoxDecorationBreak::Clone` (a Copy type with no DeepCopy), not a derive. This is a C-name collision between a variant constructor and trait naming; the IR guards only a variant named `Default` (`ir_builder.rs:1705`). This is a shared IR issue.

**(c) Gaps (all kind-based, none name-keyed)**

- **Not bound in the FFI layer at all:**
  - 12 VecRef types: `_toDbgString` and `_clone`; `U8VecRef` also eq/partialCmp/cmp/hash.
  - The DestructorOrClone structs `InstantPtrCloneCallback` and `InstantPtrDestructorCallback`: toDbgString, eq, partialCmp, cmp, hash.
  - Cause: `functions.rs:93-104` excludes VecRef and DestructorOrClone structs, and the declared-capability carve-out at `:77-88` covers only DestructorOrClone/Recursive enums and Recursive structs.
- **Bound but not surfaced idiomatically:**
  - 127 `*VecDestructor_toDbgString`: `emit_enum_modules_for` skips DestructorOrClone unions (`wrappers.rs:200-207`).
  - `AzPhysicalSizeU32_*` (5 capabilities): monomorphized aliases have no module.
  - `AzCompileTarget_clone`: the unit-enum module list at `wrappers.rs:2165-2173` omits DeepCopy.
- **Not surfaced, by design:**
  - `Json`/`Url`/`String` `_toDbgString`: the class already has a `to_string` (`:1821-1827`).
  - 438 struct `_partialCmp`: `compare` wins (`:2311-2317`).

**(d) Host-side re-implementation of derives:** none. No `Stdlib.compare`, `Hashtbl.hash` or structural `=` on ctypes structures; every capability calls its C entry point.

---

## Zig

The hello-world uses only generic surface:
- `App.create(data, …)` goes through `_asRefAny`, which ends in `ReflectModel(T).upcast`.
- `WindowCreateOptions.create(layout)` and `setOnClick(model.clone(), onClick)` go through `_asCallback` on the IR typedef.
- `ReflectModel(MyDataModel).Ref` is RefAny glue.

The example's raw `window.inner.window_state…`, `azul.C.AzButtonType_Primary` and `.inner` accesses are example style, not generator special cases.

### CERTAIN

1. **[SILENT SKIP]** Slice→Vec conversion gated on the 4-name category: `doc/src/codegen/v2/lang_zig/wrappers.rs:926-938`
   - Code: `Some(TypeCategory::Vec) => { if let Some(item) = vec_item_type(ctx, ty) { if ctx.has_c_fn(ty, "create") && ctx.has_c_fn(ty, "copyFromPtr") && ctx.has_c_fn(ty, "fromItem") {`
   - Why it is a hack: only U8Vec (27 parameters), StringVec (19) and GLuintVec (1) get `_asAzVec`. 88 by-value parameters of 39 other `*Vec` types fall back to `_asOwned`, e.g. `AzCallbackInfo_queueWindowStateSequence(&self.inner, _asOwned(C.AzFullWindowStateVec, states, …))`. All 39 export the same `create`/`copyFromPtr`/`fromItem` trio, so the category, which ir_builder assigns from `VEC_TYPE_NAMES` (`ir_builder.rs:2256`), is the only blocker.
   - Fix: key on the structural Vec layout (as `lang_cpp::has_vec_layout` and OCaml do), not on `TypeCategory::Vec`.

2. **[MINOR]** Byref-twin predicate re-derived from the name: `doc/src/codegen/v2/lang_zig/c_decls.rs:517-527`
   - Code: `&& a.type_name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) && !a.type_name.ends_with("CallbackType") && !a.type_name.ends_with("FnType")`.
   - Why it is a hack: `ir.rs:105-107` makes `CodegenIR::is_value_aggregate` the single predicate, and `lang_c.rs:1118-1121` uses it for both arguments and aggregate returns. The comment at `c_decls.rs:511` claims "exactly the predicate `lang_c::emit_c_byref_twin` uses", which is false. The result is 141 `pub extern fn …Byref` declarations that libazul does not export (e.g. `AzButton_setButtonTypeByref`, whose only owned argument is a unit enum), while 6166 twins that do exist are missing. No wrapper calls any twin, so this is latent, but `azul.C.*Byref` users would get link errors.
   - Fix: use `ir.is_value_aggregate` for arguments and return.

3. **[SILENT SKIP]** Callback-wrapper arguments gated on the managed-language allowlist: `doc/src/codegen/v2/lang_zig/wrappers.rs:957-968`
   - Code: `if is_api_function && is_callback_wrapper(ty) { return ArgConv::Callback { … } }`
   - Why it is a hack: `is_callback_wrapper` means HOST_INVOKER_KINDS. For the 15 other kinds, the wrapper struct passes through `_asOwned`: `C.AzTimer_create(_asRefAny(refany), _asOwned(C.AzTimerCallback, callback, C.AzTimerCallback_clone), _asOwned(C.AzGetSystemTimeCallback, get_system_time_fn, …))` (`azul.zig:103729`). The user must hand-build `C.AzTimerCallback{ .cb = azul._asCallback(C.AzTimerCallbackType, f), .ctx = … }`. Zig passes real C function pointers and does not need the host invoker, so this gate is inherited, not required.
   - Fix: for every struct with `callback_wrapper_info`, accept a Zig fn and build `{ .cb = _asCallback(<typedef>, f), .ctx = none }`.

### SUSPECT

1. **[HELLO-WORLD-ONLY]** `lang_zig/build_zig.rs:48`, `:59`
   - Code: `.root_source_file = b.path("hello-world.zig")` and `.name = "hello-world"`.
   - The shipped `build.zig` is the hello-world's manifest. This is packaging rather than API; judgement call.

2. **[MINOR]** Hand-written trampoline arity arms: `lang_zig/mod.rs:433-459`
   - The arms cover 0..4 parameters, then `@compileError("_asCallback: callbacks with more than 4 parameters are not supported")`.
   - Three typedefs take 5-6 parameters: `OnNodeAddedCallbackType`, `OnNodeConnectedCallbackType`, `OnNodeFieldEditedCallbackType` (`azul.zig:25459`).
   - No generated wrapper routes them through `_asCallback` today (latent), but a Zig fn can't become one of these callbacks.
   - Fix: emit arms up to the IR's maximum typedef arity.

3. **[MINOR]** Silent alias drop: `lang_zig/c_decls.rs:374-377`
   - Code: `if target.contains('<') || target.contains('>') { return; }`
   - A generic alias without `monomorphized_def` is dropped with no comment. Dead today: all 196 aliases are declared in azul.zig.
   - Fix: emit a diagnostic comment.

### Acceptable helpers (not hacks)

- `ReflectModel` upcast/downcast/borrow (`mod.rs:171-286`). `downcast` returns `Ref{ .inner = refany.* }`, a documented borrowed view that the trampoline releases.
- `_asCallback`: a comptime trampoline derived from each typedef's parameter list, so raw-typedef arguments of every kind work, including `Dom.withCallback` → `_asCallback(C.AzCallbackType, callback)` (`azul.zig:109833`).
- `_asAzString`, `_asAzOption`, `_asAzVec`, `_asOwned`, `_asConstPtr`, `_asMutPtr`: generic per kind.
- The `c_decls` pre-translation of the whole C ABI, including monomorphized aliases.
- Keyword escaping (`mod.rs:615-721`).
- Linking via `build.zig`.
- The `// SKIPPED: duplicate pub fn` comments (`wrappers.rs:333-339`, `:352-358`, `:563-568`) are visible, not silent.

### Per-language impact of shared issues

- **`TypeCategory::Vec` list:** CERTAIN #1.
- **InstantPtr/StringMenuItem filed as Vec:** no impact, because `ArgConv::Vec` also requires the create/copyFromPtr/fromItem exports, which they lack.
- **VecRef list:** no ABI impact. `c_decls` emits VecRefs with their real fields; wrapper structs for them exist only when they declare capabilities.
- **`Default`-variant skip:** the union helper namespaces take their constructors from IR functions (`wrappers.rs:737-771`), so `AccessibilityAction` and `ComponentFieldValueSource` have no constructor for `Default`. It needs a raw `C.AzAccessibilityAction{ .Default = .{ .tag = … } }` literal.
- **The 15 non-host-invoker kinds:** raw-typedef arguments are fine (generic `_asCallback`); wrapper-struct arguments are degraded (CERTAIN #3).

### Derive coverage

**(a) How each capability is surfaced**

| Derive | Zig surface |
|---|---|
| PartialEq | `eql` |
| Ord | `order`, returning a raw `u8` 0/1/2 rather than `std.math.Order` (441 struct wrappers) |
| PartialOrd | `partialOrder`, raw `u8` |
| Hash | `hash`, `u64` |
| Debug | `toDbgString`, returning an owned `C.AzString` the caller must free (1077 struct wrappers); there is no `pub fn format`, so no `{f}` printing |
| Clone | `clone`, returning the wrapper |
| Default | `createDefault() Self` (333) / `default() Raw` on unit enums (136) |

Emitter lines: structs `wrappers.rs:524-572`, enums `:456-512` (on `*const Raw`), unit-enum default `:435-448`, names `:1150-1166`.

**(b) Distinct symbols covered** (C decl = `pub extern fn`; wrapper = `C.Az*_<cap>(` call sites)

| cap | azul.h | Zig C decl | Zig wrapper |
|---|---|---|---|
| toDbgString | 2018 | 2018 | 1890 |
| partialEq | 1563 | 1563 | 1562 |
| partialCmp | 970 | 970 | 969 |
| cmp | 843 | 843 | 842 |
| hash | 871 | 871 | 870 |
| clone | 1115 | 1115 | 1114 |
| createDefault | 506 | 506 | 506 |

**(c) Gaps (kind-based, not name-keyed)**

- The C layer declares every symbol.
- The wrapper layer misses:
  - 127 `*VecDestructor_toDbgString`: `should_emit_enum_helper` (`wrappers.rs:231-244`) excludes DestructorOrClone enums without the `has_declared_capability` carve-out that structs get (`:197-209`).
  - `AzPhysicalSizeU32_*`: aliases get no namespace.
- The one "missing" clone is the `BoxDecorationBreak::Clone` variant constructor, a false positive.

**(d) Host-side re-implementation of derives:** none. No `std.meta.eql` and no byte hashing. `_cloneOr` (`mod.rs:326-332`) turns a would-be bit-copy of a non-Clone type into a compile error.


---

<!-- go_fortran.md -->

**Source partial:** Bindings hack audit: Go + Fortran (partial)

Provenance: I read the emitter source at HEAD plus the working tree. `lang_go/` is clean at HEAD (last change 2bf469b7d, 13:57). `lang_fortran/` has UNCOMMITTED edits made by someone else while this audit ran: managed.rs about +400, wrappers.rs +32, mod.rs +1, makefile.rs +13. Line numbers refer to the working tree, and findings tagged `[WIP]` exist only in that uncommitted edit. Generated output is target/codegen/{go,fortran} as regenerated at 15:36; codegen was re-run three times while I worked. The derive ground truth is partials/derive_symbols_azul_h.txt.

---------------------------------------------------------------------------

## Go

The Go binding uses purego, not cgo. The types, raw functions and wrapper layers are fully generic: layouts come from `lang_fortran::layout`, and the Byref twins are selected with `is_value_aggregate`. The hacks are all in the callback surface: an idiomatic Go closure can only be passed through two constructs, and those are the two the hello-world uses. One hand-written interface also keys on specific API names.

### CERTAIN

- **[SILENT SKIP]** `doc/src/codegen/v2/lang_go/managed.rs:725` — `"func Register{w}(fn {w}Func) Az{w} {{"` returns the RAW `Az<Kind>`. The idiomatic wrappers, however, type every callback-kind argument as the wrapper struct: `lang_go/wrappers.rs:673-676` — `if should_emit_wrapper(s, ir, config) { return format!("*{}", sanitize_identifier(&s.name)); }`. They then call `.Raw()` on it (`wrappers.rs:643-644`). That struct's only field is unexported (`wrappers.rs:213` `inner *{}`), and api.json gives `ResumeCallback`, `ButtonOnClickCallback` and `ThreadCallback` no constructor. Generated result: `func (self *FilePath) ReadBytes(data any, on_result *ResumeCallback) AzRequestId { return AzFilePath_readBytes(self.inner, azGoRefAnyOwned(data), on_result.Raw()) }`. Code outside the package has no way to build a `*ResumeCallback`.
  - Of 146 API functions that take a host-invoker callback kind, 58 get an `On<X>` smart setter and 57 are `with_*` builders duplicated by such a setter.
  - The remaining **31 functions have no idiomatic path from a Go func** and must go through the raw `Az*` layer plus `Register<Kind>`:
    - 23 async `ResumeCallback` APIs: `HttpRequestConfig.http_get/http_post/http_request/download_bytes/is_url_reachable`, `FileDialog.open_file/open_directory/open_multiple_files/save_file`, `ColorPickerDialog.open`, `Db.open/get/iterate/query_index/subscribe/sync_now`, `FilePath.read_bytes/read_string/read_dir`, `RawImage.decode_image_bytes`, `DecodedVideo.decode_mp4_h264`, `AudioDeviceList.enumerate`, `ScreenRecorder.finish`.
    - `Thread.create` and `ThreadPool.create_thread`.
    - `AppConfig.add_route` (LayoutCallback).
    - `Dom.create_virtual_view` and `NodeData.create_virtual_view`.
    - `CallbackInfo.check_for_updates`.
    - `RibbonGroup.set_launcher` and `with_launcher`.
  - The shapes that do work are `set_on_*` (the hello-world's `button.OnClick`) and the layout factory (`WindowCreateOptionsCreate`).
  - Fix: in `map_arg_type`/`format_call_args`, map every by-value host-invoker-kind argument to `<Kind>Func` and pass `Register<Kind>(fn)`, as `lang_fortran/wrappers.rs:618-625` already does. The `On<X>` setters then become redundant.
- **[NAME-KEYED SPECIAL CASE]** `lang_go/managed.rs:895` — `let Some(rest) = f.method_name.strip_prefix("set_on_") else { continue; };`. Smart setters are selected by a method-name prefix rather than by shape. `RibbonGroup.set_launcher(data: RefAny, on_click: ButtonOnClickCallback)` has the identical shape and gets nothing. Fix: select on (method, `RefAny` arg, host-invoker-kind arg) and derive the Go name from the method name. This is moot once the previous fix is in.
- **[NAME-KEYED SPECIAL CASE]** `lang_go/managed.rs:547-561` — `b.line("    Log(AppLogLevel, *String)");` … `b.line("            l.Log(AppLogLevel_Error, Str(msg))");`. The `class_has_log` predicate at `:96-102` is keyed on `f.method_name == "log"`.
  - This hand-types the signature of `CallbackInfo.log(level: AppLogLevel, message: String)`, the enum `AppLogLevel` and its variant `Error`.
  - A rename breaks the build. A signature drift is worse: Go interfaces are structural, so `s.(azGoLogger)` silently stops matching and `Bind` errors fall through to `log.Print`.
  - Fix: derive the sink from the IR, as `lang_fortran/managed.rs::kind_logger` does. That means the `log` method's argument types and the level enum's error variant.
- **[MINOR]** `lang_go/managed.rs:499-518` — `raw := AzString_fromUtf8(ptr, uintptr(len(b)))`, `ret := &String{ inner: &raw }`, `if s.Vec.Ptr == nil || s.Vec.Len == 0`.
  - The String helpers (`Str`, `GoStr`, `(*String).Value`) spell out the type, constructor and field path literally.
  - The RefAny helpers right below take the type from `TypeCategory::RefAny` (`managed.rs:245-258`).
  - Fortran hard-codes a DIFFERENT constructor name (`copy_from_bytes`).
  - Fix: resolve the `TypeCategory::String` struct and its `(*const u8, usize) -> String` constructor from the IR.
- **[SILENT SKIP]** `lang_go/functions.rs:253-271` — `"panic(\"azul: {} takes {} arguments, purego supports at most {}\")"`. Functions with more than 15 purego arguments compile but panic when called. Two are affected in the fresh output: `AzGlContextPtr_copyImageSubData` (16 args) and `AzGlContextPtr_copySubTexture3dAngle` (18 args). The only diagnostic is a comment in functions.go. Fix: a generic DLL "args-struct" twin for arity above 15, or at least a codegen warning.
- **[MINOR]** `lang_go/types.rs:244-247` — `"// {} is a C function pointer (opaque to Go; use Register* in callbacks.go)."` is stamped on every callback typedef. `Register*` exists only for the 62 host-invoker kinds, not for the roughly 130 destructor typedefs or `TimerCallbackType`, `WriteBackCallbackType` and the others. Fix: emit the hint only when `host_invoker_kinds` contains the wrapper, otherwise state that no Go closure path exists.

### SUSPECT
- `lang_go/managed.rs:835-868` generates `WindowCreateOptionsCreate(fn LayoutCallbackFunc)`, and `lang_go/wrappers.rs:240-247` suppresses the raw constructor it replaces. Both come from `layout_callback_factory_info`, which is structural and names no class, but exactly one class matches: the hello-world's entry point.
- `lang_go/functions.rs:156-160` — `|| t.ends_with("CallbackType") || t.ends_with("FnType")` is a naming heuristic that the IR lookup on the line above already covers. Harmless today.
- `lang_go/managed.rs:69-79` — `t.ends_with("CallbackInfo")` → `info`, `t == "usize"` → `index`. Callback parameter-name heuristics; cosmetic only.

### Acceptable helpers (not hacks)
- Library loading: `mod.rs:116-345` (`LibraryFileNames` per GOOS/GOARCH including the release suffixes, the `LoadLibrary("")` search, `azRegister`).
- Host-invoker glue: `managed.rs:265-306` (per-arity trampolines), `:388-451` (invoker setters, `createFromHostHandleByref`, `azInitCallbacks`), `:453-488` (handle registry and releaser).
- RefAny upcast/downcast and `Bind[T]`: `managed.rs:565-643`.
- Generic per-wrapper `Raw()`, `Close()` and finalizers (`managed.rs:924-966`, `wrappers.rs:272-303`).
- Keyword and collision escaping: `mod.rs:366-406`, `mod.rs:446-450` (`Close` → `CloseInner`), `types.rs:194-198` (`Tag` → `Tag_`).

### Per-language impact of shared issues
- `TypeCategory::Vec` (4 real Vecs plus the misfiled `InstantPtr`/`StringMenuItem`): lang_go never reads it, so there is no effect. Go also has no Vec→slice helper for any Vec (uniform).
- The `Default`-variant constructor skip has **no effect**. `types.rs:447-472` builds every union variant natively and `functions.rs:212-214` skips `EnumVariantConstructor`; the fresh types.go contains `func AzAccessibilityAction_Default()` and `func AzComponentFieldValueSource_Default()`.
- The 15 kinds outside `HOST_INVOKER_KINDS` are **unusable from Go code**. They get only `type AzTimerCallbackType unsafe.Pointer`. `TimerCreate(refany any, callback *TimerCallback, get_system_time_fn *GetSystemTimeCallback)` needs a `*TimerCallback`, which has no constructor. `RenderImageCallbackCreate(cb AzRenderImageCallbackType)` accepts only a raw C function pointer. The only way in is a hand-rolled `purego.NewCallback` with by-value struct arguments. The same applies to `RegisterComponentLibraryFnType`.

### Derive coverage
- (a) Struct wrappers (`wrappers.rs:705-771`):
  - PartialEq → `Equal(other *T) bool`
  - PartialCmp → `PartialOrder(other) uint8`
  - Cmp → `Order(other) uint8`
  - Hash → `Hash() uint64`
  - Debug → `String() string` (fmt.Stringer; frees the AzString, `:758-766`)
  - Clone → `Clone() *T` (`wrappers.rs:262-264`)
  - Default → `<T>CreateDefault()` (static factory, `:249`)

  Enums, unions and monomorphized aliases get the same methods on `*AzT`, plus `<Name>Default()` (`wrappers.rs:776-858`).
- (b) Distinct symbols referenced by both the FFI layer (functions.go) and the idiomatic layer (wrappers.go + callbacks.go), against azul.h:

  | Derive | Referenced | In azul.h |
  |---|---|---|
  | toDbgString | 2018 | 2018 |
  | partialEq | 1563 | 1563 |
  | partialCmp | 970 | 970 |
  | cmp | 843 | 843 |
  | hash | 871 | 871 |
  | clone | 1114 | 1115 |
  | createDefault | 506 | 506 |

  The one missing clone, `AzBoxDecorationBreak_clone`, is the constructor of the enum VARIANT named `Clone`: `BoxDecorationBreak` is Copy and has no deep-copy. It is a false positive in derive_symbols_azul_h.txt and the only variant-name collision in api.json, so the azul.h clone count is overstated by 1 for every language.
- (c) No derive is dropped. `should_emit_wrapper` (`wrappers.rs:156-183`) filters by type category, not by name, and keeps any class with a declared capability. There are idiom gaps (MINOR): `PartialOrder`/`Order` return the raw 0/1/2 `uint8` rather than Go's -1/0/1 `int`, and there is no `Less` or sort adapter.
- (d) Nothing is re-implemented natively. The generated code has no `reflect.` and no struct `==`.

---------------------------------------------------------------------------

## Fortran

The type, FFI and layout layers are generic; `layout.rs` has no per-type sizes. Unlike Go, the callback surface is generic too: every by-value host-invoker-kind argument of every function becomes a typed procedure argument (`wrappers.rs:618-625`). The problems are:
- the build file is the hello-world's;
- the String helper is found by method name;
- a hand-written GL table sits ahead of alias resolution;
- the derive exports of VecRef/DestructorOrClone structs are dropped;
- the shared `Default`-variant skip makes two variants unconstructible.

### CERTAIN
- **[HELLO-WORLD-ONLY]** The generated binding's Makefile builds only the hello-world.
  - `doc/src/codegen/v2/lang_fortran/makefile.rs:17-18` — "Generate the Makefile for the split binding + the Fortran hello-world example".
  - `:120` — `EXE     := hello_world`.
  - `:134-138` — `hello_world.o: hello_world.f90 azul.o` / `$(EXE): hello_world.o $(AZUL_OBJS)`.
  - It ships as target/codegen/fortran/Makefile, and `scripts/e2e_language_matrix.sh:1905` copies it over the example's Makefile. Any program not named hello_world.f90 needs a hand-edited Makefile.
  - Related staleness: the tracked `examples/fortran/Makefile` still builds the pre-split single file (`azul.o azul.mod: azul.f90`, `$(EXE): hello_world.o azul.o`). It has been dead since the module split in e937cf440 (2026-09-14) and only works because the e2e overwrites it.
  - Fix: emit an `azul` library target (archive of `$(AZUL_OBJS)`) plus `PROG ?= main`; keep the hello-world rule in the example's own Makefile.
- **[NAME-KEYED SPECIAL CASE]** `lang_fortran/wrappers.rs:437-445` — `f.class_name == s.name && f.method_name == "copy_from_bytes" && f.args.len() == 3`. The String class is found by category, but its constructor is found by method name (committed code, not WIP). If it is renamed, `find_string_class` returns `None` and every `String` parameter silently turns from `character(len=*)` into a `string_t` wrapper, with no diagnostic. Go uses a different literal (`from_utf8`). Fix: select the constructor by shape (`TypeCategory::String`, `(*const u8, usize[, usize]) -> String`) and fail codegen loudly if none matches.
- **[SILENT SKIP]** `lang_fortran/functions.rs:101-112`. The derive carve-out covers only `find_enum(..)` with DestructorOrClone|Recursive and structs `== TypeCategory::Recursive`. VecRef and DestructorOrClone STRUCTS fall through to `:117-126` `return false`.
  - The fresh FFI layer lacks 29 derive symbols:
    - `_toDbgString` ×14 and `_clone` ×12 for F32VecRef, GLbooleanVecRefMut, GLenumVecRef, GLfloatVecRefMut, GLint64VecRefMut, GLintVecRefMut, GLuintVecRef, I32VecRef, RefstrVecRef, TessellatedSvgNodeVecRef, U8VecRef and U8VecRefMut;
    - partialEq/partialCmp/cmp/hash for U8VecRef, InstantPtrCloneCallback and InstantPtrDestructorCallback.
  - The VecRef category itself comes from the hard-coded `VECREF_TYPE_NAMES` list (shared), so this drop is effectively name-keyed. Go binds all of them.
  - Fix: let `func.kind.is_declared_capability()` through for every category, as `lang_go`'s `has_declared_capability` does.
- **[MINOR]** GL alias names are wired in ahead of the generic alias resolution:
  - `lang_fortran/mod.rs:590-605` — `"bool" | "GLboolean" => "logical(c_bool)"`, `"i32" | "u32" | … | "GLint" | "GLuint" | "GLenum" | "GLbitfield" | "GLsizei"` and so on;
  - the same table at `lang_fortran/layout.rs:67-72`.

  Both files already resolve aliases through the IR (`mod.rs:646-656`, `layout.rs:117-122`). api.json declares `GLboolean = u8`, so the hard-coded arm silently changes it from `integer(c_int8_t)` to `logical(c_bool)`. `GLint64`, `GLdouble` and `GLclampd` do not exist in api.json at all. Fix: delete the GL arms.
- **[NAME-KEYED SPECIAL CASE] [WIP]** `lang_fortran/managed.rs:429-466` (`kind_logger`, uncommitted). It finds the sink with `f.method_name == "log"` and the level with `.position(|v| v.name == "Error").unwrap_or(0)`. If `Error` is ever renamed, the fallback index 0 is `AppLogLevel::Off`, which silently discards every binding error report. Fix: fall back to the stderr `azul_report` path, or fail codegen, instead of `unwrap_or(0)`.

### SUSPECT
- The smart factory `window_create_options_create(layout)`, `lang_fortran/wrappers.rs:1088-1126`, replaces the raw function-pointer constructor via `:340-343`. It comes from `layout_callback_factory_info`, which is structural, but only WindowCreateOptions matches, and it is the hello-world entry point.
- `lang_fortran/wrappers.rs:272-276` and `:330-336` resolve case-fold clashes with `try_claim`, which silently drops a unit-enum constant or a type-bound binding (the module procedure stays). This is latent: the fresh output emits all 2116 expected constants.
- The fallback `lang_fortran/mod.rs:683-685` — `"type(c_ptr)"` for unknown types "so the generated module still compiles" — is silent. No hits observed: the only `! SKIPPED:` lines are 4 generic templates (CssPropertyValue, BoxOrStatic, PhysicalSize, PhysicalPosition), whose monomorphized aliases are emitted, and there are zero legacy `WARNING` union shapes (`types.rs:369-378`).
- Example versus WIP: the fresh output (built with the uncommitted managed.rs) declares `layout_callback_iface` with `class(*), intent(inout) :: arg0`. The committed `examples/fortran/hello_world.f90` still takes `type(ref_any_t), intent(inout) :: data`, which is an interface mismatch. The person editing it needs to update the example.

### Acceptable helpers (not hacks)
- Host-invoker runtime, all derived per kind from `host_invoker_kinds`: `managed.rs:497-616` (abstract interfaces, handle table, `AzApp_set*Invoker`, `*_createFromHostHandle`, `AzRefAny_newHostHandle/getHostHandle`) and `:660-940` (invokers, registration, lazy `azul_ensure_invokers`).
- RefAny `ref_any_create`, `%get()` and the `MODEL_OF` upcast: `managed.rs:942-982`.
- String in/out helpers (`wrappers.rs:826-872`), apart from the name lookup reported above.
- Keyword, intrinsic and length handling: `mod.rs:696-826`, `wrappers.rs:548-590` (dummy_name CLASHES).
- `layout.rs`, the generic C layout computation (verified against clang).

### Per-language impact of shared issues
- `TypeCategory::Vec` and the misfiled `InstantPtr`/`StringMenuItem`: lang_fortran never reads the category, so there is no effect. It has no Vec→array helper for any Vec (uniform).
- **`Default`-variant constructor skip, CERTAIN impact.** The fresh `azul_ffi_*.f90` binds `AzAccessibilityAction_<every other variant>` and `AzComponentFieldValueSource_binding/_literal`, but no `_default`. Tagged unions are opaque blobs (`types.rs:360-378`), so `AccessibilityAction::Default` and `ComponentFieldValueSource::Default` cannot be built from Fortran at all.
- The 15 kinds outside `HOST_INVOKER_KINDS` have no interface or `azul_register_*`. For example, `timer_create(refany, callback, get_system_time_fn)` takes `type(timer_callback_t)`, which has no constructor. The only route is a user `bind(C)` function matching `AzTimerCallbackType_iface` (`azul_types_time.f90:199`), with its `c_funloc` stored in `callback%raw%cb` by hand.
- The hard-coded `VECREF_TYPE_NAMES` causes the 29 dropped derive symbols above.

### Derive coverage
- (a) Struct wrapper classes (`Ctx::wrappers`, `wrappers.rs:170-178`: structs with functions, excluding the String class and the `HOST_INVOKER_KINDS` structs) get each derive as a type-bound procedure named after the IR method. They come from `plan_classes`, `wrappers.rs:311-339`, and `takes_self`, `:510-526`:
  - `x%toDbgString()` → `character(:)`
  - `x%partialEq(y)` → `logical`
  - `x%partialCmp(y)` and `x%cmp(y)` → raw `integer(c_int8_t)`
  - `x%hash()` → `integer(c_int64_t)`
  - `x%clone()` → an owned wrapper
  - `<snake>_createDefault()` as a factory

  There is no `operator(==)`, `operator(<)` or `generic :: assignment(=)` (verified absent in the fresh azul_api.f90).
- (b) Distinct symbols in the FFI layer (`bind(C, name=…)`) and in the idiomatic layer (references in azul_api), against azul.h:

  | Derive | FFI layer | Idiomatic layer | azul.h |
  |---|---|---|---|
  | toDbgString | 2004 | 1000 | 2018 |
  | partialEq | 1560 | 751 | 1563 |
  | partialCmp | 967 | 461 | 970 |
  | cmp | 840 | 375 | 843 |
  | hash | 868 | 387 | 871 |
  | clone | 1103 | 577 | 1115 |
  | createDefault | 506 | 331 | 506 |

- (c) FFI-only derives, by kind, for toDbgString: 713 tagged unions, 227 unit enums, 62 callback-kind structs, String, and 1 monomorphized alias. clone: 461 unions and 62 callback kinds. createDefault: 136 unit enums and 37 unions. The filter is by category, not by name, but it builds on the shared kind list. The 29 symbols missing from the FFI layer are covered in CERTAIN above.
- (d) Derives re-implemented natively:
  - **CERTAIN.** Wrapper types get no `assignment(=)`, so intrinsic `b = a` copies `raw` bitwise and `owned = .true.` into both, and `_clone` is never called. Two `delete`s then double-free. The same goes for the 461 non-Copy tagged unions, which are exposed only as `bind(C)` blobs.
  - **CERTAIN, correctness rather than a hack.** A consumed wrapper argument is moved without clearing its flag: `subroutine dom_with_child(self, child)` has `type(dom_t), intent(in), target :: child` and `az_dom_with_child(..., azul_take_dom(child))`, and `azul_take_<snake>` (`wrappers.rs:887-916`) returns `x%raw` while `child%owned` stays `.true.`. A later `call child%delete()` double-frees.
  - Unit-enum PartialEq is native integer `==`, which is equivalent and fine.


---

<!-- haskell_d.md -->

**Source partial:** Bindings hack audit: Haskell and D (partial report)

Audited 2026-09-19, 15:25 to 15:40. Both emitters were moving targets during the audit:

- **lang_haskell**: line numbers are from **HEAD** (e937cf440). Another agent was rewriting the working tree at the same time (uncommitted diff to `cabal.rs` and `wrappers.rs`, +605/-259, last write 15:37). Every CERTAIN finding below exists in both HEAD and the working tree. Where the numbers differ, the WIP line is given as `[WIP n]`.
- **lang_d**: line numbers are from the **working tree**. It carries an uncommitted diff from 15:25 to 15:26 that changes callbacks from delegates to free functions. `model.rs` and `types.rs` have not changed since 16 Sep.
- **Output**: `target/codegen` was regenerated three times by another process during the audit. All output numbers come from a snapshot of `target/codegen/{haskell,d,azul.d}` taken at 15:36:33. Its Haskell part was produced by the intermediate WIP emitter. The derive-surfacing functions (`emit_show_instance`, `emit_eq_instance`, `should_wrap`, `function_name`) are byte-identical in HEAD and the WIP.
- **Correction to the audit's working figure**: `HOST_INVOKER_KINDS` has **62** entries, not 63. A regex count finds a 63rd because of the `extern "C"` inside a comment at `managed_host_invoker.rs:127`. The figure of 77 non-destructor callback typedefs with 15 outside the list (77 = 62 + 15) is still correct.

---

## Haskell

### CERTAIN

- **[SILENT SKIP]** `doc/src/codegen/v2/lang_haskell/wrappers.rs:1149` [WIP 1450]
  - Code: `if ctx.ir.callback_typedefs.iter().any(|c| c.name == t) { return None; }`, then `wrappers.rs:1291-1297` emits `"-- SKIPPED: {} ({}): an argument has no Haskell shape yet"`.
  - What happens: any function whose callback parameter is the bare fn-pointer typedef, rather than a `HOST_INVOKER_KINDS` wrapper struct, is dropped from the idiomatic layer. The only trace is a comment in the generated source. There is no codegen-time warning.
  - In the output, 13 functions are dropped: `domAddCallback`, `domWithCallback`, `nodeDataAddCallback` (these are `Dom::add_callback(event, data: RefAny, callback: CallbackType)`, the generic "attach an event handler to any node" API), plus `stringMenuItemWithCallback`, `callbackCreate`, `layoutCallbackCreate`, `renderImageCallbackCreate`, `iconProviderHandleWithResolver`/`SetResolver`, `datasetMergeCallbackFrom`, `appConfigAddComponentLibrary`, `fontRefCreate` and `refAnyNewC`.
  - The two callback shapes the hello-world uses both work: the wrapper-struct form of `buttonWithOnClick`, and the layout factory. Generic node event handlers are only reachable through the raw FFI layer.
  - Fix: when the typedef's first argument is `RefAny` and the function has exactly one `RefAny` sibling, carry the closure handle in that data `RefAny`, as lang_d already does (`lang_d/wrappers.rs:1615`). Make an unresolvable function a codegen warning or error.

- **[MINOR]** One C slot per callback kind on the raw path. `doc/src/codegen/v2/lang_haskell/cshim.rs:339-343` and `functions.rs:217-223`.
  - Code: `static {inner} g_{inner} = 0;` and `void {setter}({inner} f) { g_{inner} = f; }`. The doc comment says "One static slot per kind — the newest registration wins for the whole kind."
  - This is the only path for the 15 kinds outside `HOST_INVOKER_KINDS`: Timer, WriteBack, RenderImage, IconResolver, and others. Two timers with different Haskell handlers both run whichever was registered last.
  - The aggregate-return trampoline (`cshim.rs:357-369`) also returns an uninitialised struct when nothing is registered. Generated example: `AzTimerCallbackReturn AzTimerCallbackType_trampoline(...) { AzTimerCallbackReturn __ret; if (g_…) g_…(&_arg0, &_arg1, &__ret); return __ret; }` (`cbits/azul_callbacks.c:596`).
  - User impact: `timerCreate :: RefAny -> TimerCallback -> GetSystemTimeCallback -> IO Timer` (`Azul/Task.hs:388`) has to be fed a hand-built wrapper around the FunPtr from `registerTimerCallbackTypeCallback`.
  - Fix: store the inner function per registration in the ctx or data `RefAny`, as the host-handle path does, and zero-initialise or abort when unset.

- **[MINOR]** Literal `"RefAny"` comparisons. `doc/src/codegen/v2/lang_haskell/wrappers.rs:283, 665, 936, 938, 1177` [WIP 310, 839, 1023, 1027, 1478].
  - Code: `f.class_name == "RefAny"`, `ctx.wrapped.contains("RefAny")`, `a.type_name.trim() == "RefAny"`, `t == "RefAny"`.
  - `managed_lang_helpers::is_refany_type` (which uses `TypeCategory::RefAny`) exists and its doc says bindings "must not compare against the literal `RefAny` spellings". This code is inside the acceptable RefAny carve-out; only the spelling is name-keyed.
  - Fix: use `is_refany_type(t, ctx.ir)`.

- **[MINOR]** String marshalling keyed on a method name. `doc/src/codegen/v2/lang_haskell/wrappers.rs:273` [WIP 300].
  - Code: `let string_from_bytes = find(&string_class, "copy_from_bytes");`. The String class itself is found by category.
  - If `copy_from_bytes` is renamed, `withAzStringArg` silently disappears and every `String` parameter degrades to the `AzString` wrapper handle, because `arg_plan` falls through to `ctx.wrapped.contains(t)`. Nothing reports an error.
  - Fix: find the `(bytes, len) -> String` constructor by its signature, or fail codegen.

- **[MINOR]** Hard-coded enum size. `doc/src/codegen/v2/lang_haskell/types.rs:648-649`.
  - Code: `builder.line("sizeOf _ = 4"); builder.line("alignment _ = 4");` for every fieldless enum. Structs and unions use the cbits layout oracle; lang_d derives the backing type from `repr` (`lang_d/types.rs:265-273`).
  - Correct today, since all 227 fieldless enums are `repr(C)`. It becomes wrong the day a `repr(u8)` fieldless enum is added.
  - Fix: use the oracle's `az_hs_sizeof_<Enum>` / `az_hs_alignof_<Enum>`, as for structs.

- **[MINOR]** Stale manifest description. `doc/src/codegen/v2/lang_haskell/cabal.rs:56-59` (HEAD).
  - Code: `"exposes 'RefAny' as a phantom-typed newtype so downcasts are statically tracked"`.
  - Not what the binding does: it uses a handle table plus `Data.Dynamic`. The uncommitted WIP already rewrites this text.

### SUSPECT

- **[HELLO-WORLD-ONLY?]** Layout factory. `doc/src/codegen/v2/lang_haskell/wrappers.rs:1039-1103` and `1261-1264`.
  - `emit_layout_factory`, driven by the shared `layout_callback_factory_info`, matches exactly one class in the IR: `WindowCreateOptions`. Output: `windowCreateOptionsCreate :: LayoutCallbackHandler h => h -> IO WindowCreateOptions` (`Azul/Window.hs:1434`).
  - It also hides the raw constructor: `if has_factory && name == format!("{}Create", …) { continue; }`.
  - It is derived structurally, not keyed on a name, but it exists for the hello-world's `WindowCreateOptions.create(layout)`. Maintainer's call.
- **[NAME-KEYED SPECIAL CASE, WIP only]** `mismatch_logger`, working-tree `wrappers.rs:1035-1072` (uncommitted).
  - Callback failures are reported through the first borrowed argument whose class has a method matching `f.method_name == "log"` with a `String` third argument, at the level variant `v.name == "Error"` (falling back to the first variant).
  - It is callback glue, but it is keyed on a method name and a variant name.
- **[SUSPECT, WIP only]** Model-bound siblings, working-tree `wrappers.rs:1619-1646`.
  - Every `with_on_*` method gets a model-bound sibling `<class>On<Event>` via the shared `smart_callback_setter_info` (keyed on the `with_` prefix).
  - This is generic: HEAD output already has 58 closure-taking `with*` setters. It is prefix-keyed, and its purpose is hello-world ergonomics.
- **[SUSPECT]** `refAnyUpdate`, HEAD `wrappers.rs:713-718`.
  - A generic RefAny helper whose doc example is literally the hello-world click handler (`Update_RefreshDom`). Acceptable; the WIP removes it.
- **[MINOR]** Opaque fallback for unknown types. `doc/src/codegen/v2/lang_haskell/types.rs:1124-1128`.
  - Any type the IR does not know maps silently to `"(Ptr ())"` ("keep as opaque pointer so the binding still type-checks"). I did not count how often this triggers.

### Acceptable helpers (not hacks)

- **Handle table and RefAny helpers**: releaser, `refAnyCreate`, `refAnyGet`, `refAnyModify`, `withRefAnyClone` (`wrappers.rs:467-534, 664-731`).
- **Per-kind callback glue** for all 62 `HOST_INVOKER_KINDS`: closure type, handler class, `azulInvoke<K>`, `azulRegister<K>`, `azulEnsureManaged` (`wrappers.rs:800-1029`).
- **Raw callback glue for every typedef**: `mk_<X>`, inbound trampolines, `register<X>Callback` (`functions.rs:73-356`).
- **`cshim.rs`**: per-function `_via` shims, the layout oracle and the host-invoker `_via` shims. Fully generic, with no API names at all.
- **Codec and escaping**: the UTF-8 codec (`mod.rs:370-422`), the `shadows_prelude_type` identifier escaping (`mod.rs:666-695`) and reserved-word escaping.
- **Cabal manifest**: module list, C sources and version are all derived from the split and `ir.api_version`.

### Per-language impact of shared issues

- **`TypeCategory::Vec` name list, misfiled `InstantPtr`/`StringMenuItem`**: no impact. Haskell never reads the Vec category; `<vec>ToList` is detected by layout (`types.rs:469-493`), so all 127 Vecs get it.
- **Skipped `Default`-variant constructor**: no impact. Tagged unions are Haskell sum types written through `Storable`, so `T.AccessibilityAction_Default` and `T.ComponentFieldValueSource_Default` can be constructed directly.
- **The 15 callback kinds outside `HOST_INVOKER_KINDS`**:
  - They only get the raw one-slot `register<X>Callback` path (CERTAIN finding 2).
  - Functions that take them as raw typedefs are among the 13 skipped functions (finding 1): `renderImageCallbackCreate`, `iconProviderHandle*`, `datasetMergeCallbackFrom`, and `appConfigAddComponentLibrary` (`RegisterComponentLibraryFnType`).
  - `timerCreate` needs a hand-built `TimerCallback`.
  - All 62 listed kinds do get closure types.
- **VecRef name list and DestructorOrClone suffix rule**:
  - `functions.rs:374-385` exempts derive entry points from the category filter only for DestructorOrClone *enums* and Recursive structs.
  - As a result, 38 derive symbols are not even declared as `foreign import`s. They belong to the VecRef structs from the name list (e.g. `U8VecRef`, `GLuintVecRef`) and to `InstantPtrCloneCallback`/`InstantPtrDestructorCallback`.

### Derive coverage

**(a) How each capability surfaces.** Idiomatic surfacing exists only for wrapper classes, i.e. structs with `_delete` or with methods (`should_wrap`, `wrappers.rs:330-360`). The mapping is keyed on `FunctionKind` and `s.traits`, not on names.

| Rust trait | Haskell surface | Emitter |
|---|---|---|
| Debug | `instance Show W` via `c_Az{T}_toDbgString_via` | `wrappers.rs:617-638` |
| PartialEq | `instance Eq W` via `c_Az{T}_partialEq` | `wrappers.rs:641-658` |
| PartialOrd / Ord | not surfaced: no `instance Ord` anywhere in the output | none |
| Hash | not surfaced: no `Hashable`; the cabal file depends only on `base` and `containers` | none |
| Clone | `<class>Clone :: W -> IO W` | `function_name`, `wrappers.rs:1232-1240` |
| Default | `<class>Default :: IO W` | same |

**(b) Numbers.** Distinct symbols, intersected with the azul.h ground truth. "Haskell FFI" means `Internal/FFI/*.hs`; "Haskell idiomatic" means `Azul/*.hs` plus `Internal/{Runtime,Handles,Callbacks}`.

| capability | azul.h | Haskell FFI | Haskell idiomatic |
|---|---|---|---|
| toDbgString | 2018 | 2004 | 820 |
| partialEq | 1563 | 1560 | 701 |
| partialCmp | 970 | 967 | **0** |
| cmp | 843 | 840 | **0** |
| hash | 871 | 868 | **0** |
| clone | 1115 | 1103 | 640 |
| createDefault | 506 | 506 | 333 |

The output contains 820 C-backed `instance Show` and 701 `instance Eq`, all on wrappers in `Internal/Handles`.

**(c) Declared in the FFI but never surfaced.** Enums are never wrapped, because `wrapped` is built from `ir.structs` only, and `emit_class_functions` runs only for wrapped structs (`wrappers.rs:126-131`). So every capability of fieldless enums, tagged unions, Options, Results and plain Copy structs stays raw-only. The filter is kind-based, not name-keyed. Idiomatic gaps by kind:

| capability | missing | fieldless enum | tagged union | Option | Result | Copy struct | other struct |
|---|---|---|---|---|---|---|---|
| toDbgString | 1198 | 227 | 373 | 315 | 25 | 244 | 14 |
| partialEq | 862 | 226 | 243 | 275 | 1 | 114 | 3 |
| clone | 475 | 2 | 207 | 229 | 25 | 0 | 12 |
| createDefault | 173 | 136 | 36 | 1 | 0 | 0 | 0 |

`partialCmp`, `cmp` and `hash` are missing for every type (970, 843 and 871).

**(d) Traits re-implemented in Haskell instead of calling C.**

- [MINOR] `Show` and `Eq` are derived natively in several places:
  - 1109 struct records end in `} deriving (Show)` (`types.rs:425`).
  - 227 fieldless enums use `deriving (Show, Eq, Enum, Bounded)` (`types.rs:642`).
  - 895 tagged or monomorphized unions use `deriving (Show)` (`types.rs:797`).
  - None of these call `_toDbgString` or `_partialEq`.
  - Plain structs and unions get **no `Eq` at all**, even where Rust derives `PartialEq`.
- [SUSPECT] Bitwise copy of non-Copy types. Non-Copy unions (`Option*`/`Result*`/data enums that hold owned pointers) are plain `Storable` values.
  - Their 207 union and 229 Option `_clone` functions are never used.
  - Example: `nodeDataWithMarker :: T.OptionString -> NodeData -> IO NodeData` (`Azul/Dom.hs:3460`) does `alloca` and `poke __p1 a1`, then passes the value by value, with no clone and no move tracking. The same Haskell value can be passed to a second call, giving two owners of one `AzString` buffer.
  - In the other direction, 366 functions return `IO T.Option*` via `peek`, and the owned payload is never deleted.
  - 19 functions take a `T.Option*` by value.

---

## D

### CERTAIN

- **[MINOR]** GL alias names hard-coded. `doc/src/codegen/v2/lang_d/model.rs:62-76`.
  - `Prim::from_rust` spells out 14 GL alias names: `"bool" | "GLboolean"`, `"u32" | "c_uint" | "GLuint" | "GLenum" | "GLbitfield"`, `"i32" | "c_int" | "GLint" | "GLsizei"`, `"GLuint64"`, `"GLint64"`, `"GLfloat" | "GLclampf"`, `"GLdouble" | "GLclampd"`, and `"isize" | "ssize_t" | "intptr_t" | "GLsizeiptr" | "GLintptr"`.
  - `Model::unalias` (`model.rs:550-559`) already resolves `ir.type_aliases`, so these names are redundant.
  - `GLboolean` is deliberately mapped to D `bool` instead of its alias target `u8`.
  - Fix: resolve through `unalias` and delete the GL names.
- **[MINOR]** Literal `"String"`/`"RefAny"` checks. `doc/src/codegen/v2/lang_d/model.rs:626, 650` (`if name == "String"`) and `630, 657` (`class.category == TypeCategory::RefAny || name == "RefAny"`).
  - Fix: use `TypeCategory::String` / `TypeCategory::RefAny` only.
- **[MINOR]** Hand-written C symbol and field names in the runtime. `doc/src/codegen/v2/lang_d/runtime.rs:190-191, 207, 259-263, 268-270`.
  - It calls `AzString_fromUtf8`, `AzString_delete`, `AzRefAny_newC(data, …, &_azulHeldDestructor, 0, 0)`, `AzRefAny_isType` and `AzRefAny_getDataPtr`.
  - It fills `AzGlVoidPtrConst.ptr` and `.run_destructor`, and reads `AzString.vec.ptr` and `.len`.
  - These belong to acceptable categories (string conversion, RefAny wrap and downcast), but they are hand-spelled rather than resolved from the IR. An api.json rename would only surface when the D code is compiled. Haskell resolves its string constructor from the IR.
  - Fix: emit the names from IR lookups, or assert at codegen time that they exist.
- **[MINOR]** Literal fallbacks in the layout factory. `doc/src/codegen/v2/lang_d/wrappers.rs:1989-1995`.
  - Code: `callback_ctx_field(&fac.callback_wrapper, m.ir).unwrap_or_else(|| "ctx".to_string())`, `"AzOptionRefAny_delete(__ctx);"` and `"*__ctx = _wrap_OptionRefAny(_azulRefAny(null, __fn0));"`.
  - The `__fn0` is safe because of the `args.len() == 1` guard at line 1583. The silent fallback to the field name `"ctx"` is not.
  - Fix: skip or error when the ctx field is not found.

### SUSPECT

- **[MINOR]** Ctx read-back keyed on a method name. `doc/src/codegen/v2/lang_d/model.rs:821-831`.
  - `ctx_getter` looks for `f.method_name == "get_ctx"` returning `Some("OptionRefAny")`.
  - A callback kind whose info type lacks such a method cannot carry the D function in its ctx. It silently falls back to carrying it in the data `RefAny`, or to a raw C function pointer. This rests on a naming convention, not on IR metadata.
- **[MINOR]** Raw `extern(C)` fallback for some callbacks. `doc/src/codegen/v2/lang_d/wrappers.rs:1613-1619` and `1752-1781`.
  - When neither the ctx nor a single `RefAny` sibling can carry the function, the callback stays a raw C function pointer.
  - Affected: `Thread(Object, Object, AzThreadCallbackType callback)` and `createThread` (two `RefAny` arguments), five `withMergeCallback`/`setMergeCallback(AzDatasetMergeCallbackType)` members plus `DatasetMergeCallback.from`, `IconProviderHandle.withResolver`/`setResolver(AzIconResolverCallbackType)`, and `AppConfig.addComponentLibrary(string, AzRegisterComponentLibraryFnType)`.
  - This is a generic rule, not keyed on names, and it is visible only in the signatures. 152 member signatures take a typed D function; every other raw callback parameter is a struct field setter (e.g. `void cb(AzXCallbackType v)`) or a destructor.
- **[MINOR]** Hand-written README hello-world. `doc/src/codegen/v2/lang_d/mod.rs:221-324`.
  - It is not covered by `tests/compile_check.d` (which covers every generated member), so it can drift.
- **Note**: lang_d was under active uncommitted modification during the audit (see the header).

### Acceptable helpers (not hacks)

- **Runtime (`runtime.rs`)**: refcounted `_AzulRc` handle boxes, the `_AzulHeld` GC roots, `_azulRefAny`/`_azulObject`/`_azulFn` (RefAny wrap and downcast), the thread attach, the exception guard, and the string conversions.
- **Callback plumbing**: one trampoline and one invoker per typedef, generated on demand (`wrappers.rs:2188-2346`).
- **Name escaping**: `reserved_member` (`wrappers.rs:89-112`), `is_d_keyword` and `is_object_name` in `mod.rs`.
- **Raw layer**: `types.rs` and `functions.rs` are fully generic. They declare every exported symbol and emit the `…Struct` twin via the shared `has_callback_wrapper_arg`.
- **Generated diagnostics**: the file header lists anything left in the C ABI only. Today that list is empty; the only functions not given their own member are 10 that are covered by a same-named variant factory (`AzHttpError_*`, `AzPixelValueOrSystem_*`, `AzSvgPathElement_*`). The file also prints native-mapping stats: Option 316/317, Vec 127/127, Result 23/24.

### Per-language impact of shared issues

- **`TypeCategory::Vec`, misfiled `InstantPtr`/`StringMenuItem`**: no impact. `vec_shape` (`model.rs:713-749`) checks the name suffix, the 4-field layout and that `copyFromPtr` exists, which gives `T[]` for all 127 Vecs.
- **Skipped `Default`-variant constructor**: no impact. D writes the tag and payload itself (`wrappers.rs:1054-1067`); the output contains `static AccessibilityAction default_()` (`dom.d:678`) and `static ComponentFieldValueSource default_()` (`component.d:2629`).
- **`HOST_INVOKER_KINDS`**: not used for dispatch. D has its own trampoline per typedef; the shared list only selects the `…Struct` symbol (`wrappers.rs:2078-2084`).
  - Of the 15 kinds outside the list, Timer is typed by carrying the function in the data `RefAny`: `static Timer opCall(T)(T refany, TimerCallbackReturn function(T, TimerCallbackInfo) callback, GetSystemTimeCallback getSystemTimeFn)` (`task.d:1333`).
  - `Dom.withCallback`/`addCallback(EventFilter, T data, Update function(T, CallbackInfo))` are typed the same way, which is exactly what Haskell skips.
  - DatasetMerge, IconResolver and RegisterComponentLibrary stay raw.

### Derive coverage

**(a) How each capability surfaces.** All seven are bound in `emit_traits` (`wrappers.rs:1323-1463`), keyed on `m.trait_fn(name, FunctionKind)` (`model.rs:526-531`), with no names involved.

| Rust trait | D surface |
|---|---|
| Debug | `string toString() const` |
| Clone | `dup()`: bitwise for Copy types, otherwise `_clone`; `_take()` of a field view also uses `_clone` (`918-921`) |
| PartialEq | `opEquals(ref const rhs)` plus a by-value overload |
| Ord | `int opCmp` via `_cmp` |
| PartialOrd | `float opCmp` via `_partialCmp` (NaN when incomparable), used only when there is no Ord |
| Hash | `toHash()` |
| Default | `static defaultValue()`, `defaultValue!T()`, and `T()` when no zero-argument `create` exists |

Fieldless enums get only `defaultValue(T : Enum)()` (`wrappers.rs:716-731`).

**(b) Numbers.** Distinct symbols, intersected with the azul.h ground truth. "D raw" is `d/source/azul/raw.d`; "D idiomatic" is every other module.

| capability | azul.h | D raw | D idiomatic | azul.d |
|---|---|---|---|---|
| toDbgString | 2018 | 2018 | 1349 | 2018 |
| partialEq | 1563 | 1563 | 960 | 1563 |
| partialCmp | 970 | 970 | 72 | 970 |
| cmp | 843 | 843 | 459 | 843 |
| hash | 871 | 871 | 472 | 871 |
| clone | 1115 | 1115 | 756 | 1115 |
| createDefault | 506 | 506 | 504 | 506 |

The low `partialCmp` figure is by design, since `cmp` is preferred. 531 of the 970 types with an ordering function get `opCmp`; the remaining 439 are 188 Options, 179 fieldless enums and 72 Vecs, which are all native D types.

**(c) Declared but not surfaced.** This happens only for natively mapped types and fieldless enums: Options become `Nullable!T`, Vecs become `T[]`, `String` becomes `string`, and fieldless enums become D `enum`s. The filter is kind-based (`owned`/`is_native`), and the README documents it ("A D enum's own `==`, `<`, hashing and `to!string` stand in for the derives of a fieldless enum"). Nothing that D declares as its own type is missing a capability.

**(d) Traits re-implemented in D instead of calling C.**

- [MINOR] Fieldless enums use D's built-in `==`, `<`, `toHash` and `to!string`. The semantics match, except that `to!string` prints the lowerCamel D member (`primary`) where Rust's Debug prints `Primary`.
- [SUSPECT] Structs that have no Rust `PartialEq`/`Hash` still get D's implicit member-wise `==` and `toHash`:
  - 271 of 887 handle structs have no `opEquals`, so `==` compares the `_rc` box pointers (identity); 645 have no `toHash`.
  - 329 of 673 plain structs have no `opEquals`, so `==` compares the `_raw` C struct member by member; 443 have no `toHash`.
  - Rust defines no equality for these types. Fix: `@disable bool opEquals` and `@disable size_t toHash` where the derive is absent.
- **No bitwise copy of non-Copy types.** `dup()` of a non-Copy type calls `_clone`. Plain (bitwise) structs are only Copy types whose fields are all plain (the fixpoint in `model.rs:456-510`). `Nullable!T`/`T[]` are rebuilt as fresh C values on every crossing (`_in_*`/`_take_*`).


---

<!-- crystal_swift.md -->

**Source partial:** Crystal + Swift: bindings hack audit (partial)

Scope: `doc/src/codegen/v2/lang_crystal/` (mod, model, runtime, types, functions, wrappers) and `doc/src/codegen/v2/lang_swift/` (mod, model, runtime, wrappers), the generated `target/codegen/azul.cr` and `target/codegen/azul.swift`, and `examples/crystal/hello-world.cr` / `examples/swift/hello-world.swift`. Read-only; no cargo.

Note: `target/codegen/` was wiped and regenerated several times during the audit (a concurrent `codegen all`). All generated-output numbers come from the 15:29 to 15:33 regenerations. Generated-file line numbers are approximate.

**Bottom line for both languages:** no emitter branch or string literal is keyed on a hello-world API name. `App`, `Dom`, `Button`, `WindowCreateOptions`, `Update` and `LayoutCallback` appear only in comments, unit tests and the README's hello-world text (`lang_crystal/mod.rs:185-210`, `lang_swift/mod.rs:216-243`). Both emitters derive the shape of each type from the IR's structure:
- Option: two variants, `None` and `Some(T)`.
- Vec: `{ptr,len,cap,destructor}` plus `_copyFromPtr`.
- Result: `Ok`/`Err`.
- Unions: tagged, as tagged in the IR.
- Callbacks: any callback typedef, or any linked callback wrapper struct.

Both examples use only generated generic surface. The one special path they hit is the shared, structurally derived layout-callback factory (`WindowCreateOptions.new(->layout)` / `WindowCreateOptions(layout)`).

---

## Crystal

### CERTAIN
- **[MINOR]** `doc/src/codegen/v2/lang_crystal/wrappers.rs:405-432`: `w.l(2, "self");` (clone), `w.l(2, "value == other.value");`, `w.l(2, "value <=> other.value");`, `w.l(2, "value.hash(hasher)");`. For fieldless enums, the Rust `Clone`/`PartialEq`/`Ord`/`Hash` derives are re-implemented in Crystal by comparing discriminants, instead of calling the bound C functions. `LibAzul` declares `az<T>_partialEq/_cmp/_partialCmp/_hash/_clone` for 226/178/179/190/2 fieldless enums, and none are called (generated example: `enum LibAzul::AzUpdate` → `def ==(other : self) … value == other.value`). This depends on type kind, not on a name. It is semantically equivalent today but bypasses the C API. Fix: emit the same calls as the class path (`wrappers.rs:664-697`), passing `pointerof(__v)`.
- **[MINOR]** `lang_crystal/model.rs:440` `if name == "String" && self.classes.contains_key("String")` and `:443` `if name == "RefAny" && self.classes.contains_key("RefAny")`. String and RefAny are recognised by literal name, not by `TypeCategory::String`/`TypeCategory::RefAny`. `managed_lang_helpers::is_refany_type` exists precisely so that "bindings must not compare against the literal "RefAny" spelling". Each is the single type of its kind, so the behaviour is right today. Fix: test `class.category`.
- **[MINOR]** `lang_crystal/wrappers.rs:1265` `callback_ctx_field(&fac.callback_wrapper, m.ir).unwrap_or_else(|| "ctx".to_string())` plus `:1272` `...as(::Pointer(LibAzul::AzOptionRefAny))` and `:1276` `w.l(2, "LibAzul.azOptionRefAny_delete(__ctx)");`. In the layout-factory splice, the ctx field name falls back to a guessed literal `"ctx"`, and the delete symbol is hard-coded (every other path goes through `m.fun("OptionRefAny","delete")`). Callback glue, but a silent guess. Fix: drop the factory (return `None`) when there is no ctx field, and use `m.fun`.

### SUSPECT
- **[HELLO-WORLD-ONLY?]** `lang_crystal/wrappers.rs:1008-1019` (`layout_factory_candidate` via shared `layout_callback_factory_info`) and `:1254-1278` (splice). This produces `def self.new(layout_callback : ::Proc(T, Azul::LayoutCallbackInfo, Azul::Dom)) : Azul::WindowCreateOptions forall T` (azul.cr ≈354296). The code calls the raw `azWindowCreateOptions_create(Trampolines::LayoutCallbackType)`, then pokes the closure handle into `window_state.layout_callback.ctx` by `offsetof` chain. This is exactly what the hello-world uses (`Azul::WindowCreateOptions.new(->layout(...))`). The emitter code is generic (it walks `field_path`/`field_types`), but the shared detector matches only `WindowCreateOptions` today. For the maintainer to decide: keep as a generic "ctor with a single ctx-less callback" rule, or have api.json expose the ctx properly (`create(cb, ctx)`) so no splice is needed.
- **[MINOR]** `lang_crystal/model.rs:583-593` `f.method_name == "get_ctx" && … f.return_type.as_deref() == Some("OptionRefAny")`. "Closure travels in the callback's ctx" is keyed on an API method literally named `get_ctx`. Only 4 info types have it (`CallbackInfo`, `LayoutCallbackInfo`, `VirtualViewCallbackInfo`, `RenderImageCallbackInfo`). Every other callback needs the "data RefAny carries the closure" path, or falls back to a raw C fn pointer. Fix: derive the ctx accessor from IR metadata (e.g. mark it in api.json), not from the method name.
- **[MINOR]** Closure coverage is generic but incomplete:
  - Callbacks registered only through struct fields get raw non-capturing setters: `ListViewOnLazyLoadScrollCallback` and the 8 NodeGraph `OnNode*Callback` kinds, e.g. `OnNodeAddedCallback#cb=(value : LibAzul::AzOnNodeAddedCallbackType)`.
  - `Thread.new(thread_initialize_data : ::Reference, writeback_data : ::Reference, callback : LibAzul::AzThreadCallbackType)` (azul.cr ≈336839) is raw because `Thread.create` has two RefAny args, so neither the ctx rule nor the carry-in-data rule applies.
  - None of this is name-keyed; it is noted for completeness.

### Acceptable helpers (not hacks)
- `lang_crystal/runtime.rs` (whole file):
  - `MovedError`/`ResultError`.
  - `Wrapper`/`Storage(L)` ownership.
  - `Handles`: the RefAny handle table (`azRefAny_newC`, `azRefAny_isType`, `azRefAny_getDataPtr`, `azRefAny_clone`). This is the RefAny upcast/downcast glue.
  - `Native`: String conversion (`azString_fromUtf8`, `azString_delete`), `refany`, and `guard` (stops exceptions unwinding into C).
- Per-typedef trampolines (`wrappers.rs:1471-1547`) and erased closures (`:1429-1469`).
- `<c_name>Struct` symbol selection via the shared `has_callback_wrapper_arg` (`functions.rs:49-51,57-79`; `wrappers.rs:1339-1345`).
- Generic Option/Vec/Result conversions (`model.rs:479-567`; `wrappers.rs:1553-1734`).
- Uniform naming rules: `create`→`new`, `create_x`→`x`, `get_x`→`x`, `is_x`→`x?`, `set_x`→`x=` (`wrappers.rs:923-962`).
- Keyword escaping (`mod.rs:411-496`), `reserved_method` (`wrappers.rs:146-175`), and `@[Link("azul")]` (`mod.rs:72`).
- Unshaped functions are reported in the file header (`mod.rs:247-252`, collected at `wrappers.rs:918`). Today there is exactly one: `AzOptionX11Visual_some`.

### Per-language impact of shared issues
- **TypeCategory::Vec name list:** no impact. Crystal never reads `TypeCategory::Vec`. `model.rs:504-538` detects Vec structurally (name ends in `Vec` and has 4 fields `ptr/len/cap/destructor`, `usize` len/cap, `_copyFromPtr`, copyable element), so every qualifying Vec becomes `Array(T)`/`Bytes`. `InstantPtr` and `StringMenuItem` are ordinary wrapper classes (`class Azul::InstantPtr < Azul::Storage(LibAzul::AzInstantPtr)`, and no `Conv.in_InstantPtr`/`in_StringMenuItem` exists).
- **Enum `Default`-variant constructor skip:** no impact. Union variants are built natively in Crystal (`wrappers.rs:818-853`): `Azul::AccessibilityAction::Default.new` (azul.cr ≈64268) and `Azul::ComponentFieldValueSource::Default.new` (≈95667).
- **The 15 kinds outside HOST_INVOKER_KINDS:** Crystal does not use the host invoker for closures. It uses C-ABI-direct trampolines, one per typedef, and carries closures through ctx or data RefAny. That gives 55 trampolines, including `TimerCallbackType`, `WriteBackCallbackType` and `DbMergeCallbackType`, e.g. `Azul::Timer.new(refany : T, callback : ::Proc(T, Azul::TimerCallbackInfo, Azul::TimerCallbackReturn), …)` (azul.cr ≈338997). The remaining 12 have no API taking them together with a user-data slot, so the raw fn pointer is the only possible shape:
  - Methods that take them: `IconProviderHandle.with_resolver(resolver : LibAzul::AzIconResolverCallbackType)` (≈162214), `RenderImageCallback.new(cb)`, `DatasetMergeCallback.from(cb)`, `AppConfig#add_component_library(…, register_fn)`.
  - Struct-field-only: `CaretTween`, `SelectionTween`, `CustomE2eOp`, `GetSystemTime`, `MarginBox`, `ComponentRender`/`ComponentCompile`, `MeasureDomFn`.
  - Kinds that are in HOST_INVOKER_KINDS but have no Crystal closure: `ThreadCallback` (raw), `ListViewOnLazyLoadScrollCallback` and the 8 `OnNode*Callback`s (field-only).
- **`RegisterComponentLibraryFnType` missed by `detect_callback_arg_info`:** irrelevant here. Crystal finds callbacks through the typedef map (`wrappers.rs:966-986`), so it gets a raw `LibAzul::AzRegisterComponentLibraryFnType` parameter.

### Derive coverage
(a) What is bound and how it is surfaced (all through the C functions unless noted):

| Derive | Class / union wrappers | Fieldless enums |
|---|---|---|
| Debug | `to_s(io)` + `inspect(io)` via `_toDbgString`, `wrappers.rs:639-651` | `inspect(io)` via `_toDbgString`, `:396-404`; `to_s` stays the Crystal variant name |
| PartialEq | `==` via `_partialEq`, `:664-670` | re-implemented, `:412-418` |
| PartialOrd / Ord | `include Comparable` + `<=>`: `_cmp`, else `_partialCmp` (255 → nil), `:671-690` | re-implemented, `:419-425` |
| Hash | `hash(hasher)` via `_hash`, `:691-697` | re-implemented, `:426-432` |
| Clone | `clone`/`dup` → `__copy_from` → `_clone` (non-Copy) or bitwise `ptr.value` (Copy), `:536-554,652-663` | `self`, `:405-411` |
| Default | `self.create_default` via `_createDefault`, plus a `self.new` alias when there is no zero-arg `create` and it is not a union, `:698-717` | `self.create_default`, `:389-395` |
| Drop | `finalize` → `_delete`, `:584-591` | n/a |

(b) Distinct symbols, comparing azul.h, the `lib LibAzul` fun declarations, and references from the `Azul::` layer:

| cap | azul.h | LibAzul | Azul:: layer |
|---|---|---|---|
| toDbgString | 2018 | 2018 | 2018 |
| partialEq | 1563 | 1563 | 1337 |
| partialCmp | 970 | 970 | 126 |
| cmp | 843 | 843 | 665 |
| hash | 871 | 871 | 681 |
| clone | 1115 | 1115 | 1113 |
| createDefault | 506 | 506 | 506 |

(c) The lib layer binds 100% of these symbols. What the idiomatic layer does not use:
- The fieldless-enum re-implementations: partialEq 226, cmp 178, partialCmp 179, hash 190, clone 2 (`BoxDecorationBreak`, `CompileTarget`).
- `_partialCmp` whenever `_cmp` also exists (665 types: struct 393, union 65, Option 158, Vec 48, String 1). This is a deliberate preference, not a gap.

All filters depend on type kind; none is keyed on a name.

(d) Re-implementations:
- Only the fieldless-enum traits above (CERTAIN, MINOR).
- Copy types are copied bitwise (`wrappers.rs:546-547`). That is correct: Copy types have no `_clone` symbol.
- Classes without PartialEq/Hash keep Crystal's Reference identity `==`/`hash`. That is the language default, not an emitter re-implementation.

---

## Swift

### CERTAIN
- **[SILENT SKIP]** `lang_swift/wrappers.rs:524-535` `self.skipped.push(format!("{} (native Swift type)", f.c_name));` plus `lang_swift/mod.rs:333` `.filter(|s| !s.ends_with("(native Swift type)") && …)`.
  - Every api.json method or constructor on the 444 natively mapped types (String, 316 `Option*`, 127 `*Vec`) is dropped from `import Azul`, and then filtered out of the header's diagnostic list.
  - That is 1347 functions, e.g. `String.from_utf8_lossy`, `from_utf16_le/be`, `from_c_str`, `to_c_str`, `copy_from_bytes`, and `*Vec.get/c_get/as_c_slice/as_c_slice_range/with_capacity/from_item`.
  - Most have Swift-native equivalents, but not all (UTF-16 LE/BE decoding, lossy UTF-8, C strings), and none is reported per function.
  - The drop depends on type kind, not name.
  - Fix: list them in the header, or emit them as `extension String` / `extension Array where Element == …` members.
- **[SILENT SKIP]** `lang_swift/wrappers.rs:1047` `let Some(ex) = exact(&t) else { continue };` and `:1068-1071` `Ty::Callback(_) | Ty::RawPtr(_) => None, … let Some(get) = get else { continue };` (non-owned fields map to `Ty::Unsupported` at `model.rs:706-711`).
  - Struct fields that are raw pointers (210 user-relevant ones, e.g. `MacOSHandle.ns_window`, `WindowsHandle.hwnd`, `XlibHandle.display`, `WaylandHandle.surface`, `HttpClient.ptr`) or callback typedefs (74 `cb`-style fields) get no Swift property.
  - Nothing reports this, and `AzulValue._address` is `internal` (`runtime.rs:102-108`), so users cannot reach the raw C value to use `import CAzul` either.
  - Consequence: `OnNodeAddedCallback` in Swift has only `description/debugDescription/copy/==/</hash/callable`, with no `cb` and no initializer. Field-registered callbacks (8 NodeGraph `OnNode*Callback`, `ListViewOnLazyLoadScrollCallback`) and native window handles are unusable from Swift. Crystal exposes these as raw `cb=` / pointer getters.
  - Fix: expose pointer fields as `UnsafeRawPointer?` properties and callback fields as `@convention(c)` properties (as Crystal does), and/or make `_address` public.
- **[SILENT SKIP]** `lang_swift/wrappers.rs:1366-1381`: `emit_methods` keeps only `Constructor | StaticMethod | Method | MethodMut`, so `EnumVariantConstructor`s are never planned or listed.
  - For the 4 unions that fall back to `final class`, no variant can be built or inspected from Swift: `BoxOrStaticImageRef`, `BoxOrStaticString`, `BoxOrStaticStyleBoxShadow`, `OptionX11Visual`. The header says "tagged union -> enum with payloads 575/579".
  - The C symbols `AzOptionX11Visual_some/none` exist. Crystal lists `AzOptionX11Visual_some` in its header; Swift says nothing.
  - Fix: plan variant constructors as static factories for `Kind::Class` unions, or push them to `skipped`.
- **[MINOR]** Literal name tests instead of `TypeCategory`: `lang_swift/model.rs:666` `if name == "String"`, `:670` `class.category == TypeCategory::RefAny || name == "RefAny"`, `:690`, `:699` `Some(Kind::Class) if name == "RefAny"`, and `wrappers.rs:374` `n.as_str() != "String"`. Same fix as Crystal.
- **[MINOR]** `lang_swift/wrappers.rs:1955-1956` `callback_ctx_field(&fac.callback_wrapper, m.ir).unwrap_or_else(|| "ctx".to_string())`, plus `:1959` `UnsafeMutablePointer<AzOptionRefAny>` and `:1963` `body.push("AzOptionRefAny_delete(__ctx)".to_string());`. Same guessed-field fallback and hard-coded symbol as Crystal.
- **[MINOR]** `lang_swift/wrappers.rs:1595-1597` `if layout_factory.is_some() && !cplans.first().is_some_and(|p| p.closure) { // The raw constructor is still bound, just without a closure. }` is dead code (an empty branch).

### SUSPECT
- **[HELLO-WORLD-ONLY?]** `lang_swift/wrappers.rs:1546-1556` and `:1937-1968`: the layout-callback factory. It emits `public convenience init<T: AnyObject>(_ layoutCallback: @escaping (T, LayoutCallbackInfo) -> Dom)` (azul.swift ≈179017), with the splice `_ptr.pointer(to: \AzWindowCreateOptions.window_state.layout_callback.ctx)`. This is what the hello-world's `WindowCreateOptions(layout)` uses. The code is structural, and the shared detector matches only `WindowCreateOptions` (same judgement call as Crystal).
- **[MINOR]** `lang_swift/model.rs:869-881`: `ctx_getter` is keyed on the method name `get_ctx` returning `OptionRefAny` (same as Crystal).
- **[MINOR]** Derive semantics of native containers (see Derive coverage (d)): `[T]`, `T?` and `String` use Swift's own `==`/`hash`/`description`. `[T]` and `Optional` are not `Comparable`, so the Rust Ord/PartialOrd on 127 Vec and 316 Option types cannot be reached. This is by design and depends on type kind; the maintainer should decide whether that is acceptable.

### Acceptable helpers (not hacks)
- `lang_swift/runtime.rs` (whole file):
  - `AzulObject`/`AzulValue<Raw>` ownership.
  - `_Handles`: RefAny handle boxes (`AzRefAny_newC`, `AzRefAny_isType`, `AzRefAny_getDataPtr`, `AzRefAny_clone`); RefAny upcast/downcast glue.
  - `_Native`: String conversion and `refany`.
  - The `_Conv`/`_Trampolines` namespaces.
- Trampolines (`wrappers.rs:2239-2342`) and erased closures (`:2185-2237`).
- `<c_name>Struct` selection (`wrappers.rs:2065-2071`).
- `SWIFT_STDLIB_TYPES` renaming (`mod.rs:376-439`), which is reserved-name escaping (`Duration`→`AzulDuration`), plus keyword escaping (`mod.rs:487-556`) and `reserved_member` (`wrappers.rs:79-96`).
- Module maps and `Package.swift` (`mod.rs:50-69,163-179`).
- Generic Option/Vec/Result, union-enum and plain-struct conversions (`model.rs:723-816`; `wrappers.rs:2348-2630,732-956`).
- Uniform naming: `create`→`init`, `create_x`→`static func x`, `get_x`/`set_x`→property, `is_/has_/can_`→Bool property (`wrappers.rs:1929-2034`).
- Header statistics and skip lists (`mod.rs:291-361`), with the caveat about the "(native Swift type)" filter above.

### Per-language impact of shared issues
- **TypeCategory::Vec name list:** no impact. Swift uses the structural `vec_shape` (`model.rs:748-787`), and the header reports "Vec -> [T] 127/127". `InstantPtr` and `StringMenuItem` are ordinary classes.
- **Enum `Default`-variant constructor skip:** no impact for the 575 enum-mapped unions. Cases are built natively (`emit_to_raw`, `wrappers.rs:871-912`): `AccessibilityAction` has `case default_` (azul.swift ≈73683). Separately, as listed above, Swift never surfaces any `EnumVariantConstructor` for the 4 class-mapped unions.
- **The 15 kinds outside HOST_INVOKER_KINDS:** same architecture and the same 55 trampolines as Crystal. Timer, WriteBack and DbMerge closures work, e.g. `public convenience init<T: AnyObject>(_ refany: T, callback: @escaping (T, TimerCallbackInfo) -> TimerCallbackReturn, getSystemTimeFn: GetSystemTimeCallback)` (azul.swift ≈134638).
  - Raw only: `withResolver(_ resolver: AzIconResolverCallbackType)` (≈175987), and `Thread.init(_ threadInitializeData: AnyObject, writebackData: AnyObject, callback: AzThreadCallbackType)` (≈134030).
  - Field-only kinds (NodeGraph `OnNode*`, `ListViewOnLazyLoadScroll`, `CaretTween`, `SelectionTween`, `CustomE2eOp`, `GetSystemTime`, `MarginBox`, `ComponentRender`/`ComponentCompile`) cannot be set at all from `import Azul` (CERTAIN #2).
- **`RegisterComponentLibraryFnType`:** raw `AzRegisterComponentLibraryFnType` parameter, detected through the typedef map.

### Derive coverage
(a) What is bound and how it is surfaced (all via C calls, for classes, plain structs, fieldless enums and unions alike; borrowing is done through `_address`, `_raw` or `_toRawBorrowed`, `wrappers.rs:1143-1167`):

| Derive | Swift surface | Emitter lines |
|---|---|---|
| Debug | `CustomStringConvertible.description` + `debugDescription` → `_toDbgString` | `wrappers.rs:1118-1121,1170-1188` |
| PartialEq | `Equatable ==` → `_partialEq` | `:1122-1125,1236-1257` |
| Hash | `Hashable` (requires eq) `hash(into:)` → `hasher.combine(_hash)` | `:1126-1128,1289-1303` |
| PartialOrd / Ord | `Comparable` (requires eq) `<` → `_cmp`, else `_partialCmp`; `__o == 0` means Less | `:1129-1131,1258-1288` |
| Clone | `copy()` → `_clone` (non-Copy class or union), bitwise pointee (Copy class), `self` (plain / enum) | `:1189-1235` |
| Default | `init()`, or `static func createDefault()` when `init()` is taken or a zero-arg `create` exists → `_createDefault` | `:1304-1359` |
| Drop | `deinit` → `_drop` override → `_delete` | `:979-987`; `runtime.rs:80-90` |

The `requires eq` gating costs nothing: azul.h has 0 types with hash or ordering but no `partialEq`.

(b) Distinct symbols referenced in azul.swift, against azul.h:

| cap | azul.h | azul.swift |
|---|---|---|
| toDbgString | 2018 | 1576 |
| partialEq | 1563 | 1186 |
| partialCmp | 970 | 73 |
| cmp | 843 | 637 |
| hash | 871 | 662 |
| clone | 1115 | 756 |
| createDefault | 506 | 504 |

(c) Every symbol is declared by the `CAzul` module (azul.h). What the Swift layer does not use:
- Natively mapped types:
  - `*Vec`: toDbgString 127, clone 127, partialEq 102, partialCmp 72, cmp 48, hash 48.
  - `Option*`: toDbgString 314, partialEq 274, clone 229, partialCmp 188, hash 160, cmp 157, createDefault 1.
  - `String`: all 7.
- `_partialCmp` where `_cmp` exists (636 types: enum 178, struct 393, union 65); a deliberate preference.
- 2 enum `_clone`s, because Swift enums copy by value.

All filters depend on type kind; none is keyed on a name.

(d) Re-implementations and bypasses:
- Native containers get Swift semantics instead of the C derives: array equality and hash come from the elements' C-backed `==`/`hash`, `description` is Swift's array/optional formatting rather than Rust `Debug`, and Vec/Option ordering is unavailable (SUSPECT, MINOR).
- Swift never declares a conformance on an imported C struct. `Az*` values are wrapped, and every conformance is implemented explicitly with a C call.
- The one fieldless enum without a PartialEq derive (`NodeGraphStyle`) still gets Swift's automatically synthesized `Equatable`/`Hashable` (language behaviour, harmless).
- Copy classes are copied bitwise (`:1194`). That is correct: Copy types have no `_clone`.


---

# Appendix: method and verification

**How the audit was split**
- The shared infrastructure, the module classifier (doc/src/autofix/module_map.rs) and the api.json data checks were done directly.
- The 18 emitters were split over 9 parallel read-only passes, each covering 2 languages. Each pass:
  1. grepped its emitters for quoted API names, string comparisons (`== "`, `starts_with`, `ends_with`, `strip_prefix`, `match` on literals) and `&[&str]` lists;
  2. read the entry points;
  3. traced hand-written-looking output blocks back to their emitter line;
  4. compared the hello-world's API use with what the generator special-cases;
  5. measured derive coverage against the azul.h symbol set.

**How the results were checked**
- Every `path:line` citation in the partial reports went through a script that looks for the quoted code within ±6 lines of the cited line.
  - Of about 150 citations, all non-prose mismatches were inspected by hand.
  - Each turned out to be real code quoted in condensed multi-line form, or an identifier built through `format!`.
  - No invented quotes were found.
- These findings were re-verified directly:
  - C# `func.c_name == "AzApp_run"` (lang_csharp/wrappers.rs:1522, while api.json also has `run_tray_only`);
  - Node `lib.Az{T}_deepCopy` (bound: `lib.AzDom_clone`, azul.js:49161; called: `lib.AzDom_deepCopy`, azul.js:91397);
  - OCaml `"with_child"`/`"with_css"`/`method_name == "dom"` (lang_ocaml/wrappers.rs:1173-1176, 842-855);
  - the S17 ctx drop (azul.h:54324, layout/src/callbacks.rs:1110-1113, core/src/host_invoker.rs:442-450);
  - the per-language S17 behaviour in Ruby, Node, Go, Fortran, Java, C#, Python, Crystal, Swift and D output;
  - the constants gap (S18);
  - the alias trait gap (S15);
  - the variant/trait name collisions (S14).

**api.json facts used**

| Fact | Value |
|---|---|
| Classes | 2,459 |
| Non-destructor callback typedefs | 77 |
| `*Vec` structs | 127 |
| Generic type aliases | 180 (58 with their own `derive`) |
| Constants | 1,436 |
| Classes with `vec_element_type` / `vec_ref_element_type` set | 0 |
| `with_*` methods taking `(RefAny, <host-invoker wrapper>)` | 58 (on 43 classes) |
| Functions taking a raw host-invoker typedef | 7 |

**Temporary files**
- The partial reports and scripts are in the scratchpad (`partials/`, `check_quotes.py`, `assemble_report.py`).
- `partials/derive_symbols_azul_h.txt` is the azul.h derive-symbol list. Note that it includes the variant constructor `AzBoxDecorationBreak_clone`.
