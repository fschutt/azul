# BACKUP / HANDOFF — x86 web-lift debugging state snapshot (2026-07-06)

Branch: `x86-weblift-b1sse` · azul HEAD `beb90cba1` · remill fork `2c4c8e0` (branch `x86-jumptable-devirt`).
This supersedes `HANDOFF_web_lift_x86_SSE_swisstable_2026_06_25.md` for the *latest* fix; that doc still holds
for all the SSE-ISEL / EMPTY_GROUP / tooling detail (§1–§6) and remains the reference for recipes.

## ★ NEWEST FIX (2026-06-26) — MOVxPS bit-copy (remill `2c4c8e0`, UNCOMMITTED submodule bump)
`third_party/remill/lib/Arch/X86/Semantics/DATAXFER.cpp` — `DEF_SEM(MOVxPS)`:
- **Was:** `FWriteV32(dst, FReadV32(src))` → types MOVAPS/MOVUPS as `<4 x float>`.
- **Now:** `UWriteV64(dst, UReadV64(src))` → bit-preserving `<2 x i64>` copy.
- **Why:** MOVAPS/MOVUPS is a *bit copy*, not float arithmetic. The `<4 x float>` typing made the wasm32
  backend (no native v128) legalize to 4 scalar f32, splitting each i64 half of a Rust fat-pointer `{ptr,len}`
  into two f32 pieces. Register-pressure-dependent local-coalescing then corrupts one f32 → garbage Vec/slice
  `len` = the **class-B multi-word-drop OOB** seen in `solveLayoutReal`/`layout_document`. `<2 x i64>` keeps each
  i64 half atomic (native wasm i64), size-generic for XMM(128)/YMM(256), float-canonicalization-free.
- This is the current lead on the deep x86 value-flow mis-lift (the viewport/fat-pointer stack-arg class,
  a.k.a. "Task #9"). **Committed + pushed on the remill fork as `2c4c8e0`; the azul submodule pointer bump is
  in this backup commit but the whole azul working tree remains WIP diagnostics.**

## STATE OF THE CHASE (what's known, per the 06-25 doc + this fix)
1. ✅ 17 SSE/SSE2/SSE3/SSE4.1 ISELs added (remill `1d5dd7f`) — 0/3031 unsupported. Necessary, not the hang.
2. ✅ EMPTY_GROUP hashbrown-mirror gap fixed (PE + ELF `.rdata` 0xFF-run scan in `symbol_table.rs`) → the
   empty-map-iteration infinite loop is gone (`EMPTY_GROUP-AUTO mirrored N runs` in server log).
3. 🔧 MOVxPS bit-copy (this doc) — the current fix for the remaining deterministic OOB in `layout_document`,
   root-caused to a fat-pointer `{ptr,len}` i64-half corruption on the deep by-value-struct arg path.
4. ⏳ NOT YET VERIFIED end-to-end: rebuild dll + cold relift + CDP-verify the solve completes with MOVxPS in.
   If it completes → re-enable hydrate (`loader_js.rs:520`, drop `false &&`), add `__remill_read_memory_32` /
   CAS / atomic impls to `loader_js.rs`, revert all diagnostics, then commit clean.

## UNCOMMITTED WORKING TREE (all diagnostics — revert before the clean commit)
See §3 of the 06-25 doc for the full list. Summary of what's live:
- `dll/src/web/eventloop.rs` — `AzStartup_solveLayoutReal` DIAG/PROBE0-8/VERIFY markers.
- `dll/src/web/loader_js.rs:520` — hydrate gate still `if (false && ...)`; needs `__remill_*` impls when re-enabled.
- `third_party/rust-fontconfig/` (vendored) + `Cargo.toml` `[patch]` + `Cargo.lock` — `az_fuf_*` markers,
  `AZ_IN_WASM_SOLVE` static, `chain_cache.insert` re-enabled.
- `scripts/m9_e2e/*.sh` + `*.mjs` — the relift / probe / marker-probe tooling (keep; reusable).
- `third_party/remill` submodule — DATAXFER.cpp MOVxPS fix (now committed on the fork as `2c4c8e0`).

## KEY RECIPES (verbatim pointers — full detail in the 06-25 doc §4–§6)
- Rebuild `amd64.bc` after editing semantics: `ninja -C /c/rb/remill lib/Arch/X86/Runtime/amd64.bc`.
- Cold relift + probe: `bash scripts/m9_e2e/cold_relift.sh`.
- Warm targeted relift: delete stale `$TMPDIR/az-lift-cache/*.lifted.ll`, rerun `cold_relift.sh`.
- Address map: `file_VA = 0x180000000 + (native − dll_base)`; dll_base from a running `hello-world` process.
- Memory file (most detailed blow-by-blow):
  `C:\Users\felix\.claude\projects\C--Users-felix-Development-azul\memory\windows-weblift-hydrate-trap.md`.
