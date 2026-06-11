# Kickoff — web backend to 1:1 with desktop

You are continuing the azul **web backend** (native aarch64 → WASM via a remill fork; the App's
real layout/cascade/event-loop runs lifted in the browser). The lift works: `examples/c/hello-world.c`
lays out, shapes text, and its on_click counter round-trips through a real browser (CDP-verified).
The goal now: make the web backend run the App **1:1 like the desktop app**.

**Read first, fully:** `scripts/WEB_BACKEND_1TO1_PLAN.md` (architecture + phases + the 4 subsystem
maps it was built from). Also `doc/guide/en/internals/web.md`. The prior cleanup arc is in memory
(`web_lift_cleanup_fable_2026_06_10`).

## The model (don't relitigate)
The WASM module **is** the azul App — same event loop (`process_window_events`), cascade, layout
solver, display list, timers, threads. **Resolved model (the "render target"):** WASM is the single
source of truth (full cascade+layout+text-layout, made spec-accurate so it matches the browser); the
DOM is a passive target that JS patches with **semantic** changes only (text/class/style/structure)
— never absolute positions, never a measurement query back. Coupling = 3 points: event JS→WASM,
callback resolution, TLV result WASM→JS (→ patch DOM / timer / worker / fetch). JS host services:
event source, DOM applicator (TLV), timer host (`setInterval`), worker host (`Web Worker`).
**Host calls (clock/HTTP/devices) are injected at the IR layer** (new `FnClass::HostCall` → JS `env`
import, same as `HashmapRandomKeys`/`fmaxf`): the scan detects `Instant::now` (mandatory; 0 on wasm),
`AzHttp_*`, geolocation/sensors/video/screenshare; `AzUdp_*` is disabled (later: WebRTC bridge, same
mechanism). User code is oblivious. The wire protocol (binary events in, TLV patches out) is already
complete; the gap is the **producer** (real loop + diff + visual layer + host-call injection).

## Hard rules (inherited — do not violate)
- Fix in the **transpiler** (`dll/src/web/`) or the **remill fork**
  (`/Users/fschutt/Development/azul/third_party/remill`) or **web-specific azul code**
  (`#[cfg(feature="web_lift")]` / new web-native shims). Lift bugs are NEVER worked around by
  corrupting shared azul source.
- **Commit only when the user asks.** Commit semantically (one concern per commit), end messages
  with `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.
- **Disk + orphans:** `df -h /`; purge `/var/folders/5x/*/T/azul-web-transpiler-*`; kill orphan
  `remill-lift`/`.bin` before a relift.
- **Analysis-first.** A cold relift is ~6 min; the lift cache (now `AZ_LIFT_CACHE=1`, engine-aware)
  makes incremental relifts much faster — only changed functions re-lift.
- **CDP loop:** Chrome headless on `:9222`; `/tmp/cdp_drive.js` (click), `/tmp/cdp_probe.js`
  (`__azProbe` legs), `/tmp/cdp_rects.js` (wasm-rect vs DOM-box 1:1 check). Relift via
  `scripts/web_relift.sh examples/c/hello-world.bin /tmp/server.log` (run in background, poll port 8800).

## Immediate order of work (per the plan §3)
1. **Phase 0.2 — preflight clean-lift gate** (do FIRST). After lifting, scan every per-fn lifted IR
   for `call ptr @__remill_error` / `@__remill_missing_block`; report per-function counts + the
   guest PC of each (synth→native decode). `AZ_PREFLIGHT=1`; optionally fail-fast. Surfaces
   silently-incomplete lifts before runtime. (Lift cache + engine fingerprint already landed.)
2. **Phase 1 + 2 — real loop + diff.** Replace the hardcoded counter `SetText`
   (`eventloop.rs:2135`) with the lifted desktop chokepoint (`process_window_events` →
   `CallbackChange` transaction) + wire `core/src/diff.rs` (`reconcile_dom` + `compute_node_changes`
   → `PATCH_KIND_*` via `AzStartup_buildPatch`). Needs a readable wasm StyledDom (prefer addressing
   the discrete Vecs over fixing the `Box<StyledDom>` readback). Make "arena NodeId == az_N" an
   asserted contract.
3. **Phase 4 timers**, **Phase 3 visual layer (clip/image/transform)**, **Phase 4 threads / Phase 5
   events** — in that order (small→large; see plan).

## Blocking decisions (resolve with the user before building the diff)
- **D1 geometry ownership:** normal-flow CSS (browser lays out, ~1:1) vs wasm-authoritative
  absolute positioning (1:1 by construction) vs hybrid. The button bug (740px vs 121px) is a D1
  symptom.
- **D2 text:** real DOM text (thread the source string out of `UnifiedLayout`) vs positioned glyph
  spans. The display list hands JS glyph IDs, not strings.

Start with Phase 0.2 (independent of D1/D2), confirm D1/D2, then Phases 1-2.
