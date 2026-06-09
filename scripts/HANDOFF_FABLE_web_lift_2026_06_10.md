# Handoff → Claude Fable: finish the azul web-lift backend

**Branch:** `web-lift-text-layout`  ·  **Last commit:** `c0861ee07`  ·  **Date:** 2026-06-10
**Predecessor:** Claude Opus 4.8 (1M ctx). Full investigation log: `scripts/HANDOFF_web_helloworld_NEXT_2026_06_06.md` (g1–g223 trail — dense, read on demand, not front-to-back).

---

## 1. What this is

azul has an experimental **web backend** that takes a compiled native aarch64 dylib
(`libazul.dylib`) and **lifts it to wasm** via a fork of trail-of-bits **remill**, so the
*real* layout/text/cascade engine runs in the browser (not a reimplementation). The
transpiler lives in `dll/src/web/` (Rust) + `dll/src/web/cpp/azul_remill.cpp` (LLVM-17
opt+codegen+lld-link shim).

**Status: it works.** `examples/c/hello-world.c` lays out, renders its counter label,
and handles clicks **end-to-end on the lifted web backend**. `examples/c/web-nested-text.c`
(body > div > "Hello") lays out. Text shaping, font parsing, the SwissTable caches, the
flexbox/block/inline solver — all run lifted.

**Your job:** delete the remaining **azul-source workarounds** by fixing the *one*
underlying transpiler/remill bug they all work around. The workarounds are `#[cfg(feature
= "web_lift")]`-gated and ugly; native builds never see them. The constraint (inherited,
**do not break**): **the fix belongs in the transpiler (`dll/src/web/`) or the remill fork
(`/Users/fschutt/Development/azul/third_party/remill`), NEVER in azul source.** azul source
only gets workarounds *removed*, never new ones added.

Other standing rules: **commit only when the user asks.** **Watch disk first** (`df -h /`;
the lift writes GBs of scratch). Work **analysis-first** — a full relift is ~15–30 min, so
exhaust static IR/asm analysis before spending one.

---

## 2. The mental model you must have: two mis-lift classes

Everything hinges on distinguishing these. The previous session conflated them for ~25
iterations; keep them separate.

### Class A — loop-bound `Vec::len()` SROA → reads 0  ✅ **FIXED**
*Symptom (historical):* in optimized lifted Rust, a `Vec::len()` read used as a loop bound
read 0 even when the Vec was non-empty → loops ran 0× → empty content → nested text never
laid out.
*Root cause:* remill **decoder STUBS** (`return false`) for several AArch64 **NEON** instrs
silently truncated a function's CFG recovery (blocks after the bad instr never lifted) →
callers read garbage. Plus an over-aggressive optimizer load-forward.
*Fix (already landed):* (a) implemented the missing NEON decoders in the remill fork
(FNEG.2s, FMUL-by-elem, UCVTF scalar, FNMUL scalar — see the
`web_vec_len_mislift_systemic_2026_06_06` memory); (b) **all guest memory loads now emit
`load volatile i64`** → LLVM cannot SROA/forward them. **This class is structurally closed.**
Do not re-investigate "Vec-len SROAs to 0" — it's fixed and proven.

### Class B — multi-word Vec/struct **RETURN** via X8/sret  ❌ **OPEN — your target**
*Symptom:* a function that **returns** a `Vec`/large struct **by value** (AArch64 sret: the
caller passes a hidden destination pointer in **X8**, the callee writes the struct through
it) reads **garbage** at the call site (e.g. `Vec.len` = 161104 / 171120 — pointer-ish
values, not a real length) → glyph-grouping overflow → `SmallVec::extend` panic / OOB.
*Why it's different from A:* A is about *loads inside a loop*; B is about the *return-value
ABI across an internal call*. Volatile loads (the A fix) don't touch it because the bug is
in **how X8/the sret destination is routed**, not in the load.
*Workaround in place:* every hot Vec-returning function was rewritten to an **out-param**
(`fn f(…, out: &mut Vec<T>) -> Result<()>` instead of `-> Result<Vec<T>>`). Out-params fill
a caller-provided buffer (no multi-word struct return) and lift cleanly. **These are the
hacks to delete.**

**Fixing class B is the whole task.** One transpiler/remill fix → all the out-param hacks
in §4.A1 come out.

---

## 3. Architecture facts you'll need

- **remill State struct**: every guest register is a field in a `State` struct threaded as
  the first arg (`ptr %state`) to every lifted function. Byte offsets (these are load-bearing):
  `X0=544, X1=560, X2=576, X3=592, X8=672, X19=848 … X30=1024, SP=1040, PC=1056`.
  A lifted `mov x8, …` is `store …, ptr (state+672)`; a `str xN,[x8]` reads `state+672`.
- **X8 = the AArch64 Indirect Result Location Register (sret).** Caller sets `state+672` =
  dest ptr **before** the call; callee reads it at entry, writes the return struct through it,
  and **is free to clobber X8 as scratch afterward** (it's caller-saved). That "reads-once-
  then-reuses-as-scratch" pattern is the crux — see §5.
- **Top-level sret is wired** (`Pcs::HiddenPtrReturn { x8_offset: 672 }` in
  `transpiler_remill.rs` ~line 423): the LayoutCallback wrapper appends a trailing `i32
  out_ptr` arg and seeds `state+672` from it. **Internal `sub_<hex>` calls** share the same
  `%state`, so X8 *should* thread through — but the bug suggests it doesn't survive opt for
  internal sret callees.
- **Opt pipeline**: `azul_remill.cpp` runs `PB.buildPerModuleDefaultPipeline(Oz)` (line
  ~754) over the llvm-link'd module, **using vcpkg LLVM 17** (`build.rs:128`). Override with
  env `AZ_OPT_LEVEL=O1|O0|O2` (there's already a hook; the team noted "O1 may preserve
  state-via-register" at line ~744).
- **Existing textual IR post-processing passes** (precedent for your fix):
  `strip_noalias_from_sub_args` and `tag_state_accesses` in `transpiler_remill.rs` (~6985 /
  ~7008) rewrite the lifted IR text before handing it to the C++ opt. A class-B fix may well
  be a third such pass (e.g. force the caller's X8-setup store volatile, or inject the
  callee's entry X8-capture), *or* a remill semantic/decoder fix.

---

## 4. The workaround catalog (what to delete, by class)

Grep is the source of truth — line numbers drift. Find them all with:
`grep -rn "az-web-lift\|\[g[0-9]" layout/src/ | grep -iE "out.?param|bypass|force_ifc|sret|repr\(C"`

### A. Out-param hacks — **class B, your primary deletion target** (one fix removes all)
| Function | Where (grep marker) | Original signature |
|---|---|---|
| `ParsedFontTrait::shape_text` | `g127` — font_traits.rs, cache.rs, default.rs (×4), font.rs:2280; callers in cache.rs | `-> Result<Vec<Glyph>>` |
| `create_logical_items` | cache.rs (~5995/6027), window.rs caller | `-> Vec<LogicalItem>` |
| `collect_inline_content` | `g78` — sizing.rs (~1049/1329) | `-> Result<Vec<InlineContent>>` |
| `collect_and_measure` | `g129/g130` — fc.rs (~2529/6494/6531) | `-> Result<(Vec, HashMap)>` |
| `&Vec` not `&[InlineContent]` | `g128` — cache.rs (~5627) | fat-slice `len` also mis-lifts |

### B. HashMap-cache bypasses — **EMPTY_GROUP class, NOW likely deletable** (task #7)
Same SwissTable `static_empty` root that `c0861ee07` (g213) just fixed. **Verify with a
relift that they no longer hang, then delete:**
- `g115` bypass `self.logical_items` (cache.rs ~5861)
- `g118` bypass `visual_items` + `shaped_items` (cache.rs ~5903/5917)
- `g120` bypass `per_item_cache` (cache.rs ~6582)

### C. `g122` — `BTreeMap::get()` (FontChainKey `Ord::cmp`) mis-lift (cache.rs ~6952)
**Different class:** a jump-table/devirtualization mis-lift of the `Ord::cmp` comparator
(same family as the historic CssProperty 179-variant jump-table). Real fix = remill devirt.

### D. `force_ifc` — `FormattingContext` field-clone mis-lift (fc.rs ~388–409)
The cloned `formatting_context` field reads **garbage** after the reconcile clone → the
`match node.formatting_context` falls through to block layout. Workaround recomputes the
IFC decision from `styled_dom.has_only_inline_children`. **Class: VALUE/struct-field clone
mis-lift** — distinct from A/B/C.

### E. `repr(C, u8)` enum guards (cache.rs ~969/1373/1885/3698/3820) — **probably keepers**
`#[repr(C, u8)]` (was `repr(Rust)`) on text3 enums fixes a discriminant/niche mis-read.
`repr(C)` is a legitimate stable-ABI choice; low priority. If you fix remill's niche/
discriminant handling you *could* revert these, but they're harmless.

### F. Diagnostic scaffolding (revert at the end, not workarounds)
`g147*` / `g150` / `g132–g135` markers write constants to fixed addresses `0x60xxx` (read
by `scripts/m9_e2e/layout-flexbox.js` POST-TRAP dumps). Plus hard iteration caps (`g147`,
cache.rs ~5790/7973) — defensive, keep until the underlying loops are proven bounded.

---

## 5. Class-B: the analysis so far, and the staged experiment

**Where the previous session got to (and a trap it fell into):**

- Inspecting the lifted IR of an *already-out-param'd* function (`collect_inline_content`,
  `sub_ad3744`) showed it uses X8 (`state+672`) purely as **scratch**: **0 loads, 65
  stores**, first touch is a store. For an sret function that would be a smoking gun (it
  must *read* X8 at entry to find the dest) — **but that function is the out-param version,
  so it legitimately doesn't sret.** It does **not** witness the bug. Every Vec-returning fn
  in the tree is already out-param'd, so **no in-tree function witnesses class B.** Static
  analysis of the current tree is therefore exhausted; you need a deliberate sret witness.

- ⚠️ **`initializes` is a RED HERRING — do not chase it.** The saved `/tmp/*.opt.ll` files
  carry an `initializes((672,680),…)` attribute on the State param, which looks like it
  could DCE the caller's X8-setup store. **It does not apply:** those files were optimized by
  a stray homebrew `opt-20/21` during analysis; the *real* pipeline is **LLVM 17**
  (`build.rs:128`), which predates the `initializes` attribute (LLVM 19+). The real
  pipeline never emits or exploits it. Don't implement an `initializes`-stripping pass.

**The staged witness (paste this in, it's the minimal class-B repro):**
A generic fn calling a trait method that returns a `Vec` by value — the exact class-B
mechanism, isolated to ~10 lines so the lifted IR is readable. Add to
`layout/src/text3/cache.rs` (it was removed from the committed tree to keep it known-good):

```rust
#[cfg(feature = "web_lift")]
trait AzSretMaker { fn az_make(&self) -> Vec<u64>; }
#[cfg(feature = "web_lift")]
impl AzSretMaker for u32 {
    #[inline(never)]
    fn az_make(&self) -> Vec<u64> { vec![10, 20, 30, 40, 50, 60, 70] } // len = 7
}
#[cfg(feature = "web_lift")]
#[inline(never)]
fn az_sret_probe<T: AzSretMaker>(x: &T) {
    let v = x.az_make(); // trait-method sret-of-Vec RETURN (the class-B mechanism)
    unsafe {
        core::ptr::write_volatile(0x60910 as *mut u32, v.len() as u32);            // expect 7; garbage = class B
        core::ptr::write_volatile(0x60914 as *mut u32, v.as_ptr() as usize as u32);
        core::ptr::write_volatile(0x60918 as *mut u32, 0xC0DE0224u32);             // reached
    }
    core::hint::black_box(&v);
}
```
Call it from a lifted path, e.g. in `layout_flow` near the existing `0x60BD0` diagnostic:
`#[cfg(feature = "web_lift")] az_sret_probe(&7u32);`. Read the markers in the harness (there's
already a `const D = a => mini.AzStartup_peekU32(a) >>> 0;` helper — add a peek for 0x60918
[expect `0xC0DE0224` = reached], 0x60910 [expect 7; garbage = class B reproduced]).

**Two ways to use the witness (cheapest first):**
1. **Standalone lift, no full relift.** Build the dylib (§6), then lift `az_make` +
   `az_sret_probe` *standalone* through the real LLVM-17 transpiler and read the IR. Check:
   does `az_sret_probe` emit `store <dest>, ptr (state+672)` before `call @sub_az_make`?
   Does `sub_az_make` **load** `state+672` at entry to find the dest, or clobber it first?
   Does LLVM-17-Oz keep both? (This is the proven method that cracked class A — see the
   `web_vec_len_mislift_systemic_2026_06_06` memory: "lift fn standalone → grep `call ptr
   @__remill_error` → ID instr in objdump → add decoder".)
2. **Full relift + run** to see the runtime garbage symptom (§6).

**Working hypotheses for the X8/sret bug (test, don't assume):**
- (H1, most likely given class A's precedent) remill **mis-lifts or drops the callee's entry
  instruction that captures X8** (e.g. an `mov x19, x8` / `str x8,[sp,#…]` save) — a decoder
  or CFG-recovery gap, like the NEON stubs. → the callee never reads its sret dest.
- (H2) LLVM-17 Oz **DCE's the caller's `store …, ptr (state+672)`** because nothing local
  reads it back and the callee is opaque. → fix = make that store volatile (a textual pass
  like `strip_noalias_from_sub_args`), or `AZ_OPT_LEVEL=O1`.
- (H3) remill's `bl`→`sub_` lowering doesn't thread X8 through the shared `%state` for
  internal sret calls (only the top-level `HiddenPtrReturn` wrapper handles X8).

Bisect H2 fast: re-run the witness with `AZ_OPT_LEVEL=O0` — if the garbage disappears at O0
but returns at Oz, it's an optimizer transform (H2), not a decoder gap (H1).

---

## 6. Build · relift · disk hygiene (runnable)

```bash
# 0. DISK FIRST. The lift writes GBs of scratch.
df -h /
rm -rf /var/folders/5x/*/T/azul-web-transpiler-*      # purge old scratch
ps aux | grep -E 'remill-lift|\.bin' | grep -v grep   # kill orphans before relift

# 1. Native type-check (cheap, host target — validates azul source + web_lift cfg):
cargo check -p azul-layout --features web_lift
#   NOTE: rust-analyzer shows STALE "expected 5 args found 6" errors on the g127
#   out-param sites — IGNORE them. `cargo check` is authoritative ("Finished" = clean).

# 2. Build the lifted dylib (build-std; ~70s incremental, minutes clean):
RUSTC_BOOTSTRAP=1 RUSTFLAGS="-C target-feature=-lse,-rcpc,-rcpc2,-rcpc3 -C llvm-args=-aarch64-enable-ldst-opt=0 -C llvm-args=-enable-machine-outliner=never" \
  cargo build -p azul-dll --release --features "build-dll web web-transpiler web-transpiler-static" --no-default-features \
  -Z build-std=std,panic_abort -Z build-std-features=panic_immediate_abort --target aarch64-apple-darwin
#   build-std + -lse/-rcpc* recompiles std without LSE/RCPC atomics remill can't lift.
#   -ldst-opt=0 / outliner=never keep the lifted code in shapes remill handles.

# 3. Relift an example + wait for the wasm port (run in BACKGROUND; ~15–30 min):
#    Poll the PORT (8800), never the pid. AZ_NO_LIFT_CACHE=1 forces a fresh lift.
bash scripts/web_relift.sh examples/c/hello-world.bin /tmp/server_hw.log   # run_in_background:true

# 4. Drive the harness + read markers:
AZ_LENIENT=1 node scripts/m9_e2e/layout-flexbox.js
```

**Diagnostic env toggles (default OFF; all in `transpiler_remill.rs`):**
`AZ_FUEL=ALL` (trap loops, record GID@0x40070 — note: its per-terminator volatile writes act
as optimization BARRIERS, so a bug that "vanishes" under AZ_FUEL is optimizer-sensitive),
`AZ_READ_TRACE` (ctrl-group `load volatile double` ring @0xE0000), `AZ_REG_TRACE` (GPR-store
ring @0xF0000, decoded by the g205 block in layout-flexbox.js), `AZ_WRITE_TRACE`,
`AZ_EMPTY_GROUP_TRACE` (EMPTY_GROUP mirror native/synth/trunc per run), `AZ_OPT_LEVEL=O0|O1|O2`.

---

## 7. Suggested order of attack
1. **Confirm task #7 is free** (cheap win): relift with the `g115/g118/g120` bypasses
   deleted — the g213 EMPTY_GROUP fix should make `entry()`/`get()` lift without hanging.
   If it lays out, delete those bypasses + commit (when the user asks).
2. **Crack class B** with the §5 witness (standalone-lift first; bisect H1/H2/H3). The fix
   removes **all** of §4.A. This is the headline.
3. **Then** the longer tail: `g122` BTreeMap devirt (C), `force_ifc` field-clone (D), maybe
   revert the `repr(C,u8)` guards (E). Each is its own remill semantic gap.
4. **Last:** strip the `g147*`/`g150`/etc. diagnostic scaffolding (F) and the staged witness.

Good luck. The hard infra (NEON decoders, build-std atomics, EMPTY_GROUP mirror, snprintf,
font resolution, the whole lift+link+harness loop) is **done and working** — you're hunting
a small number of specific remill/transpiler semantic gaps, with a proven method and tools.
