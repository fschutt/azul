# Web backend → 1:1 with desktop — architecture & plan

**Date:** 2026-06-10 · **Branch:** `web-lift-text-layout` (13+ commits on top of `mobile-ios-android`)
**Status of the lift itself:** working — hello-world lays out, text shapes, the on_click counter
round-trips through a real browser. This plan is about closing the gap from "the counter works"
to "the web backend runs the azul App **1:1 like the desktop app**."

---

## 0. The mental model

> **The WASM module *is* the azul App.** JS is a thin host.

The desktop app and the web app run the **same** code inside WASM: the event loop
(`process_window_events`), the cascade (`StyledDom::create`), the layout solver
(`layout_dom_recursive`), the display-list builder (`generate_display_list`), timers, and
threads. The only thing that differs is the **final "renderer"**: desktop emits WebRender/CPU
draw commands; **web emits DOM patches.** Everything upstream of the renderer is identical — that
is what "1:1" means and why lifting (not reimplementing) is the right call.

JS provides five host services, nothing more:
1. **Event source** — DOM events → the fixed binary event buffer → `AzStartup_dispatchEvent`.
2. **DOM applicator** — the TLV patch stream → real DOM mutations (`azApplyPatches`).
3. **Timer host** — `setTimeout`/`setInterval` → "timer N ticked" messages into WASM.
4. **Worker host** — `Web Worker` per azul `Thread`, running a wrapper that calls the wasm thread entry.
5. **Clock + host services** — injected at the IR layer (see §6b), NOT threaded as a callback. The
   symbol scan detects `Instant::now` (mandatory — lifted it returns 0 on wasm, `core/src/task.rs:876`,
   so timers/animations never advance), `AzHttp_*`, and device APIs, and emits a WASM→JS→WASM bridge.
   User code is oblivious.

### The two-tier signal that the loop must mirror (do not collapse it)
Azul has **two** signals, and the web loop must carry both (agent-confirmed):
- **`Update`** (`core/src/callbacks.rs:77`) — the user-facing callback return: `DoNothing /
  RefreshDom / RefreshDomAllWindows`. Means "re-run the user's `layout_callback` and rebuild the DOM?"
- **`ProcessEventResult`** (`core/src/events.rs:82`) — the internal 7-tier granular signal,
  computed by merging `Update` with the per-`CallbackChange` results. This is what actually
  decides **repaint vs display-list-rebuild vs incremental-relayout vs full-DOM-regen**:
  `DoNothing < ShouldReRender < ShouldUpdateDisplayList < ShouldIncrementalRelayout <
  ShouldRegenerateDomCurrentWindow < …AllWindows`.

A callback returning `DoNothing` can still force a relayout if it pushed a `CallbackChange`
(e.g. `set_css_property`). **`CallbackChange` (`layout/src/callbacks.rs:167`, ~60 variants) is the
natural patch boundary** — the desktop applies them via `apply_user_change`; the web translates
each into DOM patches + a `ProcessEventResult` tier.

---

## 1. Where we are vs. where this goes

| Piece | Now | Target |
|---|---|---|
| Event **transport** (JS→wasm) | ✅ fixed 256-byte header + variable payload; click/mouse/key/focus/input/scroll/resize wired | + wheel/touch/drag/composition; variable-width TLV for text/paste |
| Patch **transport** (wasm→JS) | ✅ TLV decoder complete (12 kinds, `azApplyPatches`) | unchanged — it's ready |
| Patch **producer** (wasm side) | ❌ ONE hardcoded counter `SetText` (`eventloop.rs:2135`) | real diff → patch stream |
| Event **loop** | ❌ `dispatchEvent` = hit-test→invoke cb→counter | mirror `process_window_events` + `CallbackChange` transaction |
| wasm **StyledDom** | ❌ `Box<StyledDom>` reads back zero (m11 box-new gap); only an AzDom blob + rects array | a readable, queryable StyledDom (old + new) |
| **diff** | ❌ none (`reconcile_dom` exists in `core/src/diff.rs`, fully liftable, **unwired**) | `reconcile_dom` + `compute_node_changes` → `PATCH_KIND_*` |
| **CallbackInfo** to cb | ❌ 512-byte zero blob (`buildLayoutInfo`) — cb can't read the event | marshal event (key/mouse/text) so real callbacks work |
| **Timers** | ❌ none | `setInterval(tick_millis)` → `run_single_timer` |
| **Threads** | ❌ none | Web Worker + web-native `create_thread` (NOT the libstd lift) |
| **Visual layer** (clip/image/transform) | ❌ static cascade only; no dynamic visuals | display-list deltas → CSS patches |
| Hit-test | ⚠ geometry now authoritative but layout has bugs (button 740px vs 121px) | 1:1 geometry (depends on §6 decision) |

---

## 2. The node-identity invariant (the linchpin — make it explicit & tested)

Everything keys on `node_idx == az_N`. Today **three independently-computed index spaces happen
to coincide** by an unstated invariant:
- `html_render.rs` synthetic pre-order DFS counter `az_N` (write side; `getElementById('az_'+N)`).
- StyledDom **flat-arena `NodeId`** (`solveLayoutReal` rects + `hitTest` return).
- `core/src/diff.rs`'s new-DOM `NodeId` (the future diff output).

These are equal **only because azul's `CompactDom` arena is stored in DFS pre-order** — so
DFS-visit-order == ascending arena index (agent-confirmed: `html_render.rs:300-301` bumps the
counter per node in `first_child`/`next_sibling` order; text nodes consume a slot but emit no
element). **Action:** promote "arena `NodeId` == `az_N`" to an explicit, asserted contract (a
debug check in `html_render` + a test), so anonymous-box insertion or layout-tree reordering can't
silently break patch targeting.

---

## 3. Phase plan

### Phase 0 — dev infrastructure (DONE / in progress; the user's "first")
- **0.1 Lift cache — ENABLED + engine-aware.** `web_relift.sh` now passes `AZ_LIFT_CACHE=1`; the
  cache key (`lift_cache_path`) is `fnv(asm bytes) + lift_addr + VERSION + engine_fingerprint`,
  where `engine_fingerprint` = (len, mtime) of `remill-lift-17`. So: only functions whose machine
  bytes changed re-lift; an engine swap auto-invalidates; `LIFT_CACHE_VERSION` bumps only for
  transpiler-source changes. First relift after this is a cold full lift, then incremental.
  *Follow-up:* fold a hash of the azul source rev / transpiler crate into the fingerprint too.
- **0.2 Preflight "clean-lift" gate (TO BUILD).** After lifting, scan every per-fn `.lifted.ll`
  (or the linked module) for `call ptr @__remill_error` and `@__remill_missing_block`. Report, per
  function: count + the guest PC of each (decode via the existing synth→native map). Gate behind
  `AZ_PREFLIGHT=1`; optionally fail the server start if any non-whitelisted function is unclean.
  This surfaces silently-incomplete lifts (undecoded NEON, unrecovered jump tables) **before**
  runtime — the exact class of bug that cost this project weeks. The grep method is proven (memory
  `web_vec_len_mislift_systemic_2026_06_06`); this just automates it across the whole closure +
  attributes each error to a symbol.

### Phase 1 — the real event loop in WASM
Replace the hardcoded counter in `AzStartup_dispatchEvent` with the desktop chokepoint, lifted:
`determine_all_events` → `dispatch_events_propagated` (W3C capture/target/bubble) →
`invoke_single_callback` (`(callback.cb)(data, info)`) → collect `(Vec<CallbackChange>, Update)` →
`apply_user_change` per change → merge into `ProcessEventResult`. The wasm holds the authoritative
`LayoutWindow` (timers/threads maps, focus, scroll). Anchors: `common/event.rs:3542 / 2934 / 1146`,
`window.rs:4105`.

### Phase 2 — wasm StyledDom + diff → patches
1. Make the StyledDom **readable** wasm-side. Two routes: (a) fix the `Box<StyledDom>` readback
   (the m11 box-new lift gap), or (b) keep the StyledDom as discrete already-lifting Vecs
   (`node_data`, `node_hierarchy`, `css_property_cache`) addressed directly, never as one boxed
   value. (b) is lower-risk and matches how `solveLayoutReal` already reads.
2. Capture `prev` (node_data + layout) before a `RefreshDom` relayout (`prev_dom_ptr` field exists,
   unused).
3. Wire `core/src/diff.rs` (no_std+alloc, **fully liftable, currently unwired**):
   `reconcile_dom` → `DiffResult` + `compute_node_changes` → `NodeChangeSet` per node. Map flags →
   `PATCH_KIND_*`: `TEXT_CONTENT`→SetText, `IDS_AND_CLASSES`→Add/RemoveClass,
   `INLINE_STYLE_*`→SetInlineStyle, `IMAGE_CHANGED`→(image patch), `CHILDREN_CHANGED` /
   `NODE_TYPE_CHANGED`→Insert/Remove/Replace. Emit via the existing `AzStartup_buildPatch`
   (exported, complete, never called). Delete the `buildCounterPatch` special-case.

### Phase 3 — the visual layer (clip-paths, images, transforms — "WASM does CSS, JS applies")
The display list (`layout/src/solver3/display_list.rs:303`) already resolves every visual to
concrete pixels. Map `DisplayListItem`s → DOM/CSS on the `az_N` element:
- **Clip** = `PushClip{bounds, border_radius}` → CSS `clip-path: inset(...)` / `border-radius` /
  `overflow:hidden` (everything is reduced to rect+uniform-radius; **no polygon clips** in the
  list — SVG `path()` clip returns `None`, agent-confirmed).
- **Image** = `Image{bounds, ImageRef, border_radius}` → `<img>`/`background-image` at computed
  bounds; `ImageRef`→`DecodedImage::Raw`/`Gl` is where bytes/texture come from. Served like
  `/az/img/<id>`. This is the user's "WASM calculates which images, JS applies."
- **Border** (width/color/style/radius), **background** (color/gradient), **box-shadow**,
  **opacity**, **transform** (a resolved 4×4 matrix → CSS `matrix3d(...)`) — all map cleanly to CSS.
- These ride the **same TLV** as `SetInlineStyle` patches (a per-node computed-style delta), so
  Phase 2's diff just gains a "computed-visual" comparison alongside the DOM-structure comparison.
- **⚠ Text is glyphs, not strings** (agent-confirmed): the display list hands JS glyph IDs at xy +
  font_hash + size + color; the original string survives only in the type-erased `UnifiedLayout`.
  **Decision needed (§6):** thread the source string out for real DOM text, or position glyph spans.

### Phase 4 — timers & threads (1:1 with the desktop drivers)
- **Timers.** Mirror `process_timers_and_threads` (`common/event.rs:4314`). Per `add_timer`
  CallbackChange, JS does `setInterval(timer.tick_millis())` (default 10ms); on fire it calls a new
  `AzStartup_tickTimer(state, timer_id)` = `run_single_timer` (`window.rs:3890`) → apply changes +
  Update. `Timer::invoke` self-gates on elapsed time (so jitter is fine) and auto-emits
  `RemoveTimer` on timeout. Honor reserved IDs (system `0x0001-0x0004`, user `0x0100`). **Requires
  the JS clock** (§0).
- **Threads = Web Workers.** `Thread`'s libstd path (`std::thread::spawn` + mpsc) does NOT lift —
  supply a **web-native `create_thread`**: spawn a `Worker` running a wrapper that calls the wasm
  `ThreadCallbackType(RefAny, ThreadSender, ThreadReceiver)`; mirror `ThreadSender::send` as
  `postMessage`; on the main side mirror `run_all_threads` (`window.rs:3982`) — `WriteBack` →
  `WriteBackCallbackType(writeback_data, sent, info) -> Update` on the main thread; worker-close →
  `RemoveThread`. Payloads cross by structured-clone of the RefAny bytes.

### Phase 5 — full event coverage + CallbackInfo marshaling
- Marshal the event buffer into the `CallbackInfo` the cb reads (today `buildLayoutInfo` is a
  512-byte zero blob → callbacks can't see key/mouse/text; the counter only works because
  `on_click` ignores the event). Without this, keyboard/text/input callbacks are inert.
- Variable-width TLV for input/paste/composition; wheel/touch/drag.

---

## 4. The wire protocol (already sound — documented for completeness)
- **Event in:** `[node_idx:u32 | x:f32 | y:f32 | button_or_key:u32 | modifiers:u32]` + per-kind
  payload past offset 20. `node_idx = SENTINEL` → wasm hit-test. (`event_offset`, `eventloop.rs:143`.)
- **Patch out (TLV):** `[kind:u8 | node_idx:u32 LE | payload_len:u32 LE | payload]`, repeated.
  Kinds 1-12 (`eventloop.rs:1814`); decoder complete (`loader_js.rs azApplyPatches`).
- The transport needs **no change** — only the producer (loop+diff+visual) feeds it.

---

## 5. Sequencing & risk
1. **Phase 0.2 preflight** — cheap, high-leverage; do before anything else (it tells you which
   functions in Phases 1-4 will silently mis-lift).
2. **Phase 1 loop + Phase 2 diff** — the headline; together they make *any* state change round-trip
   (not just the counter). Highest value, medium risk (depends on readable StyledDom).
3. **Phase 4 timers** — small, mostly JS + one `tickTimer` export + the clock; high demo value.
4. **Phase 3 visual layer** — large but incremental (one CSS property at a time); the clip/image
   work the user called out lives here.
5. **Phase 4 threads + Phase 5 events** — last; needed for full app parity.

**Biggest risks:** (a) the readable wasm StyledDom (Phase 2.1 — the m11 box-new gap); (b) the
text-as-glyphs vs text-as-string decision (§6); (c) the geometry-ownership decision (§6) — it
determines whether the browser does layout (and we fight CSS to match) or the wasm owns geometry
(1:1 by construction).

---

## 6. RESOLVED — the "render target" model (user decision, 2026-06-10)
The WASM is the **single source of truth**: it runs the full cascade + layout + **text layout**
(keeping its own positioned-glyph copy internally), made **spec-accurate so it matches what the
browser would compute**. The browser DOM is a **passive render target** ("as if it were a screen"):
- JS applies **semantic** patches only — text content, classes, inline styles, structure,
  resource refs. **Never** absolute glyph positions; **never** a measurement query back to the browser.
- Because azul's layout matches the CSS spec, the browser's rendering of the patched DOM **equals**
  the WASM's internal calculation — **zero measurement round-trips**.
- The WASM keeps its full positioned copy purely for its own **hit-test + diff**; it never asks the
  browser "where is this element."

**The entire coupling surface is three points** — nothing else crosses the boundary:
1. **Event** JS → WASM (binary event buffer → `dispatchEvent`).
2. **Callback resolution** (which lifted callback the event targets).
3. **TLV result** WASM → JS → JS *acts*: patch the DOM, start/stop a timer, spawn/message a Web
   Worker, fetch a resource, etc. (the TLV vocabulary grows to cover these side-effects).

**Consequences for the build:**
- Patches are **semantic** (Phase 2 diff emits text/class/style/structure), not geometry. Phase 3's
  visual layer is CSS on the semantic elements (clip-path, transform, background-image, border) —
  still semantic, the browser paints it.
- **Text is real DOM text** — the WASM emits the source string; the browser shapes it. The 1:1
  guarantee comes from making azul's `text3`/solver **spec-accurate**, NOT from positioning glyphs
  in the browser. (That layout-correctness work — the FC/devirt fixes etc. — is the user's separate
  merge; the button-740-vs-121 bug was an instance of it, not an architecture problem.)
- **No absolute positioning, no canvas, no glyph spans, no `getBoundingClientRect`** anywhere in the
  JS host. The host is a pure applicator + event source + timer/worker/fetch driver.

---

## 6b. Host-call injection layer — the ONLY WASM→JS calls (user direction, 2026-06-10)
Beyond the three coupling points (§6), a small, fixed set of **host services** must reach real
browser APIs. These are injected **at the LLVM-IR layer**, exactly like the existing
`HashmapRandomKeys`/`fmaxf`/`__az_resolve_callback` bridges: the symbol scan
(`symbol_table.rs::classify_for_name`) detects the host symbol, the transpiler emits a body that
**calls a JS `env` import** (WASM→JS→WASM), and JS provides the real implementation. **User code is
oblivious** — it calls `Instant::now()` / `AzHttp_*` as usual and gets a real value back.

This is a new `FnClass` (e.g. `HostCall { js_import: &str }`) + an `emit_helper_ir` arm that emits
the import + a thin marshaling body. The set is deliberately tiny:

| Host symbol (detected) | JS implementation | Notes |
|---|---|---|
| `Instant::now` / `AzInstant_now` | `performance.now()` / `Date.now()` | **First / mandatory** — drives timers AND animations; lifted `Instant::now()` is 0 on wasm. |
| `AzHttp_*` | `fetch()` | request/response marshaled through linear memory; async → resolves into a TLV/event back into WASM. |
| Geolocation / sensors / video / screenshare / device APIs | the matching browser API | JS handles the permission + device interaction; only the *bridge* is injected into the IR. |
| `AzUdp_*` | **DISABLED for now** | no direct browser UDP. Later: a **WebRTC bridge** (data channels) injected **the same way** — same `HostCall` mechanism, JS implements the WebRTC side. |

Async host calls (HTTP, geolocation, WebRTC) don't return inline — they resolve by posting a
**result event/TLV back into WASM** (the same inbound path as a DOM event or a timer tick), so the
loop stays single-threaded and the user callback sees the result on a later turn. The clock is the
only *synchronous* one (it returns immediately). Design the `HostCall` ABI to support both:
sync-return (clock) and post-result-later (HTTP/devices/WebRTC).

## 7. Anchors (for the next session)
- Desktop loop: `dll/src/desktop/shell2/common/event.rs:3542` (`process_window_events`), `:2934`
  (`dispatch_events_propagated`), `:1146` (`apply_user_change`), `:4314`
  (`process_timers_and_threads`); `common/layout.rs:149` (`regenerate_layout`), `:901`
  (`incremental_relayout`). Signals: `core/src/callbacks.rs:77` (`Update`), `core/src/events.rs:82`
  (`ProcessEventResult`). Callbacks: `layout/src/callbacks.rs:167` (`CallbackChange`), `:741`
  (`CallbackInfo`); `layout/src/window.rs:4105` (`invoke_single_callback`).
- Timers/threads: `layout/src/timer.rs:35/111/194`, `layout/src/thread.rs:301/387/586/840`,
  `layout/src/window.rs:2050/3890/3982`, `core/src/task.rs` (ids/clock).
- Display list: `layout/src/solver3/display_list.rs:303` (`DisplayList`), `:598`
  (`DisplayListItem`), `:1665` (`generate_display_list`), `:2826` (`push_node_clips`), `:1659`
  (`push_image`); getters `layout/src/solver3/getters.rs`.
- Web backend: `dll/src/web/html_render.rs:300` (az_N counter), `dll/src/web/eventloop.rs`
  (state machine + TLV), `dll/src/web/loader_js.rs` (`azApplyPatches` + host), `core/src/diff.rs`
  (the unwired reconciler), `dll/src/web/transpiler_remill.rs:4914` (lift cache, now engine-aware).
