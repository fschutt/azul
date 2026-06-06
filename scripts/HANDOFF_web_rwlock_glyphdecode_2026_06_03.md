# Handoff — Web backend: RwLock spin fix → font resolution works → glyph-decode OOB cleanup

**Date:** 2026-06-03 (late) · **Branch:** mobile-ios-android · **Goal:** `examples/c/hello-world.c` end-to-end on the web (lifted) backend — counter text + "Increase counter" button + on_click.

---
## ⭐⭐ TEXT SHAPES! (2026-06-06, g127) — "Hello" → 5 glyphs on the web lift. Two fixes landed: remill FMUL-by-element (g124) + shape_text OUT-PARAM (g127). Current blocker = the ACTUAL-LAYOUT pass: layout_flow content slice-len mis-lift (g128 fix building).
**g127 RESULT: `font.shape_text RETURNED 5 glyphs` (was 171120), Stage-3 SHAPED (shaped.len=5).** The whole measure/intrinsic-sizing pipeline runs end-to-end. Two root causes fixed this arc:
1. **remill couldn't lift NEON `FMUL (by element)`** → implemented `FMUL_ASIMDELEM_R_SD` decoder+semantic in the remill fork (g124, IR-verified). Reusable recipe for any unlifted NEON: synth-PC→fn→otool→unstub Arch.cpp decoder (RegNum=scoped enum, cast thru uint8_t!)+SIMD.cpp semantic→`ninja remill-lift-17 lib/Arch/AArch64/Runtime/aarch64.bc`+cp .bc→relift.
2. **`ParsedFontTrait::shape_text` returned `Result<Vec<Glyph>>` whose len sret-mis-lifted** (built 5, caller saw 171120) → OUT-PARAM the whole chain (g127: trait + ParsedFont/FontRef/FontOrRef impls + shape_text_internal `out.append` + for_parsed_font/for_font_ref + callers cache.rs:7255/7083/7122). Use append/extend (element-wise), NOT `*out=vec`.
**⏸ PAUSED 2026-06-06 (user chose "consolidate & pause" after TEXT SHAPES).** g128 (layout_flow `content: &Vec`) did NOT clear the OOB: still `InlineContent::clone ← layout_flow` because the `content` Vec ITSELF arrives with a mis-lifted len — from **`collect_and_measure_inline_content` (fc.rs:6340/6354)** which returns `Result<(Vec<InlineContent>, HashMap)>` (tuple sret → Vec-len mis-lift, the 6th instance of this class this session).

**▶ NEXT SESSION — pick ONE:**
- **(RECOMMENDED) Systemic remill sret/multi-word-return fix.** EVERY fn returning a struct-with-Vec (Vec / Result<Vec> / (Vec,X)) mis-lifts the Vec `len` word (reads as a pointer-shaped garbage e.g. 171120/0x628c23c ⇒ smells like a FIELD-OFFSET shift in the sret store/load, NOT a value corruption). Fixed 6× by per-fn out-param (collect_inline_content, create_logical_items, measure Result via repr LayoutError, font.shape_text, layout_flow content, …). A single remill/transpiler fix to the sret write/read of multi-word returns would kill the WHOLE class AND let us revert all the out-param/&Vec workarounds. Investigate how remill lifts an aarch64 sret (x8) store of a {ptr,cap,len} Vec field — the len store/load offset.
- **(grind) Per-fn out-param `collect_and_measure_inline_content` + `_impl`** (fc.rs): out-param `out_content: &mut Vec<InlineContent>`, return `Result<HashMap>`; the 4 `Ok((content, child_map))` returns at fc.rs:6412/6632/6803/7182 → `out_content.append(&mut content); Ok(child_map)`; wrapper 6340 delegates; caller fc.rs:2433 `let mut inline_content=Vec::new(); let child_map = collect_and_measure_inline_content(ctx,...,&mut inline_content)?;`. Then expect the NEXT Vec-return in layout_flow's internals (perform_fragment_layout etc.) + the `err=21` NEON (`mvni.2s,msl`/`fneg.2s`, same remill-decoder recipe as FMUL).

**STATE (all KEEP, cargo check RC0):** remill FMUL-by-element (g124); shape_text out-param chain (g127); layout_flow content `&Vec` (g128, correct-but-insufficient alone). ★ TODO-cleanup (non-blocking, NOT compiled by `cargo build -p azul-dll`): layout/tests/test_glyph_cache_shaping.rs + test_ligature_shaping.rs call shape_text w/ old sig (add `&mut Vec::new()` arg). ★ Many 0x607x/0x608x/0x60700 diag markers to revert at final cleanup.

## ⭐ (HISTORICAL) 2026-06-06, g124 — FMUL-by-element remill fix DONE ✓; the shaper now RUNS DEEP. New blocker = jump-table MISSING_BLOCKs in the allsorts shaper → corrupt glyph count → PANIC in `SmallVec::extend`.
**FMUL-by-element is FIXED & PROVEN** (decoder+semantic in the remill fork — see g124 in memory; `--bytes` IR-verified + relift shows `__remill_error=0`, was 4). The shaper (`shape_text_correctly` → allsorts `font.shape_text`) now EXECUTES (592 allocs, 120KB not 789MB) instead of bailing. It now TRAPs `unreachable` in `<SmallVec as Extend>::extend` (allsorts glyph buffer). Since `missing_block` is NON-trapping (g119 had 34 w/ rc=0), this `unreachable` is a PANIC (panic_immediate_abort) — a capacity/iter overflow from CORRUPT glyph data produced by jump-table MISSING_BLOCKs in the shaping path (ring: 0x4de6100/0x4de776c/0x4c52e8, in unlogged/Leaf fns) returning garbage.
**NEXT (Task#2 jump-table devirt — deep):** resolve the shaping-path computed-branches. Approaches: remill `--extra_data` exact-devirt (used for M12.7 calc-jumps), or the transpiler's indirect-dispatch handling (dll/src/web/transpiler_remill.rs, AZ_NO_INDIRECT_DISPATCH gate ~1997/2571). Identify the jump-table fns: relift, map the missing_block ring PCs → fns (the synthmap method, but these PCs are in unlogged fns — may need to widen logging or trap-on-missing_block to get a named stack). The jump-tables are reached via font.shape_text (allsorts GSUB/GPOS) → SmallVec::extend. ★ HOW the FMUL instr was added (reusable for any future unlifted NEON): synth-PC→fn (synthmap) → otool-disasm dylib (slide=native−nm-vmaddr; instr=fn_vmaddr+(PC−synth_entry); remill records the FALLTHROUGH PC) → unstub/add decoder in remill Arch.cpp (RegNum is scoped enum → cast through uint8_t!) + semantic in Semantics/*.cpp + `ninja remill-lift-17 lib/Arch/AArch64/Runtime/aarch64.bc` + cp .bc to install share → relift (no azul rebuild). Verify: `remill-lift-17 --arch aarch64 --address 0x1000 --bytes <LE-hex> --ir_out x.ll`, grep `@__remill_error`(want 0).

---
## (HISTORICAL) 2026-06-06 g123 — ONE NEON instruction: remill can't lift `FMUL (vector, by element)`. [RESOLVED in g124]
The ENTIRE web text-measure pipeline now works (after ~8 lift fixes this session — see g117–g123 below): DOM→cascade→collect→logical→bidi→visual→font-chain→font-load→`shape_text_correctly`→`font.shape_text`. It bails ONLY in the allsorts glyph shaper (`text3::default::shape_text_internal`) on an **unlifted NEON instruction**: `fmul.2s v15,v0,v2[0]` / `fmul.4s v0,v0,v4[0]` = **`FMUL_ASIMDELEM_R_SD`** (vector FMUL by indexed element), used for glyph-metric math. remill's `__remill_error` is non-trapping (records LAST PC, returns garbage) → the `?` propagates Err → text stays AUTO.

**THE FIX (next, focused — deep remill-fork work, NO azul rebuild after):**
- In `azul/third_party/remill/lib/Arch/AArch64/`: `TryDecodeFMUL_ASIMDELEM_R_SD` is a STUB (Decode.cpp:26518, unnamed params=return false); ALL 24 `*_ASIMDELEM_*` (vector-by-element) decoders are stubs (0 impl). Implement the decoder (mirror the working `FMUL_ASIMDSAME_ONLY_2S/4S` vector path in Semantics/SIMD.cpp:782-784, + extract Q for 2s/4s, and the element index from the L/H/M bits + Rm-low-reg per the ARM `FMUL_asimdelem_R_SD` encoding) + a `DEF_ISEL(FMUL_ASIMDELEM_R_SD_*) = <FMUL vec-by-elt semantic>` (each lane of Vn × Vm[index]). Decoder is already wired into the table (Decode.cpp:42950-42951) — just unstub it.
- Rebuild: `ninja remill-lift-17 lib/Arch/AArch64/Runtime/aarch64.bc` then cp the .bc to the install share; relift web-text-min (NO azul rebuild). Then text SHAPES → MEASURES → height-prop → hello-world.c.
- DECODER REFERENCES (fork's hand-written decoders, Arch.cpp): `TryDecodeFMUL_ASIMDSAME_ONLY`@4506 → `TryDecodeFP_ASIMDSAME_3` (the vector-FMUL operand setup to mirror); `TryDecodeDUP_ASIMDINS_DV_V`@4547 (by-element INDEX extraction pattern: AddArrangementSpecifier + AddRegOperand kRegV ×N + AddImmOperand(index)). InstData has `.Q .size .sz .Rd .Rn .Rm .imm5 .H .L .M` (verify H/L/M exist; FMUL-by-elt SD index = H:L:M for S / H for D, Rm restricted to v0-v15 for S). The SEMANTIC is NEW (no vec-by-element FMUL exists): add an FMUL-vector-by-element DEF_ISEL in Semantics/SIMD.cpp that broadcasts Vm[index] and multiplies each lane of Vn (mirror FMUL_VEC_2S/4S@782-784 but read one indexed lane of Vm). ⚠ Get H:L:M EXACTLY right (ARM ARM `FMUL_asimdelem_R_SD`) — a wrong index = silent bad glyphs; test with a known `fmul.2s ...[idx]` via `remill-lift-17 --bytes`.
- Also verify `FNEG.2S` / `MVNI.2S,msl#16` (collect_and_measure error site) — may already lift, or the real culprit there is an fmul a few instrs up.
- HOW the instr was ID'd (reuse): map __remill_error synth PC → fn via /tmp/synthmap.txt (server-log `resolved=`; sub_<X> X=synth; container=largest entry ≤ PC), then otool-disasm the dylib (slide = native − nm-vmaddr; instr vmaddr = fn_vmaddr + (PC − synth_entry); remill records the FALLTHROUGH PC so the unlifted instr is the one BEFORE it).

**KEPT this session (the real fixes — do NOT revert):** `#[repr(C,u8)]` on InlineContent, LogicalItem, ShapedItem, FontStack, solver3::LayoutError, cache::LayoutError (niche-disc mis-lift fix); cache bypasses for logical_items/visual_items/shaped_items/per_item_shaped (empty-hashbrown mis-behavior); collect Result-ignore (sizing.rs:616); FontChainKey get→find→only-chain fallback (Ord+Eq mis-lift). **REVERT at cleanup:** all 0x607x/0x608x diag markers + their harness reads.

---
## ★★★★★★ 2026-06-06 g117–g121 — THE ROOT CAUSE: web lift mis-reads repr(Rust) NICHE discriminants. FIX = `#[repr(C, u8)]`. measure_intrinsic_widths now runs END-TO-END (Stage 1/2/3); only the font-chain lookup in shaping remains (g121 pinpointing).

**SYSTEMIC ROOT (the big one): the web lift MIS-READS any repr(Rust) NICHE discriminant** — enum-niche (disc encoded in a variant's non-null ptr field) OR Result/Option-niche (from the payload's ptr). A RAW load at offset 0 reads fine, but the match/derived-Clone/`?` niche-disc logic mis-routes. **FIX = `#[repr(C, u8)]`** → explicit u8 tag at offset 0 → simple load the lift handles (the layout other "lift-fine" enums already use). Applied to: InlineContent, LogicalItem (g117), ShapedItem (g118), solver3::LayoutError + cache::LayoutError (g119), FontStack (g121). This is the GENERAL fix — apply to every data-carrying repr(Rust) enum + every `-> Result<_, NicheErr>` error type reached in the lifted path (incl. the button/cascade path later).

**Secondary: empty hashbrown maps mis-behave on the lift** (un-mirrored ctrl despite g96's static EMPTY_GROUP). `entry().or_insert()` HANGS (false all-full → ∞ probe); `get()` returns a FALSE HIT (→ empty result). **FIX = bypass the per-call caches** (build Arc directly / always-shape). Applied: logical_items (g115), visual_items + shaped_items (g118), per_item_shaped get/insert (g120). [Deeper root candidate: the lifted allocator/ctrl-init memsets 0x00 not 0xFF — revisit for a systemic hashbrown fix so caches can be restored.]

**Progression this session (each fix advanced one stage, verified by markers):** 789MB OOB (InlineContent::clone disc) → g117 OOB GONE → Stage-2 visual_items cache HANG → g118 HANG GONE → `rc=5` collect Result Ok→Err mis-lift → g119 collect Ok + `rc=0` → text still AUTO: measure runs ALL stages (cli-entry len=1 ✓, Stage-2 visual.len=1 ✓, **Stage-3 SHAPED ✓**) but **shaped.len=0** → g120 per_item_cache false-HIT bypass (still 0) → **shape_visual_items returns empty: the `FontStack::Stack` arm's `font_chain_cache.get(&cache_key)` misses (or shape_with_font_fallback finds no font) → `shape_text_internal` (probe 0x40700) never reached.** g121 (BUILDING): FontStack repr(C,u8) + markers 0x60820 (Ref/Stack arm) / 0x60828 (chain.get Some/None) / 0x6082C (reached fallback) to pinpoint. If chain.get=None → FontChainKey Ord/Hash mismatch (BTreeMap key) or a genuine storage-vs-lookup key diff (g80b hinted "Phase2 drops fonts") → fix next. ★ TOOLING: success-path markers (read in harness even w/o a trap — POST-TRAP block only runs on trap) at uncollided 0x607D0+/0x608xx (NOTE 0x607C0/C4 collide w/ old g70). ★ REVERT-at-cleanup: all 0x607x/0x608x diag markers + harness reads; the g115/g118/g120 cache bypasses (restore once hashbrown root fixed); KEEP all the repr(C,u8) changes (the real fixes).

## ★★★★★ 2026-06-06 g113–g116 — localized the recurring 789MB OOB to create_logical_items' `content.iter()` (content.len mis-lifts ACROSS the measure→cli boundary). Cache bypass (g115) removed the Stage-2 hang. g116 = definitive content.len-inside marker (building).

**THE PICTURE NOW.** The web text-measure path is one persistent `{ptr,cap,len}` ABI mis-lift, chased through stages:
- **g113 (AZ_FUEL=ALL, no rebuild)** turned the g112 HANG into a named trap → loop was in `measure_intrinsic_widths`'s own frame.
- **g114 (create_logical_items → out-param)** did NOT fix it — identical fuel stack. So the by-value Vec RETURN was NOT the cause.
- **AZ_REMILL_KEEP_SCRATCH=1 relift (no rebuild)** kept measure's IR (`__az_dep_<native>.opt.ll`, named by addr from server-log `resolved=…@0x<native>`, NOT mangled). measure's CALLEES (`grep '@sub_…' | sort -u` → resolve via server-log): create_logical_items, calculate_id×2, reorder_logical_items, shape_visual_items_with_per_item_cache, apply_text_orientation, get_base_direction_impl, hashbrown insert/retain/reserve_rehash — ALL separate (NOT inlined). Since AZ_FUEL trapped `__az_fuel ← measure` DIRECTLY, the loop was in measure's OWN body = Stage-2 `logical_items.iter().any()`, OUTER count = `logical_items.len()` garbage (from the `self.logical_items.entry().or_insert_with(||Arc::new(li)).clone()` cache/Arc/self chain — or hashbrown empty-map entry()-probe loop).
- **g115 (BYPASS the logical_items HashMap cache** — local `Vec`+`Arc` in measure Stage-1, ~cache.rs:5786): the HANG is GONE → now the **789MB OOB** (`lastAllocSize=0x314611e0`), named stack = **`InlineContent::clone ← create_logical_items ← measure`**. ⇒ the cache/empty-map probe was the hang; now reaching cli, which **iterates `content` past its 1 real element → `_` arm clones content[1] garbage → 789MB**. (The g115 Stage-1 markers li_local.len=2 etc. are STALE — cli trapped before reaching them.)

**KEY: measure's `content.len()`=1 is CORRECT** (calculate_id, a callee, read it + completed), but `content.len()` mis-lifts to ≥2 INSIDE create_logical_items — a boundary mis-lift of the `&Vec` content (its len-load or the &Vec ptr). g112's `&Vec` fix apparently regressed (possibly g114's out-param ABI shift, OR the cache-closure vs direct-call passes content differently).

**g116 (BUILDING): marker at create_logical_items ENTRY** capturing `content.len()`@0x607D0 + `content.as_ptr()`@0x607D4 (wasm-only path). Verdict: len≥2 → boundary len-load mis-lift (fix: pass content.len() as an explicit `usize` arg + index `content[i]`, OR inline cli's loop into measure where len=1 is correct); as_ptr≠0xc518620 → the &Vec PTR mis-lifts (fix the arg passing). ★ NEW no-rebuild tooling this session: AZ_REMILL_KEEP_SCRATCH=1 (keep IR) + harness mem-dump at a marker-captured ptr (read the collected InlineContent → confirmed content is CLEAN: String len=5). ★ REVERT-at-cleanup: g115 0x607Cx + g116 0x607Dx markers + harness reads; decide keep-vs-restore the logical_items cache once cli is fixed.

## ★★★★★ 2026-06-06 g108–g112 — TWO MORE FIXES: (1) named-stack tooling pinned the OOB to InlineContent::clone; (2) the 789MB OOB is FIXED (content &Vec). Trap→HANG = OOB gone, now a downstream shaping-ish hang (g113 diagnosing).

**KEY TOOLING WIN (reuse this): `--keep-section=name` in the lift link** (transpiler_remill.rs link_objects_to_wasm ~1448: `--strip-all` → `--strip-all` + `--keep-section=name`, KEEPS --lto-O3 so indices/behavior match production, pure metadata, NO heisenbug) gives NAMED lifted trap stacks. Node prints `sub_<X>` frames; decode via the lift's server-log `sub_<X> → resolved=<NAME>@<native>` lines (nm-by-vmaddr is USELESS — sub_<X> is a shim/canonical addr). This pinned the 789MB-alloc trap to `<InlineContent as Clone>::clone` ← create_logical_items ← measure_intrinsic_widths.

**ROOT CAUSE (g111, decisive): the InlineContent is CORRECT in memory** (upstream marker in collect_inline_content_for_sizing: out.len=1, out[0].disc=0=Text). So the lift **mis-lifts `content.len()`** — the fat-slice `len` word corrupts across the `measure_intrinsic_widths → create_logical_items` call boundary (Vec.len=1 in the header is right, but the slice-arg len → garbage ~0x628c23c) → `content.iter()` runs off the 1 real element into OOB memory → the `_`/Object arm clones a garbage InlineContent → `InlineContent::clone` reads a garbage field as a Vec len → ×8 → ~789 MB → BumpAlloc memset OOB. (Same fat-slice/by-value ABI mis-lift CLASS as g78 Result<Vec> sret.)

**FIX (g112, KEEP): pass `content` as `&Vec<InlineContent>` (thin ptr) not `&[InlineContent]` (fat slice)** in `measure_intrinsic_widths` + `create_logical_items` (cache.rs) so `.len()`/`.iter()` read the len from the Vec HEADER inside the fn. Adapted the 2 non-web callers (flow pipeline in cache.rs + window.rs:5943 shape_text_for_relayout) with `.to_vec()`. Also KEEP the g110 if-let restructure of create_logical_items's content match (routes Text via a standalone if-let). RESULT: **the `memory access out of bounds` trap is GONE** — now `RESULT=HANG_100s` in solveLayoutReal = the OOB is fixed and layout runs DEEPER (into BiDi/shaping) where it hits a NEW hang.

**NEXT (g113 in progress): convert the hang→trap with `AZ_FUEL=ALL`** (env, read at LIFT time → just relaunch the binary, NO rebuild; transpiler ~1245 inject_fuel traps after AZ_FUEL_LIMIT≈200M block-execs) → the named stack (--keep-section=name still in) shows the looping fn. Likely an allsorts-shaping hashbrown empty-map (different from the g96 EMPTY_GROUP static) OR another slice-len mis-lift now that real content reaches shaping. ★ REVERT-at-cleanup: g111 cli-disc markers (sizing.rs:1051), g108 --keep-section=name (after hang ID'd), g111-era diag in collect.

## ★★★★★ 2026-06-06 g96–g105 SESSION SUMMARY — HANG FIXED (EMPTY_GROUP, permanent). Remaining text blocker = ONE garbage-alloc-size field mis-read in the measure path (NOT a jump-table derailment — that g100/g101 read was WRONG).

**WIN (keep forever): the 6-session shaping HANG is FIXED** by the systemic EMPTY_GROUP mirror (g96, details in the section below). Confirmed across g96–g105: `EMPTY_GROUP-AUTO ... mirrored 278 ... runs`, no 150s hang.

**Remaining blocker: a garbage 789MB allocation in the text-MEASURE path → `memory.fill` OOB.** Established facts (g97–g104):
- The BumpAlloc helper records each alloc size @0x40030; on the trap it = **`0x314611e0` (≈789MB)** → bump-ptr → 1.03GB (past the 512MB wasm mem) → `memset` OOB. `÷8 = 0x628c23c` = a **bump-heap pointer** (heap base 0x6000000) ⇒ a Vec/String `{ptr,cap,len}` **len-or-cap field is read as the POINTER** (same class as the AzString ptr-as-len bugs; NOT jump-table devirt). `allocCallCount=537`.
- The trap call STACK is CLEAN (`solveLayoutReal→layout_dom_recursive→layout_document→calculate_intrinsic_sizes→measure_intrinsic_widths→…→alloc`) ⇒ NOT a derailment. (My g100/g101 "corrupt PC/LR into LLVM data → jump-table derailment / Task#2" was WRONG: those were the INLINED BumpAlloc helper's STALE %pc/%X30; the "0xc0…" prefix = the missing_block COUNT 12=0xc the harness concatenates. The 12 missing_blocks are benign.)
- g103 markers in create_logical_items's chunk loop ALL read 0 (incl an unconditional mark) ⇒ the alloc is UPSTREAM of that loop: in Stage-1 `calculate_id(&content)`, the `self.logical_items.entry().or_insert_with()` cache map, or create_logical_items's pre-loop (BTreeSet boundaries / style_cache+run_overrides HashMaps).
- **The marker-relift approach is EXHAUSTED — it's HEISENBUG-SENSITIVE** (g104 markers flipped trap→HANG, no data). REVERTED all g98–g104 diagnostics → clean g96 (g105 confirms). `cargo check` both crates RC 0.

**NEXT (non-perturbing, NO relift):** read the clean-build `.opt.ll` already on disk: `KEEP_SCRATCH /var/folders/5x/.../T/azul-web-transpiler-1011/` (g96 build, libazul base 0x10c9b0000). `measure_intrinsic_widths.opt.ll = __az_dep_10d3b8320.opt.ll` (633KB). Trace the 66 alloc sites (`sub_1050adc`/inlined bump helper) for the one whose X0 (size, stored to state+x0_off pre-call) derives from a loaded struct field that holds a heap ptr (0x628c23c → ×8 → 0x314611e0). Or read create_logical_items.opt.ll (`__az_dep_10d3b8d4c.opt.ll`) + its named sub_X callees (sub_b25054 ×10 etc.). Then fix the `{ptr,cap,len}` field-offset read at SOURCE (cf. g78 out-param / g81 BTreeMap) or in the lift's Vec/HashMap field handling. (Marker-based pinpointing is OFF the table — heisenbug.) The actual layout (layout_ifc) also calls measure → can't web-skip; must fix at root.

## ★★★★★ 2026-06-06 g96 — THE SHAPING HANG IS FIXED (systemic EMPTY_GROUP mirror, DONE + PERMANENT). New blocker = a `memory.fill` OOB in the text-intrinsic path (was masked by the hang).

**The 6+ session shaping hang is GONE.** `RESULT=harness_completed` (no 150s hang). The systemic fix the older entries called for is IMPLEMENTED and CONFIRMED:
- **`dll/src/web/symbol_table.rs` `find_hashbrown_empty_group_ranges()`** (OnceLock-cached): scans libazul `__text` for `adrp Xd,#pg ; add Xd,Xd,#imm12` whose 8-aligned target begins an all-`0xFF` run ≥8 bytes → returns NATIVE `(addr,len)` ranges. **KEY DISCOVERY: EMPTY_GROUP is `[0xFF;8]` (Group::WIDTH==8, the SWAR group), NOT `[0xFF;16]` — that is why every earlier `[0xFF;16]` theory missed it.** It lives at vmaddr `0x40b0ab0` in `__TEXT.__const`, referenced by exactly one `adrp+add` in an indirectly-called fn (so the per-cb adrp scan never mirrored its page → read 0x00 → looks-all-full → RawIterRange loops).
- **`dll/src/web/transpiler_remill.rs` `inject_user_binary_data_segments`**: after `collect_synth_data_pages`, appends each range as a precise all-`0xFF` data segment (last-wins, idempotent, no whole-page false-pointer-translation OOB). Runtime mirrors 278 such runs (~2925 B). Logs `EMPTY_GROUP-AUTO (...)`. **KEEP both — permanent, no env var.** (`AZ_FORCE_MIRROR_VMADDRS` debug env kept too, harmless.)

★ **NEW BLOCKER:** `solveLayoutReal TRAPPED: memory access out of bounds`. Located via live-wasm RE (fetch `/az/mini.<h>.wasm` from the running server, map JS trap-stack `wasm-function[N]`→names via exports `__az_dep_<native>`, vmaddr=native−libazul_native_base, `nm -n`). **Stack (bottom→top):** AzStartup_solveLayoutReal → LayoutWindow::layout_dom_recursive → solver3::layout_document → calculate_intrinsic_sizes → IntrinsicSizeCalculator::calculate_ifc_root_intrinsic → TextShapingCache::measure_intrinsic_widths → **text3::cache::create_logical_items** → **func[2604] TRAP**. Trap instr (wasm-objdump, off 0xf58ca7) = **`memory.fill 0`** zero-filling a DYNAMIC STACK ALLOCA: reads lifted SP (`i32.load 2 1024`=addr 0x400), aligns size from `local 34` (=struct field `*(base+0x78)`), bumps SP, `memory.fill(SP,0,size)` → OOB b/c SP or size is GARBAGE. func[2604] is an UNEXPORTED internal-helper cluster (callees 313/239/1057/2469 also unexported) w/ atomics+__multi3 — wasm-only RE can't name it. This IS the headline-documented "calculate_intrinsic_sizes text-intrinsic pass → mis-lifted slice cursor/index → panic-fmt memory.fill OOB (+ possible TLS mis-lift)", now REACHED (hang was upstream). **NEXT (in progress): `AZ_REMILL_KEEP_SCRATCH=1` relift (g97) → read `create_logical_items*.opt.ll` to name func[2604] + see what feeds `local 34` (the alloca size) / whether the SP at 0x400 is corrupt → likely a lift mis-modeled a FIXED guest frame as dynamic (garbage size leaked into SP math), OR a real mis-lifted length field. Fix at source (cf. the existing create_logical_items scan_cursor web-lift workaround @cache.rs:5968) or transpiler.** NOTE the actual layout (layout_ifc) also calls create_logical_items → can't web-skip the intrinsic pass; must fix at root. Saved: /tmp/mini_g96.wasm, /tmp/func2604.txt, /tmp/func2312.txt, /tmp/nm_sorted.txt.

## ★★★★★ 2026-06-05 g83–g86 — [SUPERSEDED by g96: the systemic EMPTY_GROUP mirror is DONE + fixed the hang] chain+font FIXED; the allsorts hashbrown empty-map hang → SYSTEMIC transpiler empty-static mirror was the next fix.

g82/g83 confirmed the unique_font_keys BTreeMap fix populates `font_chain_cache` (cl>0) AND loads the font (loaded_fonts>0). The remaining hang is INTERNAL to allsorts `shape_text`. g85 BTreeMap'd `allsorts/src/layout.rs` `supported_features`+`lookups_index` (GSUB/GPOS feature caches) — STILL HUNG ⇒ MORE allsorts empty-maps (glyf.rs FxHashMap caches @1041/58/72, coverages/classdefs ReadCache, …). **Per-map BTreeMap whack-a-mole does NOT converge.**

★★ **THE ONE REMAINING FIX (unblocks ALL of text shaping): the SYSTEMIC transpiler hashbrown-empty-static mirror.** Force-mirror the const-folded hashbrown EMPTY ctrl static so the lifted NEON `Group` ctrl-scan reads `0xFF` (empty→terminate) not `0x00` (looks-full→loops forever). `transpiler_remill.rs collect_synth_data_pages`@2805 already mirrors EMPTY_GROUP via pointer-follow fixpoint @3034 (comment @2842 names it) but MISSES the const-folded access (address baked inline, not via `adrp` nor a mirrored pointer). Approaches: (a) scan `__DATA_CONST`/`__const` for a 16-byte-aligned `[0xFF;16]` run, always add those pages to the mirror set; (b) detect the hashbrown ctrl-scan pattern in the lift + inject a correct empty group. Fixes allsorts + std + rust-fontconfig at once → text shapes → MEASURES → height-prop → button/on_click → hello-world.c.

**Codebase:** KEEP all correct fixes — g72 (panic removal), g73 (ensure_chains_nonempty), g78 (out-param), g81 (unique_font_keys BTreeMap), g85 (allsorts supported_features/lookups_index + gsub already BTreeMap). g86 left a temp web trap (sizing.rs ~651, `panic!("[g86]...")`) so web-text-min RETURNS non-hang — REMOVE it to test the systemic fix. All g51–g86 diagnostic markers + the temp trap are REVERT-at-cleanup.

## ★★★★★ 2026-06-05 g80–g82 — ROOT of the empty chain (#4) + the hang (#3) = the g47 hashbrown EMPTY-INSERT mis-lift. Fixed unique_font_keys (BTreeMap); g81 still hung ⇒ MORE empty-inserts downstream (rust-fontconfig/allsorts).

Traced the empty `font_chain_cache` to its source: `collect_and_resolve` produces 0 chains (g80: afterResolve=0) because `collect_font_stacks_from_styled_dom` Phase 1 leaves `unique_font_keys.len=0` (g80b, read live off the g79-trapped server) EVEN THOUGH `n1.nt[0]=177` (text disc correct) and `is_text` is true. So the `unique_font_keys.entry(k).or_insert(i)` into an EMPTY HashMap (cap-0 reserve_rehash) is the **g47 lifted hashbrown EMPTY-INSERT mis-lift** — silently drops the insert. Cascade: empty unique_font_keys → 0 stacks → 0 chains → empty font_chain_cache → loaded_fonts empty → allsorts builds empty maps in shape_text → the g47 RawIter HANG. **So #3 and #4 are ONE bug: hashbrown empty-map.** FIX (g81, getters.rs:3535): `unique_font_keys` HashMap→BTreeMap (Ord key, pure local, immune). g81 STILL HUNG ⇒ the BTreeMap fix alone isn't enough — there are MORE empty-insert maps DOWNSTREAM (rust-fontconfig `query_matches`/`find_unicode_fallbacks` at /Users/fschutt/Development/rust-fontconfig-azul, and/or allsorts shaping maps — both DEPENDENCIES). g82 (relifting): CONDITIONAL trap before measure (panic only if chain still empty) to read whether the BTreeMap fix populated unique_font_keys/chain → if chain>0 the hang is purely in allsorts shaping; if chain=0 a rust-fontconfig empty-insert is next.

**STRATEGY for the remaining hang:** this is whack-a-mole across MY maps + DEPENDENCY maps. Durable fix = the SYSTEMIC transpiler empty-static mirror (force-mirror the const-folded hashbrown empty ctrl static — fixes ALL empty-map hangs incl deps at once; collect_synth_data_pages @transpiler_remill.rs:2805/fixpoint @3034 has the infra but misses const-folded refs). Per-map BTreeMap only works for MY code (getters.rs maps), NOT allsorts/rust-fontconfig (would need forking). So: BTreeMap my maps to GET PAST resolution, then the systemic mirror (or fork the deps) for the shaping maps. Diagnostic method: conditional-trap (panic only on the bad state) to stay non-hang + readable.

## ★★★★★ 2026-06-05 g78 — blocker #2 FIXED (out-param refactor). web-text-min now reaches REAL shaping → hits the g47 hang (#3 = the FINAL blocker). 2 of 4 fixed this session.

The `Result<Vec<InlineContent>, LayoutError>` by-value RETURN mis-lift (Ok→Err, blocker #2) is FIXED by converting `collect_inline_content` + `collect_inline_content_for_sizing` (sizing.rs:1009/1280) to an `out: &mut Vec` param + `-> Result<()>` (register return, no sret-of-Vec — the M12.7 "pointer arg lifts cleanly" pattern). Single caller (IFC sizer sizing.rs:611) allocs the Vec. **CONFIRMED by transition:** g77 (propagate Err) = clean `InvalidTree`; g78 (out-param) = `HANG`. InvalidTree→HANG ⇒ collect now returns Ok (real "Hello" content) → measure_intrinsic_widths → shape_text → the g47 hashbrown empty-map loop.

**4-blocker scorecard:** #1 panic@0x90 ✅FIXED(g72). #2 Result<Vec> sret mis-lift ✅FIXED(g78, KEEP). #3 g47 hashbrown empty-map hang in shape_text = **ACTIVE FINAL blocker**. #4 empty font chain (g73, latent). The codebase now HANGS web-text-min (keeps the correct #2 fix); the heartbeat harness caps at 150s.

**#3 = the one remaining blocker for text.** Shaping happens in BOTH the intrinsic pass AND the actual layout (which shares the intrinsic pass's shaping cache), so a web-skip of just the intrinsic measure only MOVES the hang. Fix at root: (a) transpiler — force-mirror the const-folded hashbrown empty static (the mirror infra `collect_synth_data_pages` @transpiler_remill.rs:2805 + pointer-follow fixpoint @3034 EXISTS but misses const-folded accesses; comment @2842 names the exact symptom), OR (b) localize the hanging map in shape_text (text3/default.rs) via panic→trap bisection (panic_immediate_abort: panic→brk→return, then read markers — you CAN'T read markers from a hung wasm call) → BTreeMap-convert it. Method that fixed #2 (out-param) is the template: avoid by-value complex returns/HFAs in lifted hot paths.

## ★★★★ 2026-06-05 g76/g77 — COMPLETE web-text-min text blocker map (3 deep lift bugs traced this session). Codebase left CLEAN (returns InvalidTree, no hang).

The InvalidTree is NOT a bad index and NOT the font chain. **g76 experiment:** degrading the IFC-sizer `collect_inline_content` Err → `Vec::new()` (sizing.rs:600) flipped the result from clean `InvalidTree` → `HARNESS_HANG_150s`. That state-change PROVES two things at once:
- **Blocker #2:** `collect_inline_content` returns Err to its caller even though the source reaches `Ok(content)` (marker B8) and has no Err path after it — i.e. the lifted by-value `Result<Vec<InlineContent>, LayoutError>` RETURN mis-lifts **Ok→Err** (M12.7 sret/HFA struct-return class). Confirmed via the explicit `match` + `0x60760` marker (1=Ok / 0xEE=Err-despite-B8).
- **Blocker #3:** proceeding PAST that InvalidTree lands in the documented **g47 HANG** — the hashbrown empty-map infinite loop in the actual-layout shaping.

**Full chain (in order):** (1) [DONE g72] panic@0x90 brk removed. (2) IFC-sizer Result<Vec,_> return mis-lift → InvalidTree (sizing.rs:600). (3) g47 hashbrown empty-map hang in shaping. (4) [latent] empty font chain (g73 `ensure_chains_nonempty`, KEEP). **g77** reverted the Err arm to PROPAGATE (clean InvalidTree, heartbeat-safe) keeping the `0x60760` diagnostic — flip back to `Vec::new()` to chase #3.

**Real fixes (both transpiler-class, multi-session):** #2 = by-value Result/Vec sret marshalling in the lift; #3 = SYSTEMIC — mirror the hashbrown empty ctrl static (transpiler_remill.rs `scan_arm64_adrp_pages` ~6630) so the lifted NEON `Group` ctrl-scan terminates on empty (fixes ALL hashbrown hangs), OR BTreeMap-convert the shaping maps. **Can't read markers from a hung synchronous wasm call** → localize hangs via panic→trap bisection (panic_immediate_abort: panic→brk→return, then markers readable). Method that worked all session: native `otool -tV` + per-Result explicit Ok/Err markers + LIVE `0x60xxx` reads (no relift).

## ★★★★ 2026-06-05 g74/g75 — InvalidTree LOCALIZED to a `tree.get(node_index)` in the IFC sizer (NOT the font chain). Confirming the bad node_index.

**Big method win:** read the layout PHASE marker LIVE without relifting — the marker is written to **0x60704** but the harness was reading **0x40704** (stack-overlap garbage, reads 0). Fixed the harness (`peekU32(0x60704)`) → instantly localized every phase. Then bisected the IFC sizer with free-band markers.

**Findings (web-text-min, g74):** `phase(0x60704)=0xA0` = `calculate_ifc_root_intrinsic_sizes` entry (sizing.rs:592), stuck (never reaches 0xA1@596). `inline-phase(0x6071C)=0xB8` = `collect_inline_content` COMPLETED Ok (my new marker). `textlen=5` ✓, tree VALID (nodes.len=2, root ok), `rc=5` = CLEAN Err return (not a trap). **Paradox = the clue:** B8 (collect complete) AND phase 0xA0 (IFC entry, pre-0xA1) both being last ⇒ `calculate_ifc_root_intrinsic_sizes` is entered **>1 time**; one call's collect finished (B8), a LATER call fails at the *first line* `let node = tree.get(node_index).ok_or(InvalidTree)?` (sizing.rs:~1018) **before B1** — a bad/garbage `node_index`. The empty font chain is downstream (0xA1/0xA2, NEVER reached) → NOT this blocker.

**g75 (relifting):** instrumented the entry `tree.get` with an explicit match → `inline-phase=0xBAD` + `0x60754=failing node_index` on the None path; added IFC call-counter `0x60758` + per-call node `0x6075C`. This pins down (a) how many IFC calls, (b) the exact bad node_index → then trace why `calculate_intrinsic_recursive` (sizing.rs:564 caller) passes a bad index (likely a mis-lifted index in the bottom-up recursion, or `get_ua_property` missing_block garbage feeding an index). missing_block ring (stable): `get_ua_property` + `__rust_dealloc` — Task#2 jump-table, suspected source of garbage. KEEP g73 getters.rs `ensure_chains_nonempty` (needed once shaping is reached).

## ★★★★ 2026-06-05 g73 — g72 CONFIRMED (text path reachable, no corruption). NEW BLOCKER: empty font chain → text node UNSIZED → InvalidTree. Fix in test.

**g72 result (relift+harness):** ALL free-band markers now read 2 — `postReconcile=2 sizingEntry(0x607B0)=2 line142=2 afterSpan=2 beforeCall=2 HEAP cache.tree=2`. new_tree SURVIVES into `calculate_intrinsic_sizes`; NO hang (`__remill_error count=0`, harness_completed). The 6-session "corruption" was 100% the panic@0x90 abort. Shaping closure now lifted (`shape_text_internal`, `allsorts gpos::Info::init_from_glyphs`).

**NEW next blocker (g73):** `compact_cache: n0(body) w=0xc35000 h=0x927c00` = 800×600 ✓ (16000 units/px) but `n1(text "Hello") w=h=0xfffffffe` = AUTO/UNSIZED → sizing bailed before measuring text → `LayoutError::InvalidTree`. Cause #1 = **empty font chain**: `serif chain css_fallback_fonts=0 unicode_fallbacks=0` though `resolve_char(H)=FONT 0x1`, `fc_cache.len()=1`. The WEB-LIFT last-resort that appends a fallback font to empty chains was (a) ABSENT from the fast path (`resolve_font_chains_fast`@getters.rs:4071 returned with no last resort) and (b) in the legacy path used `resolved.chains.values_mut()` + `pattern.unicode_ranges.clone()` — both lift-fragile (in-place values_mut mutation drop + Vec-clone drop). Cause #2 = `missing_block count=13` (ring: `__rust_dealloc`×5, `get_ua_property`=Task#2 UA-CSS jump-table, `CssVec::drop`) during sizing — not fatal (no remill_error) but suspicious.

**g73 FIX (in test, cargo-check clean):** getters.rs — new `ensure_chains_nonempty(resolved, fc_cache)` helper that REBUILDS the chain map (explicit for-loop, NO values_mut) appending a `unicode_ranges: Vec::new()` fallback (Vec::new is the convention already used across this file; avoids the clone). Applied before BOTH returns (fast @4071 + legacy @~4094). Also changed the internal `resolve_font_chains_with_registry` last-resort to `Vec::new()`. Cycle g73 relifting; checking whether n1 sizes (w != 0xfffffffe) and LayoutError clears. If chain still empty → the bug is deeper (Vec::push of FontMatch mis-lifts) → trace shape path / load_missing_for_chains. If n1 sizes but InvalidTree persists → it's the missing_block/get_ua_property path (Task#2).

## ★★★★ 2026-06-05 g72 — CORRECTION: the "new_tree 2→0 corruption" (g50–g71) was a MISDIAGNOSIS. It's the INTENTIONAL panic@0x90.

The whole g50–g71 marker-bisection arc chased a phantom "new_tree.nodes 2→0 corruption right before
the calculate_intrinsic_sizes call." **There is no corruption.** Native disasm of `layout_document`
(`otool -tV libazul.dylib`, fn @0x9f7a68) proves it cold:

```
0x9fa1e4  str  w8,[x20,#0x84]   ; afterSpan marker (0x60788) = new_tree.nodes.len() = 2  ✓ (tree FINE)
0x9fa1e8  mov  w8,#0x90 ; str w8,[x20]   ; the 0x90 marker (0x60704)
0x9fa1f0  brk  #0x1            ; <-- ABORT. panic@0x90 lowered to a bare `brk` by panic_immediate_abort.
                               ;     calculate_intrinsic_sizes, the beforeCall markers, the 0x91 marker:
                               ;     ALL dead-code-eliminated after the abort. NO `bl calculate_intrinsic_sizes`
                               ;     exists ANYWHERE in the function.
```

So `beforeCall(0x60748)=0` was never a corrupted tree — that marker store is **after the brk**, so it
NEVER EXECUTES; the harness just read uninitialized free-band memory (0). afterSpan=2 (the last marker
before the brk) is the truth: the tree is intact with 2 nodes right up to the abort.

**Two prior mistakes corrected:**
1. The "web_lift is OFF" claim (and the "string absent ⇒ feature off" check) was WRONG. `web-transpiler =
   [..., "azul-layout?/web_lift"]` (dll/Cargo.toml:651) DOES enable web_lift. The panic string is absent
   only because `-Z build-std-features=panic_immediate_abort` strips the message and emits a bare `brk`.
2. The panic@0x90 is the **intentional g48 diagnostic** (this doc line ~16/27: "panic!@marker converts a
   HANG→trap"). The g50–g71 summary lost that context and modelled the abort as a lift-level wild store.

**g72 ACTION:** removed the `#[cfg(feature="web_lift")] panic!(...)` at mod.rs:805. Rebuilding+relifting
(cycle g72). EXPECTED next state = ONE OF: (a) text measures (sizing path is now clean post-BTreeMap
work) → big win; or (b) re-exposes the **g47 hang** (hashbrown empty-map infinite loop) inside
`calculate_intrinsic_sizes`/shaping → harness reports HARNESS_HANG_150s → next target is the remaining
hashbrown maps in the sizing/shaping path (BTreeMap-convert, per the systemic fix below). Both = progress.

---
## ★★★ MILESTONE 2026-06-05 ~15:50 — solveLayoutReal RETURNS (no hang!) — the font-block hang saga is RESOLVED

**g48 (font_chain_cache→BTreeMap + a diagnostic panic@0x90) → `[diag] rc=0`, NO HANG, no fatal trap. The body's CSS size is in the compact cache (n0 w=800 h=600 ✓ — cascade+body-sizing work). The multi-hour font-block hang (load_missing / into_fontconfig_chains / set_font_chain_cache, all hashbrown empty-map RawIter loops) is CLEARED by the BTreeMap conversion.**

**The win:** `font_chain_cache` is now a BTreeMap (getters.rs `into_fontconfig_chains`, cache.rs field/inits/set/get/signatures, FontChainKey `+Ord`). BTreeMap has no ctrl-group → immune to the lifted hashbrown empty-static RawIter loop (which is address-sensitive, so per-site is_empty/with_capacity/forget hacks kept failing across address shifts — see ~14:45 below).

**Remaining blockers (concrete, pre-existing, NOT hangs at the font-block level — multi-session):**
1. **EMPTY FONT CHAIN** → `LayoutError::InvalidTree`, text can't shape. `resolve_char('H')=font 0x1 ✓`, `fc_cache.list().len()=1`, a "serif" chain EXISTS but has 0 fonts. The WEB-LIFT last-resort (getters.rs:4094-4106 + window.rs:928-940) appends `fc_cache.list().first()` to empty chains via `resolved.chains.values_mut()` but it doesn't persist — SUSPECT a lifted `values_mut()` in-place-mutation drop, OR `pattern.unicode_ranges.clone()` mis-lift. This is the PART1 "FontNotFound" class, unmasked now that the hangs are gone. FIX likely: rebuild the chain by re-inserting (not mutate-in-place), or make collect_font_stacks return a default stack.
2. **The g47 hang** (full closure, no panic) is in `calculate_intrinsic_sizes`/shaping (after 0x90) — more hashbrown maps / closure-iterators in the layout (mod.rs:769 `child_intrinsics.iter().find(closure)`, the solver caches). BTreeMap-convert them OR the systemic empty-static mirror fix.

**Cleanup pending (REVERT):** panic@0x90 (mod.rs:762), 0x406C0 markers (window.rs), the g4x diagnostic comments.

---
## ★★ LATEST 2026-06-05 ~14:45 (full detail: memory web_flexbox_lift_2026_06_01.md top entries)

**Session arc:** hinting cut (g34) → font PARSES (upem=1000) + RESOLVES (resolve_char('H')=1) → cascade ok (node_count=2) + text "Hello" correct in styled_dom → font-chain hangs being cleared. ALL fixes are KEEP (hinting skip, ManualTableProvider, **registry-skip on web** = app.rs `FcFontRegistry` gated off (fixed a flaky native StLock multithread crash), into_fontconfig_chains for-loop+with_capacity, set_font_chain_cache forget).

**THE SYSTEMIC ROOT (now fully characterized):** the lifted **hashbrown EMPTY-MAP handling** mis-lifts → infinite loop. 3 PROVEN manifestations: empty ITER (`.values/.keys/into_iter` on empty) → `is_empty()` guard; empty INSERT (`reserve_rehash` from cap-0) → `with_capacity(len)`; empty DROP (reassign/scope-end of cap-0 map) → `mem::forget`. NON-empty maps WORK (real heap ctrl, mirrored). Likely cause: hashbrown's NEON `Group` ctrl-scan reads the **un-mirrored static empty ctrl group** (the data-mirror `scan_arm64_adrp_pages`@transpiler_remill.rs:6630 is per-fn-adrp-page-selective; the empty static is likely const-folded → un-scanned → reads garbage → sees "occupied" → loops). ★ The bisect/per-site approach is UNRELIABLE: every panic/marker/edit SHIFTS lifted addresses → the hang MOVES build-to-build (heisenbug); g45 proved with_capacity+forget didn't hold across the address shift.

**NEXT STEP (address-independent durable fix):** HashMap→BTreeMap for the hot maps (BTreeMap has NO ctrl-group / NO empty static / NO NEON scan → IMMUNE regardless of addresses; the m12_7 `dom_to_layout` pattern). FcWeight derives Ord ✓. CONVERT: getters.rs `ResolvedFontChains.chains`@3423 (+ all builders @4034/4136/3912/3933/4055/4130) + `into_fontconfig_chains`@3459 (return+build BTreeMap, drop with_capacity); cache.rs `font_chain_cache` field@540,708 + inits@566,591,737,755,775 + set@806/819 (drop the forget hack) + merge@842 + get@850 + chains_map closure-collect@657 + signatures@5599,5768,6392; FontChainKey@241 & FontChainKeyOrRef@254 `+ PartialOrd, Ord`. Use fully-qualified `std::collections::BTreeMap` (cache.rs has no direct HashMap `use`). **THIS IS MULTI-SESSION** — the layout + shaping have many more hashbrown maps; text-measuring needs BTreeMap applied broadly OR the systemic lift fix (mirror the empty static / hashbrown NEON Group).

**Iteration:** `/tmp/cycle.sh <gen>` = kill→build azul-dll→relink→relift→harness(150s cap)→grep. Server stays up after lift → harness re-runs w/o relift. Markers: `0x40704` layout-fn phase (0x82=collect_and_resolve done, 0x84=load_missing done, 0x85=font-block done, 0x90/0xA0=sizing, 0x40700=shaping glyph count). `panic!@marker` converts a HANG→trap (but shifts addresses). All diagnostic panics/markers (0x406C0, the g4x panics) are REVERT-at-cleanup. macOS abort → `ls -t ~/Library/Logs/DiagnosticReports/web-text-min*` for the per-thread crash backtrace.

---
## ★ UPDATE 2026-06-05 (supersedes the "Current state" section below)

**HINTING FIX LANDED → past glyph-decode → new blocker = a font-chain panic + OUTLINED_FUNCTION_2 abort-spin.**

- **Root cause of the g33 "stall"** = the lift pulled in the WHOLE allsorts TrueType hinting **Interpreter** (`Interpreter::dispatch` + ~700 `op_*`), via `HintInstance::new`@font.rs:1193 (font parse) + `get_hinted_advance_px`@font.rs:1915 (called PER-GLYPH in measure, text3/default.rs:942-944). NOT the g33 cross-call edits (those are fine, KEPT).
- **FIX (g34, KEEP):** `web_lift` cfg-skips hinting at BOTH sites — web never rasterizes → hinting unused (and lower-quality output); INDEPENDENT of lift correctness. Confirmed: 0 hinting fns in the closure (was ~700), faster lift. cargo check web_lift+native = clean.
- **RESULT:** trap MOVED FORWARD. Font now PARSES (font-precheck upem=1000 asc=918 desc=-335) + RESOLVES (resolve_char('H')=1 nfonts=1). New trap = a **PANIC** in the font-chain last-resort setup (window.rs:921-953, `0x40704=0x82`, pre-shaping) whose **abort path SPINS in `OUTLINED_FUNCTION_2`** (missing_block ring `CssVec::drop(0xfca414) → OF2(0xedef0c) → panic(0x4c7d7c)`, mapped via `/tmp/map_ring.py`, dylib synth→canon offset 0x109214000). Both `CssVec::drop` AND the failing OF2 ARE lifted+cased → so it's a panic (likely a mis-lifted Vec/slice → `slice_end_index_len_fail`) + OF2 abort-spin, NOT an unlifted fn.
- **IN FLIGHT (g35, task bro5l2rf4):** fine `0x406C0` markers in the window.rs:928-953 last-resort loop to pinpoint the panicking op (in-loop `total` sum / `fc_cache.list()` / `unicode_ranges.clone()` / the nchains sum). **Server STAYS UP after lift → re-run harness WITHOUT relift.**

**Strategy (user steer 2026-06-05: fix SOURCE + LIFT independently):**
- A transpiler/lift fix rebuilds ONLY azul-dll (~3-5min, binary byte-identical → ZERO regression risk to the hard-won font-parse/cascade) → **PREFERRED** for the lift bug.
- Disabling the LLVM machine outliner (RUSTFLAGS `-C llvm-args=-enable-machine-outliner=never`) would kill the whole `OUTLINED_FUNCTION_*` class but rebuilds STD (~30min) + shifts all addresses (high regression risk) → last resort.
- `build_extra_data`@transpiler_remill.rs:6707 devirts adrp jump-tables (LDRB/LDRH) but NOT OF2 register tail-jumps. Force-enqueue of SP-restoring OUTLINED epilogues is @transpiler_remill.rs:1616-1713 ("21→7 missing_block state").
- The build command below is unchanged (g34's font.rs cfg needs no flag change). Heartbeat cron active (10-min).

---

This session's arc: **the multi-session "web layout hangs/OOBs" blocker was root-caused and is largely fixed.** The lifted layout now gets *far* past where it used to die. What remains is a bounded, well-localized glyph-decode jump-table cleanup.

---

## TL;DR — what we achieved

1. **★ ROOT CAUSE of the "infinite hang" found + fixed: a std queue-`RwLock` spinning forever in `lock_contended`** in single-threaded lifted wasm (it waits for another thread to release/unpark — which never exists). This was *not* shaping, gsub, or sizing as previously theorized.
2. **Only 3 RwLock instances exist** in the lifted crates (`std::sync::Mutex::lock` is `class=Leaf` = stub-returns, fine). All 3 bypassed with a no-atomic single-threaded `StLock<T>`.
3. **Result, verified:** `lock_contended` pulled **0×** (was 10), the hang is gone, and `solveLayoutReal` now reaches: **font resolution works** (`resolve_char('H')=1, nfonts=1`), font chains resolve (`layout-fn=0x82`), and execution enters **`layout_dom_recursive`** (`step=0x70`).
4. **Next blocker localized + being cleared:** un-devirt'd jump tables in the lifted allsorts glyph-decode path. Fixed `into_outlines` (glyph outline decode — skipped on web; never rasterizes) and the 3 horizontal `glyph_info::advance` callers (→ direct hmtx read). Each removed OOB shifts to the next allsorts ReadArray caller.

All fixes are **cargo-check-clean (both `web_lift` and native configs)** and are KEEPERS.

---

## The fixes (all KEEP) — files + rationale

### 1. RwLock spin → `StLock<T>` (the breakthrough) — KEEP
The queue `RwLock`'s `lock_contended` is pure-Rust, gets lifted (`Recursable`), and spins forever in single-threaded wasm. `StLock<T>` is a bare `UnsafeCell` (no atomics) whose `read()/write()/lock()` return a guard immediately — sound because the lifted backend is single-threaded.

- **`rust-fontconfig-azul/src/lib.rs`**: `pub struct StLock<T>` + `StReadGuard`/`StWriteGuard` (Deref/DerefMut) defined; `FcFontCacheShared.state` (RwLock), `.chain_cache` + `.shared_bytes` (Mutex) → `StLock`; `state_read`/`state_write` accessors updated.
- **`rust-fontconfig-azul/src/registry.rs`**: `FcFontRegistry.known_paths` (RwLock) → `crate::StLock`.
- **`azul-mobile/layout/src/font.rs`**: `ParsedFont.glyph_cache` (`Arc<RwLock<…>>`) → `Arc<rust_fontconfig::StLock<…>>`.

**Proper follow-up (not blocking):** feature-gate `StLock` so native azul keeps real `std::sync` locks (native is multi-threaded; `StLock` is single-thread-only). For now it's unconditional and works because the server's native font setup is single-threaded.

### 2. Glyph outline + hinting decode skipped on web — KEEP
**`layout/src/font.rs` `decode_glyph_inner` (~1710–1774):** both decode passes (allsorts `GlyfVisitorContext::visit` + `GlyphOutlineCollector::into_outlines`, and the TrueType hinting raw-points pass) wrapped in `if !cfg!(feature = "web_lift") { … }`. The lifted web layout never rasterizes (measures + positions → ships a display list to JS), so outlines/hinting are never needed; measure uses only hmtx advances. The `GlyphOutlineOperation` 5-arm `match` was an un-devirt'd jump table → OOB. Confirmed DCE'd (`into_outlines` 0× lifted on web).

### 3. `glyph_info::advance` → direct hmtx read on web — KEEP
**`layout/src/font.rs` `get_horizontal_advance` (~1883):** on `web_lift`, reads the hmtx longHorMetric directly (`hmtx[min(gid, num_h_metrics-1)*4 ..+2]` as u16 BE) instead of `allsorts::glyph_info::advance` (whose lifted binary `ReadArray` parse has an un-devirt'd jump table → OOB). Native keeps the allsorts path. The other two horizontal callers — `decode_glyph_inner` (~1660) and `get_space_width_internal` (~1857) — now call `get_horizontal_advance` (so they inherit the web/native split; **native behavior unchanged**).

### 4. Prior keepers (verified, from earlier in the arc)
- **Dispatcher fix** (`dll/src/web/transpiler_remill.rs`): indirect-call dispatcher routes to the dep's real `@__az_dep_<addr>` export (was a silent no-op for tail-calls/fn-ptrs). Fixes `copy_from_bytes` and similar.
- **`__multi3`** leaked libcall import (`dll/src/web/loader_js.rs` + harness).
- **AzString correctness** (`css/src/corety.rs`): `#[inline(always)]` chain + `core::str::from_utf8` fast path → web text `AzString` is correct end-to-end ("Hello", len=5).
- **`needs_scan` guard** (`layout/src/text3/cache.rs` boundary-gen): skip the scan_cursor walk for plain text (no overrides / no text-combine-upright) — perf + avoids a mis-lifting walk.

---

## Current state / what's left

**⚠ IMPORTANT — build state caveat (verify this first):**
- **g32 state** = RwLock `StLock` + `into_outlines` web skip + `get_horizontal_advance` direct-hmtx. **This LIFTS** (~7 min) and reaches the harness; it confirmed the OOB then moves to the *other* `glyph_info::advance` callers (`decode_glyph_inner` ~1660, `get_space_width_internal` ~1857).
- **g33 state (current tree)** = g32 **plus** routing those 2 callers through `get_horizontal_advance`. With these 2 edits the **lift does NOT complete** — it ran >12–16 min without reaching READY across multiple attempts in both loaded *and* recovered (load 1.4, ~2.8 GB free) environments. The foreground run shows it *is* lifting (reaches `transitive[174]+`) but never finishes — a ~2× slowdown/stall vs g32. **This is unverified and suspect.**
- **Recommended next move:** either (a) give the g33 lift more time / run it isolated and confirm whether it eventually completes and text measures; or (b) **revert the last 2 edits** (the `decode_glyph_inner:1660` and `get_space_width_internal:1857` → `self.get_horizontal_advance(…)` changes) back to the lifting g32 state, then re-approach those 2 callers with an *inline* direct-hmtx read (matching `get_horizontal_advance`'s `web_lift` body) rather than a cross-method call — in case the added call edge is what bloats/stalls the lift. The `git diff` for `layout/src/font.rs` shows exactly these two hunks.

The RwLock fix, `into_outlines` skip, and `get_horizontal_advance` direct-hmtx (the g32 set) are all solid and lift cleanly; only the final 2-caller routing is unconfirmed.

**Immediate next step (fresh env):**
1. Re-run the lift (binary is already built) and check whether text **MEASURES** (harness should show `glyphs>0` / a text rect with `h>0`, no `TRAPPED`).
2. If the OOB **moves again** to `glyph_info::advance` site ~2105 (or a vertical/vmtx path actually reached), apply the same direct-table read.
3. If a *different* allsorts function OOBs, it's the same class — see the systemic option below.

**Systemic root (the real general fix):** every OOB ring shares **`OUTLINED_FUNCTION_2`** (a size-8 machine-outlined tail-jump) as the recurring un-devirt'd target. The transpiler already has `OUTLINED_FUNCTION`/`JT_SEEDS` handling (`transpiler_remill.rs:1650–1699`, "21→7 missing_block state") — finishing the devirt of `OUTLINED_FUNCTION_2`'s computed tail-jump would fix the whole class at once (vs. per-caller hmtx workarounds). The per-caller fixes are correct + web-appropriate regardless.

**After text measures:** the goal continues — text height propagation → `hello-world.c` (counter text + button + `on_click` RefreshDom).

---

## Build / run / verify (canonical)

```bash
cd /Users/fschutt/Development/azul-mobile

# BUILD (build-std + web features; web_lift comes via web-transpiler)
RUSTC_BOOTSTRAP=1 \
RUSTFLAGS="-C target-feature=-lse,-rcpc,-rcpc2,-rcpc3 -C llvm-args=-aarch64-enable-ldst-opt=0" \
  cargo build -p azul-dll --release \
  --features "build-dll web web-transpiler web-transpiler-static" --no-default-features \
  -Z build-std=std,panic_abort -Z build-std-features=panic_immediate_abort \
  --target aarch64-apple-darwin
DYLDIR=target/aarch64-apple-darwin/release
cp "$DYLDIR/libazul.dylib" "$DYLDIR/deps/libazul.dylib"

# FAST fail-check first (catches errors in ~1 min, both configs):
cargo check -p azul-layout --no-default-features --features "std,text_layout,font_loading,web_lift"

# LINK the C example:
clang examples/c/web-text-min.c -L$DYLDIR -lazul -Iexamples/c -fno-stack-protector -o examples/c/web-text-min.bin

# RUN the server (lift ~7 min in a healthy env):
DYLD_LIBRARY_PATH=$DYLDIR \
  REMILL_LIFT_BIN=/Users/fschutt/Development/azul/third_party/remill-install/build/remill/bin/lift/remill-lift-17 \
  AZ_BACKEND=web://127.0.0.1:8800 AZ_NO_LIFT_CACHE=1 \
  nohup ./examples/c/web-text-min.bin > /tmp/server.log 2>&1 &

# HARNESS:
AZ_LENIENT=1 node scripts/m9_e2e/layout-flexbox.js
```

---

## ★ Operational lessons (these cost real time — heed them)

1. **Poll the PORT, never `kill -0` the launch pid.** The server's parent process forks+exits while the child lifts; a `kill -0 $SVPID` watchdog reports a **false "SERVER DIED"** even though the server is alive (log is block-buffered, so it shows only up to "Classified 2654…" until flush). Use `curl 127.0.0.1:8800`. Several "deaths" this session were this artifact, not crashes.
2. **The only genuine startup crashes** were from raw-address `write_volatile` markers (`0x40xxx`) placed in code reached *natively* (font parse runs in both wasm and native `render_initial_page`). Such markers SEGV the native server (those addresses are unmapped natively). Markers are only safe in **wasm-only** functions (e.g. `shape_text_internal`, which native render never reaches because it's cascade-only). To instrument `from_provider`-style dual-path code, use **native-safe log forensics** (map the harness missing_block/fn-call-ring PCs to function ranges via the server log; `base = canon_of_OUTLINED_FUNCTION_2 − its_synth`) instead of memory markers.
3. **`AZ_WASM_DEBUG=1` crashes this build** post-classification — don't use it for named traces; use the canonical-addr mapping above.
4. **Clean up between runs:** `ps -axo pid,command | grep -E 'remill|web-text-min' | grep -v grep | awk '{print $1}' | xargs kill -9; lsof -ti tcp:8800 | xargs kill -9`. Orphaned `remill-lift-17` workers accumulate and starve later lifts (lift time crept from ~7 min to >16 min after ~15 runs).
5. **zsh:** use `$pipestatus[1]`, not `$PIPESTATUS`.

---

## Pointers
- Full running notes: `~/.claude/.../memory/web_flexbox_lift_2026_06_01.md` (latest entries at top, ~21:15 onward cover this arc).
- Harness: `scripts/m9_e2e/layout-flexbox.js` (has diagnostic POST-TRAP marker reads; some are revert-at-cleanup).
- Changed files this session: `rust-fontconfig-azul/src/{lib,registry}.rs`, `layout/src/font.rs`, `layout/src/text3/{cache,default}.rs`, `dll/src/web/{transpiler_remill,loader_js}.rs`, `css/src/corety.rs`.

---

## 2026-06-05 — missing_block ring FULLY DECODED + InvalidTree localized (g50/g51)

### Headline
`solveLayoutReal` RETURNS (rc=0) but yields `LayoutError::InvalidTree` for web-text-min.
The **entire missing_block ring is now decoded to named functions**, and `node_count=2` is
confirmed **correct** (web-text-min = body + 1 Text("Hello"); body sizes 800×600 exactly).
The blocker is no longer a mystery — it's two specific lift bugs (below).

### The machine-outliner was a RED HERRING (ruled out)
g50 = full std rebuild with `-C llvm-args=-enable-machine-outliner=never`. OUTLINED count
dropped 8048→76 (flag works; the 76 remainder are build-std/prebuilt bits the llvm-arg
didn't reach), **but InvalidTree persisted unchanged**. So the outliner is NOT the cause of
InvalidTree or the hashbrown empty-static class. Do not pursue outliner-disable as a fix.
(The flag is harmless extra and is kept ONLY for fast-incremental iteration via
`/tmp/cycle_g50.sh`; revert it for the final clean build — see `/tmp/cycle.sh` = baseline.)

### Method: map a synthetic missing_block PC → function name OFFLINE (no rebuild)
The slide cancels in the synth formula, so given the server log's rebase dump you can map
any synth PC against `nm -n libazul.dylib`:
- From log: `[symbol_table] ... libazul.dylib → synth_base=0x110000, native=[0x1095b8000..]`.
- `slid_addr = native_base + (synth − synth_base)`  (e.g. synth 0xe8e5cc → 0x10a3365cc).
- `slide = slid_canonical − nm_addr` for any known symbol (get_ua_property: 0x10a3365cc −
  nm 0xd7e5cc = slide 0x1095b8000).
- `unslid = slid_addr − slide`; then `nm -n` + bisect for the containing symbol.

### The ring (`0xf62f8c 0x4cd6f0 0x4cd6f0 0xe8e5cc 0xe8e5cc 0x4e90200 0x4cd6f0 0x3ad62cc`)
| synth | function | nature |
|---|---|---|
| 0x4cd6f0 ×3 (HOT) | `__rust_dealloc` (v0-mangled `_RNvCs…___rust_dealloc`) | reached via **tail-call `b`** not intercepted `bl` → missing_block. BumpDealloc classify matches by name, but tail-`b` to it isn't intercepted. **Benign leak, not corruption.** |
| 0x3ad62cc (LAST, right before InvalidTree) | `alloc::vec::Vec::split_off::assert_failed` | a `split_off(at>len)` panic path. NOT in azul source (dep/inlined monomorph). Either a real OOB from upstream garbage or a mis-lifted `at>len` compare. |
| 0x4e90200 | `llvm::WebAssemblyDescs` (DATA section, +55504) | **garbage** computed-branch target = a jump-table devirt miss (a `br Xn` landed in LLVM static data). |
| 0xe8e5cc ×2 | `azul_core::ua_css::get_ua_property` | the systemic 179-variant CssProperty jump-table. Pulled in by every getter (display/overflow/position/float/writing_mode/font_weight) + apply_ua_css. |
| 0xf62f8c | `<azul_css::css::CssVec as Drop>::drop` | Drop glue (pulled in by DomVec::drop). |

### node_count=2 is CORRECT (was misread as "lost nodes")
web-text-min.c = `body{w800,h600,fontSize16}` + 1 `Text("Hello")` child = 2 nodes. The
harness "expected 5" is a generic flexbox-example constant, not applicable here. compact
cache: n0 body w=0xc35000=800×16000 h=0x927c00=600×16000 disp=0; n1 text w=h=AUTO disp=1.
**Body sizing + cascade structure are correct.**

### Two real blockers (both downstream of the cascade, both lift bugs)
1. **InvalidTree in the reconcile→pre-0x90 path** (the FIRST failure; returns BEFORE intrinsic
   sizing). Ruled out: cold-fast-path (mod.rs:708, guarded by `cache.tree.is_some()`),
   reconcile_recursive's only InvalidTree (cache.rs:1061, guarded by `|| old_tree.is_none()`),
   prepare_layout_context (cache.rs:1416, runs post-0x90). So it's in mod.rs 521–744
   (counters / cache_map remap / dirty+layout_roots loops / early-exit). **g51 adds bisection
   markers 0x71 (post-reconcile), 0x72 (post-counters), 0x80 (loop body) to localize it —
   the harness's last 0x40704 value names the segment.**
2. **Text AzString read from the wrong offset.** The string is boxed CORRECTLY at node+16
   (`BoxOrStatic → {ptr,len=5}` "Hello" ✓), but the shaping path reads garbage inline
   (`ptr=0 len=100673880`) → 0 glyphs. This is the NodeType::Text(AzString) storage/deref
   mis-lift in StyledDom::create's data-variant copy (ring's CssVec/DomVec::drop are adjacent).

### Build state / iteration
- g50 dylib (outliner-disabled) is in `deps/`. `/tmp/cycle_g50.sh <gen>` = fast incremental
  (keeps build-std cached). `/tmp/cycle.sh <gen>` = baseline RUSTFLAGS (30-min std rebuild).
- g51 (running): mod.rs bisection markers, incremental on g50 RUSTFLAGS.
- REVERT-at-cleanup: mod.rs markers 0x71/0x72/0x80 + panic@0x90 (mod.rs:768); outliner flag.

---

## 2026-06-05 (cont.) — ★★ web_lift is OFF + InvalidTree is INSIDE calculate_intrinsic_recursive (g51/g52)

### ★★ MAJOR: the `web_lift` feature has been OFF the whole time
The build (`--features "build-dll web web-transpiler web-transpiler-static"`) does NOT actually
enable `azul-layout/web_lift`. Proof: the `#[cfg(feature="web_lift")] panic!("reached 0x90")` at
mod.rs:768 does NOT fire (harness reaches 0x40704=0x90 then proceeds), and the panic string is
ABSENT from the compiled dylib (`strings libazul.dylib | grep "reached 0x90"` = 0).
- Cause: `web-transpiler = ["web", ..., "azul-layout?/web_lift"]` (dll/Cargo.toml:649). The WEAK
  `azul-layout?/web_lift` isn't propagating (azul-layout is `optional=true`, enabled by "web", but
  the weak-feature ref doesn't fire under this build/`-Z build-std`). To force it on: change to the
  STRONG `azul-layout/web_lift`.
- Blast radius of web_lift is SMALL — only `layout/src/font.rs` (10 sites, all hinting) + the
  mod.rs:768 panic. So the hinting-skip (g34) has been INACTIVE, but it only matters at SHAPING
  (not yet reached). The progress to date came from the UNCONDITIONAL fixes (BTreeMap conversions),
  NOT the web_lift-gated ones. **Do NOT assume web_lift gates the reconcile/sizing path — it does not.**
- Implication: web_lift being off is currently HARMLESS (even helpful — it let us pass the 0x90
  panic into sizing). Enable it (+ remove the panic) only when reaching shaping needs hinting-skip.

### InvalidTree is INSIDE calculate_intrinsic_sizes (sizing.rs), NOT in reconcile
Bisection via 0x40704 markers (mod.rs 0x71/0x72/0x80/0x90 — harness reads them now; JS-only edit,
re-runnable against the LIVE server with no rebuild):
- `0x40704=0x90` (reached calc_intrinsic) but NOT `0x91` (its post-marker) → InvalidTree fires
  INSIDE `calculate_intrinsic_sizes` → `calculate_intrinsic_recursive` (sizing.rs:128/176).
- `0x4071C=0` (inline-phase) → `collect_inline_content_recursive` NEVER reached → InvalidTree is
  in the recursion BEFORE inline collection: line 226 (`tree.get(node_index).ok_or(InvalidTree)`)
  for a STRAY child_index, or the node sizers (330/633/748). node_count=2, body sizes 800×600.
- Hypothesis (matches the documented 1073 bug): reconcile mis-lists a stray/out-of-range
  child_index; the recursion loop at sizing.rs:266 is UNGUARDED (unlike process_layout_children:1079)
  → recurse(stray) → tree.get(stray)=None → InvalidTree@226.

### g52 (running): stray-child guard + pinpoint markers
- FIX: guard at sizing.rs:266 — `if tree.get(child_index).is_none() { continue; }` (mirrors :1079).
- Markers: 0x40720=last node_index entering calculate_intrinsic_recursive; 0x40728=last child_index
  recursed (expect OUT-OF-RANGE ≥2 if stray). Harness reads both + inline-phase.
- Expected if hypothesis holds: inline-phase advances 0→B1/B2, text.len@0x40714 → 5 (or GARBAGE =
  the 2nd blocker: NodeType::Text(AzString) deref mis-lift). The split_off::assert_failed
  missing_block (ring lastPC) fires DURING sizing — likely the get_ua_property jump-table misfiring
  into random function addresses (0x4e90200=llvm data, 0x3ad62cc=split_off::assert_failed).

### Fast-iteration tooling (this session)
- `/tmp/cycle_g50.sh <gen>` = incremental build (keeps outliner-disabled build-std cached) → ~1min
  compile + ~3min lift (cached) + harness. Much faster than baseline's 30-min std rebuild.
- The lifted layout fns are wasm-only at runtime (server SSR uses create_from_dom/cascade, not
  solveLayoutReal), so raw 0x40xxx markers in layout_document/sizing are SAFE (don't SEGV the server).
- HUGE time-saver: marker reads are JS-only (harness AzStartup_peekU32) — add a read + re-run
  `node scripts/m9_e2e/layout-flexbox.js` against the LIVE server, no rebuild, for any 0x40xxx marker
  already in the lifted code (e.g. the pre-existing 0x4071C/0x40714 in collect_inline_content_recursive).

### g52 RESULT + g53 (running): InvalidTree is `tree.get(root)=None` (BROKEN reconcile tree)
g52 (stray-child guard @sizing.rs:266) did NOT fix it. Markers: last node_index(0x40720)=0,
last child_index(0x40728)=0 → calculate_intrinsic_recursive entered ONLY for node 0 and NEVER
recursed into a child. calculate_ifc_root (0xA0) + inline-phase (0x4071C) both NOT reached.
The ONLY InvalidTree reachable with these markers is **line 229 itself**: `tree.get(tree.root=0)`
returns **None** → the lifted LayoutTree's `nodes` array is empty/broken even though node_count
(styledDOM)=2 and recon_result.intrinsic_dirty is non-empty (else calc_intrinsic_sizes early-returns Ok).
→ **ROOT CAUSE = reconcile_and_invalidate produced a broken LayoutTree (lifted)**, NOT a sizing bug.
g53 (task b7tbzorms): decisive probe at sizing.rs:128 — 0x40730=tree.root, 0x40734=tree.nodes.len(),
0x40738=tree.get(root).is_some(). If len=0 or is_some=0 → confirmed broken tree → next target is
reconcile_recursive / create_node_from_dom (cache.rs) — why the lifted reconcile builds 0 usable
LayoutNodes. (The text node being "orphaned" is the same root: reconcile didn't build the tree.)
REVERT: sizing.rs 0x40720/0x40728/0x40730/0x40734/0x40738 markers (KEEP :266 guard); harness BISECT reads.

### ★★★ ROOT CAUSE FOUND (g54): `&mut new_tree` mis-lifts at the calculate_intrinsic_sizes call
The InvalidTree is NOT reconcile, NOT sizing logic, NOT a stray child. The LIFTED layout tree is
built CORRECTLY — the `&mut new_tree` REFERENCE is mis-lifted when passed to calculate_intrinsic_sizes.
PROOF (all from g54 markers, web-text-min):
- create_node_from_dom callCount(0x40500)=2, ring=[dom0,dom1] → reconcile created BOTH nodes.
- new_tree.nodes.len() = **2** post-reconcile (0x40740) AND **2** at the layout loop body (0x40744),
  read by-value in mod.rs::layout_document. The tree is intact right up to the call.
- Inside calculate_intrinsic_sizes (sizing.rs:128, via the `&mut tree` param): nodes.len()=**0**,
  get(root)=None (→ InvalidTree@229). tree.root reads 0 (could be correct or coincidental).
- mod.rs:744→781 does NOT mutate new_tree (only clone_calculated_positions, which clones a different
  field). So the only thing between len=2 and len=0 is the `calculate_intrinsic_sizes(&mut new_tree,…)` call.
- This is SPECIFIC to this call: reconcile's `&mut new_tree_builder`, the cache_map remap's
  `&new_tree.nodes`, and mark_dirty all work. So `&mut LayoutTree` passing is NOT universally broken.
→ The lifted `calculate_intrinsic_sizes(&mut ctx, &mut new_tree, text_cache, &dirty)` call mis-passes
  the 2nd arg (tree) — the callee sees an empty/wrong LayoutTree. (4 args, 3 of them &mut, generic <T>.)
g55 (task b8tp2f8ui): probe caller `&new_tree` addr (0x40748) vs callee `tree` ptr (0x4075C).
  SAME ptr → nodes-field-offset mis-lift in callee; DIFFERENT → the &mut arg is mis-passed.
LIKELY FIXES to try (Task#1 unblock): (a) #[inline(always)] on calculate_intrinsic_sizes (kills the
  call boundary — but recursion into calculate_intrinsic_recursive may re-trigger it); (b) Box the
  LayoutTree so the &mut is a HEAP ptr (heap lifts reliably per M8.4) not a stack ptr; (c) reduce/reorder
  args. REAL fix (Task#2) = the transpiler's arg/reference passing for this call shape. This is the
  SINGLE blocker for web-text-min text rendering — everything upstream (cascade, reconcile, tree build) works.
REVERT: mod.rs 0x40748/0x4074C probes; sizing.rs 0x4075C probe; all the g51-g55 markers + harness BISECT reads.

### g56 RESULT: Box did NOT fix it → it's an ARGUMENT-TRANSFER mis-lift (not stack-vs-heap)
Boxed the LayoutTree (heap). g56 callee STILL sees tree ptr(0x4075C)=0x0, nodes.len(0x40734)=0,
while caller has nodes.len=2 (0x40740/0x40744). So the `&mut LayoutTree` arg arrives NULL in
calculate_intrinsic_sizes regardless of Box. NOT an SROA/stack-address issue.
- Refined root cause: the lifted call `calculate_intrinsic_sizes(&mut ctx, &mut new_tree, text_cache,
  &dirty)` does NOT transfer arg2 (the `&mut LayoutTree`). The callee's `tree` param = 0 (null).
  Reads of tree.root/nodes.len/get all return 0 (reading wasm low-memory at addr 0).
- It is SPECIFIC to the `&mut LayoutTree` arg: node_index (x2 in the recursion, marker 0x40720=0=correct)
  and dirty_nodes (x3, non-empty → passed the is_empty check) transfer FINE. So NOT all args; just tree.
- (g55's `&new_tree`=0x0 probe was a HEISENBUG — address-taking perturbed codegen; ignore that run's
  caller-len=1. The reliable signal is g56's clean callee tree=0.)
- The Box change (mod.rs:519 Box::new + :1077 `Some(*new_tree)`) is a NO-OP for the bug → REVERT candidate.

### NEXT-SESSION fix options for the arg-transfer mis-lift (THE single remaining blocker for web-text-min)
1. **Transpiler (Task #2, the real fix):** find why the lifted `bl calculate_intrinsic_sizes` drops the
   2nd arg (the &mut LayoutTree). Is it x1-specific, or &mut-vs-& (dirty is `&`, works; tree is `&mut`)?
   Confirm with a self(x0)/tree(x1) ptr probe at calculate_intrinsic_recursive entry (sizing.rs:~178).
   Likely related to the M12 complex-struct-passing class (self(X0)-relative / sret routing).
2. **Source workaround — make the sizing fns NOT take `&mut LayoutTree` as arg2:** e.g. store the tree
   ref in IntrinsicSizeCalculator (accessed via self=x0, which transfers OK) — but watch borrow-checker
   (can't borrow self.tree mutably while calling self.method()). Or pass tree as the FIRST arg of free
   functions. Or a `_pad: usize` dummy arg before tree to shift it off x1 (hacky but minimal test).
3. Quick A/B test: does `&` (shared) transfer when `&mut` doesn't? If yes → the bug is &mut-specific.

### ★ CORRECTION (g58): pointer-cast probes mis-lift to 0 → the REAL bug is address-computation/SP
g58 probed all 4 args of calculate_intrinsic_sizes at entry via `x as *const _ as usize`:
ctx/x0=0, tree/x1=0, text_cache/x2=0, dirty/x3=0 — ALL zero, including ctx (definitely valid)
and dirty (definitely valid — the fn passes its `dirty.is_empty()` check and proceeds). So:
- **`ref as *const _ as usize` (address-as-int) MIS-LIFTS TO 0** in the lifted code. Therefore
  ALL pointer-cast probes are INVALID: g55 (`&new_tree`=0), g57 (self/tree — also collided with
  cb markers 0x40760-68), g58 (4-arg). Ignore their pointer values.
- The RELIABLE signal is the FIELD-ACCESS `tree.nodes.len()` (not a cast): **2 in mod.rs
  (0x40740/0x40744) → 0 inside calculate_intrinsic_sizes (0x40734)**. The tree arg genuinely
  arrives empty/wrong in the callee. THIS still stands.
- REFRAME: the bug is an ADDRESS-COMPUTATION / stack-pointer lift bug, NOT "arg2 dropped".
  Taking `&mut new_tree` (a stack-local address, SP-relative) lifts to garbage/0 → the callee gets
  a bad tree. Box didn't help, and NO reference-based source fix will (they all need address-of).
- KEY: the `&mut new_tree` address-of happens ONCE in layout_document; the &mut then THREADS
  through calc_intrinsic_sizes → recursion → all sizers WITHOUT re-taking an address. So the fix
  scope is SMALL — make that single `&mut new_tree` (or the SP at that point) correct.
- LEADING HYPOTHESIS (ties to memory's M12 AZ_FIX_SP / "CssProperty::clone leaks guest SP"): a
  preceding lifted call in layout_document (reconcile, or get_ua_property's 179-variant jump-table —
  it's in the missing_block ring) CORRUPTS the guest SP, so the subsequent SP-relative `&mut new_tree`
  computes wrong. => TASK #2 (transpiler SP-preservation / address-of) is likely the REAL fix.

### NEXT-SESSION plan (the LAST blocker for web-text-min; everything upstream works)
1. Confirm the SP/address-of hypothesis WITHOUT casts: in mod.rs, write a sentinel into a tree FIELD
   before the call (e.g. read new_tree.children_offsets.len() — should be 2) and read the SAME field
   inside calc_intrinsic_sizes; if it also reads 0 while nodes.len does → whole-tree ptr wrong (SP).
2. Investigate transpiler SP preservation (AZ_FIX_SP / enforce_sp_preservation) — is it active for
   this build? Does get_ua_property (jump-table, in the ring) restore SP? Fixing get_ua_property's
   devirt (Task #2) may fix BOTH the missing_blocks AND the SP corruption → the tree transfer.
3. Cheap empirical test: move the `let (new_tree,..) = reconcile()` + the calc_intrinsic_sizes call
   into a SEPARATE small #[inline(never)] fn (fresh stack frame, no preceding SP-corrupting calls) —
   if the tree transfers there, it confirms SP corruption by a preceding call.
REVERT-at-cleanup (all diagnostic, none are fixes): mod.rs Box (519/1077) + 0x40740/0x40744;
sizing.rs 0x40720/0x40728/0x40730-38/0x4075C/0x407B0-BC probes (KEEP the :266 stray-child guard);
all harness BISECT reads; the panic@0x90; the outliner flag in cycle_g50.sh.

### enforce_sp_preservation coverage (transpiler_remill.rs:5210) — for the SP hypothesis
DEFAULT-active (AZ_NO_FIX_SP=1 disables). Wraps every `%res = [tail ]call ptr @sub_<hex>(ptr %state,…)`
by save/restore of X19-X28/FP/SP/D8-D15 from the remill State around the call (parse_sub_call:5263).
So layout_document's DIRECT dep calls (reconcile, compute_counters, calculate_intrinsic_sizes) ARE
wrapped → its SP *should* be preserved when it computes `&mut new_tree`. NOT wrapped: indirect calls
(__az_indirect_dispatch), missing_block (__remill_function_call), void calls w/o `%res =`. So if the
tree-transfer is SP, the leak is from a path enforce_sp_preservation misses — OR it's not SP (arg-pass
/ callee-frame). Pointer probes can't disambiguate (casts→0). NEXT session needs IR/disasm of the
lifted calculate_intrinsic_sizes (how it reads its `tree` arg) or a contained-frame refactor test.
This is genuine TASK #2 (transpiler) territory — the precise root cause is nailed; the fix is deep.

### How to get the lifted IR (next session, NO rebuild — just a relift)
Relaunch the existing web-text-min.bin server with `AZ_REMILL_KEEP_SCRATCH=1 AZ_NO_LIFT_CACHE=1`
(scratch kept in `$TMPDIR/azul-web-transpiler-<pid>/`, per transpiler_remill.rs:607/627). Then read
`<stem>.lifted.ll` / `.patched.ll` for calculate_intrinsic_sizes (find its stem in /tmp/server_*.log:
"transitive[N]: lifting _ZN…calculate_intrinsic_sizes… export_as=__az_dep_<hex>"). Inspect how the
lifted body reads its `tree` arg (remill State struct: args arrive in State GPR fields X0=ctx, X1=tree,
…; State SP=offset 1040). Look for: tree spilled to an SP-relative slot then reloaded from a DRIFTED SP,
or the arg read from the wrong State field. That pinpoints the transpiler fix for the `&mut new_tree`
mis-transfer — THE last blocker for web-text-min text measuring.

### Heartbeat 2 (g59): pivot to a RELIABLE field-access bracket (casts are broken)
The KEEP_SCRATCH relift succeeded — IR is in `$TMPDIR/azul-web-transpiler-66849/` (layout_document =
`__az_dep_10d21840c.{lifted,patched,opt,linked}.ll`, ~2.6MB linked). But IR archaeology is slow:
the sizing fns ("intrinsic") appear NOWHERE in the lift log (1106 fns) — likely inlined — and marker
addresses are computed (not literal 264452) so grep-by-address fails. PIVOTED to a reliable test:
since pointer-CASTS mis-lift to 0 but FIELD ACCESS (`tree.nodes.len()`) is reliable, g59 brackets
the drop with 3 nodes.len reads in calculate_intrinsic_sizes: 0x407B0=entry, 0x407B4=after
compute_dirty_ancestor_closure (returns a HashSet by sret — PRIME SUSPECT for corrupting new_tree's
adjacent stack slot), 0x40734=line 142 (after IntrinsicSizeCalculator::new). Whichever read first
hits 0 names the corrupting op. NEXT: read g59 BISECT "calc_intrinsic_sizes nodes.len" → fix that op
(if compute_dirty_ancestor_closure: it's a HashSet sret — try BTreeSet or inline it; if entry=0: the
call/inline boundary). This is finally a RELIABLE localization after a session of broken-cast noise.

### g59 RESULT (reliable): tree is empty at calc_intrinsic_sizes ENTRY → lost at the CALL boundary
g59 bracket (reliable field-access nodes.len): entry(0x407B0)=0, afterClosure(0x407B4)=0,
line142(0x40734)=0. So tree.nodes.len() is ALREADY 0 at the VERY ENTRY of calculate_intrinsic_sizes
(before compute_dirty_ancestor_closure) — that suspect is CLEARED. The tree (=2 at mod.rs:752,
0x40744) is lost between there and the callee entry — i.e. at the `calculate_intrinsic_sizes(&mut
new_tree,…)` CALL itself (clone_calculated_positions is the only other thing between, and it touches a
different field). This RELIABLY confirms (via field access, not broken casts) the call-boundary /
&mut-LayoutTree-arg-transfer mis-lift. g60 (task b6ha3h5uy): adds before-call read (0x40748, expect 2)
+ #[inline(always)] on calculate_intrinsic_sizes. If 0x407B0 jumps 0→2 → inlining removed the boundary
→ boundary CONFIRMED + calc fixed (recursion calculate_intrinsic_recursive is then the next boundary,
can't inline — needs the &mut-arg transpiler fix or a tree-in-self refactor). If 0x407B0 stays 0 →
fn wasn't inlined / deeper bug.

### g60 RESULT: NOT the call boundary — new_tree is corrupted WITHIN layout_document, BEFORE the call
g60 (added before-call read 0x40748 + inline(always), reliable field access): timeline =
post-reconcile(0x40740)=2, loopBody(0x40744)=2, **beforeCall(0x40748)=0**, entry(0x407B0)=0.
So new_tree.nodes.len() drops 2→0 between mod.rs:759 (loopBody marker) and mod.rs:792 (beforeCall
marker) — INSIDE layout_document, BEFORE calculate_intrinsic_sizes is even called. inline(always)
was irrelevant (reverted). So it's NOT the &mut-arg transfer / call boundary — it's a STACK-SLOT
CORRUPTION of new_tree by an intervening op in that span: the `calculated_positions.clone()`s
(762 pre-loop, 771-774 in-loop) and the `probe::Probe::span(...)` guards (Leaf-stubbed on web,
return a Span by sret) + `reset_peak()`. An sret return-slot or memcpy dest overlapping new_tree's
stack slot (the M12 self(X0)-relative / sret-routing class). g61 (task b3tn0e6u0): brackets each
op (0x4074C after pre-loop clone, 0x407C0 after in-loop clone+Span, 0x407C4 after reset_peak+Span)
→ names the corruptor. THEN fix = remove/restructure that op (e.g. the probe spans are pure
profiling — cfg/stub them out on web; or hoist the clones; or the clone's sret slot needs separating
from new_tree). This is finally a SOURCE-fixable corruption, not a transpiler-only bug.

### g61 = HEISENBUG wall; g62 = PRINCIPLED FIX TEST (disable probe → ZST Span, no sret)
g61 finer bracket: loopBody=2, preClone=2, inLoopClone=2, afterReset=2, beforeCall=0 → "corruption
between afterReset and beforeCall" = only the 0x90 marker write, which is nonsensical → confirms the
HEISENBUG (my dense markers perturb the address-sensitive corruption; fine bisection no longer
converges). The corruption is REAL (InvalidTree predates markers) but resists marker bisection.
★ STRONGEST LEAD: probe IS ON in the web build — dll/Cargo.toml _internal_deps (active via build-dll)
includes "azul-layout?/probe" (line 518, "always-on"). probe.rs: the real `Span` (cfg
all(feature="probe", not(wasm))) has fields `name:&str + start:Instant` → returned BY SRET; the
comment (probe.rs:41) confirms the 17 `Probe::span(..)` guards are INLINED into layout_document. So
new_tree's 2→0 corruption is very likely an sret return-slot (or Instant/TLS spill) overlapping
new_tree's stack slot — the M12 self(X0)-relative/sret class, address-sensitive (hence heisenbug).
g62 (task bn44hn53d): DISABLED "azul-layout?/probe" → ZST `Span` (struct Span;, no fields, no sret).
If new_tree survives (sizing entry 0x407B0 = 2) and we pass InvalidTree → CONFIRMED + the fix is to
make probe ZST for web-lift (gate on web_lift — but fix its propagation first — or a web-transpiler
cfg; don't lose desktop profiling). This is the best, most principled, address-independent lead of
the session. NOTE: g62 keeps the dense markers (one variable at a time); if it half-works, re-test
with markers stripped.

### g62 = probe NOT it; g63 = ★ MARKER-CLOBBER hypothesis (possibly self-inflicted bug!)
g62 (probe disabled → ZST Span): timeline UNCHANGED (afterReset=2, beforeCall=0). So the probe Span
sret was NOT the corruptor. Reverted probe-disable. ★ NEW HYPOTHESIS: the corruption is consistently
at the DENSE-MARKER region (between afterReset 0x407C4 and beforeCall 0x40748), and probe-disable didn't
move it → the corruptor may be MY OWN MARKER WRITES. The wasm stack likely grows INTO the 0x407xx
marker region during deep layout_document calls; a marker write (e.g. afterReset's `str` to 0x407C4)
then overwrites new_tree.nodes.ptr on the stack → next read sees garbage (len 0). This would make much
of this session's "corruption" SELF-INFLICTED by markers. g63 (task bcnvx4rdx): REMOVED the dense
loop-body markers (0x40744/4C/0x407C0/C4/0x40748); kept only post-reconcile(0x40740) + sizing
entry(0x407B0) + line142(0x40734). If sizingEntry(0x407B0) now reads 2 → CONFIRMED, markers were
clobbering → strip ALL raw-address markers in the layout hot path and re-test the REAL state (text may
measure!). NOTE: existing long-standing markers (0x40704 phase, 0x40700 shaping, etc.) may ALSO clobber
— if g63 still shows 0, move ALL markers to a safe region BELOW the synth-data band (synth starts
0x110000; stack-overlap risk is 0x407xx) or just minimize them. This reframes the whole InvalidTree.

### g63 RESULT: markers weren't the cause — corruption is REAL + address-sensitive (heisenbug)
g63 (dense loop-body markers removed): postReconcile(0x40740)=2, sizingEntry(0x407B0)=0. So removing
the markers did NOT fix new_tree → they weren't the (sole) corruptor. But the drop point SHIFTED
(g61 with markers: survived to afterReset/line779; g63 without: gone by calc:98) → confirms the
corruption is ADDRESS-SENSITIVE (heisenbug — manifestation moves with any edit). RULED OUT this
session: probe-Span sret (g62), inline boundary (g60), my dense markers (g63), Box (g56), the
"arg-transfer" framing (was broken-cast noise). CONFIRMED: real stack corruption of new_tree's nodes
Vec, somewhere in layout_document between post-reconcile (line ~527) and the sizing call (~793),
address-sensitive. This is the deep M12 self(X0)-relative / SP / sret-slot-overlap class — NOT
crackable by marker bisection (heisenbug). RELIABLE NEXT STEP = IR analysis: relift with
AZ_REMILL_KEEP_SCRATCH=1 (bash /tmp/keepscratch.sh), read layout_document = __az_dep_<hex>.opt.ll /
.patched.ll, find new_tree's stack alloca/SP-slot and the UNEXPECTED store to it (the corruptor) —
likely a callee whose lifted epilogue drops an SP/callee-saved restore that enforce_sp_preservation
(transpiler:5210) doesn't cover (indirect/missing_block call, or a sret routed through a wrong slot).
OR the robust source fix: move new_tree off layout_document's stack (store in cache.tree early +
re-fetch heap ref each use) — sidesteps the stack corruption (borrow-checker work needed).
REVERT-at-cleanup: Box (mod.rs:519/1077); all remaining markers; probe is restored; cycle_g50.sh outliner flag.

### CONSOLIDATED STATE (after ~14 builds): deep M12-class bug, marker bisection EXHAUSTED
IR analysis is intractable by grep: marker addrs are computed (not literal constants), 10294 load/store
in layout_document's remill State-based IR (all SP-relative). The bug is the M12 stack/SP class which
historically took MULTIPLE sessions (M12.5x→y→6→7). Marker bisection is defeated by the heisenbug.
★ REFINED HYPOTHESIS (most likely): it's SP-DRIFT, not data corruption. `new_tree.nodes.len()` is an
SP-relative read; it reads 2 at post-reconcile but 0 at sizing because a callee BETWEEN them LEAKS the
guest SP (drops its epilogue `add sp,#N`), so layout_document's SP drifts down and the later
SP-relative read of new_tree lands on wrong (zeroed) stack memory. enforce_sp_preservation
(transpiler:5210) wraps DIRECT `%res = call @sub_<hex>` calls but NOT: indirect calls
(__az_indirect_dispatch), missing_block (__remill_function_call), or result-less calls. The corruption
window (post-reconcile → sizing) contains: compute_counters, mark_dirty loops, LayoutContext build,
2× calculated_positions.clone(), compute_dirty_ancestor_closure. One of these (or a callee of theirs)
leaks SP via a path enforce_sp_preservation misses → TASK #2.
THE TWO REAL FIX PATHS (need a focused session, not heartbeat single-builds):
  (A) TRANSPILER: extend enforce_sp_preservation to also wrap indirect/missing_block/result-less calls
      (parse_sub_call:5263 only matches `%res = [tail ]call ptr @sub_`). OR verify it actually restores
      SP for the corruption-window calls (add an SP read-back assertion). This is the principled fix.
  (B) SOURCE: move new_tree off layout_document's stack into heap-backed cache.tree (set after reconcile,
      access cache.tree.as_mut() each use) so SP-drift can't misread it. ~25 use-sites + borrow-checker.
RULED OUT (don't repeat): markers (g63), probe sret (g62), inline (g60), Box (g56), arg-transfer/call-
boundary (broken-cast noise). Upstream all WORKS (cascade/reconcile/tree-build=2 nodes). web-text-min
needs ONLY this fix. REVERT-at-cleanup: Box(mod.rs:519/1077), all g51-g63 markers, harness BISECT reads,
inline comment, outliner flag in cycle_g50.sh; probe is restored.

### ★★★ ROOT CAUSE NAILED + FIX (g64): enforce_sp_preservation missed __remill_function_call
EVIDENCE CHAIN (all reliable, no markers): (1) AZ_NO_FIX_SP=1 relift → cascade TRAPS (OOB in
StyledDom::create) → SP-leaks are REAL and ACTIVE; enforce_sp_preservation is load-bearing. (2)
layout_document.patched.ll (KEEP_SCRATCH IR): 131 wrapped @sub_ calls, but 2 `__remill_function_call`
(remill's indirect/computed-call dispatcher) that enforce_sp_preservation does NOT wrap (parse_sub_call
only matched `%res = call @sub_`). Cascade's IR had 0 such calls → cascade worked with sub_-wrapping
alone; layout_document's 2 leak SP in the post-reconcile→sizing window → SP drifts → the SP-relative
read of new_tree.nodes.len() reads ZEROED stack (2→0) → tree.get(root)=None → InvalidTree.
FIX (transpiler_remill.rs parse_sub_call:5263): match `@__remill_function_call` in addition to
`@sub_` → enforce_sp_preservation now save/restores SP+callee-saved around those calls too.
g64 (task: cycle_g64.out): rebuild azul-dll + relift + harness. SUCCESS = sizingEntry(0x407B0)=2
(new_tree survives) → past InvalidTree → inline-phase advances (B1/B2) → text.len=5 → maybe SHAPING.
If it works: this is THE fix for web-text-min; clean up all g51-g63 markers + Box; then height
propagation → hello-world.c. If sizingEntry still 0: the leaking call is elsewhere (indirect via a
different IR form, or a callee's callee) — widen the wrap or trace which __remill_function_call.

### g64 RESULT: __remill_function_call wrap didn't fix new_tree → corruption is deeper than call-wrapping
g64 (wrapped @sub_ + __remill_function_call = all 133 calls): sizingEntry(0x407B0) STILL 0. Per-function
SP-wrapping handles nested leaks too, so new_tree's corruption is NOT from an unwrapped call — it's
either inlined SP handling within layout_document's own frame, or a data-corruption store to new_tree's
stack slot. The __remill_function_call fix is KEPT (correct, principled, no regression — cascade still
works, may fix latent leaks). The AZ_NO_FIX_SP evidence (cascade traps without sp-fix) proves SP-leaks
are real but doesn't prove new_tree's specific corruption is SP (could be a wild store).
★ RECOMMENDED FIX = PATH B (cache.tree, sidesteps BOTH SP-drift AND stack-data-corruption): store the
LayoutTree in the HEAP-backed `cache.tree` (accessed via the stable `cache: &mut LayoutCache` ARGUMENT,
not a deep SP-relative stack local) so neither an SP-drift misread nor a stack-targeted wild store can
hit it. PLAN (mod.rs layout_document, ~35 new_tree uses, ~15 whole-tree &mut/&):
  1. After reconcile: `cache.tree = Some(new_tree_val);` (remove the g56 Box). 
  2. At each use, `let tree = cache.tree.as_mut().unwrap();` (or as_ref) re-fetched locally, NOT held
     across other cache.* uses. Watch borrows: ctx is SEPARATE (owns cache_map moved from cache@575),
     so `calculate_intrinsic_sizes(&mut ctx, cache.tree.as_mut().unwrap(), text_cache, &dirty)` is OK
     IF text_cache isn't a cache.* field (CHECK: what is text_cache? if it's &mut cache.something →
     conflict → take it out first). The cache_map remap block (cold-skipped) + mark_dirty use
     &new_tree.nodes — fine via cache.tree.as_ref().
  3. cache.tree stays set at the end (drop the final `cache.tree = Some(*new_tree)`).
  4. cargo check -p azul-layout iteratively until borrows clear, then build+lift.
If path B works (sizingEntry=2) → text measures → clean up all g51-g64 markers → height-prop →
hello-world.c. If it does NOT work → the corruption hits the heap too (a truly wild store) → IR analysis
of the corrupting store is unavoidable (Task #2 deep session). Upstream all WORKS.

### g65: PATH-B VALIDATION (cheap test before the 35-edit refactor)
Before committing to the full cache.tree refactor, g65 validates it cheaply: after the 0x80 marker
(post remap+early-exit, where new_tree is still valid=2), CLONE new_tree into the heap-backed
cache.tree (`cache.tree = Some((*new_tree).clone())`). Right before the sizing call, read BOTH:
0x40748 = stack new_tree.nodes.len() (expect 0 = corrupted), 0x4074C = HEAP cache.tree.nodes.len()
(expect 2 IF path B sidesteps it). cache is the stable &mut arg (read correctly throughout), so
cache.tree is NOT a deep-SP-relative stack local. VERDICT: heap=2 & stack≠2 → ✓ PATH B WORKS → do the
full refactor (all ~35 new_tree uses → cache.tree.as_ref()/as_mut(); text_cache is a separate arg, no
conflict). heap ALSO 0 → wild store reaches the heap → path B won't help, need IR-level store trace.
cargo check passed (LayoutTree:Clone ✓, cache.tree:Option<LayoutTree> ✓). g65 = task cycle_g65.out.

### g65 RESULT: PATH B (clone-based) RULED OUT — heap cache.tree=1 (not 2). Corruption is address-sensitive + reaches heap.
g65: stack new_tree(0x40748)=0, HEAP cache.tree clone(0x4074C)=1 (expected 2). The heap clone is ALSO
wrong → path B (clone) does NOT cleanly protect. The "1" (not 0, not 2) + the across-build heisenbug =
the corruption is DEEPLY ADDRESS-SENSITIVE and either (a) LayoutTree::clone mis-lifts (Vec::clone drops
a node → clone=1; ties to memory's "lift DROPS elements in mapped/collected closures" + the cascade
From-impl fix), or (b) the corruption window already reached the 758 clone-point in THIS build (the
heisenbug shifts where new_tree's 2→1→0 progression lands). NOTE the progression 2→1→0 (not a single
zero) suggests a Vec-element-drop or repeated store, NOT a one-shot wild store.
★ EXHAUSTED (all ruled out): probe sret, inline, dense markers, Box, arg-transfer/call-boundary,
__remill_function_call SP-wrap (g64, KEPT-correct), path-B clone (g65). This is the deep M12 lift class
(NEON/STP_Q/memcpy/SP — historically cracked only by SPECIFIC transpiler instruction fixes across
multiple sessions). Marker bisection is DEAD (heisenbug). The ONLY remaining reliable path is
IR/disasm-level: trace which lifted instruction in layout_document's corruption window writes
new_tree.nodes.len/ptr — needs a focused session reading the .patched.ll / otool-disasm of
__az_dep_<layout_document> around the Vec ops (clone, the dirty loops). Possibly: Vec::clone or a
Vec mutation mis-lifts (drops element / mis-writes len). KEEP: __remill_function_call SP-fix (correct).
REVERT-at-cleanup: g65 clone(mod.rs ~758)+markers(0x40748/4C); all g51-g65 markers; Box; outliner flag.
HONEST: web-text-min is blocked on this ONE deep lift bug; everything else (cascade/reconcile/tree-build/
font-parse/SP-for-cascade) WORKS. This is multi-session deep-debugging, not a heartbeat single-build fix.

### g66 FINAL: PATH B definitively ruled out + marker bisection EXHAUSTED. Needs focused IR/disasm session.
g66 (within ONE build): at line 758 (after clone) src new_tree=2 AND clone cache.tree=2 (both valid);
at line 786 (before call) stack=0 AND heap=1 (both corrupted). So: (a) Vec::clone does NOT mis-lift
(clone was 2) → PATH B (move-based) RULED OUT — the corruption reaches a SEPARATE heap allocation, so
moving off-stack can't help; (b) the corruption is in the tiny window 758→786 and affects BOTH the
stack tree (via Box ptr) AND the heap clone (via cache ptr) → it's SP-DRIFT on the pointer READS (both
loaded SP-relative), not data corruption. WINDOW SUSPECTS all ELIMINATED: probe (g62 ZST, no help),
calculated_positions.clone() (it IS a wrapped @sub_ call — Vec::clone lifted at transitive[185/186/264],
so enforce_sp_preservation wraps it), my markers (g63 minimal-marker still corrupted), the 0x80/0x90
0x40704 writes (the 0x80 write at 757 did NOT corrupt the 764 read → 0x40704 writes are harmless). What
remains in 758→786: the LOOP control flow + the wrapped clone's SP restore possibly being insufficient.
★ CONCLUSION: marker bisection is FUNDAMENTALLY DEAD here (heisenbug shifts the manifestation every
build). The ONLY reliable path is IR/disasm: relift KEEP_SCRATCH, isolate layout_document's LOOP-BODY
basic blocks (758→786) in .patched.ll, and find the State.SP (offset 1040) store/restore imbalance —
a callee whose SP-restore enforce_sp_preservation emits is wrong, OR an inlined frame adjustment, OR a
store that mis-writes State.SP. This is the M12 SP class (multi-session, cracked only by specific
transpiler instruction fixes). KEPT: __remill_function_call SP-wrap (correct). web-text-min is blocked
SOLELY on this; cascade/reconcile/tree-build/font-parse/cascade-SP all WORK. REVERT-at-cleanup: g65
clone(mod.rs:763)+markers, g66 markers, all g51-g66 markers, Box, outliner flag.

### ★★★★★ THE BUG WAS SELF-INFLICTED BY MARKERS (g68): they overlap the layout wasm's STACK
The stack-relocation log (server_g64.log) is the smoking gun:
  M9-3: relocated stack for azul-mini (slot 0): SP → 196608 (0x30000)   → mini stack  [0x10000..0x30000]
  M9-3: relocated stack for transitive-lift (slot 1): SP → 327680 (0x50000) → layout stack [0x30000..0x50000]
layout_document runs in the TRANSITIVE-LIFT wasm, whose stack is [0x30000..0x50000]. My diagnostic
markers write to 0x40000-0x40800 — SQUARELY INSIDE that active stack. Every `write_volatile(0x40xxx,…)`
during layout_document CLOBBERS its own live stack — new_tree's slot, the Box ptr, the `cache` arg ptr.
This explains EVERYTHING that made no sense: address-sensitive (which marker hits new_tree's slot shifts
per build = the "heisenbug"); probe-off/markers-off didn't fully help (other markers still hit);
Box didn't help (Box ptr on the same stack); both stack new_tree AND heap-via-cache-ptr corrupt
(g65/g66 — the cache ptr is on the stack); the 2→1→0 garbage (markers writing into the Vec header).
THE INVALIDTREE IS NOT A LIFT BUG — it's the markers corrupting the layout wasm's stack. FIX (g68):
moved ALL layout-path markers (mod.rs+sizing.rs, 33 of them) from 0x40xxx → 0x60xxx (the FREE band
[0x50000..0x110000], above both wasm stacks, below the synth-data band at 0x110000). If g68 shows NO
InvalidTree + valid rects → CONFIRMED → the real layout WORKS → strip all markers, then text-measure →
height-prop → hello-world.c. (The __remill_function_call SP-fix from g64 is still correct/kept.)
LESSON: the operational must-do "no raw markers in code reached natively" missed the deeper trap —
markers in LIFTED code whose wasm stack happens to sit at the marker addresses. Future marker addrs
MUST be in [0x50000..0x110000], NOT 0x40xxx.

### g68/g69/g70: markers RULED OUT as corruptor, BUT moving them off-stack KILLED the heisenbug → reliable bisect
g68 (mod.rs+sizing.rs markers → 0x60xxx) and g69 (ALL azul-layout markers → 0x60xxx free band): new_tree
STILL corrupts (postReconcile(0x60740)=2, sizingEntry(0x607B0)=0). So the markers were NOT the corruptor
— it IS a real lift bug. HOWEVER: the marker WRITES were in 0x40000-0x40800 = INSIDE transitive-lift's
stack [0x30000..0x50000], so they ADDED noise (the heisenbug). With all markers now in the FREE band
[0x50000..0x110000], marker bisection is FINALLY RELIABLE. g70 (reliable, re-run vs live g69 server):
atClone(0x607C0)=2 → beforeCall(0x60748)=0, and BOTH stack new_tree AND heap cache.tree go to 0 → the
corruption is reliably in the window between the at-clone marker (~770) and the before-call marker (~803):
the in-loop `cache.calculated_positions.clone()` (783-785), `reset_peak()` (790), `Probe::span` (791).
g70 adds finer free-band markers (0x60780 after the clone, 0x60784 after reset_peak, 0x60788 after the
Span) → names the exact corruptor. Since it hits BOTH stack+heap reads, it's SP-drift (both read via
stack ptrs). Probe was ruled out (g62) and the clone is a wrapped @sub_ — so if g70 fingers one of them,
the wrap/probe-ZST is insufficient and needs a transpiler look. KEEP: markers in free band (permanent —
0x40xxx overlaps the layout wasm stack); __remill_function_call SP-fix.
