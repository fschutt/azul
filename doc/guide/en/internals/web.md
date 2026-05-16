---
slug: internals/web
title: Web Backend Internals
language: en
canonical_slug: internals/web
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: WASM target - DOM-attachment and OffscreenCanvas
prerequisites: []
tracked_files:
  - dll/src/web/cb_gen.rs
  - dll/src/web/classify.rs
  - dll/src/web/config.rs
  - dll/src/web/html_render.rs
  - dll/src/web/loader_js.rs
  - dll/src/web/mini_gen.rs
  - dll/src/web/mod.rs
  - dll/src/web/server.rs
  - dll/src/web/transpiler.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:50:16Z
default-search-keys:
  - StyledDom
  - EventFilter
  - Dom
  - Css
  - RefAny
  - LayoutCallback
---

# Web Backend Internals

## Overview

*WIP — Phase 0.* Of the five planned phases (A–E), **A** (classify), **D** (HTML pre-render), and **E** (HTTP server) are functional. **Phase B** (mini.wasm generation) is still a stub at the orchestrator level; the underlying lift toolchain (WB1.1) is wired into `RemillTranspiler`. **Phase C** (per-callback transpile) has the lift→llc→wasm-ld subprocess pipeline landed but is missing the four IR passes (intrinsic lowering, signature rewrite, symbol intercept, opt -O2) that real callbacks require. All callbacks currently execute server-side via `POST /az/exec/{node_id}`.

The web backend turns an Azul application into an HTTP server: setting `AZ_BACKEND=web://0.0.0.0:8080` makes `App::run` dispatch to `run_web` instead of opening a native window. The layout callback runs natively on the server, the resulting `StyledDom` is serialized to HTML plus a per-node `#az_N { … }` stylesheet, and a small bootstrap JavaScript wires up callback dispatch. There is **no** client-side WASM today; that is the long-term goal Phases A–C are converging on.

**The architecture summary.** A working web backend ships *two* tiers of WebAssembly: a single `azul-mini.wasm` containing the framework primitives (~200 `Az*` C functions lifted from the native build) plus one small per-callback wasm module for each registered user callback. The two tiers share a single linear memory imported from `azul-mini.wasm`, so pointers are interchangeable. Each callback module imports the `Az*` symbols it needs (and a small set of host shims like `__az_js_fetch` for browser-native operations). The browser dispatches a click event by `fetch()`-ing the relevant callback module and invoking its exported entry point — no server round-trip, no full page rebuild.

The pipeline that gets us there is a six-step LLVM-IR transformation, not a direct "machine bytes → wasm" lower; see [Lift pipeline architecture](#lift-pipeline-architecture) below.

## Backend selection — web://ip:port

The URL is parsed by `parse_web_url`:

```rust,ignore
pub fn parse_web_url(s: &str) -> Option<SocketAddr>
```

It accepts `web://127.0.0.1:8080`, `web://0.0.0.0:3000`, and `web://[::1]:8080`. The `web://` prefix is case-insensitive; an optional `?query` (e.g. `?tls=cert.pem`) is stripped before `SocketAddr::from_str`. The result is wrapped in `AzBackend::Web(SocketAddr)` and consumed by the desktop runner, which calls `run_web` instead of the native shell.

## run_web — the five-phase orchestrator

```rust,ignore
pub fn run_web(
    app_data: RefAny,
    config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    font_registry: Option<Arc<FcFontRegistry>>,
    root_window: WindowCreateOptions,
    bind_addr: SocketAddr,
) -> Result<(), WindowError>
```

The phases run in order:

- **Phase A.** Functional. Decompresses embedded `api.json` and classifies (`classify_api_functions`). Output is not yet consumed by Phases B/C; that wiring is part of WB1.2.
- **Phase B.** Stub at the orchestrator level (`generate_mini_wasm` returns the 8-byte WASM header). The toolchain it would use — `RemillTranspiler::lift_and_link_framework` — is implemented but missing the IR-level passes the real lift needs (see [Lift pipeline architecture](#lift-pipeline-architecture)).
- **Phase C.** Stub at discovery (`discover_and_transpile_callbacks` returns `Vec::new()`); the per-function `RemillTranspiler::lift_function` is wired. Same gap as Phase B: the lift produces verbose unsafe IR that needs four passes before llc can produce a usable callback module.
- **Phase D.** Functional. `render_initial_page` renders the initial page for each route.
- **Phase E.** Functional. `run_server` starts the HTTP listener.

Phase D walks `config.routes` and calls `render_initial_page` for each. When there are no routes, the root window's layout callback is rendered at `/`. Each `RenderOutput` carries an HTML body, a vector of `CollectedImage`, and a vector of `CollectedFont`. Image and font IDs are *per-render*; the orchestrator rebases them onto a global ID space and rewrites the URLs in the HTML so different routes don't collide.

Phase E hands the merged state to `server::run_server`, which blocks forever serving HTTP.

## Phase A — classify

The classifier decompresses an embedded brotli'd `api.json` (roughly 120 KB compressed, 3.7 MB raw) and bins every C function into one of three categories *today*:

```rust,ignore
pub enum FnClass {
    Framework,             // AzDom_*, AzRefAny_*, ...      → goes into mini.wasm
    ServerEntryPoint,      // AzApp_run                     → never in WASM
    ReplaceWithDomPatcher, // AzDisplayList_*, AzGl_*       → emit setStyle() instead
}
```

Classification rules:

```rust,ignore
fn classify_fn(name: &str) -> FnClass {
    match name {
        "AzApp_run" => FnClass::ServerEntryPoint,
        n if n.starts_with("AzDisplayList_") => FnClass::ReplaceWithDomPatcher,
        n if n.starts_with("AzGl_") => FnClass::ReplaceWithDomPatcher,
        _ => FnClass::Framework,
    }
}
```

The brotli blob is built at codegen time (`target/codegen/api.json.br`, produced by `azul-doc codegen all`). `classify_api_functions` is called from `run_web` for diagnostics only. Phase 0 doesn't act on the classification.

**Planned refinement (WB1.2).** The intercept pass (described below) decides per-symbol whether the implementation lives in lifted wasm, in `azul-mini.wasm`, or as a JS-side host import. The three-variant enum collapses several of these decisions into "Framework" today; WB1.2 splits it into five variants:

```rust,ignore
pub enum FnClass {
    LiftAsIs,                 // pure compute, no env: lift the bytes
    ImportFromAzulMini,       // framework primitive: AzDom_new, AzRefAny_*
    ImportFromJsHost,         // env call: AzHttp_fetch, AzClipboard_*, AzWindow_setTitle
    DomPatcher,               // AzDisplayList_* / AzGl_* → JS-side patch
    ServerOnly,               // AzApp_run, AzFile_open (native paths) — error if reached from a callback
}
```

The split matters because each variant maps to a different *implementation* strategy in `azul-mini.wasm`:

- `LiftAsIs` and `ImportFromAzulMini` are the two cases the intercept pass actually emits — `LiftAsIs` for the callback's own bytes, `ImportFromAzulMini` for any framework call the callback makes.
- `ImportFromJsHost` is the escape valve: `AzHttp_fetch` doesn't have a sensible lift (it calls `reqwest`/`ureq` natively); in the browser it routes to `fetch()` via a JS shim. The shim's signature still goes through `azul-mini.wasm`, so callbacks only ever import `Az*` symbols — the host-shim detail is hidden behind the mini boundary.
- `DomPatcher` covers GL/display-list operations that have no wasm-target meaning (a native GL context handle is not transferrable). These get replaced at intercept time with calls to a JS-side patcher.
- `ServerOnly` is a correctness check: a lifted callback that calls `AzApp_run` is a bug; the intercept pass refuses to emit and falls back to server dispatch.

`classify_api_functions` becomes the source of truth for the intercept pass — given a symbol, return the variant, drive the rewrite.

## Phase B — mini_gen

`generate_mini_wasm` returns the smallest valid WASM module:

```rust,ignore
const WASM_HEADER: [u8; 8] = [
    0x00, 0x61, 0x73, 0x6D,  // \0asm magic
    0x01, 0x00, 0x00, 0x00,  // version 1
];
```

Browsers will load and parse this 8-byte module without complaint, so the `<link rel="preload" href="/az/mini.{hash}.wasm">` hint in the generated HTML resolves rather than 404'ing. The eventual implementation will lift ~200 framework C functions from the running binary through `Transpiler::lift_and_link_framework`.

**Current state.** WB1.1 (commit `18706f526`) wired `RemillTranspiler::lift_and_link_framework` into a real subprocess pipeline that produces *valid but bloated* WASM (no IR-level optimisation between lift and llc, so the remill `%struct.State` register-file isn't evaporated). The orchestrator's `generate_mini_wasm` still returns the 8-byte stub; switching it to the real path requires WB1.2 (iterate `FnClass::Framework` from Phase A's classifier) plus the IR passes described in [Lift pipeline architecture](#lift-pipeline-architecture). Without those passes, ~200 framework functions would lift to tens of megabytes of wasm and refuse to load; with them, expected output is ~500 KB – 2 MB.

## Phase C — transpiler and cb_gen

The transpiler trait is:

```rust,ignore
pub trait Transpiler {
    fn lift_function(&self, fn_name: &str, fn_addr: usize, fn_size: usize)
        -> Result<WasmModule, TranspileError>;
    fn lift_and_link_framework(&self, functions: &[(String, usize, usize)])
        -> Result<WasmModule, TranspileError>;
    fn is_available(&self) -> bool;
    fn name(&self) -> &str;
}
```

Two implementations today: `StubTranspiler` (always returns `TranspileError`, used when the `web-transpiler` feature is off) and `RemillTranspiler` (real subprocess pipeline; opt-in via `web-transpiler`). The naive end-to-end pipeline `RemillTranspiler` runs is:

```text
running native binary
  ─ dladdr / DWARF ─►  (fn_name, fn_addr, fn_size)
  ─ remill-lift-17 ─►  LLVM IR (with %struct.State + __remill_* intrinsics)
  ─ llc -mtriple=wasm32 ─► WASM object (LARGE — State struct not evaporated)
  ─ wasm-ld --no-entry --export=sub_<addr> ─► WASM module
```

This pipeline works end-to-end for a leaf function (validated via `experiments/transpile-blueprint`: an aarch64 `add w0,w0,w1; ret` produces a valid 230-byte module with `\0asm` magic). It does **not** work for real callbacks, which call into framework functions and reference memory the lift naively models as opaque pointers into a State struct. The missing pieces are described in the next section.

`cb_gen` is the consumer. It would walk `config.routes`, collect every callback function pointer in the resulting DOM, resolve each pointer to a symbol via `dladdr`, and feed them into `Transpiler::lift_function`. Today it returns `Vec::new()`, which means the HTML emitter has no `<link rel="preload" href="/az/cb/*.wasm">` hints to add and the server's `/az/cb/{name}.wasm` route always 404s.

## Lift pipeline architecture

The naive `lift → llc → wasm-ld` pipeline that ships in WB1.1 produces *correct* wasm for leaf functions but degrades pathologically for anything that calls into the framework or touches memory. The reason is that remill lifts at the abstraction level of *a CPU executing instructions against a register file in memory* — each guest register is a field of a `%struct.State` value the lifted function reads from and writes to, and every memory access is threaded through an opaque `%memory` token. Without intermediate IR passes, llc sees that verbose register-machine code and lowers it literally, producing dozens of KB of wasm per source instruction.

The real pipeline is six stages, not three:

```text
running native binary
  ─ dladdr / DWARF ─►  (fn_name, fn_addr, fn_size)
  ─ remill-lift-17 ─►  IR with %struct.State + __remill_* intrinsics
  ─ intrinsic-lower pass ─►  __remill_function_return / __remill_read_memory_* etc. given concrete bodies
  ─ signature-rewrite pass ─►  define ptr @sub_X(state, pc, memory) → define <ABI> @<name>(<args>)
  ─ symbol-intercept pass ─►  __remill_function_call → typed extern (classify-driven)
  ─ opt -O2 ─►  SROA + mem2reg evaporate the State struct; clean dataflow IR
  ─ llc -mtriple=wasm32 -O2 ─►  WASM object
  ─ wasm-ld --import-module=azul-mini --import-memory ─►  WASM module with Az* imports
```

The four middle passes are the work WB1.2 / WB1.3 will land. Each is independent and can be tested in isolation against `experiments/transpile-blueprint`.

### The State struct evaporates — once `opt -O2` runs

The whole architecture rests on one LLVM property: the State pointer is `noalias`, every field access is a constant-offset GEP, and nothing outside the lifted function takes the State's address. Combined, these let LLVM's alias analysis prove that all State accesses are independent locations. **SROA** (Scalar Replacement of Aggregates) then splits the struct into individual scalar slots; **mem2reg** promotes those slots to SSA values. For a lifted `add w0, w0, w1; ret`, the optimisation flattens 80+ lines of register-file shuffling into:

```llvm
define i32 @add(i32 %a, i32 %b) {
  %r = add i32 %a, %b
  ret i32 %r
}
```

The State struct, the GEPs, the PC bookkeeping, the `__remill_function_return` indirection — all gone. What remains is the pure dataflow the native instructions computed. This is why remill chose its register-machine representation: verbose to emit, but mechanically optimisable into something native-looking that lowers cleanly to any LLVM target. The pipeline never explicitly translates "register memory model → linear memory model" — the register model gets optimised *out of existence* before codegen sees it.

`opt -O2` cannot do this work *until* the four preceding passes have run, because the `__remill_*` intrinsics are declared-but-undefined externs that LLVM treats as side-effecting. They must be lowered to concrete IR first; otherwise SROA refuses to touch the State struct on the assumption that the opaque calls might alias.

### The intrinsic-lowering pass

remill emits a fixed set of opaque intrinsics that LLVM cannot reason about:

| Intrinsic | Lowered body |
|---|---|
| `__remill_function_return(state, pc, memory)` | Read the return register from State (X0 for aarch64, RAX for x86-64), return it directly. |
| `__remill_read_memory_N(memory, addr)` | `load iN, ptr %addr` (the memory token is dropped). |
| `__remill_write_memory_N(memory, addr, val)` | `store iN %val, ptr %addr` (memory token returned unchanged). |
| `__remill_function_call(state, target_pc, memory)` | *Handled by the intercept pass below* — replaced with a typed extern call. |
| `__remill_jump`, `__remill_missing_block`, `__remill_error` | Lowered to `unreachable` / panics; should never fire for traces lifted from well-formed code. |

The pass is a one-time IR walk per module. Once intrinsics have concrete bodies, the lifted function is no longer opaque to LLVM and downstream optimisation can proceed.

### The signature-rewrite pass

remill produces every lifted function with the same uniform signature:

```llvm
define ptr @sub_100000000(ptr noalias %state, i64 %program_counter, ptr noalias %memory)
```

The return type is `ptr` because the body ends in `tail call ptr @__remill_function_return(...)`. That's wrong for the wasm boundary — a caller wants `define i32 @on_click(i32 %data_ptr, i32 %event_ptr)` matching the source-level Rust ABI, not a State pointer.

The pass:

1. Looks up the symbol's source signature in `api.json` (Azul has typed metadata for every `Az*` export).
2. Generates a thin wrapper that allocates the State struct on the stack, writes the input args into the ABI's argument registers (X0/X1/... or RDI/RSI/... depending on host arch), calls the original `sub_<addr>` body, reads the return register out, and returns it as the source-level type.
3. The wrapper is `internal`-visibility; the original `sub_<addr>` becomes a leaf the wrapper inlines into. After `opt -O2`, the wrapper is the only callable export and the State struct never escapes (no separate alloca survives optimisation).

This pass is one of two places that needs per-host-arch logic (the other is the intercept pass's argument extraction). aarch64-host and x86-64-host lowering are the only two we need to support today; both are fully specified by the AArch64 PCS and the System V AMD64 ABI respectively.

### The symbol-intercept pass — where the real architectural insight lives

The byte map handed to remill should contain *only the callback's own bytes*, not the framework's. Remill then lifts every framework call as an unresolved `__remill_function_call(state, constant_target_pc, memory)` with the call target baked in as an `i64` constant.

The intercept pass:

```rust,ignore
for call in module.calls_to("__remill_function_call") {
    let target_addr = call.constant_arg("target_pc");
    let sym = dladdr(target_addr)?;  // address → "AzCallbackInfo_setCssProperty"

    match classify_api_functions().get(&sym) {
        FnClass::ImportFromAzulMini => {
            // Look up source signature in api.json, extract args from State
            // following the host ABI, emit typed extern declaration + call,
            // write result back to State.
            let extern_fn = module.get_or_declare_extern(&sym, signature_from_api_json(&sym));
            let args = extract_args_from_state(&call, host_arch_abi());
            let result = builder.call(extern_fn, args);
            store_result_to_state(&call, result);
            call.erase();
        }
        FnClass::ImportFromJsHost => { /* same shape; target is host shim like __az_js_fetch */ }
        FnClass::DomPatcher => { /* same shape; target is __az_dom_patch */ }
        FnClass::ServerOnly => return Err("callback called server-only function — fall back to /az/exec"),
        FnClass::LiftAsIs => unreachable!("LiftAsIs is for the callback itself, not its callees"),
    }
}
```

After this pass runs, the IR has clean direct calls to `@AzCallbackInfo_setCssProperty` (and `@__az_js_fetch`, etc.); the `__remill_function_call` indirection is gone. `wasm-ld --import-module=azul-mini` then marks those externs as imports the linker doesn't need to resolve locally — the browser-side wasm loader satisfies them at instantiation time from `azul-mini.wasm`'s exports.

The same machinery handles framework primitives (`AzDom_new`), browser-native operations (`AzHttp_fetch` → `fetch()` via a JS shim hosted in `azul-mini.wasm`), and DOM-patch hooks. The only thing that changes per-symbol is which `FnClass` variant `classify.rs` returns — the rewrite shape is uniform.

### Shared linear memory

Each callback wasm and `azul-mini.wasm` must operate on the same `RefAny`, the same DOM arena, the same StyledDom property cache. Pointers in one module's linear memory are not pointers in another module's *unless they share the underlying buffer*. The wasm dynamic-linking convention is:

```wat
;; azul-mini.wasm
(memory (export "memory") 16)   ;; 16 pages = 1 MiB initial

;; each per-callback wasm
(import "azul-mini" "memory" (memory 1))
```

`wasm-ld` flag for the callback side is `--import-memory --shared`. The JS-side `WebAssembly.instantiate` calls pass the same `WebAssembly.Memory` instance to every module, so `i32.load`/`i32.store` against any address see the same bytes. This is the load-bearing piece that lets a callback's compiled-from-Rust `data.users.insert(k, v)` mutate state that `azul-mini.wasm`'s `AzDom_new` later reads.

### Fallback to server-side dispatch

Not every callback will lift cleanly. Anticipated failure modes:

| Mode | Cause | Mitigation |
|---|---|---|
| **Excessive size** | Callback inlines std collections (`HashMap::insert` → ~100 KB wasm per call site). | Per-callback dry-run during build classifies as Liftable / TooBig / FallbackToServer; emit `<link rel="preload">` only for Liftable. |
| **Indirect calls** | `dyn Trait` dispatch lands at a runtime address with no symbol. | Either ship every possible target (closed-world) or fall back to server. |
| **`malloc`/`errno` references** | Callback inlined libc internals. | Stub in `azul-mini.wasm` with wasm-friendly equivalents (`wee_alloc` / `dlmalloc`-WASM); ignore `errno`. |
| **Panic paths** | `core::panicking::panic_fmt` chain lifts to megabytes. | Compile callbacks with `panic = "abort"`; intercept-pass replaces the panic call chain with `unreachable`. |

The existing `POST /az/exec/{node_id}` server-side path is the safety net for everything that doesn't lift. The architectural commitment is "try-lift → fall back" per-callback, not "lift everything or nothing."

### Build-time vs. runtime lifting

Build-time lifting (DWARF + symbol table available, no fork/exec at runtime) is the common case. `discover_and_transpile_callbacks` is the natural home: walk the registered callback function pointers, build an address→symbol map once from the binary's `.symtab`, lift each callback in parallel. Output goes into `target/` as `cb_<symbol_hash>.wasm` and gets served from the build artifact.

Runtime lifting is only needed for callbacks created from JIT'd closures with no static symbol — vanishingly rare in Azul's model (closures monomorphise at compile time). When it's needed, `dladdr` provides the same `(fn_name, fn_addr, fn_size)` triple and the same pipeline runs.

### Architecture verdict

The plan as drafted (Phases A–E + WB1.1) holds up; the work to make Phases B and C real is *concentrated in the four IR passes*, not in additional pipeline stages. Subprocess invocations to `remill-lift-17`, `llc`, and `wasm-ld` are already wired. The next-session work is:

1. **WB1.2** — intrinsic lowering + signature rewrite passes (write as a Rust LLVM pass via `llvm-sys` or stage as a separate IR transformation tool; mcsema's `remill-opt` is a reference).
2. **WB1.3** — intercept pass driven by the refined `classify.rs` enum. Surface the resulting extern declarations as `imports_from_mini` (currently `Vec::new()` in `RemillTranspiler`).
3. **WB1.4** — wire `opt -O2` into the subprocess pipeline between the passes and `llc`.
4. **WB1.5** — switch `generate_mini_wasm` from the 8-byte stub to `lift_and_link_framework(classify_api_functions()::Framework)`.
5. **WB2.x** — same for `discover_and_transpile_callbacks` (per-callback discovery + lift + emit `<link rel="preload">`).

The "stream into the browser as needed" piece is already structurally correct: per-callback wasm modules with content-hashed URLs (`/az/cb/{name}.{hash}.wasm`, `Cache-Control: immutable, max-age=1y`), `<link rel="preload">` hints emitted by `html_render`, `WebAssembly.instantiateStreaming` on the browser side. Only the *contents* of those modules are stubbed today.

## Phase D — html_render

`render_initial_page` produces a full HTML document. The pipeline:

1. **Run the layout callback** with a `LayoutCallbackInfo` constructed from the same `RefAny` and `FullWindowState` the desktop backend uses. `image_cache` and `gl_context` are empty — no GPU on the server. Active route info is threaded through `LayoutCallbackInfoRefData` so route-aware layout callbacks see the matched pattern.

2. **Run the cascade**: `StyledDom::create_from_dom(dom)` resolves all conditional CSS (OS, theme, viewport, container queries, language) on the server, leaving only interactive pseudo-states (`:hover`, `:focus`, `:active`, `:focus-within`, etc.) to the browser. By the time HTML is emitted, every node has a fully-resolved `computed_values[node]` entry in the property cache.

3. **Walk the StyledDom flat arena** depth-first via `RenderContext::render_node_recursive`. Each node:
   - Gets a synthetic `id="az_N"` where `N` is a per-render counter.
   - Emits `<{tag} id="az_N" class="..." data-az-cb="N" ...>` — `data-az-cb` is present iff the node has callbacks. `data-az-ev` records the JS event name (e.g. `click`, `mousedown`) derived from the first callback's `EventFilter`.
   - Image nodes encode the bitmap to PNG via `azul_layout::image::encode_png`, push a `CollectedImage`, and rewrite the `src` to `/az/img/{id}`.
   - The `id` and `class` attributes from the DOM are preserved as `data-az-id` and `class=`, since `id="az_N"` is reserved for the synthetic node ID.

4. **Emit CSS rules**: `emit_css_from_cache` produces:
   - `#az_N { property: value; … }` for the base computed values.
   - `#az_N:hover { … }` / `:focus` / `:active` / etc. for properties that the property cache marks as state-dependent. The `pseudo_state_to_css` helper maps `PseudoStateType::Dragging` to `:active` because the browser has no "dragging" pseudo-class.

5. **Bundle fonts** as `@font-face` rules pointing at `/az/font/{id}`, then concatenate everything into a single `<style>` block.

6. **Inject the loader JS** via `loader_js::generate_loader_js("stub", &cb_wasms)`.

`RenderOutput` carries the assembled HTML plus the collected image and font vectors that the server will serve under `/az/img/` and `/az/font/`.

### Per-route ID rebasing

The orchestrator rewrites image and font URLs after a route renders so that route 0's `/az/img/3` becomes route 1's `/az/img/8` (or whatever the offset is). The simple `.replace(&old, &new)` is safe because the URLs include a leading `/` and unambiguous numeric suffix.

## Phase E — server

```rust,ignore
pub fn run_server(bind_addr: SocketAddr, state: WebServerState)
    -> Result<(), String>
```

A `TcpListener` accept loop spawning a `std::thread` per connection. Zero external dependencies. The request line and headers are parsed inline via `BufReader::read_line`. The 16 MB body cap is the only DoS guard.

### Routes

- `GET /az/loader.js` returns the bootstrap JS string. Immutable cache OK, not cached today.
- `GET /az/mini.{hash}.wasm` returns `state.mini_wasm`, an 8-byte stub. Cached, immutable.
- `GET /az/cb/{name}.{hash}.wasm` returns per-callback WASM. Always 404 in Phase 0.
- `GET /az/img/{id}` returns the encoded image. Cached, immutable.
- `GET /az/font/{id}` returns font bytes. Cached, immutable.
- `POST /az/exec/{node_id}` runs server-side callback dispatch in Phase 0.
- `GET /favicon.ico` returns 204 No Content.
- `GET /<route-pattern>` returns pre-rendered HTML for the matching route.

Image, font, and WASM responses include `Cache-Control: public, max-age=31536000, immutable` because their URLs embed a content hash. HTML responses are not cached.

### POST /az/exec/{node_id} — Phase 0 callback dispatch

The current implementation is a placeholder:

```rust,ignore
("POST", p) if p.starts_with("/az/exec/") => {
    let _node_id_str = p.strip_prefix("/az/exec/").unwrap_or("0");
    if content_length > 0 {
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body).map_err(...)?;
    }
    let html = re_render_body(state);
    send_response(&mut stream, 200, "text/html; charset=utf-8", html.as_bytes())
}
```

The node ID is parsed but unused. The body is read but discarded. `re_render_body` re-runs the layout callback with the current `app_data` and returns the entire new HTML page. The browser replaces its document with the response. **No actual callback runs**. Every POST behaves like a forced re-layout. The intended dispatch path — parse `node_id`, look up the registered callback, invoke it with the deserialized `CallbackInfo`, feed the resulting `Update` back through the layout system — is unimplemented.

### Route matching

The three-stage fallback for `GET /<path>` is:

1. Direct lookup in `state.rendered_routes` keyed by the literal path.
2. Loop through registered routes and call `azul_core::resources::match_route(&pattern, path)` — for parameterized patterns like `/users/{id}` this finds a template, but Phase 0 serves the un-parameterized template HTML rather than re-rendering with the captured params.
3. Fall back to the `/` route, then to any registered route, then to `404 No routes configured`.

### WebServerState

```rust,ignore
pub struct WebServerState {
    pub app_data: Arc<Mutex<RefAny>>,
    pub config: AppConfig,
    pub fc_cache: Arc<FcFontCache>,
    pub font_registry: Option<Arc<FcFontRegistry>>,
    pub window_state: FullWindowState,
    pub mini_wasm: Vec<u8>,
    pub cb_wasms: Vec<CallbackWasm>,
    pub layout_callback: LayoutCallback,
    pub rendered_routes: HashMap<String, RenderedRoute>,
    pub images: Vec<CollectedImage>,
    pub fonts: Vec<CollectedFont>,
}
```

`Arc<Mutex<RefAny>>` is the only synchronization point. `re_render_body` locks it, calls into `render_initial_page`, and drops the lock. Concurrent requests serialize through this mutex. There is no per-connection state.

## loader_js — bootstrap script

`generate_loader_js` returns a fixed JavaScript string. Three things happen on `DOMContentLoaded`:

1. **Callback wiring**: every element with `data-az-cb` gets an event listener bound to its `data-az-ev` event type. The listener POSTs to `/az/exec/{cb-id}` with `{x, y, button, key}` JSON and replaces the document with the response via `document.open() / document.write() / document.close()`.

2. **Link interception**: every `<a href="/...">` (excluding `/az/`) becomes an SPA navigation. `azNavigate(path)` does `fetch(path) → document.open/write/close + history.pushState`.

3. **`popstate` handler**: browser back/forward triggers a `fetch(location.pathname)` and the same document-replacement dance.

`document.write` after `document.open` is a documented anti-pattern because it tears down the script that called it. Phase 0 gets away with it because each response is a complete page. For incremental client-side updates the eventual replacement is `morphdom`-style diffing or `documentElement.innerHTML = …`.

The `_mini_wasm_hash` and `_callbacks` parameters are accepted but ignored in the Phase 0 generator. The `<link rel="preload">` hints are emitted by `html_render`, not by `loader_js`.

### loader_js_hash

FNV-1a 64-bit hash of the loader JS, used for cache-busting URLs. Mirrors `html_render::content_hash` exactly (same constants: `0xcbf29ce484222325` offset basis, `0x100000001b3` prime). Both could be unified into a `util::content_hash` if the duplication grows.

## Asset URL summary

```text
GET /                         → pre-rendered HTML
GET /az/loader.js             → bootstrap (always served from /az/loader.js)
GET /az/mini.{hash}.wasm      → framework wasm (8 bytes today)
GET /az/cb/{name}.{hash}.wasm → callback wasm (404 today)
GET /az/img/{id}              → image bytes
GET /az/font/{id}             → font bytes
POST /az/exec/{node_id}       → callback dispatch → returns full HTML
```

The `/az/` prefix is the only reserved namespace. Any other path is matched against registered routes.

## Cross-references

- [DOM Internals](dom.md) — the `Dom` / `NodeData` / `NodeType` model the renderer walks.
- [Styling — Cascade](styling/cascade.md) — the `StyledDom` and property cache the renderer reads.
- [Events](events.md) — the `EventFilter` enum mapped to JS event names.

## Coming Up Next

- [Rendering](rendering.md) — From `StyledDom` to pixels
- [Rendering — WebRender Bridge](rendering/webrender-bridge.md) — How azul talks to WebRender
- [FFI Codegen](build-and-codegen.md) — How `cargo build` cascades and the codegen pass
