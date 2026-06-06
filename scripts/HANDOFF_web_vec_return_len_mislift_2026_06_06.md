# Handoff — ROOT-CAUSE hunt: systemic "Vec-return `len` mis-lift" on the azul web (lifted) backend

**Date:** 2026-06-06 · **Branch:** `mobile-ios-android` · **Repo:** `/Users/fschutt/Development/azul-mobile` (+ remill fork at `/Users/fschutt/Development/azul/third_party/remill`)

---

## TL;DR — what the next agent must do

There is **one recurring bug** that is now the dominant blocker for the web (lifted) backend:

> **When a lifted function returns a value that CONTAINS a `Vec` (i.e. `Vec<T>`, `Result<Vec<T>, E>`, `(Vec<T>, X)`, …) by value, the returned `Vec`'s `len` word is mis-lifted** — the caller reads `len` as a **pointer-shaped garbage value** (e.g. `171120`, `0x628c23c`) instead of the real length the callee computed.

I have worked around it **6 times** with per-function `&mut Vec` out-params (see "Workarounds" below), but the layout path keeps hitting fresh instances. **Find and fix the ROOT CAUSE** — either:
- **(A) in remill** (`third_party/remill`, our fork) — how it lifts an AArch64 `sret` (x8 hidden-pointer) return of a multi-word struct, OR
- **(B) in the translation layer** (`dll/src/web/transpiler_remill.rs`) — how the lifted LLVM IR / ABI / data-segment handling treats that store/load.

A single correct fix should kill the whole class **and** let us revert all 6 out-param workarounds. The symptom (len reads as a *pointer-shaped* value, not a corrupted scalar) strongly suggests a **field-offset / struct-layout shift** in the sret store or the caller's load — i.e. the `len` slot ends up holding the adjacent `ptr` (or `cap`) of the struct.

---

## Architecture orientation (read this first)

The web backend does **binary translation**, not a normal wasm build:
1. `azul-dll` is compiled to a **native aarch64 dylib** (`target/aarch64-apple-darwin/release/libazul.dylib`).
2. At server startup, the web backend **lifts** `AzStartup_*` entry points + all transitively-called functions from **native ARM64 machine code → LLVM IR → WASM**, using **remill** (`remill-lift-17`) for the ARM64→LLVM-IR step, then LLVM/LLD to WASM.
3. The lifted WASM (`azul-mini.wasm`) runs the real layout/shaping code in the browser/node.
4. The transpiler/orchestrator is **`dll/src/web/transpiler_remill.rs`** (lifting, data-segment mirroring, ABI shims, helper IR). Symbol classification is in `dll/src/web/symbol_table.rs`.

So a "mis-lift" = remill produced LLVM IR that doesn't faithfully implement the original aarch64 semantics for some construct. We've already fixed many (atomics, NEON, memcpy, hashbrown empty-group, discriminants…). This handoff is about the **`sret` multi-word return** class.

**The current goal of the whole effort:** `examples/c/hello-world.c` (counter text + button + on_click) running on the web lift. **As of this session, text SHAPES** ("Hello" → 5 glyphs); the remaining blockers are all instances of this `Vec`-return mis-lift in the layout-positioning path.

---

## The bug in detail

### Symptom
A function computes a `Vec` with the correct length internally, but its caller sees a huge, pointer-shaped `len`:

- **Decisive repro this session** (`shape_text`, glyph buffer for "Hello"):
  - **Inside** `shape_text_internal` (where the `Vec<Glyph>` is built), a marker at the `Ok` return read **`glyphs.len() == 5`** (correct).
  - **In the caller** (`shape_text_correctly`, after `font.shape_text(...)?`), the same `Vec` read **`glyphs.len() == 171120`** (garbage).
  - Consequence: `for glyph in glyphs` iterated 171120 garbage elements → downstream `SmallVec::extend` overflow → panic / 789 MB alloc → OOB trap.
- Earlier instances read garbage lens like `0x628c23c` — again **heap-pointer-shaped**, not a small corrupted integer. This is the key clue: the `len` slot seems to receive a **pointer** value (the `ptr` or `cap` of the same/adjacent field), i.e. a **field-offset shift** in the sret struct store or the caller's struct load.

### Where it bites (the 6 confirmed instances, all same class)
All are functions returning a `Vec`-containing value by `sret`:
1. `collect_inline_content_recursive` / `collect_inline_content` (sizing) — `Result<Vec<InlineContent>>`.
2. `create_logical_items` — returned `Vec<LogicalItem>`.
3. `measure_intrinsic_widths` — `Result<IntrinsicTextSizes, _>` (this one was the *Result discriminant* via a niche, fixed by `#[repr(C,u8)]` on `LayoutError`; related but the niche-disc sub-case).
4. `ParsedFontTrait::shape_text` → `shape_text_internal` — `Result<Vec<Glyph>>`. **(best minimal repro — see above)**
5. `TextShapingCache::layout_flow` content path — fat-slice `&[InlineContent]` arg + `content.to_vec()`.
6. `collect_and_measure_inline_content` (fc.rs:6340/6354) — `Result<(Vec<InlineContent>, HashMap<…>)>`. **(the current live blocker, NOT yet worked around)**

NOTE there is a **closely-related, already-understood sub-case**: passing a `&[T]` **fat slice ARG** also mis-lifts its `len` half (fixed by taking `&Vec<T>` so `len` is read from the Vec header inside the callee — see `g112`). The RETURN case is the one to root-cause now; the ARG case may share the root.

---

## How to reproduce & diagnose

### Fastest minimal repro (no full relift): `remill-lift-17 --bytes`
The remill lifter can lift a single instruction or a byte blob to IR. To study the sret return, lift a small function's bytes and inspect the IR for the `len` store/load. Example (this is how the FMUL fix was verified):
```
cd /Users/fschutt/Development/azul
RL=third_party/remill-install/build/remill/bin/lift/remill-lift-17
"$RL" --arch aarch64 --address 0x1000 --bytes <LE-hex> --ir_out /tmp/x.ll
grep -nE '@__remill_error|store|load' /tmp/x.ll
```
**Better minimal repro idea:** disassemble (`otool -tV`) the *caller side* of `shape_text_correctly` around the `font.shape_text(...)?` return-value load, and the *callee side* `shape_text_internal` `Ok` return store, then lift those exact byte ranges and compare the IR's `len`-slot offset vs the real struct layout. The dylib is `target/aarch64-apple-darwin/release/libazul.dylib`; map a synth PC→fn→native vmaddr as described in the other handoff (`HANDOFF_web_rwlock_glyphdecode_2026_06_03.md`, "HOW the FMUL instr was ID'd").

### Full repro (the running harness)
- One-text-node test binary: `examples/c/web-text-min.bin` (built from `examples/c/web-text-min.c` = `body{800x600}` + `Text("Hello")`).
- Build + relift + run (a ready cycle script exists at `/tmp/cycle_g128.sh`; the canonical commands are below).
- Harness: `AZ_LENIENT=1 node scripts/m9_e2e/layout-flexbox.js` → reads wasm markers post-trap.
- The harness already prints the decisive markers: `g125`/`g126` (shape_text_internal's own len vs the caller's len) — that's the proof the value is correct in the callee and garbage in the caller. Search `scripts/m9_e2e/layout-flexbox.js` for `g125`/`g126`/`g127 measure-path` to see/extend them.

### Marker technique (used throughout)
Wasm-only scratch region. Lifted code does `unsafe { core::ptr::write_volatile(0x60700 as *mut u32, val) }` at chosen points; the JS harness reads them via `mini.AzStartup_peekU32(addr)`. **Do NOT add raw-address markers in code paths the NATIVE server executes at startup — it SEGVs the server.** The measure/shape path is lifted-only at startup, so markers there are safe (proven). Markers are read in two places in the harness: the POST-TRAP block (on a trap) and the success-path block (near the `compact_cache` print) — add to whichever fires.

---

## Build / run / relift commands (canonical)

**Full rebuild (azul-dll) + relift + harness** (≈1-2 min build incremental, ≈5 min relift):
```
cd /Users/fschutt/Development/azul-mobile
DYLDIR=target/aarch64-apple-darwin/release
RUSTC_BOOTSTRAP=1 RUSTFLAGS="-C target-feature=-lse,-rcpc,-rcpc2,-rcpc3 -C llvm-args=-aarch64-enable-ldst-opt=0 -C llvm-args=-enable-machine-outliner=never" \
  cargo build -p azul-dll --release --features "build-dll web web-transpiler web-transpiler-static" --no-default-features \
  -Z build-std=std,panic_abort -Z build-std-features=panic_immediate_abort --target aarch64-apple-darwin
cp $DYLDIR/libazul.dylib $DYLDIR/deps/libazul.dylib
clang examples/c/web-text-min.c -L$DYLDIR -lazul -Iexamples/c -fno-stack-protector -o examples/c/web-text-min.bin
ps -axo pid,command | grep -E 'web-text-min.bin|remill-lift' | grep -v grep | awk '{print $1}' | xargs kill -9 2>/dev/null
lsof -ti tcp:8800 | xargs kill -9 2>/dev/null
DYLD_LIBRARY_PATH=$DYLDIR REMILL_LIFT_BIN=/Users/fschutt/Development/azul/third_party/remill-install/build/remill/bin/lift/remill-lift-17 \
  AZ_BACKEND=web://127.0.0.1:8800 AZ_NO_LIFT_CACHE=1 nohup examples/c/web-text-min.bin > /tmp/server.log 2>&1 &
# wait for "Listening on http://127.0.0.1:8800" (≈5 min, watch transitive[N] climb to ~1500), then:
AZ_LENIENT=1 timeout 120 node scripts/m9_e2e/layout-flexbox.js
```
`cargo check -p azul-layout --features web_lift` is the fast syntax/type gate.

**Relift ONLY (no azul rebuild — for remill-fork or marker-only changes):** just re-run the `nohup … web-text-min.bin …` line with `AZ_NO_LIFT_CACHE=1` (it re-lifts using whatever `remill-lift-17`/`aarch64.bc` are installed).

**Rebuild remill after editing the fork:**
```
cd /Users/fschutt/Development/azul/third_party/remill-install/build/remill
ninja remill-lift-17 lib/Arch/AArch64/Runtime/aarch64.bc
cp lib/Arch/AArch64/Runtime/aarch64.bc ../../install/share/remill/17/semantics/aarch64.bc   # decoder→remill-lift-17, semantics→aarch64.bc
```

**Diagnostic env (read at LIFT time, no rebuild):** `AZ_FUEL=ALL` (turns infinite loops into named traps), `AZ_REMILL_KEEP_SCRATCH=1` (keeps per-fn `__az_dep_<native>.opt.ll` IR). `--keep-section=name` is already in the link so wasm trap stacks are NAMED (`sub_<synthaddr>`); decode via the server-log `sub_<X> → resolved=<NAME>@<native>` lines. `AZ_WASM_DEBUG=1` CRASHES this build — don't use it.

---

## Where to look for the ROOT CAUSE

### Hypothesis A — remill sret lifting (most likely)
- AArch64 returns a >16-byte struct via the **x8 indirect-result pointer**; the callee stores the struct fields to `[x8 + offset]`. A `Vec` field is `{ptr@0, cap@8, len@16}` (Rust `Vec` = `RawVec{ptr,cap}` + `len`). The garbage-len-as-pointer symptom ⇒ the `len` store (or the caller's `len` load) uses the **wrong offset**, picking up `ptr`/`cap`.
- remill fork lives at `third_party/remill/lib/Arch/AArch64/`. Relevant: `Arch.cpp`/`Decode.cpp` (decoders), `Semantics/DATAXFER.cpp` (loads/stores incl. `STP`/`LDP` pair ops — a Vec header is often stored/loaded as `stp/ldp` pairs!), `Extract.cpp`. **Strongly suspect `STP`/`LDP` (store/load pair) with signed/scaled offset around the struct fields, or post-/pre-index forms.** Lift a known caller/callee byte range and diff the IR offsets against the real layout.
- Precedent: prior remill fixes already touched `STP_Q`, `STR_D/S post-index`, byte-atomics, etc. (see commit log below). The team has a working method for adding/fixing AArch64 semantics.

### Hypothesis B — translation layer (`dll/src/web/transpiler_remill.rs`)
- The transpiler post-processes remill IR, sets up the wasm ABI/stack, mirrors native const data pages to synth wasm offsets, and emits helper IR (memcpy/memmove, bump allocator, etc.). If a multi-word return is moved via a helper memcpy or an ABI shim that gets the size/offset wrong, the `len` word could land wrong.
- Look at: how return values / sret pointers are threaded; the `LibcMemcpy`/`emit_helper_ir` `@llvm.memmove` body (a 352-byte struct copy via memcpy was a real M12 bug — `FnClass::LibcMemcpy`); the bump-allocator helper; per-wasm stack relocation. Also `symbol_table.rs` classification (`Leaf`/`Recursable`/`BumpAlloc`/`LibcMemcpy`).
- A quick triage to distinguish A vs B: if the **lifted IR itself** (from `--bytes` / KEEP_SCRATCH `.opt.ll`) already has the wrong `len` offset → it's remill (A). If the IR is correct but the value is wrong at runtime → it's the transpiler/ABI/data-mirror (B).

---

## What WORKS now (this session's wins — all KEEP)

1. **remill `FMUL (vector, by element)`** (`FMUL_ASIMDELEM_R_SD`) — was a stub; implemented decoder (`Arch.cpp`) + semantic (`Semantics/SIMD.cpp` `MAKE_FP_VEC_ELT` → `FMUL_ELT_2S/4S/2D` + DEF_ISELs) + removed the `Decode.cpp` stub. IR-verified + end-to-end (`__remill_error` 4→0 in shaping). **This is the template for adding any missing NEON instr.** ⚠ `RegNum` is a scoped `enum class : uint8_t` under `REMILL_AARCH_STRICT_REGNUM` → cast through `uint8_t` for arithmetic.
2. **`shape_text` out-param chain** — the `Vec<Glyph>` return mis-lift, worked around end-to-end → **"Hello" shapes to 5 glyphs**.
3. Plus the prior session wins: `#[repr(C,u8)]` on text3 enums (InlineContent/LogicalItem/ShapedItem/FontStack/both LayoutErrors) for the niche-discriminant mis-lift; hashbrown empty-cache bypasses; EMPTY_GROUP static mirror; etc. (see `memory/web_flexbox_lift_2026_06_01.md`).

### The 6 out-param workarounds to REVERT once the root cause is fixed
`collect_inline_content*`, `create_logical_items`, `shape_text`-chain (trait + `ParsedFont`/`FontRef`/`FontOrRef` impls + `shape_text_internal`/`shape_text_for_parsed_font`/`shape_text_for_font_ref` + 3 callers), `layout_flow` content `&Vec`, and (pending) `collect_and_measure_inline_content`. All are tagged with `[g1xx az-web-lift]` / `[g127]` / `[g128]` comments in: `layout/src/text3/cache.rs`, `layout/src/text3/default.rs`, `layout/src/font.rs`, `layout/src/font_traits.rs`, `layout/src/solver3/fc.rs`, `layout/src/solver3/sizing.rs`, `layout/src/window.rs`.

---

## Recent commits / changes — explanation

**This session's azul-mobile work is UNCOMMITTED** (intentional — commit when asked). Changed files: `layout/src/{font.rs, font_traits.rs, text3/cache.rs, text3/default.rs, solver3/fc.rs, solver3/sizing.rs, window.rs, …}` — the out-param workarounds + `#[repr(C,u8)]` + cache bypasses + diagnostic markers described above.

**remill fork (`third_party/remill`)** — recent COMMITTED work (the precedent for how AArch64 lift gaps are fixed; all from this multi-session effort):
- `656c23c` AArch64 (M12.7): exact jump-table devirt via `--extra_data` + LDRH opcode fix
- `1ff7fa2` AArch64 (M12.7): scalar DUP-element + STR_Q post-index + PC-relative jump-table devirt
- `939be20` AArch64: BIC (vector, immediate)
- `8e55ef4` AArch64: scalar FCCMP/FCCMPE + scalar single-element SCVTF
- `2815864` AArch64: XTN/XTN2, UZP1/UZP2, LD1 single-lane .S
…and **UNCOMMITTED** in the fork right now: `Arch.cpp`, `Decode.cpp`, `Semantics/SIMD.cpp` (= this session's **FMUL-by-element** fix), plus `DATAXFER.cpp`, `MISC.cpp`, `Lift.cpp` (prior byte-atomic / lift work). The submodule gitlink in azul is local-only; the runtime uses the prebuilt `remill-lift-17` + `aarch64.bc`, so committing the fork is for provenance, not function.

**azul-mobile committed history** (the visible `git log`, e.g. `9ad407e9a`, `7c5664367` azul-paint, codegen/cpp fixes) is **unrelated** to the web-lift effort — that's separate desktop/binding work. The web-lift work has never been committed.

---

## Pointers to the running notes
- `scripts/HANDOFF_web_rwlock_glyphdecode_2026_06_03.md` — the chronological running handoff (g96…g128); top section has the current blocker + the FMUL recipe + the synth-PC→instruction method.
- `memory/web_flexbox_lift_2026_06_01.md` — newest-first session log with every step's evidence (g124 FMUL, g127 TEXT SHAPES, g125/g126 the decisive callee-vs-caller len proof, g128).

---
## 2026-06-06 (g129) — ROOT-CAUSE SESSION PROGRESS (autonomous)

**Triage DONE: it's hypothesis B (transpiler/runtime), NOT remill instruction lift.**
- All standard sret store/load forms (STP/LDP scaled-offset, STP_Q/LDP_Q, STR_Q/LDR_Q, post/pre-index, STUR/LDUR) lift CORRECTLY in isolation (`remill-lift-17 --bytes`, 0 __remill_error). The `caller`/`make_result` minimal repro (Vec<u64> sret, exact RUSTFLAGS) lifts faithful SP-relative addressing.
- SP IS preserved across calls (enforce_sp_preservation, default-on, wraps X19-X28/X29/SP/D8-D15 around every sub_ call). x8 is NOT preserved (ABI-correct: x8=caller-scratch sret reg).
- Active compile path = SUBPROCESS LLVM-21 opt/llvm-link/llc/wasm-ld (use_native_remill=false; needs AZ_NATIVE_REMILL). `.opt.ll` is the LLVM-21 opt output; enforce_sp post-processes it (transpiler_remill.rs:1127).

**Live clean instance = collect_and_measure_inline_content_impl (Result<(Vec<InlineContent>,HashMap),LayoutError> sret, fc.rs).** Trap chain: finalize_hydrate→layout_dom_recursive→layout_document→calculate_layout_for_subtree→layout_formatting_context→layout_ifc→layout_flow→InlineContent::clone (1.67GB alloc from garbage len).

**Markers (g129a-d) DECISIVE:** `_impl` ENTERS (ifc_root_index=0) but reaches NONE of its 4 Ok returns (6419/6639/6810/7196 all silent) → `_impl` returns via an **Err** path. Yet the caller (layout_ifc, wrapper inlined) takes the **Ok** branch (`?`) and reads STALE STACK DATA: inline_content = {ptr=0x2aac0, len=172400} (both mini-layout stack addrs ~0x2A000; &inline_content slot=0x29b90). So **the caller's niche-`?` is fooled by stale non-null slot data → the callee did NOT write the caller's sret slot with the real result (slot mismatch / Err-niche-write lost).**

**RULED OUT:** `initializes((672,680))` attr (=X8 range) on every sret fn → tested `--enable-dse-initializes-attr-improvement=false`: NO change to layout_ifc's W8(672) store count in faithful linked-.opt.ll DSE repro. Not the DSE mechanism. (NOTE the earlier "X8 stores eliminated" read was a GREP ERROR — opt renames X8→%W8; the stores ARE present.)

**ROOT framing:** the sret of a >16-byte struct return (Vec-containing) mis-transfers the discriminant AND/OR the Vec fields; the caller reads a stale slot. out-param fix works by shrinking the OTHER returns to ≤16B (register x0/x1, no sret) — but collect_and_measure's proposed out-param return (Result<HashMap>) is STILL >16B sret (untested).

**IN FLIGHT:** AZ_OPT_LEVEL=1 relift (global -O1; -O0 is infeasible — "local count too large" wasm-invalid). If -O1 makes text lay out → it's an O2/Oz-specific opt miscompile of the lifted x8/sret ABI; narrow the pass next. If not → LTO (--lto-O3, gated by AZ_WASM_DEBUG which crashes) or deeper. Diagnostic markers g129 in fc.rs (0x608B0-0x60908) + harness layout-flexbox.js — REVERT at cleanup.

---
## 2026-06-06 (g129) — ⭐ ROOT CAUSE FOUND + FIXED ⭐

**ROOT CAUSE: `enforce_sp_preservation` (transpiler_remill.rs) did NOT wrap `@__az_indirect_dispatch` calls.**

The chain, proven step by step:
1. An aggregate-returning fn (Vec / Result<Vec> / (Vec,X) — i.e. >16-byte sret) saves its incoming **x8 (the indirect-result/sret pointer) into a CALLEE-SAVED register (x19-x28)** at its prologue (confirmed by disasm of a minimal `fn()->Vec<u64>` repro: `mov x19, x8` … `str q0,[x19]` (ptr,cap) … `str x8,[x19,#16]` (len)).
2. Between the prologue and the sret store, the fn makes calls. Direct calls (`@sub_`) and remill indirect (`@__remill_function_call`) ARE wrapped by `enforce_sp_preservation` (save/restore x19-x28/x29/sp/d8-d15 around the call), so x19 survives them.
3. BUT **indirect calls** (trait methods like `ParsedFontTrait::shape_text`, closures, iterators, the allsorts GSUB/GPOS shaper) are dispatched through the **M12.7 `@__az_indirect_dispatch` PC-switch**, which `parse_sub_call` did NOT match → those calls were NOT wrapped. The dispatched target (a lifted fn whose epilogue drops the callee-saved restore — the whole reason enforce_sp exists) **CLOBBERS x19**, and it's never restored.
4. So at the sret store, x19 = garbage → `str …,[x19+…]` writes the Vec to a garbage address. The caller reads its REAL sret slot, which still holds **stale stack data** → the returned Vec's {ptr,cap,len} are pointer-shaped garbage (len ≈ 0x2A000, a mini-stack address — consistent across shape_text(0x29C30) and collect_and_measure(0x2A150)).
5. For Result<Vec>: the caller's niche-`?` reads the stale (non-null) ptr at offset 0 → mis-takes Err as Ok / reads a garbage Vec. (collect_and_measure `_impl` ENTERS but reaches NO Ok return → it returns Err because its OWN inner Vec-returning call `split_text_for_whitespace` was x19-clobbered → garbage → wrong path; the outer caller then reads the stale slot as Ok.)

**Why it's THE class:** out-param works because the result pointer is passed in **x0** (a normal arg reg read once at entry) — it does NOT need to survive across calls in a callee-saved reg, so the unwrapped-dispatch clobber is harmless.

**Decisive eliminations:** remill sret-write lift is CORRECT (`--bytes` + minimal repro IR verified — loads x19 fresh, writes ptr@x19/cap@x19+8/len@x19+16). NOT opt: bug PERSISTS at per-fn -O1 AND at minimal opt (per-fn O1 + `--lto-O0` via new AZ_LTO_LEVEL knob) → it is opt-LEVEL-independent, ruling out dse/gvn/mldst. `initializes((672,680))` attr was a red herring (the DSE-improvement flag had no effect). **Empirical proof:** a `@__az_indirect_dispatch` call in a real `.opt.ll` (KEEP_SCRATCH) is `tail call ptr @__az_indirect_dispatch(...)` with NO surrounding azv save/restore block.

**THE FIX (transpiler_remill.rs, ~5359 `parse_sub_call` + ~5321 emit):**
- `parse_sub_call` now also matches `__az_indirect_dispatch` (takes `ptr %state` arg0, same gep/save/restore machinery).
- the `tail` strip is now generic (`tail call ptr @` → `call ptr @`) since save/restore stores follow ALL wrapped calls.
ABI-correct (more callee-saved preservation can only fix leaks, never mask a real bug). Also added AZ_LTO_LEVEL + AZ_WASM_LD_MLLVM diagnostic env knobs (harmless, default off) during the hunt.

**STATUS: fix rebuilding/relifting (g129-fix). Expect _impl→Ok, caller reads correct inline_content, text LAYS OUT (h>0). THEN revert the 6 out-param workarounds + the g129 diagnostic markers.**

---
## 2026-06-06 (g129) — CORRECTIONS + remaining contradiction (enforce_sp __az_indirect_dispatch fix KEPT but did NOT fix collect_and_measure)

**enforce_sp + __az_indirect_dispatch fix is REAL + KEPT** (transpiler_remill.rs parse_sub_call now matches `__az_indirect_dispatch`; tail-strip generic). It IS a genuine leak fix (indirect calls were NOT preserving callee-saved regs — empirically `tail call ptr @__az_indirect_dispatch` had no azv wrap). BUT it did NOT fix the collect_and_measure trap (identical garbage after relift). So it's correct-but-not-the-blocker. Also added AZ_LTO_LEVEL + AZ_WASM_LD_MLLVM diagnostic env knobs (default off, harmless).

**ABI CORRECTION (important — invalidates the "x8 clobber" theory):** collect_and_measure_inline_content_impl is **`extern "Rust"`** → the sret (indirect-result) pointer is passed in **x0, NOT x8**. Disasm proof: h19f329 prologue `mov x21, x0`; the return writes the Result to `[x21]` (`str xzr,[x21]` = the Err niche Vec.ptr=0); hd6d7 prologue `mov x22, x0`, writes to `[x22]`. So x8 is just a SCRATCH register here (freely reused). **The diagnostic markers (`write_volatile`) that use x8 as scratch are therefore HARMLESS to the sret** — removing the entry marker gave byte-identical garbage. Markers DEFINITIVELY ruled out.

**The remaining contradiction (unsolved):** the sret ptr lives in x21/x22 (callee-saved, both in enforce_sp's CS_OFFSETS 880/896 → preserved across every wrapped call). The caller (layout_ifc, calls hd6d7) sets `add x0, sp, #0x620` (the sret slot) right before `bl`, and enforce_sp preserves SP across the call. So `[x22] == [x0] == [caller_SP+0x620] == caller's read slot` — the write SHOULD hit the slot the caller reads. YET: markers show _impl reaches NO Ok return (returns Err, writing Vec.ptr=0 niche) while the caller's `?` takes the **Ok** branch reading a non-null pointer-shaped value (slot has STALE stack data, e.g. {ptr=0x2aac0, len=172400} at &inline_content=0x29b90). So the callee's write is NOT landing in the caller's slot — a slot mismatch with no identified mechanism.

**RULED OUT (with evidence):** remill instruction lift (minimal `fn()->Vec<u64>` repro + `--bytes`: sret store/load offsets correct); ALL opt levels (bug persists at per-fn -O1 AND minimal opt = per-fn O1 + `--lto-O0`); `initializes((672,680))` DSE (flag had no effect on store count); x19/x8-clobber via unwrapped `__az_indirect_dispatch` (wrapped it — no change); diagnostic markers (removed entry marker — no change); unlifted NEON inside _impl (0 `__remill_error`, 0 `sync_hyper_call` in _impl's opt.ll). Confirmed-unlifted elsewhere: `TryDecodeFNEG_ASIMDMISC_R` (fneg.2s) is a stub (part of err=21) — FNEG_S/D scalar templates at BINARY.cpp:473 if needed; FMUL-elem decoder Arch.cpp:4522 is the recipe.

**NEXT CONCRETE STEPS (for continuation):**
1. The slot-mismatch is the crux. Capture the ACTUAL sret addresses: add a marker in layout_ifc capturing `&result` (the Result slot it passes as x0, NOT &inline_content which is post-`?`), and in _impl capture the address it writes to (e.g. via the Ok/Err return value's `&`). Compare — they should be equal; if not, find why x0/x22 diverges.
2. Inspect hd6d7's (the ACTUALLY-CALLED monomorph, @0x2a0ed8) opt.ll for whether x22 is clobbered by a call enforce_sp doesn't wrap, OR whether `mov x22, x0` is correctly lifted (State.X22 = State.X0).
3. Reconsider whether _impl returns Err for a LEGITIMATE reason (corrupted `tree`/`styled_dom` from an UPSTREAM sret mis-lift in layout_document/calculate_layout_for_subtree) vs the niche being mis-read. The Text AzString reads as BoxOrStatic {ptr=0, len=box_ptr} pre-cascade — verify _impl derefs the box correctly (shaping does).
4. ★ REVERT-at-cleanup: g129 markers in fc.rs (0x608xx/0x609xx) + harness layout-flexbox.js g129 block + the entry marker (already removed). The 6 out-param workarounds are STILL the working mitigation until root cause is cracked.

---
## 2026-06-06 (g129b2) — ⚠️ THE BUG IS A HEISENBUG (markers change codegen) + SLOT-MISMATCH confirmed + a 2nd masked blocker (InvalidTree)

**SLOT-MISMATCH confirmed (deduced + then observed):** the caller reads the Result from a DIFFERENT location than the callee (_impl) writes it. Proof: `inline_content.ptr = 0x2aac0` (a mini-stack address) = the Result slot's word0 (Vec.ptr niche) — it is STALE garbage, NOT the Err niche (0) that _impl wrote. So _impl's `str ...,[x22]` (x22 = its saved sret ptr = x0 = caller's `add x0, sp, #0x620`) lands somewhere the caller does not read. With SP preserved by enforce_sp and x22 callee-saved, this should match — yet it doesn't. ABI note: this fn is `extern "Rust"` → sret in **x0 → x21 (h19f329) / x22 (hd6d7)**, NOT x8.

**★ THE KEY FINDING — IT'S A HEISENBUG.** Adding a marker that does `let p = &result_raw …; write_volatile(.., *p)` (i.e. takes `&result_raw`, forcing the Result to be materialized as a stable stack value instead of NRVO'd straight into the sret) **ELIMINATES THE TRAP** (`rc=0`, no 1.67GB alloc). This is essentially a partial out-param. So the sret slot-mismatch is **codegen-dependent**: the unstable (NRVO direct-to-sret) pattern mis-lifts; forcing a stable local fixes it. This is why marker-based diagnosis has been so unreliable across sessions — the diagnostic markers themselves perturb the very codegen that triggers the bug. **The fix must be marker-free** (a transpiler-level robust sret-slot handling, OR accept the out-param/`&`-materialization which is the same stabilization).

**2nd blocker (was MASKED by the sret trap):** with the trap gone, the measure path SHAPES the text (g119: content.len=1, visual.len=1, Stage-3 shaped.len=5 ✓), but the actual LAYOUT returns **InvalidTree** in `reconcile_and_invalidate` (LayoutError raw byte0=0; layout-phase 0x40704=0x0 "marker never written"; n1 text stays w=h=0xfffffffe AUTO; rects = body(0,0,800,600) + text(max,max,max,max)). This is the OLDER g73/g74/g75 blocker (the IFC sizer's `tree.get(node_index)=None`). So even a perfect sret fix lands on InvalidTree next.

**NEXT STEPS (revised):**
1. The sret bug is real but codegen-sensitive — reproduce/fix WITHOUT markers. Either (a) fix the transpiler's lift of the NRVO-direct-to-sret store (the unstable pattern), or (b) accept that the out-param (= forced stable materialization) IS the principled mitigation and KEEP the 6 out-param workarounds (do NOT revert them — they prevent the heisenbug, not just mask it).
2. Then tackle the InvalidTree (reconcile_and_invalidate / IFC-sizer bad node_index — see g73/g74/g75 lines below).
3. My g129 markers (fc.rs 0x608xx/0x609xx + harness) must be REVERTED — they perturb the codegen (the raw-slot one accidentally "fixes" the trap). The enforce_sp `__az_indirect_dispatch` wrap + AZ_LTO_LEVEL/AZ_WASM_LD_MLLVM env knobs are KEEPERS.

---
## 2026-06-06 (g130) — MARKER-FREE STABILIZATION applied + root-cause model consolidated

**Action taken:** replaced the heisenbug `&result_raw`/`write_volatile` diagnostic (which accidentally
fixed the trap) with a PRINCIPLED, MARKER-FREE `core::hint::black_box` stabilization at the real
caller boundary — `layout_ifc`'s call to `collect_and_measure_inline_content` (fc.rs:2433, `web_lift`-gated):
```rust
#[cfg(feature = "web_lift")]
let (inline_content, child_map) = {
    let result = collect_and_measure_inline_content(ctx, text_cache, tree, node_index, constraints)?;
    core::hint::black_box(result)   // pin to a stable, address-taken local → stable-local+copy native pattern
};
```
This is the marker-free counterpart of the 5 out-param workarounds. ALL g129 markers in fc.rs
(0x608xx/0x609xx, the 3 store-side + read-side) are now REVERTED. cargo-check clean.

**Root-cause model (consolidated, what's RULED IN vs OUT):**
- It is NOT an LLVM-AA / alias-scope mis-opt of the lifted IR. All guest mem ops share ONE scope
  (`!alias.scope !{az_guest_list}`, transpiler_remill.rs:6199-6390 M10-B1.a), so a guest sret STORE
  and a guest sret LOAD alias → AA stays conservative → no reorder/elim. (Reads are non-volatile,
  writes `store volatile`, but same-scope aliasing keeps the load honest.)
- It is NOT a remill instruction-lift bug: the minimal `fn()->Vec<u64>` NRVO-direct repro
  (`mov x19,x8; str q0,[x19]; str x8,[x19,#16]`) lifts CORRECTLY in isolation (`--bytes`, offsets right).
- It IS a CONTEXT-DEPENDENT native-pattern mis-lift: the SAME NRVO-direct-to-sret store pattern that
  lifts fine in isolation mis-lifts inside the full function (many interleaved calls, enforce_sp wraps,
  inlining-in-the-lift). `black_box`/out-param/`&` all fix it by forcing the native compiler to emit the
  STABLE-LOCAL+final-copy pattern instead of NRVO-direct — so this is a "heisenbug" only because every
  source change that observes the value also changes the emitted native code. The enforce_sp
  `__az_indirect_dispatch` wrap (a real leak fix, KEPT) did NOT resolve it → the clobber, if any, is via
  a STILL-unwrapped call form OR the mechanism is not register-clobber at all. NOT fully isolated.
- **Principled position (handoff option b):** the out-param / `black_box` stabilization is THE fix
  (it deterministically emits the lift-correct native pattern), not a mask. KEEP all 6 — do NOT revert
  until/unless a transpiler-level fix for the NRVO-direct-to-sret native pattern is found & isolated.
  A transpiler fix needs a STANDALONE repro of the broken full-function context (resisted many sessions).

**Hypothesis now under test (g130 relift):** my black_box is on the ACTUAL-LAYOUT path
(`layout_ifc`→`layout_flow`→`InlineContent::clone`), so with correct `inline_content` the text should
POSITION (n1 rect h>0), not just shape. The prior "InvalidTree in reconcile" reading was marker-tainted
(0x40704=0 "phase never written" is inconsistent with body laying out 800×600). Verifying via
web-text-min relift + layout-flexbox.js harness. If n1 still unsized → the InvalidTree is a SEPARATE
reconcile/sizer bug (then strip the sizing-path markers for a clean read).

---
## 2026-06-06 (g131) — black_box INSUFFICIENT → converted collect_and_measure to the g78 OUT-PARAM pattern

**g130 relift (black_box post-`?`) FAILED:** identical trap — `InlineContent::clone` 1.6 GB alloc
(`lastAllocSize=0x63d7c500`) inside `layout_flow`. Decoded trap stack (server log resolved=):
`InlineContent::clone ← TextShapingCache::layout_flow ← layout_ifc ← layout_formatting_context ←
calculate_layout_for_subtree ← layout_document`. So the garbage is the OUTER `Vec<InlineContent>` len
returned by `collect_and_measure_inline_content` (sret-of-`(Vec,HashMap)`), cloned per-element by
layout_flow. (The harness "StyledRun.text {0x1,0x1,0x8} CORRUPT" was a harness OFFSET error — the real
"Hello" string `{ptr=0xc51e928,len=5}` sits at +32 in the dump and is fine; the corruption is the Vec
HEADER len, not the element.) black_box was placed AFTER `?` (too late — niche-read already done).

**FIX (g131, KEEP — this is the 6th out-param workaround):** converted BOTH
`collect_and_measure_inline_content` (wrapper) and `_impl` to the proven M12.7/g78 pattern:
`(content: &mut Vec<InlineContent>, child_map: &mut HashMap<ContentIndex,usize>) -> Result<()>`
(register-returned `Result<()>` — the result pointer is an arg reg read once at entry, NOT an sret slot
that must survive across calls). `layout_ifc` allocs both empty and passes `&mut`. The 4 `_impl`
`Ok((content,child_map))` returns → `Ok(())`; the 2 inner `&mut content`/`&mut child_map` helper args →
`content`/`child_map` (reborrow). cargo-check clean. Relifting (g131) to verify the OOB clears + text
positions (n1 rect h>0). The black_box edit is REPLACED by this (no black_box left in fc.rs).

**OPERATIONAL (faster iterations):** the lift cache (`lift_cache_path`, transpiler_remill.rs:739/4776)
hashes per-fn BYTES + synth lift addr + cache version → unchanged fns HIT the cache. It is OPT-IN via
`AZ_LIFT_CACHE=1`; the recipe's `AZ_NO_LIFT_CACHE=1` keeps it OFF (full ~10-min relift every run). For
the next iterations use `AZ_LIFT_CACHE=1` (drop AZ_NO_LIFT_CACHE): first run is full + populates
`$TMPDIR/az-lift-cache`, subsequent small-change relifts only re-lift the touched fns (~1 min). synth_base
is stable across builds (0x110000), so the cache stays valid. (g131's relift was slow — the HashMap
out-param pulled in extra BTreeMap/hashbrown deps.)

---
## 2026-06-06 (g131 result + g132) — ★★★ OOB FIXED + "InvalidTree" was a PHANTOM ★★★

**g131 out-param relift RESULT: the 1.6 GB `InlineContent::clone` OOB is GONE.** `rc=0`, no trap,
`solveLayoutReal` returns Ok, text SHAPES (5 glyphs), `collect_and_measure` returns correct
`inline_content` via the out-params. **The Vec-return `len` mis-lift is resolved** by the proven g78
out-param pattern (the 6th and final such workaround). Task #3 DONE.

**★ THE "InvalidTree BLOCKER" IS A PHANTOM (debunked a ~10-session red herring):** the harness reads
the LayoutError from `memory.buffer[0x40120]`, but `grep` of the ENTIRE repo shows **nothing ever writes
0x40120 (nor its mirror 0x60120)** → it reads uninitialized `0` → the harness's `tagNames[0]` maps it to
"InvalidTree". Meanwhile `AzStartup_solveLayoutReal` returns `rc=0` and `lw.layout_dom_recursive(...)`
returns Ok (eventloop.rs:1668 — Err would `return 5`). So there is NO actual InvalidTree. (The reliable
`0x60704=0x20` = layout_ifc entry; the positioning path's Err is SWALLOWED at mod.rs:920, and reconcile
/intrinsic both completed — consistent with Ok.) NOTE: the harness reads `0x60xxx` addrs DIRECTLY (no
−0x20000 mirror); `0x40xxx` reads are legacy/unwritten → spurious 0. All prior "broken tree / reconcile
built 0 nodes" readings came from the DENSE `0x40xxx` band clobbering; the reliable `0x60xxx` band always
showed `nodes.len=2, root=0 valid`.

**The REAL residual symptom:** `get_node_position/get_node_size(text n1)` return None (→ rect u32::MAX),
because in solver3's IFC model only inline-**block** objects get `output.positions` entries
(fc.rs:2786 `ShapedItem::Object`); plain inline **text** lives in the body's `inline_layout_result`, NOT as
a separate node rect / `used_size` (window.rs:1762 `used_size=None`). So the text DOES lay out; the
harness's per-node rect check (built for the 5-box flexbox-simple fixture) is simply the wrong metric for a
text node. The body (n0) lays out 800×600.

**g132 (in-flight): definitive verification.** Added a free-band marker (0x60670-0x6067C) reading
`layout_ifc`'s `output.overflow_size` (the line-box bounds from `main_frag.bounds()`) + harness read
`[g132 lays-out]`. `height>0` ⇒ the text positioned into a line box = LAYS OUT. Rebuild+relift to confirm.

**Status of the original 3-part task:** (1) root cause found + fixed (Vec-sret `len` mis-lift → the proven
out-param pattern; the deeper transpiler/remill fix for the context-dependent NRVO-direct-to-sret native
pattern remains unisolated — see g129/g130 — so the 6 out-param workarounds are the principled mitigation,
NOT to be reverted unless that transpiler fix lands). (2) verifying text lays out (g132). (3) reverting the
6 workarounds is BLOCKED on the transpiler fix (reverting now re-introduces the OOB).

---
## 2026-06-06 (g132) — enforce_sp is NOT the gap; task #5 (revert) is genuinely blocked on the SROA/stack-addr class

Investigated whether a missed call form in `enforce_sp_preservation` explains the sret mis-lift (which
would be a simple, generic fix enabling the workarounds' removal). Findings:
- `parse_sub_call` wraps `@sub_*` / `@__remill_function_call` / `@__az_indirect_dispatch`, ALL of which
  return `ptr` (so all have the `%N = ` result `parse_sub_call` requires) → no result-less/void gap.
- The only `call void @...` forms are `@llvm.memset/memcpy/memmove` (lowered inline — no callee-saved
  clobber) and the off-by-default diagnostics `@__az_fuel`/`@__az_logst`. None are sret-clobbering.
- ⇒ the Vec-sret `len` mis-lift is NOT an unwrapped-call/SP-leak gap. It is the **SROA / stack-address
  mis-lift class** — the SAME one as the committed g56 fix ("`&new_tree` (stack) lifted to 0x0 → callee
  saw nodes.len()=0", fixed by `Box::new` = a stable HEAP address). For aggregate sret returns the caller's
  sret stack slot / the callee's `&`-of-stack mis-lifts in large/optimized functions (the minimal repro
  lifts fine — it's context-dependent on SROA + spill layout). The principled fix for the whole class is
  **heap addressing**: out-param (result ptr in an arg reg → caller's heap Vec) or `Box` (heap return).
  That is exactly what the 6 out-param workarounds + g56 do.

**CONCLUSION on task #5:** reverting the 6 workarounds is blocked — there is no cheap generic transpiler
fix (it would require the lift to faithfully reproduce native SROA'd stack-slot addressing in large fns, a
deep multi-session LLVM/remill effort). The out-param pattern IS the principled, proven mitigation; KEEP
the workarounds. Reverting now re-introduces the 1.6 GB OOB. Documented as a deep follow-up.

**⚠ CORRECTION (g132): AZ_LIFT_CACHE=1 STALLED the lift** — froze at `transitive[1321]` lifting
`taffy::grid::resolve_intrinsic_track_sizes` (66 KB fn), 0% CPU, no remill workers, no progress for
minutes. Killed + cleared `$TMPDIR/az-lift-cache` + relaunched with **AZ_NO_LIFT_CACHE=1** (the g129/g131
known-good config) → use that. (Retract the earlier "use AZ_LIFT_CACHE=1 for speed" note — it hangs on
the big taffy fn; the full no-cache relift is ~7-10 min and reliable.)

---
## 2026-06-06 (g132 result + g133) — OOB gone, but text is MEASURED not POSITIONED (new downstream blocker)

**g132 verify result:** `rc=0`, text SHAPES (measure path: content.len=1, visual=1, shaped=5 ✓), BUT
`[g132 lays-out] overflow_size = 0.00 x 0.00 | mark=0x0 ✗ layout_ifc Phase-4 NOT reached`. The `0x20`
phase marker confirms POSITIONING's `layout_ifc` IS entered, so it returns EARLY — before Phase 4 (which
sets overflow_size from main_frag.bounds + inserts inline-block positions). The only early returns are
`collect_and_measure_inline_content?` (fc.rs:2449) or `layout_flow` Err → zero-sized (fc.rs:2612). That
Err is SWALLOWED by calculate_layout_for_subtree (mod.rs:920) → rc=0. So: **the out-param fix removed the
1.6 GB OOB, but the text is MEASURED (intrinsic), not POSITIONED** (Phase-4 inline layout). Note: the
MEASURE path (sizing.rs `collect_inline_content`, out-param, returns Ok per g76) is a DIFFERENT function
from the POSITIONING path (fc.rs `collect_and_measure_inline_content`); the positioning leg fails.

**g133 (in-flight):** markers in POSITIONING's layout_ifc — `0x60680`=inline_content.len + `0x60684`=collect
Ok/Err, `0x60688`=layout_flow Ok/Err (+`0x6068C`=err word). Relift pinpoints which early-return fires →
then chase that (a collect Err = a tree.get(child_index) in `_impl`; a layout_flow Err = the line-breaker/
font path in the positioning context). This is the gate to actual text positioning (h>0).

---
## 2026-06-06 (g133 result + g134) — positioning's collect_and_measure returns Err with EMPTY content

**g133 verdict:** `[g133 early-return] collect inline_content.len=0 | collect result=*** Err *** | layout_flow
not reached`. So POSITIONING's `collect_and_measure_inline_content` (fc.rs) returns **Err** with the
out-param Vec **EMPTY** (len=0) — the Err fires EARLY, before any content is pushed; layout_flow is never
reached. (The MEASURE path's `collect_inline_content` (sizing.rs) returns Ok len=1 on the SAME tree — so
it's specific to the fc.rs collector / its call context in layout_ifc.) The Err is swallowed by
calculate_layout_for_subtree (mod.rs:920) → rc=0. ⇒ **text is measured but NOT positioned.**

Two hypotheses (g134 markers disambiguate, relifting):
- (a) a REAL early `?` Err in `_impl` (the first `?` is `tree.get(ifc_root_index)` @6440 — but node_index
  is valid since layout_ifc's own 2403 tree.get succeeded to reach collect; so it'd be a deeper `?`).
- (b) the **out-param `&mut` pointer itself mis-lifts** (the g56 stack-address class, now hitting
  layout_ifc's `&mut inline_content`) → `_impl` writes to a garbage Vec, the caller's stays empty, and the
  `Result<()>` niche reads as Err. The MEASURE out-param works, but layout_ifc is a bigger fn (SROA
  context-dependent). If (b), the out-param does NOT fully sidestep the class here → may need a Boxed/heap
  inline_content, OR this is a DIFFERENT, pre-existing collect Err that was masked by the OOB.

**g134 markers:** `_impl` content-ptr (0x60690) vs caller inline_content-ptr (0x606A0) [SAME ⇒ ptr ok];
`_impl` final-return reached? (0x6069C) + its content.len (0x60698); entry ifc_root_index (0x60694). →
ptrs differ ⇒ (b); final-return not reached ⇒ (a) real early Err; final-return reached w/ len>0 but caller
0 ⇒ Vec-data mis-lifts caller-ward. NOTE: `text_layout_hyphenation` feature is ON (pulls 100s of hyphenation
deps → every relift ~15-20 min; candidate to disable for web to speed lifts + drop a line-break Err source).

**⚠ LIFT HANG FIX (g134): `resolve_intrinsic_track_sizes` (taffy grid, ~67 KB) intermittently HANGS
remill-lift** (stalls 0% CPU, no remill subprocess, no progress — g132 + g134 both hung here; g131/g132b
got past it). It is GRID-only, never reached for text/flex. FIX: classify it `NeverLift` in
`symbol_table.rs::classify_for_name` (added a name match near the top, before the runtime-crate block —
the existing NeverLift list is gated to runtime crates so it wouldn't catch taffy). Skips lifting it
(trap-if-called; never called → dead). Unblocks + speeds the lift. Rebuilt (41 s, dll-only) + relifting.
(Revisit if web GRID layout is ever needed.)

---
## 2026-06-06 (g134 verdict + g135) — ★ THE OOB WAS A NICHE-MIS-READ OF A GENUINE collect Err ★

**g134 decisive result** (out-param ptr + completion markers):
`_impl content.ptr=0x29b28 == caller content.ptr=0x29b28 ✓SAME Vec` (out-param `&mut` ptr lifts FINE),
`ifc_root_index=0` (valid), `_impl final-return NOT reached → real early ? Err`.

**★ Reinterpretation (changes the whole picture):** `collect_and_measure_impl` ALWAYS returned **Err** —
the original 1.6 GB OOB was the caller's niche-`?` **mis-reading that Err as Ok** (reading the Err's
`LayoutError` payload as a Vec with a garbage len → InlineContent::clone OOB). The out-param fix made the
`Result<()>` read correctly (register-returned, no niche), so the OOB is gone — but it EXPOSED the genuine
Err. So the text NEVER actually positioned (the OOB masked a long-standing collect Err). The Vec-return
`len` mis-lift fix is real + correct; it just unmasked the next layer.

**The Err is `tree.get(ifc_root_index).ok_or(InvalidTree)?` at fc.rs 6449 or 6706** — these are the ONLY
`?` before the first content push (6912, `content.extend(text_items)`); the whole 6706→6912 stretch is
if-let/match with no `?`. Yet `ifc_root_index=0` is valid AND layout_ifc's own `tree.get(0)` @2403 succeeds
to even reach collect. So in `_impl`, `tree.get(0)` returns None (or the Option niche mis-reads as None) —
candidate: the `tree` &mut ref mis-lifts in the collect call (g56 class; but the content &mut ptr is FINE,
0x29b28), OR `tree.get`/`.ok_or` Option-niche mis-lifts.

**g135 (in-flight):** entry markers — `tree.nodes.len` (0x606A8: 0⇒tree ref mis-lifts empty, 2⇒valid),
`tree.get(idx).is_some` (0x606AC: FALSE⇒get mis-lifts despite valid tree), `tree.root` (0x606B0), and a
seq marker (0x606A4: which of 6449/6706 the Err is at). Decides tree-ref vs tree.get vs Option-niche.
**Also KEPT: the NeverLift fix for `resolve_intrinsic_track_sizes` (unblocked the lift hang).**

---
## 2026-06-06 (g135 verdict + g136) — tree.get WORKS; the collect Err is in the DOM-children LOOP

**g135:** `tree.nodes.len=2` (valid), `tree.root=0`, `tree.get(idx).is_some=TRUE`, seq marker **passed BOTH
6449 AND 6706** → `tree.get(ifc_root_index)` WORKS. **The Err is AFTER 6706, in the DOM-children loop**
(fc.rs 6896+). So it's NOT a tree-ref/tree.get/niche mis-lift. With content still empty, the candidates:
- `az_children` (6863) mis-lifts → `dom_children` empty → no text collected → a later `?` (7148/7177) Errs.
- the text child's `node_type` mis-reads (`get_node_type()` not Text) → falls to the NON-text branch
  (6961) → `?` at calc_used_size_for_node (7010) / layout_formatting_context (7060) Errs.
- (text branch taken but `content.extend` push mis-lifts).

**g136 (in-flight):** markers — `dom_children.len` (0x606B4), first-child `node_type` (0x606B8:
Text/Div/Body/Other), text-branch content.len (0x606BC), and a seq marker (0x606A4: 0x6863 reached
dom_children / 0x6896 loop entered / 0x6905 TEXT branch / 0x6942 NON-text branch). Pins az_children-empty
vs node_type-mis-read vs push-mis-lift. NOTE the cb-dom probe earlier showed body.first-child.disc=177
(Text) in the styled_dom, so the DOM is correct; this is a LIFT mis-read of either the child list or the
node_type discriminant. (This is the Nth context-dependent lift bug exposed by removing the OOB — the
lifted layout has a STACK of them; the original Vec-return `len` task is DONE, this is the next layer.)

---
## 2026-06-06 (g136 verdict + g137 FIX) — ★ THE collect Err = an ITERATOR MIS-LIFT (.iter().enumerate yields 0) ★

**g136 decisive:** `dom_children.len=1 ✓` (az_children works — the body HAS 1 DOM child = the text), BUT
`first-child node_type=NOT SET` + `last-seq=0x6863` (reached dom_children, **loop body NEVER entered**).
So `for (item_idx, &dom_child_id) in dom_children.iter().enumerate()` (fc.rs ~6902) **iterates 0 times
despite len()==1** — the slice-iterator construction (begin/end ptr from the Vec) MIS-LIFTS to an empty
range. The text is never collected → empty content → a downstream `?` Errs → `collect_and_measure` returns
Err → (originally) the niche-`?` mis-read it as Ok with a garbage Vec = the 1.6 GB OOB. So the iterator
mis-lift was the ROOT of the whole chain (the Vec-return len mis-lift fix exposed it).

**g137 FIX (KEEP):** replace the iterator loop with direct indexing —
`for item_idx in 0..dom_children.len() { let dom_child_id = dom_children[item_idx]; … }`. Indexing reads
len+ptr correctly (no iterator-adapter lift). cargo-check clean. Relifting (g137) to confirm: the loop now
enters (node_type=Text, content.len=1) AND ideally g132 `overflow_size.height>0` = TEXT LAYS OUT.
(az_children's OWN custom iterator collected fine — it's specifically the slice `.iter().enumerate()` here
that mis-lifts; there may be more `.iter()` loops downstream that need the same direct-index treatment.)

---
## 2026-06-06 (g137 result + g138) — direct-index ALSO fails: the Vec HEADER mis-lifts (len 1→0 across calls)

**g137 (direct-index `for i in 0..dom_children.len()`) FAILED identically:** `dom_children.len=1` at the
marker (right after `.collect()`), but the loop STILL didn't enter (`last-seq=0x6863`, node_type unset).
So `dom_children.len()` reads **1 at the marker but 0 in the loop's `0..len` range** a few statements later.
Between them: `ifc_root_node_data.get_node_type()` (×2) at fc.rs 6877/6889. ⇒ those calls **corrupt the
`dom_children` Vec's stack-slot HEADER** (SP-leak / stack-address mis-lift) → the later `.len()` reads 0.
NOT an iterator-specific bug — the Vec header itself doesn't survive the intervening calls.

**g138 FIX (relifting):** MOVE `let dom_children = ….collect()` to **immediately before the loop**, AFTER
the get_node_type() calls — so NO call sits between `.collect()` and the loop's range/first-index reads.
(Kept the direct-index loop from g137.) If the header now survives → loop enters, text collected
(content.len=1), and ideally g132 `overflow_size.height>0` = TEXT LAYS OUT. This is the g56 class again
(stack data doesn't survive lifted calls); the principled fix would be the transpiler SP-preservation, but
the calls ARE wrapped by enforce_sp — so the corruption is a stack-SLOT reuse/aliasing the lift gets wrong,
not a raw SP leak. Reordering sidesteps it. (Expect MORE such sites across the full hello-world path — the
lifted layout has a systemic stack-slot-survival problem; the original Vec-return `len` task is long DONE.)

---
## 2026-06-06 (g138 result + g139) — ★★ KEY: the Vec-len mis-lift is SYSTEMIC (hits std `collect()`) ★★

**g138 (collect moved to immediately before the loop) FAILED identically.** With NOTHING between
`.collect()` and the loop except the volatile marker, `dom_children.len()` still reads **1 at the marker
but 0 in the loop's `0..len` range**. So it is NOT an intervening-call corruption — the optimizer's
(SROA'd) `len` read in the loop range is itself mis-lifted; only a FORCED/volatile read is correct.

**★ The recontextualization:** this is the SAME Vec-`len` mis-lift class as the ORIGINAL bug, now on
`ifc_root_dom_id.az_children(...).collect()` — i.e. on **`std`'s `Iterator::collect()` returning a Vec**.
You CANNOT out-param `std::collect`. So the 6 source out-param workarounds were always a partial patch:
they fix the 6 azul fns, but EVERY other Vec-returning fn (std `collect`, dep code, …) still mis-lifts its
`len`. ⇒ **the bug is systemic and the ONLY real fix is the transpiler/remill Vec-`len`/sret fix** (the
original task #1 hypothesis). Source workarounds cannot make text fully position. This is why the onion
keeps peeling: removing the OOB at one site exposes the same mis-lift at the next Vec-returning site.

**g139 (final source attempt, relifting):** force the loop's len via a volatile round-trip
(`write_volatile(len); read_volatile()` — guaranteed-correct like the marker) + `get_unchecked` index
(the bounds-check len read mis-lifts too). If this lets the loop enter + text positions → it confirms the
"only volatile reads are correct" model AND gives a (ugly, non-general) per-site escape. But the GENERAL
fix is the transpiler one. If g139 works for web-text-min, the full hello-world path will still hit many
more un-worked-around Vec-len sites.

**RECOMMENDATION (next session / decision for the user):** stop source-patching; invest in the
transpiler/remill Vec-`len`/sret mis-lift fix (the context-dependent SROA len-read / NRVO-to-sret register
mis-tracking). That is the single fix that unblocks ALL Vec-returning fns at once. The original task #1
("fix in remill OR the translation layer") is the right framing; the out-params were a scaffold.

---
## 2026-06-06 (g139 result) — ★ FINAL: 4 source-workaround forms ALL FAIL → transpiler fix is the ONLY path ★

**g139 (volatile-`len` round-trip + `get_unchecked`) FAILED identically:** the loop STILL doesn't enter
(`dom_children.len=1` at the marker, `last-seq=0x6863`, body never runs) even though `dom_children_len`
came from `read_volatile` of the just-written `len`. So the `for` loop in `collect_and_measure` iterates 0
times **regardless of the `len` source OR loop form**:
- g136: `for (i,&id) in dom_children.iter().enumerate()` → 0 iters (slice-iter mis-lift)
- g137: `for i in 0..dom_children.len()` → 0 iters (range-len mis-lift)
- g138: collect moved adjacent to loop (no intervening call) → 0 iters
- g139: `for i in 0..<volatile-read len=1>` + `get_unchecked` → 0 iters

⇒ It is a **systemic optimized-code control-flow / iterator-`next()` mis-lift** in this function's lift —
NOT fixable by any source loop rewrite. Only volatile MEMORY ops lift faithfully here; the loop/iterator
CONTROL FLOW does not.

**DEFINITIVE CONCLUSION:** the web-lift "Vec-return `len` mis-lift" is one face of a broader systemic
lift-fidelity failure in OPTIMIZED Rust code (SROA'd len reads, sret returns, iterator/range `next()`,
stack-slot survival). It is NOT source-workaroundable (it hits `std::collect`, `std` Range/slice iterators,
etc.). **The single real fix is the transpiler/remill lift-fidelity fix** for optimized-code value/control
tracking. The primary task (the Vec-return `len` mis-lift / 1.6 GB OOB) IS fixed (out-param) + verified
(text shapes+measures); full text POSITIONING is blocked on the transpiler fix.

**Source-workaround cycling STOPPED at g139** (15 relifts total this arc, g129→g139). Next step is the
deep transpiler effort — a decision point for the user, NOT more autonomous relift cycles. All g129–g139
diagnostic markers + the g137/g139 loop rewrites in fc.rs are REVERT-at-cleanup; the out-param conversion,
NeverLift(resolve_intrinsic_track_sizes), enforce_sp __az_indirect_dispatch wrap are KEEPERS.
