# HANDOFF — x86 Windows web-lift: SSE coverage CLOSED, SwissTable hang remains (2026-06-25)

## TL;DR
The cron's stated root cause (AzButton_dom garbage `children` / dropped movups) is **DISPROVEN — do not re-chase it.**
The real deep bug was **incomplete remill x86 SSE coverage**: the Rust auto-vectorizer + explicit SSE4.1
intrinsics in the layout solver emit packed/double-precision SSE instructions that remill's XED decoded but
had **no DEF_SEM/DEF_ISEL** for → `HandleUnsupported` → `__remill_error` → the lifted fn bails. That is why
**HashMaps trap/hang but BTreeMaps work** (SwissTable is auto-vectorized).

- ✅ **17 SSE ISELs added + COMMITTED** to the remill fork (branch `x86-jumptable-devirt`, commit **`1d5dd7f`**).
  The layout solver now lifts **100% clean: 0 / 3031 functions unsupported.**
- 🔴 **The layout solve still HANGS** (probe: `[2c] hydrateStyledDom rc=0` but never `[2d]`). This is a
  **separate, deeper SwissTable CORRECTNESS mis-lift** (a HashMap's table pointer lifts to garbage → the
  hashbrown probe loop never terminates). Independent of coverage (0 unsupported) and of the `__remill` stubs.
  **THIS is the remaining foundational blocker.**

The goal is still: make the lifted layout solve complete → re-enable hydrate (`dll/src/web/loader_js.rs:520`,
drop `false &&`) → CDP-verify real hit-testing → commit.

---

## 1. What is COMMITTED (remill fork `1d5dd7f`, branch `x86-jumptable-devirt`)
File: `third_party/remill/lib/Arch/X86/Semantics/{SSE.cpp, MMX.cpp, MISC.cpp}` (+359 lines).
17 instructions, all ISEL-verified present in `amd64.bc`, lift scratch 0 unsupported:
- **SHUFPD / PSHUFHW** — added the *memory-operand* variants (were register-only). SHUFPD is the SwissTable's
  auto-vec 128-bit-const load `shufpd $0x2,const(%rip),%xmm`.
- **MOVSHDUP / MOVSLDUP / MOVMSKPS / MOVMSKPD** — were *completely missing* (only MOVDDUP/PMOVMSKB existed).
  MOVSHDUP is on the main path (`layout_document` horizontal float reductions). MOVMSKPD = `psllq $0x3f,xmm;
  movmskpd` branchless float-sign idiom (layout_bfc/ifc).
- **MAXPD / MINPD / SQRTPS / SQRTPD / RCPPS / RSQRTPS** — packed (were scalar-only). SQRT* loop the scalar
  `SquareRoot32/64` helpers; RCP/RSQRT use the exact reciprocal (the Newton refine step is a no-op on it).
- **ROUNDPS / ROUNDPD / ROUNDSS / ROUNDSD** — SSE4.1 `_mm_ceil_ps`/`_mm_floor_ps` (explicit intrinsics for
  pixel-snapping). **GOTCHA:** XED decodes `roundps xmm,xmm,imm` as the **`_XMMps_`** iform, NOT `_XMMdq_` —
  the first attempt with only `_XMMdq_` left them unsupported. Added BOTH iform variants.
- **MISC.cpp DoCPUID** — inline-emulates CPUID (writes 0 feature bits) so feature-detection doesn't hypercall.

⚠️ The azul superrepo's submodule pointer (`git status` shows `M third_party/remill`) **still needs bumping to
`1d5dd7f`** and committing in the azul repo.

---

## 2. THE REMAINING BLOCKER — SwissTable correctness mis-lift (the hang)
Symptom: probe prints `[2c] hydrateStyledDom rc=0 node_count=5` then **never `[2d]`** (80s timeout, no trap).
- `try/catch` in full-cycle.js catches **traps, NOT hangs** → the post-return PROBE markers can't be read.
- The hang is a HashMap in `azul_layout::solver3` (uses HashMaps 114×). The diagnostic comment in
  `eventloop.rs` (~line 1899) nails it: **`func[248]: i64.load [garbage_ptr-8]→RDX in a loop` = a hashbrown
  ctrl/bucket walk on a GARBAGE table pointer.** The table ptr lifts to garbage → the probe never finds an
  empty/matching slot → infinite loop.
- RULED OUT as the cause: (a) coverage — 0 unsupported; (b) `__remill_*` stubs — provided (see §3);
  (c) `NOOP_MEM` — only set by `AZ_NOOP_MEM=1`, unset here, so memset really fills.
- This is a CORRECTNESS bug in the lift of a *supported* instruction (a mis-lifted load/store of the HashMap's
  `table.ctrl`/bucket pointer), NOT a missing instruction.

### Next step: BISECT to localize, then disasm
`solveLayoutReal` (`eventloop.rs` ~1484) runs: DIAG block (BTreeMap/Vec/SSE tests, markers 0x40700+, ~1502) →
`AZ_IN_WASM_SOLVE.store` (~1647) → **PROBE0** (minimal `HashMap<String,u32>::insert`, marker 0x40870, ~1648) →
PROBE2-8 + a VERIFY block (font-resolution probes, markers 0x40690 `0xAAAA000N`, ~1724-1929) → the real solve.
Add `return <marker>;` progressively (after DIAG → after PROBE0 → after the font PROBEs); rebuild dll + relift +
probe. When `[2d]` appears, the early-return was reached → the hang is AFTER that point. PROBE0 is the key test:
if returning right after PROBE0 gives `[2d]`, the minimal SwissTable insert works and the hang is in the
font-resolution/main-solve; if it still hangs, the SwissTable insert itself is the bug.
Then disasm the hanging fn (file_VA = `0x180000000 + (native − dll_base)`; get dll_base from a running server,
see §5) at the probe loop to find the mis-lifted `i64.load`/`i64.store` of the table pointer, and fix the
remill lift (likely a sret/pointer-spill mis-lift; compare native disasm vs lifted .ll).

---

## 3. MESSY WORKARONDS / uncommitted diagnostic state (all in azul repo `git status`)
These are diagnostics + workarounds — **revert before the final commit, but keep for now** (needed to debug):

- **`scripts/m9_e2e/full-cycle.js`** (the probe): I added
  - a `[STUB-0]` logger in `stubFor` (logs `__remill_*` imports the wasm needs but the harness stubs to 0/no-op).
  - **real impls** for `__remill_read_memory_32`, `__remill_compare_exchange_memory_64/8` (CAS),
    `__remill_atomic_begin/end` in `realEnv`, and changed `cbEnv = {...realEnv, memory}` so **layout.wasm** gets
    them too. (Stubbed CAS `()=>0` → a CAS-retry loop never succeeds → that was a *secondary* hang, now fixed,
    but NOT the primary one.) Remaining stubs (`async_hyper_call`, `undefined_8`) are benign (my ops ignore the
    mem token). **⚠️ `dll/src/web/loader_js.rs` (production loader) ALSO lacks these `__remill_*` impls — when
    hydrate is re-enabled, the real layout path will need read_memory_32/CAS/atomic added to loader_js.rs too.**
- **`dll/src/web/eventloop.rs`** — `AzStartup_solveLayoutReal` is a DIAGNOSTIC probe entry: DIAG block +
  PROBE0-8 + VERIFY block + markers (0x40578/0x406xx/0x40700+/0x40830+/0x40870). The real hydrate path calls
  the `azul_layout` solver directly, NOT this. Revert all of it for the final commit.
- **`third_party/rust-fontconfig/`** (vendored, untracked) + **`Cargo.toml`** `[patch.crates-io] rust-fontconfig
  = { path = ... }` + **`Cargo.lock`** — the vendored crate has `az_fuf_*` markers + `AZ_IN_WASM_SOLVE` static +
  **`chain_cache.insert` DISABLED** (resolve_font_chain_impl ~lib.rs:2740, commented out). Re-enable the insert +
  drop the markers + drop the Cargo patch + delete the vendored dir for the final commit.
- **`dll/src/web/loader_js.rs:520`** — `if (false && initRc === 0 && domPtr && ...)` — the hydrate gate.
  Drop `false &&` to RE-ENABLE hydrate (the cron's goal) once the solve no longer hangs.

---

## 4. KEY MECHANISMS / RECIPES discovered this session (reuse these!)
- **Verify a remill ISEL WITHOUT a 27-min relift** — lift a single instruction:
  ```
  cd /c/rb/remill
  bin/lift/remill-lift-17.exe -arch amd64 -address 0x1000 -bytes 660f3a08c00a -ir_out o.ll   # 66 0f 3a 08 c0 0a = roundps xmm0,xmm0,0xa = _mm_ceil_ps
  grep -cE 'i32 (noundef )?257\)' o.ll        # want 0 (=ISEL matched; 257 = HandleUnsupported hypercall)
  grep llvm.ceil.f32 o.ll                     # confirms the semantic applied (intrinsic, not a libcall → no load crash)
  ```
- **FAST targeted re-lift** (~5 min vs ~27 min cold) — the lift cache `.lifted.ll` is keyed by (fn-bytes, lift_addr):
  ```
  CACHE=$TMPDIR/az-lift-cache   # = C:/Users/felix/AppData/Local/Temp/az-lift-cache
  grep -lE 'i32 (noundef )?257\)' "$CACHE"/*.lifted.ll   # = the stale HandleUnsupported entries
  rm <those>                                             # the .o obj-cache is keyed by IR CONTENT → auto-invalidates
  bash scripts/m9_e2e/cold_relift.sh                     # WARM: re-lifts ONLY the deleted fns, cache-hits the ~3000 others
  ```
- **Find stubbed imports** — the `[STUB-0]` logger in full-cycle.js `stubFor` (already added).
- **POST-REBOOT**: ASLR gives the dll a NEW base each boot → the address-keyed lift cache MISSES → a COLD relift
  (~22-27 min) is forced. The warm-cache shortcut only works within one boot. `AZ_LIFT_CACHE_CLEAR=1` does NOT
  actually clear the cache; `rm -rf $CACHE` (PowerShell `Remove-Item -Recurse -Force` if "Directory not empty").

---

## 5. KEY PATHS / COMMANDS / ADDRESS MAP
- **Edit remill semantics** → `third_party/remill/lib/Arch/X86/Semantics/{SSE,MMX,MISC}.cpp`.
- **Rebuild amd64.bc** (semantics are BITCODE loaded at runtime, NOT linked into the exe — must rebuild the .bc):
  `/c/Users/felix/tools/ninja/ninja.exe -C /c/rb/remill lib/Arch/X86/Runtime/amd64.bc lib/Arch/X86/Runtime/amd64_avx.bc`
  Verify: `third_party/remill/dependencies/install/bin/llvm-dis.exe /c/rb/remill/.../amd64.bc -o - | grep ISEL_<NAME>`
- **Cold relift + probe**: `bash scripts/m9_e2e/cold_relift.sh` (kills hello-world, restarts server on :8800, waits
  "Listening on", runs the AZ_HYDRATE probe). Tees to `/c/rb/cold_relift.log`; server log `/c/rb/server_cold.log`.
- **Probe directly** (server already up on :8800): `export PATH="$PWD/third_party/remill/dependencies/install/bin:/c/Users/felix/tools/node:$PATH"; timeout 90 env AZ_PORT=8800 AZ_HYDRATE=1 AZ_FONT=1 AZ_DUMP_DOM=1 node scripts/m9_e2e/full-cycle.js`
- **dll rebuild** (ONLY if dll/Rust changes — e.g. the bisection, the diagnostic revert, re-enable hydrate):
  `RUSTC_BOOTSTRAP=1 RUSTFLAGS="-Zunstable-options -Cpanic=immediate-abort" cargo build -p azul-dll --release --no-default-features --features "build-dll web web-transpiler" -Z build-std=std,panic_abort --target x86_64-pc-windows-msvc`
  then KILL the server holding `examples/c/azul.dll` FIRST, then `cp target/.../azul.dll{,.pdb} examples/c/`.
- **Lift scratch**: `$TMPDIR/azul-web-transpiler-<pid>/*.lifted.ll` (per-fn lifted IR, `AZ_REMILL_KEEP_SCRATCH=1`).
  Check unsupported: `grep -lE 'i32 (noundef )?257\)' <scratch>/*.lifted.ll | wc -l`.
- **ADDRESS MAP**: `file_VA = 0x180000000 + (native − dll_base)`. Get dll_base from a RUNNING server (ASLR-random
  each boot): `powershell "(Get-Process hello-world).Modules|? ModuleName -eq 'azul.dll'|Select -First 1 -Exp BaseAddress"`.
  Synth map (alt): `file_VA = 0x180000000 + (synth − 0x10F000)` for synth < 0x1C49004. Disasm:
  `third_party/remill/dependencies/install/bin/llvm-objdump.exe -d --start-address=<fileVA> --stop-address=<...> examples/c/azul.dll`.

## 6. Persistent memory
Full blow-by-blow is in `C:\Users\felix\.claude\projects\C--Users-felix-Development-azul\memory\windows-weblift-hydrate-trap.md`
(the top "★★★★★ CURRENT ROOT CAUSE" header supersedes the older refuted analyses — CPUID/fallbacks-swap/clone/insert).
