# Web 1:1 SUPER-PLAN — make the web backend run azul apps like the desktop

**Goal:** everything in `AzCallbackInfo_*` + the full input/output surface usable on the web.
**Method (user directives 2026-06-11):** iterate in increasing complexity; ALWAYS relift + verify
via CDP after each slice; FIX lift bugs found along the way (in `dll/src/web/` transpiler or the
remill fork, never azul-source hacks); keep relifts fast via smart caching. IGNORE layout-number
mismatches (azul-calc vs browser) — user-deferred. Track progress in THIS file (update before
ending every session). Cron `…` fires :13/:43 — when all slices are ✅ or hard-blocked: update
this file, CronList → CronDelete.

**Read first each session:** this file → `scripts/SESSION_PROMPT_web_1to1.md` (hard rules, CDP
loop, build commands) → `scripts/WEB_BACKEND_1TO1_PLAN.md` (architecture reference).

## Slices (user's order, increasing complexity)

- **S0 — infra (in flight)**
  - [x] Engine-aware lift cache (`AZ_LIFT_CACHE=1`, key = fn-bytes + lift-addr + version +
        remill-binary fingerprint). Baseline cold relift = **250 s** (was 15-30 min claim).
  - [x] Preflight clean-lift gate (`AZ_PREFLIGHT=1`): reports per-fn `__remill_error` (undecoded
        instr) + `__remill_missing_block` counts at startup. **Found 22 fns / 27 error sites**
        (core smallsort ×4, Vec::from_iter ×2, Map::fold ×2, taffy grid+flexbox ×5, solver3 fc ×5,
        positioning ×2, allsorts cmap/woff2 ×2, FcFontCache::get_font_bytes,
        layout_dom_recursive, build_compact_cache_with_inheritance_debug).
  - [x] **Undecoded instructions identified + FIXED**: triage (scripts/web_lift_triage.py —
        nm addr → extract fn bytes → standalone remill-lift-17, stderr names the instr) showed
        only ONE real gap across all 22 fns: `mrs/msr NZCV` (flag save/restore in Rust's
        smallsort networks). Implemented in the remill fork (Arch.cpp SystemReg + SYSTEM.cpp
        semantics, commit 70050e0 — bundled with the prior session's FP batch FRINTM/N/P/Z,
        FSQRT, FABD, FCMGT, FCVTN). All 22 fns now standalone-lift with ZERO decode errors.
        Residual __remill_error sites = fall-through blocks after noreturn `bl panic` at byte-
        range end — unreachable, benign. Triage rule: stderr-silent = benign; stderr-noisy = real.
  - [x] Commit S0 (azul-mobile 4-file commit) + remill fork 70050e0.
  - [ ] Preflight v2 (optional): auto-classify the benign noreturn-tail class so the report
        only screams on real gaps.
  - [ ] **Cache-v2 (MEASURED)**: lift-cache misses 100% across code-moving dylib rebuilds
        (789/789) — key + IR embed layout-derived synth addrs (symbol_table.rs:627). Same-dylib
        re-lifts hit 100% (hello-world regression run: 0 writes). BUT a `sample` of a cache-hot
        run showed the REAL bottleneck: `produce_object_from_lifted_ir → run_tool(llvm-link|
        opt|llc)` — 3 subprocess spawns per fn, run SEQUENTIALLY per BUNDLE walk, and the
        per-bundle closure walks recompile the same shared deps up to N+2 times (mini + layout
        + 9 cbs ≈ 4× hello-world's 3 bundles ≈ the observed 4× slowdown).
  - [x] **cache-v2a: content-keyed OBJECT cache** (obj_cache_path in transpiler_remill.rs):
        key = patched IR + helper IR + opt flag + IR-mutating env toggles + engine fingerprint;
        fn-matched diagnostic envs (AZ_REG_TRACE etc.) disable caching for that fn. Collapses
        the cross-bundle recompiles within ONE relift AND across relifts of an unchanged dylib.
        Verify with the events4 relift timing. NOTE: azul-source edits still cold-start both
        caches (synth churn) — stable name-keyed synth stays the (deferred, risky) endgame.
  - [ ] **AZ_NATIVE_REMILL=1 experiment** (in-proc LLVM-17 compile, no subprocesses; currently
        OFF and mutually exclusive with the lift cache, line 760): measure + e2e-validate on
        hello-world AFTER the trap fix. Caveat: in-proc is vcpkg LLVM-17, subprocess tools are
        LLVM-21 — different opt pipelines, needs its own green run before flipping defaults.

- **S1 — INPUT events: every JS event reaches its wasm callback (CDP-proven)**
  - Have: click, mousedown/up, dblclick, keydown/up, focusin/out, input(text), scroll, resize
    listeners + encoders; per-node kind filter (`cb_node_kinds[64]`, data-az-ev).
  - [x] IMPLEMENTED (pending relift+CDP verify): mousemove (rAF-throttled), wheel (→ EVT_SCROLL
        at pointer, desktop parity), mouseenter/mouseleave (capture-phase, DOM-TARGET routed —
        leave coords lie outside the node, never hit-test), contextmenu (preventDefault only on
        registered targets). event_kind += MOUSEENTER 12 / MOUSELEAVE 13 / CONTEXTMENU 14.
  - [x] Broadcast routing: RESIZE/SCROLL/KEYDOWN/KEYUP dispatch to EVERY node registered for
        that kind (azul Window-filter semantics), never bbox-hit-test. RESIZE records
        viewport_w/h in EventloopState. focused_node_idx tracked from FOCUSIN/FOCUSOUT
        (Focus-filter keyboard precedence deferred to S2).
  - [x] html_render: EventFilter::Window(...) arm added to event_filter_to_js_name (resize cbs
        previously registered as "click"!), Hover(RightMouseUp) → "contextmenu";
        azEvNameToKind learned mousemove/mouseover/mouseenter/mouseleave/contextmenu/input/resize.
  - [x] azDispatchTargeted + azNodeIdxFromTarget (az_N from DOM target) for focus/enter/leave/key.
  - Multi-cb-per-node is OUT of scope v1 (cb_fn_cache is one-cb-per-node); test app uses one
    node per event kind.
  - [ ] VERIFY: `examples/c/web-events.c` (9 cbs, counters[16] model) via
        `scripts/m9_e2e/web-events-cdp.js` — synthesizes real CDP input per kind, asserts
        counters via `__azProbe.mini.AzStartup_peekU32(__azProbe.modelPtr + 4*kind)`.
        Then hello-world relift as regression.

- **S2 — OUTPUT: callback modifies CSS, wasm cascades, TLV sets inline attrs**
  - The bug: `AzStartup_dispatchEvent` passes raw event bytes as the cb's `info` ptr
    (eventloop.rs:2126) — every AzCallbackInfo_* call today reads garbage.
  - [ ] Real `CallbackInfo` wasm-side: `changes: Arc<Mutex<Vec<CallbackChange>>>` lives in
        EventloopState (init like cb_fn_cache); ref_data → zeroed blob (grown later);
        hit_dom_node = {dom:0, node:node_idx+1}; cursors from event x/y. AzCallbackInfo_* bodies
        are GENERATED (target/codegen/dll_api_internal.rs, include!d in dll) → lift inside each
        cb closure automatically — no per-API wiring.
  - [ ] Drain after cb returns: lock + iter + clear IN PLACE (`take_changes()` returns Vec by
        value = OPEN class-B sret bug — avoid). Translate: ChangeNodeCssProperties /
        OverrideNodeCssProperties → `CssProperty::format_css()` (css/src/props/property.rs:4545,
        same fn html_render uses) → PATCH_KIND_SET_INLINE_STYLE; ChangeNodeText → SET_TEXT;
        SetFocusTarget → FOCUS; ScrollTo → SCROLL_TO. Apply the change to the wasm-side styled
        state too (wasm stays authoritative; cascade equality with desktop).
  - [ ] JS `azApplyPatches` case 4: switch from `setAttribute('style', …)` (clobbers
        server-rendered inline styles) to per-declaration `el.style.setProperty/removeProperty`.
  - [ ] Test: `examples/c/web-setcss.c` — click → `set_css_property(background-color + width)`;
        CDP click → assert `el.style.backgroundColor`.

- **S3 — TIMERS (drive animations)**
  - [ ] `Instant::now` HostCall injection at IR layer (clock = `performance.now()`); lifted
        Instant::now returns 0 on wasm → timers never advance without this.
  - [ ] Drain AddTimer/RemoveTimer changes → new TLV kinds (13 AddTimer {timer_id, tick_ms},
        14 RemoveTimer) → JS `setInterval`/`clearInterval` → each tick calls new export
        `AzStartup_tickTimer(state, timer_id)` → invokes the timer's cb (timer cb fn-ptr needs a
        pre-lifted per-cb wasm: server must lift-seed timer callbacks found in the binary — the
        fn-ptr lift-seed groundwork from commit 953162fa7) → drain changes → patches back.
  - [ ] Test: `examples/c/web-timer.c` — timer animates a div's width via set_css_property;
        CDP: watch style change over ~10 ticks.

- **S4 — IMAGES + FONTS (dynamic)**
  - Fonts already load via `/az/` URLs from the server (see loader_js azBootstrap font fetch +
    AzStartup_setFallbackFont) — extend to runtime-added fonts.
  - [ ] Image output: ChangeNodeImage/AddImageToCache/RemoveImageFromCache changes → TLV kinds
        (15 SetImage {node, src-bytes or /az/img/<id> URL}, …) → JS sets `<img src>` /
        background-image; RGBA buffers → canvas/ImageBitmap → blob URL.
  - [ ] Image callbacks (RenderImageCallback) + UpdateImageCallback/UpdateAllImageCallbacks →
        re-invoke image cb wasm → RGBA out → canvas. Enables infinite scroll demos.
  - [ ] Test: image swap on click + an image-callback node rendering a gradient buffer.

- **S5 — THREADS (async.c on the web)**
  - [ ] AddThread change → TLV → JS spawns a dynamically generated Web Worker whose shell loads
        the thread-cb wasm (server pre-lifts `ThreadCallbackType` fns same as timer cbs);
        ThreadSender/ThreadReceiver ↔ postMessage; writeback cb runs on main; worker close →
        RemoveThread. `std::thread::spawn` does NOT lift — the web-native create_thread is the
        IR-injected HostCall.
  - [ ] Target: `examples/c/async.c` (or its web twin) works end-to-end.

- **S6 — AzHttp → fetch**
  - [ ] Detect `AzHttp_*` symbols in the lift closure → FnClass::HostCall → JS `fetch()`;
        async result posts back as an event/TLV into wasm (same inbound path as DOM events).
  - [ ] Test: http demo fetches a URL and displays status/body length.

- **S7 — ua_css.rs Chrome parity**
  - [ ] Diff `layout/src/…ua_css…` against Chrome's html.css defaults (margins on body/h1-h6/p,
        display types for table/list elements, form-control styling); align so unstyled content
        renders like Chrome.

## Key architecture facts (verified 2026-06-11)
- `AzCallbackInfo_*` = generated transmute wrappers → `azul_layout::callbacks::CallbackInfo`
  methods → `push_change(CallbackChange::…)`. In the dylib ⇒ lifted automatically per-cb.
- `CallbackInfo` repr(C) (layout/src/callbacks.rs:741): {ref_data*, hit_dom_node: DomNodeId,
  cursor_relative_to_item, cursor_in_viewport: OptionLogicalPosition, changes:
  *const Arc<Mutex<Vec<CallbackChange>>>} (std build).
- `CallbackChange` (layout/src/callbacks.rs:167): ~60 variants — the natural patch boundary.
- TLV transport complete (kinds 1-12 both sides); `AzStartup_buildPatch` exists, unused.
- Dispatch flow: JS listener → 256-byte event buffer → AzStartup_dispatchEvent → hitTest
  (positioned_rects) → kind filter → __az_resolve_callback → __az_call_indirect(table_idx,
  refany_lo, hi, info_ptr) → per-cb wasm `callback` export.
- Per-cb wasms share memory+table with mini; bootstrap instantiates per `[data-az-cb][data-az-wasm]`
  element; `data-az-ev` → kind registration (html_render.rs:388-405).
- Mutex = macOS os_unfair_lock = out-of-image → Leaf no-op stub = correct for single-thread wasm.
- Relift loop: `bash scripts/web_relift.sh examples/c/hello-world.bin /tmp/server.log` (bg,
  poll 8800; 250 s cold, less warm). CDP: Chrome on :9222, /tmp/cdp_drive.js + cdp_probe.js +
  cdp_rects.js. Disk: `df -h /`, purge `/var/folders/*/T/azul-web-transpiler-*`.

## Session log
- **2026-06-11 (overnight, autonomous):**
  - S0: preflight+cache shipped+committed; NZCV decoder fixed in remill 70050e0 (the only real
    decode gap of 22 flagged; rest = benign noreturn-tail artifacts; triage script saved).
  - Generic byte-image hydration (size+bytes in az-hydrate; RefAny::get_data_len added) —
    legacy path alloc'd 4 bytes for EVERY model; >4B models corrupted the bump heap.
  - S1 implemented (kinds 12-14, broadcast routing for RESIZE/SCROLL/KEY*, focus tracking,
    targeted dispatch, html_render Window-filter names, rAF mousemove, wheel→scroll).
  - hello-world REGRESSION GREEN on new engine (counter 5→6 via CDP).
  - web-events TRAP bisected: `unreachable` in lifted collect_font_stacks_from_styled_dom;
    NATIVE repro test (layout/tests/web_events_repro.rs) PASSES ⇒ MIS-LIFT confirmed (user
    called it). Marker bisection (cdp_patch_probe): pre-loop markers set, post-phase-1 tag
    unset, bump allocator healthy ⇒ trap = tier2b_text[i]/tier1_enums[i] slice bounds panic
    ⇒ lifted build_compact_cache produced SHORT arrays (or Box deref wrong). Probe markers
    0x606A0-AC added (lens + Box addr + node_count) — relift events4 in flight.
  - Object cache (cache-v2a) implemented same rebuild.
  - User directives: many "layout bugs" are mis-lifts (confirmed); compare native vs lifted
    against azul-doc reftests as the systematic shaker (TODO: lifted-reftest runner — one
    binary, model carries the test xht via generic hydration, diff positioned_rects native vs
    lifted per test).

## Trap root-cause trail (live, 2026-06-11 ~02:30)
- Probe verdict: tier2b_text = (cap=19 ✓, ptr=3 ✗, len=0 ✗); tier1 fully correct; compact Box
  sane. The **minimal sret witness PASSES** (len=7 ✓) — NOT a blanket class-B failure.
- Producer found: `CompactLayoutCache::with_capacity` (css/src/compact_cache.rs:1466, native
  0xf0da5c) — real call from build_compact_debug, returns the whole 5-Vec struct via sret
  (x8→x19). Native tail (f0ddf0-f0de48) stores 21 fields straight-line: tier2b cap=x26@+0x48
  (LANDS lifted), ptr=x25@+0x50 + len=x20@+0x58 (GARBAGE lifted). x25/x26 are set by the
  3-mov chain right after the 3rd `bl __rust_alloc` (f0dd3c-44: mov x13,x25; mov x25,x0;
  mov x26,x20).
- **Raw remill IR of the whole fn is FAITHFUL** (standalone-lift: tail stores X26/X25/X20 →
  X19+72/80/88 correct; post-alloc mov chain correct; 0 decode errors). ⇒ corruption enters in
  the REAL pipeline's transform stack: BumpAlloc helper inlining (its X0 store is NON-volatile
  byte-GEP state+544 with HOST metadata from tag_state_accesses, vs guest struct-GEP loads) +
  opt -O2 + FIX_SP — alias/metadata-driven mis-forwarding is the prime suspect.
- Tooling armed: relift events6 running with AZ_LOG_STORES=CompactLayoutCache13with_capacity
  AZ_LSWIN_HI=0x7000000 (M12.5y store ring @0x41000/0x41010+k*16); /tmp/cdp_patch_probe.js
  now dumps the ring post-trap.

## Trap hunt II (events6-9, ~02:35-03:10)
- **with_capacity EXONERATED by ring**: its lifted tail writes the FULL struct correctly
  (tier2b cap=19/ptr=0x6043478/len=19 to sret x19=0x2ebf8). Its fill loops write correct
  sentinels (0x7fff) to the vec buffers.
- **build_compact's body exonerated by logic**: it indexes tier2b_text[i] per node during the
  build — len must have been 19 throughout, else it would have trapped much earlier.
- **Suspect narrowed to TWO copies**: (1) build_compact_debug's return q-copy (d5bbb8-d5bc1c:
  ldur q pairs from its stack → str q through saved-x8 x24; the doomed +0x50 chunk = `ldur q1,
  [sp,#0xe8]; str q1,[x24,#0x50]`), then (2) create_from_compact_dom's 432-byte memcpy
  (LibcMemcpy/llvm.memmove) of the struct into the Box @0x6046b00 (ring8: dest+len logged 2×).
- **Standalone lifts of EVERYTHING are faithful** (with_capacity whole-fn; build_compact
  whole-fn — its only error block is the brk panic via sync_hyper_call; the ldur-q snippet
  lifts with correct offsets 216/232 + i128 composition). ⇒ the corruption is injected by the
  REAL pipeline's per-fn opt stage (alwaysinline helper bodies + alias-scope tagging + FIX_SP
  + selfloop-rewrite over the linked module) — the g188/g189 "barrier-defeatable optimizer
  mis-transform" family — or lives in the memcpy helper body path.
- ⚠ METHOD BUGS FIXED: (a) remill-lift --bytes wants MEMORY-ORDER hex (byte-reversed words) —
  single-word "evidence" earlier was garbage; (b) "stderr-silent error = benign tail" is
  unreliable — silent decode-fail exists. Triage must locate error blocks positionally.
- Tooling: AZ_LSWIN_LO added (transpiler) — windowed store ring [lo,hi). events9 in flight:
  AZ_LOG_STORES=build_compact… LO=0x2e000 HI=0x30000 → only the caller-frame writes (the
  return q-copy's 11 stores) → verdict on hop (1) vs (2). Scratch watcher armed to capture
  __az_dep_*.instr.ll + .opt.ll (scratch is cleaned post-lift — copy DURING).

## Trap hunt III — THE POISON WRITE (events8-11, ~02:55-03:10)
- ring8 (create_from instrumented, complete 1292 entries) contains the corruption being
  WRITTEN: sites wrote (cap=0x13 @0x2f0f8, ptr=3 @0x2f100, len=0 @0x2f108) — the compact
  triple at its stack home (struct base 0x2f0b0) — i.e. create_from_compact_dom's lifted code
  stomps tier2b.{ptr,len} AFTER build_compact returns. build_compact + with_capacity + every
  copy hop are exonerated (ring6/ring9 + faithful standalone lifts).
- ⚠ Ring ids are PER-BUILD (instrumentation numbers post-opt output) — never map one build's
  ids onto another build's IR. events11 (in flight) re-instruments create_from; its ring +
  captured /tmp/cfc_instr.ll are id-consistent.
- NEW HYPOTHESIS from the IR read: the logger also logs State-REGISTER alloca stores (%PC,
  %W3…); their ring addresses reveal where llc placed the module's SHADOW-STACK frames. If
  those allocas land in 0x2fxxx, the lifted module's own stack frames OVERLAP the guest
  memory region where create_from's structs live → every register spill stomps guest data.
  (Would explain heisenbugs galore: g188 "barrier-defeatable", M12.5x self-relative stores.)
- AZ_LSID_LO added (id-floor tail tracing). Source context: after build_compact,
  `compact_cache = Some(compact)` then `prune_compact_normal_props()` then generate_tag_ids.

## ⚠ STANDING BUG found en route (fix with the batch): stack-slot/mirror collision
`relocate_stack_if_non_mini` (transpiler_remill.rs:2838) hands each wasm module SP =
192KiB + slot×128KiB, "below the bump heap at 1 MiB" — but slots 7+ land AT/ABOVE 0x110000,
inside the data-mirror/bump band (0x110000..). web-events has ELEVEN modules (mini + layout +
9 cbs) → slots 7-10's shadow stacks overwrite mirrored data pages. hello-world (3 modules)
never hit it. Fix: move STACK_BASE_FIRST band somewhere safe for ≥32 modules (e.g. carve
below the mirror: raise the bump/mirror base, or stride into a dedicated high band) — needs
the memory-map audit (mirror pages, bump base 96MiB, ring buffers 0x40000-0x50000).

## Trap hunt IV (events11-12) — RED HERRING cleared, timeline bisection armed
- The 0x2f0f8 "triple" was a RED HERRING: ids 592-594 are State-REGISTER GEP stores
  (`store i64 …, ptr %W3` / `ptr %PC`) — `ptrtoint %W3`=0x2f0f8 just means the lifted State
  struct sits there; the (0x13,3,0) values are transient register contents, NOT the compact
  struct. The REAL corruption (confirmed by markers) is in the HEAP Box @0x6046bd8:
  tier2b_text reads cap=19 ✓ but ptr=3, len=0 ✗ (tier1_enums fully correct).
- prune_compact_normal_props EXONERATED (only retain()s css_props/cascaded_props; never
  touches the compact Vecs). So corruption is either build_compact's RETURN (real pipeline,
  not the faithful standalone lift) or the `compact_cache = Some(compact)` move.
- events12 (in flight): source bisection probe in create_from (styled_dom.rs:1086) — 0x60660
  tier2b just after build_compact returns (pre-move), 0x60670 just after the Some-move. Reads
  ptr/len/cap at both. Decides: A bad ⇒ build_compact return mis-lift; A good+B bad ⇒ the move;
  both good ⇒ a later aliasing write (instrument heap window 0x6046000-0x6047000 next).

## ⚠ METHOD FIX (events12 wasted): azul-CORE had NO web_lift feature
- events12's A/B markers read 0 because `#[cfg(feature="web_lift")]` in core/styled_dom.rs was
  DEAD — azul-core never had a web_lift feature; only azul-LAYOUT did (that's why getters.rs
  probes fire but styled_dom.rs ones didn't). FIXED: added `web_lift = []` to core/Cargo.toml,
  and `azul-layout/web_lift` now forwards `azul-core/web_lift` (layout/Cargo.toml:146).
  ⇒ ANY future core probe/shim under web_lift now actually compiles. events13 rebuilds with it.
- Reminder for the diff-vs-reftest plan (user): also lets us add web-only shims in core, not
  just layout.

## ⚠ LESSON (events13 segfaulted): never put a fixed-address marker write in a
## CASCADE function — they run NATIVELY too.
- create_from_compact_dom runs natively in the server's render_initial_page (HTML emit), so my
  `write_volatile(0x60660,…)` probe hit unmapped native memory → SIGSEGV at server start.
  Layout-time fns (getters.rs) are wasm-only so their az_mark is safe; cascade fns are NOT.
- FIX: reverted the styled_dom.rs probe (kept the core web_lift feature — harmless/useful).
  For cascade-stage bisection use TRANSPILER instrumentation (AZ_LOG_STORES, wasm-only by
  construction) instead of source markers. events14 in flight: AZ_LOG_STORES=create_from_compact_dom
  windowed to the heap Box [0x6040000,0x6048000] → see the tier2b triple written into the Box.
- Refined signature: tier2b_text {ptr@0=3 ✗, cap@8=19 ✓, len@16=0 ✗} — 1st+3rd words of the
  24-byte Vec wrong, middle right. with_capacity provably returns it len=19 (vec![default;n]);
  build_compact fills by index (would panic if len<n; native doesn't). So len got ZEROED in
  build_compact's RETURN or the Some-move. Not a clean truncation (cap survived). Smells like
  2 dropped/misrouted stores of a specific field's ptr+len in a lifted sret/memcpy.

## SHARPER LEAD (2026-06-11 ~03:30): the trap is NODE-COUNT dependent
- hello-world (5 nodes, 1 text node) lays out FULLY — its tier2b compact cache is NOT corrupted
  (counter 5→6 verified). web-events (19 nodes, 9 text nodes) traps in collect_font_stacks with
  tier2b {ptr=3, len=0}. ⇒ the mis-lift is exposed by SIZE (more nodes/text), not universal.
  Likely a vec-grow / SROA-threshold / loop-bound path that only triggers above N. This also
  means a MINIMAL repro exists between 5 and 19 nodes → far cheaper to bisect than the full app.
- AZ_LOG_STORES can't see the `Some()` move (it lowers to llvm.memmove, not a plain `store`;
  the log only instruments `store`/memset-len). events14 heap window saw only adjacent css-cache
  vec builds, not the compact copy. ⇒ runtime marker bisection (events15, native-safe probe
  with the <4GiB wasm-address guard) is the right tool.

## ✅ ROOT CAUSE LOCATED (events15, 2026-06-11 ~03:50): class-B LARGE-struct sret
- A_afterbuild = (ptr=0x2, len=0, cap=0x13) — tier2b is corrupt THE INSTANT build_compact
  returns, before the move. cap (2nd word) survives; ptr (1st) + len (3rd) garbage.
- ⇒ build_compact_cache_with_inheritance returns a 176-byte CompactLayoutCache via X8/sret and
  the lifted return-copy corrupts it. THE class-B bug from HANDOFF §2 — but the staged witness
  (24-byte Vec<u64>) was too SMALL to reproduce; this 176B/6-Vec+BTreeMap struct is the real
  reproducer. Standalone lift was faithful ⇒ a REAL-PIPELINE opt transform breaks it (H2 family:
  load-forward / DSE / reorder of guest mem ops across inline-cloned alias scopes — the g188/g189
  "barrier-defeatable" class). Node-count dependent because more nodes = bigger struct/more copy.

## events16 verdict + STRATEGIC PIVOT (2026-06-11 ~04:00)
- events16 (build_compact + with_capacity at -O0): STILL CORRUPT (ptr=0x2, len=0). ⇒ NOT an
  optimizer transform on build_compact's own body. The corruption rides a pass that runs
  regardless: FIX_SP (post-opt textual, events17 testing), the sret call-bridge/X8 threading
  (handoff H3), or the raw lift's internal-sret routing. Static IR reads aren't converging
  (post-opt is fully inlined/renamed; 672 appears as both the X8 State offset AND incidental
  frame offsets).
- **DECISION**: class-B large-struct sret is the prior session's KNOWN-HARD bug (25+ iters,
  handed off unsolved). Solving it properly is a multi-hour remill/transpiler effort. It is
  PRE-EXISTING, not introduced by S1. So: (a) keep bisecting cheaply via env toggles
  (events17 FIX_SP, then AZ_NO_HOST_SCOPE) to localize for a future fix; (b) IN PARALLEL
  unblock the actual deliverable — verify S1 event-routing on examples/c/web-events-min.c
  (1 click div + 2 body-level Window cbs = hello-world-class node count, stays under the
  trap) → commit S1; (c) track class-B as its own deep item.
- web-events-min.c written: single-target hit-test (click) + broadcast (keydown/resize Window
  filters) — the two S1 routing paths — with ~5 nodes.

## class-B PRECISELY LOCALIZED (2026-06-11 ~04:00) + remaining tests
- events17 (FIX_SP off) INCONCLUSIVE: all markers 0 incl w_reached ⇒ pipeline breaks EARLIER
  without FIX_SP (it's load-bearing per M12) — can't use as a class-B diagnostic.
- build_compact return copy = 11× `str q` (NEON 128-bit) covering the 176B struct. Mapping
  (current dylib hb535, fn@0xd59f64): tier2b cap@0x48 rides `str q0,[x24,#0x40]` (LANDS),
  tier2b ptr@0x50 + len@0x58 ride `str q1,[x24,#0x50]` (CORRUPT). x24 = sret dest (mov x24,x8
  at entry d59fc4; x8 = state+672). Same x24 base for both, so NOT a wrong-dest — the q1
  VALUE/transfer is the bug.
- BOTH endpoints lift FAITHFULLY in isolation: `str q1,[x24,#0x50]` (0117803d) → 2× write_memory_64
  at +0x50/+0x58 ✓; `ldur q1,[sp,#…]` → 2× read_memory_64 at correct offsets ✓. And -O0 doesn't
  fix it. ⇒ the bug is NOT the instruction decoders and NOT opt — it's either (a) remill's
  memory-model lowering / alias-scope handling of the q-transfer's HIGH half in the linked
  pipeline, or (b) a body store with a mis-lifted address clobbering tier2b's ptr/len in
  build_compact's LOCAL frame before the return copy reads it (fits node-count dependence:
  more fill iterations = more clobber chances).
- REMAINING cheap tests (need the port, currently held by the S1 min relift): AZ_NO_HOST_SCOPE=1
  (alias-scope off — if it fixes tier2b, the tagging is wrong on the q-pair copy); then
  instrument the LOCAL frame (window build_compact's sp-region) to see if tier2b's local slot
  is correct just before the return copy (distinguishes (a) vs (b)).

## Next action
1. web-events-min.bin relift up → if it lays out (under threshold): node
   scripts/m9_e2e/web-events-min-cdp.js → click + keydown-broadcast + resize-broadcast all
   increment ⇒ S1 VERIFIED → COMMIT the S1 batch (remill NZCV already committed; this commits
   eventloop.rs/loader_js.rs/html_render.rs S1 + hydration size/bytes + RefAny::get_data_len +
   core web_lift feature + transpiler obj-cache + AZ_LSWIN_LO/ID + triage script + the test
   apps/scripts). REVERT the getters.rs + styled_dom.rs probes + az_sret witness first.
   - If min ALSO traps (threshold ≤6 nodes): class-B blocks all non-trivial apps → must fix it
     before S1 can be e2e-verified. Apply the sanctioned out-param workaround to
     build_compact_cache_with_inheritance (web_lift-gated, documented TODO-remove) as a STOPGAP
     to unblock, then deep-fix.
2. class-B: AZ_NO_HOST_SCOPE test → local-frame instrument → permanent transpiler/remill fix.
3. Resume S2 (real CallbackInfo + CSS-out).

## (historical) events16 = build_compact + with_capacity forced -O0 via AZ_LOWOPT_FNS
1. node /tmp/cdp_patch_probe.js → A_afterbuild_*:
   - GREEN at O0 (ptr valid, len=19): CONFIRMED optimizer transform. Narrow: try AZ_NO_HOST_SCOPE=1
     (alias-scope off) at normal opt — if that also fixes it, the bug is the host/guest alias
     tagging on the sret copy → fix the tagging (don't mark sret-dest stores noalias vs the
     struct's field loads). Else it's a generic opt (DSE/forward) → add a volatile barrier on
     the sret return-copy stores in the transpiler.
   - STILL BAD at O0: not opt — it's the lift/helper/fixsp. Bisect AZ_NO_FIX_SP=1, then the
     memcpy/memmove helper body, then remill sret semantics.
2. Permanent fix in transpiler/remill (NOT a per-fn -O0 hack, though that's a valid STOPGAP to
   unblock S1 if the real fix is deep). Relift → markers green → CDP suite → hello-world
   regression → COMMIT batch → S2.
3. Parallel cheap track still open: web-Ntext.c binary-search for minimal trapping N.

## (historical) earlier events14 note
1. node /tmp/cdp_patch_probe.js → A_afterbuild_* (0x60660, post-build pre-move) + B_aftermove_*
   (0x60670, post Some-move):
   - A bad (ptr=3/len=0): lifted build_compact RETURN garbles tier2b — sret/return mis-lift of
     the 4th Vec field; instrument the return q-copy by heap window; suspect i128/q-pair chain
     or FIX_SP over the inlined return.
   - A good + B bad: the Some()-move memcpy drops tier2b — chase LibcMemcpy/llvm.memmove body/len.
   - both good: later aliasing write — window heap 0x6046xxx across the layout pass.
2. Fix in transpiler/remill → relift → markers green (tier2b len=19, no trap) → full CDP suite
   → hello-world regression → COMMIT batch (S1 + hydration + obj-cache + AZ_LSWIN_LO/ID +
   core web_lift + the fix + stack-slot fix) → revert probes/witness → S2.
