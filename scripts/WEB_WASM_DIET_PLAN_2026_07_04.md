# azul-mini.wasm diet — architecture plan (2026-07-04)

**Status: PLAN ONLY — no implementation.** Companion doc:
`scripts/RELEASE_SIZE_MEMORY_AUDIT_2026_07_04.md` (release-artifact size +
RSS audit of the same date; its native-size findings feed the estimates
here, since lifted wasm scales ~1:1 with the native bytes fed to remill).

## 1. Problem statement

The web backend (`AZ_BACKEND=web://`) lifts the ~40 `EVENTLOOP_SYMBOLS`
(`AzStartup_*`, `dll/src/web/mod.rs:60`) plus their transitive closure out of
the running libazul via remill at server startup and links them into one
`azul-mini.wasm`. Measured sizes on record:

- `scripts/WEB_1TO1_SUPERPLAN.md:949` — 27 MB (un-opt'd, wasm-opt fell back)
- `scripts/WEB_1TO1_SUPERPLAN.md:975` — 26 MB (same fallback)
- current user-observed: ~25 MB, lazy-loaded but far too large
- original budget: **< 500 KB** (`scripts/M8_ARCHITECTURE_2026_05_19.md:709`)

The transitive graph is ~1,100 → ~1,444 fns (x86 devirt growth,
`scripts/HANDOFF_FABLE_web_lift_x86_windows_2026_06_13.md:947`), up to
~1,684 fueled fns. The native source it lifts is essentially *the whole
layout framework*: `azul_layout + azul_core + azul_css + allsorts + taffy +
alloc + core + hashbrown` ≈ **9.6 MiB of native text** plus the const data
they reference (hyphenation dictionaries alone are ~2.8 MiB, see audit
§2.3). Remill-lifted, `opt -O2`'d, `wasm-ld --lto-O3`'d code lands at
roughly 1–2× native bytes ⇒ 25 MB is the *expected* output of the current
architecture, not an anomaly. Getting to sub-MB requires cutting **what is
lifted**, not squeezing **how it is lifted**.

Three levers, in order of effort:

- **L0** — recover the losses in the existing pipeline (opt fallback, wire
  compression). No architecture change. Est. 25 MB → ~4–6 MB *on the wire*.
- **L1** — classifier-level subsystem cuts + browser-native substitution
  (image decoding, hyphenation data, font tables). Est. module 25 MB → ~10–14 MB.
- **L2** — per-app reachability at server startup ("ship what THIS app can
  execute"). Est. → 1–5 MB for typical apps.
- **L3** — module layering + per-fn shards (existing `BoundaryImport`
  design turned on). Est. initial payload → < 500 KB, rest on demand.

These compose: L0 applies to whatever L1–L3 produce.

---

## 2. L0 — stop shipping known waste (pipeline fixes, no redesign)

### 2.1 wasm-opt fallback is (or was) eating the whole -Oz pass

`postprocess_wasm_opt()` (`transpiler_remill.rs:5633`) runs
`wasm-opt --enable-bulk-memory -Oz --strip-debug --strip-producers --vacuum`
but **falls back to the un-opt'd module on any error**
(`:5659-5660`), and both recorded 26/27 MB numbers were captured during
exactly that fallback ("error validating input"). Every future size number
is meaningless while this silently degrades.

Plan:
1. At server startup, log wasm-opt version + run result at WARN (not
   buried), and export the outcome into the `/az/` status JSON so e2e can
   assert "opt ran".
2. Reproduce the validation error with the archived module; likely causes:
   binaryen version too old for a feature wasm-ld emits (bulk-memory
   `memory.copy`, sign-ext, nontrapping-fptoint from LLVM 21) → pass
   `--all-features` (or explicit `--enable-sign-ext
   --enable-nontrapping-float-to-int --enable-mutable-globals`) instead of
   only `--enable-bulk-memory`, and pin a minimum binaryen version in
   `WEB_LIFTER_INSTALL.md` + discovery probe.
3. Historical data says post-link `-Oz` on lifted code shaves 10–20 %
   (comment at `transpiler_remill.rs:1837-1841`); per-callback experiments
   saw far more (50–200 KB → <2 KB per leaf cb,
   `scripts/WEB_BACKEND_PLAN_2026_05_18.md:282`) because `%struct.State`
   spill/reload chains are what -Oz folds best. Expect the real number on
   the mini to be north of 20 %. (This is the *lifted* module — native-side
   "don't use -Oz" perf concerns do not apply; wasm-opt -Oz here removes
   redundant State traffic, it does not deoptimize hot loops the way
   rustc -Oz would.)

### 2.2 The wire: 25 MB is served raw

`server.rs:220-221` serves `/az/mini.{hash}.wasm` via
`send_response_cached` with **no `Content-Encoding`** — grep of
`server.rs`/`loader_js.rs` finds no brotli/gzip anywhere on the response
path (the `brotli_decompressor` dep is for *embedded assets*, unrelated).
Lifted wasm is extremely repetitive (State-struct load/store idioms) and
brotli-compresses ~4–6×.

Plan:
1. Brotli-encode (`q=9`... `q=11` offline since the module is immutable
   per server run — compress once at startup, cache both raw + `.br`).
2. Honor `Accept-Encoding`; keep raw fallback.
3. Client: `WebAssembly.instantiateStreaming(fetch(...))` if not already —
   decode+compile overlap matters at this size.
4. Also applies to per-cb shards and `app.wasm`.

Est.: 25 MB → ~4–6 MB transfer with zero module changes. This alone
probably ends the "way too large" pain for LAN/dev use, but does not fix
compile time or memory on low-end clients — L1/L2 still needed.

### 2.3 Names/custom sections

`wasm-ld --strip-all` is already passed (non-debug), but verify the
served module really has no `name` section when `AZ_REMILL_DEBUG` paths
are off — a 1,444-fn lifted module with mangled+hashed names carries
hundreds of KB of names. `wasm-objdump -h` on the served bytes in an e2e
check, not just trust in flags.

---

## 3. L1 — cut subsystems the browser already provides (classifier-level)

Mechanism already exists and is proven: `FnClass` in
`dll/src/web/symbol_table.rs` (`classify_for_name`, `:2330`) already
Leaf-stubs whole subsystems (`display_list` painters ~300+ fns,
`:2440-2442`; probe; panic family → `NeverLift`) and already synthesizes
JS-backed helper bodies for allocator/libc ops (`BumpAlloc`,
`LibcMemcpy`, …). L1 = extend the same two tools:

- **`Leaf`** (stub, returns through) for things that must never run
  client-side, and
- a new **`JsImport`** class (synthetic body = call an imported JS
  function, marshal via linear memory) for things the *browser* provides.

### 3.1 Image decoding → browser-native (`JsImport`)

Native azul decodes png/jpeg/webp/gif/tiff via `image`/`zune_jpeg`/
`image_webp`/`tiff` (~1.1 MiB native text in the audit). The browser has
`fetch` + `createImageBitmap` (and `<img>` decode) with hardware codecs.

Plan:
1. Classify the image-decode entry points (`azul_layout::image::decode*`,
   `image::*`, `zune_*`, `image_webp::*`, `tiff::*`, `png::*`) as cut
   points: everything *below* them → never lifted.
2. Server already serves originals at `/az/img/{id}` (web.md flow). Client
   decodes via `createImageBitmap` → writes RGBA (or keeps it GPU-side for
   the DOM-patcher path — images in the HTML render are just `<img>`
   anyway; the wasm only ever needs *dimensions + existence*, not pixels,
   until somebody paints into a canvas).
3. The wasm-side need is exactly: `fn decode(bytes) -> (w, h, ptr)`. For
   layout, `(w, h)` alone usually suffices → ship an even cheaper
   `image_probe` import that answers from the server-computed manifest
   (the server has already decoded every image during Phase D pre-render;
   it can emit `{url → w,h}` into the hydration JSON for free — then image
   decoding contributes **zero** wasm and zero client work).

### 3.2 Font parsing/shaping → precomputed server-side tables

`allsorts_azul` + `azul_layout::font` + the text3 shaping cache are the
single biggest *legitimate* closure members (text shaping is needed for
relayout after DOM patches). But the *parsing* half (`ParsedFont::from_bytes`,
cmap/glyf/loca/GSUB decode) does not need to run in wasm at all:

1. Server startup already parses every font (it laid the routes out
   natively). Serialize the *shaping-relevant* results — glyph advances,
   kerning pairs actually present, cmap as a flat range table, space/x-height
   metrics — into a compact binary blob per font (the audit shows the
   parsed-font heap cost natively is dominated by a 21.4 MiB boxed `glyf`
   table; the blob must NOT include outlines — the browser rasterizes text,
   wasm only measures).
2. Client fetches `/az/fontmetrics/{hash}` lazily per font actually used
   (the `WEB-FONT-VIA-JS` fallback-font hook, `mod.rs:63`, is the
   precedent: JS already registers font buffers into wasm memory).
3. Classifier: `allsorts_azul::*` + `font::parsed::ParsedFont::from*` →
   cut point; keep the *lookup* structs (`get_hinted_advance_px` reads a
   table) Recursable.
4. Fallback ladder for correctness: browsers can also measure via
   `canvas.measureText` — acceptable for latin UI text, wrong for complex
   scripts; keep it as a debug-compare tool, not the primary path.

Note the `web_lift` feature already stubs the NAME-table decode and glyph
*outlines* (`layout/Cargo.toml` `web_lift` comment; HANDOFF 2026-06-03
confirmed `into_outlines` 0× lifted) — this plan extends an existing
pattern from "skip broken decode paths" to "skip parsing entirely".

### 3.3 Hyphenation dictionaries: 2.8 MiB of const data

`hyphenation` is built with `embed_all` (`layout/Cargo.toml:48`) → ~70
languages of patterns land in `__TEXT,__const` and get dragged into wasm
data segments the moment any text path references them (crate is
`Recursable` by default — not on the Leaf list in `classify_for_name`).

Plan: cut `hyphenate*` as a `JsImport` (CSS `hyphens: auto` /
`Intl.Segmenter` cover the browser side) **or** Leaf-stub to
"no hyphenation points" (text still lays out, just doesn't hyphenate) —
each locale's dictionary becomes a lazily fetched asset if real
hyphenation-in-wasm is ever needed. Also fix natively: default feature
should embed en-US only (see audit §2.3 — this is a native win too).

Same class, bigger on Linux servers: **ICU4X baked data**
(icu_segmenter 4.0 + icu_datetime 3.7 + icu_collator 1.1 MiB const data
in the Linux dylib the server lifts from, audit §2.10c). Any text3 path
that touches icu segmentation drags the referenced tables into wasm data
segments. Browser-side `Intl.Segmenter`/`Intl.DateTimeFormat` are the
natural `JsImport` substitutes; the audit's "externalize the blob"
native fix shrinks the lift input for free.

### 3.4 Things that must be *proven absent* (guard tests)

turso/SQLite (~2.1 MiB), printpdf/lopdf, rustls/ureq, regex (~0.46 MiB),
`encoding_rs` big tables — none should be reachable from
`EVENTLOOP_SYMBOLS`, but nothing today *asserts* that. The lift log
already prints every lifted fn; add a startup **deny-list check**: if any
lifted fn matches `turso_|printpdf|rustls|regex_|lopdf|ureq`, log ERROR
with the root-path (the BFS knows the parent chain). Cheap, catches
closure regressions the day they happen instead of at the next size audit.

### 3.5 Per-fn size accounting (measurement infra, do this first)

One-line change class: the transitive lift already logs
"lifted N fns" (`transpiler_remill.rs:2390,2942`); extend to log
`(fn_name, native_bytes, lifted_wasm_bytes)` per item + a per-crate rollup
at startup, and dump to `AZ_LIFT_REPORT=path.json`. Every estimate in
this plan becomes a measured number at the next server start, and the
report is diffable in CI. **All L1/L2 work should be sequenced by what
this report says, not by the estimates above.**

---

## 4. L2 — per-app reachability at server startup ("don't ship what the app can't call")

Key observation: at server startup the server has *perfect knowledge* of
the app: Phase D pre-renders every route and collects every callback
fn-pointer (`run_web` → `discovered_per_route`), the layout callback is
dladdr-resolvable, and the `SymbolTable` (M8.8) holds the full
addr→name→bytes call graph. Today that knowledge is unused: the mini
always lifts the union of *everything the eventloop could ever do*.

### 4.1 Root-set construction

Roots for THIS app =
- the app's layout callback(s) + all discovered event callbacks,
- the `AzStartup_*` entry points *actually addressable by the client
  loader* (hydrate/dispatch/relayout/patch); NOT the debug/diagnostic ones
  (`peekU32`, `getCascadeProbe`, `pokeLastLayout`, …) unless
  `AZ_WEB_DEBUG=1` — the EVENTLOOP_SYMBOLS list is ~40 entries of which a
  third are M9–M12 diagnostics that ship to every user today.

### 4.2 Closure pruning passes (cheap, static, on the SymbolTable BFS)

1. **Dead-diagnostic pruning** (above) — free.
2. **CSS API usage**: if no discovered callback's closure touches
   `azul_css::parser*` / property reflection (`CssProperty::from_str`,
   `interpolate`, …), those subtrees never enter the lift. The cascade
   itself (StyledDom::create) is only needed wasm-side if a callback can
   *mutate styles or DOM structure*; a pure `SetText`-patching app (the
   hello-world counter!) needs text relayout but not the full cascade —
   the classifier can key on "which `Update`/patch kinds can this app's
   callbacks emit", conservatively derived from the closure (does it call
   `Dom::create*`? `set_inline_style`? `RefreshDom` vs `DoNothing`?).
3. **Widget pruning**: widgets are monomorphized DOM builders; only the
   ones in the app's closure lift.
4. **Report before/after**: reachability report (§3.5 format) diffed
   against the full-mini baseline, printed at startup:
   `"[azul-web] app closure: 214 fns / 1.9 MB (full mini: 1444 fns / 25 MB)"`.

### 4.3 Correctness spine: what must ALWAYS ship

hydrate + dispatch + hit-test + patch-builder + text measurement for the
fonts in use + allocator shims. That set is the "boot module" of L3 and
the floor of L2 (~a few hundred KB by the M10-E evidence: per-fn shards +
shared runtime got hello-world's cb+layout to 13.4 kB).

### 4.4 Risk: dynamic dispatch escapes static closure

Function pointers stored in RefAny state (timers, threads, dataset
callbacks) can escape the BFS. Mitigations:
- the same mechanism callbacks already use: every `extern "C"` fn the app
  can register flows through known registration APIs (`set_on_*`,
  `Thread::create`, `Timer::*`); Phase D walk + api.json give the full
  registration surface → collect fn-ptrs from *values* too (the discovery
  walk already does this for DOM callbacks).
- runtime backstop: a lifted-code `missing_block` trap on an un-lifted
  address triggers a **lazy lift-and-fetch** of that one function (the
  shard pipeline exists; this is its natural error path) + WARN log. So a
  missed root degrades to one extra round-trip, not a broken app.

---

## 5. L3 — layering + on-demand shards (turn the existing design on)

`FnClass::BoundaryImport` + per-fn shards (M10-D, `AZ_ENABLE_SHARDS`,
`symbol_table.rs:113-123,2477-2496`; `BoundaryShard`,
`transpiler_remill.rs:4316-4345`) already implement "each framework fn =
its own wasm module, imported across boundaries". M10-E measured the
shared-runtime variant at **13.4 kB** for the v5 hello-world set. The plan
is not to invent lazy loading — it is to promote this from experiment to
default, with a small number of *bundles* instead of thousands of tiny
files:

- **boot.wasm** (< 500 KB target): hydrate, dispatchEvent, hit-test,
  TLV/patch builder, allocator + libc shims. Loads with the page.
- **layout.wasm**: solver3 + text3 measurement (+ fontmetrics fetch).
  Loaded lazily on the *first event whose callback can mutate layout*
  (until then, server HTML is inert; first-interaction latency hides the
  fetch, and §2.2 brotli makes it ~1–2 MB).
- **cb-*.wasm**: per-callback shards (already the design for app code).
- **rare.wasm**: everything else L2 kept (grouped by "reachable only
  from" analysis), fetched on `missing_block` trap (§4.4 backstop).

Bundle granularity beats per-fn: HTTP/2 makes many small files *possible*,
but per-fn shards lose wasm-ld's cross-fn `--gc-sections`/LTO and repeat
the State-struct glue; group by call-graph community instead (the BFS
already has the edges; a simple "dominator = AzStartup entry" grouping is
enough for v1).

Prereq (from M10-D notes): the sharded path must go e2e green; the known
blocker list lives in `scripts/` M10-D/M10-E notes.

---

## 6. Sequencing & acceptance gates

| step | what | gate | est. effect on served bytes |
|---|---|---|---|
| 0 | §3.5 per-fn size report + §3.4 deny-list | report exists, e2e asserts wasm-opt ran | (measurement) |
| 1 | §2.1 fix wasm-opt fallback | mini validates, -Oz applied | −10…25 % module |
| 2 | §2.2 brotli + streaming instantiate | `content-encoding: br` on /az/*.wasm | ÷4…6 wire |
| 3 | §3.1 image-decode cut (+ dims-from-server manifest) | reftest: pages with png/jpg/webp render + relayout OK | −1…3 MB module |
| 4 | §3.3 hyphenation cut | text reftests, no hyphenation regressions on web | −2.8 MB data |
| 5 | §3.2 font-parse cut + fontmetrics blob | text3 measure parity harness (native vs wasm advance sums) | −2…5 MB module |
| 6 | §4 per-app closure | hello-world mini ≤ 2 MB; widgets demo ≤ 8 MB | app-dependent |
| 7 | §5 boot/layout split, shards default-on | boot ≤ 500 KB; TTI unchanged on demo | first-load ≤ 500 KB |

Measurement discipline: every step lands with the §3.5 report diff pasted
into the PR description, from the same machine/toolchain, and e2e
(`scripts/mechb_harness/`-style click-through) green — the 2026-06-13
lesson stands: *more faithful lifting grows the graph*; only the report
tells you what a change really did.

## 7. Non-goals / rejected

- **rustc `-Oz`/opt-level tuning of the native dylib to shrink the lift
  input** — tried, net regression (new unlifted jump-tables/instructions;
  see `Cargo.toml` `[g140 az-web-lift]` note). Lift fixes are tuned to
  opt-3 codegen. All cutting happens in the *classifier/pipeline*, never
  by changing native codegen.
- **Hand-written wasm runtime** ("just compile azul to wasm32 directly").
  Deliberately out of scope: the whole point of the remill path is running
  the *same audited native bits*; a `wasm32-unknown` port is a different
  product with its own maintenance surface. Revisit only if L0–L3 land
  and the residual is still unacceptable.
- **Serving api.json / bindings to the client** — never needed; the
  client speaks the TLV patch protocol only.
