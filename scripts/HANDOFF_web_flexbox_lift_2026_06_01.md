# Handoff — web flexbox layout lift (2026-06-01)

Branch `mobile-ios-android`. Goal: prove the lifted (remill→wasm) layout solver produces the
SAME box positions as the native solver, for a pure-box flexbox (inline CSS, no fonts). Then PART 2:
font/image resource rewriting via a `RequestResources` TLV.

## TL;DR
- **Lifted layout now RUNS and inline CSS now APPLIES** (verified: a body with `height:600px` lands
  `600px` in the compact cache). Got there via 3 real fixes (below).
- **Remaining blocker is SYSTEMIC**: remill mis-lifts large `match`→jump-table dispatch. Every big-match
  function over `CssProperty`/`CssPropertyType` (clone, apply, `get_property_slow`, solver matches)
  either mis-dispatches (silent wrong/zeroed values) or can't resolve the computed target
  (`__remill_MISSING_BLOCK` trap). With CSS now applied, the solver's `get_property_slow` (font-size
  resolution) TRAPS. Per-site source workarounds don't scale; the real fix is the lift's jump-table
  devirt (`build_extra_data` / remill `--extra_data`).

## ⚠️ CORRECTION (later same day, read-only disasm — supersedes the jump-table claim above)
The "remaining blocker = CssProperty jump table in get_property_slow" attribution is **WRONG**:
- `objdump --disassemble-symbols=...get_property_slow` (@0xe12ad4 in `target/release/libazul.dylib`) shows
  **NO jump table and NO indirect branch** — only a csel-chain + a loop. It cannot be the MISSING_BLOCK site.
- The old trap PC `0x800f6b328` is from prior notes and unreliable (harness guest base is `0x80000000`,
  `symbol_table.rs:1336`, not `0x800000000`).
- The real cost/trap surface (base-independent, straight from `/tmp/server.log`): the full flexbox lift
  pulls **1680 transitive deps, 627 of them `allsorts_azul` font parsing** (woff2/gvar/variable_fonts/
  gsub/gpos/cmap/indic-syllable), dragged in via `rust_fontconfig::FcFontRegistry::request_fonts_fast` /
  `FcParseFontFaceFast` / `find_best_cmap_subtable` (server.log:3276). A minimal example lifts in 31 deps
  and serves fine — so it's the **font machinery** that bloats + traps, not CSS.
- **Runtime-safe to cut**: `collect_and_resolve_font_chains_with_registration` (getters.rs:3967) returns
  `ResolvedFontChains` (NOT a `Result`) → failed font resolution = empty chains, never a `LayoutError`; its
  fast-path skips the allsorts resolver when the DOM has no text codepoints. `FontNotFound` is only a
  `FontReloadError` (font.rs:36). A no-text box layout never needs a font → allsorts is only statically
  lifted, never executed.
- **FIX UNDER TEST**: `symbol_table.rs::classify_for_name` now has a WEB FONT BOUNDARY — `if crate_name ==
  "allsorts_azul" { return FnClass::Leaf; }` (before the catch-all `Recursable`). Principled: web fonts come
  from the browser (PART 2), never parsed in-wasm; CSS font-SIZE math is in azul_core, not allsorts, so
  geometry is unaffected. This unifies PART 1 (empty stub) + PART 2 (resource-request emitter). Relifting +
  gating now; expect allsorts dep count 627→~0. (So `build_extra_data` jump-table devirt is NOT the path.)

## REAL FIXES (keep — these are the deliverable)
1. **Niche-read false-Err** (`layout/src/solver3/positioning.rs` + call sites `mod.rs`/`paged_layout.rs`):
   `adjust_relative_positions`, `adjust_sticky_positions`, `position_out_of_flow_elements` returned
   `Result<(),LayoutError>`. `Ok(())` is niche-encoded into the 5-variant `LayoutError`; remill mis-lowers
   that niche-discriminant read so the `?` saw a FALSE `Err` → aborted EVERY lifted layout. Fix: those
   (Ok-always) fns return `()`; drop the `?`. PROVEN: bare body lays out 800px wide, matches native.
2. **`CssProperty::clone` zeroes the discriminant** (`core/src/styled_dom.rs` ~2027, in
   `convert_dom_into_compact_dom_internal`): `copy_special` did `style: self.style.clone()`;
   `CssProperty`'s derived `Clone` is a ~100-arm `match self{V(x)=>V(x.clone())}` → jump table → remill
   mis-lifts → cloned `CssProperty` comes back with discriminant 0 → all inline CSS silently dropped.
   Fix: the conversion CONSUMES the Dom, so MOVE the style instead of cloning:
   `let mut copy = dom.root.copy_special(); copy.style = core::mem::take(&mut dom.root.style);`
3. **`apply_css_property_to_compact` jump table** (`core/src/compact.rs`, the inline-CSS loop in
   `build_compact_cache_with_inheritance_debug`): same ~100-arm match jump-table mis-lift → never reaches
   the right arm. Fix: dispatch the layout-critical props (Width/Height/FlexGrow/Display) via
   single-variant `if let` (direct compares, no jump table) before falling back to `apply` for the rest.
4. **`AzStartup_alloc`/`free` checked→unchecked** (`dll/src/web/eventloop.rs`): `Layout::from_size_align`
   calls `is_size_align_valid` (core::alloc, Leaf-stubbed → returns 0=false) → alloc returned 0 → empty
   DOM. Fix: `Layout::from_size_align_unchecked(size, 8)` (align 8 is always valid).
5. **`hashmap_random_keys` Leaf-stubbed** (`dll/src/web/symbol_table.rs` `FnClass::HashmapRandomKeys` +
   `transpiler_remill.rs` helper IR): std HashMap entropy syscall can't lift → fixed-seed helper.
   NOTE: the seed is HARDCODED (deterministic-env; fine for layout, but flag it).
6. **`Vec::resize` + slice sorts Leaf-stubbed** (`dll/src/web/symbol_table.rs` classify_for_name, after
   the raw_vec/btree exemption): these do real work but defaulted to no-op `Leaf` (the `core/std→Leaf`
   heuristic). Exempted to `Recursable` — same pattern as the existing raw_vec/btree/FnOnce/OnceLock
   exemptions. (Real latent fix; didn't fix CSS but is correct.)

## THE UNDERLYING BUG (root cause of everything remaining)
remill mis-lifts LARGE `match` jump tables. The dispatch pattern (M10-E3 in
`dll/src/web/transpiler_remill.rs::build_extra_data`) is:
```
ADR  Xj, <table>      ; table inline in __TEXT after the block
LDRB Wk, [Xj, Wm, UXTW]
ADD  Xj, Xj, Wk, LSL #2
BR   Xj
```
`build_extra_data` scans `adrp/adr` data refs and ships a 256-byte window so remill can resolve the `BR`.
For the ~100-arm `CssProperty` matches it either resolves WRONG (mis-dispatch → discriminant reads 0 /
wrong arm) or not at all (`__remill_MISSING_BLOCK guest PC=0x800f6b328` trap, seen in
`CssPropertyCache::get_property_slow`). **This single class explains: clone zeroing, apply not matching,
and the get_property_slow trap.**

### Two fix paths
- **(a) SYSTEMIC (recommended)**: fix the jump-table devirt so all large matches lift correctly. Look at
  `build_extra_data` / `scan_arm64_adrp_accesses` (transpiler_remill.rs ~6409): why does the 256-byte
  ADR-window miss `get_property_slow`'s table? Likely a different dispatch pattern, a >256-case table, or
  the table not adr-referenced. Fixing this fixes clone/apply/get_property_slow/solver at once and lets us
  drop the per-site if-let/move workarounds.
- **(b) per-site**: keep bypassing each trapping function (whack-a-mole; the solver has more large matches).

### Alternative (sidesteps cascade jump tables, NOT solver): native-side cascade
Cascade the cb's DOM natively (server-side) and ship the resolved `compact_cache` into wasm; the layout
solver runs on it. Fixes clone/apply/get_property_slow (all in the cascade) but the SOLVER still has its
own large matches in wasm, so it only half-helps.

## DIAGNOSTIC SCAFFOLDING (being removed per request — `write_volatile` markers + inspectors)
All removed from `dll/src/web/eventloop.rs` (the `0x40080..0x401C8` `write_volatile` markers in
`solveLayoutReal`, the `AzStartup_getRootRuleCount`/`getBodyHeightRaw`/`getStyledNode0Rules`/
`testGetProperty` inspectors + their `EventloopState` fields + hydrate captures), the matching export
entries in `dll/src/web/mod.rs`, and the signatures in `transpiler_remill.rs`. The reliable-inspector
methodology (state-field + getter, NOT fixed-address `write_volatile` which the lift reuses) is the
recommended way to probe lifted state if you re-add any.

## ARTIFACTS (committable test harness)
- `layout/tests/web_flexbox_simple_ref.rs` — native reference rect-dump (the ground truth).
- `examples/c/web-flexbox-simple.c` (string CSS), `web-direct-body.c` (direct props, no parser),
  `web-barebody.c`, `web-sizedbody.c` (bisection controls).
- `scripts/m9_e2e/layout-flexbox.js` — the gate (asserts wasm rects == `flexbox-ref.json`). Has graceful
  `typeof mini.AzStartup_x === 'function'` guards, so it still runs after the inspectors are removed.

## BUILD / RUN
- Build: `cargo build -p azul-dll --release --features build-dll,web-transpiler-static`
- After EVERY dll build: `cp target/release/libazul.dylib target/release/deps/libazul.dylib` (else the
  server loads a 16KB stub).
- Run (lift ~7-15 min, ~1716+ transitive deps): `DYLD_LIBRARY_PATH=target/release
  REMILL_LIFT_BIN=…/remill-lift-17 AZ_BACKEND=web://127.0.0.1:8800 AZ_NO_LIFT_CACHE=1 AZ_WASM_DEBUG=1
  nohup ./examples/c/web-direct-body.bin >/tmp/server.log 2>&1 &`
- Gate: `AZ_LENIENT=1 node scripts/m9_e2e/layout-flexbox.js` (lenient=report; strict=assert rects).
- `rm -rf $TMPDIR/azul-web-transpiler-*` before each lift; keep disk >2GB.

## GOTCHAS
- **Editing azul-core regenerates `target/codegen/dll_api_internal.rs`** from `api.json`. The other agent's
  az-paint work flip-flops `paint_dot(&Brush↔Brush)`, which breaks the codegen mid-build. Web never uses
  GL/RawImage paint → the 4 generated paint wrappers are no-op'd in the generated file (transient; reapply
  if a build fails on `expected Brush/&Brush`). Leave az-paint to the other agent.
- Adding an `AzStartup_*` web export needs BOTH the export list (`dll/src/web/mod.rs`) AND a
  `signature_for_eventloop_fn` arm (`transpiler_remill.rs` ~133) — missing the signature makes the WHOLE
  `azul-mini.wasm` fall back to an 8-byte stub.
- `doc/fonts/SourceSerifPro-Regular.ttf` was restored (a 05-30 release reshuffle dropped it;
  `include_bytes!` at `eventloop.rs` needs it under `web`). TODO: re-track in git.

## PART 2 (font/image resource TLV) — NOT STARTED (gated on PART 1 layout)
Plan recorded in memory `web-flexbox-lift-2026-06-01.md`: `RequestResources` TLV (kind 13) carrying
content hashes; `azApplyPatches` case 13 injects/removes `<link rel=preload href=/az/img|font/{hash}>`;
server already has `/az/img/{id}` + `/az/font/{id}` routes (re-key id→hash).
