# Azul Language-Binding Expansion — Research Report & Implementation Roadmap

**Date:** 2026-07-06
**Thesis under test:** *Azul's C-ABI + host-invoker codegen can bind to ANY
language — including languages that do not exist yet.*
**Scope:** survey every notable language NOT yet bound, classify it against
the two existing binding archetypes, name the closest template emitter,
estimate effort, and produce a prioritized "implement blindly" wave plan.

> This is research + roadmap only. No emitters, `api.json`, or workflows are
> touched here. Odin is being implemented concretely by a separate agent; this
> report references its approach but does not duplicate it.

---

## 0. What already exists (the two archetypes)

Azul ships a single prebuilt `libazul` C-ABI shared library (`Az*`/`az_*`
`extern "C"` functions + `repr(C)` structs, header at
`target/codegen/azul.h`). `azul-doc` (`doc/src/codegen/v2/lang_*`) generates
bindings for **27 languages** today: ada, algol68, c, cpp, csharp, fortran,
freebasic, go, haskell, java, kotlin, lisp, lua, node, ocaml, pascal, perl,
php, powershell, python, ruby, rust, scala (rides java), smalltalk, vb6, zig.

Every binding is one of two archetypes:

### Archetype A — C-ABI-DIRECT

The language produces **real C function pointers** whose bodies honor the
platform calling convention, *including passing aggregate structs by value*.
The generated callback is handed straight to `Az<Widget>_setOnClick(self,
data, cb)` — `cb` is a bare fn-ptr, and the user's per-instance context
arrives through the `data: RefAny` argument, not a closure capture. No handle
table, no per-kind invoker slots.

Current archetype-A bindings: **zig, go, c, cpp** (and Odin, in progress).
Template emitters: `doc/src/codegen/v2/lang_zig/` (single-file, cleanest),
`lang_go/` (sub-package layout), `lang_c.rs` (raw header only).

### Archetype B — HOST-INVOKER

Managed/interpreted runtimes (Lua, Ruby, Perl, PHP, OCaml, Node, C#, Java,
Kotlin, Python, Lisp, Pascal, Fortran) cannot generate C-ABI trampolines for
callback typedefs that take **aggregate args by value** — that is a
libffi / LuaJIT-FFI / ruby-ffi / P/Invoke limitation, *independent* of whether
the language can make a simple `void(*)(void*)` pointer at all. The host-invoker
pattern (`core/src/host_invoker.rs`, codegen in
`doc/src/codegen/v2/managed_host_invoker.rs`) solves this:

1. libazul compiles **one static thunk per callback kind** (via
   `impl_managed_callback!`). The framework calls the thunk with by-value args
   on a native frame — exactly as it already expects.
2. The thunk reads the host-language `u64` handle out of the callback's
   `ctx: OptionRefAny` (built by `Az<Kind>_createFromHostHandle(id)` /
   `AzRefAny_newHostHandle(id)`), then dispatches across the C ABI to a
   per-kind invoker the host registered once at load time
   (`AzApp_set<Kind>Invoker(...)`), passing every arg **by pointer** and the
   return value through an **out-pointer** (LuaJIT can't return aggregates
   > 8 bytes from a callback, so this is uniform).
3. A single shared releaser (`AzApp_setHostHandleReleaser`) fires when a
   host-handle `RefAny`'s refcount hits zero, so the host drops its
   `id → callable` table entry — mirroring Python's `Py<PyAny>` lifetime story
   without libazul linking any host runtime.
4. A `AzApp_setGenericInvoker` fallback lets a host wire **one** dispatch site
   for every kind (and cover user-defined custom kinds) — this is the escape
   hatch for the most FFI-restricted hosts.

The host side is thus reduced to: (a) an id→callable table, (b) one libffi
closure per callback kind cast to the pointer-arg `Az<Kind>Invoker` signature,
(c) one releaser. All emitted by `managed_host_invoker.rs::emit_cdef_block`
for C-syntax FFI parsers, or translated by hand for declarative FFIs
(ruby-ffi `attach_function`, P/Invoke `[DllImport]`, JNA interfaces).

Template emitters: `lang_lua/`, `lang_ruby/`, `lang_python.rs`,
`lang_java/`, `lang_csharp/`, `lang_lisp/`.

---

## 1. Genericity thesis

### The ONE requirement

> A language can host an Azul binding **iff** it can (1) load a native shared
> library and (2) call a C-ABI function through it — passing integers,
> pointers, and (for archetype A) `repr(C)` structs, and reading a return value.

That is the *entire* contract. Everything else Azul needs is layered on top of
that single capability:

- **Struct types?** Every FFI that can call C can describe a `repr(C)` layout
  (by header import, `define-cstruct`, `[StructLayout]`, `ftype`, or hand
  count). Azul structs are plain `repr(C)`.
- **Callbacks?** Two tiers, and Azul supports both:
  - If the language can mint a C-callable function pointer (even a
    non-capturing / top-level-only one), it uses **archetype A** — the
    per-instance state rides in the `RefAny data` arg, so "no capture" is not a
    limitation.
  - If the language **cannot** mint a fn-pointer for aggregate-by-value
    callbacks (every managed runtime), it uses **archetype B** — it registers
    *one* pointer-arg invoker per kind and routes through the id table. The
    hard part (aggregate-by-value on a native frame) is done by libazul's static
    thunk, on the native side, where it is trivial.
- **Memory / GC?** The RefAny refcount + host-handle releaser gives every
  language a deterministic "drop my table entry now" signal, so a GC'd host and
  a manually-managed host share one lifetime story.
- **Threads?** The only callback that fires off the main thread is
  `ThreadCallback`; the per-VM lock-acquire table (§5) covers it, and
  single-threaded hosts use the writeback-only pattern.

Because requirement (1)+(2) is the *floor* of what "systems interoperability"
means, essentially every general-purpose language clears it. The design is
universal precisely because it asks for nothing above the C ABI — the lowest
common denominator every language already targets to talk to the OS.

### Why even fn-pointer-less languages are covered

The subtle point: archetype B does **not** require the host to produce a C
function pointer *at all* in the hard case. `AzApp_setGenericInvoker` takes a
single `extern "C" fn(handle, kind, args[], n, ret)` — the host only needs to
supply **one** libffi closure with a fixed, pointer-only signature (no
aggregate by value, no per-kind variance). Any FFI that can make even a single
callback of the shape `void(uint64, const char*, const void* const*, size_t,
void*)` can drive *every* Azul callback kind. That is the weakest possible ask.

### The falsification test — languages that genuinely CANNOT bind

Three classes fail, and each failure is instructive:

| Class | Example(s) | Why it fails | Is Azul reachable at all? |
|---|---|---|---|
| **Sandboxed-by-design** (no FFI, ever) | **Starlark** (Bazel/Buck config), **Elm** | Deliberately hermetic — no native linking, no `dlopen`, FFI intentionally absent for reproducibility/safety. | No. Not a general-purpose UI language; out of scope by construction. |
| **WASM-sandboxed** | **AssemblyScript**, pure-WASI targets | Compiles to WebAssembly, not native code; can only call *imported host functions*, never `dlopen` a `.so`. Boundary passes only numbers + linear-memory offsets. | **Indirectly** — run the AS module inside a WASM host (Wasmtime/JS) that bridges to `libazul`, OR target Azul's own wasm build (the "web-lift" story). The binding work lives in the host/glue layer, not in the language. |
| **BEAM (no in-language FFI)** | **Erlang, Elixir, Gleam** | The VM only loads native code as a **NIF** (compiled C via `erl_nif.h`) or a Port. There is no way to call an arbitrary `extern "C"` symbol from Erlang/Elixir/Gleam source. | **Yes, via a thin C NIF shim** — a hand-written (or `rustler`/`zigler`-generated) `.so` that includes `erl_nif.h`, forwards to `libazul`, and is loaded with `load_nif`. Bindable, but *not pure-FFI*: it needs a compiled shim, and a NIF crash takes down the VM. |

**Conclusion of the falsification test:** No general-purpose *native-capable*
language was found that cannot bind. The only hard "no" is a language that has
**removed FFI on purpose** (Starlark, Elm) — and those are config/frontend DSLs,
not systems or application languages, so they fall outside the thesis's domain.
Everything else lands in archetype A, archetype B, or "B + a compiled C shim"
(BEAM). The thesis holds.

---

## 2. Master table — every surveyed language

Legend — **Arch:** A = C-ABI-direct, B = host-invoker, B* = host-invoker via a
compiled C shim (NIF), "rides X" = reuses an already-bound host runtime's
binding wholesale. **Effort:** T = trivial, M = moderate, H = hard, — = blocked.

| Language | FFI mechanism (cited) | Arch | Closest template | Effort | Audience / rationale |
|---|---|---|---|---|---|
| **Odin** | `foreign import` + `foreign` blocks, default `"c"` conv; `proc "c"` for callbacks [odin] | A | `lang_zig` | T | Systems/gamedev; being done concretely by the Odin agent. |
| **Nim** | `{.importc, dynlib.}` pragmas (runtime `dlopen`); `{.cdecl.}` procs = C fn-ptr [nim1][nim2] | A | `lang_zig` | T | Python-ergonomics-with-C-speed crowd; gamedev, tooling. |
| **D** | `extern(C)` decls + `pragma(lib)`; `extern(C)` fn address = C fn-ptr [d] | A | `lang_zig` | T | C++ refugees, systems; keep GC roots alive for held callbacks. |
| **Crystal** | `@[Link]` + `lib`/`fun`; `->`(Proc) callback types, box closure into `void*` [cr1][cr2] | A | `lang_zig`/`lang_go` | T | Ruby-syntax-native-speed fans; retain boxed Procs vs GC. |
| **V (vlang)** | `fn C.name(...)` + `#flag -lazul`; V fn-ptr == C fn-ptr [v] | A | `lang_zig` | T | Simplicity/Go-like systems crowd; transpiles to C, C-native. |
| **Zig** *(done — ref)* | `@cImport`/`extern fn`/`export fn callconv(.C)` [zig] | A | `lang_zig` (self) | — | Already shipped; the reference archetype-A template. |
| **Objective-C** | Strict C superset — call `extern "C"` directly; `dlopen`/`NSBundle` for dynamic [objc] | A | `lang_cpp` | T | Apple/AppKit devs wanting a cross-platform Rust core. |
| **Swift** | Clang module import or `dlopen` + `unsafeBitCast` to `@convention(c)` [swift1][swift2] | A | `lang_go`/`lang_zig` | T | Huge Apple audience; `@convention(c)` no-capture OK (ctx via RefAny). |
| **Vala** | Compiles to C; `.vapi` with `[CCode(...)]`; `delegate` types = C fn-ptr [vala1][vala2] | A | `lang_c.rs` + vapi | M | GNOME/GTK devs; must author a correct `.vapi` by hand. |
| **Hare** | `@symbol(...)` fn decls + `types::c`, `-l` link; fn-ptr ABI-compatible [hare] | A | `lang_zig` | M | Suckless/minimalist systems; early, Linux/BSD-only, no header importer. |
| **Nelua** | `<cimport>`/`<cinclude>` annotations; first-class C `function(...)` ptr types [nelua] | A | `lang_zig` | M | Lua-lovers wanting AOT/no-GC; niche, hand-written decls. |
| **Mojo** | `sys.ffi.DLHandle`+`get_function[T]`/`external_call`; `abi("C")` fn effect for callbacks [mojo1][mojo2] | A | `lang_zig`/`lang_python` | M–H | AI/ML + Python-superset crowd; volatile API, no header import. |
| **Chapel** | `extern proc`/`extern record`, `require "h","-lazul"`; `c_fn_ptr`+`c_ptrTo` [chapel] | A | `lang_zig` | M | **HPC / supercomputing** — a native GUI on cluster-side tools. |
| **Julia** | `ccall((:sym,"libazul"),Ret,(Args...),...)`; `@cfunction` = C fn-ptr from any fn [jl1][jl2] | A (or B) | `lang_python`/`lang_lua` | M | **Scientific computing** — plotting/dashboards without Python glue. |
| **Dart** | `dart:ffi` `DynamicLibrary.open`; `NativeCallable`(closures/cross-thread) / `Pointer.fromFunction`(static) [dart1][dart2][dart3] | B | `lang_node` | M | **Flutter refugees** wanting native desktop without Skia/engine bulk. |
| **R** | base `dyn.load`+`.Call` (needs C wrapper); `rdyncall::new.callback` for fn-ptr [r1][r2] | B | `lang_lua` | H | **Statisticians / data science**; base R needs `.Call` shims or rdyncall. |
| **F#** | .NET P/Invoke — `[<DllImport>]`/`extern`; `Marshal.GetFunctionPointerForDelegate` [fs1][fs2] | B / **rides C#** | `lang_csharp` | T | .NET functional crowd; **reuses the C# binding assembly wholesale**. |
| **VB.NET** | .NET P/Invoke — `Declare`/`<DllImport>`; same delegate→fn-ptr as F# [vb1][vb2] | B / **rides C#** | `lang_csharp` | T | Enterprise/.NET LOB devs; reuses C# assembly. (Distinct from legacy `lang_vb6`.) |
| **Clojure** | JVM — no native FFI; rides JNA/JNR-FFI/Panama; closures = SAM/functional-iface [clj1][clj2] | B / **rides Java** | `lang_java` | T–M | JVM Lisp/data crowd; **references the Java JNA binding JAR directly**. |
| **Groovy** | JVM — JNA/JNR/Panama; Groovy closures coerce to Java SAM callbacks [gr1][gr2] | B / **rides Java** | `lang_java` | T | JVM scripting/Gradle crowd; reuses Java JNA JAR. |
| **Racket** | `ffi/unsafe`: `ffi-lib`+`get-ffi-obj`+`_fun`; procedure at `_fun` = C fn-ptr [rkt1][rkt2] | B | `lang_lisp`/`lang_lua` | M | Scheme/PLT/teaching; richest native FFI ergonomics of the Lisps. |
| **Chez Scheme** | `load-shared-object`+`foreign-procedure`; `foreign-callable` = C fn-ptr (pin w/ `lock-object`) [chez] | B (A-capable) | `lang_lisp` | M | Scheme purists; very stable (Racket-CS is built on it). |
| **Guile Scheme** | `(system foreign)`: `dynamic-link`+`pointer->procedure`; `procedure->pointer` = C fn-ptr [guile1][guile2] | B (A-capable) | `lang_lisp` | M | GNU extension-language crowd; `to_kebab_case` helper already exists. |
| **Tcl** | `cffi` pkg (libffi/dyncall) `Wrapper create`; `cffi::callback` — **call-scoped only** [tcl1][tcl2] | B | `lang_lua` | M | EDA/test-automation legacy; ⚠ callbacks can't be stored/fired async. |
| **SWI-Prolog** | `ffi` pack `c_import`(libffi+header parse) or `library(shlib)`; callbacks undocumented [swipl1][swipl2] | B | `lang_lua` | H | Logic-programming/AI research; callback support unstable/undocumented. |
| **Forth (gforth)** | `c-library`/`c-function` (compiles C wrapper at runtime); `c-callback` = C fn-ptr [forth1][forth2] | A/B | `lang_c.rs`/`lang_lua` | M | Embedded/retro/Forth diehards; needs a C compiler present at runtime. |
| **Elixir** | BEAM NIF (`erl_nif.h`, `:erlang.load_nif`); tooling `rustler`/`zigler` [beam1][beam2] | B* (C shim) | `lang_c.rs` (NIF) | M | Phoenix/BEAM crowd wanting native desktop; NIF crash kills VM. |
| **Erlang** | BEAM NIF (`ERL_NIF_INIT`, `erlang:load_nif/2`) or Port/C-node [beam1][beam3] | B* (C shim) | `lang_c.rs` (NIF) | M | Telecom/distributed-systems; same VM-crash caveat. |
| **Gleam** | `@external` to Erlang or JS target only — **no direct C**; hop via Erlang NIF or JS [gleam] | rides Erlang/Node | (via elixir/node) | H | Typed-BEAM newcomers; C is always one host-language hop away. |
| **AssemblyScript** | WASM only — no `dlopen`; `@external` WASM imports, numbers + linear-memory offsets [as1][as2] | — (host bridge) | `lang_node` (wasm) | — | Web/wasm crowd; only via a WASM host or Azul's own wasm build. |
| **Carbon** | C++ interop (LLVM-based); C reachable via C++ layer [carbon] | A (design) | `lang_cpp` | — | C++ successor hopefuls; **blocked — no shippable toolchain until ~2026+**. |
| **Jai** | `#foreign` + built-in bindings generator; C-ABI calling conventions [jai] | A | `lang_zig` | — | Gamedev; **blocked — closed/invite-only beta, no public compiler**. |
| **x86-64 / ARM64 assembly** | Follow the platform calling convention (SysV/AAPCS64); a label honoring it IS a C fn-ptr [asm] | A | `lang_c.rs` (header) | T | The genericity ground-truth: the C ABI *is* a register/stack contract. |
| **Ada** *(done — ref)* | `pragma Import(C,...)` / `Interfaces.C` | B | `lang_ada` (self) | — | Already shipped; aerospace/defense/high-integrity. |

---

## 3. Prioritized "implement blindly" wave plan

Ordered by **tractability × audience value**. Each item names the exact template
emitter to copy and the CI toolchain-install action to add. (CI actions already
present in `.github/workflows/rust.yml`: setup-python, setup-node, setup-ruby,
setup-java, setup-dotnet, setup-ocaml.)

### WAVE 1 — C-ABI-direct, trivial (biggest ROI, near-mechanical)

These produce real C function pointers incl. aggregate-by-value; copy the
archetype-A `lang_zig` template (single-file, direct `Az*_setOnClick`). Each is
a "read azul.h → emit foreign decls + a thin idiomatic wrapper + fn-ptr
callbacks" job with no host-invoker plumbing at all.

| Lang | Template | CI toolchain action | Notes |
|---|---|---|---|
| **Nim** | `lang_zig` | `jiro4989/setup-nim-action` | `{.cdecl.}` procs; `dynlib` = no link step. |
| **D** | `lang_zig` | `dlang-community/setup-dlang` | keep GC roots on held callbacks. |
| **Crystal** | `lang_zig`/`lang_go` | `crystal-lang/install-crystal` | box Proc into `void*`, retain vs GC. |
| **V (vlang)** | `lang_zig` | `vlang/setup-v` | `#flag -lazul`; transpiles to C. |
| **Swift** | `lang_go`/`lang_zig` | `swift-actions/setup-swift` (macOS: preinstalled Xcode) | `@convention(c)` no-capture; ctx via RefAny. **High audience value.** |
| **Objective-C** | `lang_cpp` | preinstalled (Xcode/clang on macOS runner) | reference C-superset case; near-free once cpp exists. |

*Wave-1 stretch (same archetype, niche/harder toolchain):* **Hare**
(build-from-source, Linux-only), **Nelua** (build-from-source), **Chapel**
(`apt install chapel` / spack — HPC audience), **Forth/gforth**
(`apt install gforth`, needs a C compiler at runtime). Template `lang_zig` /
`lang_c.rs`.

### WAVE 2 — host-invoker, moderate (managed runtimes, real audiences)

Copy an archetype-B template; wire the id→callable table, per-kind invoker
closures, and the releaser. `managed_host_invoker.rs::emit_cdef_block` already
emits the C-syntax cdef these need.

| Lang | Template | CI toolchain action | Notes |
|---|---|---|---|
| **Dart** | `lang_node` | `dart-lang/setup-dart` | `NativeCallable.isolateLocal` for closures; `.listener` for cross-thread. **Flutter-refugee audience.** |
| **Julia** | `lang_python`/`lang_lua` | `julia-actions/setup-julia` | `@cfunction` (could be archetype A); two-layer shim for foreign-thread callbacks. **Sci-computing.** |
| **Racket** | `lang_lisp`/`lang_lua` | `Bogdanp/setup-racket` | `_fun` auto-wraps callbacks; richest FFI. |
| **Chez Scheme** | `lang_lisp` | `apt install chezscheme` | `foreign-callable` + `lock-object`. |
| **Guile** | `lang_lisp` | `apt install guile-3.0` | `procedure->pointer`; `to_kebab_case` helper already present. |
| **Tcl** | `lang_lua` | `apt install tcl tcllib` (cffi) | ⚠ callbacks call-scoped only — event-driven fire needs care; may need generic-invoker + a stored-command bridge. |

### WAVE 3 — JVM / .NET / BEAM riders (near-free reuse of existing bindings)

These ride an already-shipped binding the way `scala` already rides `java`.
Mostly a language-surface veneer + example, not a new FFI layer.

| Lang | Rides / template | CI toolchain action | Notes |
|---|---|---|---|
| **F#** | rides **C#** (`lang_csharp`) | `actions/setup-dotnet` (present) | reference the C# binding assembly; or emit F# `extern` verbatim. |
| **VB.NET** | rides **C#** (`lang_csharp`) | `actions/setup-dotnet` (present) | `Declare`/`<DllImport>`; distinct from legacy `lang_vb6`. |
| **Clojure** | rides **Java** (`lang_java`) | `DeLaGuardo/setup-clojure` | call the Java JNA JAR via interop; closures = SAM callbacks. |
| **Groovy** | rides **Java** (`lang_java`) | `apt`/SDKMAN + `actions/setup-java` (present) | Groovy closures coerce to Java SAM; reuse Java JAR. |
| **Elixir** | C NIF shim (`lang_c.rs` + `erl_nif.h`) | `erlef/setup-beam` (elixir-version) | B* — write/generate a NIF that forwards to libazul; VM-crash risk. |
| **Erlang** | C NIF shim (`lang_c.rs` + `erl_nif.h`) | `erlef/setup-beam` (otp-version) | B* — same shim, loaded by `erlang:load_nif`. |
| **Gleam** | rides **Erlang** (via NIF) or **Node** | `erlef/setup-beam` (gleam-version) | `@external` to the Erlang NIF module or a JS binding. |

### WAVE 4 — hard / niche / demonstrative

| Lang | Template | CI toolchain action | Notes |
|---|---|---|---|
| **R** | `lang_lua` | `r-lib/actions/setup-r` | base R needs `.Call` C wrappers; or depend on `rdyncall::new.callback`. **Statistics audience** justifies the cost. |
| **SWI-Prolog** | `lang_lua` | `apt install swi-prolog` / setup-swipl | `ffi` pack; callback support undocumented — validate first. |
| **Mojo** | `lang_zig`/`lang_python` | `curl … modular install` (no marketplace action) | archetype A but volatile API + no header import; wait for stability. |
| **x86-64 / ARM64 assembly** | `lang_c.rs` (consume azul.h) | `apt install nasm` / binutils (preinstalled) | **Demonstrative genericity proof** — one hand-written hello-world proving the C ABI is the floor. Not a general binding. |
| **AssemblyScript** | `lang_node` (wasm bridge) | `actions/setup-node` (present) + `npm i assemblyscript` | only via a WASM host bridging to libazul, or Azul's own wasm build. |
| **Carbon** | `lang_cpp` | — | **blocked** until a shippable toolchain (~2026+). |
| **Jai** | `lang_zig` | — | **blocked** — closed beta, no public compiler. |

---

## 4. The "future language" checklist (future-proofing proof)

A hypothetical language invented tomorrow gets an Azul binding **iff** it can
tick these boxes. The list is short because the C ABI is the floor:

**Mandatory (without these, no binding — the true falsifiers):**

1. **Load a native shared library** at build or run time (link flag, `dlopen`,
   or module import). *If absent → sandboxed DSL (Starlark/Elm) → cannot bind.*
2. **Call an `extern "C"` function** through it: pass ints/pointers, receive a
   return value. *This alone enables the entire non-callback API surface.*
3. **Describe a `repr(C)` struct** by value (header import, manual layout, or
   struct-marshalling attribute). Needed to pass Azul's config/DOM structs.

**One of these two (determines archetype):**

4a. **Mint a C-callable function pointer** — even non-capturing/top-level only.
   → **Archetype A.** Per-instance state rides the `RefAny data` arg, so "no
   capture" is not a limitation. (Nim `cdecl`, Swift `@convention(c)`, Julia
   `@cfunction`, assembly label, …)

4b. **Register a single generic callback** of the fixed shape
   `void(uint64, const char*, const void* const*, size_t, void*)` (the
   `AzGenericInvoker`), even if it can't do aggregate-by-value or per-kind
   variance. → **Archetype B.** libazul's static thunk does the by-value
   plumbing; the host only ever sees pointers. *This is the weakest possible
   callback requirement — one libffi closure of a pointer-only signature.*

**Nice-to-have (polish, never blocking):**

5. A deterministic "object is dead" hook (destructor, finalizer, or GC
   callback) to pair with `AzApp_setHostHandleReleaser`. Absent → the host
   leaks its id-table entries, but callbacks still fire correctly.
6. A native module system to mirror `api.json`'s modules (see
   `BINDING_STRATEGY_PER_LANGUAGE.md`). Absent → a flat namespace, still works.
7. A way to acquire the host VM lock for off-main-thread `ThreadCallback` (§5).
   Absent → use the writeback-only pattern (main-thread `WriteBackCallback`).

**Corollary — a language that satisfies (1)–(3) but neither (4a) nor (4b)**
would be bindable for the *entire non-callback API* and could still drive
callbacks through a **compiled C shim** (the BEAM/NIF path). So the only genuine
"cannot bind at all" case remains a language that removes (1)+(2) on purpose.
Because those are always sandboxed config/frontend DSLs — never systems or
application languages — the design is future-proof for its intended domain.

---

## 5. Reference — per-VM lock table for `ThreadCallback` (off-main-thread)

Reused from `BINDING_STRATEGY_PER_LANGUAGE.md`. Only `ThreadCallback` fires on
a worker thread; the per-language invoker must acquire the host VM lock before
dispatching. Everything else (`Callback`, `LayoutCallback`, 19 widget kinds,
`WriteBackCallback`) fires on main and needs no lock.

| VM | Acquire / Release |
|---|---|
| CPython | `PyGILState_Ensure()` / `PyGILState_Release(state)` |
| MRI Ruby | `rb_thread_call_with_gvl(fn, data)` (wraps acquire+call+release) |
| JVM (Java/Kotlin/Clojure/Groovy) | `AttachCurrentThread` / `DetachCurrentThread` (cache `JavaVM*`) |
| CLR (C#/F#/VB.NET) | `[UnmanagedCallersOnly]` self-trampolines from any thread |
| Node (N-API) | `napi_*_threadsafe_function` lifecycle |
| Dart | `NativeCallable.listener` (async, any thread) |
| OCaml | `caml_acquire_runtime_system()` / `_release_` |
| SBCL / Chez / Guile | foreign-callable auto-attaches |
| Julia | schedule real callback via a C shim (callbacks unsafe on arbitrary threads) |
| Lua / Perl / PHP / Tcl (single-threaded) | no lock; use writeback-only pattern |
| Go / Zig / Nim / D / Crystal / V / Odin / Rust / Swift / Objective-C / assembly (native) | no lock needed |

---

## 6. Sources

- **Nim:** [nim1] https://nim-lang.org/docs/manual.html#foreign-function-interface · [nim2] https://nim-lang.org/docs/dynlib.html
- **D:** [d] https://dlang.org/spec/interfaceToC.html
- **Crystal:** [cr1] https://crystal-lang.org/reference/latest/syntax_and_semantics/c_bindings/lib.html · [cr2] https://crystal-lang.org/reference/latest/syntax_and_semantics/c_bindings/callbacks.html
- **V:** [v] https://docs.vlang.io/v-and-c.html
- **Zig:** [zig] https://ziglang.org/documentation/master/#C
- **Odin:** [odin] https://odin-lang.org/news/binding-to-c/
- **Objective-C:** [objc] https://developer.apple.com/library/archive/documentation/DeveloperTools/Conceptual/DynamicLibraries/100-Articles/UsingDynamicLibraries.html
- **Swift:** [swift1] https://www.swift.org/documentation/cxx-interop/ · [swift2] https://docs.swift.org/swift-book/documentation/the-swift-programming-language/attributes/
- **Vala:** [vala1] https://docs.vala.dev/tutorials/programming-language/main/06-00-libraries/06-03-binding-libraries-with-vapi-files.html · [vala2] https://docs.vala.dev/developer-guides/bindings/writing-a-vapi-manually.html
- **Hare:** [hare] https://harelang.org/documentation/usage/system-libraries.html
- **Nelua:** [nelua] https://nelua.io/clibraries/
- **Mojo:** [mojo1] https://docs.modular.com/mojo/stdlib/sys/ffi/ · [mojo2] https://docs.modular.com/mojo/stdlib/sys/ffi/external_call/
- **Chapel:** [chapel] https://chapel-lang.org/docs/technotes/extern.html
- **Julia:** [jl1] https://docs.julialang.org/en/v1/manual/calling-c-and-fortran-code/ · [jl2] https://docs.julialang.org/en/v1/base/c/
- **Dart:** [dart1] https://dart.dev/interop/c-interop · [dart2] https://api.dart.dev/stable/dart-ffi/Pointer/fromFunction.html · [dart3] https://api.dart.dev/dart-ffi/NativeCallable-class.html
- **R:** [r1] https://stat.ethz.ch/R-manual/R-devel/library/base/html/dynload.html · [r2] https://rdrr.io/rforge/rdyncall/man/dyncallback.html
- **F#:** [fs1] https://learn.microsoft.com/en-us/dotnet/fsharp/language-reference/functions/external-functions · [fs2] https://learn.microsoft.com/en-us/dotnet/api/system.runtime.interopservices.marshal.getfunctionpointerfordelegate
- **VB.NET:** [vb1] https://learn.microsoft.com/en-us/dotnet/api/system.runtime.interopservices.dllimportattribute · [vb2] https://learn.microsoft.com/en-us/dotnet/standard/native-interop/best-practices
- **Clojure:** [clj1] https://github.com/Chouser/clojure-jna · [clj2] https://github.com/jnr/jnr-ffi
- **Groovy:** [gr1] https://blog.bloidonia.com/post/26134471586/using-jna-with-groovy · [gr2] https://github.com/jnr/jnr-ffi
- **Racket:** [rkt1] https://docs.racket-lang.org/foreign/index.html · [rkt2] https://docs.racket-lang.org/foreign/foreign_procedures.html
- **Chez:** [chez] https://www.scheme.com/csug8/foreign.html
- **Guile:** [guile1] https://www.gnu.org/software/guile/manual/html_node/Foreign-Functions.html · [guile2] https://www.gnu.org/software/guile/manual/html_node/Dynamic-FFI.html
- **Tcl:** [tcl1] https://cffi.magicsplat.com/ · [tcl2] https://cffi.magicsplat.com/cffi-Concepts.html
- **SWI-Prolog:** [swipl1] https://www.swi-prolog.org/pack/list?p=ffi · [swipl2] https://www.swi-prolog.org/pldoc/man?section=shlib
- **Forth:** [forth1] https://gforth.org/manual/Calling-C-Functions.html · [forth2] https://gforth.org/manual/Callbacks.html
- **BEAM (Erlang/Elixir):** [beam1] https://www.erlang.org/doc/system/nif.html · [beam2] https://elixir-lang.org/blog/2025/08/18/interop-and-portability/ · [beam3] https://www.erlang.org/doc/apps/erts/erl_nif.html
- **Gleam:** [gleam] https://gleam.run/documentation/externals/
- **AssemblyScript:** [as1] https://www.assemblyscript.org/concepts.html · [as2] https://www.assemblyscript.org/runtime.html
- **Carbon:** [carbon] https://github.com/carbon-language/carbon-lang/blob/trunk/docs/design/interoperability/philosophy_and_goals.md
- **Jai:** [jai] https://www.oskarmendel.me/p/using-jais-bindings-generator-to
- **Assembly (SysV ABI):** [asm] https://refspecs.linuxbase.org/elf/x86_64-abi-0.99.pdf
- **Starlark (falsifier):** https://github.com/bazelbuild/starlark/blob/master/spec.md

---

*Report generated 2026-07-06. Internal architecture references:
`core/src/host_invoker.rs`, `doc/src/codegen/v2/managed_host_invoker.rs`,
`doc/src/codegen/v2/lang_{zig,go,c,lua,ruby,java,csharp,lisp}/`,
`scripts/BINDING_STRATEGY_PER_LANGUAGE.md`.*
