# CI-Only Language Bindings — FFI Capability Research

**Date:** 2026-07-06
**Companion to:** `scripts/LANGUAGE_EXPANSION_RESEARCH.md` (the broad survey +
wave plan). This report **narrows** to languages we intend to *implement blindly
and validate only in CI* (no easy local macOS/darwin toolchain), and answers the
five implementation-driving questions per language with a bias toward the one
detail that actually decides the archetype: **can the FFI pass/return a
`repr(C)` struct by value, and can it mint a C-callable function pointer whose
callback args are by-value aggregates?**

> Not duplicated here: the genericity thesis, the future-language checklist, the
> per-VM `ThreadCallback` lock table, and the full 40-language master table —
> those live in `LANGUAGE_EXPANSION_RESEARCH.md`. This file is the *FFI
> mechanics dossier* that drives the emitter work + CI wiring.

---

## 0. The archetype decision, restated in FFI terms

Azul exports each callback-taking API as a **triple** (see
`managed_host_invoker.rs::has_callback_wrapper_arg`):

- `Az<X>` — takes a **bare C fn-ptr** typedef (`Az<Kind>CallbackType`).
- `Az<X>WithCtx` — fn-ptr + destructured host-handle ctx.
- `Az<X>Struct` — the **whole wrapper struct by value** (the api.json shape;
  what managed bindings link, via `managed_c_symbol`).

So the archetype question decomposes into two independent FFI capabilities:

| Capability | Needed for | If absent |
|---|---|---|
| **C1. Pass/return small `repr(C)` structs by value** in *ordinary* calls | The whole non-callback API surface (configs, DOM nodes, `AzString`, `AzOption*`) | Language is **unbindable for the core API** — a true falsifier, not just a B. |
| **C2. Mint a C fn-ptr whose signature includes a by-value aggregate arg** (and, for a few kinds, a by-value aggregate *return*) | **Archetype A** (bind `Az<X>` directly) | Fall back to **Archetype B**: register one pointer-only invoker per kind (`AzApp_set<Kind>Invoker`); libazul's static thunk does the by-value plumbing. Weakest form: a single `AzApp_setGenericInvoker` closure of shape `void(uint64,const char*,const void* const*,size_t,void*)`. |

**C1 is the real falsifier test.** Almost every native-capable FFI has C1.
C2 is what libffi-class runtimes (LuaJIT, ruby-ffi, P/Invoke marshaller, koffi,
Dart, Guile) lack — hence archetype B exists. A language that has C1 but not C2
is a perfectly good binding; it just rides the host-invoker path.

---

## 1. Summary table

Legend — **Arch:** A = C-ABI-direct (binds `Az<X>` fn-ptr form), B =
host-invoker, B\* = host-invoker via a compiled C shim (NIF), A/B = A-capable but
B is the pragmatic choice. **SBV** = struct-by-value support (C1). **Verdict:**
✅ bindable / ⚠ bindable-with-caveat / ❌ falsifier.

| Language | Arch | FFI mechanism | SBV (C1) | Difficulty | Verdict |
|---|---|---|---|---|---|
| **Fortran** *(partial today)* | A-capable, emitted B | `bind(C)` interface + `iso_c_binding`; `c_funloc`/`c_funptr` for fn-ptrs | ✅ POD via `value`; ❌ **tagged unions emitted opaque** (bug) | Moderate (fix codegen) | ⚠ core works, `AzOption/Result`-by-value broken |
| **PowerShell** *(shipped)* | B / rides .NET | `Add-Type`+`[DllImport]` P/Invoke; delegates→fn-ptr | ✅ `[StructLayout(Sequential)]` by value | Trivial (done) | ✅ shipped (`lang_powershell`) |
| **D** | A | `extern(C)` decls + link/`pragma(lib)` | ✅ native C-layout structs | Trivial | ✅ |
| **Crystal** | A | `@[Link]` + `lib`/`fun`; `->` Proc → fn-ptr | ✅ `struct` in `lib` by value | Trivial | ✅ (retain boxed Procs vs GC) |
| **V (vlang)** | A | `fn C.name()` + `#flag -lazul`; transpiles to C | ✅ native (it *is* C) | Trivial | ✅ |
| **Julia** | A (or B) | `ccall((:sym,"libazul"),...)`; `@cfunction`→fn-ptr | ✅ isbits `struct` by value | Moderate | ✅ (thread-callback needs shim) |
| **Swift** | A | Clang module map **or** `dlopen`+`unsafeBitCast`; `@convention(c)` | ✅ imported C structs by value | Trivial–Moderate | ✅ (not yet emitted) |
| **Dart** | B (A-capable) | `dart:ffi` `DynamicLibrary.open`; `NativeCallable`/`Pointer.fromFunction` | ✅ `final class X extends Struct` by value | Moderate | ✅ |
| **Elixir** | B\* (C NIF shim) | `erl_nif.h` + `:erlang.load_nif` (or Port) | via shim only | Moderate | ⚠ needs compiled NIF; crash kills VM |
| **Erlang** | B\* (C NIF shim) | `ERL_NIF_INIT` + `erlang:load_nif/2` | via shim only | Moderate | ⚠ same |
| **Guile Scheme** | B | `(system foreign)`: `dynamic-link`+`pointer->procedure`; `procedure->pointer` | ⚠ by-value structs weak pre-3.0; use pointers | Moderate | ✅ |
| **Chez Scheme** | B (A-capable) | `load-shared-object`+`foreign-procedure`; `foreign-callable`+`lock-object` | ✅ `ftype` by value | Moderate | ✅ |
| **Racket** | B | `ffi/unsafe`: `ffi-lib`+`_fun`; `define-cstruct` | ✅ `_cstruct` by value | Moderate | ✅ (in progress emitter) |
| **Tcl** | B | `cffi` pkg (libffi/dyncall); `cffi::callback` | ✅ `Struct` by value | Moderate | ⚠ **callbacks call-scoped** — async fire is the risk |
| **Ada** *(shipped)* | A-capable, emitted B | `pragma Import(C)` / `Interfaces.C`; `access ... Convention=>C` | ✅ record `Convention=>C_Pass_By_Copy` | Trivial (done) | ✅ shipped (`lang_ada`) |
| **Pony** *(new)* | A | `use "lib:azul"` + `@sym[T](...)`; `@{(...)}` bare lambda → fn-ptr | ✅ `struct` C-ABI by value | Moderate | ✅ interesting strengthener |
| **Janet** *(new)* | A/B | `ffi/native`+`ffi/lookup`+`ffi/call`; `ffi/trampoline` | ✅ `ffi/struct` by value | Moderate | ✅ strengthener |
| **Wren** *(new)* | ❌ / host-shim | foreign methods bind to **host C only**, no `dlopen` | n/a | — | ❌ needs a C host that pre-wires libazul |
| **Grain / AssemblyScript** | ❌ (WASM) | WASM imports only; numbers + linear-mem offsets | n/a | — | ❌ WASM-sandboxed (host-bridge only) |
| **Elm / Starlark** | ❌ (no FFI) | none by design | n/a | — | ❌ hermetic DSL |

---

## 2. Per-language specifics

### Fortran — *partial binding exists; note the gaps*

1. **Archetype.** Modern Fortran (F2003+) is actually **archetype-A capable**:
   `iso_c_binding` provides `c_funloc()` to take the address of a
   `bind(C)`-convention procedure, yielding a genuine `c_funptr` (a real C
   function pointer). Our current `lang_fortran/managed.rs` nonetheless emits the
   **host-invoker** path — and does so **incompletely**.
2. **FFI mechanism.** One `azul.f90` module: types as `type, bind(C)` derived
   types, functions inside an `interface … end interface` block with
   `bind(C, name="AzFoo_create")` carrying the verbatim C symbol (Fortran is
   case-insensitive for its own names but `name=` is case-sensitive). No runtime
   `dlopen`; links `-lazul` at build time.
3. **Struct-by-value (C1).** POD works: `type(AzAppConfig), value :: config`.
   **Showstopper gap:** tagged unions (`AzOption*`, `AzResult*`, `AzError`) are
   emitted as **ABI-opaque blobs** (tag + `c_ptr` payload, ~12 bytes) instead of
   the Rust `#[repr(C, u8)]` **inline** variable-size union. Any C-ABI function
   that takes an `AzOption/AzResult` **by value** therefore receives a
   wrong-layout struct → silent corruption. This is *the* reason Fortran E2E is
   blocked (see `memory/fortran_codegen_2026_05_13.md`).
4. **Other current gaps.** (a) The invoker stub signature is wrong — it takes
   only `(id)` where libazul calls `(id, arg0…, out_ptr)`; extra args land in
   registers and are silently discarded (no dispatch). (b) No dispatch body. Both
   are the same fixes Pascal already received.
5. **Difficulty / hello-world.** Moderate — but the fix is a **codegen rewrite**
   of `layout.rs` (inline-union blob sizing per variant) + `managed.rs`
   (Pascal-style full-arg invoker stubs). Once fixed, a counter is: `App_create`
   → `WindowCreateOptions` with a `LayoutCallback` (registered via `c_funloc` of
   a `bind(C)` function, so Fortran could even go pure archetype-A here) →
   `App_run`. **Recommendation:** flip Fortran to archetype A for the callback
   (it can make the fn-ptr) and only keep host-invoker for the by-value-return
   kinds; fix the inline-union emission first regardless.

### PowerShell — *already shipped (`lang_powershell`)*

1. **Archetype B**, riding .NET P/Invoke exactly like C#. PowerShell cannot emit
   a raw C fn-ptr from script directly, but `Add-Type` compiles inline C# that
   uses `Marshal.GetFunctionPointerForDelegate`; the aggregate-by-value marshaller
   limitation is why it uses the invoker path, same as C#.
2. **FFI.** `Add-Type -TypeDefinition '<C#>'` with `[DllImport("libazul")]` and
   `[UnmanagedFunctionPointer(CallingConvention.Cdecl)]` delegates.
3. **SBV.** `[StructLayout(LayoutKind.Sequential)]` structs pass by value.
4. **No showstopper** — but the **CFRunLoop/pwsh REPL conflict on macOS** means
   Windows is the supported target (see `memory/powershell_macos_eventloop.md`).
5. **Status: done.** Listed for completeness — no new work.

### D

1. **Archetype A.** `extern(C)` function *addresses* are genuine C function
   pointers (D and C share the SysV/AAPCS64 ABI). Non-capturing `extern(C)`
   functions serve as callbacks; per-instance state rides `RefAny data`.
2. **FFI.** `extern(C) AzApp AzApp_create(AzRefAny, AzAppConfig);` declarations in
   a module; link with `pragma(lib, "azul")` or `-L-lazul`. No runtime loader
   needed.
3. **SBV.** D `struct`s are C-layout by default and pass/return by value across
   `extern(C)` — direct, no attributes.
4. **Showstopper:** none. Caveat: D is GC'd — a callback (or any `RefAny`-boxed
   closure) held by libazul must be kept as a **GC root** (`GC.addRoot`) or the
   collector may free it under a live native reference.
5. **Difficulty trivial.** Copy `lang_zig`. Counter = declare externs → `App`,
   `Button.setOnClick(&myExternCFn, state)` → `App.run`. CI:
   `dlang-community/setup-dlang` (dmd/ldc).

### Crystal

1. **Archetype A.** Crystal's `->`(Proc) types lower to C function pointers; a
   non-capturing proc passed to `fun`-typed args is a real fn-ptr. Closures that
   capture are boxed — but Azul doesn't need capture (ctx via `RefAny`).
2. **FFI.** `@[Link("azul")]` on a `lib LibAzul` block containing `fun` decls and
   `struct`/`union` type definitions. Compile-time link.
3. **SBV.** `struct` inside a `lib` maps to a C struct and passes by value.
4. **Showstopper:** none. Caveat: retain any boxed `Proc`/data you hand to
   libazul against Crystal's GC (keep a reference in a container that outlives the
   widget).
5. **Difficulty trivial.** CI: `crystal-lang/install-crystal`.

### V (vlang)

1. **Archetype A** — the strongest case: V **transpiles to C**, so its `fn`
   pointers *are* C function pointers and its structs *are* C structs.
2. **FFI.** `fn C.AzApp_create(...) AzApp` declarations + `#flag -lazul`
   (and `#include` if using the header). Structs declared with `C.` prefix or
   `@[typedef]`.
3. **SBV.** Native — no marshalling layer exists to get in the way.
4. **Showstopper:** none. Watch V's still-moving `@[heap]`/autofree semantics for
   long-lived callback data.
5. **Difficulty trivial.** CI: `vlang/setup-v`.

### Julia

1. **Archetype A (or B).** `@cfunction(f, Ret, (Args…))` produces a **real C
   function pointer** from an *arbitrary* Julia function, including by-value
   struct args — so Julia can bind `Az<X>` directly. The one hazard:
   `@cfunction` closures invoked on a **foreign thread** (Julia's runtime isn't
   reentrant from arbitrary OS threads) — so `ThreadCallback` must be routed
   through a tiny C shim that `jl_call`s on a Julia-owned task, or use the
   writeback pattern. Main-thread callbacks are fine directly.
2. **FFI.** `ccall((:AzApp_create, "libazul"), AzApp, (AzRefAny, AzAppConfig),
   data, cfg)`. No explicit `dlopen` needed (`ccall` resolves the library name),
   though `Libdl.dlopen` is available.
3. **SBV.** Any `isbits` `struct` (immutable, no pointers-to-GC) passes and
   returns by value through `ccall`/`@cfunction` — matches Azul's `repr(C)` PODs.
4. **Showstopper:** none for the core; the thread-callback reentrancy caveat is
   the only real footnote.
5. **Difficulty moderate** (mostly the `isbits` struct mirroring + the thread
   shim). CI: `julia-actions/setup-julia`. Audience: scientific/plotting.

### Swift — *not yet emitted; high audience value*

1. **Archetype A.** `@convention(c)` closures compile to C function pointers.
   They **cannot capture** — which is exactly Azul's model (ctx via `RefAny`), so
   this is a non-issue.
2. **FFI (two options).** (a) A Clang **module map** (`module.modulemap`
   pointing at `azul.h`) so `import CAzul` exposes the whole C API typed; or
   (b) `dlopen`/`dlsym` + `unsafeBitCast` to a `@convention(c)` function type for
   a header-free build. Option (a) is the idiomatic, less error-prone path.
3. **SBV.** C structs imported through the module map pass/return by value
   natively; Swift bridges them to imported struct types.
4. **Showstopper:** none. On the macOS CI runner the Swift + Xcode clang
   toolchain is preinstalled, making this one of the cheapest new bindings to
   validate.
5. **Difficulty trivial–moderate** (write a correct module map + a thin idiomatic
   wrapper struct with `deinit` calling `_delete`). Template `lang_go`/`lang_zig`.
   CI: `swift-actions/setup-swift` (Linux) / preinstalled (macOS).

### Dart

1. **Archetype B (A-capable).** `dart:ffi` can make native callbacks two ways:
   `Pointer.fromFunction` (static/top-level only, restricted return types) and
   `NativeCallable.isolateLocal` / `.listener` (closures, incl. cross-thread via
   `.listener`). Modern `dart:ffi` *does* support compound (struct) args in
   `NativeCallable`, so archetype A is technically reachable — but the
   host-invoker path (template `lang_node`) is the pragmatic, uniform choice and
   sidesteps the return-type restrictions of `fromFunction`.
2. **FFI.** `DynamicLibrary.open("libazul.dylib")` then `.lookupFunction<
   NativeSig, DartSig>("AzApp_create")`.
3. **SBV.** `final class AzAppConfig extends Struct { … }` with `@Uint32()` etc.
   field annotations; passes and returns by value.
4. **Showstopper:** none. Caveat: keep `NativeCallable` objects alive (close them
   explicitly) — GC'ing one while libazul holds it dangles the trampoline.
5. **Difficulty moderate.** CI: `dart-lang/setup-dart`. Audience: Flutter
   refugees wanting native desktop without the engine.

### Elixir / Erlang — *host-invoker via a compiled C NIF shim (B\*)*

1. **Archetype B\*.** The BEAM has **no in-language FFI** to arbitrary
   `extern "C"` symbols. Native code enters only as a **NIF** (a `.so` including
   `erl_nif.h`, loaded by `:erlang.load_nif`/`erlang:load_nif/2`) or a Port
   (separate OS process). So neither language can bind libazul *directly* — it
   needs a **thin compiled C shim** that `#include`s `erl_nif.h`, converts BEAM
   terms ↔ C args, and forwards to `libazul`. `rustler` (Rust) or `zigler` (Zig)
   can generate that shim.
2. **FFI mechanism.** `ERL_NIF_INIT(...)` + a `dirty` scheduler for the blocking
   `AzApp_run` event loop; callbacks come *back* into BEAM via
   `enif_send`/`enif_make_*` to a registered process (the id→callable table
   becomes a pid registry). Effectively the generic-invoker pattern, but the
   "invoker" is a NIF-side `enif_send`.
3. **SBV.** Only inside the C shim (real C) — never at the Erlang/Elixir source
   level.
4. **Showstopper (soft):** a NIF that crashes or blocks a scheduler takes down /
   stalls the VM; `AzApp_run` **must** be a dirty NIF or run on its own OS thread
   with the window on the main thread. Not a falsifier — bindable — but not
   pure-FFI: it ships a compiled artifact per platform.
5. **Difficulty moderate** (the shim is real work; the Elixir/Erlang surface is
   thin). CI: `erlef/setup-beam`. Gleam reaches this by `@external` to the Erlang
   NIF module (one host-language hop).

### Guile Scheme

1. **Archetype B.** `procedure->pointer` wraps a Scheme procedure as a libffi
   closure → a callable C fn-ptr, but with the same aggregate-by-value limits as
   every libffi host, so it uses the invoker path.
2. **FFI.** `(use-modules (system foreign))`; `(dynamic-link "libazul")` +
   `(dynamic-func "AzApp_create" lib)` + `pointer->procedure` to type it.
3. **SBV.** ⚠ Guile's FFI describes types as lists of primitives; **by-value
   struct passing is weak/limited** in older Guile and best done by pointer.
   Azul's managed path already passes everything by pointer, so this aligns —
   but any *direct* by-value POD arg needs a pointer round-trip.
4. **Showstopper:** none. `to_kebab_case` helper already exists in
   `managed_host_invoker.rs` for Scheme naming.
5. **Difficulty moderate.** CI: `apt install guile-3.0`. Template `lang_lisp`.

> **Which Schemes have C FFI?** Guile (`system foreign`), Chez
> (`foreign-procedure`/`foreign-callable` — the strongest, real by-value via
> `ftype` and pinnable callbacks via `lock-object`), Racket (`ffi/unsafe`,
> richest ergonomics — separate emitter in progress), Chicken
> (`foreign-lambda`/`define-external`), Gambit, Bigloo. Guile is the GNU
> extension-language target; Chez is the "purist + stable" target (Racket-CS runs
> on it).

### Tcl — *callback lifetime is the real risk*

1. **Archetype B.** The `cffi` package (libffi or dyncall backend) does calls
   and `cffi::callback` makes C-callable closures.
2. **FFI.** `package require cffi`; `cffi::Wrapper create azul libazul[info
   sharedlibextension]`; `azul function AzApp_create {…} …`.
3. **SBV.** `cffi::Struct create` defines layouts; by-value pass/return is
   supported.
4. **Showstopper (⚠ soft falsifier for callbacks).** `cffi::callback` trampolines
   are documented as **call-scoped** — valid only while a Tcl-initiated C call is
   on the stack. Azul's callbacks fire **asynchronously from the native event
   loop** (`AzApp_run` never returns to Tcl until quit), so a naive stored
   callback is unsafe/stale. Mitigation: use the **generic-invoker** with a
   long-lived `cffi::callback` created once and kept for the process lifetime, or
   a Tcl event-loop integration that re-enters. This must be **validated first**
   — Tcl could end up bindable-for-non-callback-API only.
5. **Difficulty moderate–hard.** CI: `apt install tcl tcllib` (+ `cffi`).

### Ada — *already shipped (`lang_ada`)*

1. **Archetype A-capable, emitted B today.** GNAT can make a C-callable pointer
   via `access procedure` with `Convention => C`, so Ada could bind `Az<X>`
   directly; the current emitter uses the host-invoker path.
2. **FFI.** `pragma Import (C, App_Create, "AzApp_create");` +
   `Interfaces.C`/`Interfaces.C.Strings`. Link `-lazul`.
3. **SBV.** `record` with `pragma Convention (C, T)` (or `C_Pass_By_Copy`) passes
   by value.
4. **Showstopper:** none. Aerospace/defense/high-integrity audience.
5. **Status: done.** Listed because it was in the candidate set; no new work.

### Pony — *new, strong strengthener*

1. **Archetype A.** Pony's FFI is first-class: `@azul_app_create[App](data, cfg)`
   calls a C symbol; a **bare lambda** `@{(data: RefAny, info: CallbackInfo):
   U32 => …}` compiles to a C function pointer with C convention — no capture,
   which fits Azul's `RefAny`-ctx model exactly.
2. **FFI.** `use "lib:azul"` (link directive) + `use @azul_app_create[App](...)`
   declarations; symbols resolved at link time.
3. **SBV.** Pony `struct` types are C-ABI compatible and pass by value across FFI.
4. **Showstopper:** none, but Pony's actor/capabilities model means the GUI event
   loop should own an actor; callback data must respect reference capabilities
   (`iso`/`val`). Interesting because it proves the thesis extends to a
   capabilities-typed actor language with zero special-casing.
5. **Difficulty moderate.** CI: `ponylang/setup-ponyc-action`. Template
   `lang_zig`.

### Janet — *new, strengthener*

1. **Archetype A/B.** Janet has a real runtime FFI on x86-64/aarch64:
   `ffi/native` to load the lib, `ffi/lookup` + `ffi/signature` + `ffi/call`, and
   `ffi/trampoline` to expose a Janet function as a C callback. So it can go
   archetype A; falling back to B (generic invoker) is also clean.
2. **FFI.** `(def lib (ffi/native "libazul.dylib"))`;
   `(ffi/call (ffi/lookup lib "AzApp_create") sig data cfg)`.
3. **SBV.** `ffi/struct` builds a struct type descriptor; by-value pass/return is
   supported by the `ffi/call` machinery.
4. **Showstopper:** none, though Janet's FFI is platform-gated (needs the
   assembly trampolines for the target arch). Embeddable-Lisp audience.
5. **Difficulty moderate.** CI: build janet from source or `apt`. Template
   `lang_lua`/`lang_zig`.

---

## 3. Falsifiers — languages that genuinely cannot bind (and exactly why)

A falsifier fails **C1** (can't call/pass a `repr(C)` struct across a native
boundary) — usually because it removed native linking on purpose. None of these
is a systems/application language; each is a config, web, or embedded-scripting
DSL, so all fall outside the thesis's domain.

| Language | Why it fails | Reachable at all? |
|---|---|---|
| **Elm** | Purely functional web frontend; FFI (`Native`) was **removed on purpose** for reproducibility. No `dlopen`, no C. | No. (Ports talk to JS only.) |
| **Starlark** | Bazel/Buck config DSL; deliberately hermetic, no I/O, no native linking. | No. |
| **AssemblyScript** | Compiles to **WebAssembly**, not native code. Can only call *imported host functions*; the boundary carries numbers + linear-memory offsets, never a `.so` symbol or a by-value C struct. | Indirectly — a WASM host (Wasmtime/JS) bridges to libazul, or target Azul's own wasm build. Binding lives in the host glue, not the language. |
| **Grain** | Same as AssemblyScript — WASM-first, no native FFI to `dlopen` a shared lib. | Indirectly, via a WASM host bridge. |
| **Wren** | Embeddable scripting language: "foreign" methods bind to functions the **embedding C host explicitly registers** — there is no `dlopen`/`dlsym` from Wren source. Wren cannot reach libazul on its own. | Yes, but only if a C host program pre-wires libazul as Wren foreign methods — i.e. the binding is a C shim, like BEAM. Not pure-FFI. |
| **Gleam** *(as a direct target)* | `@external` targets **Erlang or JavaScript only** — no direct C. | Yes, one hop: via an Erlang NIF (B\*) or a Node/JS binding. |
| **Erlang / Elixir** *(as pure-FFI)* | The BEAM has no source-level FFI to arbitrary `extern "C"`. | **Yes, via a compiled C NIF shim** (classified B\* above, not a hard falsifier). Listed here only to mark that they are *not* pure-FFI bindings. |

**Conclusion.** Every falsifier that is a true "no" is a language that removed
native FFI **by design** (Elm, Starlark) or targets a **sandbox VM** (WASM:
AssemblyScript/Grain; BEAM without a NIF; Wren without a C host). The moment a
language can `dlopen`/link a `.so` and pass a `repr(C)` struct by value, it lands
in archetype A or B — no exceptions were found in the candidate set. The thesis
holds; the boundary is precisely "has C1 or not."

---

## 4. CI wiring notes (implement-blindly checklist)

For each new binding, CI validation needs: (a) a toolchain-install step, (b) the
prebuilt `libazul` on the runner's link/`dlopen` path, (c) an example counter that
builds + runs a headless smoke (create app → register a callback via the chosen
archetype → assert the callback fires with the right `RefAny` state → quit).

| Language | GitHub Action | Notes |
|---|---|---|
| D | `dlang-community/setup-dlang` | dmd or ldc |
| Crystal | `crystal-lang/install-crystal` | |
| V | `vlang/setup-v` | |
| Julia | `julia-actions/setup-julia` | |
| Swift | `swift-actions/setup-swift` / preinstalled on macOS | module map path |
| Dart | `dart-lang/setup-dart` | |
| Elixir/Erlang/Gleam | `erlef/setup-beam` | + NIF compile step (cc/rustler/zigler) |
| Guile | `apt install guile-3.0` | |
| Chez | `apt install chezscheme` | |
| Tcl | `apt install tcl tcllib` + `cffi` | validate callback lifetime FIRST |
| Pony | `ponylang/setup-ponyc-action` | |
| Janet | build from source / `apt` | arch-gated FFI |
| Fortran | `fortran-lang/setup-fortran` (gfortran) | **fix inline-union codegen before E2E** |

---

*Report generated 2026-07-06. Grounded in: `doc/src/codegen/v2/lang_odin/` and
`lang_nim/` (archetype A), `managed_host_invoker.rs` +
`lang_fortran/{mod,layout,managed}.rs` (archetype B + the Fortran gaps),
`memory/fortran_codegen_2026_05_13.md`, `memory/powershell_macos_eventloop.md`.
Companion: `scripts/LANGUAGE_EXPANSION_RESEARCH.md`.*
