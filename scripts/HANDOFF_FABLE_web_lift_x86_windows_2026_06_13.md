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
