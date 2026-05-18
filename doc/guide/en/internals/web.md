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
  - dll/src/web/cpp/azul_remill.cpp
  - dll/src/web/cpp/azul_remill.h
  - dll/src/web/eventloop.rs
  - dll/src/web/headless.rs
  - dll/src/web/html_render.rs
  - dll/src/web/hydration.rs
  - dll/src/web/loader_js.rs
  - dll/src/web/mod.rs
  - dll/src/web/native_remill.rs
  - dll/src/web/server.rs
  - dll/src/web/symbol_table.rs
  - dll/src/web/transpiler.rs
  - dll/src/web/transpiler_remill.rs
last_generated_rev: b1470628a
generated_at: 2026-05-18T16:30:00Z
default-search-keys:
  - StyledDom
  - RefAny
  - LayoutCallback
  - AzStartup_hydrate
  - AzStartup_dispatchEvent
  - LayoutCallback
---

# Web Backend Internals

> **📋 Architectural retrospective:** see
> [`scripts/M9_REVIEW_AND_OPTION_A.md`](../../../../scripts/M9_REVIEW_AND_OPTION_A.md)
> for the post-M9 review that drove the synthetic-address lift
> fix described under "Synthetic-address lift" below.

## Status (as of M9-after-review, 2026-05-18)

**Synthetic-address lift scheme shipped** (`23d7174d5`). The
previous `lift_addr = native_runtime_addr` convention is gone;
remill now sees a small per-image synthetic address at
`--address=`, which means the lifted `adrp+ldr` page targets
land in a predictable wasm-friendly band of linear memory
instead of at ~200 MiB truncated runtime addresses. Three
direct improvements:

  - **Wasm memory dropped 1 GiB → 128 MiB.** The previous
    bloat absorbed the high adrp targets; with synth
    addresses they never exceed 128 MiB and the bump heap
    sits at 96 MiB.
  - **On_click counter e2e (5→12) passes** in BOTH subprocess
    and `AZ_NATIVE_REMILL=1` modes through the FULL dispatch
    path. The cb's lifted `adrp+add` for `_MyDataModel_RttiTypeId`
    produces a synth address; `html_render.rs` translates the
    server-captured native type_id through
    `SymbolTable::native_to_synth` so the JS-supplied hydrate
    value matches what the cb computes. Without that
    translation `MyDataModel_downcastMut` would fail and the
    cb would return DoNothing.
  - **Minimal layout probe passes** end-to-end (`hello-world-minimal.bin`
    → `initLayoutCache rc=0`, current_dom populated).

What's still NOT working — full `examples/c/hello-world.c`
layout probe traps deeper in libazul's lifted code (now at
`wasm-function[103]:0x25b3c` instead of the pre-synth
`wasm-function[19]:0x57c8`). The trap moved from "no memory to
deref" to "some libazul-side `adrp+ldr` landing in an
unmirrored data section." Solvable by extending the mirror
filter (currently `__TEXT.__cstring`, `__TEXT.__const`,
`__DATA.__data`, `__DATA.__const`, `__DATA_CONST.__const`) to
include `__DATA_CONST.__got` and friends, or by per-symbol
SymbolTable classification of the specific reads.

**Trade-off**: mini.wasm grew from ~9 KiB to ~27 MiB to carry
the libazul data mirror. Compresses well on the wire (a few
MiB after gzip) and is a one-time download per session; the
alternative is per-cb data-segment partition which is more
complex to implement.

## Synthetic-address lift

ARM64 `adrp x<n>, IMM` lifts to:

```llvm
%pc = load i64, ptr %PC, align 8
%target_page = and i64 %pc, -4096   ; (PC & ~0xFFF) + imm<<12, simplified
                                    ; when imm = 0 (same-page target)
store i64 %target_page, ptr %X<n>, align 8
```

`%pc` comes from `--address=…` at lift time. With the previous
`lift_addr = post_ASLR_runtime_addr` convention, `%pc` was a
runtime address like `0x10cf12345`; truncated to 32 bits for
wasm32, the lifted code's `inttoptr i32 %addr to ptr; load`
hit ~200 MiB OOB. The synthetic scheme replaces this with a
small per-image base such that all post-lift addresses fit
inside the wasm `initial-memory` cap.

### Per-image rebasing

`SymbolTable::assign_synthetic_addresses` walks the loaded
images (filtered by `is_system_image`), records each one's
runtime span as an `ImageRebase`, and assigns a unique
synth base in monotonically increasing order:

```
synth offset │ what lives here
─────────────┼─────────────────────────────────────────
0x0    .. 0x10000    wasm runtime stack (per-wasm via the
                     post-link `relocate_stack_*` patch)
0x10000+            image 0 (typically user binary, ~64 KiB)
0x100000+           image 1 (libazul.dylib, ~80 MiB span)
…                   subsequent images, 1 MiB-aligned bases
~96 MiB             bump-allocator heap base (@__az_bump_ptr)
~128 MiB            end of initial wasm memory
```

For every entry: `synth = synth_base + (canonical_addr - native_base)`.

### Symbol-name flow through the pipeline

```
bytes-scan in BFS pre-walk:
    finds `bl 0x100013e7c`   (native target)
    table.resolve(0x100013e7c) → libazul canonical entry
    queue canonical_native for lift

lift queue iteration:
    addr = canonical_native
    lift_addr = symbol_table::get().lookup(addr).synthetic_addr
               = libazul_synth
    remill emits `define ptr @sub_<libazul_synth>`
    bl targets in this fn lift as
        `call sub_<lift_pc_synth + (native_target - lift_pc_native)>`
        = synth_of_native_target

post-lift rewrite_sub_names_to_canonical:
    Reads `sub_<HEX>` tokens, HEX is in synth space.
    For each: resolve_synth(HEX) chases the synth chain
    (mirroring the native `chain` map but populated with
    each pair's synth addrs). Emits `sub_<canonical_synth>`.

data-section mirror:
    Per `image_rebases`, walks each image's data sections.
    Writes a wasm Data segment at synth_base + file_offset.
    The cb's lifted `adrp+add+ldr` lands in this region.
```

### Why `_MyDataModel_RttiTypeId` needed special handling

The C macro:

```c
static uint64_t const structName##_RttiTypePtrId = 0;
static uint64_t const structName##_RttiTypeId =
    (uint64_t)(&structName##_RttiTypePtrId);
```

stores `_RttiTypeId = native_address_of_RttiTypePtrId` in
`__DATA_CONST.__const`. The user's data upcast captures
this NATIVE address into `RefAny.type_id`. Server emits
the value into the hydrate JSON.

The cb's lifted `MyDataModel_downcastMut` does
`adrp x1, _RttiTypeId@PAGE; add x1, x1, _RttiTypeId@PAGEOFF`
to compute the SAME address — but in SYNTH space because of
the synth lift. The two values mismatch → `isType` returns
false → cb returns DoNothing.

`html_render.rs` translates the captured native value to
synth via `SymbolTable::native_to_synth` BEFORE emitting the
hydrate JSON. Both sides then see the SAME synth value and
the comparison succeeds.

## Old status (M9-3b experiments, deleted)

**What works end-to-end (hello-world on_click counter):**

User clicks the button → wasm-side `AzStartup_dispatchEvent`
hit-tests, resolves the cb → `__az_call_indirect` invokes the
lifted `on_click` wasm with the hydrated `AzRefAny` → cb
increments the counter in-place → wasm reads the new counter,
encodes a `SetText` TLV patch → JS applies. Counter 5→12 in 7
clicks passes in BOTH `AZ_NATIVE_REMILL=1` and subprocess
modes. No JS-side regex hit-test, no JS-side direct cb call, no
`textContent =` hardcode.

**Architectural pieces built (M9-1..M9-6):**

  - **`Pcs::HiddenPtrReturn`** (`transpiler_remill.rs`) — wrapper
    synthesis for callbacks returning `>16B` aggregates via
    AArch64's hidden X8 register. Wrapper takes an extra `i32
    out_ptr` arg, seeds State.X8, returns `i32` status. Used by
    the layout cb (`(i64, i64, i32, i32) → i32` shape).

  - **Post-link stack relocator**
    (`transpiler_remill::patch_wasm_sp_init`) — assigns each wasm
    a non-overlapping stack region by rewriting `global[0]`'s
    init value. Mini gets slot 0 (192 KiB), each non-mini wasm
    gets a unique 128 KiB-strided slot. Fixes the cross-module
    State corruption bug (mini called layout cb → layout's
    wrapper zeroed mini's State because both stacks landed at
    ~64 KiB).

  - **`AzStartup_dispatchEvent` rewrite** — single entry point for
    the click flow. Hit-tests if event's `node_idx` is `SENTINEL`,
    resolves cb fn-addr → table_idx, invokes via
    `__az_call_indirect` with the hydrated refany_ptr (no more
    FAKE_REFANY), and on `RefreshDom` reads `state.model_ptr` to
    encode a SetText TLV patch into a returned buffer.

  - **`AzStartup_buildCounterPatch`** — wasm-side u32-to-decimal
    + TLV-encoding routine. Avoids the snprintf-noop trap (Leaf
    bodies now zero X0 so unlifted libc returns "0" instead of
    a stale buffer pointer).

**What is stub or scaffolding (NOT production):**

  - **`AzStartup_hitTest`** — currently returns
    `state.last_registered_cb_node_idx` (the most-recent cb the
    JS bootstrap reported). For hello-world's single-button DOM
    this is exact; for any non-trivial layout it's wrong. Real
    bbox-based hit-test requires the LayoutWindow embed (NOT
    SHIPPED — see "Gap" below).

  - **`AzStartup_initLayoutCache`** — runs the lifted layout cb
    once and stores the returned `AzDom` blob pointer in
    `state.current_dom_ptr`. But **no `Dom → StyledDom`
    conversion happens**, **no layout runs**, **no
    `cb_fn_cache` is populated by walking the DOM**. The "WASM
    DOM" is a placeholder pointer.

  - **`AzStartup_buildLayoutInfo`** — returns a 512-byte
    zero-blob. Hello-world's layout cb doesn't read any
    `LayoutCallbackInfo` fields so this works; a real cb that
    queries `info.system_fonts` or `info.window_size` would
    deref a NULL `ref_data` and trap.

  - **Data-section mirror** (`SymbolTable::enumerate_low32_data_for_wasm`,
    `patch_wasm_add_data_segments`) — sound scaffolding but
    limited to data sections whose runtime address truncated to
    32 bits falls below 1 MiB. Typical macOS ASLR slides for
    user binaries are multi-MiB, so in practice this is rarely
    triggered.

**Full `hello-world.c` (the one with snprintf + const strings)
still TRAPS** during `AzStartup_initLayoutCache` because the
lifted layout cb's body reads from libazul `__const` at
truncated runtime offsets around 200 MiB — past wasm's 16 MiB
linear memory. `examples/c/hello-world-minimal.c` (returns
`AzDom_createBody()` with no const strings) works end-to-end
through the WASM-resident dispatch path.

**Gap vs the user's intent** (see "User intent vs implementation"
below for the full mapping): of the user's five steps —
(1) HTML+RefAny, (2) WASM-StyledDom + layout cache,
(3) JS-events → WASM hit-test, (4) WASM cb dispatch,
(5) WASM patches → JS apply — steps 1, 4, 5 work. Steps 2 and
3 are stubs. The architectural skeleton for steps 2-3 is in
place; the WASM-resident StyledDom + layout cache + real
hit-test are the genuine next work.

Backup: `m8.9-victory` tag at commit `9780a92b3` is the M8.9
close-out. The M9 work is on `layout-debug-clean` (commits
`7a9250fde` through `b1470628a`).

## User intent vs implementation

The user's five-step request:

| step | intent | built | status |
|--:|---|---|---|
| 1 | Site init returns RefAny serialized inside HTML | `html_render.rs` emits `<script id="az-hydrate">` with `{type_id, json}`. JS `azHydrate()` reads it + calls `AzStartup_hydrate(type_id, data_ptr, data_size)` to build a wasm-resident `AzRefAny`. | ✅ Works |
| 2 | WASM initializes, uses RefAny to create initial WASM-StyledDom, runs layout to populate layout cache | `AzStartup_initLayoutCache` invokes the lifted layout cb via `__az_call_indirect_layout4`; cb writes returned `AzDom` blob through X8 into a 256-byte slot owned by `EventloopState.current_dom_ptr`. NO `Dom → StyledDom`. NO layout run. NO cache populated. | ❌ Stub. The Dom blob is stored but never processed. |
| 3 | JS calls populated layout cache with events for hit testing | `AzStartup_hitTest(state, x_bits, y_bits) → node_idx` exists but returns `state.last_registered_cb_node_idx` — the most recent cb JS registered, ignoring x/y entirely. | ❌ Stub. No real hit-test; only works for single-cb demos. |
| 4 | WASM determines events, looks up instantiated callback in lookup table, then executes | `AzStartup_dispatchEvent` calls hit-test, resolves cb fn-addr → table_idx via JS-imported `__az_resolve_callback`, invokes cb via `__az_call_indirect` with the hydrated `refany_ptr` (no more FAKE_REFANY). | ✅ Works for the on_click counter. |
| 5 | WASM returns the "stuff to do" list back to JS | `AzStartup_buildCounterPatch` encodes a SetText TLV (kind=1, node_idx, payload_len, ASCII decimal of the counter). `AzStartup_dispatchEvent` returns `(buf_ptr, buf_len)`; loader.js `azApplyPatches` decodes + applies. | ✅ Works for SetText. Other patch kinds (SetAttr, InsertNode, etc.) deferred until step 2/3 land. |

**Why this is so hard (honest answer):**

The chosen approach — *lifting the user's ARM64-compiled layout
cb into wasm* — drags in the user binary's transitive dependency
graph (146+ libazul functions for hello-world's layout cb).
Two consequences:

  1. **Address-space pollution.** Lifted ARM64 code bakes
     post-ASLR runtime addresses (e.g. `adrp x0, 0x10c000000`)
     as wasm `i32.const` values. Truncated to 32 bits those
     addresses land at offsets the wasm linear memory doesn't
     cover (libazul `__const` at ~200 MiB; wasm memory is
     16 MiB). Bumping memory to 1 GiB absorbs the loads but
     reads zeros (data isn't mirrored), so the cb completes
     with wrong data → silent garbage. The real fix is at lift
     time: pass a small synthetic `--address` to remill-lift
     OR post-IR rewrite of `i64.const <high>` constants. Both
     are nontrivial.

  2. **Architectural mismatch.** The user's intent
     ("WASM creates the initial WASM-StyledDom") naturally
     reads as "wasm-native runtime code constructs StyledDom".
     The current path ("lifted ARM64 user cb returns Dom; we
     store the blob and call it the WASM DOM") doesn't deliver
     a StyledDom at all. A native-wasm32 azul-wasm-runtime
     crate (parses a Dom description; builds StyledDom; runs
     layout; exposes hit-test + dispatch) would skip the lift
     entirely for the framework path. The user cb could still
     be lifted, with a much smaller transitive surface (only
     the Dom-building APIs).

These are the architectural calls the next session needs to
make. The M9 scaffolding (wrapper synthesis, stack relocator,
dispatch flow, patch encoding) is sound and reusable in either
direction; the question is whether to keep pushing on the lift
approach or pivot to the native-wasm runtime.

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

The per-cb / per-layout pipeline. Two code paths based on
`use_native_remill()`:

**Subprocess path** (`lift_with_transitive_deps_sequential`):

1. Lift each root.
2. Parse externs from the lifted IR; for each, use the
   SymbolTable's `resolve()` to canonicalize.
3. If the resolved entry's classification is `Recursable`, enqueue
   the canonical address as a dep.
4. Continue until queue empties or `MAX_RECURSIVE_DEPTH = 256`.
5. wasm-ld over all `.o` files with the standard flags.

**Native batched path** (`lift_with_transitive_deps_batched`,
M8.9-3b):

1. BFS pre-walk via `scan_arm64_bl_b_targets` — bytes-scan every
   fn's body for ARM64 BL (0b100101) and B (0b000101) imm26
   instructions, decode targets, resolve through SymbolTable, and
   enqueue Recursable canonical addresses. No lift needed in this
   phase — fast (~ms).
2. One `az_remill_lift_batch` call lifts the entire dep set in a
   single `LoadArchSemantics`-amortized session. Per-fn cost
   drops from ~50 ms to ~5 ms.
3. Each per-fn IR feeds `produce_object_from_lifted_ir` (cache
   hits across deps shared with other roots/layouts).
4. wasm-ld over all `.o` files.

Bytes-scan caveats (handled by SymbolTable's `resolve()` chain):

  - BL targets land at canonical addrs directly.
  - B targets inside the fn's own range are intra-function
    branches; SymbolTable lookup returns None → skipped.
  - B targets outside the range are tail-call shims; `resolve()`
    chases through the `chain` map to the canonical callee.
  - BLR / BR (indirect) aren't statically resolvable from bytes —
    bridged via `__az_call_indirect` in helper IR.

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
0x1000000 └──────────────────────────┘  ◄── 16 MiB cap (no grow today)
```

All in one address space, so the cb's `ldr w8, [X0]` (where X0 =
modelPtr from the hydrated chain) lands at the byte JS hydration
wrote, and the cb's `*counter += 1` is observable from JS.

The wasm-ld flags are (post-M8.9 + cleanup):

- `azul-mini.wasm`: `--no-entry --allow-undefined --gc-sections
  --strip-all --lto-O2 --import-table --initial-memory=16777216`,
  exports `memory` + every `AzStartup_*`.
- per-cb / per-layout: same flags plus `--import-memory`. The
  `--initial-memory` here is just the import descriptor — the
  actual memory comes from mini at instantiate time.
- Post-link: `wasm-opt -Oz --strip-debug --strip-producers
  --vacuum` (best-effort; skipped if binaryen isn't installed).

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

### AzStartup_dispatchEvent (M9-6)

Decodes a 256-byte event buffer:

```text
+0   u32 node_idx     [SENTINEL = 0xFFFFFFFF → wasm hit-test]
+4   f32 x            (clientX, as f32 bits)
+8   f32 y            (clientY, as f32 bits)
+12  u32 button_or_key
+16  u32 modifiers
```

1. If `node_idx == SENTINEL`, calls `AzStartup_hitTest(state,
   x_bits, y_bits)` (M9-4 stub returns
   `state.last_registered_cb_node_idx`).
2. Resolves cb fn-addr → table_idx via JS-imported
   `__az_resolve_callback` (the single remaining JS↔WASM dispatch
   round-trip).
3. Invokes the cb via `__az_call_indirect(table_idx,
   state.refany_ptr as u64, 0, event_bytes_ptr)`. **M9-6: uses
   the hydrated refany_ptr, not FAKE_REFANY.**
4. If `update >= UPDATE_REFRESH_DOM` AND `state.model_ptr != 0`:
   reads counter from `*(state.model_ptr)`, calls
   `AzStartup_buildCounterPatch` to encode a SetText TLV into
   the lazily-allocated `state.patch_buf_ptr` (32 bytes),
   returns `(patch_buf_ptr, used_bytes)`.
5. Otherwise returns 0 (no patches); surfaces the cb's `update`
   value in `*out_len_ptr` for diagnostic logging.

### Other M9 mini exports

  - `AzStartup_setLayoutCbTableIdx(state, idx)` — JS hands the
    layout cb's `WebAssembly.Table` slot to mini after
    instantiation.
  - `AzStartup_setRefAny(state, refany_ptr)` — JS hands the
    hydrated `AzRefAny` pointer to mini.
  - `AzStartup_setModelPtr(state, model_ptr)` /
    `AzStartup_setDisplayNode(state, node_idx)` — JS plumbs
    the per-route model location + text-display node_idx so
    `AzStartup_dispatchEvent` can encode SetText patches
    without JS round-trips. Hello-world hardcodes these; a
    real implementation would discover them by walking the
    StyledDom (NOT SHIPPED).
  - `AzStartup_registerCbNode(state, node_idx)` — JS calls
    this once per per-cb wasm instantiation so the M9-4
    `AzStartup_hitTest` stub knows which nodes carry callbacks.
  - `AzStartup_initLayoutCache(state, vw, vh, theme)` — invokes
    the lifted layout cb via `__az_call_indirect_layout4`,
    stores the returned `AzDom` blob pointer in
    `state.current_dom_ptr`. **NO `Dom → StyledDom`, NO layout
    run** — the "WASM DOM" is a placeholder blob.
  - `AzStartup_getCurrentDomPtr` / `_getLastLayoutStatus` —
    JS-side accessors for debugging.
  - `AzStartup_buildCounterPatch(out_buf, cap, node_idx,
    counter)` — wasm-side u32-to-decimal + SetText TLV encoder.

## Phase B — browser bootstrap (`loader_js.rs`)

```text
1. Find /az/mini.<hash>.wasm URL from <link rel="preload">
2. azTable = new WebAssembly.Table({initial: 64, element: 'anyfunc'})
3. Build mini imports:
     env.__indirect_function_table = azTable
     env.__az_resolve_callback     = fnAddr → table_idx via Map
     ... + Proxy fallback: shape-guessed noops by name pattern
4. WebAssembly.instantiateStreaming(fetch(miniUrl), imports)
     → azMini, azMemory = azMini.memory
5. azState = azMini.AzStartup_init(0, 0)
6. azHydrate():
     a. Read #az-hydrate JSON  → {type_id, json}
     b. azModelPtr = mini.AzStartup_alloc(4)
        DataView.setUint32(modelPtr, counter)
     c. azRefAnyPtr = mini.AzStartup_hydrate(
          typeIdLo, typeIdHi, modelPtr, 4)
     d. M9-6: mini.AzStartup_setRefAny(azState, azRefAnyPtr)
              mini.AzStartup_setModelPtr(azState, azModelPtr)
              mini.AzStartup_setDisplayNode(azState, 1)
                  [hello-world: counter node_idx = 1]
7. For each [data-az-cb][data-az-wasm] in DOM:
     a. WebAssembly.instantiateStreaming(fetch(url), {
            env: { memory: azMemory,
                   __indirect_function_table: azTable,
                   ...Proxy noop fallback }})
     b. azTable.set(nodeIdx, cb.exports.callback)
     c. M9-4: mini.AzStartup_registerCbNode(azState, nodeIdx)
8. M9-2/3a: instantiate /az/layout/<name>.<hash>.wasm with the
   same env, place its `callback` export in azTable, then:
     mini.AzStartup_setLayoutCbTableIdx(azState, slot)
     mini.AzStartup_initLayoutCache(azState, vw, vh, 0)
9. azWireListeners():
     body.addEventListener('click', evt =>
         azDispatch(EVT_CLICK, evt))
     [no mousedown/keydown/focus/resize/scroll — hack #2]
```

## Phase C — click → cb → DOM update (M9-6 wasm-side dispatch)

```text
1. body 'click' fires → azDispatch(EVT_CLICK, evt)
2. JS encodes a 256-byte event buffer:
     [0..4]   = SENTINEL_NO_NODE (0xFFFFFFFF) — let WASM hit-test
     [4..8]   = clientX as f32 bits
     [8..12]  = clientY as f32 bits
     [12..16] = button/keycode
     [16..20] = modifier bitmask
3. JS calls AzStartup_dispatchEvent(azState, EVT_CLICK,
                                      evtPtr, 256, outLenPtr)

   ─── inside mini.wasm: AzStartup_dispatchEvent ───
   a. Read event_node_idx from evtPtr+0. If SENTINEL:
        node_idx = AzStartup_hitTest(state, x_bits, y_bits)
        (M9-4 stub: returns state.last_registered_cb_node_idx —
         for hello-world that's 3 = the button)
   b. cb_fn_addr = node_idx (M8.5a stub: identity)
   c. table_idx = __az_resolve_callback(cb_fn_addr)
        (JS-imported bridge — the ONE remaining JS↔WASM
         dispatch round-trip per the WASM-resident DOM vision)
   d. refany_lo = state.refany_ptr (M9-6: HYDRATED, not FAKE)
   e. update = __az_call_indirect(table_idx, refany_lo, 0,
                                   evtPtr)
        ─── inside on_click cb wasm (unchanged from M8.9) ───
        Wrapper synthesizes Pcs::Callback shape, seeds X0/X1/X2,
        invokes lifted body. Body downcasts refany, increments
        counter at *(inner_ptr + 0), returns AzUpdate_RefreshDom.
        ────────────────────────────────────────────────────
   f. if update >= UPDATE_REFRESH_DOM AND state.model_ptr != 0:
        counter = *((u32*) state.model_ptr)
        if state.patch_buf_ptr == 0:
            state.patch_buf_ptr = AzStartup_alloc(32)
        used = AzStartup_buildCounterPatch(
            state.patch_buf_ptr, 32,
            state.display_text_node_idx (M9-6: hardcoded 1),
            counter)
        *outLenPtr = used
        return state.patch_buf_ptr
   g. otherwise:
        *outLenPtr = update    (diagnostic)
        return 0

   ─── back in JS ───
4. patches_len = *(u32) outLenPtr
5. if patches_ptr != 0 and patches_len > 0:
     azApplyPatches(patches_ptr, patches_len)
       — decodes TLV: kind=1 (SetText) → element.textContent = text
6. mini.AzStartup_free(evtPtr, 256)
   mini.AzStartup_free(outLenPtr, 4)
```

The `patch_buf_ptr` lives for the eventloop's lifetime — JS
reads-then-applies before the next dispatch overwrites it.
No double-free.

### What's STUBBED in this flow

  - `AzStartup_hitTest` returns the last registered cb node; it
    does not consult any layout cache. For multi-cb DOMs this
    routes every click to the most-recently-registered cb,
    which is wrong.
  - `state.display_text_node_idx` is hardcoded by JS to `1`
    (the counter div in hello-world). Real demos would need
    the wasm side to discover text-bearing nodes by walking a
    populated StyledDom.
  - `state.model_ptr` assumes the user's data is a `u32` at
    offset 0 of the model. Hello-world specific.

Closing these stubs requires the WASM-resident StyledDom +
layout cache (a separate, larger piece of work — see the
"User intent vs implementation" section at the top).

## In-process pipeline (M8.9)

The native pipeline lives in `dll/src/web/cpp/azul_remill.{cpp,h}`
(C++ wrapper) + `dll/src/web/native_remill.rs` (Rust FFI) + the
`web-transpiler-static` Cargo feature.

C-ABI surface:

  - `az_remill_lift` — single-fn lift (debug / standalone test).
  - `az_remill_lift_batch` — N-fn lift sharing one
    `LoadArchSemantics`; output is N separate per-fn IR strings.
    See `lift_batch_inner` for the shared-`LiftMemory` +
    multi-entry `SimpleTraceManager` setup.
  - `az_remill_compile_to_wasm32_obj` — takes an array of LLVM IR
    strings, parses each into its own Module, merges via
    `llvm::Linker::linkInModule`, then runs `opt -O2` via
    `PassBuilder` + `llc` via the legacy `PassManager`.
  - `az_remill_wasm_link` — invokes `lld::wasm::link` with
    `--gc-sections --strip-all --lto-O2 --no-entry --allow-undefined
    [--import-memory] [--import-table] --initial-memory=N
    --export=...`; the output wasm is read back into a heap buffer.
  - `az_remill_free{,_buf}` — release strings / byte buffers.

Thread safety: `FFI_LOCK` in `native_remill.rs` serializes every
FFI call. LLVM's `TargetRegistry` is read-only after
`initialize_llvm_targets()`, but `lld::wasm::link` uses
CommandLine globals that aren't reentrant. Per-fn `LLVMContext`
instances are local to each call — they could be parallelized if
`FFI_LOCK` were dropped for `compile_to_wasm32_obj` (deferred —
needs LLVM thread-safety audit).

Static link line: see `dll/build.rs::build_remill_link_libs`. ~110
static archives, ~95 MB linker input, produces a 130 MB
`libazul.dylib`. Build host requirements: macOS arm64 (with the
`vcpkg_macos-13_llvm-17-liftingbits-llvm_xcode-15.0_arm64` cxx-common
bundle in `third_party/cxx-common/`) or Linux x64/arm64 (bundle path
mirrored in `build.rs`; not runtime-tested yet on Linux).

The pipeline is enabled by:

  - **build-time**: `--features web-transpiler-static` (default off
    on the `web` feature — the dylib is 35 MB without static link).
  - **run-time**: `AZ_NATIVE_REMILL=1` env var. Without it, the
    pipeline falls back to subprocess `remill-lift-17` + `opt` +
    `llc` + `wasm-ld` as it did pre-M8.9.

Post-link: every wasm runs through binaryen `wasm-opt -Oz
--strip-debug --strip-producers --vacuum` (best-effort — skipped
silently if binaryen isn't installed; override with
`AZ_REMILL_SKIP_WASM_OPT=1`).

## What's bypassed / status post-M9

Updated for M9 close-out. ✓ = wired by M9, ✗ = still bypassed,
〜 = wired but stub.

| built | wired? | notes |
|---|---|---|
| `AzStartup_dispatchEvent` | ✓ (M9-6) | now uses hydrated `state.refany_ptr`, calls `__az_call_indirect` with real refany; on `RefreshDom` emits `SetText` TLV via `buildCounterPatch`. The legacy `azInvokeCbDirect` JS path is DELETED. |
| `AzStartup_registerStateDeserializer` | ✗ | hydration still uses `AzStartup_hydrate`'s hand-rolled path. Lifting the user's `_fromJson` would close this but needs the user-binary data-section mirror to succeed (the `_fromJson` body reads user-binary const strings). |
| `/az/layout/<name>.<hash>.wasm` | 〜 (M9-2/3a) | instantiated by `loader.js`. Reserved table slot. `AzStartup_initLayoutCache` invokes via `__az_call_indirect_layout4`. **For hello-world-minimal works end-to-end + writes a real `AzDom` blob; for full hello-world.c traps in lifted libazul `__const` reads.** No `Dom → StyledDom` conversion happens yet. |
| `azApplyPatches` TLV decoder | ✓ (M9-5) | `loader.js` decoder gets real `SetText` TLVs from `AzStartup_buildCounterPatch`. Other TLV kinds (`SetAttr`, `InsertNode`, …) deferred until a real `Dom`-diff lands. |
| WASM-side hit-test | 〜 (M9-4) | `AzStartup_hitTest` exported; routed through `AzStartup_dispatchEvent` when JS encodes `SENTINEL`. **Stub: returns `state.last_registered_cb_node_idx` regardless of (x, y).** Real bbox walking needs the StyledDom + LayoutWindow embed. |
| WASM-resident StyledDom | ✗ | `state.current_dom_ptr` stores a raw `AzDom` blob from the cb but never gets converted to `StyledDom`. No layout cache. The "WASM DOM" is a placeholder. |
| User-binary data-section mirror | 〜 (M9-3b) | scaffolding shipped: `SymbolTable::enumerate_low32_data_for_wasm` + `patch_wasm_add_data_segments`. Filter is `<1 MiB` to fit under the bump heap; typical macOS ASLR slides are multi-MiB so most runs get zero matches. Real fix is at the lift, not the mirror. |
| WASM-side hit-test | ✗ | JS-side `azNodeIdxFromEvent` regex on `id="az_N"` IDs |
| `POST /az/exec/<node_id>` server fallback | ✗ | server-side path exists but loader doesn't fall back to it |
| `EventloopState.current_dom` | ✗ | always `None`; no `HydrationPayload` is serialized into the HTML even though `dll/src/web/hydration.rs` defines the shape |
| `HeadlessApp` + `LayoutWindow` cache | ✗ | `HeadlessApp::new()` is dead code; every HTTP request re-runs the layout cb. No layout cache, no font manager init, no hit-tester. |
| DOM tree navigation in WASM | ✗ | no `getParent` / `getChildren` / `findById` exports in `EVENTLOOP_SYMBOLS`. Native StyledDom has the data; the wasm boundary doesn't expose it. |

The full catalog of remaining hacks (19 items grouped into 5
categories) is in
[`scripts/HACKS_REVIEW_2026_05_16.md`](../../../../scripts/HACKS_REVIEW_2026_05_16.md);
the M8.9-era status snapshot is in
[`scripts/STATUS_REPORT_2026_05_18.md`](../../../../scripts/STATUS_REPORT_2026_05_18.md);
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
GET  /                              → pre-rendered HTML (~20 KB)
GET  /az/loader.js                  → bootstrap JS (inline-embedded)
GET  /az/mini.{hash}.wasm           → mini wasm (~2.7 KB)
GET  /az/cb/{name}.{hash}.wasm      → per-cb wasm (~7 KB)
GET  /az/layout/{name}.{hash}.wasm  → per-layout wasm (~285 KB for hello-world)
GET  /az/img/{id}                   → image bytes
GET  /az/font/{id}                  → font bytes
POST /az/exec/{node_id}             → server-side fallback dispatch
```

The `/az/` prefix is the only reserved namespace. Any other path
is matched against registered routes.

Hello-world total wasm payload (post wasm-opt -Oz): 295 KB.

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
