# HANDOFF → NEXT AGENT: get `hello_world.c` working on the azul **web (lifted) backend**

**Date:** 2026-06-06 · **Branch:** `mobile-ios-android` · **Repo:** `/Users/fschutt/Development/azul-mobile`
**Remill fork:** `/Users/fschutt/Development/azul/third_party/remill`
**Goal:** `examples/c/hello-world.c` (a styled button + counter) renders and reacts on the web backend
(native ARM64 `libazul.dylib` → remill-lifted to `azul-mini.wasm`; layout/shaping run in wasm, no GPU).

This is the clean, actionable handoff. The blow-by-blow evidence is in the chronological log
`scripts/HANDOFF_web_vec_return_len_mislift_2026_06_06.md` (554 lines, sections g129→g139). Memory:
`~/.claude/.../memory/web_vec_len_mislift_systemic_2026_06_06.md` + `web_flexbox_lift_2026_06_01.md`.

---

## 2026-06-06 g145 — hello-world.c relift: lift COMPLETE + cascade OK; new (non-truncation) layout blocker

Ran `hello-world.c` on the 4-NEON-fix remill (`/tmp/cycle_hello.sh`; server `/tmp/server_hello.log`,
harness `/tmp/cycle_hello.log`). **Two big confirmations:**
- ✅ **`[diag] __remill_error count = 0`** — hello-world's lift is ALSO complete; the 4 fixes (FNEG/FMUL-
  elem/UCVTF/FNMUL) cover it, NO new undecoded NEON instrs. (`missing_block=17` = benign indirect-dispatch.)
- ✅ **Cascade works**: `[1] cascade ok: styled_dom node_count=5 (expected 5)`, button styled
  (`button.style.rules=1`, `button.children=1`). The old "cascade CssProperty jump-table" fear is resolved.
- ✅ **Block layout partially runs**: `[lenient] rects(5): (0,0,784,21) (0,0,784,0) MAX (8,16,784,13) MAX`
  — body = 784×**21** (one text line of height!), an element positioned at (8,16,784,13); the 2 text
  nodes are u32::MAX (EXPECTED for inline text, as in web-text-min).

**The remaining blocker (NEW, NOT the truncation class — lift is complete):** inline text isn't fully
collected/positioned (`[g132] overflow_size=0 Phase-4 not reached`, `[g133] collect not reached`,
`[g136] dom_children.len=0 last-seq=0x0` = those markers UNSET this run), and an "InvalidTree in
reconcile_and_invalidate" is reported — BUT on the UNRELIABLE dense `0x40704`/`0x4071C` band (handoff §0:
`0x40xxx` reads are spurious 0), while REAL rects DID compute (so it's not a total reconcile failure). The
reliable free-band `0x6071C=0` says collect_inline_content_recursive wasn't reached. Net: hello-world's
deeper body→button→[label,counter] tree lays out the BLOCK boxes but the inline text inside isn't measured
into a line box (body got height 21 from somewhere, so SOME text path ran). A `[cb-dom-azstring]` probe
showed the Text node's AzString header as `{ptr=0,len=100677224}` "pre-cascade" — but that is LIKELY a
PROBE-OFFSET error (the NodeType hexdump shows disc byte `0xb1`=177=Text + a heap ptr `0x6003668` at +16,
and "Hello" is in memory at 0x13f79), consistent with prior probe-offset bugs — do NOT trust it without
re-deriving the AzString offset.

**★ `curl 127.0.0.1:8800/` shows hello-world RENDERS the full UI (server-side initial render):**
`<div id="az_0"><div id="az_1">5</div><button id="az_3" class="__azul-native-button __azul-btn-primary">Increase counter</button></div>`
with correct cascaded CSS (`#az_1` font-size 32px = counter **"5"** — the feared `__snprintf_chk`-empty-
counter did NOT happen here; `#az_3` = button w/ padding + 1px `#c8c8c8` border + `cursor:pointer`). So the
NATIVE DOM-build + cascade + styling pipeline is CORRECT end-to-end. The remaining gap is purely the
WASM-side (lifted) layout that the harness exercises (`mini.wasm` + `layout.wasm` are preloaded client-side
for hydration/relayout/events) — block boxes lay out but the nested inline text isn't fully positioned in
the lifted layout, and click-reactivity (lifted event dispatch) is unverified.

**★ REFINED via RELIABLE free-band markers (offline, from the same hello-world harness run):**
- `postReconcile(0x60740)=3`, `sizingEntry(0x607B0)=3`, `g70 at-clone/heap all=3`, `get(root).is_some=1`
  → **reconcile builds a valid 3-node LayoutTree and it survives into sizing.** The "InvalidTree in
  reconcile" was the `0x40xxx` PHANTOM (debunked again). Block layout runs (rects produced; body 784×21).
- BUT `[g135] collect_and_measure _impl tree.nodes.len(0x606A8)=0`, `tree.get(idx).is_some=0`,
  `last-passed-? = Err AT 6449 tree.get(first)` → **`collect_and_measure` (the inline-text POSITIONING
  pass) receives an EMPTY tree (nodes.len=0) and Errs immediately**, even though reconcile/sizing hold the
  3-node tree. `0x6071C=0` (collect_inline not reached) is consistent.
- ⇒ **The blocker is a TREE-REFERENCE mis-pass to `collect_and_measure` for hello-world's NESTED IFC**
  (text inside the button/counter divs), NOT present for web-text-min's flat body→text IFC. `__remill_error`
  =0, so it's NOT a decode truncation — it's a `&tree`/`&mut tree` reference passed empty/garbage across the
  `layout_ifc`→`collect_and_measure` (or `layout_formatting_context`→`layout_ifc`) call for a CHILD IFC.
  This is the **g56 stack-address/reference class** (`&stack_local` → 0x0/empty across a lifted call; g56
  fixed one via `Box::new`). web-text-min's single top-level IFC dodges it; the nested per-child IFC hits it.

**NEXT for hello-world (concrete, in priority order):**
1. ⭐ Root the empty-tree: the `tree` ref reaching `collect_and_measure` for a CHILD IFC is empty while the
   caller's is 3-node. Lift `layout_ifc` / `layout_formatting_context` / `calculate_layout_for_subtree`
   standalone (`__remill_error`=0 so no truncation — look at how `&tree`/`&mut tree` is threaded to the
   child-IFC `collect_and_measure` call) OR add a RELIABLE `0x60xxx` marker capturing `tree.nodes.len` at
   the `layout_ifc` ENTRY for the child IFC vs inside `collect_and_measure` (g135 already shows the callee
   side =0; need the caller side). If caller=3 callee=0 → the `&tree` arg mis-lifts for this call → fix like
   g56 (Box/heap the tree ref, or out-param). NOTE web-text-min's collect_and_measure saw tree.nodes.len=2
   correctly — so the per-CHILD-IFC call path differs from the top-level one.
2. Then §6: `__snprintf_chk` counter (NOTE: the SERVER-rendered HTML already shows "5", so the native path
   is fine; verify the LIFTED counter rebuild on click), click/dispatchEvent.
3. The 6 out-param workarounds + g137/g139 fc.rs loop rewrites are candidates to REVERT now (lift complete).

---

## ★★★★ 2026-06-06 g144 — ✅ WEB-TEXT-MIN TEXT POSITIONS (root cause fixed, lift is COMPLETE) ★★★★

**`web-text-min.c` ("Hello") now COLLECTS → SHAPES (5 glyphs) → POSITIONS.** Harness:
`[g132 lays-out] overflow_size = 39.10 x 20.05 ✓✓✓ TEXT LAYS OUT (h>0)`, `[g133] collect=Ok, layout_flow=Ok`,
and decisively **`[diag] __remill_error count = 0`** (was 21). The text node's own rect stays `u32::MAX` —
EXPECTED (solver3 keeps plain inline text in the body's `inline_layout_result`, not a separate node rect;
`overflow_size>0` is the positioning proof; body n0 = 800×600).

**THE FIX = 4 missing AArch64 NEON decoders implemented in the remill fork** (all UNCOMMITTED, dated
`2026-06-06`; same recipe as the committed FMUL-by-element fix — decoder in `Arch.cpp`, stub deleted from
`Decode.cpp`, semantic+`DEF_ISEL` in `SIMD.cpp`/`BINARY.cpp`/`CONVERT.cpp`):
1. **FNEG vector** `fneg.2s` (`FNEG_ASIMDMISC_R`) — truncated `collect_and_measure` (dropped the whole
   DOM-children loop). Arch.cpp + SIMD.cpp (`MAKE_FP_VEC_NEG`).
2. **FMUL scalar-by-element** `fmul s,s,v[i]` (`FMUL_ASISDELEM_R_SD`) — truncated `perform_fragment_layout`.
   Arch.cpp + SIMD.cpp (`FMUL_ELTSCALAR_S/D`).
3. **UCVTF scalar** `ucvtf s,s` (`UCVTF_ASISDMISC_R`) — same fn. Arch.cpp + CONVERT.cpp (`UCVTF_Scalar32/64`).
4. **FNMUL scalar** `fnmul s,s,s` (`FNMUL_S/D_FLOATDP2`) — same fn. Arch.cpp + BINARY.cpp (`FNMUL_Scalar32/64`).

Each was found by the **proven method**: lift the function standalone
(`remill-lift-17 --bytes <hex> --address <nativeAddr>`), grep for `call ptr @__remill_error`, read the
block BEFORE it to get the last-decoded instruction, find that instruction in `objdump -d`, the NEXT
instruction is the undecoded one → implement decoder+semantic from the FMUL/CVTF/FNEG template → rebuild
remill (`ninja remill-lift-17 lib/Arch/AArch64/Runtime/aarch64.bc` + cp `aarch64.bc` to install) → relift.
**Efficiency win (`/tmp/scan2.sh`):** to avoid one-relift-per-instruction, extract each function's distinct
FP/SIMD instruction words from `objdump`, lift EACH standalone, flag those that emit `__remill_error` —
finds ALL of a function's undecoded instructions in one offline pass. Scanned `perform_fragment_layout`,
`layout_flow`, `apply_text_orientation`, `collect_and_measure_impl` → confirmed the 4 above are the
complete set for the text-positioning path (after which `__remill_error` = 0 globally).

**NEXT = hello-world.c** (the actual goal). It shares the layout code (so the 4 fixes apply) but exercises
EXTRA paths (button cascade/`CssProperty`, auto-height, `__snprintf_chk` counter, click/dispatch — handoff
§6) that web-text-min doesn't, so expect a few MORE undecoded NEON instrs in the button/cascade functions
+ the §6 blockers. Use the SAME scan method on hello-world's path functions. The 6 out-param workarounds +
g137/g139 loop rewrites in fc.rs are likely now REVERTABLE (they patched truncated/garbage lifts; with the
lift complete they may be unnecessary — verify by reverting one at a time + relift). Build/run recipe in §5
(swap `web-text-min.c` → `hello-world.c`). Remill rebuild recipe above.

---

## ★★★ 2026-06-06 g140–g143 — ROOT CAUSE FOUND (OVERTURNS the "systemic value mis-lift" theory) ★★★

**The g129–g139 conclusion ("systemic optimized-code value/control mis-lift; only the transpiler/remill
optimized-code fix can help; source workarounds can't touch std") was WRONG.** The actual root cause is
UN-DECODED NEON INSTRUCTIONS that truncate the lift (the FNEG one below was the first of 4 — see g144):

> **remill cannot decode `fneg.2s` (FNEG vector, `FNEG_ASIMDMISC_R` — a `return false` stub in
> `Decode.cpp`). When remill's CFG recovery hits it (at native `0x2710d0` in `collect_and_measure`, in the
> loop PREHEADER, building an FP sentinel via `mvni.2s; fneg.2s`), it emits `__remill_error` and STOPS
> recovering that path. Every block after it — including the entire DOM-children loop body (`b 0x271168`
> → the body at `0x271168`+ that collects+measures the text) — is SILENTLY never lifted. At runtime the
> lifted code reaches the unlifted region (a `__remill_missing_block` no-ops + returns, rc=0, no trap), so
> the loop body's markers never run → "the loop iterates 0 times" / `dom_children.len` reads 1 but the
> loop is empty.**

**How it was proven (g140–g142), each step ruling out the old theory:**
- **g140 (opt-level=1 on azul-layout/core/css): NET REGRESSION.** Different native code hit NEW unlifted
  jump-tables (missing_block 0→39), broke font parse + shaping. Reverted. (Cargo.toml note kept.) ⇒ not opt.
- **The loop guard is CORRECT at every level.** Disasm: `cbz w9, 0x272700` (`0x271054`). Standalone remill
  lift (`remill-lift-17 --bytes` of the whole fn): the `icmp eq W9,0` + branch are faithful (with the
  read-back len=1 it enters the loop). The transpiled `.opt.ll` (KEEP_SCRATCH): same — `%v.i2499 = load
  i32` from `0x606B4` (opt did NOT mis-forward; the intervening guest store to `0x606A4` may-alias under
  the shared `az_guest` scope), `icmp eq → br` to the loop-enter block. `llc → wasm`: same (`i64.ne; br_if`
  to label129). So remill-IR ✓, opt-O2 ✓, llc-wasm ✓ — the guard enters the loop with len=1.
- **The loop BODY's markers are ABSENT from the lifted IR** (raw `.lifted.ll`, `.patched.ll`, `.opt.ll`,
  AND my standalone lift): `0x6896`(26774, in-loop seq) = 0 occurrences; `0x606B8` node-type marker = 0.
  Machine code HAS them (3 copies). **Store count: 76 `str w` in `.text` vs only 57 `write_memory_32` in
  the lift — 19 stores never lifted.** ⇒ a whole region is silently dropped, NOT a value/forwarding bug.
- **g142 (per-fn `opt -O0` via `AZ_LOWOPT_FNS=collect_and_measure...`): FAILED identically.** ⇒ not opt —
  the body was never in the IR for any opt level to keep.
- **The bail site:** in the fresh standalone lift, the block right after `mvni.2s` (`0x2710cc`) does
  `store i32 32, %state; tail call @__remill_error; ret`. The next PC is `0x2710d0` = `fneg.2s v0,v0`
  (`0x2ea0f800`). Recovery never reached the `b 0x271168` (`0x2710e8`) that enters the loop body. g129's
  own note already flagged `TryDecodeFNEG_ASIMDMISC_R` as an unimplemented stub ("part of err=21").

**THE FIX (implemented this session, UNCOMMITTED, in the remill fork — same recipe as the committed
FMUL-by-element fix):**
- `lib/Arch/AArch64/Arch.cpp`: real `TryDecodeFNEG_ASIMDMISC_R` (mirrors `TryDecodeCVTF_ASIMDMISC`:
  `sz=data.size&1`, `Q=data.Q`, `AddArrangementSpecifier` → `_2S/_4S/_2D`, Rd write + Rn read).
- `lib/Arch/AArch64/Decode.cpp`: deleted the `return false` stub (real decoder now in Arch.cpp).
- `lib/Arch/AArch64/Semantics/SIMD.cpp`: `MAKE_FP_VEC_NEG` macro (unary negate per lane, mirrors scalar
  `FNEG_S/D`) + `FNEG_VEC_2S/4S/2D` + `DEF_ISEL(FNEG_ASIMDMISC_R_2S/4S/2D)`.
- Rebuild: `ninja remill-lift-17 lib/Arch/AArch64/Runtime/aarch64.bc` + cp `aarch64.bc` to install share.
- **Verification IN FLIGHT** (`/tmp/cycle_fneg.sh`, log `/tmp/cycle_fneg.log`): rebuild remill → relift
  web-text-min (opt-3 dylib unchanged) → harness. EXPECT: loop body now lifted → `g136` loop ENTERS
  (node_type=Text, content.len=1) → `g132 lays-out overflow_size.height>0` = TEXT POSITIONS.

**IMPLICATIONS (big):** the "Vec-return `len` mis-lift", "sret/NRVO mis-transfer", "iterator `next()`
iterates 0×", "SROA'd len reads 0" symptoms were ALL most likely downstream of TRUNCATED LIFTS — a fn that
hits an undecoded NEON instr returns garbage/incomplete, so its caller reads a wrong `len`/Vec. The real
program is: **decode every NEON instruction remill is missing** (baseline `__remill_error count=21` across
all lifted fns — FNEG is one; expect a few more, each a `return false` stub findable the same way: relift,
find the `__remill_error` block, read the PC's instruction in the disasm, implement via the FMUL/CVTF
template). Once the lift is COMPLETE, the 6 out-param workarounds + g137/g139 loop rewrites may be
revertable (they were patching around truncated/garbage lifts). **If the fneg fix makes text position, the
next move is to find+decode the remaining undecoded NEON instrs (not more source workarounds).**

---

## 0. TL;DR (PRE-g143 — superseded by the section above; kept for context)

- The headline "Vec-return `len` mis-lift" (the 1.6 GB OOB) is **FIXED** for `collect_and_measure` (out-param).
  Text now **shapes (5 glyphs) + measures** for `web-text-min.c`. `rc=0`, no trap.
- "InvalidTree" was a **PHANTOM** (harness misread a never-written slot) — do NOT chase it.
- **The real blocker for hello_world.c is a SYSTEMIC lift-fidelity failure in OPTIMIZED Rust code:**
  SROA'd `Vec::len()` reads return 0 (vs 1 via a volatile read), sret/NRVO aggregate returns mis-transfer,
  and **`for`-loops over ranges/iterators iterate 0 times** (the iterator `next()` mis-lifts). Proven NOT
  fixable by source rewrites (it hits `std::collect`, `std` Range/slice iterators — un-out-param-able).
- ⇒ **The single fix that unblocks everything (text positioning AND hello_world.c) is the
  transpiler/remill lift-fidelity fix.** Stop per-site source workarounds; they don't generalize.

---

## 1. The systemic root cause (READ THIS FIRST — it's THE blocker)

When the native code is **optimized** (`--release`, SROA + iterator inlining), the remill lift mis-tracks
register/SSA values and control flow at RUNTIME. The static lifted IR usually looks *correct*; the
**execution** is wrong. Concretely observed this session:

| symptom | site | proof |
|---|---|---|
| `Vec::len()` reads **0** in a loop range but **1** via a volatile read of the same Vec | `dom_children` in `collect_and_measure_inline_content_impl`, fc.rs ~6896 | g137/g138/g139: marker `0x606B4`=1, loop `0..len` empty |
| `for x in vec.iter().enumerate()` iterates **0 times** despite `len()==1` | same | g136 |
| sret / NRVO aggregate (`Vec`,`(Vec,HashMap)`,`Result<Vec>`) return mis-lifts its `len` | every aggregate-returning fn across a lifted call | g129–g131; the original "len reads pointer-shaped garbage" |
| `&stack_local` lifts to 0x0 across a call | committed g56 fix (`Box::new(new_tree)`) | mod.rs:521 |

These are **one bug class** ("optimized-code value/control mis-lift"), with many faces. It hits **std**
(`collect`, `Range`/slice iterators) → **cannot** be worked around in azul source. 4 source-workaround
forms were tried on the `dom_children` loop and ALL failed (g136 slice-iter, g137 `0..len`, g138 reorder,
g139 volatile-len + `get_unchecked`).

**Why it resisted isolation:** minimal `--bytes` repros lift to correct IR; only the full optimized
function mis-lifts at runtime. So it's a remill execution-fidelity issue, not a static IR offset bug.

---

## 2. Three candidate fixes — try in THIS order (cheap→deep)

**(A) ⭐ FIRST, CHEAP, HIGH-VALUE: build `azul-layout` at a lower opt-level.** The mis-lifts are
optimizer-induced (SROA + iterator inlining). Add to the workspace `Cargo.toml`:
```toml
[profile.release.package.azul-layout]
opt-level = 1     # or "s"/0 — try 1 first; fewer SROA/iterator transforms for remill to mis-track
```
(Possibly also `azul-core`, `azul-css`.) Rebuild + relift `web-text-min.c`, run the harness, check
`[g132 lays-out] overflow_size.height > 0`. **If the loop now enters / text positions → this is the
pragmatic unblock for hello_world.c** (much cheaper than fixing remill). ⚠ NOTE: per-fn `-O1` did NOT fix
the *sret* face earlier (g129), but the *iterator/loop* face was never tested at low opt — worth it.
Risk: `-O0` makes wasm functions exceed the local-count limit ("local count too large") — use `1` or `s`.

**(B) The remill execution-fidelity fix (THE real, general fix).** Root-cause why optimized-code
register/SROA tracking + iterator control flow mis-execute in the lift. Approach: lift the *full*
`collect_and_measure_inline_content_impl` (two monomorphs in the dylib at native `0x26f488` and `0xabf5fc`
— `nm libazul.dylib | grep collect_and_measure_inline_content_impl`) via
`remill-lift-17 -bytes <hex> -address <addr> -ir_out /tmp/x.ll`, then **execute/trace** the lifted loop
(not just read the IR) and diff register/memory state vs native. Likely culprits: a spill/reload or a
PHI the lift mis-models for the loop induction var / Vec len. This is deep, multi-session remill work.

**(C) Last resort: keep adding source workarounds.** PROVEN INSUFFICIENT (can't touch std). Only buys
individual azul sites; will NOT get hello_world.c fully working. Avoid as the primary plan.

---

## 3. What is FIXED / KEEP (do not revert)

- **`collect_and_measure_inline_content`(+`_impl`) → out-param** (`&mut Vec<InlineContent>`,
  `&mut HashMap<ContentIndex,usize>`, `-> Result<()>`), caller `layout_ifc` (fc.rs:2433) allocs + passes
  `&mut`. Removes the 1.6 GB `InlineContent::clone` OOB. (The OOB was the caller's niche-`?` MIS-READING a
  genuine collect Err as Ok → garbage Vec; the out-param made `Result<()>` read correctly.)
- **`resolve_intrinsic_track_sizes` → `NeverLift`** (`symbol_table.rs::classify_for_name`, near top): a
  67 KB taffy-GRID fn that intermittently HANGS remill-lift; grid-only, never called for text/flex.
- **enforce_sp `__az_indirect_dispatch` wrap** + `AZ_LTO_LEVEL`/`AZ_WASM_LD_MLLVM` env knobs
  (`transpiler_remill.rs`): real leak-fix / harmless.
- Prior committed/keeper fixes: g56 `Box::new(new_tree)` (mod.rs:521), `#[repr(C,u8)]` on text3 enums,
  the 5 earlier out-param workarounds, font-resolution last-resort, build-std atomic fixes, etc.

## 4. REVERT-at-cleanup (diagnostic scaffolding, all `web_lift`-gated)

- `layout/src/solver3/fc.rs`: all `[g129..g139 az-web-lift]` `write_volatile(0x606xx/0x607xx/0x608xx ...)`
  markers + the g132/g133/g134/g135/g136 marker blocks + the g137/g139 loop rewrites (the
  `for item_idx in 0..` + `get_unchecked` + `_ifc_root_node_type` rename — revert to the original
  `for (item_idx,&dom_child_id) in dom_children.iter().enumerate()` once the lift is fixed).
- Many older `write_volatile(0x60704/0x6071C/...)` step/phase markers across `fc.rs`, `sizing.rs`,
  `mod.rs`, `window.rs`, `getters.rs`, `text3/{cache,default}.rs`, `cache.rs`, `layout_tree.rs`
  (grep `write_volatile` in `layout/src`). These PERTURB codegen (heisenbug) — strip them for clean reads.
- `scripts/m9_e2e/layout-flexbox.js`: the `[g119..g136]` POST/SUCCESS marker-read blocks (it's an
  untracked scratch harness — low priority).

## 5. Build / run recipe (KEEP — this is the working loop)

```bash
cd /Users/fschutt/Development/azul-mobile
DYLDIR=target/aarch64-apple-darwin/release
# fast gate (~25s):
cargo check -p azul-layout --features web_lift
# build the lifted dylib (build-std; ~70s incremental, longer clean):
RUSTC_BOOTSTRAP=1 RUSTFLAGS="-C target-feature=-lse,-rcpc,-rcpc2,-rcpc3 -C llvm-args=-aarch64-enable-ldst-opt=0 -C llvm-args=-enable-machine-outliner=never" \
  cargo build -p azul-dll --release --features "build-dll web web-transpiler web-transpiler-static" --no-default-features \
  -Z build-std=std,panic_abort -Z build-std-features=panic_immediate_abort --target aarch64-apple-darwin
cp "$DYLDIR/libazul.dylib" "$DYLDIR/deps/libazul.dylib"
# link the C example (-fno-stack-protector is REQUIRED — strips ___stack_chk_guard):
clang examples/c/web-text-min.c -L$DYLDIR -lazul -Iexamples/c -fno-stack-protector -o examples/c/web-text-min.bin
# run the server (RELIFT ~15-30 min; SLOWER under browser CPU load):
DYLD_LIBRARY_PATH=$DYLDIR \
  REMILL_LIFT_BIN=/Users/fschutt/Development/azul/third_party/remill-install/build/remill/bin/lift/remill-lift-17 \
  AZ_BACKEND=web://127.0.0.1:8800 AZ_NO_LIFT_CACHE=1 \
  nohup ./examples/c/web-text-min.bin > /tmp/server.log 2>&1 &
# harness (after the port answers 200):
AZ_LENIENT=1 node scripts/m9_e2e/layout-flexbox.js
```
**Gotchas (cost real time):** ① `AZ_LIFT_CACHE=1` **HANGS** the lift — always `AZ_NO_LIFT_CACHE=1`.
② Poll the PORT (`curl 127.0.0.1:8800`), never `kill -0` the launch pid (it forks+exits). ③ Clean orphans
between runs: `ps -axo pid,command | grep -E 'remill|web-text-min' | grep -v grep | awk '{print $1}' | xargs kill -9; lsof -ti tcp:8800 | xargs kill -9`.
④ `AZ_WASM_DEBUG=1` crashes this build. ⑤ Diagnostic markers write `0x60xxx` (read directly by the harness
via `AzStartup_peekU32`); `0x40xxx` reads are legacy/unwritten → spurious 0 (THIS is the "InvalidTree" phantom).
⑥ Markers in code reached NATIVELY (font parse runs in both) SEGV the server — keep markers wasm-only.

## 6. The road to hello_world.c (after the systemic fix lands)

`web-text-min.c` (body + "Hello") is the minimal repro currently used. Once text POSITIONS there
(`overflow_size.height>0`), move to `hello-world.c`, whose KNOWN additional blockers (from
`web_flexbox_lift_2026_06_01.md`) are:
1. **Button styling** — the cascade of border/bg/padding/gradient by-value `CssProperty` (the 179-variant
   jump-table mis-lift class; restyle `CssProperty::clone`). Earlier "cb-OOB" was the stack-protector
   canary → already fixed via `-fno-stack-protector`.
2. **Auto-height** — a block sizing to its text content (button height): earlier got `Height:100%` instead
   of content-height (compact-cache `Percent(100)`); bisect `apply_ua_css` vs `compute_inherited_values`.
3. **Counter text** — `__snprintf_chk` classified `Leaf` → counter renders empty; needs a real lift/impl.
4. **Click** — `AzStartup_dispatchEvent` hit-test + cb + TLV SetText patch (M9 path exists; re-verify).

Most of (1)-(3) are likely the SAME systemic optimized-code mis-lift (jump-tables, Vec/iterator, sret) —
so fix (B)/(A) FIRST; many of these may fall out together.

## 7. Status of uncommitted work
EVERYTHING is uncommitted (per instruction — commit only when asked). `git diff --stat` = ~35 files,
~2900 insertions across the arc. Keepers vs revert-at-cleanup are in §3/§4.
