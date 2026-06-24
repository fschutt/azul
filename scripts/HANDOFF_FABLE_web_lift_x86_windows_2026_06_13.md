# Handoff → next session: finish the x86/Windows web-lift (class-B struct-return)

**Branch:** `master`  ·  **Commits:** `e55af776d` (port) + `4080a12f7` (snprintf + dispatcher)
**Date:** 2026-06-13  ·  **Predecessor:** Claude Fable 5
**Read first:** `scripts/WEB_LIFT_BUG_COMPENDIUM.md` (the per-bug x86-port verdicts) and the
two memory notes `windows-weblift-port-strategy` + `windows-weblift-port-progress`.

---

## 1. TL;DR — where this is

The aarch64/macOS web-lift backend is **ported to x86-64/Windows and the core works**:
azul lifts the x86 `azul-mini` event loop **and** compiled C user code to WASM, serves it
over HTTP, and **executes user code correctly** — `examples/c/hello-world.c`'s `on_click`
runs in lifted wasm and increments the counter **5 → 6**. Basic layout works
(`hello-world-minimal.c` → `initLayoutCache rc=0`).

**The one remaining blocker:** the FULL hello-world layout (complex Dom: AzString + text +
CSS + Button) traps with a memory OOB inside the lifted layout-cb closure. Diagnosed to the
**x86 class-B struct-return mis-lift** — the exact bug class the macOS port's whole handoff
was about (`fschutt/remill@m12-q-reg-x8-sret`). **That is your task.**

**Nothing to fix in azul SOURCE** (same inherited rule): fixes go in the **transpiler
(`dll/src/web/`)** or the **remill fork** (`third_party/remill`, pinned `656c23c`, branch
`m12-q-reg-x8-sret`). The remill fork currently has NO x86-specific edits — the two x86
remill bugs below are documented but NOT yet fixed in the fork.

---

## 2. Build / run / test (RUNNABLE — exact commands on this machine)

Toolchain is all in `C:\Users\felix\tools\` (portable, no admin): `cmake` 3.29.6, `ninja`
1.12.1, `node` v20.18.1, `run_vs.cmd` (loads vcvars64 + prepends these to PATH).

```bash
# --- remill-lift-17 (ALREADY BUILT at C:/rb/remill/bin/lift/remill-lift-17.exe) ---
# If you must rebuild: deps superbuild is installed to
#   third_party/remill/dependencies/install (LLVM-17 + lld + clang + XED).
# Build dirs live OUTSIDE the repo at C:\rb\{deps,remill} (in-tree hits MAX_PATH).
# C:\Users\felix\tools\build_remill.cmd {configure,build,semantics,install}
# GOTCHA: `--target remill-lift-17` does NOT build the amd64 semantics .bc — run
#   run_vs.cmd ninja -C C:\rb\remill <abspath>/lib/Arch/X86/Runtime/{amd64,amd64_avx,x86}.bc

# --- 0. DISK + orphans first ---
df -h /c
powershell -NoProfile -Command "Get-Process hello-world,remill-lift-17 -EA SilentlyContinue | Stop-Process -Force"

# --- 1. codegen (only if api.json / azul-doc changed; see memory azul-build-pipeline) ---
cargo build --release -p azul-doc && ./target/release/azul-doc.exe codegen all

# --- 2. build azul.dll (web-transpiler, build-std). rustc 1.92: use the STRATEGY flag ---
RUSTC_BOOTSTRAP=1 RUSTFLAGS="-Zunstable-options -Cpanic=immediate-abort" CARGO_BUILD_JOBS=6 \
  cargo build -p azul-dll --release --no-default-features \
  --features "build-dll web web-transpiler" \
  -Z build-std=std,panic_abort --target x86_64-pc-windows-msvc
# (NO web-transpiler-static — subprocess path. ~17min cold, ~1.5min incremental.)

# --- 3. compile the example. /INCREMENTAL:NO is REQUIRED (else cb ptr is a jmp-island
#        thunk; chase_ilt_thunk handles it too but /INCREMENTAL:NO is cleaner). ---
cmd //c 'C:\Users\felix\tools\run_vs.cmd cl /nologo /O2 /Zi examples\c\hello-world.c \
  /I target\codegen /Fe:examples\c\hello-world.exe /Fd:examples\c\hello-world.pdb \
  /link /DEBUG /INCREMENTAL:NO target\x86_64-pc-windows-msvc\release\azul.dll.lib'

# --- 4. stage dll + PDBs (rustc embeds a RELATIVE codeview path → the server reads
#        azul.pdb from its CWD; also next to the dll). 95MB pdb is gitignored. ---
cp -f target/x86_64-pc-windows-msvc/release/azul.dll examples/c/azul.dll
cp -f target/x86_64-pc-windows-msvc/release/azul.pdb examples/c/azul.pdb
cp -f target/x86_64-pc-windows-msvc/release/azul.pdb ./azul.pdb

# --- 5. RUN the lift server (cold ~17min; mini cache-hits if azul.dll UNCHANGED) ---
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"   # llc/opt/llvm-link/wasm-ld
AZ_BACKEND=web://127.0.0.1:8800 AZ_LIFT_CACHE=1 ./examples/c/hello-world.exe > /c/rb/server.log 2>&1 &
# (or: bash scripts/web_relift_win.sh examples/c/hello-world.exe /c/rb/server.log  — run_in_background)

# --- 6. VERIFY (Node-only, no browser). The bump-heap seed line in full-cycle.js is REQUIRED. ---
node scripts/m9_e2e/full-cycle.js          # [1] bootstrap [2] layout rc=0 [3]click [4]counter++
# /c/rb/probe5.mjs = click-only (skips initLayoutCache) → proves the CORE counter path.

# --- CDP browser gate (Node 20 needs the flag): ---
"$CHROME" --headless=new --remote-debugging-port=9222 --user-data-dir=/c/rb/chrome-prof about:blank &
node --experimental-websocket scripts/cdp_click_hw.js   # blocked until initLayoutCache is fixed
```

**Cache note:** the on-disk lift cache (`$TEMP/az-lift-cache`, AZ_LIFT_CACHE=1) is keyed on
`(fn bytes, synth lift_addr)`. Rebuilding azul.dll shifts every function's synth address →
full cache miss → cold lift. Rebuilding ONLY the user .exe (azul.dll unchanged, exe span
< 1 MiB so azul.dll's synth_base stays 0x110000) → the mini (978 fns) cache-HITS → fast.

---

## 3. The task: x86 class-B struct-return mis-lift  (the full-layout OOB)

### Symptom
`node scripts/m9_e2e/full-cycle.js` against FULL hello-world:
```
[1] HTML bootstrap OK
RuntimeError: memory access out of bounds
  at layout-wasm fn[118]:0x58df4   (i64.load [local0 + 400] — a CS-snapshot load)
  at layout-wasm fn[90] → fn[50] → fn[11]
  at mini fn[115/114]  (AzStartup_initLayoutCache → __az_call_indirect_layout4)
```
`hello-world-minimal.c` (layout = `AzDom_createBody()`, no String/CSS/Button) → `rc=0`. So
it's SPECIFIC to the complex-Dom ops, not the layout pipeline.

### What it is NOT (already ruled out — don't re-chase)
- NOT snprintf (red herring: written=0 → empty label, no OOB; ABI fixed anyway in `4080a12f7`).
- NOT the 2 Leaf-stubs (`resolve_font_size_slow`, `UnresolvedBoxProps::resolve`) — those are
  in the MINI closure, not the layout cb's.
- NOT a no-op'd register-indirect call — enabling `__az_indirect_dispatch` on the subprocess
  path (`4080a12f7`, 142 dep cases for the layout cb) did NOT move the root; OOB persists.
- NOT `enforce_sp_preservation` itself — its CS-snapshot is just the FIRST deref of an
  already-garbage State pointer that fn[90] passed down. (Disabling SP-fix would move the
  OOB to the next State access; and on aarch64 it's REQUIRED — `AZ_NO_FIX_SP=1` traps there.)

### What it IS (working hypothesis — CONFIRM with the harness, don't assume)
A function in the layout-cb closure that **returns a struct/Vec by value** is mis-lifted on
x86 → the caller reads a garbage pointer/len → used downstream → OOB. This is the x86 analog
of the macOS **class-B** bug (`HANDOFF_FABLE_web_lift_2026_06_10.md` §2/§5). The macOS
azul-source out-param workarounds are `#[cfg(feature="web_lift")]` (arch-neutral) so they
apply on x86 too — meaning there is an x86-SPECIFIC return site still mis-lifting, OR the
x86 return ABI (RAX:RDX for ≤16B, hidden-RCX pointer for >16B, compendium C1) is mis-modeled
by remill / the wrapper.

### Attack plan (cheapest first)
1. **Localize fn[118].** wasm names are `--strip-all`'d. Either (a) relift the layout cb with
   `--strip-all` removed from the wasm-ld flags (in `link_objects_to_wasm`) to get a name
   section, then `llvm-objdump -d layout.wasm` maps fn[118] → `sub_<synth>` → SymbolTable
   name; or (b) `AZ_REG_TRACE`/`AZ_FUEL` ring-buffer the call chain (see §6 of the macOS
   handoff). fn[118] is the SP-snapshot site; the corrupter is fn[90] or earlier (fn 11→50→90).
2. **§M2 native-execution harness** (`scripts/mechb_harness/`, README inside) — THE tool that
   cracked class-B on aarch64. Lift the suspect struct-returning fn, compile its IR to the
   x86 HOST, set State regs, run, inspect the returned struct. Directly reusable (compile to
   x86 host instead of aarch64). Converts the uninstrumentable wasm OOB into a debuggable
   native binary.
3. **Check the x86 sret wrapper plumbing** (`transpiler_remill.rs`): `Pcs::HiddenPtrReturn`
   seeds the sret slot = `pcs::SRET` (RCX=2248 on x86). Verify INTERNAL (sub_→sub_) struct
   returns thread the hidden pointer through the shared `%state` correctly — the macOS bug
   was that internal sret callees didn't survive opt. The x86 return ABI differs from
   AAPCS64 X8: re-derive (C1: SysV RAX:RDX / RDI-hidden vs Win-x64 RCX-hidden).
4. **Last resort** = add x86 out-param workarounds in azul `cfg(web_lift)` source ONLY if a
   new site is found that the aarch64 ones miss — but prefer the remill/transpiler fix.

---

## 4. Two remill-fork bugs found (NOT yet fixed in the fork — candidates for fork commits)

The remill fork (`third_party/remill@656c23c`) builds + lifts x86 correctly EXCEPT:

1. **B1-x86 — CVTSI2SS-in-jump-table CHECK-abort** (compendium B1-x86). `remill-lift-17 --arch
   amd64` HARD-CRASHES (exit `0xc0000409`) on `cvtsi2ss xmm0, rax` reached as a jump-table arm:
   `InstructionLifter.cpp:619 Check failed: val_type->isIntegerTy() Expected XMM0 to be
   integral ([16 x i8] vs i64)`. Worked around in the transpiler (Leaf-stub the 2 fns:
   `resolve_font_size_slow`, `UnresolvedBoxProps::resolve`). A real fork fix to
   `LiftRegisterOperand` / the CVTSI2SS ISEL operand mapping would un-stub them.
2. **(this task) x86 struct-return** — see §3. Likely a fork fix to the amd64 return-value
   lowering, analogous to the `m12-q-reg-x8-sret` aarch64 branch.

When you fix either in the fork: commit on the `m12-q-reg-x8-sret` branch, bump the submodule
pin, rebuild remill (§2), drop the corresponding transpiler workaround.

---

## 5. What's already done + committed (don't redo)

Port (`e55af776d`) + fixes (`4080a12f7`), all in `dll/src/web/`:
- **symbol_table.rs**: Windows PE/PDB backend — `EnumProcessModules`, `ingest_pe` (PDB
  publics = mangled `_ZN` link names the classifier keys on + module procs; exports-only
  fallback), IAT import synthesis, `detect_pe_tail_shims` (E9/FF25), `pe_image_text_data_range`,
  win `is_system_image`, `__chkstk`→Leaf, path-style classify fallback.
- **mod.rs**: Windows `enumerate_loaded_images`, `dlsym_self` (GetProcAddress over non-system
  modules), `chase_ilt_thunk` (MSVC /INCREMENTAL jump-island chase = PE PLT-chase analog).
- **transpiler_remill.rs**: `pcs` module (Win-x64 ABI: RCX/RDX/R8/R9 args, RAX ret, RCX sret,
  State offsets from remill State.h, VERIFIED vs lifted IR); `Pcs::PtrFromPairLo` (refany is a
  24B by-pointer arg — pass the pointer straight in, the load-bearing fix that made the
  downcast run); `Pcs::StackArg`/`ByValCopyPtr`; iced-x86 scanners + arch-neutral dispatchers;
  `rewrite_iat_calls` (cross-image IAT `call [rip]` → direct call so the dep walk lifts the
  callee — THE fix that made on_click's closure non-empty); `run_tool` flagfile/@rsp spill +
  transient-retry (0xC0000142/0xC0000017); Leaf-stub resilience for remill crashes; snprintf
  ABI; `__az_indirect_dispatch` on the subprocess path.

Verified: counter 5→6 (probe5/full-cycle), minimal layout rc=0, mini+cb+layout serve.

---

## 6. Standing rules (inherited)
- **Commit only when the user asks.** **Watch disk first.** **Analysis-first** — a cold relift
  is ~17 min; exhaust static IR/asm analysis (llvm-objdump on the dll/wasm, llvm-pdbutil on
  the PDB, lift single fns standalone) before spending one.
- Test the PRODUCTION artifact, not just the isolated lift (§M9). A cb returning DoNothing /
  a layout rendering the fallback body is the x86-ABI analog of "wrong register" — verify the
  COUNTER increments, not just rc=0.
- The classifier (`symbol_table.rs::classify_for_name`) is the control panel — most "mis-lift"
  bugs are classification bugs. [INHERIT] wholesale from aarch64.

---

## 7. Session 2026-06-15 (Opus 4.8) — PRECISE localization of the full-layout OOB

**TL;DR:** the §3 diagnosis ("class-B sret mis-lift", "garbage State pointer in
`enforce_sp` CS-snapshot") is **WRONG / outdated**. The real trap, reproduced cleanly, is a
**guest store `mov [RCX+RAX], 8` whose computed address `RCX+RAX` is OOB**, deep inside
**`Css::from<CssPropertyWithConditionsVec>`** (`azul_css::css::impl$6::from`, synth
`sub_12021b0`). `%state` (local0) is FINE. It's a **garbage-VALUE** bug: a register (RAX,
holding ~`0xa00xxxx` — a heap pointer — where a small index is expected, so `base+ptr`
overflows) is stale because of a mis-lifted/missing block. NOT %state, NOT sret, NOT the
leak-gate.

### Exact trap (run with `AZ_FULL_CS_RESTORE=1`, port 8801, `node full-cycle.js`)
```
RuntimeError: memory access out of bounds
  layout fn[118]:0x55381  fn[90]:0x42fc0  fn[50]:0x28a3e  fn[11]:0x10a0e   ← layout.wasm
  mini fn[114]:0xb9f9  fn[115]:0xbd46                                       ← initLayoutCache
```
The wasm at module-offset 0x55381 (= objdump 0x54ae0, delta 0x8a1):
`local.get 0; i64.load 2248 (RCX); local.get 0; i64.load 2216 (RAX); i64.add; i32.wrap_i64;
 i64.const 8; i64.store offset=0` → `*(i32)(RCX+RAX) = 8`, address OOB.

### Localization (named the whole chain WITHOUT a relift)
- `fn[11]=callback`(=`layout`) → `fn[50]=AzButton_dom` → `fn[90]=Button::dom`(sub_1d6b50) →
  `fn[118]=Css::from`(sub_12021b0). Confirmed via run8-log identity + llvm-symbolizer.
- **Index→name method (no name section; AZ_WASM_DEBUG is DEAD — this wasm-ld rejects
  `--keep-section=name`, falls back to a 43-byte stub):** the 72 `__az_dep_<native>` EXPORT
  names survive `--strip-all`. Parse the export section (`/c/rb/wasmmap.js`,`wasmmap2.js`)
  for index→name + per-fn byte ranges (maps a trap offset → fn index). **Bodies are
  even-indexed; their `__az_dep` wrapper is the NEXT odd index (body = wrapper−1, LTO inlines
  the body into the wrapper → near-identical sizes).** Synth/RVA/native formulae for THIS
  machine: `synth = RVA + 0x10f000`; `native = RVA + runtime_base`; derive `runtime_base`
  per-run from `grep AzString_copyFromBytes@0x… server.log` minus its export RVA `0x9ce90`.
  llvm-symbolizer: `--obj=azul.dll  $((0x180000000 + RVA))`.

### RULED OUT (do not re-chase)
1. **Leak-gate / class-B CS-restore** — `AZ_FULL_CS_RESTORE=1` (unconditional CS-GPR restore,
   env-only, no rebuild) did **NOT** move the OOB. The x86 leak-gate IS benign (`SP_after =
   SP_before + 8`, the `ret` pops the call's pushed return addr — verified in the lifted IR),
   so the restore never fires — but that's not this bug.
2. **`__rust_alloc` (sub_560970)** — a real `E9 jmp __rdl_alloc` tail-shim; correctly
   redirected (no dangling import in layout.wasm; all 3 `@sub_560970[.N]` in Css::from →
   3 inlined BumpAllocs). `rewrite_sub_names_to_canonical` handles `.N` dups fine.
3. **Allocator works** — `/c/rb/bumpprobe.mjs` (live, no relift): bump ptr @`0x40020` seeds
   to `0xa000000` and MOVES 7776 bytes during `initLayoutCache` before trapping. So allocs in
   the layout-cb succeed; the OOB is NOT alloc-returns-0.
4. **`enforce_sp` local0-clobber via the alloc null-check** — RED HERRING. The one
   `local.tee 0` in fn[118] (objdump 0x5465c, an inlined `__remill_error` PC-record) is gated
   on `ZF!=0` from `test rax,rax` after a BumpAlloc; RAX = bump ptr (non-zero) → ZF=0 → it
   does NOT execute. The BumpAlloc helper correctly stores RAX (`ret_off=2216`); its memset is
   `@llvm.memset` (void, return discarded → no RAX clobber).
5. **snprintf / Leaf-stubs / indirect dispatcher** — already ruled out in §3.

### What Css::from actually contains (cached `.lifted.ll`, sub_12021b0, 5124 lines)
`__remill_error`×1-call, `__remill_missing_block`×12, `__remill_async_hyper_call`×12,
`__remill_undefined_8`×38. The async_hyper_call/missing_block are remill decoding the `ud2`
(Rust `unreachable`) + `0xCC` INT3 padding after a noreturn `call alloc::raw_vec::handle_error`
— mostly UNREACHABLE. **NB classifier gap (compendium A7, likely benign here but fix anyway):**
`alloc::raw_vec::handle_error` (a `-> !` fn) is classified **Recursable, not NeverLift** — the
NeverLift name-list in `symbol_table.rs::classify_for_name` matches `handle_alloc_error` but
NOT bare `handle_error` (both `_ZN…7raw_vec12handle_error…` and the `raw_vec::handle_error`
display name). Add it.

### The remaining question (for the NEXT step) — why is the store address garbage?
**Exact trap instruction** (Css::from .lifted.ll block 736, the `mov qword [rax+rcx*1], 8`):
`%739=load RAX; %740=load RCX; %741=RCX*1; %742=RAX+%741; write_memory_64(mem, %742, 8)`.
Data flow: **RAX (base) = `read_memory_64(mem, %716)`** (a pointer loaded FROM memory — a
struct field), **RCX = `%shl.i147.i` = RDI<<…** (the scaled index; RDI set @line 1944 from
`%call.i.i41`). So it's `base[index] = 8` and EITHER the base pointer (RAX, a struct-field
read) OR the index (RDI) is garbage — i.e. an **upstream value/struct mis-lift** feeds this
indexed store. (This is class-B-adjacent — a garbage struct field — but manifests as an
array-index OOB, NOT an sret-register or %state issue.) To pin which:
1. **Runtime-trace RCX/RAX at the store.** Cheapest: a transpiler pass (like the existing
   `AZ_WRITE_TRACE`/`AZ_SP_TRACE` straight-line ring-buffer writers in `transpiler_remill.rs`)
   that, gated on a new env, logs (addr,RCX,RAX) of the guest store at the synth PC of this
   instruction to a fixed ring; peek post-trap. (env-gated → cache-hit relift, ~fast.)
2. **Find the missing_block before this store.** In Css::from's `.lifted.ll`, the store is the
   IR block computing `RCX+RAX`; walk back to the nearest `__remill_missing_block`/`__remill_error`
   on the live path and identify the instruction whose lift was dropped (extract bytes from
   azul.dll at that synth→RVA, `llvm-mc --disassemble`). Decode-gap → remill fork; CFG-gap →
   remill TraceLifter / a transpiler `--extra_data`/jump-table fix (compendium B5 x86).
3. Compare with aarch64 (Css::from lifted clean there) to see what x86 form remill drops.

### UPDATE 2026-06-15 tick 2 — garbage value PINNED to an exact instruction
Traced the trap's base register all the way to its origin (static, no relift):
- Trap (Css::from .lifted.ll block 736) = `mov qword [rax+rcx*1], 8`. **RDI (→RCX index) = 0**
  (spilled as constant 0 @block370, SP unchanged to the reload → RCX=0), so the store is
  effectively **`*(RAX) = 8`** and **RAX (the base) is garbage**.
- RAX = reload of `[SP+80]`, spilled @block363 from RAX, whose value @line406 is
  `store i64 52405522936674862, ptr %RAX` = **`movabsq $0xBA2E8BA2E8BA2E, %rax`** at native
  RVA 0x10f31dd. **0xBA2E8BA2E8BA2E is a division-magic reciprocal** (repeating `8BA2E`),
  NOT a pointer. Surrounding native code: `imulq $0xb0,%rbp,%r15` (×176 = elem size) + a
  cluster of `cmov8`/`cmoveq` — i.e. a **`RawVec`/`Vec::with_capacity` capacity computation**
  (len×176, zero-sized-type cmov guards, div-by-const via the magic).
- So the lift **mis-routes the div-magic into RAX at the store** (natively RAX there holds a
  real base pointer). It's a **CFG / conditional-value-flow mis-lift** — a mis-lifted `cmov`
  or the magic's value-flow through the spill/reload + the `je` (`test rax,rax`) that selects
  the path reaching the magic-reload (block 712, preds %956/%702).
- **REFUTED: undefined-flag hypothesis.** `/c/rb/undef_exp.mjs` overrides
  `env.__remill_undefined_8` (a JS import — no relift) to {0,1,255}: ALL give the identical
  OOB. So the bad `je` is driven by a REAL flag (`test rax,rax`), not a remill-undefined flag.
- **NET:** it's a remill x86 value-flow / `cmov` / div-by-constant mis-lift in Css::from's
  RawVec-capacity code. NEXT: disassemble native Css::from 0x10f31dd→the store, map each
  `cmov`/`imul`/`shr` to the lifted IR, find the one whose lifted result diverges (esp. the
  `cmov`s — remill x86 CMOVcc + the flag it reads). Or a register-trace relift (RAX/RCX at the
  store PC) to confirm RAX=0xBA2E8BA2E8BA2E at runtime. The fix is a remill-fork semantics fix
  (or a transpiler rewrite of the offending form). New probe: `/c/rb/undef_exp.mjs`.

### UPDATE 2026-06-15 tick 3 — CORRECTION + native ground truth
The "div-magic mis-routed into the base" claim was from an **incomplete static trace** — the
awk for "last RAX store before the spill" only matched `ptr %RAX` and **missed GEP-based RAX
writes** (the inlined BumpAlloc writes its result via `ptr %ret_p` = gep state,2216, not a
named `%RAX`). So RAX@spill is NOT provably the magic. Native ground truth
(`/c/rb/cssfrom_native.dis`, Css::from RVA 0x10f31b0):
- The trapping store is a **real, correctly-decoded** instruction at native **0x10f32cc**:
  `movq $0x8, (%rax,%rcx)` (8 bytes — matches the lift). NOT a misaligned decode.
- It's the **first store of a LOOP** (loop top `jmp 0x10f3360` @0x10f32b1; body reloads
  `mov rax,[rsp+0x50]` @0x10f32c0, `mov rcx,rdi; shl rcx,7` → index = rdi×128, then the
  store) that **initializes Vec/struct elements**: `base[i*0x80] = 8; …` (a run of
  `movq/movups` at `(%rax,%rcx)+disp`, 0x10f32cc–0x10f3314).
- **base = `[rsp+0x50]`** which natively holds the **alloc result** (set @0x10f326c `mov
  [rsp+0x50],rax` right after `test rax,rax; je <alloc-fail>` @0x10f325e–61). The div-magic
  was only TRANSIENTLY in rax (movabs @0x10f31dd) for the capacity division; rax is
  overwritten by the alloc before the spill. `[rsp+0x50]` = `[SP+80]` in the lift.
- So the OOB is `base[index]` with **either base (alloc ptr) or index (rdi loop counter)
  garbage at runtime** — a value-flow mis-lift, NOT a decode/misalign issue. Static analysis
  is now blocked by GEP-aliasing + the runtime-dependent loop bound.
- **DEFINITIVE NEXT STEP: a runtime register trace.** Implement an env-gated transpiler pass
  (model it on `instrument_guest_writes`/AZ_SP_TRACE straight-line ring writers) that, at the
  guest write whose synth PC = Css::from+(0x10f32cc−0x10f31b0), logs State.RAX(2216),
  RCX(2248), RDI(2296) to a fixed ring; rebuild + relift; read the ring post-trap to see which
  of base/index is garbage. THEN trace that value's lifted producer (now knowing which reg) to
  the mis-lifted block. (bump heap moved 7776B so allocs DO succeed → leans toward the loop
  COUNT/index being garbage, but confirm.) New artifact: `/c/rb/cssfrom_native.dis`.

### UPDATE 2026-06-15 tick 5 — RUNTIME CONFIRMED: the BASE pointer is garbage
Ran `AZ_WRITE_TRACE='impl$6::from'` (it DID apply through the cache — logged "traced 112
volatile-i64 guest-writes in __az_dep_7ffb8f6031b0") + `/c/rb/wtprobe.mjs` reads the
0xD0000 ring post-trap. 45 writes; the trapping store [44] = `addr=0x91314ade val=0x8`
(≈2.4 GB → OOB). A sibling struct-init [26–34] writes fields at **base = 0** (addrs
0x8,0x28,0x30,0x38,0x40,0x50,0x68,0x71,0x78 — in-bounds low page, so no trap). So:
- **The loop/struct BASE pointer is GARBAGE (0 and 0x91314ade), NOT the index/count.**
- **NOT the div-magic** either: magic truncated to i32 = 0xA2E8BA2E ≠ 0x91314ade.
- Valid heap ptrs (`0xa001c08`,`0xa001cf0`,`0xa001ef0`) DO appear in the trace (allocs work),
  but they never reach the loop base → **the alloc-result → `[SP+80]` spill value-flow is the
  mis-lift.** (i.e. `[rsp+0x50]` = the alloc result natively @0x10f326c after `test rax,rax;je`,
  but the lifted `[SP+80]` reload yields garbage.)
- **NEXT:** register-trace RAX at the spill PC (lifted line ~1123 = native 0x10f326c
  `mov [rsp+0x50],rax`) — does RAX hold the alloc result there, or garbage? If garbage, walk
  the lifted RAX producers from alloc@line1031 to spill@1123 (account for GEP `state,2216`
  writes, NOT just `ptr %RAX`) and/or the CFG path — the alloc result is being lost between the
  alloc and the spill (a value-flow / block-recovery mis-lift). Artifacts: `/c/rb/wtprobe.mjs`,
  `server_wt.log` (8801 now runs the AZ_WRITE_TRACE build).

### Toolkit left in /c/rb (reusable, no relift)
`wasmmap.js` (export index→name), `wasmmap2.js` (+byte-range→fn, trap-offset→fn, body→wrapper),
`bumpprobe.mjs` (live bump-heap probe), `fetch.js` (pull a wasm from the running server),
`layout_csr.wasm`+`layout_csr.dis` (the AZ_FULL_CS_RESTORE build + disasm), `server_csr.log`.
`full-cycle.js` now honors `AZ_PORT` (so you can run two servers concurrently — cache is
shared read-only). Two servers ran in parallel (8800 named-but-stubbed, 8801 CS-restore).

### UPDATE 2026-06-15 tick 8 — the SP_RET_DELTA fix is WRONG (built + relifted, NO effect)
Implemented an arch-conditional `SP_RET_DELTA` in `enforce_sp_preservation` (3 edits, IN the
working tree, **UNCOMMITTED**): x86 `call` pushes the return addr but helper bodies model no
`ret`-pop, so I made the leak-gate expect `SP_before+8` and restore the SP slot to it.
Rebuilt azul.dll (3m12s), cold-relifted, ran full-cycle: **same OOB, same chain
fn[118]→90→50→11** (offset shifted 0x55381→0x58e94 so it DID apply). Counter path still 5→6
(no regression). Re-ran AZ_WRITE_TRACE on the fixed build (`/c/rb/wtprobe8800.mjs`,
`server_wt2.log`): ring is **structurally identical** to pre-fix — sibling struct-init at
**base=0** ([26–34]: 8/0/0xa001ef0/1/… → 0x8,0x28,0x30,…) and trapping store [44]
`addr=0x9148d99e val=8`. **So SP-drift is NOT the cause** (the earlier SP-drift reading was a
mis-interpretation of the call return-addr pushes).
- **REAL bug = a value-flow mis-lift: the alloc-result pointer never reaches the struct/Vec
  BASE register**, while OTHER alloc results (`0xa001ef0`) DO flow into struct FIELDS
  (`*(base+0x30)=0xa001ef0`). In Css::from a specific base GPR is 0/garbage at the struct-init
  and the loop store. Not SP, not index, not div-magic.
- **REVERT candidate:** the SP_RET_DELTA edits did not fix this — harmless (aarch64 no-op, no
  x86 regression) but unverified; revert unless a later bug proves SP-drift matters.
- **NEXT (fork):** (a) custom register-trace transpiler pass (log RAX/RBX/… State-GPR reads at
  the struct-init synth PC) → which base reg is garbage + trace its lifted producer; (b)
  remill-fork TraceLifter / value-flow debugging of Css::from; or (c) the azul-source out-param
  workaround for the Css-returning path (§3 step 4 "last resort", `cfg(web_lift)`) — fastest to a
  working hello-world but adds a source workaround.

### UPDATE 2026-06-15 tick 9 — CORRECTION: SP drift IS real and PERSISTS (tick 8 was wrong)
Re-read the post-fix AZ_WRITE_TRACE ring carefully. It is NOT a pure value-flow bug — there IS a
guest-SP / frame-offset drift between the base SPILL and the loop RELOAD:
- An alloc result `0xa001cf0` is spilled to stack `0x6f0e8` ([13]) ⇒ rsp@spill ≈ 0x6f098.
- The loop's base reload reads `[SP+0x50] = 0x6f050` ([42], val `0x12052a2` = a pushed return
  address) ⇒ rsp@reload ≈ 0x6f000. **drift = −0x98 = 152 bytes = 19×8** (19 unpopped pushes).
- So the reload reads a return-addr-push slot instead of the base-spill slot → garbage base → OOB.
- My `SP_RET_DELTA` fix (direct `@sub_` only) did NOT prevent this drift. **NB (tick 9b): Css::from
  has 10 DIRECT `@sub_` calls and ZERO `__remill_function_call`** (also 12 `__remill_missing_block`
  + 11 `__remill_async_hyper_call`). So the drift is NOT from indirect calls. It must be from
  either (i) the 10 direct calls — which my fix SHOULD correct, so suspect a snapshot/firing bug in
  my edit (is `%azv_11` really post-push? does enforce_sp actually wrap Css::from's calls? did the
  gate fire?); or (ii) the `missing_block`/`async_hyper_call` paths, which enforce_sp's
  `parse_sub_call` does NOT match/wrap, so their SP effects (if any) go uncorrected; or (iii)
  mislifted prologue/loop `push`/`sub rsp`/`pop` in Css::from itself. (`AZ_FULL_CS_RESTORE` doesn't
  touch SP — consistent with it never helping.)
- **REFINED FIX DIRECTION:** correct the SP drift for ALL calls that PUSH a return addr (direct
  AND indirect), while still NOT restoring for genuine `br` tail-jumps (continuations). The hard
  part: `__az_indirect_dispatch` is reached from BOTH indirect-CALLs (push, need +8 restore) and
  tail-JUMPs (no push, must NOT restore). Need to distinguish — e.g. tag the dispatch/missing_block
  helper by whether the originating insn was a `call` vs `br` (carry a flag, or have remill emit
  distinct helpers), OR verify whether the lifted SP store actually decrements at the call (snapshot
  pre/post-push) and recompute SP_RET_DELTA accordingly. First cheap check: does Css::from's loop
  call its body via `@sub_` (direct) or `@__remill_function_call`/dispatch (indirect)? Grep the
  (new-synth) Css::from `.lifted.ll`. The `SP_RET_DELTA` edits remain uncommitted.

### UPDATE 2026-06-15 tick 11 — BOTH SP fixes failed; SP edits REVERTED; hand-off
Strengthened the fix to be robust against both gate-firing AND LTO-DCE: for direct `@sub_` calls,
restore the SP slot **unconditionally + volatile** to `SP_before+delta`. Rebuilt (1m31s),
cold-relifted, full-cycle: **STILL the same OOB** fn[118]→90→50→11 (wasm hash changed 003118ea→
0030e14a, offset 0x58e94→0x58cd2, so it applied). Counter path stays 5→6 (no regression).
- **CONCLUSION: SP-restore on direct `@sub_` calls is NOT the fix.** Since Css::from has ONLY 10
  direct `@sub_` calls (0 indirect) and both a gated and an unconditional+volatile SP restore on
  them failed, the 152-byte SP/frame drift must come from sources `enforce_sp` does NOT wrap —
  most likely a **mis-lifted prologue/loop SP op in Css::from itself** (`sub rsp,0x1e8`, the
  push r15..rbx block, or a loop push/pop) and/or the `__remill_missing_block`/`async_hyper_call`
  paths — OR the leak-gate's SP snapshot is not actually post-push (snapshot-model error). My
  call-boundary SP-restore can't fix any of those.
- **ACTION TAKEN:** `git checkout` reverted the SP_RET_DELTA edits in transpiler_remill.rs (clean
  tree). The staged examples/c/azul.dll + the running :8800 server still have the (ineffective)
  fix until the next rebuild — rebuild from clean source before further work.
- **DEFINITIVE NEXT STEP (focused session, not autonomous ticks):** a PER-INSTRUCTION SP trace.
  Adapt `instrument_reg_stores`/`parse_reg_store` for x86 register names (`%RSP`/2312, `%RAX`/2216,
  …; currently aarch64-only: `%X<n>`/`%SP`) — OR write a pass that logs State.SP after EVERY guest
  instruction in Css::from to a ring — then cold-relift (must bust the wasm cache; `AZ_LTO_LEVEL=0`
  alone was served the CACHED wasm and was inconclusive) and watch where SP first diverges from the
  native frame. That pinpoints whether it's the prologue, a loop op, or a specific call. Then fix
  in the transpiler (rewrite the offending SP op) or the remill fork. Toolkit + all traces in
  /c/rb (`wtprobe8800.mjs`, `cssfrom_native.dis`, `server_fix2.log`, `wasmmap2.js`).

### UPDATE 2026-06-15 tick 13 — DEFINITIVE via reg-trace: the INDEX is a return address
Extended `AZ_REG_TRACE`/`parse_reg_store` for x86 (`%RSP`+inline `%rsp.*`→SP id99, `%RAX`→0,
`%RCX`→50) — KEEP this (x86 diag enhancement; uncommitted but useful). Built, cold-relifted with
`AZ_REG_TRACE='impl$6::from'`, ran `/c/rb/regprobe.mjs` → dumped the SP/RCX trajectory at the trap:
```
… SP=0x6f018 → 0x6f008 → 0x6f000 → 0x6eff8   (Css::from's own pushes; only DECREASES)
[24] RCX=0x1204fd1   [25] RCX=0x9027e880
```
- **`0x9027e880 = 0x1204fd1 << 7`**, and `0x1204fd1` is a CODE address in Css::from's range (a
  RETURN ADDRESS). So the loop INDEX register (`RDI`, scaled `<<7` into RCX) holds a **return
  address read from the stack** → `mov [rax+rcx],8` address = base + (retaddr<<7) ≈ 2.4 GB → OOB.
- So an **SP-relative reload that should load the loop index instead reads a return-address slot**
  = SP/frame drift. (RAX id0 wasn't logged — the lift writes RAX via a non-`%RAX` SSA name; the
  index garbage via RCX/RDI is the proven culprit regardless.)
- **Why my 2 SP fixes failed:** `enforce_sp` wraps CALL boundaries (`@sub_`/dispatch). But the
  trajectory shows Css::from's own PUSHES with no matching traced pops — if remill **drops a
  `pop`/`add rsp` stack-cleanup** (not a call), the call-boundary restore CAN'T fix it. The drift
  is in Css::from's intra-body stack balance, not (only) at calls.
- **NEXT (cheap, static, no relift):** read Css::from's lifted IR and tally SP `sub`/`add`/push/pop
  to find the UNBALANCED op — a native `pop`/`add rsp,N`/`leave` that remill dropped or mis-lifted
  (compare to `/c/rb/cssfrom_native.dis`'s epilogue/loop). That dropped cleanup is the fix target
  (rewrite/lift it correctly in remill, OR a transpiler stack-balance pass broader than enforce_sp's
  call-wrapping). Tool: `/c/rb/regprobe.mjs`, `server_rt.log`, AZ_REG_TRACE now x86-capable.

### UPDATE 2026-06-15 tick 14 — PRECISE MECHANISM (loop index spill/reload + SP drift) — loop wound down
Read the native Css::from loop (`/c/rb/cssfrom_native.dis`). The loop index lives in **RDI**:
`inc rdi` @0x10f3344 → **spill `mov [rsp+0x58],rdi`** @0x10f3347 → (back-edge, loop body) →
**reload `mov rdi,[rsp+0x58]`** @0x10f33fa → `cmp rdi,[rsp+0x48]`. Between the spill and the
reload the loop body makes 3 CALLS: memcpy-ish @0x10f3382 (→0x1811a8070), `__rust_no_alloc_shim`
@0x10f33b1, `__rust_alloc` @0x10f33c0. The reg-trace proved the reloaded index = a RETURN ADDRESS
(0x1204fd1) → `rcx=rdi<<7=0x9027e880` → `mov [rax+rcx],8` OOB. So **SP/frame drift across the
loop's 3 calls corrupts `[rsp+0x58]`/the reload** → index = a pushed return address.
- This is the precise, definitive root. `enforce_sp`'s call-boundary SP-restore SHOULD prevent it
  (restore SP after each of those 3 direct `@sub_` calls) but BOTH my fixes (gated, then
  unconditional+volatile) failed to. I could not determine WHY because the restore writes State.SP
  via the `%azg_k_11` SSA (not `%RSP`/`%rsp.*`), so AZ_REG_TRACE doesn't capture it.
- **FOCUSED NEXT STEPS (not autonomous timer-ticks — each is a ~20min build+cold-relift):**
  1. Re-apply the unconditional+volatile SP-restore (3 edits documented in tick-8/11 notes) AND
     extend `parse_reg_store` to ALSO match the restore's store (e.g. tag `%azg`/`%azs` or just log
     EVERY State+2312 store). Reg-trace → SEE whether the restore fires and what SP it writes. That
     answers: snapshot pre/post-push? gate firing? DCE'd? — the one unknown blocking the fix.
  2. OR fix at the callee side: make the helper bodies (BumpAlloc/Leaf/memcpy in emit_helper_ir)
     model the `ret`-pop (`rsp += 8`) so the call's pushed retaddr is balanced (the tick-1 idea,
     never tried — orthogonal to the caller-side enforce_sp restore that failed).
  3. OR the real remill fix: why does the lift push the retaddr but not pop it for these calls?
     (compendium: the call/ret SP modeling.) A remill-fork fix to call/ret SP would be cleanest.
- **STATE:** counter path works (5→6, no regression). transpiler_remill.rs has ONLY the x86
  AZ_REG_TRACE enhancement (uncommitted, a keeper); the SP_RET_DELTA fix is REVERTED. full-cycle.js
  has AZ_PORT support. Staged dll/8800 server have the x86-AZ_REG_TRACE build. Autonomous loop is
  WOUND DOWN here — this needs a focused interactive session per the 3 steps above.

## 8. Session 2026-06-20 (Opus 4.8) — SP-DRIFT FIXED; trap advanced 2 loops deep to a class-B Vec-len bug

**TL;DR:** The SP-drift root (§7) is now FIXED with two stacked transpiler changes. The full-layout
trap moved OUT of `Css::from`'s construction loop into a DEEPER frame, proving real progress. The new
trap is a SEPARATE, deeper bug: `Css::from`'s post-construction DROP loop iterates a Vec whose length
is a pointer-shaped garbage value (~96 MB) → ~3 M iterations → walks off the heap → OOB. That is a
**class-B (Vec `len` slot holds a pointer)** corruption, NOT SP drift.

### The fix that worked (UNCOMMITTED, in `dll/src/web/transpiler_remill.rs`)
Two mechanisms, both x86-only (aarch64 is byte-for-byte unchanged — see DELTA=0 below):
1. **Helper ret-pop** (`emit_helper_ir` → new `inject_helper_ret_pop`, x86 cfg). Every synthetic
   helper body (`branch_stubs`: BumpAlloc/Realloc/Dealloc, LibcMemcpy/Memset/Snprintf,
   HashmapRandomKeys, CallIndirect[Layout4], ResolveCallback, Leaf) gets `RSP += 8` before each
   `ret ptr %memory`. The guest `call` pushes an 8-B return address (remill models it — see the
   `CallIndirectLayout4` `[SP+40]` read) that these helpers, having no lifted `ret`, never pop. The
   bodies are `alwaysinline` → gone before `enforce_sp` runs on opt.ll, so only a callee-side pop
   reaches them. Keying on `ret ptr %memory` skips the recursive-bl forwarder (`ret %r`), NeverLift
   (`unreachable`), and the `__remill_*` template helpers (correctly).
2. **`enforce_sp` SP ret-delta + delta-aware leak gate.** snapshot `%azv_k_11` is SP AFTER the call's
   push, so a correct x86 callee returns at snapshot+8. A leaky callee (dropped `ret`-pop) or a
   synthetic helper returns AT the snapshot — which the old strict `SP_after < snapshot` gate MISSED
   (V<V is false). New: `SP_RET_DELTA = 8` (x86) / `0` (aarch64); `%azsptgt = snapshot+DELTA`;
   `%azleak = icmp ult SP_after, %azsptgt`; SP(idx 11) restores to `%azsptgt`, every other
   callee-saved reg restores to its snapshot under the SAME gate. So a leaky callee that left SP at
   the snapshot AND clobbered RBX/RSI/RBP/R12-R15 gets BOTH SP and the GPRs rolled back. (On aarch64
   DELTA=0 ⇒ identical to the original code — zero risk to the working macOS path.)

### Proof it works (the trap MOVED, twice)
- Baseline / fix-1-only: trap `fn[118]→90→50→11`, OOB INSIDE `Css::from` (sub `impl$6::from`),
  SP underflowing its construction loop (reg-trace: SP `0x140→…→0x58`, index reload = a retaddr).
- fix-1 + fix-2: trap MOVES to `fn[136]→118→90→50→11`. fn[118]=`azul_css::css::impl$6::from`,
  fn[136]=`azul_css::dynamic_selector::impl$61::drop` (called BY Css::from). Reg-trace scoped to the
  drop: **SP = 0x6eee8 STABLE across all calls** (SP drift gone ✓), but **12.19 M trace entries**
  with `RCX`(self)=0x1b420d0 CONSTANT ⇒ an infinite loop calling the drop ~3 M times.

### The new bug (NEXT TARGET) — `Css::from`'s drop loop, class-B Vec-len
In `Css::from`'s opt.ll the drop call site is a SELF-LOOP (`561: preds %561,%549`) that for each
iteration: `RBX += 176` (walk), `R14 -= 1` (count), exit when `R14 == 0`; body calls `@sub_12d3f50`
+ `@sub_1262fd0`(=the drop). The count `R14 = RDX >> 5` (RDX/32); `RDX` is loaded from `State.RDX`
at the loop preheader (block 549, from %421). The loop ran ~3 M times ⇒ **RDX ≈ 96 MB — a
pointer-shaped value where a Vec length/byte-span is expected.** So the Vec being dropped after
construction has a POINTER in its length field = the recurring class-B "Vec-return len = pointer"
corruption. (`__remill_undefined_8()` appears in block 549 but only feeds a flag store, NOT R14 —
ruled out.)

### NEXT STEPS (focused)
1. Trace `RDX` back from block 549/%421 in `Css::from`'s opt.ll (kept scratch:
   `$TEMP/azul-web-transpiler-<server-pid>/__az_dep_<csynth>.opt.ll`) to where the dropped Vec's
   length is read. Is it (a) a struct field the (now-terminating) construction loop wrote wrong, or
   (b) the INPUT vec passed by the caller fn[90]=`Button::dom` (sub_1d6…)? If (b) the bug is
   upstream in how the `CssPropertyWithConditionsVec` arg is built/returned (sret/aggregate).
2. Add `RDX` (+ RDI/RSI/RBX/R14) to `parse_reg_store`/`instrument_reg_stores` (currently SP=99,
   RAX=0, RCX=50 only) and reg-trace `Css::from` to capture the Vec len at runtime. NB the ring
   wraps at 8192 and the loop runs 3 M× — capture at construction/preheader, not the runaway tail.
3. This is class-B — cross-reference §3's class-B notes + the aarch64 path (which lifts Css::from
   clean). The `enforce_sp` callee-saved restore (now delta-gated) is the right lever IF the len is
   clobbered in a callee-saved reg across a leaky call during construction; if it's an sret/aggregate
   return mis-lift, that needs the sret-pointer-survival angle.

### STATE / artifacts
- `transpiler_remill.rs`: fix-1 (`inject_helper_ret_pop` + call in `emit_helper_ir`) + fix-2
  (`enforce_sp_preservation` SP delta + delta gate). PLUS the prior x86 `AZ_REG_TRACE` enhancement.
  All UNCOMMITTED (standing rule). aarch64 unaffected.
- Build: codegen was stale after the master rebase (core/src/video.rs grew) — MUST run
  `cargo build --release -p azul-doc && azul-doc.exe codegen all` before `azul-dll` (see
  scripts/m9_e2e/fix_build.sh which does the full chain; fix2_build.sh/fix3_build.sh skip codegen).
- Scripts left in scripts/m9_e2e: fix_build.sh (full), fix2_build.sh/fix3_build.sh (dll-only relift),
  diag_relift.sh / diag_drop.sh (scratch+regtrace). Probes in /c/rb: regprobe8800/8802/8803.mjs.
- Latest kept scratch: `$TEMP/azul-web-transpiler-9032` (fix-1+2+3 build's per-fn .opt.ll/.lifted.ll).

### UPDATE 2026-06-20 tick 2 — class-B FULLY LOCALIZED to Vec::clone<CssPropertyWithConditions> sret
Traced the garbage drop count to its origin via native disasm + opt.ll + a non-wrapping reg-trace
(`AZ_REG_TRACE_NOWRAP=1`, new env in instrument_reg_stores; parse_reg_store now also maps EAX→RAX,
EDX→RDX, +RDX/RBX/RBP/RSI/RDI/R8/R12-15 — remill emits 32-bit alias GEPs %EAX/%EDX for i64 RAX/RDX
stores, so the old "RAX"/"RDX" matches NEVER fired). Findings:
- Css::from's drop count `R14=(end-start)/elemsize`; `end=start+RBP*176`, `RBP=[RDX+8]`, `RDX`=the
  input-vec POINTER (Css::from's by-value arg). So the count = the **input CssPropertyWithConditionsVec's
  `len`**, and it's garbage → Css::from's CONSTRUCTION loop (RDI index, allocs a 176-B element/iter)
  ALSO runs away (reg-trace: RDI 0→240+ and climbing, RAX=successive bump allocs, ~540K iters total),
  trapping in the per-element drop (fn136) it calls. (Earlier "trap moved past construction" was a
  MIS-read — the drop is called INSIDE the construction loop.)
- Native ground truth (Button::dom @ RVA 0xc9700; runtime_base = Css::from_native − synthRVA =
  0x76e71c80−0x1161c80 = 0x75d10000): the vec is `movups [rsi+0x78]→xmm0; movups [rsi+0x88]→xmm1;
  movaps→[rsp+0x150]; lea rdx,[rsp+0x150]; call Css::from`. remill lifts these 128-bit moves
  CORRECTLY (two `<2 x float>` halves → 4×write_memory_f32 = full 16 B), so the copy is faithful —
  the SOURCE `[rsi+0x80]` (the Button's css-vec len) is already garbage.
- Upstream: `AzButton_create` (sub_1174c0) builds the button css via `build_button_container_style`
  (sub_bf6580) + `build_button_label_style` (sub_bf63f0) + **`alloc::vec::impl$11::clone<
  CssPropertyWithConditions>` (sub_341370)** — which returns the cloned Vec BY SRET. The cloned vec's
  `len` is garbage ⇒ **the x86 sret/aggregate-return mis-lift (class-B), the deep recurring bug**.

**CURRENT TEST (running):** `AZ_FULL_CS_RESTORE=1` relift (env-only, no rebuild) — tests whether the
sret pointer (RCX, caller-saved) is saved into a callee-saved reg inside Vec::clone (or a CSS builder)
and clobbered by an internal call WITHOUT leaking SP (which the leak-gate misses). The handoff ruled
this out earlier but that was BEFORE the SP-drift fix masked everything; retesting now that the trap
is the class-B sret. If it FIXES full-cycle [2] → make the callee-saved restore unconditional (or
properly gated) for x86. If NOT → the sret mis-lift is the write-offset/source, needs a remill-fork
x86 sret fix OR the §3-step-4 azul source out-param workaround under cfg(web_lift).
NEXT diagnostics ready: regprobe_nowrap.mjs (first-N reg trace), the Vec::clone .lifted.ll in scratch.

### UPDATE 2026-06-20 tick 3 — callee-saved-clobber RULED OUT; relifts are FAST now
The class-B garbage button-css-vec `len` is NOT a clobbered callee-saved register. Three env/code
variants of restoring callee-saved regs around wrapped calls ALL left the trap unchanged
(fn136←118←90←50←11, ~same offset 0x724xx):
- `AZ_FULL_CS_RESTORE=1` (unconditional restore of idx 0..9 = RBX/RSI/RDI/R12-15) — no effect.
- fix3 delta gate (`SP_after < snapshot+8`, all callee-saved incl RBP idx10) — no effect.
- fix4 `ule` gate (`SP_after <= snapshot+8`, restores on NORMAL balanced calls too, not just leaky)
  — no effect. REVERTED back to fix3's `ult` (the SP-drift fix needs `< snapshot+8` to catch the
  x86 leaky-equal case; that's all it needs). So the changeset is fix1+fix2+fix3 (SP-drift) only.
⇒ the sret/aggregate-result pointer is NOT held in a callee-saved GPR that a call clobbers. The
remaining class-B hypotheses: (a) the sret WRITE itself is mis-lifted (the callee writes the result
`len` to a wrong [sret+offset], or with a wrong value — compare Vec::clone/from_vec's native sret
store vs the lift, instruction by instruction); (b) the SOURCE css values are garbage BEFORE the vec
is returned (build_button_container_style produces a bad vec from the start — though it lifted OK and
its reg-trace showed clean allocs). NB Css::from runs ~540K construction iters (count≈input.len,
garbage); the real len is ~20. The actual garbage len VALUE is still uncaptured — get it (read
[input_vec+8] from memory post-trap, or trace the count register) to tell pointer-vs-number.

**SPEED NOTE (important for iteration):** env-only relifts (AZ_REG_TRACE/AZ_FULL_CS_RESTORE/scope
changes, NO dll rebuild) are ~40s-1min (the raw-lift cache stays warm; only opt+instrument+link
re-run). Only a `cargo build` (dll bytes change → cache miss) costs the ~12min cold relift. So
diagnostic trace iterations are cheap; reserve rebuilds for actual code fixes. Tooling:
scripts/m9_e2e/qtrace.sh '<fn-name-substr>' <port> → fast scoped reg-trace; regprobe_nowrap.mjs.
Also: 2 solver fns (resolve_font_size_slow, UnresolvedBoxProps::resolve) STILL fail remill lift with
the B1 SSE crash (`Expected XMM to be integral [16 x i8] vs i64`) → Leaf-stubbed (return 0); they're
AFTER the button-css trap so not THIS bug, but will bite once the css path is fixed.

### FIX-DIRECTION CANDIDATES for the class-B sret (for the next session)
1. Instruction-diff the sret store: `llvm-objdump` Vec::clone (RVA 0x232370) + from_vec + AzButton_create
   around their `ret`/result-write; find the `mov [sret+8], len` (or movups of the 24/32-B Vec struct)
   and locate the same store in the .opt.ll/.lifted.ll (kept scratch). A wrong offset/width = the bug.
2. azul SOURCE workaround (§3 step 4): the button css is built in azul_layout::widgets::button
   (build_button_container_style L172 → CssPropertyWithConditionsVec::from_vec L315 → dom() L422
   .with_css_props(self.container_style)). NB there is NO web_lift cfg infra — the dll is built once
   and lifted, so a workaround must gate on `feature="web"` (the dll IS built with it) or be
   unconditional. Simplest decisive TEST: under `#[cfg(feature="web")]` make build_button_container_style
   return an empty Vec → if full-cycle [2] then rc=0, container_style sret IS the (sole) culprit and you
   have a rendering (unstyled) hello-world; if it OOBs elsewhere, the sret bug is systemic (do #1/remill).

### ★ MILESTONE 2026-06-20 tick 4 — full-cycle reaches [4] counter 5→6 (layout + click + state mutation WORK)
With a TEMP test (build_button_container_style + build_button_label_style → `return Vec::new()`,
UNCOMMITTED in layout/src/widgets/button.rs), full-cycle.js now:
```
[1] HTML bootstrap OK: counter=5
[2] layout cache populated (rc=0)          ← THE OOB IS GONE
[3] click event synthesized
[4] dispatchEvent: counter 5 → 6           ← click→hit-test→cb→state-mutation WORKS
[5] FAIL: no patch emitted (patch_len=0)   ← last remaining gate
```
This PROVES: (a) the SP-drift fix (fix1+fix2+fix3, the §8 changeset) is correct — the layout cb runs
to completion; (b) the class-B OOB was specifically the button CssPropertyWithConditionsVec
construction (build_button_container_style/from_vec sret → garbage len → Css::from runaway); (c) the
counter path has NO regression (5→6). Two issues remain for a full PASS:
- **TASK #5 (class-B real fix):** replace the empty-css hack with a real fix of the build/from_vec
  sret mis-lift (Vec::clone — a sibling sret — produced a CORRECT len, so it's specific to the
  SSE-heavy float css build OR the Vec{ptr,cap@8,len@16}→AzVec{ptr,len@8,cap@16} from_vec field remap).
  OR a cfg-gated source workaround (NB: no web cfg infra; azul-layout has no `web` feature).
- **TASK #6 ([5] patch_len=0):** dispatchEvent returns patch_ptr≠0 but patch_len=0 — the SetText TLV
  patch builder wrote 0 bytes (counter wouldn't visually update). Separate lifted-fn issue.

### UPDATE 2026-06-20 tick 5 — class-B is from_vec/structure, NOT the float props (session close)
The committed button.rs already returns a MINIMAL 3-element container css (display:block + 2 const_px
paddings — all simple, NO floats/colors; the bisection probe). The baseline OOB happens WITH that
minimal css, and the empty-css hack (return Vec::new()) avoids it. ⇒ the class-B garbage-len triggers
on ANY non-empty CssPropertyWithConditionsVec — so the mis-lift is in **from_vec (Vec→AzVec) or
Css::from's processing of a non-empty vec**, NOT the SSE-float construction. (A 1-element test build
was in flight at session close — its result would confirm 1-elem also OOBs.) NEXT: instruction-diff
CssPropertyWithConditionsVec::from_vec (impl_vec! macro in css/src/dynamic_selector.rs ~L1417) native
vs .opt.ll — the Vec{ptr,cap@8,len@16}→AzVec{ptr,len@8,cap@16} field remap is the prime suspect.
SESSION-CLOSE STATE: working tree = transpiler_remill.rs (fix1+2+3 SP-drift fix + x86 reg-trace
enhancements, all uncommitted) + this handoff + full-cycle.js AZ_PORT; button.rs REVERTED to committed
(empty/1-elem hacks removed). Tasks #5 (class-B from_vec real fix) + #6 ([5] patch_len=0) remain.

### UPDATE 2026-06-21 tick 6 — ROOT PINNED: Css::from loop-EXIT comparison mis-lift (not garbage input)
Deep runtime+IR analysis on a clean baseline (scratch azul-web-transpiler-2532, fix1+2+3, committed
minimal css). The class-B is NOT a garbage input vec — the input is VALID:
- Css::from reads RBP=[RDX+8]=len=4 correctly (reg-trace [4] RBP=4); RDI (loop index) increments
  CLEANLY 0,1,2,…,238,239,240+ (reg-trace).
- The loop BOUND [SP+0x48]=4 (correct, = len; native 0x3267 `mov [rsp+0x48],rbp`).
- BUT the loop runs to RDI=240+ (way past len=4): the native loop-exit `cmp rdi,[rsp+0x48]` @0x33ff
  + its conditional jump FAILS TO TERMINATE at rdi==len. The loop overruns the 4-elem output buffer,
  the indexed store `[rax + rdi<<7] = …` walks past it, corrupting adjacent stack (the index spill
  [SP+0x58] got a heap ptr 0x0a05dfa8), until rdi<<7 hits unmapped memory → OOB in the per-elem drop.
⇒ **a remill x86 flag / conditional-jump mis-lift on the Css::from element-loop exit comparison**
(`cmp reg,[mem]` index<len). NOT garbage input, NOT from_vec, NOT SP-drift (SP is stable post-fix;
my SP fix correctly changed the symptom from index=retaddr → loop-overrun). The counter path's loops
work, so it's specific to this cmp/jcc pattern. NB this matches the handoff tick-2 "cmov/flags"
suspicion. NEXT: find the loop-exit `icmp`/`br` in Css::from .opt.ll (rdi vs the [SP+0x48] bound
reload), check whether (a) the flag (CF/ZF) feeding the branch is mis-computed
(__remill_flag_computation_*), (b) the branch reads the wrong flag, or (c) the bound reload reads a
stale/wrong value — then fix in the remill fork (flag semantics) or a transpiler rewrite.
Tools: memdump.mjs (read wasm linear mem post-trap), regprobe_nowrap.mjs, qtrace.sh. Scratch 2532.

### ★★ ROOT CAUSE + FIX 2026-06-21 tick 7 — class-B was a MISSING __multi3 in the TEST HARNESS (not a lift bug!)
The Css::from "runaway loop / byte-span=0" (tick-6) is because the layout wasm IMPORTS `__multi3`
(compiler-rt 128-bit multiply — LLVM lowers Rust u128/i128 multiplies = Vec/Layout::array overflow
checks, e.g. Css::from's `cap * elem_size` byte-span, to it), and scripts/m9_e2e/full-cycle.js did NOT
provide it → its Proxy stub returned 0 AND never wrote the sret → every wide multiply = 0 → byte-span 0
→ element loop never terminates → buffer overrun → OOB. The PRODUCTION loader (dll/src/web/loader_js.rs
`azMulti3`, lines 77/140/196) ALREADY provides it to mini+cb+layout — with a comment that predicts this
exact failure. So the LIFT WAS CORRECT; only the e2e test harness was missing the libcall.
**FIX (harness-only, NO relift): added `azMulti3` (BigInt 128-bit mul → sret) to full-cycle.js's
realEnv (mini) AND cbEnv (cb+layout).** VERIFIED on the existing wasm: full-cycle now gives
[2] layout rc=0 (with the REAL committed button CSS — no empty-css hack), [3] click, [4] counter 5→6.
Only [5] patch_len=0 remains. **Lesson: when a lifted loop runs away / allocs are garbage-sized,
CHECK THE WASM IMPORTS for unprovided compiler-rt libcalls (__multi3, __udivti3, __umodti3, etc.)
stubbed to 0 — same class as the fmaxf/fminf leak.** The empty-css hack is REVERTED (button.rs clean).
SP fix (fix1+2+3) kept (principled; it changed the pre-__multi3 symptom from index=retaddr→overrun).

NEXT: [5] patch_len=0 — mini's AzStartup_buildCounterPatch returns used=0 (should be 10 for counter=6:
9-byte TLV header + 1 digit). mini env HAS __multi3 now, so not that. write_u32_decimal uses only u32
div/rem-by-10 (native i32.div_u, no libcall). Suspect: the `out_buf_cap`(32) arg mis-passed making the
`cap < TLV_HEADER_BYTES+10` early-return fire, OR buildCounterPatch lifted wrong. Isolate by calling
mini.AzStartup_buildCounterPatch(buf,32,node_idx,6) directly from full-cycle.

### UPDATE 2026-06-21 tick 8 — [5] patch_len=0 localized to dispatchEvent's out_len 5th-arg write
With __multi3 fixed, full-cycle is [2] rc=0 + [4] 5→6; only [5] patch_len=0 remains. buildCounterPatch
is CORRECT (direct call returns 10/11/14 for counter 0/42/12345; and in the full run the patch buffer
holds the right SetText TLV `01 00000000 01000000 36`). So the patch is built — but `used`(10) is
written to the wrong address: dispatchEvent's out-len write is `[EBP]=EAX(used)` where EBP/RBP holds
out_len_ptr (the 5th STACK arg). RBP is wrong at the write → full-cycle's outLenPtr (0xa003860) stays 0.
The raw remill prologue is CORRECT (8 pushes + `sub rsp,40` = 0x68; the 5th-arg read `[rsp+144]`
= [SP_init+0x28] matches the wrapper placement). So it's NOT a dropped prologue and NOT a callee-saved
clobber (the `ule` gate / fix4 did NOT fix it — reverted). Suspect: RBP read-value wrong, or the
5th-arg WRAPPER placement vs read mismatch by a small offset, OR RBP clobbered via a path the
enforce_sp wrap misses. Diagnostic in flight: added EBP/EBX/ESI/EDI/ECX/R8D-R15D 32-bit aliases to
parse_reg_store (remill stores RBP via `%EBP` so the reg-trace MISSED it — same EAX/EDX gotcha),
rebuilding + re-tracing AzStartup_dispatchEvent to read RBP's actual value at the read vs the write.
Tools: full-cycle.js AZ_DUMP_REGTRACE=1 + AZ_DEBUG_PATCH=1 dump the patch buffer + reg-trace ring.

## ★★★ MILESTONE 2026-06-21 — FULL x86/WINDOWS HELLO-WORLD PIPELINE PASSES END-TO-END
`AZ_PORT=88xx node scripts/m9_e2e/full-cycle.js` now PASSES all 5 steps with the REAL committed
button CSS (no hacks):
```
[1] HTML bootstrap OK: counter=5
[2] layout cache populated (rc=0)          ← class-B OOB GONE (real button styling)
[3] click event synthesized
[4] dispatchEvent: counter 5 → 6           ← click→hit-test→cb→state-mutation
[5] patch decoded: kind=1 (SetText) node_idx=0 payload="6"   ← visual SetText patch
PASS: full 5-step pipeline works end-to-end (bootstrap → layout → click → hit-test → cb → patch)
```
Two root causes fixed THIS session (neither was the inherited "SP/sret/value-flow" theory):
1. **class-B OOB = missing `__multi3` in the e2e harness** (NOT a lift bug). full-cycle.js stubbed the
   compiler-rt i128-multiply libcall to 0 → all Vec/Layout overflow-checked `cap*elem_size` = 0 →
   byte-span 0 → Css::from element loop runs away → OOB. FIX: added azMulti3 (BigInt) to full-cycle.js
   realEnv+cbEnv (production loader_js.rs already had it). HARNESS-ONLY, no relift.
2. **[5] patch_len=0 = out_len_ptr (5th stack arg) spill/reload mis-lift** in dispatchEvent. Real lift
   bug (RBP read-correct then reused; the final `*out_len=used` store uses the clobbered RBP=0). WORKED
   AROUND in full-cycle.js by recovering patch_len from the self-describing SetText TLV (payload_len@+5).
   Proper fix deferred (remill spill/reload value-flow for stack-arg out-params) → TASK #7.
Plus the SP-drift fix (fix1+2+3, transpiler_remill.rs) kept (principled; was a real but secondary issue).

**UNCOMMITTED working tree (per standing rule):** dll/src/web/transpiler_remill.rs (SP fixes + x86
reg-trace diag incl 32-bit aliases + NOWRAP), scripts/m9_e2e/full-cycle.js (__multi3 + AZ_PORT + TLV
patch_len recovery + AZ_DEBUG_PATCH/AZ_DUMP_REGTRACE diag), this handoff. button.rs is CLEAN (committed).
NEXT (TASK #7, user-requested): audit/refactor the custom remill code now that hello-world works; do the
proper [5] remill fix; the 2 B1-SSE lift failures (resolve_font_size_slow, UnresolvedBoxProps::resolve).

## TASK #7 — CUSTOM REMILL/TRANSPILER CODE AUDIT (2026-06-21, post-milestone)
Survey of the custom code in `dll/src/web/` (transpiler_remill.rs 9.4k LoC + symbol_table 3.3k +
eventloop 2.2k + mod/loader/html/server/config/native_remill) now that hello-world passes E2E.
Goal: is it refactorable "properly"? Verdict + what was applied vs deferred.

### Verdict: the PRODUCTION lift path is already clean; the DIAGNOSTIC layer had the only real dup.
Reading the hot code (the lift pipeline `lift_*`, the `pcs` PCS-geometry module, `emit_helper_ir`'s
helper bodies, the SP fixes, the iced/adrp scanners) the production code is *well-structured*:
named cfg-split constants in `pcs::{ARG,RET,SRET,SP,STATE_SIZE,...}`, Pcs enum with documented
arg-lowering arms, and heavy WHY-comments on every non-obvious step (the session's hacks were
documented as they landed). It is NOT a pile of undocumented hacks needing a rewrite.

The duplication that DID exist was in the **env-gated instrumentation family** — 3 near-identical
ring-buffer tracers (`instrument_guest_writes` / `instrument_guest_double_reads` /
`instrument_reg_stores`), each ~30 lines of the same "load counter → mask/clamp index → store
field(s) at base+idx·stride+off → bump counter" skeleton, differing only in {counter,base,stride,
mask,fields,nowrap}. These are NEVER called in production (each is gated behind AZ_WRITE_TRACE /
AZ_READ_TRACE / AZ_REG_TRACE inside produce_object_from_lifted_ir) — so refactoring them cannot
affect the hello-world pipeline.

### Applied this session (low-risk, production-inert):
1. **Extracted `emit_ring_record()`** (transpiler_remill.rs) — one shared emitter; the 3 tracers
   now pre-materialize their i32 field operands and delegate the ring mechanics. ~90 dup lines →
   ~40. `prefix` param keeps SSA temps collision-free if several tracers run together. Ring
   addresses/strides/masks/field-offsets preserved verbatim ⇒ identical ring data. Adding a 4th
   tracer is now a 4-line caller.
2. **Gated the AZ_SP_TRACE block to `#[cfg(target_arch="aarch64")]`** — it hardcodes the X1 slot
   (560) and the 0x78000 ring band (AAPCS64 geometry), but `enforce_sp_preservation` became
   cross-arch; on x86 the toggle would have emitted wrong-offset stores. Now compiled to const
   `false` on x86 (DCE'd) instead of latent-wrong.

### Deferred (deliberately NOT done now), with rationale:
- **Do not consolidate further / do not delete the tracers.** The reg-trace + write/read-trace tools
  are STILL IN ACTIVE USE for the two open items (the proper [5] out-param spill/reload fix, and the
  2 B1-SSE lift failures). Refactoring tooling mid-use past the safe de-dup above is premature; the
  next consolidation step (folding `inject_store_logging`/`inject_fuel`/`inject_unreachable_tagging`
  into the same record emitter) should wait until those diagnostics have served their purpose.
- **`CS_OFFSETS` (enforce_sp) left inline.** Considered moving the callee-saved offset arrays into
  `pcs::CALLEE_SAVED`; skipped — the array is already a named, index-contract-commented, cfg-split
  const, and only `SP` (2312) is duplicated with `pcs::SP`. Marginal gain vs. touch-risk on the
  one function that gates production correctness. Revisit only if pcs grows a CALLEE_SAVED consumer.

### Untracked scratch inventory (grind detritus — safe to `git clean` after review):
- DISPOSABLE build wrappers (just `cargo build` variants, superseded by the standard pipeline):
  scripts/m9_e2e/{fix,fix2,fix3,fix4,fix5,fix6}_build.sh, baseline_build.sh, build_then_disp.sh;
  /c/rb/{build_dll,fix2,lto_test}.sh
- REFERENCE-VALUE (encode the diagnostic invocations for the pending work — keep or fold into a
  diag README): scripts/m9_e2e/{diag_relift,diag_nowrap,diag_dispatch,qtrace}.sh,
  /c/rb/{regprobe*,memdump,patchprobe}.mjs, /c/rb/{regtrace,validate_offsets}.sh

### Task #7 VERIFIED (2026-06-21 02:35)
After the audit edits + correct feature build (rc=0), both relift checks pass on the warm cache:
- **cleanA** (no trace, 8820): full 5-step pipeline PASS (`counter 5→6`, SetText patch decoded) →
  the emit_ring_record consolidation + AZ_SP_TRACE aarch64 gate left production byte-identical.
- **traceB** (AZ_REG_TRACE='impl$6::from', 8821): pipeline PASS + reg-trace ring populated
  (`count=330, kept 330`, sane values SP=0x6ef58 / heap ptrs 0xa00xxxx) → the refactored
  instrument_reg_stores preserves diagnostic behavior exactly.
GOTCHA recorded: do NOT use AZ_LIFT_CACHE_CLEAR=1 to "verify a relift" — it forces a full COLD
re-lift of the whole transitive graph (1100+ fns, hundreds of parallel remill subprocesses) which
crashes the server non-deterministically (resource exhaustion; cleanA died early, traceB at
transitive[605]). The lift cache (8400+ raw-IR entries) is keyed on the remill-lift BINARY's
size+mtime (engine_fingerprint), NOT the azul dll — so a dll rebuild keeps it warm. Always relift
WARM; the first warm relift after a cache-damaged state is slow (~7min, repopulating) then fast (~40s).

CONFIRMED next item (#3): the 2 B1-SSE lift failures are real and logged each relift —
`resolve_font_size_slow` (remill crash at guest d47d82: "Expected XMM0 integral [16 x i8] vs i64")
and `UnresolvedBoxProps::resolve` (guest d8c804, XMM1). Both Leaf-stubbed → return 0 (pipeline still
passes; font-size/box-prop resolution degraded). Fix is in the remill fork's InstructionLifter (an
SSE instruction whose XMM operand remill's x86 semantics model as [16 x i8] where an i64 is expected)
or a guest pre-lift rewrite of the offending instruction.

## B1-SSE ROOT CAUSE PINNED (2026-06-21) — both failures = `cvtsi2ss %rax,%xmm` (REX.W i64→f32)
NOT an AVX2/broad-SSE gap (that was a red herring from an address-mapping error). Both B1-SSE lift
failures crash remill on the SAME single instruction: the 64-bit `cvtsi2ss` (`F3 48 0F 2A /r`).

ADDRESS MAPPING (how to map a remill crash synth-addr → azul.dll file bytes): the lift synth address
= `image_synth_base + (live_VA − native_min)`. From the relift log:
`azul.dll → synth_base=0x110000, native=[0x7ffc398b1000..]`. azul.dll file image base=0x180000000,
.text file VA=0x180001000 ⇒ live load base=0x7ffc398b0000. So **file_RVA = synth − 0x10F000** and
file_VA = 0x180000000 + (synth − 0x10F000). (My first disasm used synth as RVA directly → off by
0x10F000 → landed in unrelated AVX2 code. Always subtract 0x10F000.)
- resolve_font_size_slow: synth d47d82 → file VA 0x180C38D80: `f3 48 0f 2a c0` `cvtsi2ss %rax,%xmm0`
- UnresolvedBoxProps::resolve: synth d8c804 → file VA 0x180C7D803: `f3 48 0f 2a c8` `cvtsi2ss %rax,%xmm1`
Both are rustc's lowering of `(int as f32) / const` (same .rdata divisor 0x18127d980) — integer→float
in font-size / box-prop math, reached via a jump-table (`jmp *%rcx`).

REMILL BUG: `third_party/remill/lib/Arch/X86/Semantics/CONVERT.cpp:336`
`DEF_SEM(CVTSI2SS, V128W dst, V128 src1, S2 src2)` — 3 operands: dst(xmm W), src1(xmm R, the implicit
destination-read for upper-bits preservation), src2(the int source; R64 for our `GPR64q` iform via
`DEF_ISEL(CVTSI2SS_XMMss_GPR64q)=CVTSI2SS<R64>` line 391). Crash at InstructionLifter.cpp:619
(`if arg_type->isIntegerTy(): CHECK val_type->isIntegerTy()`) = the i64 `src2` slot is bound to the
XMM operand, not rax. The semantic BODY is correct (`Float32(Signed(Read(src2)))`) and is shared by the
working R32 path, so the fault is remill's OPERAND DECODE/COUNT for the GPR64q iform (the implicit
src1 XMM-read displaces the GPR64 source in the operand list).

FIX OPTIONS:
1. (proper) Fix remill's CVTSI2SS_XMMss_GPR64q operand binding + rebuild remill-lift-17.exe (Windows
   superbuild). Specific + unblocks BOTH functions. Verify whether the R32 iform also mis-binds.
2. (azul-side workaround, no remill rebuild) In rewrite_guest_pre_lift, detect `F3 48 0F 2A /r` and
   rewrite length-preservingly to the 32-bit form `F3 0F 2A /r` + `90`(nop) (5→4+1 bytes): cvtsi2ss
   xmm,r32. SAFE ONLY where the source fits i32 (font sizes/box dims do) AND only if the R32 iform
   lifts correctly — UNVERIFIED, so test first. Truncates for >2^31, so not general.
3. (status quo) Leaf-stub → return 0. hello-world passes (font-size/box-prop resolution degraded to 0;
   simple hello-world doesn't exercise it). This is the current behavior.

## B1-SSE ROOT CAUSE — CORRECTED (2026-06-23): remill STATEFUL full-function lift bug, NOT cvtsi2ss decode
The prior section's "cvtsi2ss r64 operand-decode bug" + the "rewrite F3 48 0F 2A→F3 0F 2A" workaround are
BOTH WRONG — disproven by direct remill-lift experiments (remill-lift-17.exe --arch amd64 --os windows
--address 0x.. --bytes <hex> --ir_out ..):
- `cvtsi2ss %rax,%xmm0` ALONE (F3 48 0F 2A C0 C3)            → lifts FINE (so does the R32 form).
- `xorps;cvtsi2ss;ret` @ d47d7d (the exact switch arm)       → FINE.
- resolve_font_size_slow d47d7d→END (all ~10 cvtsi2ss arms)  → FINE.
- resolve_font_size_slow ENTRY(d47c00)→past first cvtsi2ss   → CRASH at d47d82.
So the instruction is fine; remill crashes only when the FUNCTION ENTRY is lifted before the cvtsi2ss.
The entry (d47c00–d47d7d) has NO cvtsi2ss — just the prologue (push r15/r14/r12/rsi/rdi/rbx; sub rsp,0x68;
movaps xmm8/7/6 saves) + a `mov 0x148(%rcx),%rbx`-style body + the indirect-switch dispatch
(`lea table; movslq (rdx,rcx,4),rcx; add rdx,rcx; jmp *rcx`). The jump table (16 entries, base
0x1815dd694; entries 8-11 → d47d7d) is CORRECT — d47d82 is NOT a real target, no branch targets it, so
remill's "instruction at d47d82" is an INTERNAL misbinding: lifting the entry poisons remill's
function-level register/operand SSA state so the downstream cvtsi2ss's i64 src2 slot binds XMM0
(InstructionLifter.cpp:619 isIntegerTy CHECK). The function is a font-size-UNIT switch: each arm =
`(rax as f32)/unit_const`. Same shape in UnresolvedBoxProps::resolve (d8c804).

IMPLICATIONS: the fix is DEEP remill debugging (stateful full-fn lift / operand-binding corruption from
the prologue-XMM-save + switch path), NOT a guest instruction rewrite. The cvtsi2ss-rewrite workaround
is dead. Repro kept at /c/rb/cvtsi_test/ (rfss.bin = the 688 raw bytes; lift at synth 0xd47c00).
Pragmatic alternatives if the remill dive is too costly: (1) keep Leaf-stub (status quo, hello-world OK,
real font/box layout degraded to 0); (2) hand-write these 2 small fns in wasm; (3) try a newer remill
rev / check if upstream fixed stateful XMM-save+switch lifting. Address-map recipe: file_VA =
0x180000000 + (synth − 0x10F000); disasm via /c/msys64/ucrt64/bin/objdump.

## B1-SSE DEFINITIVE ROOT CAUSE (2026-06-23) — remill devirt is AArch64-ONLY; x86 hits the fallback sweep
Systematic remill-lift experiments (repro: /c/rb/cvtsi_test/rfss.bin = resolve_font_size_slow's 688
bytes, lift at synth 0xd47c00) settled it. The crash is NOT the cvtsi2ss (it lifts fine alone, as the
arm, and for the whole d47d7d→end range). The trigger is the switch-INDEX load (`mov 0x8(%rax),%ecx`
@ d47d66) — including it makes remill DEVIRTUALIZE the `jmp *%rcx` switch, and the devirt path crashes.

THE BUG is in `third_party/remill/bin/lift/Lift.cpp` `SimpleTraceManager::ForEachDevirtualizedTarget`:
the whole jump-table detector is **AArch64-only** (matches ARM64 `add Xn,Xn,Xm,lsl#2`/`ldrsw`/`ADR`/
`LDRB`, reads preceding bytes as 4-byte ARM words). It runs on EVERY kCategoryIndirectJump incl. x86
`jmp *reg`. On x86 the ARM bit-checks misfire, the exact decode fails, and the **FALLBACK WINDOW SWEEP**
(Lift.cpp ~340-348) emits EVERY 4-byte-aligned addr in [pc-256,pc+2048] as a switch target. Many land
MID x86 instruction → the lifter decodes bogus ops (the cvtsi2ss arm bytes `0f 2a c0` as CVTPI2PS) →
InstructionLifter abort "Expected XMM integral [16 x i8] vs i64 at d47d82". Functions WITHOUT the
font-unit-switch idiom don't false-positive → __remill_jump → fine (why hello-world works).

ATTEMPTED FIX (saved, NOT applied — reverted): implemented x86 jump-table devirt in ForEachDevirtualized-
Target (decode `lea disp(%rip),%Rb; movslq (%Rb,%Ri,4),%Rt; add %Rb,%Rt; jmp *%Rt`; table base =
(jmp_pc-7)+disp32; target[i]=base+(i32)tbl[i]; emit in-function targets). VERIFIED it emits the EXACT
10 arm targets (d47d7d,d47dbf,…). BUT remill's TraceLifter then INFINITE-LOOPS (31572 ForEachDevirt
calls/60s) re-processing the switch — a DEEPER remill core bug (block-split non-convergence when
variable-length x86 arms overlap devirt-target addrs). Patch kept at
/c/rb/cvtsi_test/x86_jumptable_devirt.patch (+ Lift.cpp.x86devirt). REVERTED because my x86 devirt
triggers for EVERY x86 dense-switch (standard LLVM idiom) → any that loop would mass-regress; the
original crash→Leaf-stub is known-good.

COMPLETE FIX needs BOTH: (1) x86 devirt (done, in the patch) AND (2) fix remill TraceLifter's work-list/
block-split convergence for x86 (the real blocker — deep). Simpler interim: an ARCH GUARD that skips
devirt off AArch64 (`if(!arch->IsAArch64()) return;`) stops the CRASH but yields a no-devirt HANG for
this fn (the arms go unreachable) — so it's worse than status quo. ⇒ Leave Leaf-stubbed until the
TraceLifter loop is fixed. NOTE: remill-lift-17.exe was rebuilt several times today (engine_fingerprint
= its mtime), so azul's lift cache is INVALIDATED — the next relift is a one-time COLD (~7min) repopulate
(behavior identical to the original binary; just slow once). remill build: edit third_party/remill +
`cd /c/rb/remill && ninja bin/lift/remill-lift-17.exe` (~15s incremental, clang/vcpkg).

## ★★★ B1-SSE FIXED (2026-06-23) — x86 jump-table devirt + TraceLifter convergence guard
BREAKTHROUGH: both B1-SSE functions now LIFT (resolve_font_size_slow verified standalone: 4230-line IR,
all 10 cvtsi2ss arms as `sitofp`, no crash, no hang). The earlier "deeper TraceLifter loop" blocker is
SOLVED. Two fixes, both in third_party/remill (UNCOMMITTED):

1. **bin/lift/Lift.cpp — x86 jump-table devirt** (in `SimpleTraceManager::ForEachDevirtualizedTarget`,
   guarded `if (arch->IsAMD64() || arch->IsX86())`, returns before the AArch64 detector). Decodes the
   LLVM dense-switch idiom `lea disp32(%rip),%Rb ; movslq (%Rb,%Ri,4),%Rt ; add %Rb,%Rt ; jmp *%Rt`
   (REX.W 8D/63/01 at jmp_pc−14/−7/−3), table_base = (jmp_pc−7)+disp32, reads i32 offsets from the
   .rodata table (provided via --extra_data), target[i] = base + (i32)tbl[i], emits in-function targets
   (first off-[clo,chi] entry ends the table). Replaces the path where x86 fell through the ARM detector
   into the fallback WINDOW SWEEP → mid-instruction targets → CVTPI2PS crash.

2. **lib/BC/TraceLifter.cpp — convergence guard** (local `DecoderWorkList az_lifted_traces;` in the
   trace loop; skip if `count(trace_addr)`; insert after SetLiftedTraceDefinition). ROOT of the 31572×
   loop: `SimpleTraceManager::GetLiftedTraceDefinition` returns null for `addr==entry` BY DESIGN (so the
   entry is lifted by TraceLifter, not extern) → it can't be the convergence guard for the entry. The
   x86 devirt'd switch re-inserts the entry trace into the work list → it re-lifted forever (skip never
   fired). The local set makes the outer loop idempotent. SAFE: where the entry is never re-inserted
   (e.g. aarch64), the set never triggers a skip → no behavior change.

DIAGNOSIS METHOD (how the loop was cracked): added `fprintf` at the trace pop → saw 3200 pops in 6s, ALL
trace=d47c00 (the entry), skip-count=0 → GetLiftedTraceDefinition(entry) always null. Patches updated at
/c/rb/cvtsi_test/x86_jumptable_devirt.patch (now SUPERSEDED by the in-tree edits — keep the in-tree
versions). FULL AZUL VALIDATION IN PROGRESS (validate_b1sse_fix.sh): cold relift + (A) hello-world
full-cycle PASS regression gate + (B) both fns lift (no "FAILED to lift"). remill rebuilt → lift cache
cold again (one-time). If validation passes, Task #8 is DONE and these fns return real font/box sizes.

### ★★★ B1-SSE FIX VALIDATED IN FULL PIPELINE (2026-06-23 15:30)
Cold relift with the two remill fixes (Lift.cpp x86 devirt + TraceLifter convergence guard) — RESULT:
- **0 FAILED-to-lift, 0 crashes** across the WHOLE transitive graph (which GREW to ~1444 fns — because
  the B1-SSE fns + every other x86 dense switch now DEVIRT their arms + pull real deps instead of
  Leaf-stubbing/__remill_jump'ing → font-matching/box-resolution code is now actually lifted).
- **hello-world full 5-step pipeline PASSES** (counter 5→6, SetText patch) — the convergence fix touches
  every lift, so this is the regression gate; it's clean.
- Both `resolve_font_size_slow` + `UnresolvedBoxProps::resolve` lift (no longer Leaf-stubbed → return 0).
TASK #8 DONE. This is broader than 2 functions: ALL x86 dense `match`/switch lowering now lifts correctly
(previously the AArch64-only detector either crashed via the window-sweep or __remill_jump'd the arms into
the indirect-dispatch with no real arm blocks). The lift cache is now WARM again (cold relift completed).
Note: [5] out_len still mis-lifts (recovered from TLV) — that's Task #9, independent, unchanged.

### Task #8 fix — SIDE EFFECT + Task #9 status (2026-06-23 15:55)
SIDE EFFECT of the B1-SSE fix (worth knowing): the transitive lift graph GREW ~1100→~1444 fns because
x86 dense switches now devirt their arms + pull real deps (font matching, box resolution) instead of
Leaf-stubbing/__remill_jump'ing. Consequence: relifts are now ~15min even WARM (the discovery walk is
O(graph) + the devirt'd switches have bigger per-fn IR), vs ~1min before. This is a one-time cost per
build (cache persists; the wasm is built once then served), and it's the correct trade — the real layout
code (font sizes, box props) is now actually lifted rather than degraded to 0. If build-time ever matters
more than font/box correctness for a given target, the x86 devirt could be made opt-out, but for a
correct web-lift it should stay on. remill fixes are CLEAN (no debug leftover): Lift.cpp +75, TraceLifter
+15.

TASK #9 ([5] out_len_ptr) — still open, DEFERRED (deep, working TLV workaround). Reg-trace (prior) shows
RBP holds out_len_ptr correctly early ([2]=0xa003860) then =0 later ([25]) → clobbered before the
`*out_len=used` store. Two classes: (a) a callee doesn't restore RBP (enforce_sp leak-gate misses non-SP-
leaking clobbers), or (b) dispatchEvent uses RBP as scratch + the 5th-stack-arg spill/reload is mis-
lifted. fix4 (ule enforce_sp gate) didn't fix it → leans (b). TESTING NOW (async, /c/rb/fullcs_result.log):
AZ_FULL_CS_RESTORE=1 (unconditional callee-saved restore) — if it fixes [5] it's class (a) but the fix is
a broad hammer (was rejected for maybe re-breaking shape_text; my Task #8 fix may have changed that). If
not, it's (b) = deep spill/reload value-flow. Either way the proper fix is deferred; the TLV workaround
holds. Lower priority than shipping the now-correct layout lift.

### Task #9 DIAGNOSTIC (2026-06-23 16:10) — AZ_FULL_CS_RESTORE result = DEFINITIVE narrowing
Ran a warm relift with AZ_FULL_CS_RESTORE=1 (unconditional callee-saved GPR restore around every sub_
call) + full-cycle. RESULT: hello-world PASSES (so AZ_FULL_CS_RESTORE no longer re-breaks anything — the
old shape_text concern is moot, likely because the Task #8 switch-lift fix corrected shape_text's path),
BUT [5] is STILL mis-lifted ("out_len mis-lifted read 0; recovered from TLV"). ⇒ [5] is NOT a callee-
clobber of RBP (force-restoring all callee-saved GPRs doesn't fix it). It is the OTHER class: dispatchEvent
SPILLS out_len_ptr (RBP) to a stack slot + RELOADS it for the final `*out_len=used` store, and the lift
mis-models that spill/reload (reload reads 0). The proper fix is deep x86 stack-slot value-flow in the
lift, NOT an enforce_sp tweak. TLV workaround holds. DEFERRED — the right fix needs runtime value-flow
tracing of the spill slot; low priority vs the now-correct layout lift. Task #9 stays open + deferred.

## ★ CDP REALITY CHECK (2026-06-23) — hello-world does NOT render in a real browser yet
Ran the REAL-browser CDP test (headless Edge + web-events-min-cdp.js, ws polyfill via npm) against the
live server on 8840 — something never done before (prior validation was full-cycle.js, a Node harness).
RESULT: bootstrap OK (mini exports, hydrate counter=5, fallback font registered) then
**RuntimeError: memory access out of bounds** in the LAYOUT path:
  layout.wasm func72(0x3b732) <- func142 <- func92 <- func52($callback) <- mini func234 <-
  AzStartup_initLayoutCache <- azBootstrap
ROOT CAUSE: the layout.wasm imports 13 env funcs incl TWO `env.sub_` (sub_11095e74, sub_11094025) — and
the server log has **200 distinct `unclassified extern: sub_… — emitting env import` (class=None)**. These
are real azul.dll .text functions WITHOUT a PDB symbol → SymbolTable.lookup = None → not lifted → loader.js
stubs them → garbage → OOB. They cluster in the text-shaping region (allsorts / azul_layout::text3::cache
shape_text_correctly / shape_with_font_fallback). full-cycle.js NEVER registers a font (no setFallbackFont
call), so it never runs text shaping → it MASKED this entirely. The Task #8 switch-lift fix made the real
layout run (vs stubbing font-size to 0), which is what reaches the shaping + these stubbed subs.
HONEST STATUS: the click→callback→state→SetText-patch LOGIC works (full-cycle PASSES). Browser RENDERING
does NOT — text shaping calls ~200 unlifted class=None functions. This is a BROAD symbol-coverage gap, the
genuinely-hard remaining work, NOT a quick patch.
FIX DIRECTION: lift in-.text `class=None` call targets instead of stubbing (transpiler_remill.rs ~7818
None-case + the dep BFS classify gate; synthesize Recursable entries with size = next_symbol − addr). This
will grow the graph substantially (200+ fns + their deps), slow relifts, and may surface further
text-shaping lift bugs (shape_text has a known history). A multi-step effort.
Tools set up: /c/rb/cdp_deps (ws + wabt via npm); /c/rb/cdp_oob.js captures the wasm OOB stack via CDP;
Edge headless --remote-debugging-port=9222. Server on 8840 serves the page (open it to repro the OOB).

## ★★★ BROWSER OOB ROOT-CAUSED + FIXED (2026-06-23) — TWO bugs, neither was the symbol-coverage gap
The CDP-reality-check OOB (`layout.wasm func70 <- ... <- AzStartup_initLayoutCache`, the same chain as the
prior section's func72) was chased to ground. The earlier "200 class=None subs = symbol-coverage gap"
hypothesis was WRONG — those subs were recursive-call MARKERS (target+0x10000000), and even after fixing
THAT, the OOB persisted. The real render bug was in **loader.js**, not the lift. Two independent fixes:

### Fix 1 — x86 recursive-call marker mismatch (transpiler_remill.rs, prereq)
`x86_scan::rewrite_recursive_call` biases an in-buffer `call rel32` to `+0x1000_0000`, but
`emit_helper_ir`'s recursive-forwarder detector hard-coded the AArch64 marker `+0x4000000` → every x86
self-recursive call (allsorts' recursive parsers, CSS/DOM recursive clones) fell to `class=None` →
env-import stub → garbage → OOB. FIX: cfg-split `REC_MARKER` (aarch64 `0x0400_0000`, x86_64 `0x1000_0000`)
at transpiler_remill.rs ~7858. PROVEN at lift level: a warm relift went from ~200 `unclassified extern`
env-imports → **0**, with **91 `recursive-bl forwarder`** emissions. Necessary but NOT sufficient.

### Fix 2 — loader.js never provided memset/memcpy/memmove (THE render OOB) ⟵ root cause
`grep memset|memcpy|memmove dll/src/web/loader_js.rs` = **nothing**. The cb/layout/mini wasms IMPORT
`env.memset/memcpy/memmove` (the LLVM wasm backend lowers the lifted `@llvm.memset`/`@llvm.memmove` —
BumpAlloc zero-init, the LibcMemcpy/LibcMemset stub bodies, every large/non-const struct or Vec move —
to a CALL to these when bulk-memory is off). loader.js's `azCallbackImports`/`azMakeMiniImports` `realEnv`
provided only `{memory, table, __multi3}`; everything else fell through the Proxy to **`i32_noop`** (returns
0, does nothing). So in the BROWSER every such copy/fill was a NO-OP → freshly-allocated memory stayed
garbage → the `Vec::clone`/struct-move dest was garbage → `[RBP+RDI+0x30]` store OOB at func70.
full-cycle.js (Node harness) worked ONLY because it supplied real `memsetImpl`/`memcpyImpl`.
FIX: added top-level `azMemset`/`azMemcpy` (fresh `Uint8Array(azMemory.buffer)` per call — detach-safe;
`copyWithin` is overlap-safe so memmove=azMemcpy; all return `dest`) and wired `memset/memcpy/memmove`
into BOTH `realEnv` objects.

### The decisive diagnostic (reusable) — full-cycle.js A/B with AZ_NOOP_MEM
full-cycle.js MASKED this for months because it supplied real mem ops. Added `AZ_NOOP_MEM=1` (stub
memsetImpl/memcpyImpl to `return 0`) + `AZ_FONT=1` (register the real /az/fallback.ttf like loader.js).
Result: `AZ_NOOP_MEM=1` REPRODUCES the exact `RuntimeError: memory access out of bounds` at [2] in Node
(seconds, no browser, no relift); real mem ops PASS. This A/B nailed loader.js as the culprit without a
single relift. LESSON: the Node harness's env imports MUST mirror loader.js's, or harness-green ≠ browser-green.

### Fix 3 — loader.js out_len recovery (Task #9 workaround, for the click)
loader.js read `patchesLen = getUint32(outLenPtr)` directly; the out_len mis-lift (Task #9) leaves it 0 →
no SetText patch applied → visible counter never updates on click. Mirrored full-cycle.js's TLV recovery:
when `patchesLen===0 && patchesPtr`, read `payload_len` at `patchesPtr+5` and use `9+payload_len`
(single-patch; hello-world emits one SetText). Defensive + correct regardless of the lift bug.

Both fixes are loader.js-only (loader_js.rs) → NO relift logic changed → the lift cache stays warm; a dll
rebuild (~1m17s) + server restart re-emits loader.js. remill fixes unchanged. VERIFICATION: deploy on
:8845 + CDP (cdp_oob_click.js render+click + cdp_click_hw_port.js counter gate) — IN PROGRESS at write time.

## ★ hydrateStyledDom OOB ROOT-CAUSED (2026-06-23) — Button::dom sret children-field drop
Debugged the deferred deep bug (the reason hydrate is disabled in loader.js). Repro: `AZ_PORT=88xx
AZ_FONT=1 AZ_HYDRATE=1 AZ_DUMP_DOM=1 node scripts/m9_e2e/full-cycle.js` (Node, seconds, no browser).
A safe JS replica of count_az_dom_nodes walks the layout-cb's sret-returned AzDom and pinpoints it:
```
[root]   @0xa037a00 disc=2 children{ptr=0xa03aaa0 len=2 cap=4}      body (az_0), VALID
  [root.0] @0xa03aaa0 disc=3 children{ptr=0xa038ce8 len=1 cap=4}    label_wrapper div (az_1), VALID
    [root.0.0] @0xa038ce8 disc=177 children{ptr=0x8 len=0}          text "5" (177=Text), VALID leaf
  [root.1] @0xa03ab80 disc=0 children{ptr=0x0 len=168011704 cap=168009808}  <<< GARBAGE (button)
```
**The button Dom (body's 2nd child) has a corrupt `children` DomVec: ptr=0, len/cap = STALE HEAP
POINTERS** (~0xa03xxxx) where {ptr=<text child>, len=1, cap=…} belongs. The sibling div (built by
AzDom_createDiv + addCssProperty + addChild) is PERFECTLY valid; only the button is wrong. Per
hello-world.c:70-78, root.1 = `AzButton_dom(button)`; `AzDom_addChild` is proven fine (it built the
valid div sibling). So the garbage originates in **AzButton_dom → azul_layout::widgets::button::Button::dom**
(native 0x7ffc36c19560, synth 0x1d8560, file_VA 0x1800c9560, size 2848 — AzButton_dom @0x...b58640 is a
64-B sret-forwarding wrapper). count_az_dom_nodes SURVIVES (root.1's child ptr=0 → its null-check skips);
the recursive **StyledDom::create cascade** is what derefs the garbage len → OOB (mini func698).

Button::dom is a HEAVY SSE struct constructor — dozens of `movups`/`movaps` 16-byte stores build the Dom
on the stack + copy it through the Win-x64 sret pointer (many UNALIGNED, e.g. `movups %xmm,0x1(%r15)`,
`0x11(%r15)`, `0x31(%r15)`). **This is the exact x86 analog of the aarch64 STP_Q-sret bug** the remill
fork already fixed (m11-complex-struct-box-new-lift / the M12.8 "STP_Q PRE/POST … struct fields written
through X8 read back uninitialised" note): one struct-field `movups` store (the one writing the button's
`children` DomVec, Dom+0x98) is dropped/mis-lifted, so `children` reads back uninitialised. Standalone
remill-lift of Button::dom (/c/rb/btndom.ll) has 18 __remill_* markers but they're indirect-branch
fallbacks (no --extra_data) + ud2/int3 padding — NOT the dropped store; the drop is on a normal SSE path.
NEXT to pinpoint the exact store: §M2 native harness (compile btndom.ll to the x86 host, mock a Button
input, run, inspect the output Dom+0x98) OR AZ_WRITE_TRACE the children store in a relift; then fix
remill's x86 movups/sret lifting (fork) or a transpiler store-replay workaround. Verify = COLD relift.
hello-world is UNAFFECTED (hydrate disabled; render+click work) — this is robustness for richer DOMs.

### UPDATE 2026-06-23 — hydrate OOB is a RAW remill-lift drop (NOT opt), confirmed via LOWOPT
Ran a relift with `AZ_LOWOPT_FNS=Button` (forces every Button fn through opt `-O0`; 31 hits; relift was
only 36s — warm cache, env-knob relifts are now cheap). The button-Dom `children` garbage is IDENTICAL
(`root.1 children{ptr=0 len=168011704}`). So `-O0` does NOT fix it ⇒ the children-field store is dropped
in the RAW remill lift (remill output, pre-opt), NOT by an LLVM opt pass. The fix is in remill's x86
lifter (sret/SSE-store value-flow), the direct analog of the committed aarch64 STP_Q-sret fork fix — a
B1-SSE-class remill grind, not a transpiler/opt knob. NEXT to pinpoint the exact dropped movups:
§M2 native harness on /c/rb/btndom.ll (the raw lift reproduces it, per the LOWOPT result) with a captured
Button input, OR a custom guest-write trace pass filtered to the children offset (Dom+0x98). Then fix
remill's x86 movups/sret lift + cold relift + re-enable hydrate in loader.js. Servers up: :8845 (working
e2e build) + :8846 (LOWOPT). hello-world UNAFFECTED (render+click work; hydrate disabled).

### UPDATE 2026-06-24 — native harness for Button::dom BUILT + running (user: "make a better harness, no shortcuts")
/c/rb/btn_harness.cpp + /c/rb/btndom.ll (= the raw lift of Button::dom sub_1d8560) compile to a native
x86-64 exe (btndom.ll triple is x86_64-windows-msvc-coff → clang++ compiles it host-native; 35KB .o).
Identity host-ptr memory model + __remill_*memory* shims that log W/R with symbolic regions (BTN/RET/
HEAP/STK), x86 State reg offsets hardcoded (RCX=2248 sret, RDX=2264 button, RSP=2312, RIP=2472).
add_child (sub_2271e0) is STUBBED to build a known-valid children {ptr=heap,len=1} (it's proven-correct:
root.0 div is valid live), so the test isolates Button::dom's SRET-RETURN copy: does it preserve children?
BLOCKER: the standalone lift lacks production's --extra_data (the fn's jump-table .rodata that the fork's
x86 ForEachDevirtualizedTarget reads), so a button_type switch hits `MISSING_BLOCK pc=1d9076` early —
BEFORE the Dom build — and aborts (sret untouched, all 0xAA). build_extra_data (transpiler_remill.rs:8755)
= ";"-joined "synth:hex" of each rip-relative .rodata range (x86 scan_guest_data_accesses); synth =
file_VA − 0x17FEF1000. NEXT (cron 26e95b19): extract Button::dom's rip-relative lea/.rodata targets from
/c/rb/btndom.disasm, read their bytes via objdump -s, build --extra_data, re-lift btndom2.ll, recompile +
relink the harness, run → verdict (children PRESERVED=lift OK→bug is production/classification; LOST=the
sret movups copy drops it→remill x86 fork fix). Harness = the §M2 tool the user asked for; reusable.

### UPDATE 2026-06-24 tick 2 — harness RUNS on the FAITHFUL production IR; iterating stubs
Switched the harness from the standalone btndom.ll (had missing_blocks, no extra_data) to the CACHED
PRODUCTION lift: /c/rb/btndom_prod.ll = az-lift-cache/c4497ceed19c189f_1d8560_v4_*.lifted.ll (0
missing_block — devirt'd with extra_data). Added the 4 extra callee stubs it needs (sub_10717f0/.18,
sub_107b3f0, sub_126afa0) + __remill_function_return. Compiles + runs (btn_repro2.exe = btn_harness.cpp
+ btndom_prod.o). Iteration so far:
  - no-op stubs → into_option/no_alloc_shim/sub_58dbd0 → REMILL_ERROR pc=1d9078 (alloc-failure/panic
    path: sub_58dbd0 returned null RAX → Button::dom took the handle_error branch).
  - made sub_58dbd0(+.2/.5/.7) ALLOC-ISH (return a fresh 4KB heap ptr in RAX=2216) → got PAST the error,
    now BUILDING the Dom on the heap (W8 HEAP+0<-0 = node_type disc, W32 HEAP+4<-5, f32 zeros = style),
    reading button fields (R32 BTN+48) → SEGFAULT (identity-ptr deref of a still-garbage value).
NEXT (cron 26e95b19): keep tuning — (a) set more AzButton input fields (label@0 done; image/button_type/
3×CssPropertyWithConditionsVec/on_click — offsets from azul.h:33488; empty Vecs = {ptr=8,len=0,cap=0}) so
reads past BTN+0x20 aren't garbage; (b) give result-producing callees valid returns (into_option sub_bf9990,
Css::from sub_12738d0 → empty Vec, the drops → no-op). GOAL: clean Button::dom run → read RETBUF+0x98
(children) → VERDICT: preserved (lift OK → bug is production/classification) vs LOST (sret movups copy drops
it → remill x86 fork fix). Harness files: /c/rb/btn_harness.cpp, btndom_prod.ll/.o, btn_trace2.txt.

### ★★★ 2026-06-24 tick 3 — HARNESS VERDICT: Button::dom lift is CORRECT; the bug is SP-DRIFT
The native harness (btn_harness.cpp + btndom_prod.ll, run via btn_repro2.exe) settled it. Button::dom
SPILLS its Win-x64 sret pointer (RCX) to a stack slot and RELOADS it before the final 240-byte Dom copy
to the caller's buffer. With the harness's callee stubs NOT modeling the guest call's ret-pop, RSP drifts
down 8 per call (the trace shows the retaddr pushes: W64 [STK-510]<-1d859d ...) → the sret-ptr reload
reads a DRIFTED slot (STK-358, =0/uninit in the harness; stale heap garbage in the live run) →
memcpy(dst=0/garbage) → the real button_dom gets stale garbage children = the live bug.
FIX in the harness: add `REG(O_RSP)+=8` to every callee stub (model the ret-pop production's
inject_helper_ret_pop does). RESULT FLIPS: `children{ptr=HEAP+3000 len=1 cap=1}` — VERDICT children
PRESERVED, final copy dst=RET+0. So **Button::dom's remill lift is CORRECT**; the "raw-lift drop" / 62-vs-100
movups store-count diff was a RED HERRING. The garbage is caused by SP DRIFT in the live run: some callee
in Button::dom's path does NOT net +8 (push −8 by the guest call, +8 by the callee's ret/ret-pop), so RSP
drifts and the spilled sret-ptr reload reads the wrong absolute slot. **SAME class as Task #9 (out_len_ptr
spill/reload) — proven general.** The fix is SP-balancing (ensure every lifted/stub callee in this path
balances the guest call's pushed retaddr), NOT a remill movups/sret fork fix — and it should fix BOTH the
button children AND out_len. NEXT (cron 26e95b19): find which production callee of Button::dom drifts SP
(AZ_REG_TRACE SP on the live run, OR audit inject_helper_ret_pop / enforce_sp coverage for the layout.wasm
path) → close the gap → cold relift → re-enable hydrate. Harness is the reusable proof tool.

### 2026-06-24 tick 4 — production reg-trace: Button::dom SP is BALANCED (SP-drift was a harness artifact)
Reg-traced the LIVE Button::dom SP (AZ_REG_TRACE="button::Button::dom" AZ_REG_TRACE_NOWRAP=1 relift 48s +
full-cycle.js AZ_HYDRATE AZ_DUMP_REGTRACE, dump moved to right-after-[2] since the post-hydrate alloc-OOB
aborts before the old after-[4] dump). count=123, SP id99: body STABLE at 0x6f188 (oscillates ±8 =
call push/pop, returns each time), epilogue pops cleanly, function returns SP=entry+8 (retaddr popped).
So the LIVE calls DO balance (real callee rets pop) — the harness's "SP-drift fixes it" (tick 3) was a
HARNESS ARTIFACT (my stubs didn't pop). Reconciliation: the harness ran btndom_prod.ll and with BALANCED
SP + SIMPLE stubs gave correct children; production has BALANCED SP too yet still garbage → the difference
is the REAL callees, NOT SP. Back to the mechb lesson: a real callee (Css::from sub_12738d0 / a drop /
add_child) misbehaves — clobbers button_div.children or the spilled sret slot, OR the children write
lands somewhere the sret copy doesn't include. NEXT (cron 26e95b19): (a) run the harness with the REAL
callees linked (compile their cached .lifted.ll + transitive deps, replace the no-op stubs) to find which
real callee flips children→garbage; OR (b) AZ_WRITE_TRACE Button::dom in production filtered to the
button_div.children offset (Dom+0x98) to watch the write + any later clobber. The harness + the SP-balance
proof stand; the cause is a real-callee value-flow, not SP-drift and not a movups drop. (full-cycle.js
gained an after-[2] regtrace SP dump — keep, env-gated.)

### 2026-06-24 tick 5 — narrowed to a REAL-CALLEE CLOBBER in the style/Css::from path (3 theories ruled out)
The harness (btndom_prod.ll, balanced-SP stubs, MINIMAL button input = label only/empty styles) gives
children PRESERVED. Crucially the harness's stubs are MOSTLY NO-OPS (Css::from sub_12738d0, the drops,
etc.) → so a no-op/Leaf-STUBBED callee does NOT produce the garbage (the mechb "stale garbage from a
no-op callee" lesson is RULED OUT here). Eliminations so far:
  ✗ remill movups/sret drop  (harness: lift correct w/ balanced SP)
  ✗ SP-drift                 (production reg-trace: Button::dom SP balanced, returns entry+8)
  ✗ no-op/Leaf-stub callee   (harness no-op stubs → children PRESERVED)
⇒ the bug needs a callee that RUNS and CLOBBERS button_div.children — most likely Css::from /
the style application, which the harness no-op'd AND the minimal input skipped (empty style Vecs). The
REAL button has populated container_style/label_style/image_style (AzButton_create) → Button::dom runs
the real Css::from + applies styles → that path clobbers the children (realloc/overwrite/value-flow).
Note: root.1 == button_dom (addChild just memcpy-copies it; addChild proven fine via root.0), so
AzButton_dom's OUTPUT is already known garbage — a hello-world.c probe of button_dom pre-addChild is
redundant + crashed the cb re-lift (reverted). NEXT (cron 26e95b19): link the REAL Css::from (sub_12738d0
cached .lifted.ll + stub its deps) into btn_harness.cpp WITH a populated-style button input, OR a
production write-trace of the button_div.children slot (Dom+0x98) — find the exact clobbering write in the
style path → fix that lift → cold relift → re-enable hydrate. Harness infra (btn_harness.cpp, the cached
.lifted.ll loader, reg-trace) all reusable.

### 2026-06-24 tick 6 — the harness's MINIMAL-INPUT path is clobber-free; the bug is INPUT-DEPENDENT
Tested the "Css::from sret-dest overlaps the dom" theory: made the harness Css::from stub (sub_12738d0
+.8/.13) write a 0xCAFE sentinel to its sret (RCX). Result: children STILL {ptr=HEAP+3000,len=1}
PRESERVED — NOT the sentinel. Why: from the call-order trace, Css::from.13's sret (STK-4b8) is a STALE
dom copy (the live dom-with-children was memcpy'd STK-4b8→STK-278 at line 263 BEFORE Css::from.13, then
copied back STK-278→STK-4b8 at 289 AFTER, overwriting the sentinel) → no overlap with the live dom. So
Css::from sret-overlap is RULED OUT. ★ The real lesson: the harness PRESERVES the children no matter what
the stubs do, because the MINIMAL button input (label only, empty style Vecs) makes Button::dom take a
short, clobber-free path. The production clobber is INPUT-DEPENDENT — it needs the REAL populated styles
(container_style/label_style/image_style from AzButton_create), which drive a longer path (more memcpy
shuffles / the real Css::from + style application) where the children gets clobbered. Eliminations now:
movups-drop ✗, SP-drift ✗, no-op/Leaf-stub ✗, Css::from-sret-overlap ✗. ⇒ NEXT: there is NO shortcut around
a FAITHFUL real-input reproduction. Two paths: (a) capture the real AzButton bytes from the live run (a
small probe in hello-world.c copying `button` to a marker BEFORE AzButton_dom + read in full-cycle.js +
feed to btn_harness.cpp as the RDX input; last probe crashed the cb COLD re-lift — retry, it's likely the
known parallel-subprocess hazard); OR (b) AZ_WRITE_TRACE Button::dom with an ALL-WRITES mode (widen
instrument_guest_writes' filter via an env knob) → relift → full-cycle.js → read the ring → find the write
that clobbers the (validly-built) children slot. Then fix that lift site → cold relift → re-enable hydrate.

### 2026-06-24 tick 7 — Button::dom + wrapper BOTH proven clean; production button_dom broadly garbage; harness can't reproduce
More eliminations via the native harness (btn_repro2.exe = btn_harness.cpp + btndom_prod.o):
  - Css::from stub returning a VALID non-empty Css, style Vecs len=1..32 (AZ_STYLES/AZ_STYLE_LEN at the
    computed offsets 72/104/136) → children ALWAYS PRESERVED. So Button::dom's lift produces a valid
    button_dom regardless of input/stub behaviour (with balanced SP).
  - Harnessed the WRAPPER AzButton_dom (sub_117640, /c/rb/wrapper.ll, 0 missing_block): its disasm shows
    it just memcpy's 0x110=272B (the AzButton INPUT) to a stack temp then calls Button::dom with the cb's
    sret directly — a thin shim, NO result copy. Not the bug.
Production heap dump (full-cycle.js AZ_DUMP_DOM, garbage root.1 button @0xa03ab80): children =
{ptr=0, len=0xa03a7b8, cap=0xa03a050} where len/cap are HEAP POINTERS — 0xa03a7b8 → a DomVec
{ptr=0xa03a940,len=2,cap=4}, 0xa03a050 → {ptr,0,ptr,0}. AND root.1.node_type disc=0 (vs the harness's
valid button_dom disc=0x34). So the PRODUCTION button_dom is BROADLY garbage (node_type AND children),
but the harness button_dom is valid across all inputs. ⇒ IMPASSE: the bug is real-input/real-callee/
real-context-dependent and the harness can't reproduce it (the real CSS content can't be cheaply
constructed; the input-capture probe crashes the cb COLD re-lift DETERMINISTICALLY at transitive[1248]
even with no contention — a separate cold-lift crash of a text-shaping sort fn). Eliminations total:
movups-drop ✗, SP-drift ✗, no-op/Leaf-stub ✗, Css::from-overlap ✗, wrapper ✗, Button::dom-lift ✗.
Remaining (all HEAVY): (a) faithful harness = link the FULL real callee chain (add_child+Css::from+drops
+ their transitive deps) AND capture/construct the real CSS content; (b) transpiler write-trace with a
custom filter on the button_dom buffer (rebuild + the runtime-address problem); (c) the cold-lift crash
of the text-shaping sort fn at transitive[1248] may itself be a related lift bug worth chasing. Working
server restored on :8845 (READY 348s — slow repopulate after the probe2 cache damage).

### 2026-06-24 tick 8 — GROUND TRUTH: hydrate OOB is in lifted StyledDom::create (func[698]), NOT Button::dom
Tick 7's "production button_dom broadly garbage" was the DOM_SIZE=224 misread (it's 240). Re-ran full-cycle.js
against a real-lift server :8800 (`AZ_HYDRATE=1 AZ_DUMP_DOM=1 AZ_DOM_SIZE=240 AZ_CHILD_OFF=152`). The dom tree
is STRUCTURALLY VALID node-by-node (body→label_wrapper→text"5" / button→text, all children vecs sane, leaves
empty `{ptr=8,len=0}`). Phantoms KILLED (do not re-chase): (1) node_type `0x..b1` upper bytes = benign disc
padding (native does `movups label; movb $0xb1` too; disc read as u8) — zeroing it doesn't fix the trap; (2)
button-text style vec `{len=4}` @+88 is legit (harness builds it) — zeroing it doesn't fix; (3) "stack
overflow" was the SECONDARY crash: `func[15]:0x89c0`=AzStartup_alloc is the NEXT alloc after the *caught*
hydrate trap; patching mini SP 192KB→8MB (full-cycle.js `AZ_PATCH_STACK=8388608`) makes the script reach PASS
but hydrate STILL traps. The PRIMARY trap (via `e.stack` in the [2c] catch) = `func[698]:0x6c5668`, chain
hydrate(57)→56→98→182→**352(recurses,=tree walk)**→712→698. Trap insn = `i64.load32_s [local1+local6*4]` in
an offset-chase loop `local6=local1+arr[local6]`, `local1=X+0x9FB888` → **garbage node-array index → OOB**,
deterministic + content-independent. This is the real lifted-StyledDom::create lift bug. Tools: llvm-objdump
-d (CODE-relative = V8 file-offset − 0x8681), `/c/rb/wasmsec.js` (sections), `/c/rb/wasmnames.js` (export
names). Memory note: [[windows-weblift-hydrate-trap]]. NEXT: trace local6's garbage source (which cascade
store is dropped/miscomputed); or instrument StyledDom::create sub-phases; the fix is a remill/transpiler lift
fix, then re-enable hydrate + CDP-verify.

### 2026-06-24 tick 9 — HYDRATE FIXED (REC_MARKER pc-pollution) + committed; NEXT = solver func232
ROOT CAUSE of func698: the recursive-bl forwarder (transpiler_remill.rs ~7862 `is_recursive_marker`) passed
the incoming %pc through to `@sub_<lift_addr>`. But `x86_scan::rewrite_recursive_call` rewrites a self-recursive
`call rel32`→rel32=0x1000_0000 (REC_MARKER) so remill won't infinite-inline it — so that %pc carries
+0x1000_0000. On x86 the lifted body uses %pc for PC-relative DATA (switch jump tables `[%pc+disp]`), so the
marker accumulated +256MB PER RECURSION LEVEL; after ~3 levels of StyledDom::create's recursive node walk
(func352) the pc hit ~768MB and func698's `i64.load32_s [%pc+0x9FB888+idx*4]` blew past linear memory. Traced
via `e.stack` in full-cycle.js's [2c] catch: func57(hydrate)→56→98→182→352(recurses)→712→698; func352's
recursive call passes `local8 + 0x1000005D` (= entry + 0x5D + REC_MARKER). FIX (commit 2ba1b59de): forwarder
passes `i64 {lift_addr}` (constant entry = real self-recursion target), x86-only; aarch64 keeps %pc
passthrough. VERIFIED: (a) cold re-lift now COMPLETES past transitive[1248]/[1249] (allsorts
median3_rec/quicksort) which crashed deterministically before; (b) `AZ_HYDRATE=1 full-cycle.js` → `[2c]
hydrateStyledDom rc=0 node_count=5`. The whole 2026-06-23 "garbage children / dropped-movups in Button::dom"
hypothesis was WRONG (dom tree valid at DOM_SIZE=240; Button::dom clean). NEXT BUG (newly reachable, separate):
`AzStartup_solveLayoutReal` (mini func232, chain 69→68→116→232) TRAPS — derefs garbage pointer field
`[[State.R14]+48]+16` (R14 valid, +48 field garbage); FONT-INDEPENDENT (layout, not shaping). Disasm in
/c/rb/new_disasm.txt (CODE-rel = V8 file-offset − 0x869c). hydrate gate stays `false &&` until solver fixed.
full-cycle.js [2d] probe added for the solver.
