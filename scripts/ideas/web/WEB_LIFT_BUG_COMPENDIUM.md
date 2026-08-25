# Web-Lift Bug Compendium — every bug chased on the aarch64 remill lift, with its real conclusion

**Purpose:** make the **x86-64 / Windows** remill port fast by telling you, per bug, whether the
x86 port **inherits the fix for free**, must **re-solve it for the new ISA**, must **re-solve it
for the new ABI**, or whether it is **still open**. Written 2026-06-12 after the aarch64 web
backend reached: hello-world full render→click→patch cycle, real OpenType shaping, ua_css
Chrome-parity.

## The pipeline in one paragraph (so the verdicts make sense)
`libazul.dylib` (native aarch64) → per-fn machine bytes → **remill-lift-17** (fork
`fschutt/remill@m12-q-reg-x8-sret`) → LLVM IR in `%struct.State` form → byte/IR rewrites +
**classifier** (`dll/src/web/symbol_table.rs::classify_for_name`) decides per-callee whether to
recurse/stub/trap → `llvm-link → opt → llc → wasm-ld` → wasm shards → JS loader runs them against
one linear memory with a bump allocator + `__remill_*` memory shims. The classifier and the
JS/loader pieces are **target-agnostic**; the remill decoder and the ABI assumptions are **not**.

## Verdict legend
- **[INHERIT]** target-agnostic — the x86 lift gets this fix for free if it reuses
  `dll/src/web` + the loader. Verify, don't re-derive.
- **[ISA]** aarch64 instruction-decoder/semantics gap — x86 has its own decoder (Intel **XED**,
  far more mature than the AArch64 SLEIGH path), so expect *fewer* gaps, but re-run the method.
- **[ABI]** depends on AArch64 AAPCS64 — **must** be re-solved for System V AMD64 **and**
  Windows x64 (they differ from each other too).
- **[OPEN]** not fixed; still gated by a workaround.

---

## Category A — target-agnostic pipeline / classifier bugs  → **[INHERIT]**
These were the hardest to find but they live in `dll/src/web` (classifier, loader, shims), not in
arch-specific code. The x86 lift inherits every one. **This is most of the iceberg.**

### A1. Mechanism B — the classifier Leaf-stub gap *(the keystone bug; fixed `cb017d266`)*
- **Symptom:** text layout traps; a `String`'s `len` field holds a heap pointer (`0xa294828`),
  building a 170 MB phantom `&str` → `finish_grow` OOB. Garbage shaped like `{ptr=1, len=<ptr>}`.
- **Root cause:** `classify_for_name` defaulted runtime crates `alloc`/`core` to `FnClass::Leaf`
  (a no-op env-import stub). So out-of-line monomorphizations the compiler didn't inline — e.g.
  `alloc::str::join_generic_copy` behind `<[&str]>::join` — were **never lifted**; the call did
  nothing and the caller read 24 bytes of stale stack as the result. NOT a register mis-lift.
- **Why it took ~4 sessions:** the bisect tools (`AZ_BISECT_FN`, `AZ_LOWOPT_FNS`) only act on
  functions *being lifted*, so every "still corrupt at opt-0" result was **vacuous** — we were
  debugging a function that didn't exist in the wasm.
- **Fix:** default `alloc`+`core` → `Recursable` (they are `no_std`/no-syscall by construction;
  allocator shims already matched `BumpAlloc*` by name, diverging panics already `NeverLift`,
  `core::ops::function::impls` keeps a known-trap Leaf carve-out). This **subsumed and retired**
  a whole whack-a-mole list of prior one-off exemptions (raw_vec, btree, from_iter, spec_extend,
  resize, sort, binary_search, utf8, FnOnce, OnceLock) — every one had been a separate incident
  of this single gap.
- **[INHERIT]** Pure classifier logic, no arch content.
- **LESSON (write this on the wall for x86):** *when lifted code yields garbage, FIRST `grep` the
  relift log for the producing function's `class=` line and confirm it was actually lifted —
  before debugging its lift.* This one check would have saved days.

### A2. `LibcMemcpy` / `LibcMemset` — out-of-image libc primitives *(M10/M12)*
- **Symptom:** `Box::new(big_struct)` and `<[T]>::to_vec` silently leave the destination
  zero-filled (`node_data.len == 0`); hashbrown tables come back empty / `HashMap::insert` hangs.
- **Root cause:** Rust lowers large struct moves to an out-of-line `bl _memcpy`; the symbol
  PLT-chases to a libsystem address no rebase covers → default `Leaf` stub **returns without
  copying**. `memset` same story → hashbrown's `0xFF` control-byte init never happens.
- **Fix:** dedicated `FnClass::LibcMemcpy`/`LibcMemset`/`LibcSnprintf` matched by name; helper IR
  emits a real `@llvm.memmove`/`@llvm.memset`/minimal `%d` formatter body.
- **[INHERIT]** Name-matched, body is LLVM intrinsics. **x86/Windows caveat:** confirm the same
  names appear (`memcpy`/`memmove`/`memset`/`__memcpy_chk`/`_platform_*`). **Windows adds two new
  spellings the matcher must learn:** (a) **IAT import thunks** — `jmp qword ptr [rip+disp]` →
  `__imp_memcpy` entry → import name (the PE analog of the aarch64 PLT/branch-island chase that
  caused the M12 `Box::new` zero-fill); (b) **`__chkstk`** — MSVC/rust9x emits a stack-probe call
  at the prologue of *every* function with a frame > 4 KB, so it appears all over big lifted fns
  and needs its own no-op-with-correct-SP-semantics class (a plain `Leaf` that returns is wrong —
  it must leave SP as if the probe ran). Grep a Windows relift log for the actual import names and
  extend the matcher.

### A3. `HashmapRandomKeys` — degenerate SipHash seed *(M12.7)*
- **Symptom:** every lifted `std::HashMap` reads back empty (the `dom_to_layout` symptom,
  systemic).
- **Root cause:** `std::sys::random::hashmap_random_keys` is a `getentropy` syscall wrapper that
  can't be lifted; the `Leaf` stub returns 0 → `RandomState`'s `(u64,u64)` keys are unusable.
- **Fix:** dedicated class returning a **fixed non-zero seed** (consistency, not randomness, is
  all that's needed in a single-process lift).
- **[INHERIT]** Name-matched. **x86 caveat:** Windows uses `BCryptGenRandom`/`RtlGenRandom` under
  a different std path — the matcher keys on `hashmap_random_keys`, which is the std-internal name
  regardless of OS, so it should still hit; verify.

### A4. Bump allocator family — `BumpAlloc` / `BumpRealloc` / `BumpDealloc`
- `__rust_alloc`/`_zeroed`/`realloc`/`dealloc` (+ `__rdl_*`/`__rg_*` aliases) → helper IR bumps
  `@__az_bump_ptr`. Realloc copies `min(old,new)`; dealloc is a no-op (bump never frees).
- **[INHERIT]** Name-matched; the bump heap is target-agnostic. **x86 caveat:** the allocator
  shim tail-call chain (`__rust_alloc → __rdl_alloc`) is detected via `detect_arm64_tail_shims`
  (`b` immediate). x86 tail calls are `jmp rel32`/`jmp [rip+...]` — **the tail-shim detector is
  ISA-specific** (see B7). If the chain isn't followed, the canonical address lands on the wrong
  name and the `BumpAlloc` match misses.

### A5. LLVM libcall math stubs — `fmaxf` / `fminf` / `roundf` *(M12.7, the body-width=0 bug)*
- **Symptom:** every laid-out width collapsed to 0.
- **Root cause:** LLVM lowers `f32::max/min`/`.round()` to `@llvm.maxnum/minnum.f32`+`roundf`
  **libcalls**, which the JS harness stubbed to `()=>0` → every `.max(0.0)` floored to 0.
- **Fix:** real `fmaxf`/`fminf`/`roundf` in `dll/src/web/loader_js.rs` (`azMakeMiniImports`
  realEnv) + `AZ_MATH` in the e2e JS.
- **[INHERIT]** JS loader env. **x86 caveat:** x86 LLVM may keep these as SSE inline
  (`maxss`/`roundss`) rather than libcalls, meaning the *import may not appear at all* — if it
  doesn't, no action; if it does, the stub already exists.

### A6. Mechanism A — EarlyCSE across `!alias.scope`/`!noalias` *(fixed `dd055272e`)*
- **Symptom:** a `&str` fat-pointer's two words (ptr@sp+472, field@sp+488) collapse to one value.
- **Root cause:** remill tags State-register vs guest-memory accesses as mutually `noalias`
  (sound on real HW with disjoint address spaces, **unsound on wasm's single linear memory**
  where 32-bit-truncated guest pointers can alias the State struct). `-O2` EarlyCSE then forwards
  a non-volatile register load across volatile guest stores.
- **Fix:** `strip_alias_scope_metadata()` on the linked IR before `opt` (default-on;
  `AZ_KEEP_ALIAS_SCOPE=1` to keep). Also `strip_noalias_from_sub_args`.
- **[INHERIT]** This is a **remill-metadata-vs-wasm** mismatch, independent of source ISA — remill
  emits the same alias scopes for x86. **Keep this strip on for x86.** (Method that found it:
  opt-bisect on a dup-store heuristic; see Methods §M4.)

### A7. `panic`/diverging `-> !` helpers → `NeverLift` *(M12.5y)*
- **Symptom:** a lifted caller branches to a `-> !` panic handler whose `Leaf` stub *returns*,
  falling through into the dead padding the compiler emits after a `noreturn bl` → remill can't
  decode it → `__remill_missing_block` → `unreachable` trap.
- **Root cause:** noreturn functions never restore SP/callee-saved; a stub that returns corrupts
  the caller's frame.
- **Fix:** `NeverLift` (trap-if-called) for `panicking`, `handle_alloc_error`, `capacity_overflow`,
  `begin_panic`, `rust_begin_unwind`, `panic_access_error`, `already_borrowed`, `unwrap_failed`,
  `expect_failed`, `slice_*_fail`, `panic_nounwind`, etc.
- **[INHERIT]** Name-matched on std/core panic paths.

### A8. TLV / thread-locals *(post-rebase trap-peel)* — **partially [INHERIT], partially OS-specific**
- **Symptom:** lifted `adrp x0,<tlv_desc>; ldr x8,[x0]; blr x8` fell into the dispatcher's
  unknown-drop → std read descriptor bytes as a LocalKey state byte → `panic_access_error`.
- **Fix:** TlvRegion geometry in `symbol_table` + mirror-seed at the chokepoint + thunk rewrite to
  `AZ_TLV_MAGIC_PC` + a dispatcher case computing `X0 = tls_base_synth + desc.offset`. (TLS =
  statics in single-threaded wasm.)
- **Verdict:** the *concept* (TLS → fixed statics) is **[INHERIT]**, but the **mechanism is
  Mach-O-specific**: macOS uses `__thread_vars`/`__thread_bss` + the `tlv_get_addr` thunk. Linux
  x86 uses ELF `.tdata`/`.tbss` + `%fs`-relative `mov %fs:0x...`; **Windows uses `_tls_index` +
  `__tls_array` (`%gs:0x58`) + TLS callbacks**. **[ISA/OS — RE-SOLVE]:** the descriptor layout,
  the access instruction pattern, and the thunk name all change. Expect this to be a chunk of the
  Windows port.

### A10. Heap base vs synthetic-address band collision *(standing class)*
- **Symptom:** after the dylib grew, the bump allocator's base overlapped the image's mirrored
  synthetic-address band → allocations stomped mirrored data ("TLV descriptors read as heap
  garbage"). Patched by moving the bump base 96 MiB → 160 MiB.
- **[INHERIT] but it's a LANDMINE, not a fix:** any future image growth past the band re-collides.
  **Port lesson — do this right on x86:** derive the bump base at startup from the symbol table's
  **max synthetic address + a guard margin**, never hardcode it. Watch the `TLV: seeded` / band
  logs on every relift. (x86 images are bigger; this *will* bite if hardcoded.)

### A9. `display_list` / `probe` / GRID-monster boundaries → `Leaf`/`NeverLift`
- Painters (`generate_display_list` and ~300 `paint_*`) are dead on web (TLV patches, not a
  display list) → `Leaf` boundary. `probe` (timing spans, reads TLS) → `Leaf`.
  `resolve_intrinsic_track_sizes` (67 KB, hangs the lifter) → `NeverLift` (GRID-only, never
  reached for text/flex).
- **[INHERIT]** Pure lift-surface trimming; all name-matched.

---

## Category B — aarch64 ISA decoder / semantics gaps  → **[ISA: re-solve, method transfers]**
These were real holes in the AArch64 decoder. **x86 uses XED (mature) so expect far fewer**, but
SSE/AVX and some Rust-emitted forms will still surface. **The method is the asset, not the list.**

### B1. Undecoded NEON stubs = the "class-A" systemic bug *(2026-06-06)*
- **Symptom:** vector loads/stores silently produce `__remill_error`; lengths come back garbage.
- **Gaps found (one method, many instructions):** `STUR/STR/LDR/LDUR Q`, `FMOV`, `FSQRT`,
  `FABD`, `FCMGT`, `FCVTN`. Method: standalone-lift the failing fn → `grep __remill_error` → read
  the undecoded bytes → add the decoder/semantics to the fork → re-lift to 0 errors.
- **[ISA]** x86 analog: SSE/AVX move/convert ops (`movaps`, `cvt*`, `sqrtss`, `pcmpgt*`). XED
  decodes these, but remill's *semantics* functions may be incomplete for some — same triage.
- **Run the method PROACTIVELY on x86 day one:** lift the *whole* set, `histogram` the
  `__remill_error` stubs by frequency, fix decoders top-down — don't wait for a downstream
  garbage symptom (an undecoded instruction looks exactly like data corruption later).
- **x86 sibling instructions to pre-empt** (the prebuilt `remill-lift-17` already advertises
  `-arch amd64`/`x86` via XED, so decode coverage starts high — these are the semantics/rewrite
  watch-list): **`rep movsb`/`rep stosb`** (inlined memcpy/memset — a *semantics loop*, must lift
  or rewrite or it silently no-ops like the libc stub did), **`cmpxchg16b`** (16-byte CAS, the
  fat-pointer atomic), **SSE 16-byte `movups`/`movdqu` pairs** (the x86 sibling of the aarch64
  Q-register ldp/stp class — B3), **`cpuid`/`rdtsc`** (classify `NeverLift` or fixed-value
  helpers), and **`lock`-prefixed RMW** (the LDAPR sibling — consider a `lock`-prefix-strip
  pre-lift rewrite for single-threaded targets, exactly as LDAPR→LDAR in B4).

### B1-x86. CVTSI2SS-in-jump-table → remill `InstructionLifter` type confusion *(CONFIRMED on x86, 2026-06-12)*
- **Symptom:** `remill-lift-17 --arch amd64` HARD-CRASHES (Windows exit `0xc0000409`,
  STATUS_STACK_BUFFER_OVERRUN — really a glog `LOG(FATAL)`/`CHECK`) on
  `azul_layout::solver3::getters::resolve_font_size_slow`:
  `InstructionLifter.cpp:619 Check failed: val_type->isIntegerTy() Expected XMM0 to be an
  integral type (val_type: [16 x i8], arg_type: i64)` at the `cvtsi2ss xmm0, rax`
  (`F3 48 0F 2A C0`).
- **What it is NOT:** not a decoder gap (XED decodes cvtsi2ss fine — it lifts clean in
  isolation, in 2-instr sequences, and right after a bare indirect `jmp`); not a transpiler
  devirt gap (crashes EVEN WITH the correct `--extra_data` jump table provided). The CVTSI2SS
  semantic itself is structurally right (`DEF_SEM(CVTSI2SS, V128W dst, V128 src1, S2 src2)` —
  reads xmm0 as src1 to preserve the upper lanes).
- **Trigger:** `cvtsi2ss` reached as an arm of a ~13-way enum **jump table**
  (`lea rdx,[rip+tbl]; movslq (rdx,rcx,4),rcx; add rdx,rcx; jmp rcx`) inside a function with
  live cross-block XMM6/7/8 spills. remill's TraceLifter maps the xmm0 operand to the `i64`
  src2 slot in that devirt-reached context → the `val_size>arg_size` integer branch CHECKs
  `val_type->isIntegerTy()` on the `[16 x i8]` register and aborts. Minimal repro NOT isolable
  (needs the full function's register flow).
- **[ISA — OPEN, remill fork fix]** A proper fix lives in remill's `LiftRegisterOperand` /
  the CVTSI2SS ISEL operand mapping. Deep + low-ROI for the first bring-up.
- **WORKAROUND SHIPPED (transpiler, the allowed place):** `lift_with_transitive_deps_sequential`
  now catches a per-function lift failure and substitutes a **Leaf stub `.o`**
  (`produce_leaf_stub_object`) instead of aborting the whole closure — so one fn that defeats
  remill degrades to a no-op (returns 0) rather than stubbing the ENTIRE mini wasm (which had
  lost every `AzStartup_*` export → 8-byte stub). Loud per-fn warning + post-loop summary.
  Safe here because `resolve_font_size_slow` is a slow-path getter reached only via the mini's
  `solveLayoutReal`; the e2e click flow runs the client-side layout wasm instead.
- **LESSON for x86:** the §M1 single-instruction `__remill_error` histogram (all-zero here)
  does NOT catch this class — it's a multi-block CHECK-abort, invisible to per-instruction
  lifts. Pair §M1 with a "lift every fn in the closure standalone, watch for non-zero exit
  codes / FATAL" pass, and keep the lift pipeline resilient to a single fn's hard failure.

### B2. NZCV flag save/restore — `mrs/msr NZCV`
- The only decode gap across 22 text fns: Rust's overflow-checked arithmetic reads/writes the
  condition flags via `mrs x,NZCV` / `msr NZCV,x`. Added to the fork (committed `70050e0`).
- **[ISA]** x86 keeps flags in `EFLAGS` and remill's x86 path models them natively (`pushfq`,
  `lahf/sahf`, arithmetic flag side-effects) — **likely a non-issue on x86**, but verify Rust's
  `checked_*`/overflow paths lift clean.

### B3. NEON Q-register `LDP/STP` (incl. post-index) + the `STP_Q` signed-offset hole
- 128-bit paired load/store of the `q` registers (`ldp q,q,[x],#0x40` etc.) — the original M12
  cascade blocker. Forked remill for `STP_Q` signed-offset (branch `m12-q-reg-x8-sret`).
  `RUSTFLAGS=-C llvm-args=-aarch64-enable-ldst-opt=0` reduces how often LLVM emits the paired form.
- **[ISA]** x86 has no Q-reg pairs; the analog is `movups`/`vmovdqu` of XMM/YMM. Different shape,
  same risk class (wide vector mem ops). Note: the `-aarch64-enable-ldst-opt=0` RUSTFLAG is
  **aarch64-only** — drop it for x86 and find the x86 equivalent knobs if vector pairing bites.

### B4. `LDAPR → LDAR` rewrite *(byte rewrite, `rewrite_ldapr_to_ldar`)*
- ARMv8.3 `LDAPR` (RCpc acquire) isn't in remill's decoder; rewritten to plain `LDAR` (sound for
  single-threaded wasm). Pure 4-byte opcode patch.
- **[ISA]** x86 has no LDAPR; memory-ordering on x86 is implicit (TSO). **Drop this rewrite for
  x86.** But watch for `LOCK`-prefixed RMW and `xchg` if remill's x86 atomics are incomplete.

### B5. Jump-table devirtualization — large enum-match dispatch
- **Symptom:** `CssProperty::{clone,get_type,eq,hash,cmp}` (~73 KB / 179 arms) used `br →
  __remill_jump` → mis-dispatch → cascade OOB cloning a gradient/font-family.
- **Fix:** in-process exact jump-table decode (`azul_remill.cpp ForEachDevirtualizedTarget`) fed
  the `.rodata` offset tables at synthetic addresses via `--extra_data`; small fns (≤12 KB) keep
  a window-sweep fallback.
- **[ISA]** x86 jump tables are `jmp [rax*8 + table]` (different addressing). The *devirt concept*
  transfers but the **table-discovery + the indirect-jump pattern are ISA-specific** — re-solve
  the decode, keep the architecture.

### B6. `adrp + ldr/add` page-relative addressing scans
- `scan_arm64_adrp_pages` / `scan_arm64_adrp_accesses` / `scan_arm64_adrp_add_code_targets`
  mirror the `adrp`-referenced data/code at wasm-friendly low synthetic addresses.
- **[ISA]** x86-64 uses **RIP-relative** (`lea rax,[rip+disp32]`, `mov rax,[rip+disp32]`) — a
  *single* instruction, no `adrp`/`add` pair. **Rewrite these scanners for RIP-relative**; in some
  ways simpler (one instruction), but you must parse x86 variable-length encoding to find them
  (see B8).

### B7. Tail-call shim detection — `detect_arm64_tail_shims`
- Follows `b <imm26>` tail-call chains (e.g. `__rust_alloc → __rdl_alloc`) to a canonical address.
- **[ISA]** x86 tail calls are `jmp rel32` / `jmp [rip+disp]`. **Re-solve for x86 jmp forms** —
  feeds A4 (allocator classification).

### B8. `rewrite_recursive_bl` — self-referential call fixup  → **mostly [INHERIT] in spirit**
- Rewrites in-buffer `bl` targets so recursion lifts. The *need* (relocate intra-buffer calls) is
  target-agnostic, but the **encoding is `bl imm26`** (AArch64). x86 `call rel32` needs its own
  rewriter. **[ISA]** for the encoding, **[INHERIT]** for the reason it exists.
- **Bigger x86 structural issue:** AArch64 is fixed 4-byte instructions, so every scanner above
  strides by 4 and masks opcodes trivially. **x86 is variable-length (1–15 bytes).** Every
  byte-level scan/rewrite (`rewrite_recursive_bl`, `rewrite_ldapr_to_ldar`, all `scan_arm64_*`)
  assumes 4-byte alignment and **must be rebuilt on a length-decoder** (XED gives instruction
  length). This is the single biggest *structural* difference for the x86 port.

---

## Category C — ABI / calling convention  → **[ABI: re-solve for BOTH System V and Windows x64]**
The lift assumes **AAPCS64** everywhere registers carry arguments/returns. x86-64 has two ABIs
that differ from AAPCS64 **and from each other**. This is the second-biggest x86 surface after C/B8.

### C1. Struct return via `X8` (sret) — pervasive
- AAPCS64 returns large structs through a caller-allocated buffer pointed to by **`X8`**. The
  lift/loader seed `X8` everywhere (e.g. `CallIndirectLayout4` adds an `out_ptr` arg → State.X8).
- **[ABI]** System V returns small structs in `RAX:RDX`, large via a **hidden first pointer arg in
  `RDI`** (no dedicated sret register). **Windows x64** returns anything >8 bytes via a hidden
  first pointer in **`RCX`**, shifting all other args right. **Every `X8`/sret assumption must be
  re-derived per ABI.** Expect this to touch `CallIndirectLayout4`, the dispatcher, and the
  hidden-ptr-return plumbing.
- **[ABI — CONFIRMED OPEN on x86/Windows, 2026-06-13]** The top-level `Pcs::HiddenPtrReturn`
  was re-derived (sret = `pcs::SRET` = RCX on Win-x64) and the LayoutCallback runs. BUT the
  full hello-world layout still traps with a memory OOB in the layout-cb closure: an INTERNAL
  struct/Vec-by-value return (AzString / Vec<NodeData> / AzDom) is mis-lifted on x86 → the
  caller reads a garbage pointer → deref OOB (surfaces in `enforce_sp_preservation`'s
  CS-snapshot derefing the garbage-as-State). This is the **x86 analog of the macOS class-B
  bug** (`m12-q-reg-x8-sret`). Ruled out: snprintf, the Leaf-stubs, no-op'd indirect calls
  (the subprocess `__az_indirect_dispatch` is now ON and didn't move it). Minimal layout
  (`AzDom_createBody`) → rc=0, so it's the complex-Dom return sites only. **Next session:**
  `scripts/HANDOFF_FABLE_web_lift_x86_windows_2026_06_13.md` §3 — localize the site (un-strip
  wasm names or AZ_REG_TRACE), confirm with `scripts/mechb_harness/` compiled to the x86 host,
  fix the amd64 return-value lowering in the remill fork or the wrapper's internal-sret plumbing.

### C2. Argument registers
- AAPCS64: `X0–X7`. The lift reads args from those State slots.
- **[ABI]** System V: `RDI, RSI, RDX, RCX, R8, R9`. Windows x64: `RCX, RDX, R8, R9` (only 4) +
  **32-byte shadow space** the caller must reserve. The State-slot indices the lift reads for
  "arg N" are AArch64-specific and **all change**.

### C3. Fat-pointer / two-word value passing — `{ptr, len}` in `X0:X1`
- `&str`/slice/`DomNodeId` pass as two GPRs. Several bugs (getHitNode, the mechB witness) were
  about a two-word value's second word. The *pairing* is ABI-dependent.
- **[ABI]** x86-64 SysV also uses two GPRs for a 16-byte aggregate (`RDI:RSI` or `RAX:RDX`
  return) but the **classification rules differ** (SSE vs INTEGER eightbyte classes). Windows x64
  passes any aggregate >8 bytes **by hidden pointer**, never in two registers — so fat pointers
  are a *pointer to {ptr,len}*, a structurally different lift. **Re-derive.**

### C4. HFA (homogeneous float aggregates)
- AAPCS64 passes structs-of-floats in `V0–V7`. Surfaced in earlier font/geometry lifts.
- **[ABI]** No HFA concept on x86. SysV uses SSE-class eightbytes (`XMM0–7`); Windows x64 passes
  float aggregates by pointer. **Re-derive float-aggregate passing.**

### C5. SP preservation / callee-saved restore *(AZ_FIX_SP, M12 RC#1)*
- `CssProperty::clone` leaked the guest SP; a 12-register wrap (`enforce_sp_preservation`)
  restored it. Callee-saved set is AArch64's (`X19–X28`, `X29/X30`).
- **[ABI]** x86-64 callee-saved is `RBX, RBP, R12–R15` (SysV) + `RSI, RDI` also callee-saved on
  **Windows x64**. The frame/SP-restore logic is register-set-specific. **Re-derive the saved set.**

---

## Category D — still open  → **[OPEN]**
### D1. g147 FC-assignment mis-lift *(classifier-INDEPENDENT; bypass stays in `fc.rs`)*
- **Symptom:** with the IFC-recompute bypass removed, child nodes' stored `formatting_context`
  enum reads **garbage discriminant (0)** → they fall to the `_` dispatch arm; the
  assignment-side markers never fire; click hit-test then misses (`patches_len=0`).
- **Status:** proven NOT the classifier gap (retested after the mechB fix — still red). A real,
  still-live mis-lift in the **`LayoutTreeBuilder` FC-assignment store** of a `repr(C, u8)` enum
  field for child nodes. Bypass (`has_only_inline_children` recompute at dispatch) kept; page
  renders, click works *with* the bypass.
- **Recommended attack:** the **native-execution harness** (`scripts/mechb_harness/`, see §M2) —
  lift the builder's assignment fn, run it against a mock multi-node `StyledDom`, watch the
  `formatting_context` field store. This is arch-neutral, so cracking it on aarch64 fixes x86 too.
- **[OPEN]** — the one carried-forward bug. Likely **[INHERIT]** once solved (it's in the lifted
  Rust semantics / repr(C,u8) enum store, not arch decode).

---

## The METHODS that actually cracked these (the real transferable asset)
The bug list ages; the methods don't. Port these to x86 first.

- **§M1. Standalone single-fn lift + `__remill_error` grep.** `remill-lift-17 --arch <a> --address
  <hex> --bytes <hex> --ir_out x.ll`; `grep -c __remill_error`. Isolates decoder gaps (Category B)
  with zero runtime. The `AZ_PREFLIGHT=1` gate runs this per-fn automatically and reports holes.
- **§M2. Native-execution harness** (`scripts/mechb_harness/`, README inside). Lift the suspect
  fn, compile its IR **to the host ISA** with a ~150-line C harness implementing the `__remill_*`
  memory shims over a flat buffer + stubbing callees, set the State registers to the inputs,
  execute, inspect the output struct. **This is what cracked mechanism B** — it converts an
  uninstrumentable wasm/std bug into a normal native binary you can step in lldb. *Directly
  reusable for x86* (compile to x86 host instead of aarch64). Recommended for D1.
- **§M3. Relift-log `class=` grep (the mechB lesson).** Before debugging any garbage value,
  confirm its producer was actually lifted: `grep "<fn>" server.log` → check `class=Recursable`
  not `class=Leaf`. A no-op stub *looks identical* to a mis-lift at the call site.
- **§M4. opt-bisect for LLVM-opt miscompiles.** Capture real per-fn IR with
  `AZ_REMILL_KEEP_SCRATCH=1` (`{stem}.linked.ll` pre-opt, `{stem}.opt.ll` post-opt), then
  `opt -O2 -opt-bisect-limit=N` binary-search a corruption heuristic (e.g. two adjacent guest
  `store volatile i64` with the same value = a mis-collapsed multi-word value). Found mechanism A.
  Per-fn knobs: `AZ_BISECT_FN`/`AZ_BISECT_LIMIT`, `AZ_LOWOPT_FNS` (per-fn `opt`+`llc -O0`).
  **Caveat (mechB):** these only act on lifted fns — a "still corrupt at limit 0" on a fn that was
  never lifted is meaningless. Pair with §M3.
- **§M5. The classifier is the control panel.** Most "mis-lift" bugs were actually
  classification bugs (stub vs lift vs trap). `dll/src/web/symbol_table.rs::classify_for_name` +
  its unit tests are the first place to look and the cheapest place to fix. **[INHERIT] wholesale.**
- **§M7. "Hang" and "garbage value" are the SAME bug class.** A Leaf-stubbed helper either
  corrupts a value *or* fails to advance a loop → infinite loop (BTreeMap `dying_next`, iterator
  `scan_cursor += len_utf8()`, search loops are the canonical victims; the gsub/gpos "hang" was
  this). So a hang gets the same first move as garbage: §M3 (grep the producer's `class=`).
- **§M8. When a probe changes the result, STOP probing.** Runtime address-observation (marker
  reads, `build_get`) repeatedly shifted or hid the corruption (the M12.5x heisenbug). Switch to
  **marker-free constant-injection bisection**, transpiler-side logging, or the native harness
  (§M2) — never keep stacking in-guest probes.
- **§M9. Test the PRODUCTION artifact, not just the isolated lift.** "Every instruction lifts
  correctly in isolation" proves nothing: the isolated `remill-lift` exercises only the decode
  step. Real bugs lived in the byte-rewrites, sub-name canonicalization, alias-metadata strip,
  jump-table devirt, classification, and the JS import env. Use the isolated lift only as a
  *control* against the full pipeline output.
- **§M6. CDP gate.** `node scripts/cdp_click.js` (counter 5→click→6) and
  `scripts/cdp_uacss.js` (geometry parity vs a `data:` URL — reference **must** carry
  `<!DOCTYPE html>`). End-to-end truth, target-agnostic.

## x86 port — suggested order (cheapest-first, from the verdicts above)
1. **Reuse `dll/src/web` + loader unchanged.** All of Category A (and M5) is [INHERIT]; you start
   with the classifier, bump heap, libc shims, alias-strip, panic-trap, HashmapRandomKeys already
   correct. Just rebuild the spelling lists (A2/A3) against a real x86/Windows relift log.
2. **Rebuild the byte-level scanners on a length decoder (B8).** This is the structural blocker —
   variable-length x86 breaks every 4-byte-stride scan/rewrite. Do this before anything else in B.
3. **Re-solve RIP-relative addressing (B6) and x86 jmp tail-shims (B7)** — feeds allocator
   classification (A4) and data mirroring.
4. **Re-derive the ABI (Category C) for your target first** — pick System V (Linux/macOS x86) or
   Windows x64 and do one fully; they diverge enough that doing both at once will confuse the
   sret/arg plumbing. The `CallIndirectLayout4` + dispatcher + `X8`-sret code is where it lands.
5. **Triage decoder gaps with §M1/AZ_PREFLIGHT** — expect SSE/AVX semantics holes (B1/B3), far
   fewer than aarch64 thanks to XED. NZCV (B2) and LDAPR (B4) drop away.
6. **Carry D1 as a shared hunt** — solve it once (native harness, §M2) and both arches benefit.

**Bottom line for x86:** the expensive, multi-session discoveries (Category A) are **already paid
for and inherited**. The new spend is mechanical-but-real: a length-decoder rebuild of the byte
scanners (B8), RIP-relative + jmp re-solves (B6/B7), and one ABI (Category C). Decoder gaps (B)
should be lighter than aarch64 because XED is mature.
