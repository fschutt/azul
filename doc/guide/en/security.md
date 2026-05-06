---
slug: security
title: Security Model
language: en
canonical_slug: security
audience: external
maturity: wip
guide_order: 500
topic_only: false
short_desc: What azul does and doesn't defend against
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

> **WIP.** The desktop trust model is mature: `RefAny`'s FFI guarantees have been stable since the project's first release. The web backend's sandbox model is still phase 0 — server-side execution with no untrusted client code path. The shape of the threat model won't change when WASM lifting lands; the implementation will.

Azul is a library, not an isolation boundary. Application code runs with full process privilege. The framework's job is narrower:

1. Keep the FFI surface (`RefAny`, callbacks, C-ABI types) sound when crossed from any supported binding language.
2. Keep the layout, parser, and rendering paths free from untrusted-input vulnerabilities (panic-free on hostile bytes, no buffer over-reads).
3. Define a sandbox boundary for the web target so callbacks lifted to the browser cannot reach beyond what the server exports.

## Trust boundaries

- **Rust to C, C++, or Python bindings.** Both sides trusted. The mechanism is `#[repr(C)]` types plus runtime checks in `RefAny`.
- **Layout callback to application data.** The callback is trusted, the data is not. Guarded by `RefAny` type-id and borrow check.
- **Desktop process to user input.** The process is trusted, the input bytes are not. Hardened via parser fuzzing. Layout never panics on UTF-8 input.
- **Web server to network client.** The server is trusted, the HTTP request is not. Mechanism is method and path matching, a 16 MB body cap, and no auth.
- **Web server to lifted WASM.** The server is trusted, browser-resident WASM is not. Phase 0: not yet a boundary.

Inside a single desktop process, all callbacks share an address space and can corrupt each other if they break Rust's aliasing rules through `unsafe`. The framework provides no in-process sandbox.

## What RefAny guarantees

Every callback receives a `RefAny` and downcasts it to a typed reference. The framework guards five invariants:

- **Type identity**. The downcast compares a 64-bit type id before any cast. A wrong-type downcast returns nothing; it never produces an undefined-behavior pointer.
- **Allocation alignment**. Heap allocation uses the type's alignment so the pointer is always correctly aligned for the stored type.
- **Reference counting**. Strong, weak, and mutable counts are atomic. Drop-when-last-ref is race-free across threads.
- **Runtime borrow check**. Shared and mutable borrows can't coexist. A second mutable borrow while one is live returns nothing.
- **Type-correct destruction**. The destructor is monomorphized per type at construction time, so the right `Drop` runs even though the pointer crosses FFI as `*mut c_void`.

```rust
use azul_core::refany::RefAny;

let mut data = RefAny::new(42i32);

assert!(data.downcast_ref::<i32>().is_some());
assert!(data.downcast_ref::<u32>().is_none());        // wrong type → None
{
    let _live = data.downcast_mut::<i32>().unwrap();
    assert!(data.downcast_ref::<i32>().is_none());    // already mut-borrowed
}
assert!(data.downcast_ref::<i32>().is_some());
```

## What RefAny does not guarantee

Where the safety story stops:

- **No isolation between callbacks.** Two callbacks holding clones of the same `RefAny` see each other's mutations. This is the point of `RefAny` (backreferences depend on it), but a buggy callback can corrupt data another callback relies on.
- **Foreign-language type IDs are trust-on-first-use.** Bindings to C, Python, or C++ supply their own type id. The framework only compares for equality. Two distinct types registered with the same numeric id are indistinguishable. Rust callers get a collision-free id automatically; foreign callers must enforce uniqueness themselves.
- **No protection against `unsafe` in callbacks.** Once a callback downcasts to `&mut T`, it can do anything `T` permits.
- **Custom destructor is trusted.** A wrong destructor will leak, double-free, or read freed memory. Rust callers get a correct one for free; FFI callers must supply one that matches the type.

The boundary is: if you stay in safe Rust on the consumer side and construct `RefAny` only via `RefAny::new`, the contract holds. Once you construct one manually with a custom destructor, the soundness obligations move to you.

## C-ABI callback trampolines

Callback function pointers cross the FFI as `extern "C"` functions. What crosses the boundary at every call:

- A cloned `RefAny` (refcount bump, no allocation copy).
- A reference to `LayoutCallbackInfo` or `CallbackInfo` whose lifetime is the call itself.
- A return value that is one of a fixed set of C-compatible enums.

What does not cross: Rust closures, generics, trait objects, or any type that isn't `#[repr(C)]`. The codegen pipeline rejects bindings that try to expose those.

The `LayoutCallback` is the only entry point that produces a `Dom`. The framework calls it; user code never calls it directly. This means:

- Panicking across `extern "C"` is undefined behavior unless declared `extern "C-unwind"`. Rust callbacks should return an empty `Dom` or set a flag rather than panic; foreign-language callers must catch their own exceptions before returning.
- The returned `Dom` is taken by value. The framework owns it after the call returns; the callback retains nothing.

## Parser and layout robustness

The framework consumes three kinds of untrusted input by default:

- **CSS source** — malformed declarations produce errors, not panics. Unknown properties are dropped.
- **HTML/XML source** — used by the headless renderer. Malformed input produces a structured error.
- **Font and image bytes** — hostile font tables can fail decode but not corrupt memory; image decoders honor max-dimension caps.

None of these surfaces are hardened to the level of a browser engine. A targeted DoS through quadratic CSS selector explosion or malformed glyph tables is in scope for someone shipping azul as a public-facing service.

## Web backend — server attack surface

`AZ_BACKEND=web://` exposes a TCP listener bound to whatever address you passed. The server accepts HTTP/1.1 connections and serves:

- pre-rendered HTML for registered routes,
- immutable assets under `/az/`,
- `POST /az/exec/{node_id}` which re-renders the root layout.

Hardening you do not get out of the box:

- **No TLS.** Front with a reverse proxy.
- **No authentication.** Any connection that reaches the bound port sees the full app. Bind to `127.0.0.1` and gate at the proxy.
- **No CSRF protection** on `POST /az/exec/`. A cross-origin site can trigger callback execution if the server is reachable from the browser. Restrict origins at the proxy.
- **No rate limiting.** The 16 MB body cap is the only built-in limit. Connection-rate or request-rate limiting is the proxy's job.
- **No structured logging or audit trail.** Standard error output is the only signal.

The body-size cap rejects oversized POSTs with `413` *before* reading the body. `Content-Length` is parsed case-insensitively and capped; streaming uploads are not supported.

## Web backend — sandbox model

Phase 0 has no client-side execution surface to sandbox. All callbacks run server-side, with the same trust as a desktop callback. The sandbox boundary becomes meaningful when framework WASM ships:

- **Phase 0 (today).** The browser tab submits POSTs to `/az/exec/`. The server sees every interaction.
- **Phase 2 (planned).** The browser tab re-cascades and re-lays out client-side. The server sees only callback POSTs.
- **Phase 3 (planned).** The browser tab runs pure callbacks locally. The server sees POSTs only for I/O-bound callbacks.

Once the WASM bundle is loaded, the sandbox boundary is the WebAssembly memory model. A lifted callback gets read access to its own `RefAny`, imported function pointers for DOM ops, and nothing else from the host. It does not get filesystem, network, threads, the wider DOM API, JavaScript globals, or the server's app data. Anything the callback needs from the server side has to round-trip via POST.

Until lifting is enabled, every callback is server-side and the sandbox boundary is "the network". Treat the desktop and web threat models as identical for now.

## Where the model breaks

Three failure modes worth flagging explicitly:

1. **Foreign-language `RefAny` collision.** A binding that hashes type names to compute the type id can collide. The framework will hand out a wrong-typed reference without complaint. Use a counter-based scheme or rely on the codegen-generated wrappers.
2. **Mutex poisoning is ignored on app data.** After a panicking callback, subsequent web requests proceed against possibly inconsistent app state instead of failing closed.
3. **No content-security headers.** The served HTML has no CSP, no X-Frame-Options, no Strict-Transport-Security. Add these at the reverse proxy if the page is reachable from the open web.

For an authoritative threat model: trust the process boundary, don't deserialize untrusted data into `RefAny` without a validated type id, and front the web target with an HTTP-aware proxy.

## Coming Up Next

- [Web Deployment](web-deployment.md) — Building for the browser via WASM
- [Code Generation](code-generation.md) — How `azul-doc` regenerates bindings from `api.json`
- [Headless Rendering](headless-rendering.md) — Running the pipeline without a window
