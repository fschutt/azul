---
slug: security
title: Security Model
language: en
canonical_slug: security
audience: external
maturity: wip
guide_order: 500
topic_only: false
short_desc: Security model — how callbacks, RefAny, and external data sources are isolated, and what threats the framework does and does not defend against.
prerequisites: [architecture, web-deployment]
tracked_files:
  - core/src/refany.rs
  - core/src/callbacks.rs
  - dll/src/web/config.rs
  - dll/src/web/mod.rs
  - dll/src/web/server.rs
  - dll/src/web/transpiler.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# Security Model

> **WIP.** The desktop trust model is mature: `RefAny`'s FFI guarantees
> have been stable since the project's first release. The web backend's
> sandbox model is Phase 0 — server-side execution with no untrusted
> client code path. The shape of the threat model won't change when
> WASM lifting lands; the *implementation* will.

Azul is a library, not an isolation boundary. Application code runs
with full process privilege. The framework's job is narrower:

1. Keep the FFI surface (`RefAny`, callbacks, C-ABI types) sound when
   crossed from any supported binding language.
2. Keep the layout, parser, and rendering paths free from untrusted-
   input vulnerabilities (panic-free on hostile bytes, no buffer
   over-reads).
3. Define a sandbox boundary for the web target so callbacks lifted to
   the browser cannot reach beyond what the server exports.

## Trust boundaries

| Boundary | Trusted side | Untrusted side | Mechanism |
|---|---|---|---|
| Rust ↔ C/C++/Python bindings | Both | Neither | `#[repr(C)]` types + runtime checks in `RefAny` |
| Layout callback ↔ application data | Layout cb | App data | `RefAny::downcast_*` (type id + borrow check) |
| Desktop process ↔ user input | Process | Input bytes | Parser fuzz hardening; layout never panics on UTF-8 input |
| Web server ↔ network client | Server | HTTP request | Method/path matching, 16 MB body cap, no auth |
| Web server ↔ lifted WASM (Phase 1+) | Server | Browser-resident WASM | Phase 0: not yet a boundary — WASM is empty |

Inside a single desktop process, all callbacks share an address space
and can corrupt each other if they break Rust's aliasing rules through
`unsafe`. The framework provides no in-process sandbox.

## What `RefAny` guarantees

Every callback receives a `RefAny` and downcasts it to a typed
reference. The framework guards five invariants:

- **Type identity**. `downcast_ref` and `downcast_mut`
  (`core/src/refany.rs:840` and `:906`) compare a 64-bit `type_id`
  before any cast. A wrong-type downcast returns `None`; it never
  produces an undefined-behavior pointer.
- **Allocation alignment**. `RefAny::new_c` (`refany.rs:677`) takes the
  type's `align_of` and uses `Layout::from_size_align` so the heap
  pointer is always correctly aligned for the stored type.
- **Reference counting**. `num_copies`, `num_refs`, `num_mutable_refs`
  are all `AtomicUsize` with `SeqCst` ordering. Drop-when-last-ref is
  race-free across threads.
- **Runtime borrow check**. Shared and mutable borrows can't coexist.
  A second `downcast_mut` while a `RefMut` is live returns `None`.
- **Type-correct destruction**. The `custom_destructor` stored on
  construction (`refany.rs:608`) is the only function that frees the
  inner value. It's monomorphized per type at construction time, so
  the right `Drop` runs even though the pointer crosses FFI as `*mut c_void`.

```rust
# extern crate azul_core;
# use azul_core::refany::RefAny;
let mut data = RefAny::new(42i32);

assert!(data.downcast_ref::<i32>().is_some());
assert!(data.downcast_ref::<u32>().is_none());        // wrong type → None
{
    let _live = data.downcast_mut::<i32>().unwrap();
    assert!(data.downcast_ref::<i32>().is_none());    // already mut-borrowed
}
assert!(data.downcast_ref::<i32>().is_some());
```

## What `RefAny` does not guarantee

Where the safety story stops:

- **No isolation between callbacks.** Two callbacks holding clones of
  the same `RefAny` see each other's mutations. This is the point of
  `RefAny` — backreferences depend on it — but it means a buggy
  callback can corrupt data another callback relies on.
- **Foreign-language type IDs are trust-on-first-use.** Bindings to C,
  Python, or C++ supply their own `type_id`. The framework treats the
  ID as opaque and only compares for equality. Two distinct types
  registered with the same numeric ID are indistinguishable to
  `downcast_*`. The Rust builder (`RefAny::new`) uses
  `TypeId::of::<T>()` and is collision-free; foreign callers must
  enforce uniqueness themselves.
- **No protection against `unsafe` in callbacks.** Callbacks are plain
  functions. Once one downcasts to `&mut T`, it can do anything `T`
  permits — including raw pointer dereference, transmute, or FFI calls
  that escape the framework entirely.
- **Custom destructor is trusted.** `new_c` accepts an arbitrary
  `extern "C" fn(*mut c_void)`. A wrong destructor will leak, double-
  free, or read freed memory. Rust callers get a correct one for free
  via `RefAny::new`; FFI callers must supply one that matches the type.
- **Send/Sync claims are upheld by callers.** `RefAny` is unconditionally
  `Send + Sync`. The runtime borrow checker stops data races on the
  inner value, but if the foreign caller stores a pointer that
  `RefAny` was never told about, that pointer escapes the model.

The boundary is: *if you stay in safe Rust on the consumer side and
construct `RefAny` only via `RefAny::new`, the contract holds*. Once
you call `new_c` directly, the soundness obligations move to you.

## C-ABI callback trampolines

Callback function pointers cross the FFI as `extern "C"` (`core/src/callbacks.rs:113`):

```rust,ignore
pub type LayoutCallbackType = extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom;
pub type CallbackType       = extern "C" fn(RefAny, CallbackInfo) -> Update;
```

What crosses the boundary at every call:

- A *cloned* `RefAny` (refcount bump, no allocation copy).
- A `&LayoutCallbackInfo` / `&CallbackInfo` whose lifetime is the call
  itself.
- A return value that is one of a fixed set of `#[repr(C)]` enums.

What does *not* cross: Rust closures (no stable layout), generics
(monomorphized away), trait objects (no stable vtable layout), or any
type that isn't `#[repr(C)]`. The codegen pipeline rejects bindings
that try to expose those.

The layout callback is the only entry point that produces a `Dom`. The
framework calls it; user code never calls it directly. This means:

- Panicking across `extern "C"` is undefined behavior unless the
  function is declared `extern "C-unwind"`. Rust callbacks should
  return an empty `Dom` or set a flag rather than panic; foreign-
  language callers must catch their own exceptions before returning.
- The returned `Dom` is taken by value (move-out). The framework owns
  it after the call returns; the callback retains nothing.

## Parser and layout robustness

The framework consumes three kinds of untrusted input by default:

- **CSS source** — parsed by `azul-css`. Malformed declarations
  produce errors, not panics. Unknown properties are dropped.
- **HTML/XML source** — parsed by `azul-layout::xml`. Used by the
  reftest harness and the headless renderer. Malformed input produces
  a structured error.
- **Font and image bytes** — handled by `allsorts` (fonts) and the
  image decoders. Hostile font tables can fail decode but not corrupt
  memory; image decoders honor max-dimension caps.

None of these surfaces are hardened to the level of a browser engine.
A targeted DoS through quadratic CSS selector explosion or malformed
glyph tables is in scope for someone shipping Azul as a public-facing
service.

## Web backend — server attack surface

`AZ_BACKEND=web://` exposes a TCP listener bound to whatever address
`parse_web_url` accepted (`dll/src/web/config.rs:17`). The server
(`dll/src/web/server.rs`) accepts HTTP/1.1 connections and serves:

- pre-rendered HTML for registered routes,
- immutable assets under `/az/`,
- `POST /az/exec/{node_id}` which re-renders the root layout.

Hardening you do *not* get out of the box:

- **No TLS.** Front with a reverse proxy.
- **No authentication.** Any connection that reaches the bound port
  sees the full app. Bind to `127.0.0.1` and gate at the proxy.
- **No CSRF protection on `POST /az/exec/`.** A cross-origin site can
  trigger callback execution if the server is reachable from the
  browser. Restrict origins at the proxy.
- **No rate limiting.** The 16 MB body cap (`dll/src/web/server.rs:132`) is the
  only built-in limit. Connection-rate or request-rate limiting is the
  proxy's job.
- **No structured logging or audit trail.** `eprintln!` to stderr is
  the only output.

The body-size cap rejects oversized POSTs with `413` *before* reading
the body. `Content-Length` is parsed case-insensitively and capped;
streaming uploads are not supported.

## Web backend — sandbox model

Phase 0 has no client-side execution surface to sandbox. All callbacks
run server-side, with the same trust as a desktop callback. The
sandbox boundary becomes meaningful in Phase 2+, when `azul-mini.wasm`
ships real framework code:

| When the WASM bundle is loaded | The browser tab can | The server still sees |
|---|---|---|
| Phase 0 (today, 8-byte stub) | Submit POSTs to `/az/exec/` | Every interaction |
| Phase 2 (planned, framework only) | Re-cascade and re-layout client-side | Only callback POSTs |
| Phase 3 (planned, lifted callbacks) | Run pure callbacks locally | POSTs for I/O-bound callbacks only |

The sandbox boundary in Phases 2+ is the WebAssembly memory model. A
lifted callback gets:

- read access to its own `RefAny` (cloned into linear memory),
- imported function pointers into `azul-mini.wasm` for DOM ops,
- nothing else from the host.

It does *not* get: filesystem, network, threads, the wider DOM API,
JavaScript globals, or the server's `app_data`. Anything the callback
needs from the server side has to round-trip via POST. The
`Transpiler` trait (`dll/src/web/transpiler.rs:39`) refuses to lift
functions that need those — they fall back to server-side execution
automatically.

`StubTranspiler::is_available()` returns `false` in Phase 0
(`transpiler.rs:97`). Until that flips, every callback is server-side
and the sandbox boundary is "the network". Treat the desktop and web
threat models as identical for now.

## Where the model breaks

Three failure modes worth flagging explicitly:

1. **Foreign-language `RefAny` collision.** A binding that hashes type
   names to compute `type_id` can collide. The framework will hand out
   a wrong-typed reference without complaint. Use a counter-based ID
   scheme or rely on the codegen-generated wrappers.
2. **Mutex poisoning is ignored on app_data.** The web server's
   `Arc<Mutex<RefAny>>` (`dll/src/web/server.rs:43`) recovers from
   poisoning via `unwrap_or_else(|e| e.into_inner())`. After a
   panicking callback, subsequent requests proceed against possibly
   inconsistent app state instead of failing closed.
3. **No content-security headers.** The served HTML has no CSP, no
   X-Frame-Options, no Strict-Transport-Security. Add these at the
   reverse proxy if the page is reachable from the open web.

For an authoritative threat model, the answer is the same as for any
GUI library written in Rust: trust the process boundary, don't deserialize
untrusted data into `RefAny` without a validated `type_id`, and front
the web target with an HTTP-aware proxy.
