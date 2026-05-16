---
slug: internals/web
title: Web Backend Internals
language: en
canonical_slug: internals/web
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: WASM target — server-side render, lift, dispatch
prerequisites: []
tracked_files:
  - dll/src/web/classify.rs
  - dll/src/web/config.rs
  - dll/src/web/eventloop.rs
  - dll/src/web/headless.rs
  - dll/src/web/html_render.rs
  - dll/src/web/hydration.rs
  - dll/src/web/loader_js.rs
  - dll/src/web/mod.rs
  - dll/src/web/server.rs
  - dll/src/web/transpiler.rs
  - dll/src/web/transpiler_remill.rs
last_generated_rev: 38ff46cf3a85513e90205c82e4613e2a22173e3b
generated_at: 2026-05-16T20:50:00Z
default-search-keys:
  - StyledDom
  - RefAny
  - LayoutCallback
  - AzStartup_hydrate
  - AzStartup_dispatchEvent
  - LayoutCallback
---

# Web Backend Internals

## Status (as of M8.7c, 2026-05-16)

The web backend executes the hello-world `on_click` callback as
remill-lifted WebAssembly: the click increments a counter that
lives in shared linear memory, and the new value is read back +
applied to the DOM by JS. The lift pipeline, the recursive
transitive lift, the shared-memory protocol, the hydration of
`AzRefAny` from the server-embedded JSON snapshot, and the cb
invocation itself all work end-to-end on macOS arm64.

The dispatch path that *would* run the wasm-side hit-test and emit
TLV patches via `AzStartup_dispatchEvent` is built and lifted but
not yet wired in `loader.js`; the current bootstrap calls the cb
directly and applies a hardcoded `textContent =` update on the
counter node. See [What's bypassed](#whats-bypassed) below and
[`scripts/HACKS_REVIEW_2026_05_16.md`](../../../../scripts/HACKS_REVIEW_2026_05_16.md)
for the full catalog of remaining work + the
[`scripts/M8.8_NEW_SESSION_PROMPT.md`](../../../../scripts/M8.8_NEW_SESSION_PROMPT.md)
fix-order document.

Backup: tag `m8.7c-victory` + branch
`backup/m8.7c-victory-2026-05-16` both point at the
known-good commit `7530483aa`.

## Three-phase architecture

The whole web backend runs in three temporal phases:

- **Phase A — server startup.** Once per `run_web` invocation,
  before any HTTP is served. Validates the user's `RefAny` has a
  registered JSON serializer, classifies api.json, lifts
  `azul-mini.wasm`, pre-renders every route's HTML, lifts every
  discovered callback's wasm + every layout callback's wasm,
  builds `WebServerState`, starts the HTTP listener.
- **Phase B — browser bootstrap.** Once per page load. JS fetches
  + instantiates the mini wasm, runs `AzStartup_init` +
  `AzStartup_hydrate`, fetches + instantiates every per-callback
  wasm, wires event listeners.
- **Phase C — user interaction.** Per user input. JS resolves the
  event target to a node id, invokes the cb wasm with the
  hydrated `azRefAnyPtr`, reads back the mutated user data,
  applies a DOM update.

The rest of this document walks each phase in order, citing the
real symbols in the code. Where the implementation has a known
hack, the section is annotated with a forward reference to the
HACKS_REVIEW item that tracks it.

## Backend selection — web://ip:port

`parse_web_url` accepts the same URL forms as before
(`web://127.0.0.1:8080`, `web://0.0.0.0:3000`,
`web://[::1]:8080`). The `web://` prefix is case-insensitive; an
optional `?query` (e.g. `?tls=cert.pem`) is stripped before
`SocketAddr::from_str`. The result is wrapped in
`AzBackend::Web(WebConfig)` and consumed by the desktop runner,
which dispatches to `run_web` instead of opening a native window.

## Phase A — server startup

```rust,ignore
pub fn run_web(
    app_data: RefAny,
    config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    font_registry: Option<Arc<FcFontRegistry>>,
    root_window: WindowCreateOptions,
    web_config: config::WebConfig,
) -> Result<(), WindowError>
```

The orchestrator runs the following steps:

### 1. RefAny serializer validation (`headless.rs::HeadlessApp::validate`)

Calls `azul_layout::json::refany_serialize_to_json(&app_data)`. If
the result is `OptionJson::None`, the user forgot to register a
`_toJson` fn via `AZ_REFLECT_JSON`; the backend prints a fatal
error and returns `WindowError::PlatformError` before any HTTP
traffic is served. The hydration payload depends on this
serializer, so failing fast keeps misconfigured apps from
silently rendering a broken page.

### 2. api.json classification (`classify.rs::classify_api_functions`)

Decompresses the brotli-compressed embedded api.json (~120 KB
compressed, ~3.7 MB raw, built by `azul-doc codegen all` into
`target/codegen/api.json.br`). Walks
`{version}/api/{module}/classes/{cls}/{constructors|functions}/{fn}`
and synthesizes `Az{Cls}_{camelCase(fn)}` symbol names matching
the `cabi_export` symbol table. Each fn gets one of:

```rust,ignore
pub enum FnClass {
    Framework,             // most Az* fns
    ServerEntryPoint,      // AzApp_run
    ReplaceWithDomPatcher, // AzDisplayList_*, AzGl_*
}
```

Result is 2532 functions. **Currently built but not yet
consumed** outside the startup log line — the per-cb transitive
lift uses dladdr on actual bl targets rather than driving off the
classification. The "pre-compile every api.json function at
startup" architecture from the M8.7 plan is still future work.

### 3. azul-mini.wasm lift (`transpiler_remill.rs::lift_and_link_eventloop`)

Iterates `EVENTLOOP_SYMBOLS`:

```rust,ignore
pub const EVENTLOOP_SYMBOLS: &[&str] = &[
    "AzStartup_alloc",
    "AzStartup_free",
    "AzStartup_init",
    "AzStartup_hydrate",
    "AzStartup_dispatchEvent",
    "AzStartup_registerStateDeserializer",
];
```

For each symbol:

1. `dlsym_self(name)` → host address.
2. `resolve_fn_ptr(addr)` →
   `FnPtrSymbol { name, addr, size: LIFT_READ_WINDOW }`
   (the size is a flat 4 KiB read window per
   [hack #4](#whats-bypassed)).
3. `produce_object_for(...)` runs the lift pipeline (next
   section).
4. `wasm-ld --no-entry --import-table --initial-memory=2097152`
   over all six `.o` files → `azul-mini.wasm` (~5 KB) with
   `memory` exported.

Lift order matters: `lift_addr = native_addr` for each fn so that
when (e.g.) `AzStartup_hydrate` does `bl AzStartup_alloc`, the
lifted IR's `call sub_<alloc_native_addr>` matches the body
emitted by alloc's lift at the same name. The synthetic
`0x100000000 + i*0x1000` lift-addr scheme used in earlier drafts
caused cross-fn calls to fall through to noop stubs.

### 4. Per-route HTML pre-render (`html_render.rs::render_initial_page`)

For each route (or the root window's layout cb if no routes
configured):

1. Call the layout cb **natively** with a `LayoutCallbackInfo`
   constructed from the same `RefAny` + `FullWindowState` the
   desktop backend uses. `image_cache` and `gl_context` are
   empty.
2. Run `StyledDom::create_from_dom(dom)` — Azul's full CSS
   cascade resolves OS / theme / viewport / container / language
   queries on the server. Only interactive pseudo-states
   (`:hover`, `:focus`, `:active`, `:focus-within`) survive as
   browser-side CSS.
3. Walk the StyledDom flat arena. Each node gets a synthetic
   `id="az_N"` and, if a callback is bound,
   `data-az-cb="N" data-az-ev="click"
   data-az-wasm="/az/cb/<sym>.<hash>.wasm"`.
4. Emit `<link rel="preload" as="fetch" crossorigin>` hints for
   `/az/mini.<hash>.wasm` + each cb's wasm + each layout cb's
   wasm.
5. **Embed the hydration payload** as
   `<script id="az-hydrate" type="application/json">
   {"type_id":"<decimal_u64>","json":<user_toJson_output>}
   </script>`
   where `type_id` is `app_data.get_type_id()` and `json` is the
   output of the user's registered `_toJson`. For hello-world
   that's `{"type_id":"4298653512","json":5}`.
6. Concatenate the resulting body HTML, the bundled stylesheet
   (cascade-resolved `#az_N { … }` rules), and the inline loader
   JS (`loader_js::generate_loader_js`).

### 5. Per-cb wasm lift (`mod.rs::discover_and_transpile_callbacks`)

For each unique callback fn-address discovered in the route walk:

1. `resolve_fn_ptr` → name (user-binary or libazul).
2. `transpiler.lift_function(name, addr, size)` →
   `lift_with_transitive_deps(roots=vec![...])`.

The recursive transitive lift is the centerpiece (see
[Lift pipeline](#lift-pipeline) below). For hello-world's
`on_click`, the closure includes:

```
on_click                              (user)
MyDataModelRefMut_create              (user, AZ_REFLECT macro)
MyDataModel_downcastMut               (user, AZ_REFLECT macro)
MyDataModelRefMut_delete              (user, AZ_REFLECT macro)
AzRefAny_isType                       (libazul, via PLT stub)
AzRefCount_canBeSharedMut             (libazul, via PLT stub)
AzRefCount_increaseRefmut             (libazul, via PLT stub)
AzRefAny_getDataPtr                   (libazul, via PLT stub)
AzRefCount_decreaseRefmut             (libazul, via PLT stub)
AzRefAny_delete                       (libazul, via PLT stub)
AzRefCount_clone                      (libazul, via PLT stub)
RefAny::get_type_id                   (mangled azul_core internal)
RefCount::can_be_shared_mut           (mangled azul_core internal)
RefAny::get_data_ptr                  (mangled azul_core internal)
```

11–14 functions, all linked into a single ~14 KB `.wasm` that
imports only `env.memory` + `env.__indirect_function_table` +
the JS Proxy fallback for unresolved `sub_<hex>` and remill helpers.

### 6. Per-layout-cb wasm lift (`mod.rs::lift_layout_callbacks`)

Same pipeline applied to each unique `LayoutCallback.cb`
referenced by the configured routes. The closure for hello-world
is ~42 functions (DOM construction, AzString, AzCssProperty,
AzButton, ...). The bytes are served at
`/az/layout/<name>.<hash>.wasm` but **the current loader does not
instantiate them** — they wait on the [diff-and-patch](#whats-bypassed)
work.

### 7. Start HTTP listener (`server.rs::run_server`)

`std::net::TcpListener` accept loop, one `std::thread` per
connection. The 16 MiB body cap is the only DoS guard. Routes:

```text
GET  /                            → pre-rendered route HTML
GET  /az/loader.js                → inline bootstrap
GET  /az/mini.<hash>.wasm         → mini bytes (~5 KB)
GET  /az/cb/<name>.<hash>.wasm    → per-cb bytes (~14 KB)
GET  /az/layout/<name>.<hash>.wasm → per-layout bytes (preloaded)
GET  /az/img/<id>                 → image bytes
GET  /az/font/<id>                → font bytes
POST /az/exec/<node_id>           → server-side fallback dispatch
                                    (unused by current loader.js)
```

Wasm + asset responses set
`Cache-Control: public, max-age=31536000, immutable` because
URLs embed a content hash.

## Lift pipeline

The lift pipeline ships in `transpiler_remill.rs` under the
`web-transpiler` Cargo feature. It runs **per function** (for
eventloop fns) or **per transitive closure** (for cbs + layouts).
The per-function piece is `produce_object_for`; the closure
walker is `lift_with_transitive_deps`.

### Per-function lift — `produce_object_for`

```text
host bytes
  ─ rewrite_tailcall_wrapper (arm64) ──►
  ─ remill-lift-17 --arch aarch64 --os macos --address <lift_addr>
       --entry_address <lift_addr> --bytes <hex> ──► .lifted.ll
  ─ parse_extern_sub_declares  ──►  list of sub_<hex>[.N] externs
  ─ resolve each extern ───────────►
       branch_target_to_host_addr (strip .N) → host_addr
       resolve_fn_ptr             → dladdr + PLT-stub chase
       classify_branch_extern     → RustAlloc / AzCallIndirect /
                                    AzResolveCallback / Noop
  ─ emit_helper_ir ────────────────►  wrapper + per-kind bodies
                                       + PLT-stub thunks .helper.ll
  ─ llvm-link patched.ll helper.ll  → linked.ll
  ─ opt -O2                          → opt.ll
  ─ llc -mtriple=wasm32 -O2          → .o
```

### Tail-call wrapper byte rewrite (`rewrite_tailcall_wrapper`)

Many C-ABI shims in libazul are compiled as a single
`b <inner>` (unconditional branch to a Rust internal). Example:

```text
_AzRefCount_canBeSharedMut:
    b __ZN9azul_core6refany8RefCount17can_be_shared_mut...
```

remill bails on bare `b imm26` by lifting it as
`__remill_missing_block` and returning immediately — the body
appears empty and the wrapper looks like an identity function.

Workaround (arm64-only,
[hack #6](../../../../scripts/HACKS_REVIEW_2026_05_16.md#6-tail-call-wrapper-byte-rewrite-is-arm64-only)):
detect the encoding `bits 30..26 == 0b000101` (B unconditional)
and rewrite the 4 input bytes to `BL imm26` + `RET` (8 bytes
total) before feeding to remill. The lift then produces a normal
call+return; the PLT-stub thunk machinery wires the call to the
real lifted inner body.

### Extern parsing — `.N` suffix handling

remill emits a fresh `declare ptr @sub_<hex>(...)` per call site
when the same bl target appears multiple times in a function;
duplicates get the `.1`, `.2`, … suffix from LLVM's IR-level
symbol-table dedup. `AzStartup_hydrate` doing two
`bl __rust_alloc` produces:

```llvm
declare ptr @sub_<rust_alloc_addr>(ptr noalias, i64, ptr noalias)
declare ptr @sub_<rust_alloc_addr>.2(ptr noalias, i64, ptr noalias)
```

Both `parse_extern_sub_declares` and `branch_target_to_host_addr`
strip the `.N` suffix so all variants resolve to the same host
addr and each gets its own bump-allocator body emitted under its
suffixed name. Without this, the `.N` variants became unresolved
`env.sub_<hex>.N` imports that JS satisfied with shape-guessed
noops — the second `__rust_alloc` returned 0 and the whole Box
chain unraveled silently.

### PLT-stub chase — `mod.rs::resolve_macos_arm64_stub`

dladdr on a `__TEXT.__stubs` trampoline returns the
`cb_<hex>` placeholder because the stub has no symbol of its own.
Workaround (macOS arm64 only,
[hack #5](../../../../scripts/HACKS_REVIEW_2026_05_16.md#5-plt-stub-resolver-is-macos-arm64-only)):
parse the canonical Apple Silicon stub pattern

```text
adrp x16, GOT_PAGE
ldr  x16, [x16, GOT_OFF]
br   x16
```

compute the GOT slot address, deref it, and re-dladdr the
resolved target. Modern macOS arm64 eagerly populates `__got`
at process load, so the slot is valid by the time the server
runs. `resolve_fn_ptr` does the chase inline so every caller
sees one address-→-symbol map.

The dep that gets enqueued for transitive lift uses the
**resolved (libazul) address** for its lift, but the caller's
lifted IR references the **stub address** in the user binary —
the linker can't match these names directly. This is what the
thunk emission below fixes.

### Helper-IR emission — `emit_helper_ir`

Per-fn `.helper.ll` contains:

1. A **wrapper** that exposes the lifted body to a JS-callable
   signature (currently the canonical
   `callback(i64 refany_lo, i64 refany_hi, i32 info_ptr) → i32`,
   [hack #9](../../../../scripts/HACKS_REVIEW_2026_05_16.md#9-per-cb-wrapper-signature-is-hardcoded-to-i64-i64-i32--i32)
   for the generalization plan). The wrapper:
   - Allocates a 1088-byte `state_buf` on the wasm shadow stack
     to hold the lifted body's `%struct.State`.
   - Allocates a 4096-byte `stack_buf` for SP-relative spills
     ([hack #13](../../../../scripts/HACKS_REVIEW_2026_05_16.md#13-the-4-kib-stack-inside-each-wrapper-is-hardcoded)).
   - `memset state_buf` to 0.
   - Stores incoming args into `State.X<n>` slots at the AArch64
     PCS offsets baked into `signature_for_callback_kind`.
   - Sets `State.SP = top(stack_buf)`.
   - `call sub_<addr>(state, pc, memory)`.
   - Loads the return register slot (`State.X0` etc.) and
     returns.

2. A **per-extern body** per resolved branch:
   - `RustAlloc` / `RustAllocZeroed` → bump-allocator body
     reading `size` from `State.X0`, bumping the
     `@__az_bump_ptr` global, writing the old value back to
     `State.X0`. `linkonce_odr` + `alwaysinline` so wasm-ld
     dedupes across `.o` files into one shared heap. Initial
     `@__az_bump_ptr = 1048576` (1 MiB) leaves the wasm
     shadow-stack region untouched.
   - `AzCallIndirect` → `call_indirect` bridge through
     `__indirect_function_table` (used by the lifted
     `AzStartup_dispatchEvent` to invoke per-cb table slots).
   - `AzResolveCallback` → wasm `env` import bridge resolved
     JS-side (used by `AzStartup_dispatchEvent` for the
     fn-addr → table-idx lookup).
   - `Noop` with `real_addr != stub_addr` → **thunk**:

     ```llvm
     declare ptr @sub_<real_addr>(ptr, i64, ptr)
     define linkonce_odr ptr @sub_<stub_addr>(
         ptr %state, i64 %pc, ptr %memory) {
       %r = musttail call ptr @sub_<real_addr>(
              ptr %state, i64 %pc, ptr %memory)
       ret ptr %r
     }
     ```

     Routes the caller's `call sub_<stub_addr>` through to the
     real body the transitive lift emitted at
     `sub_<real_addr>`. opt usually inlines the musttail away.
   - `Noop` with `real_addr == stub_addr` → **no body emitted**.
     The extern stays unresolved; wasm-ld either pairs it with a
     sibling `.o`'s real body (when the recursive lift covered
     it) or leaves it as an `env.sub_<hex>` import that JS's
     Proxy fallback satisfies with shape-guessed noops at
     runtime
     ([hack #8](../../../../scripts/HACKS_REVIEW_2026_05_16.md#8-helper-ir-no-longer-emits-noop-bodies-for-noop-kind)).

   The earlier design — emit `alwaysinline` noop bodies for every
   extern — was the load-bearing bug. opt -O2 was inlining
   those noops into every call site, erasing the call before
   wasm-ld could retarget at the real body. Dropping the noop
   body emission was the unlock that let the cb actually invoke
   real lifted code.

3. The shared globals at module bottom:

   ```llvm
   @__az_bump_ptr = linkonce_odr global i32 1048576, align 4
   @__az_call_observer = linkonce_odr global i32 0, align 4
   declare i32 @__az_resolve_callback(i64) #1
   ```

### Recursive transitive lift — `lift_with_transitive_deps`

The per-cb / per-layout pipeline. Given a set of root functions:

1. Lift each root.
2. Parse externs; for each, run `resolve_fn_ptr` (which already
   does the PLT-stub chase).
3. If the resolved name passes `is_recursable_dep`, enqueue it
   as a dependency to lift at `lift_addr = resolved.addr`.
4. Continue until the queue empties or `MAX_RECURSIVE_DEPTH = 64`
   is reached.
5. wasm-ld over all the resulting `.o` files (one per fn) with
   `--import-memory --import-table --allow-undefined`.

The recursable-dep filter (`is_recursable_dep`):

- Skip `_dyld_*`, `_dispatch_*`, `_pthread_*`, `_objc_*` (libSystem).
- Skip anything containing `__rustc` or `__rust_` (compiler-internal
  symbols handled by `classify_branch_extern`'s RustAlloc kind).
- For Itanium-mangled `_ZN<len><crate>...E`: parse the crate name.
  Skip the known-noisy runtime crates (`core`, `std`, `alloc`,
  `compiler_builtins`, `panic_abort`, `panic_unwind`,
  `rustc_demangle`, `backtrace`, `addr2line`, `gimli`, `object`,
  `miniz_oxide`). Recurse into everything else — most importantly
  `azul_core`, `azul_css`, `azul_layout`, `webrender_*`, and any
  user-named crate the cb pulls in.
- Skip `_R*` (Rust v0 mangling) conservatively for now.
- Skip leading-underscore C internals (`_malloc`, `_memcpy`, ...)
  unless they're `Az`-prefixed.

This filter
([hack #7](../../../../scripts/HACKS_REVIEW_2026_05_16.md#7-recursion-filter-has-a-hand-curated-allowlist))
is hand-curated; the "pre-compile every api.json fn" architecture
in the M8.7 plan would replace it with a positive whitelist
driven by the classification.

Each dep gets exported as `__az_dep_<resolved_addr>` (the
JS-callable wrapper using the canonical signature) — those
wrappers are not invoked in production but stay around as anchors
keeping the lifted bodies from being DCE'd by wasm-ld's
`--gc-sections`.

### Memory layout

One `WebAssembly.Memory` (2 MiB initial, exported by mini,
imported by every cb / layout):

```text
0x000000  ┌──────────────────────────┐
          │ wasm-ld static data      │  mini's globals, cb's
          │ (per-module overlays —   │  globals overlay the same
          │  not currently isolated) │  region today
~0x010000 ├──────────────────────────┤
          │ per-cb stack             │  alloca [4096 x i8] inside
          │ (4 KiB per cb wrapper    │  each wrapper, lives on the
          │  invocation)             │  wasm shadow stack
~0x100000 ├──────────────────────────┤  ◄── @__az_bump_ptr starts here
          │ EventloopState           │  AzStartup_init
          │ MyDataModel { counter }  │  hydrate's alloc(4) for model
          │ RefCountInner            │  hydrate's alloc(128)
          │ AzRefAny                 │  hydrate's alloc(32)
          │ ... subsequent allocs    │  AzStartup_alloc on demand
0x200000  └──────────────────────────┘  ◄── 2 MiB cap (no grow today)
```

All in one address space, so the cb's `ldr w8, [X0]` (where X0 =
modelPtr from the hydrated chain) lands at the byte JS hydration
wrote, and the cb's `*counter += 1` is observable from JS.

The wasm-ld flags are:

- `azul-mini.wasm`: `--import-table --initial-memory=2097152`,
  exports `memory`.
- per-cb / per-layout: `--import-memory --import-table
  --initial-memory=2097152`. The
  `--initial-memory` here is just the import descriptor — the
  actual memory comes from mini at instantiate time.

## AzStartup_* surface — `eventloop.rs`

Mini exports six C-ABI fns:

| symbol | signature | role |
|---|---|---|
| `AzStartup_alloc(size: u32) -> u32` | bump | allocate `size` bytes of zero-init linear memory, return wasm offset |
| `AzStartup_free(ptr: u32, size: u32)` | bump | currently no-op (bump heap doesn't free) |
| `AzStartup_init(json_ptr: u32, json_len: u32) -> u32` | state | allocate `EventloopState`, return its wasm ptr |
| `AzStartup_hydrate(type_id_lo, type_id_hi, data_ptr, data_size: u32) -> u32` | hydration | build wasm-side `AzRefAny` tree, return refany ptr |
| `AzStartup_dispatchEvent(state, kind, evt_ptr, evt_len, out_len_ptr: u32) -> u32` | dispatch | decode event, hit-test, resolve cb, invoke via `__az_call_indirect`, emit patches (BUILT, not yet wired by loader) |
| `AzStartup_registerStateDeserializer(state: u32, fn_addr: u64)` | deser | store user's `_fromJson` fn-ptr on the state (not used today) |

### AzStartup_hydrate

```rust,ignore
pub unsafe extern "C" fn AzStartup_hydrate(
    type_id_lo: u32, type_id_hi: u32,
    data_ptr: u32, data_size: u32,
) -> u32
```

Builds the wasm-side `AzRefAny → RefCount → RefCountInner →
user data` tree without going through `Box::new(struct_literal)`
(whose codegen loads `sizeof::<T>()` + `alignof::<T>()` from
arm64 const pools that don't lift cleanly,
[hack #6 in the M8.7c lessons-learned](#6)). Instead:

1. `data_alloc = AzStartup_alloc(128)` for `RefCountInner` (~112 B
   real + padding).
2. `refany_alloc = AzStartup_alloc(32)` for `AzRefAny` (24 B real
   + padding).
3. Fields written via `core::ptr::addr_of_mut!` + direct stores —
   no struct-literal init.
4. `sharing_info.ptr = data_alloc`,
   `sharing_info.run_destructor = false` (hydrated RefAny lives
   for the lifetime of the wasm instance,
   [hack #11](../../../../scripts/HACKS_REVIEW_2026_05_16.md#11-azstartup_hydrate-is-a-hand-rolled-refany-builder)).
5. `RefCountInner.type_id = (type_id_hi << 32) | type_id_lo`.
6. `RefCountInner._internal_ptr = data_ptr` (caller-allocated
   user data buffer).
7. `num_copies = 1, num_refs = 0, num_mutable_refs = 0`.

`data_align` is hardcoded to 8; `custom_destructor` points at a
no-op `extern "C" fn`. Sufficient for `is_type` /
`can_be_shared_mut` / `getDataPtr` / `increase_refmut` /
`decrease_refmut` chains to succeed.

The longer-term plan is to drop `AzStartup_hydrate`'s hand-rolled
approach in favor of calling the user's lifted `_fromJson`
deserializer via `__az_call_indirect` (the path
`AzStartup_registerStateDeserializer` exists for) — see
[M8.8 Step 3](../../../../scripts/M8.8_NEW_SESSION_PROMPT.md#step-3--lifted-user-_fromjson-hydration-path).

### AzStartup_dispatchEvent

Decodes a 256-byte event buffer with the layout from
`event_offset`:

```text
+0   u32 node_idx
+4   f32 x
+8   f32 y
+12  u32 button_or_key
+16  u32 modifiers
```

Looks up the cb fn-addr via the wasm-side `cb_fn_cache`, calls
`__az_resolve_callback(fn_addr)` to get the table index, then
`__az_call_indirect(table_idx, FAKE_REFANY_LO=0x101,
FAKE_REFANY_HI=0, event_bytes_ptr)`.

**Currently bypassed by loader.js** (which calls the cb directly
with the real hydrated `azRefAnyPtr` instead). The dispatchEvent
chain works in isolation but uses the FAKE_REFANY placeholder
because there's no `AzStartup_setAppData` helper yet — see
[M8.8 Step 2](../../../../scripts/M8.8_NEW_SESSION_PROMPT.md#step-2--wire-azstartup_dispatchevent-for-full-event-handling).

## Phase B — browser bootstrap (`loader_js.rs`)

```text
1. Find /az/mini.<hash>.wasm URL from <link rel="preload">
2. azTable = new WebAssembly.Table({initial: 64, element: 'anyfunc'})
3. Build mini imports:
     env.__indirect_function_table = azTable
     env.__az_resolve_callback     = fnAddr → table_idx via Map
     ... + Proxy fallback: shape-guessed noops by name pattern
         (_64 / _f64 → 0n;
          *_memory | barrier | exception_clear → undef;
          else → 0)
4. WebAssembly.instantiateStreaming(fetch(miniUrl), imports)
     → azMini, azMemory = azMini.memory
5. azState = azMini.AzStartup_init(0, 0)
6. azHydrate():
     a. Read #az-hydrate JSON
     b. typeId = BigInt(payload.type_id)
     c. counter = payload.json (assumes number — hello-world only)
     d. azModelPtr = mini.AzStartup_alloc(4)
        DataView.setUint32(modelPtr, counter)
     e. azRefAnyPtr = mini.AzStartup_hydrate(
          typeIdLo, typeIdHi, modelPtr, 4)
7. For each [data-az-cb][data-az-wasm] in DOM:
     a. WebAssembly.instantiateStreaming(fetch(url), {
            env: { memory: azMemory,
                   __indirect_function_table: azTable,
                   ...Proxy noop fallback }})
     b. azTable.set(nodeIdx, cb.exports.callback)
     c. azNodeCbFns.set(nodeIdx, cb.exports.callback)
8. azWireListeners():
     body.addEventListener('click', azInvokeCbDirect)
     [no mousedown/keydown/focus/resize/scroll — hack #2]
```

## Phase C — click → cb → DOM update (current direct path)

```text
1. body 'click' fires → azInvokeCbDirect(evt)
2. nodeIdx = azNodeIdxFromEvent(evt)        [regex on id=az_N]
3. cbFn = azNodeCbFns.get(nodeIdx)
4. infoPtr = mini.AzStartup_alloc(256)
5. update = cbFn(BigInt(azRefAnyPtr), 0n, infoPtr)
   ─── inside the cb wasm ───
   Wrapper:
     a. alloca 1088 B (state) + 4096 B (stack)
     b. memset state to 0
     c. State.X0 = refany_ptr  (low 32 = wasm offset)
        State.X1 = 0
        State.X2 = info_ptr (zext)
     d. State.SP = top(stack_buf)
     e. call sub_<onclick_addr>(state, pc, memory)

   Lifted on_click:
     - Read State.X0 = refany_ptr
     - Spill to stack-relative slot
     - call sub_<MyDataModelRefMut_create>
     - Reload refany_ptr
     - call sub_<MyDataModel_downcastMut>:
         call sub_<stub_isType> → musttail thunk → sub_<real_isType>:
           call sub_<get_type_id>:
             load *(refany_ptr + 0)  = inner_ptr
             load *(inner_ptr + 56) = type_id
             return type_id in X0
           compare X0 vs X19 (saved type_id arg)
           cset W0 = 1
           return
         check W0 & 1 → success branch
         call sub_<stub_canBeSharedMut> → thunk → real
           (lifted from rewritten BL+RET bytes)
           call sub_<can_be_shared_mut>:
             load num_refs, num_mutable_refs (both 0)
             return 1
         check W0 & 1 → success
         call sub_<stub_increaseRefmut> → thunk → real
           atomic incr num_mutable_refs in linear memory
         call sub_<stub_getDataPtr> → thunk → real
           return *(inner_ptr + 0) = modelPtr in X0
         store modelPtr to local.RefMut.ptr
         return W0 = 1
     - tbnz w0, #0 → success branch
     - ldr x9, [sp + local_refmut_offset] = modelPtr
     - ldr w8, [x9] = counter (5)
     - add w8, w8, #1 = 6
     - str w8, [x9] = write back to *modelPtr
     - mov w_ret, #1 = AzUpdate_RefreshDom
     - call sub_<MyDataModelRefMut_delete>
     - return W0 = 1

   Wrapper:
     f. Load State.X0 (low 32 = 1)
     g. Return as i32

   ─── back in JS ───
6. mini.AzStartup_free(infoPtr, 256)
7. if (update >= 1):
     newCounter = DataView(memory).getUint32(modelPtr)
     document.getElementById('az_1').textContent = newCounter
                                  ^^^^^^^^^^^^^^^
                                  hardcoded — hack #1
```

## What's bypassed

Even though the corresponding code is built and (in some cases)
lifted into wasm, the current bootstrap does not use:

| built | invoked? | why bypassed |
|---|---|---|
| `AzStartup_dispatchEvent` | ✗ | `FAKE_REFANY_LO=0x101` hardcoded; needs `setAppData` wiring + JS to call through it instead of `azInvokeCbDirect` |
| `AzStartup_registerStateDeserializer` | ✗ | hydration uses `AzStartup_hydrate` hand-rolled path instead of lifting the user's `_fromJson` |
| `/az/layout/<name>.<hash>.wasm` | ✗ | preloaded by `<link rel="preload">` but loader never instantiates; needed for `Update::RefreshDom` re-layout in the browser |
| `azApplyPatches` TLV decoder | ✗ | the decoder is in `loader.js` (handles `SetText`) but no wasm-side producer emits patches yet; JS does the direct `textContent =` hardcode |
| WASM-side hit-test | ✗ | JS-side `azNodeIdxFromEvent` regex on `id="az_N"` IDs |
| `POST /az/exec/<node_id>` server fallback | ✗ | server-side path exists but loader doesn't fall back to it |

The full catalog of remaining hacks (19 items grouped into 5
categories) is in
[`scripts/HACKS_REVIEW_2026_05_16.md`](../../../../scripts/HACKS_REVIEW_2026_05_16.md);
the prioritized fix order is in
[`scripts/M8.8_NEW_SESSION_PROMPT.md`](../../../../scripts/M8.8_NEW_SESSION_PROMPT.md).

## What's principled and worth keeping

- **api.json walk** in `classify.rs` — drives off the brotli
  blob, no hand-coded function list.
- **Bump allocator in helper IR** — minimal, well-isolated,
  matches wasm linear-memory semantics.
- **`--import-memory` + `--import-table` for cb / layout wasms**
  — the right architecture for shared state across modules.
- **`AzStartup_hydrate` as a new C-ABI surface** — extends the
  surface in a way every language binding can call. Per user
  direction "ship more `AzStartup_*` functions".
- **PLT-stub THUNK emission** (vs renaming `sub_<addr>` symbols
  in the lifted IR) — keeps lifted bodies unmodified; lets
  wasm-ld handle linkage normally. Survives the planned
  symbol-table-driven replacement of the byte parse.
- **Pre-validation of RefAny serializer at startup** in
  `headless.rs::HeadlessApp::validate` — fail-fast for
  misconfigured apps.

## Asset URL summary

```text
GET  /                              → pre-rendered HTML
GET  /az/loader.js                  → bootstrap JS (inline-embedded)
GET  /az/mini.{hash}.wasm           → mini wasm (~5 KB)
GET  /az/cb/{name}.{hash}.wasm      → per-cb wasm (~14 KB)
GET  /az/layout/{name}.{hash}.wasm  → per-layout wasm
GET  /az/img/{id}                   → image bytes
GET  /az/font/{id}                  → font bytes
POST /az/exec/{node_id}             → server-side fallback dispatch
```

The `/az/` prefix is the only reserved namespace. Any other path
is matched against registered routes.

## Cross-references

- [DOM Internals](dom.md) — the `Dom` / `NodeData` / `NodeType`
  model the renderer walks.
- [Styling — Cascade](styling/cascade.md) — the `StyledDom` and
  property cache the renderer reads.
- [Events](events.md) — the `EventFilter` enum mapped to JS event
  names.
- [`scripts/HACKS_REVIEW_2026_05_16.md`](../../../../scripts/HACKS_REVIEW_2026_05_16.md)
  — catalog of remaining hacks.
- [`scripts/M8.8_NEW_SESSION_PROMPT.md`](../../../../scripts/M8.8_NEW_SESSION_PROMPT.md)
  — prioritized fix order for the next session.

## Coming Up Next

- [Rendering](rendering.md) — From `StyledDom` to pixels
- [Rendering — WebRender Bridge](rendering/webrender-bridge.md) — How azul talks to WebRender
- [FFI Codegen](build-and-codegen.md) — How `cargo build` cascades and the codegen pass
