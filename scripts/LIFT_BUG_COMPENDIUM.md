# LIFT BUG COMPENDIUM — every chased bug, its ACTUAL conclusion, and what it means for the x86/Windows port

Compiled 2026-06-12 after the aarch64 web-lift reached: hello-world full input cycle green,
real gsub/gpos OpenType shaping, ua_css pixel-identical to Chrome, probe-free tree.
Purpose: make the x86-64 (Windows) remill port FAST by not re-deriving any of this.
Every entry: **Symptom → Chase (what it cost) → Actual root cause → Fix → x86/Windows note.**
Tags: [PORTABLE] applies as-is to any arch · [ARM] ARM-specific, has an x86 sibling to
pre-empt · [METHOD] process lesson.

---
## 0. THE LAW (read this if you read nothing else)

**When a lifted-code value is garbage, FIRST grep the relift log for the producing
function's `class=` line.** Confirm the function was actually LIFTED before debugging its
lift. The single most expensive incident in this project (mechanism B, ~4 sessions, ~25
relifts, 12+ "ruled-out" hypotheses) was a function that was never lifted at all — every
experiment on its lift quality was vacuous. Corollary: env-var diagnostics
(AZ_BISECT_FN, AZ_LOWOPT_FNS) only bite on functions IN the lift set; applying them to a
stubbed fn silently no-ops and produces misleading "still corrupt" results. [METHOD]

---
## 1. Classifier policy bugs (the biggest family by damage)

### 1.1 mechanism B — alloc/core Leaf default [PORTABLE — THE big one]
- **Symptom:** `<[&str]>::join` returned String {ptr=1, len=<heap-ptr>} → 170MB phantom
  &str → finish_grow OOB trap. Gated the entire web backend.
- **Chase:** weeks. Chain-split probes proved the corruption "inside join"; isolated
  instruction lifts all correct; opt-bisect-0 + llc -O0 "still corrupt" (vacuous, see §0);
  concluded "uninstrumentable register value-flow mis-lift" — wrong.
- **Root cause:** `classify_for_name` defaulted runtime crates (`alloc`, `core`) to
  `FnClass::Leaf` = no-op env-import stub. Out-of-line monomorphizations
  (join_generic_copy, and before it: raw_vec, btree, from_iter, spec_extend, Vec::resize,
  slice::sort, binary_search, utf8 validation, FnOnce trampolines, OnceLock init — EACH a
  separately-diagnosed incident of the same gap) never executed; callers read stale stack
  garbage as results.
- **Fix:** alloc+core default `Recursable` (cb017d266). They are no-syscall crates by
  construction: their only extern edges are allocator shims (own classes) and diverging
  panics (NeverLift). `std` stays Leaf-by-default (syscall surface).
- **x86/Windows:** the classifier is arch-independent — this fix carries over directly.
  Port the WHOLE classify_for_name policy as-is, including:
  - allocator name family → BumpAlloc/BumpRealloc/BumpDealloc
  - memcpy/memmove/memset/snprintf (all spellings incl `_chk`, `_platform_`, and on
    Windows: `__imp_*` IAT thunk spellings!) → real helper-IR bodies
  - hashmap_random_keys → fixed-seed helper
  - diverging panic/error helpers → NeverLift (a Leaf stub RETURNS with an unrestored
    frame and corrupts the caller — see §5.2)
  - `core::ops::function::impls` (fn-ptr blanket impls) → keep Leaf; lifts to an
    `unreachable` trap (still un-root-caused; re-test on x86).

### 1.2 Leaf-stubbed helper inside a loop = HANG, not crash [PORTABLE]
- **Symptom:** solveLayout hangs with NO trap; suspected allsorts gsub/gpos lookup
  machinery; bypassed with AZ_SKIP_GSUB_GPOS for weeks.
- **Root cause:** same as 1.1 — a Leaf-stubbed core/alloc helper inside the lookup loop
  never advanced its state ⇒ infinite loop. After the classifier fix, all of gsub/gpos
  lifts and runs (+260 fns, exports 2053→2313), bypass deleted (efe936394).
- **Lesson:** "hang" and "garbage value" are the SAME bug class — a no-op'd helper either
  corrupts a value or fails to advance a loop. BTreeMap drop loops (`dying_next`), iterator
  walks (`scan_cursor += len_utf8()`), and search loops are the canonical victims.

### 1.3 memcpy via anonymous branch island [ARM, has x86 sibling]
- **Symptom (M12):** Box::new(352-byte struct) kept zero-init bytes; node_data.len=0.
- **Root cause:** Rust lowers big moves to out-of-line `bl _memcpy`; on the huge dylib the
  call goes through an ANONYMOUS branch island (no symbol at the target) → classifier
  defaulted Leaf → memcpy did nothing. Fixed by PLT-chasing islands to the real symbol +
  FnClass::LibcMemcpy emitting a real `@llvm.memmove` body.
- **x86/Windows:** no branch islands, but the SAME anonymity problem appears as: IAT
  thunks (`jmp [__imp_memcpy]`), tail-call shims, and /INCREMENTAL ILT stubs. The
  PLT-chase logic must learn PE patterns: `jmp qword ptr [rip+disp]` → IAT entry → import
  name. Also `__chkstk` (stack probing on Windows, called for frames >4KB!) needs a
  no-op-with-correct-semantics class — it WILL appear in big lifted fns.

---
## 2. Lifted-IR / optimizer bugs

### 2.1 mechanism A — EarlyCSE + remill's noalias metadata [PORTABLE]
- **Symptom:** two adjacent guest stores that must hold different values collapsed to one
  (a &str fat-pointer's two words got one value).
- **Root cause:** remill tags State-register vs guest-memory accesses as mutually-noalias
  (`!alias.scope`/`!noalias`) — sound on hardware (disjoint address spaces), UNSOUND on
  wasm where one linear memory holds both and 32-bit-truncated guest pointers can hit the
  State struct. EarlyCSE forwarded a register load across volatile guest stores.
- **Fix:** `strip_alias_scope_metadata()` on linked IR before opt (default-on).
- **x86/Windows:** identical issue if the runtime is wasm-hosted. If the x86 port targets
  NATIVE host execution instead of wasm, the address spaces are still one — keep the strip
  unless State provably can't alias guest memory.

### 2.2 "every instruction lifts correctly in isolation" proves NOTHING [METHOD]
The pipeline = byte-rewrites → batched in-process lift at SYNTHETIC addresses →
sub-name canonicalization → helper IR → metadata strip → opt → llc → wasm-ld → JS loader
→ dispatcher. An isolated remill-lift of original bytes at native addresses exercises step
2 only. Bugs lived in steps 1 (LDAPR rewrite), 3 (name collisions), 5 (alias strip), 7
(jump tables), 9-10 (classification, imports). Always test the PRODUCTION artifact; use
the isolated lift only as a control.

### 2.3 Jump-table devirtualization [ARM, x86 sibling guaranteed]
- **Symptom:** big enum-match fns (CssProperty::clone etc., ~73KB, 179 arms) → `br` →
  `__remill_jump` → mis-dispatch → OOB.
- **Fix:** exact jump-table decode in azul_remill.cpp (ForEachDevirtualizedTarget) fed by
  `--extra_data` carrying the .rodata offset tables at synth addresses; window-sweep
  fallback for small fns.
- **x86/Windows:** x86 jump tables look different: `lea rax,[rip+table]; movsxd rcx,
  dword [rax+rdx*4]; add rcx,rax; jmp rcx` (clang) or absolute-address tables (MSVC).
  The devirt pattern-matcher must learn both. Rust9x/MSVC codegen may also emit bounds
  checks differently. Budget real time here — this WILL bite again.

---
## 3. Decoder gaps (arch-specific by nature)

### 3.1 The class-A method [METHOD — portable]
Undecoded instructions lift to `__remill_error` stubs that LOOK like data corruption
downstream. Method that found them ALL: standalone-lift every suspect fn →
`grep -c __remill_error` → add the missing decoder to the fork. Run this proactively over
the WHOLE lift set on x86 day one: lift everything, histogram the error stubs, fix
decoders by frequency.

### 3.2 ARM decoders added to the fork (fschutt/remill) [ARM]
NEON ldp/stp Q-register (incl. post-index/writeback), single ldr q, STP_Q signed-offset,
LDRH encoding 0x5C3→0x3C3, NZCV flag decoding, LDAPR→LDAR byte-REWRITE (pre-lift, because
remill lacks RCpc; equivalence OK for single-threaded wasm).
- **x86 equivalents to expect:** SSE/AVX moves (movups/movaps/movdqu over 16B pairs — the
  x86 sibling of the Q-pair class!), `lock`-prefixed RMW ops (the LDAPR sibling — consider
  a lock-prefix-strip rewrite for single-threaded targets), `cmpxchg16b`, `rep movsb`
  (inlined memcpy! — semantics loop, MUST decode or rewrite), x87 if any old code paths,
  `cpuid`/`rdtsc` (classify NeverLift or fixed-value helpers).

### 3.3 Build-std flags that keep code liftable [ARM, x86 sibling needed]
aarch64 build: `-C target-feature=-lse,-rcpc,-rcpc2,-rcpc3` (no LSE atomics / RCpc),
`-aarch64-enable-ldst-opt=0` (no ldp/stp fusion into shapes remill fumbles),
`-enable-machine-outliner=never`, `-Z build-std-features=panic_immediate_abort`.
- **x86 siblings:** consider `-C target-feature=-avx,-avx2,-sse4.2,...` to pin the ISA to
  what the decoder set covers; disable the outliner identically; panic_immediate_abort
  identically. On Windows+rust9x check what the custom toolchain enables by default.

---
## 4. Runtime / memory-model bugs

### 4.1 macOS TLV (thread-locals) [ARM/macOS, Windows sibling is DIFFERENT]
- **Symptom:** `adrp x0,<tlv-desc>; ldr x8,[x0]; blr x8` fell into the dispatcher's
  unknown-drop → std read descriptor bytes as LocalKey state → panic_access_error.
- **Fix:** TlvRegion geometry in symbol_table + mirror-seed + thunk→AZ_TLV_MAGIC_PC
  rewrite + dispatcher computing `tls_base_synth + desc.offset`. TLS = plain statics
  (single-threaded wasm).
- **Windows x64 sibling:** TLS goes through `gs:[0x58]` (TEB → TLS array) + `_tls_index`.
  The lift will see `mov rax, qword ptr gs:[58h]` — needs either a segment-read intrinsic
  mapped to a synthetic TEB, or a byte-rewrite of the whole TLS access pattern to a fixed
  synth base. Plan it BEFORE first std::HashMap use (RandomState is TLS-adjacent).

### 4.2 Heap / synth-band collision [PORTABLE]
Bump-allocator base collided with the image's synthetic-address band after the dylib grew
(allocations stomped mirrored data → "TLV descriptors read as heap garbage"). Fix: bump
base moved (96→160MiB), but this is a STANDING CLASS: any image growth past the band
re-collides. Port lesson: derive the bump base FROM the symbol table's max synth address
+ guard margin at startup; never hardcode. Watch the `TLV: seeded` / band logs on relifts.

### 4.3 Stack discipline [PORTABLE]
- Guest SP lives in State; mini-wasms relocate SP into a private band; cross-module
  corruption appeared when a callee leaked guest SP (fixed by enforce_sp_preservation,
  12-reg wrap, then made default).
- Diverging fns MUST trap, not return (§1.1) — a returned unrestored frame corrupts the
  caller's SP-relative locals invisibly.
- **Windows:** shadow space (32 bytes) + `__chkstk` interact with frame setup; the
  SP-preservation wrapper must account for the home area.

### 4.4 sret / hidden-pointer returns [ARM, x86 sibling is THE highest-risk port area]
X8 carries the sret pointer on aarch64; multiple incidents (X8 stomped by helper calls,
Pcs::HiddenPtrReturn plumbing for layout-cb, caller-allocated dest buffers).
- **x86-64:** SysV puts sret in RDI (first arg, shifts everything); **Windows x64 puts it
  in RCX and the callee RETURNS the pointer in RAX**; structs ≤8 bytes return in RAX
  directly (different size threshold than SysV's 16!). EVERY HiddenPtrReturn site, the
  __az_call_indirect_layout4 shape, and the dispatcher's State seeding must be re-derived
  for Win64. Expect the "16-byte multi-word return drops a half" family to reappear here
  in Win64-flavored form (RAX:RDX pairs).

### 4.5 The mirror / page-translation seam [PORTABLE if wasm-hosted]
Per-page mirror pointer translation + 32KiB stack buffer; 16-byte accesses crossing a
mirror page seam were an early suspect class. Data mirroring is driven by precise
adrp+ldr/pair/unscaled scans — x86 sibling: RIP-relative `lea`/`mov` scans (much easier:
displacement is explicit in the instruction).

---
## 5. Things that LOOKED like lift bugs but weren't

### 5.1 Vacuous experiments (see §0) [METHOD]
### 5.2 fmaxf/fminf/roundf JS stubs returning 0 [PORTABLE — wasm-host]
M12.7: every `.max(0.0)` floor zeroed every width — LLVM lowers f32::max to `fmaxf`
LIBCALLS, and the JS import env stubbed them to ()=>0. Real implementations in the
loader's import object. x86: same libcalls appear; same fix; audit the WHOLE import
object against silent ()=>0 defaults.
### 5.3 Quirks mode in the verification harness [METHOD]
Chrome `data:` URL references render in QUIRKS mode without `<!DOCTYPE html>` — quirks
swallows the first block child's top margin (a 13.4px phantom diff that looked like a
ua_css margin-collapse bug). Always DOCTYPE the reference page (scripts/cdp_uacss.js).
### 5.4 Heisenbug probes [METHOD]
Runtime address-observation (build_get, marker reads) repeatedly SHIFTED or hid the
corruption (M12.5x). When a probe changes the result, switch to: constant-injection
bisection (marker-free), transpiler-side logging, or the native harness (§6.1) — never
keep stacking in-guest probes.

---
## 6. The diagnostic toolkit (all portable; port these FIRST)

### 6.1 scripts/mechb_harness/ — native executor for lifted fns [THE unlock]
Lift the exact bytes standalone → compile the .ll NATIVELY on the host with a ~200-line
C++ harness (State offsets read at runtime via probe.ll; logged memory intrinsics; PC-keyed
callee stubs) → one run