---
slug: host-invoker
title: Host-Invoker Pattern (managed-FFI callbacks)
language: en
canonical_slug: host-invoker
audience: contributor
maturity: mature
guide_order: null
topic_only: false
short_desc: How LuaJIT/Ruby/PHP/Node/etc. wire up Azul callbacks despite libffi's struct-by-value limit
prerequisites: [build-and-codegen, code-organization]
tracked_files:
  - core/src/host_invoker.rs
  - core/tests/host_invoker.rs
  - doc/src/codegen/v2/managed_host_invoker.rs
  - doc/src/codegen/v2/lang_lua/managed.rs
  - doc/src/codegen/v2/lang_ruby/managed.rs
  - doc/src/codegen/v2/lang_php/managed.rs
  - doc/src/codegen/v2/lang_node/managed.rs
  - doc/src/codegen/v2/lang_lisp/managed.rs
  - doc/src/codegen/v2/lang_csharp/managed.rs
  - doc/src/codegen/v2/lang_java/managed.rs
  - doc/src/codegen/v2/lang_kotlin/managed.rs
default-search-keys:
  - host-invoker
  - impl_managed_callback
  - register_callback
  - libffi
---

# Host-Invoker Pattern

## Why this exists

Azul's callback typedefs pass aggregates by value:

```rust
pub type CallbackType        = extern "C" fn(RefAny, CallbackInfo) -> Update;
pub type LayoutCallbackType  = extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom;
pub type ButtonOnClickCallbackType
                             = extern "C" fn(RefAny, CallbackInfo) -> Update;
```

That's perfectly C-ABI — every C and C++ user can pass a `&extern "C" fn`
straight in. But every **managed-FFI** binding we ship — LuaJIT FFI,
ruby-ffi, PHP FFI, koffi (Node), CFFI (Common Lisp), FFI::Platypus
(Perl), ctypes Foreign.funptr (OCaml) — sits on top of **libffi**, and
libffi's closure builder cannot synthesise a C-callable trampoline whose
signature has aggregate-by-value args. A naive
`ffi.cast('AzCallbackType', luaFn)` either silently produces a junk
pointer or refuses to parse the typedef.

We can't fix libffi. What we ship instead is a **C-side adapter** built
into `libazul`: a per-callback-kind static thunk whose signature *is*
aggregate-by-value (so the framework can call it without changes), and
which *forwards* to a host-side closure whose signature is pointer-args
only (so libffi *can* synthesise it).

## Architecture in one picture

```
                       managed-FFI host (Lua / PHP / Node / …)
       ┌───────────────────────────────────────────────────────────┐
       │  user code:                                                │
       │      function on_click(data, info) … end                   │
       │                                                            │
       │  registerCallback('Callback', on_click)  ┐                 │
       │      └─► allocate id, stash fn in handles[id]              │
       │      └─► call AzCallback_createFromHostHandle(id) ─────────┼──┐
       │                                                            │  │
       │  registered libffi closure (pointer args only):            │  │
       │      callback_invoker(id, *RefAny, *CallbackInfo, *Update) │  │
       │          fn = handles[id]; ret = fn(...); *out = ret       │  │
       │      ▲                                                     │  │
       └──────┼─────────────────────────────────────────────────────┘  │
              │                                                        │
              │ (registered once at module load via                    │
              │  AzApp_setCallbackInvoker)                             │
              │                                                        │
              ▼                                                        │
       ┌───────────────────────────────────────────────────────────┐   │
       │ libazul (Rust)                                            │   │
       │                                                           │   │
       │   AzCallback { cb: az_callback_thunk, ctx: <handle> } ◄───┼───┘
       │                                                           │
       │   az_callback_thunk(data: RefAny, info: CallbackInfo)     │
       │       -> Update {                                         │
       │     let handle = info.get_ctx().refany_to_host_handle();  │
       │     let invoker = CALLBACK_INVOKER.get();                 │
       │     let mut out = Update::DoNothing;                      │
       │     invoker(handle, &data, &info, &mut out);              │
       │     out                                                   │
       │   }                                                       │
       └───────────────────────────────────────────────────────────┘
```

The arrows in red ink: the framework calls `cb` with by-value args
(works because the thunk is compiled by Rust); the thunk calls the
registered libffi closure with pointer args (works because libffi can
synthesise that). The user's host-language function never sees the
aggregate-by-value version of the typedef.

## The Rust side: `impl_managed_callback!`

`core/src/host_invoker.rs` defines:

* **`AzApp_setHostHandleReleaser(extern "C" fn(u64))`** — process-global
  hook fired when a host-handle `RefAny`'s last clone drops. Lets the
  host drop its `id → callable` table entry.
* **`AzRefAny_newHostHandle(u64) -> AzRefAny`** + **`AzRefAny_getHostHandle(*const AzRefAny) -> u64`**
  — same id-keyed path serves user data, so callbacks and `refanyCreate`
  share one releaser and one map.
* The macro **`impl_managed_callback! { … }`** — expands per kind to a
  static thunk, an `AzApp_set<Kind>Invoker` setter, and an
  `Az<Wrapper>_createFromHostHandle(u64)` constructor.

A typical invocation:

```rust
azul_core::impl_managed_callback! {
    wrapper:        ButtonOnClickCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: BUTTON_ON_CLICK_INVOKER,
    invoker_ty:     AzButtonOnClickCallbackInvoker,
    thunk_fn:       az_button_on_click_callback_thunk,
    setter_fn:      AzApp_setButtonOnClickCallbackInvoker,
    from_handle_fn: AzButtonOnClickCallback_createFromHostHandle,
}
```

For widget callbacks that take state extras
(e.g. `(RefAny, CallbackInfo, CheckBoxState) -> Update`), append:

```rust
    extra_args: [ state: CheckBoxState ],
```

The macro forwards every extra by pointer through the libffi closure —
no aggregate-by-value anywhere on the host-facing signature. It also
expands a handful of `Display`/`Debug`/`Clone` impls that the wrapper
struct needs to satisfy `repr(C)`'s derive bounds.

`default_ret` is what the thunk returns when:
* the framework called the typedef directly without going through this
  path (`OptionRefAny::None` ctx),
* the ctx came from somewhere that isn't a host-handle RefAny,
* or no invoker has been registered yet for this kind.

Pick a value that can't be confused with a "real" return — typically the
kind's "do nothing" / "empty body" default.

## Where the ctx is preserved (critical)

The host-handle is carried in the wrapper's `ctx: OptionRefAny` field.
Three call sites in the framework would otherwise drop it:

* `dll/src/desktop/shell2/common/layout.rs::regenerate_layout` — calls
  `info.set_callable_ptr(&layout_callback.ctx)` before invoking.
* `layout/src/window.rs::invoke_single_callback` —
  `CallbackInfoRefData.ctx = callback.ctx.clone()` (was hard-coded
  `None` once upon a time).
* `layout/src/callbacks.rs::Callback::from_core` — preserves ctx when
  reconstructing from the storage form.

If `info.get_ctx()` returns `None` inside the thunk, the host-invoker
falls back to `default_ret` and the user's callback never fires. That's
the symptom to look for if you add a kind and the dispatch silently
no-ops.

## How to register a new widget callback

Three steps. Once any one is wrong the host-language side either won't
codegen (silent allowlist filter) or won't dispatch (silent ctx loss).

### 1. Apply `impl_managed_callback!` next to the widget's `impl_widget_callback!`

```rust
// layout/src/widgets/my_widget.rs
pub type MyWidgetOnFooCallbackType =
    extern "C" fn(RefAny, CallbackInfo, MyWidgetState) -> Update;

impl_widget_callback!(
    MyWidgetOnFoo,
    OptionMyWidgetOnFoo,
    MyWidgetOnFooCallback,
    MyWidgetOnFooCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        MyWidgetOnFooCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: MY_WIDGET_ON_FOO_INVOKER,
    invoker_ty:     AzMyWidgetOnFooCallbackInvoker,
    thunk_fn:       az_my_widget_on_foo_callback_thunk,
    setter_fn:      AzApp_setMyWidgetOnFooCallbackInvoker,
    from_handle_fn: AzMyWidgetOnFooCallback_createFromHostHandle,
    extra_args:     [ state: MyWidgetState ],
}
```

The convention: `invoker_static` is `SCREAMING_SNAKE`,
`invoker_ty` / setter / handle-fn keep the `Az` prefix and the wrapper
name verbatim, `thunk_fn` is `snake_case`. The codegen does not parse
these names — they only need to be unique within `core::host_invoker`.

### 2. Register the wrapper in `HOST_INVOKER_KINDS`

```rust
// doc/src/codegen/v2/managed_host_invoker.rs
pub const HOST_INVOKER_KINDS: &[&str] = &[
    "Callback",
    // …existing entries…
    "MyWidgetOnFooCallback",
];
```

Every managed-FFI adapter (`lang_lua/managed.rs`, `lang_ruby/managed.rs`,
…) iterates this list, so adding the entry here is enough — no per-language
edit needed.

The wrapper name here is the struct name **without** the `Az` prefix
and **without** the `Type` suffix. (The codegen helper strips them in
`wrapper_name(cb)`.)

### 3. Rebuild the dll, rerun codegen

```bash
cargo build --release -p azul-dll
cargo run --bin azul-doc -- codegen all
```

That re-emits `azul.lua`, `azul.rb`, `Azul.php`, `azul.js`, `azul.lisp`,
`Azul.cs`, `AzulHostInvoker.java`, `Azul.kt`, and `Azul.psm1` with the
new kind wired up automatically. There's no per-language "register"
step; every adapter walks the same allowlist.

## How to add a new language adapter

Three tiers, picked by the host language's FFI capabilities.

### Tier A — Native struct-by-value + closures

Languages that can synthesise a C-callable function pointer from a
host-side closure with aggregate args:
**C# / .NET (P/Invoke + delegates), Java/Kotlin (JNA Callback), Python
(PyO3, compiled in), Haskell (`foreign export ccall "wrapper"`), Zig,
Go (with cgo)**.

These don't *need* the host-invoker pattern, but apply it anyway for
uniformity. `lang_csharp/managed.rs` is the reference. The shape:

* A sibling `NativeMethodsManaged` class (or namespace) holding
  `[DllImport]` declarations for the host-invoker C-ABI exports.
* A static `HostInvoker` class with `RegisterCallback(...)` factories
  per kind, plus `RefanyCreate(value)` / `RefanyGet(refanyPtr)`.
* Per-kind delegate types matching the libffi pointer-arg invoker
  signature.
* GC-pinning is one static `List<Delegate>` so
  `Marshal.GetFunctionPointerForDelegate(delegate)` can't have its
  trampoline collected.

### Tier B — No closures, struct-by-value works

Static-procedure languages — **Fortran, COBOL, Ada, Pascal, FreeBASIC,
VB6, Algol 68**. Closures don't exist; the user defines a static
procedure and stashes per-instance state in the `RefAny`.

These don't need a `managed.rs` at all. The codegen emits ordinary
`bind(c)` / `Convention(C)` / `cdecl;` declarations against the
production `azul.h` and the user passes `c_funloc(my_proc)` directly.
The host-invoker is unused; the framework's RefAny refcount handles
lifetime.

### Tier C — Libffi-restricted (the host-invoker tier)

Languages whose FFI library can't synthesise aggregate-by-value
trampolines: **Lua (LuaJIT FFI), Ruby (ruby-ffi), Perl
(FFI::Platypus), PHP (built-in FFI), OCaml (ctypes), Node (koffi —
Bun/Deno are technically capable but ride along for uniformity),
Common Lisp (CFFI), Smalltalk (Pharo UnifiedFFI)**.

These need a per-language `lang_<X>/managed.rs`. Reference: any of
`lang_lua/managed.rs` or `lang_php/managed.rs`. The shape:

1. **cdef declarations** for the host-invoker C-ABI exports — splice
   into the language's cdef block. Reuse
   `managed_host_invoker::emit_cdef_block(out, ir)` for a C-syntax
   payload that LuaJIT, PHP FFI, koffi, and CFFI all accept.
2. **Per-kind libffi closure registration** at module load — a closure
   per kind whose signature is `(u64 id, …pointer args…, T* out)`. The
   closure looks up `_handles[id]` and dispatches.
3. **`registerCallback(kind, fn)`** — allocates a host-handle id,
   stashes `fn`, returns
   `Az<Wrapper>_createFromHostHandle(id)`.
4. **`refanyCreate(value)` / `refanyGet(refany)`** — same id-keyed path
   so user data and callbacks share one lifetime story.

Wire the adapter into `lang_<X>/mod.rs` between the existing types/functions
emitters and the wrapper emitter — the wrappers will reference
`registerCallback` once you teach them to (see "Wrapper substitution"
below).

## Wrapper substitution (idiomatic call sites)

By default a Tier C user has to write:

```lua
local on_click_cb = azul.registerCallback('Callback', on_click)
button:set_on_click(data:clone(), on_click_cb)
```

The wrapper-emitter substitution in `lang_<X>/wrappers.rs` lets the user
write:

```lua
button:set_on_click(data:clone(), on_click)  -- closure handed in directly
```

The substitution rule, implemented in `lang_lua/wrappers.rs::emit_callback_pin_lines`:

* For every method arg whose IR `callback_info` is `Some`, prepend
  `arg = azul._register_callback('<Wrapper>', arg)` before the C call.
* Special-case: when the C ABI takes the *raw fn pointer typedef* (e.g.
  `WindowCreateOptions::create(LayoutCallbackType)` — passes the cb but
  drops ctx), bypass via `_default()` + direct field assignment so the
  host-handle ctx survives.

Tier A languages can do the same substitution targeting their native
delegate type. Tier B doesn't need it — static procedures don't have an
"is this a closure?" question.

## Generic byte-buffer invoker (deferred)

For **user-defined custom callback kinds** in Tier C hosts (i.e. the
user wants a kind that isn't in `HOST_INVOKER_KINDS` and can't easily
recompile `libazul`), one design option is a single C-ABI export:

```c
typedef void (*AzGenericInvoker)(
    uint64_t handle,
    const char* kind,        /* wrapper name as a null-terminated string */
    const void* args[],      /* array of pointers to args */
    size_t args_count,
    void* return_ptr         /* where to write the return value */
);
extern void AzApp_setGenericInvoker(AzGenericInvoker);
```

The macro's static thunk would, when no per-kind invoker is registered,
fall back to the generic invoker with packed args. Users in
libffi-restricted hosts could then add a custom kind by writing a small
C trampoline that calls the generic invoker — no upstream Rust patch.

This isn't built today. The per-kind path scales fine for the
~20 callback kinds the framework ships, and the
`HOST_INVOKER_KINDS` allowlist is the only thing each new kind has to
touch (one constant entry, one macro callsite). If user-extensible kinds
become a recurring ask, this is the design to pick up.

## Tests

Process-global slots make these tests serialise on a `Mutex`, but the
coverage is enough:

* `host_handle_to_refany(id)` round-trips through `refany_to_host_handle`.
* The destructor stamped into host-handle RefAnys forwards the id to
  the registered releaser exactly once, when the *last* clone drops.
* `refany_to_host_handle` returns `None` for unrelated RefAnys (so a
  user-data RefAny accidentally fed into a callback's ctx slot can't
  free a foreign id).
* The macro-generated thunks short-circuit safely when no invoker has
  been registered yet.

```bash
cargo test -p azul-core --test host_invoker
```

The end-to-end "thunk fires invoker with the right by-value args" path
is exercised through `examples/lua/hello-world.lua` — the click counter
increments through `on_click → Lua → counter mutation → next-frame
layout cb`, which only works if every layer of the host-invoker
plumbing is wired correctly.
