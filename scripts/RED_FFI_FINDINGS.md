# Red (red-lang.org) FFI capability audit — 2026-07-06

**Question:** Can the Red programming language get a native GUI via Azul by loading
`libazul` (a C-ABI shared library) and calling `Az*` functions — and can it receive
Azul callbacks? This is a deliberate falsification test for the "Azul can bind ANY
language" thesis. Red is NOT installed locally; this is a documentation-only audit of
Red's published FFI surface plus a constructed binding.

## TL;DR verdict: **FEASIBLE (via Red/System), ALPHA / unverified**

Red is a *full-stack* language: a high-level Rebol-like dialect (**Red**) sitting on
top of a low-level, statically-typed, C-like dialect (**Red/System**). The two compile
together to a single native, dependency-free executable via the same ~1 MB toolchain.

- **Red/System has complete, bidirectional C FFI.** It can `#import` an external shared
  library, call its functions (including struct-by-value args and returns), and produce
  real C-callable function pointers from Red/System functions (for callbacks). This is
  everything Azul's C ABI needs.
- **High-level Red reaches C only *through* Red/System** — via the `routine!` type and
  `#system` / `#system-global` embedded blocks. Pure interpreted Red (the console) has
  **no** general "dlopen an arbitrary .so and call it" facility; the bridge is always
  compiled Red/System. This is a real and important distinction (see "Honest limits").

So Red does **not** falsify the thesis — a Red program *can* drive an Azul GUI — but the
truthful framing is "via Red/System," not "from pure interpreted Red." The binding is
therefore emitted as a Red/System include, `azul.reds`.

---

## 1. How Red/System calls an external C shared library — `#import`

Red/System loads external shared libraries at executable load time. The `#import`
compiler directive maps library symbols into the current context. It supports both
`cdecl` (C convention — what libazul's `extern "C"` exports use) and `stdcall`, any
number of libraries/functions, and imported variables.

```red
Red/System [Title: "loadlib"]
#import [
    "testlib.dll" stdcall [
        inc: "inc" [n [integer!] return: [integer!]]
    ]
]
print inc 123    ;-- prints 124
```

> "Red/System is able to load external shared libraries at the time a Red/System
> executable is loaded by the operating system. This feature is called *library import*
> ... supported by a specific compiler directive: `#import`." — Red/System spec,
> "Importing external functions."

**Argument/return types available to imported functions:** `integer!`, `byte!`,
`logic!`, `float!`, `float32!`, `c-string!`, `pointer! [...]`, `struct! [...]`, and
`function! [...]` (function-pointer / callback args). — Red/System spec, "#import" and
"Datatypes."

### Struct-by-value (the crux for Azul)

Azul's C ABI passes and returns many aggregates *by value* (e.g. `AzDom` ≈ 240 B,
`AzString`, `AzWindowCreateOptions`). Red/System supports this:

> "In the current implementation, `pointer!`, `integer!`, `byte!`, `float!`, `float32!`
> and `logic!` arguments are passed by value, while `c-string!` and `struct!` arguments
> are passed **by reference**." — Red/System spec, "Function arguments."

> "Adding the `value` keyword after a `struct!` type specification allows it to be passed
> **by value** (works also for returned value)." — Red/System spec, "struct!".

> "By default, a struct is returned by reference. To return a struct by value, a `value`
> keyword needs to be added in the return specification block."
> Example: `foo: func [return: [struct! [n [integer!] value]]] [...]`

> "On the implementation side, Red/System follows the common ABI used by mainstream C
> compilers, so that passing structs to/from a C library should work fine."
> — Red/System spec, struct-passing notes.

**Conclusion:** with the `value` keyword on struct arguments/returns, Red/System can bind
Azul's by-value struct functions, and it claims mainstream-C-ABI compatibility. Struct
types are named via the `alias` keyword: `AzDom!: alias struct! [ ... ]`, so the imported
signatures read `data [AzRefAny! value]` etc.

## 2. Producing a C-callable function pointer FROM Red (callbacks)

Red/System can hand a native function pointer to external C code — no host-invoker is
strictly required for the *mechanism* (unlike libffi/LuaJIT hosts):

> "It is possible to obtain a function address ... to pass it, for example, as an argument
> to external calls with callbacks." Syntax: `:functionName`. — Red/System spec, "Getting
> a function's address."

Two attributes make a Red/System function safe as a C callback:

- `[cdecl]` — "Changes function's calling convention to C convention. This allows to
  safely pass a Red/System function as argument to imported C functions."
- `[callback]` — "The purpose of the callback attribute is to manually force a callback
  compilation mode for a function that the compiler failed to infer as a callback...
  if Red/System function pointers are passed to an external API in an indirect way
  (filling an array or a structure), and those functions will be later called by the
  external code (OS or a library), the callback attribute *must* be used."
  — Red/System spec, "Function attributes."

Azul stores the callback fn-ptr *inside a wrapper struct* that the framework calls later,
which is exactly the "indirect / stored in a structure, called later by the library"
situation → the `[callback]` attribute applies.

### Why we still use the host-invoker pattern (not direct fn-ptrs)

Even though Red/System *can* emit direct C callbacks, the binding routes callbacks through
libazul's **host-invoker** plumbing (`core/src/host_invoker.rs`), identically to the
Fortran/Pascal bindings, because:

1. The per-kind invoker signature is **all pointers + one out-pointer** — no aggregate
   passed *by value across the callback boundary*. That side-steps every remaining ABI
   risk in Red/System's by-value-struct-in-a-callback path (the least-exercised corner of
   its FFI) while keeping the by-value plumbing on the well-trodden libazul C side.
2. It gives Red one uniform lifetime story (`AzApp_setHostHandleReleaser`) shared by
   RefAny user data and callbacks — no bespoke GC bookkeeping.

So the Red callback path is: Red/System `[callback]`-attributed dispatcher fn → registered
once via `AzApp_set<Kind>Invoker` → libazul's static thunk does the by-value work and calls
the dispatcher with pointer args → dispatcher looks the user routine up by handle id.

## 3. High-level Red → C bridge (`routine!`, `#system-global`)

A high-level Red program embeds Red/System via:

- `#system-global [ ... ]` — inject Red/System code (including an `#import` block) at
  global scope of the compiled program.
- `routine!` — a Red function whose body is Red/System, callable from high-level Red;
  the toolchain compiles it to native code. This is the only way high-level Red values
  cross into C.

libRed additionally exposes Red-the-runtime *to* C (the reverse direction), but that is
not what we need here.

**Sources:** Red/System Language Specification (`static.red-lang.org/red-system-specs-light.html`
and `.../red-system-specs.html`); Red blog "0.3.3: Shared libraries and Android!"
(`red-lang.org/2013/08/033-released-shared-libraries-and.html`); Red blog "0.6.2: LibRed
and Macros" (`red-lang.org/2017/03/062-libred-and-macros.html`); Red docs
(`doc.red-lang.org`); `github.com/red/red`.

---

## Honest limits (why this is ALPHA, not shippable-green)

1. **Not compiled/tested.** No Red toolchain is installed. `azul.reds` and
   `examples/red/hello-world.red` are constructed from the published spec, not verified
   by `redc`. Treat as ALPHA (documentation-grade), like other unverified-tier bindings.

2. **It is Red/System, not interpreted Red.** The GUI is driven from the compiled
   low-level dialect. Pure interpreted Red (the REPL/console) cannot dlopen libazul on its
   own. High-level Red reaches it only by embedding Red/System (`routine!` /
   `#system-global`). The thesis holds ("a Red program can", same toolchain, same binary)
   but must be stated truthfully.

3. **64-bit integers are a Red/System weak spot.** Red/System's `integer!` is 32-bit and
   the dialect historically lacks a portable 64-bit integer type. The host-invoker handle
   ids and any `i64`/`u64` API fields are affected. Mitigations used by the binding:
   handle ids start at 1 and stay small, and are declared as pointer-width where the ABI
   needs 64 bits. Any real use of 64-bit-valued API fields needs a Red/System `int64!`
   shim (or the current toolchain's 64-bit-int support if present). Documented, not
   hand-waved.

4. **Exact tagged-union sizing.** Azul's `AzOption*` / `AzResult*` / union types must be
   byte-exact opaque blobs for by-value calls to stay ABI-correct (the Fortran/Pascal
   bindings compute this via a shared `layout` pass). The Red generator emits regular
   structs field-accurately and unit enums as integer `#define`s; union blobs are marked
   for the same `layout`-pass wiring as a follow-up (see WIRING_red.md). Until then the
   generated `azul.reds` covers the scalar/struct/pointer surface the counter demo needs
   and flags unions.

5. **arm64 by-value aggregates.** Red/System claims mainstream-C-ABI compatibility, but
   the AArch64 rules for >16-byte aggregates (indirect via pointer, x8 sret) are subtle
   and this corner of Red/System is not something we could exercise. macOS-arm64 is the
   platform most likely to expose a gap; first verification should be x86-64 Linux/Windows.

## What Red would need to be a first-class (green) Azul target

- A verified 64-bit integer type in Red/System (for handle ids / API i64/u64 fields).
- CI availability of the Red toolchain (a `redc` GitHub Action) so the binding can be
  compiled and the counter e2e-driven, promoting it out of ALPHA.
- Confirmation of struct-by-value returns for large (>16 B) aggregates on arm64.

None of these are conceptual blockers — they are verification/toolchain gaps. **Red does
not falsify "Azul can bind any language."**
