# HANDOFF → NEXT AGENT: get `hello_world.c` working on the azul **web (lifted) backend**

**Date:** 2026-06-06 · **Branch:** `mobile-ios-android` · **Repo:** `/Users/fschutt/Development/azul-mobile`
**Remill fork:** `/Users/fschutt/Development/azul/third_party/remill`
**Goal:** `examples/c/hello-world.c` (a styled button + counter) renders and reacts on the web backend
(native ARM64 `libazul.dylib` → remill-lifted to `azul-mini.wasm`; layout/shaping run in wasm, no GPU).

This is the clean, actionable handoff. The blow-by-blow evidence is in the chronological log
`scripts/HANDOFF_web_vec_return_len_mislift_2026_06_06.md` (554 lines, sections g129→g139). Memory:
`~/.claude/.../memory/web_vec_len_mislift_systemic_2026_06_06.md` + `web_flexbox_lift_2026_06_01.md`.

---

## ✅ 2026-06-08 (g147) — REBUILD+RELIFT DONE; web-text-min STILL POSITIONS with crates.io BTreeMap deps

Rebuilt the stale dylib (was Jun-6 17:27, built against OLD local-path forks; deps committed to crates.io
at 23:07/23:52 *after* it) → fresh `libazul.dylib` (Jun-8 10:53) now links published **allsorts-azul 0.16.5
+ rust-fontconfig 4.4.3** (the latter has `patterns: BTreeMap<FcPattern,FontId>` = upstream original, NOT
the `Vec` the old fork used + adds the `single-thread-unsafe-locks` feature web_lift enables). Relifted
web-text-min + ran the harness:
- **`[g132 lays-out] overflow_size = 39.10 x 20.05` ✓✓✓ TEXT LAYS OUT (h>0)**, `[g133] collect=Ok len=1,
  layout_flow=Ok`, `[g135] _impl tree.nodes.len=2 tree.get=TRUE`, `__remill_error count=0`, rc=0.
- ⇒ **BTreeMap-of-FcPattern lifts FINE now** (the NEON decoders resolved the `Ord` mis-lift) — g146 risk
  CLEARED. No need to re-add the `Vec` patterns / publish 4.4.4.
- (The "InvalidTree in reconcile (520)" + "LayoutError byte0=0" harness lines remain the known 0x40xxx
  PHANTOM; real signals all green. `node_count=2 (expected 5)` is the harness's stale flexbox assertion.)

PROVENANCE GAP (flagged to user, not yet fixed): crates.io has rust-fontconfig **4.4.3** but the local
`/Development/rust-fontconfig` repo is still **4.4.2** (no `single-thread-unsafe-locks` feature, not bumped,
clean tree) — the 4.4.3 source bump was never committed back locally ("stopped before midnight"). allsorts
0.16.5 IS fully committed (`cbf5599`) + matches crates.io. remill decoders committed (`6aabb45`).

**hello-world relift (g147) — blocker REPRODUCES + sharpened.** Same dylib, harness markers:
- `cascade ok node_count=5 ✓`, `__remill_error count=0 ✓`, `rc=0`.
- `[g73] LayoutTree@sizing: root=0 nodes.len=3 get(root).is_some=1` ← tree VALID (3 nodes) at SIZING.
- `[g135] _impl tree.nodes.len=0, tree.root=0, tree.get(idx)=FALSE, Err AT first tree.get` ← collect_and_measure
  (POSITIONING) sees an EMPTY tree and Errs immediately. **`tree.root=0` reads CLEANLY (not garbage)** ⇒ the
  `&mut tree` ptr is likely VALID but pointing at an EMPTY/default LayoutTree, NOT the real 3-node one.
- `[g132] overflow_size=0`, rects `(0,0,784,21)(0,0,784,0) MAX (8,16,784,13) MAX` ← INTERMITTENT across IFCs:
  one div laid out a text line (8,16,784,13 h=13), another got height 0. Classic context-dependent ref
  mis-pass (g56 class), not uniform failure.

### ★ g147 RESULT — the g145 "empty-tree / `&tree` mis-pass" theory is DEBUNKED (was a marker misread)

The g147 caller-vs-callee table came back `(no g147 slots set — neither layout_ifc nor _impl reached in
positioning)`. And the EXISTING `0x606AC` marker writes `is_some | 0xC0DE0000` (so ANY execution leaves
`0xC0DE000x`), yet it reads plain **`0x0`** ⇒ **`collect_and_measure_impl` is NEVER ENTERED.** So g145's
"`_impl` receives an empty tree (`nodes.len=0`)" was a **misread of UNINITIALIZED markers** (`0x606A8/AC/B0`
default to 0 because the entry block never ran; the "Err AT 6449" was seq=0=never-written). ⇒ **STOP chasing
the `&tree` ref / nested-IFC empty-tree.**

**REAL blocker (g147): the POSITIONING pass never reaches `layout_ifc` for hello-world's nested text-divs.**
`layout_formatting_context` doesn't dispatch `Inline → layout_ifc` for the `label_wrapper` (div + "5") or
the button's text. web-text-min reaches it only because its body DIRECTLY contains text (body FC = Inline).
Block rects DO compute (body 784×21, button 8,16,784,13) so positioning runs — but the inline text path is
skipped. `[g75] IFC-sizer calls=0` says even SIZING never collected inline content ⇒ points at FC-assignment
/ tree-construction or a cache short-circuit, NOT a ref mis-pass.

**g147b diagnostic (rebuild+relift in flight):** per-node markers — `calculate_layout_for_subtree` entry
(0x60980+, did positioning reach the node), `layout_formatting_context` FC discriminant (0x609A0+), and the
dispatch arm taken (0x609C0+ = layout_bfc vs layout_ifc). Harness `[g147b dispatch]` table decides:
(a) div FC reads `Block` ⇒ tree-construction FC assignment wrong; (b) FC reads `Inline` but arm = layout_bfc
⇒ match dispatch mis-lifts; (c) FC marker UNSET though calc reached the node ⇒ cache-hit short-circuits
`layout_formatting_context`. Markers fc.rs (layout_formatting_context entry ~383 + Block/Inline arms) +
cache.rs (calculate entry ~1973); harness read block after `[g147]`. ALL `web_lift`-gated / REVERT-at-cleanup.
(The g147 caller/callee markers 0x60900–0x60960 are KEPT too, harmless, revert-at-cleanup.)

### ★ g147c RESULT + the NATIVE-REPRO pivot (markers are unreliable; stop using node-data markers)

g147b/g147c reliable facts: body → `layout_bfc` in PerformLayout (node 0, arm marker ✓); nodes 1,2 reached
in ComputeSize ONLY; `layout_formatting_context` takes NO dispatch arm for nodes 1,2 (reliable arm markers
unset) ⇒ `calculate(node1/2, ComputeSize)` returns BEFORE the FC dispatch (cache-hit or early-Err); `layout_ifc`
never runs; `[g75] IFC-sizer calls=0` (the intrinsic-sizing pass ALSO never collected the divs' inline content).
⚠ **Marker reliability caveat (important):** markers that READ node/tree data before writing
(`match node.formatting_context`→0x609A0, `bfc_children.len()`→0x60A00) are SILENTLY DROPPED by the lift,
while plain constant writes inside match-arms (0x609C0, 0x60980) survive. So "marker unset" ≠ "not reached".
Do NOT trust node-data-derived markers; only constant-write markers at reliable control points.

**PIVOT — verify NATIVELY (relift-free, reliable).** Added `layout/tests/web_lift_nested_text_repro.rs`:
runs `body > div(font-size:32) > text("5")` through the REAL solver3 layout natively (no lift) and asserts
the div height > 0. `cargo test -p azul-layout --test web_lift_nested_text_repro -- --nocapture`.
- div height > 0 NATIVELY ⇒ source logic CORRECT ⇒ blocker is LIFT-ONLY (the nested div→text inline path
  mis-lifts; focus the lift: how `layout_bfc` Pass-1 / `calculate(child,ComputeSize)` / the FC dispatch for a
  nested inline div is lifted vs the flat body-IFC case web-text-min uses).
- div height == 0 NATIVELY ⇒ real layout-logic bug ⇒ fix in source + verify with this test in seconds.

### ★ g147d — native IFC logic WORKS ⇒ LIFT-only bug = spurious ComputeSize cache HIT (experiment in flight)

The fresh `web_lift_nested_text_repro` test SEGVs (font-shaping with SYSTEM fonts under debug — a test-env
artifact, NOT the layout). But the EXISTING `tests/ifc_caching.rs` runs: **4 inline-layout tests PASS
natively** (incl. `test_cached_inline_layout_with_constraints_has_metrics`) — so native IFC layout LOGIC is
sound. (One test SEGVs, also in system-font shaping.) ⇒ the nested-div blocker is **LIFT-ONLY**.

Root-cause hypothesis (from RELIABLE constant-write markers): `calculate(div, ComputeSize)` for the nested
divs returns BEFORE `layout_formatting_context` — the ComputeSize cache-miss marker `0x60A60` (a CONSTANT
write = the reliable kind) NEVER fires ⇒ a **spurious cache HIT**: the lifted `get_size`/`get_layout`
mis-reads a fresh/empty `cache_map` entry as `Some` (Option-niche/compare mis-lift), short-circuiting
`layout_formatting_context → layout_ifc` so `<div>text</div>` never lays out. web-text-min dodges it (body
is the IFC root, laid out directly in PerformLayout — no child-ComputeSize-cache path).

EXPERIMENT (g147d, rebuild+relift in flight): bypass the per-node cache READ under `web_lift`
(cache.rs ~2004, `let _az_cache_read_enabled = !cfg!(feature="web_lift")`); the STORE is untouched. Forcing a
miss → layout_bfc Pass-1 recomputes the child → `layout_ifc` → text positions. If `[g132] overflow_size.h>0`
for hello-world ⇒ CONFIRMED; then narrow the real fix to `get_size`/`get_layout`'s lifted Option/compare
(don't leave the whole cache disabled — it's a perf hammer). If NOT ⇒ the recompute ALSO skips layout_ifc
(deeper: FC-assignment for nested inline divs, or an early Err in prepare_layout_context).

### ★ g147d RESULT = cache hypothesis WRONG; g147e all-arm trace in flight

g147d FAILED: with the cache READ bypassed under web_lift, `[g132] overflow_size` STILL 0; nodes 1,2 still
take no dispatch arm. ⇒ spurious cache hit was NOT the cause. So `calculate(node1/2, ComputeSize)` reaches
the COMPUTE path but still doesn't dispatch `layout_ifc`.

New hypothesis: the real `match node.formatting_context` (fc.rs:411) has arms my Block/Inline markers didn't
cover (InlineBlock, Table, Flex|Grid, TableCell, `_ => layout_bfc`). If the nested divs' `formatting_context`
MIS-READS as garbage, the match falls to `_`/InlineBlock → `layout_bfc` not `layout_ifc` → text never laid
out, looking like "no dispatch".

g147e (rebuild+relift in flight): PURE-CONSTANT markers (reliable) on lfc entry (`0x609E0=0xC0DE0042`, before
any node read) + EVERY arm (`0x609C0`: Block=1,Inline=2,InlineBlock=3,Flex/Grid=4,Table=6,TableCell=7,`_`=9).
Harness `[g147b dispatch]` decodes all. lfc-ENTERED+arm=9/3 ⇒ FC mis-read (fix the lifted
`node.formatting_context`/LayoutNodeHot field read); lfc-ENTERED+no-arm ⇒ `tree.get()?` Err; lfc NOT-entered
though calc entered ⇒ early return in calculate before lfc. (g147d cache-bypass KEPT to isolate compute path.)

### ★★★ g147e RESULT + g147f ROOT CAUSE & FIX — FormattingContext niche-discriminant mis-lift ★★★

g147e all-arm constant markers (RELIABLE) gave the decisive answer:
```
node[0] body: lfc=ENTERED✓ | arm=→layout_bfc(Block)              ← correct
node[1] div:  lfc=ENTERED✓ | arm=→layout_bfc(_UNKNOWN/garbage-FC) ← WRONG (should be →layout_ifc Inline)
node[2] div:  lfc=ENTERED✓ | arm=→layout_bfc(_UNKNOWN/garbage-FC) ← WRONG
```
`layout_formatting_context` IS entered for the nested divs, but `match node.formatting_context` reads a value
matching NO explicit arm → `_` fallback → `layout_bfc` instead of `layout_ifc` → text never lays out.

**ROOT CAUSE:** `FormattingContext` (core/src/dom.rs:1060) was `#[derive(Clone,PartialEq)]` with NO `#[repr]`
and THREE payload variants (`Block{bool}`, `Float(LayoutFloat)`, `OutOfFlow(LayoutPosition)`) mixed with
payload-less ones. Rust niche-packs the payload-less variants' discriminants into the payloads' invalid byte
values. The remill lift MIS-DECODES that niche encoding: `Block` (byte 0/1) reads right, but `Inline` (a niche
value) reads as garbage → `_`. `determine_formatting_context_for_display` correctly returns `Inline` for a
block div whose only children are inline text (`has_only_inline_children` → Inline), so the VALUE is right;
the niche READ is wrong. web-text-min dodged it because body's FC=Inline is read in a path that happened to
work (or body got Block... actually body has block children so FC=Block, which reads fine — the bug only bites
the INLINE-FC nested divs).

**FIX (g147f, core/src/dom.rs):** added `#[repr(C, u8)]` to `FormattingContext` → explicit u8 discriminant at
offset 0, no niche packing → the lift reads it correctly. SAME established pattern as the text3 enums
(InlineContent/LogicalItem/ShapedItem/FontStack/LayoutError). Unconditional (correct + harmless for native).
Rebuild+relift IN FLIGHT (azul-core changed → full rebuild). EXPECT: node[1,2] arm → `→layout_ifc(Inline)`,
`[g132] overflow_size.h>0`, text positions. If confirmed: revert the g147d cache-bypass + the g147a-e
diagnostic markers; then move to hello-world's remaining blockers (§6: counter snprintf, click/dispatch).

### g147f RESULT = repr(C,u8) did NOT change dispatch; g147g reads raw disc

After `#[repr(C,u8)]`, nodes 1,2 STILL fall to `_`. So it's NOT a niche-decode issue. ALSO learned: my
match-based entry marker (0x609A0) was DROPPED while the pure-constant one (0x609E0) fired ⇒ **reading
`node.formatting_context` itself destabilizes the lifted code for the divs** (constant writes survive; a
`match`/read on that field doesn't). g147g (rebuild+relift in flight): in the `_` arm, `read_volatile` the
RAW disc BYTE (offset 0 under repr(C,u8)) → 0x60B40+slot. disc=1 ⇒ value IS Inline, the dispatch MATCH
mis-lifted (jump-table/branch bug — fix by replacing the `match` with explicit `if`/disc compares, the
pattern that fixed other jump-table mis-lifts); disc≠1 ⇒ tree construction stored the wrong FC (chase
determine_formatting_context_for_display / has_only_inline_children / the reconcile clone at
layout_tree.rs:920). repr(C,u8) KEPT for now (makes the raw disc well-defined; decide keep/revert after).

### g147g/h RESULTS — can't read FC at all; FC comes from reconcile-clone, not determine_

- g147g: the raw `read_volatile` disc read was DROPPED too (no trap, rc=0) ⇒ **ANY read of
  `node.formatting_context` destabilizes the lifted code** and skips the following write. So the FC value
  cannot be observed in-lift. BUT the styled_dom NodeType discs ARE correct (`[2,3,177,52,177]` =
  Body,Div,Text,Button,Text) ⇒ DOM is fine; corruption is confined to the LayoutTree FC field.
- g147h: constant markers in `determine_formatting_context_for_display` NEVER fired ⇒ that fn is NOT
  reached in the lifted layout pass. The lifted layout RECONCILES (clones) a pre-built tree
  (layout_tree.rs:920 `formatting_context: hot.formatting_context.clone()`), so the FC is CLONED, not
  recomputed. ⇒ suspect the FC clone OR the tree.get/Vec stride.

### g147i (in flight) — testing the `tree.get` Vec-stride hypothesis

node 0 (index 0) reads FC correctly (Block); nodes 1,2 (index>0) read garbage AND reads destabilize the
lift — classic signature of `tree.get(index)=&nodes[index]` mis-lifting the stride (`base + index*sizeof`)
for index>0, making nodes 1,2 garbage references. g147i marks the node REFERENCE ADDRESS (0x60B80, not a
field deref → reliable) at lfc entry. Uniform Δ == sizeof(LayoutNodeHot) ⇒ stride OK (FC-field read is the
specific bug → look at the FC clone / field offset / repr); irregular Δ ⇒ stride mis-lift (fix tree.get /
Vec indexing / LayoutNodeHot size). NOTE web-text-min only ever dispatches node 0 (its text is body's inline
content, no index>0 dispatch) — which is why it dodges this entirely.

### ★ g147i RESULT + g147 FIX (recompute IFC from styled_dom) — in flight

g147i: node ref addresses are UNIFORM (0x6293bc8 / +80 / +80) ⇒ `tree.get(index)` stride is CORRECT, nodes
1,2 are VALID refs. So the `formatting_context` FIELD VALUE is garbage (node 0 = Block correct; nodes 1,2 =
garbage from the reconcile clone of `FormattingContext::Inline` at get_full_node:920). The deep root is the
lifted clone/enum-read of `FormattingContext::Inline` (a complex enum: 3 payload variants Block{bool}/
Float/OutOfFlow + many payload-less); repr(C,u8) alone didn't fix it.

**FIX (g147, fc.rs layout_formatting_context, web_lift-gated):** instead of trusting the garbage cloned
`node.formatting_context`, recompute the IFC decision from the RELIABLE styled_dom — if `node.dom_node_id` is
Some and `has_only_inline_children(styled_dom, dom_id)` (made `pub(crate)`), route straight to `layout_ifc`
(a block container with only inline children establishes an IFC, CSS 2.2 §9.2.1), bypassing the corrupted
field. Marker 0x60BA0 confirms it fired. Rebuild+relift in flight. EXPECT: nodes 1,2 force_ifc FIRES,
`[g132] overflow_size.h>0`, text positions. CAVEAT (refine later): doesn't check display, so a flex/grid/
inline-block node with only-inline children would be wrongly forced to IFC — fine for hello-world (plain
block divs); gate more precisely (skip if display is flex/grid/table/inline-block) before un-web_lift-gating.
If force_ifc fires but text STILL doesn't position ⇒ node.dom_node_id ALSO mis-lifts, or layout_ifc itself
has a downstream nested-IFC issue (then chase layout_ifc/collect_and_measure for the child IFC).

### ★★ g147 FIX RESULT — DISPATCH FIXED (divs now route to layout_ifc); cache-bypass caused a HANG ★★

The force_ifc fix WORKS: the relift now lifts `collect_font_stacks_from_styled_dom` + the IFC text path
(it wasn't lifting those before) ⇒ the divs are now routed into `layout_ifc`. So `node.dom_node_id` reads
FINE and `has_only_inline_children(styled_dom)` works in-lift — **the FC-dispatch root cause is fixed.**
BUT `solveLayoutReal` then HANGS (node exit 124 / timeout; harness stops at "EARLY extern-input" before the
layout markers — a synchronous hang, not a trap). ROOT of the hang: the **g147d cache-bypass** (forces every
`calculate_layout_for_subtree` to recompute) — harmless before (divs did trivial layout_bfc), but now that
the divs do REAL IFC layout it creates an **unbounded reflow loop that never converges without the cache**
(scrollbar/reflow re-layout). FIX: REVERTED the g147d cache-bypass (cache.rs:2004 restored to
`if node_index < ctx.cache_map.entries.len()`); kept the force_ifc fix. Rebuild+relift in flight. EXPECT:
layout converges → `[g132] overflow_size.h>0` → hello-world text POSITIONS. If it STILL hangs with the cache
restored ⇒ the hang is in layout_ifc/collect_and_measure for the nested IFC itself (use AZ_FUEL=ALL at lift
time to convert the infinite loop into a named trap + read POST-TRAP step 0x40710).

### ★ g147 — HANG LOCATED via AZ_FUEL: infinite loop in TextShapingCache::layout_flow Stage-5 flow loop

AZ_FUEL=ALL relift → harness TRAPS (not hangs). Trap stack (resolved via server-log `dep: sub_X →
resolved=NAME`): `layout_dom_recursive → layout_document → calculate_layout_for_subtree(body) →
layout_formatting_context(body) → layout_bfc(body) → calculate_layout_for_subtree(div) →
layout_formatting_context(div) → layout_ifc(div) → TextShapingCache::layout_flow → __az_fuel`. ~10 frames
(NOT recursion — bounded stack) ⇒ an infinite LOOP in `layout_flow`. Its only loop is the **Stage-5 flow loop
`for fragment in flow_chain`** (text3/cache.rs:5761) — should run ~1× for one line and `break` on
`cursor.is_done()`, but on the lift it never terminates for the NESTED IFC (the flow_chain iterator or the
`cursor.is_done()` break mis-lifts — SAME systemic iterator/loop class as g136-g139, now ∞ instead of 0).
`g116 create_logical_items content.len=0` (Vec-len mis-lift) likely the trigger. web-text-min's flat IFC dodges it.

**WORKAROUND (g147, text3/cache.rs:5761, web_lift-gated):** hard iteration cap (`_az_flow_iters > 256 → break`)
+ marker 0x60BC0. Text lays out on iteration 1, so capping converges instead of hanging. Rebuild+relift
(NORMAL, no fuel) in flight. EXPECT: no hang → `[g132] overflow_size.h>0` → hello-world text FINALLY POSITIONS;
then §6 (counter renders server-side already; click/dispatch). If overflow_size still 0 (no hang) ⇒ the capped
loop laid out nothing (content.len=0 mis-lift) → chase the content.len Vec mis-lift into layout_flow. 256-cap
is a band-aid; the real fix is the iterator/Vec-len mis-lift.

### ★★ g147 SESSION-END (2026-06-08) — FC DISPATCH FIXED; remaining = systemic content.len=0 mis-lift ★★

Capping the flow loop (5761) AND the line-build loop (`while !cursor.is_done()` 7916) did NOT stop the hang —
the infinite loop is ONE LEVEL DEEPER, inside **`break_one_line`** (called at 8046; inlined, so each cap fires
once then break_one_line never returns). Cap-chasing is NOT converging.

**CONFIRMED ROOT (AZ_FUEL run's `g116` marker): `create_logical_items content.len=0`** — for the NESTED div,
`layout_ifc → collect_and_measure` produces EMPTY inline content (systemic Vec-len/iterator mis-lift;
`__remill_error=0` so NOT decode truncation — runtime value mis-lift, same class as g136-g139). Empty content
starves the BreakCursor → break_one_line/`cursor.is_done()` spin forever. A perfect loop-cap only converts
hang→EMPTY text (overflow_size still 0), never hang→correct text. **The real fix is the content.len mis-lift
(make collect_and_measure return the div's 1 text item) = the transpiler/remill optimized-code Vec-len fix
flagged since g139** — NOT more source caps.

**DONE + KEEP:** (1) rebuilt against crates.io BTreeMap deps — web-text-min still positions. (2) ★ **FC-dispatch
FIX (fc.rs `layout_formatting_context`, web_lift-gated: recompute IFC from styled_dom
`has_only_inline_children`→`layout_ifc`, bypassing the garbage cloned FC field)** = the session breakthrough,
KEEPER (refine display-gate: skip flex/grid/table/inline-block). (3) `has_only_inline_children` → `pub(crate)`.

**REVERT-at-cleanup:** g147d cache-bypass ALREADY reverted. The two loop caps (text3/cache.rs:5761 +
`_az_line_iters` ~7920) + all g147a-i diagnostic markers (0x609xx/0x60Axx/0x60Bxx) + harness g147* reads.
`#[repr(C,u8)]` on FormattingContext (core/dom.rs) is harmless/principled (didn't fix dispatch — keep or revert).

**NEXT CONCRETE STEP:** fix `collect_and_measure_inline_content_impl`'s collection for the NESTED IFC so it
returns content.len=1 (not 0). Same DOM-children loop / Vec-len mis-lift as g136-g139 (NEON fixes resolved it
for web-text-min's FLAT body-IFC; recurs for the nested div). Lift the fn standalone + trace why its content
Vec ends empty for the nested call. Deep transpiler/remill Vec-len mis-lift; source loop-rewrites (g137-g139)
already proven insufficient.

**g147 confirm (no relift):** `nm` shows collect_and_measure_inline_content_impl is ONE monomorph
(native 0x34e04c → 0x35198c, ~14.6 KB). So web-text-min (content.len=1, WORKS) and hello-world's nested div
(content.len=0) run the SAME lifted code — the mis-lift is RUNTIME INPUT-DEPENDENT (body→text yields 1,
div→text yields 0), not structural ⇒ the DOM-children loop mis-counts for the div's input specifically.
Deep remill EXECUTION-fidelity issue (§2-B): lift 0x34e04c standalone + EXECUTE/trace the dom_children loop
for the div input vs body input to find the spill/reload/PHI the lift mis-models. NOT a single-cron-cycle fix.
DECISION ASKED OF USER (pending): (a) deep collect_and_measure standalone trace, (b) pause for remill-strategy
input, (c) commit the FC-dispatch breakthrough first.

**g147 FINAL confirm (standalone lift of 0x34e04c, no relift):** `__remill_error: 0`, `missing_block: 1`
(benign) ⇒ collect_and_measure lifts CLEANLY, NO undecoded instr / NO decoder to add. Static IR is CORRECT ⇒
content.len=0 is purely a remill EXECUTION-fidelity mis-lift (Vec::len reads 0 at runtime for the div input).
Confirmed THREE ways (one-monomorph input-dependent + runtime err=0 + static IR clean). NOT autonomously
crackable in cron cycles — needs either (1) an EXECUTE-and-diff harness for the lifted collect_and_measure
(run the wasm fn, diff register/memory vs native, find the mis-modeled spill/reload/PHI of the dom_children
loop induction/len), or (2) a transpiler post-pass that stabilizes optimized-code Vec-len reads. **Autonomous
cron loop PAUSED here (cron 2e90bba5 deleted)** pending the user's call on the deep remill investment. The
FC-dispatch breakthrough (the session's big win) is intact + documented; web-text-min still positions;
everything UNCOMMITTED per standing instruction.

---

## ⚠️ SESSION-END STATUS (2026-06-06 g146) — COMMITTED + PUBLISHED. READ THIS FIRST.

**Branches changed — the title-block "Branch: mobile-ios-android" above is STALE:**
- **azul-mobile → branch `web-lift-text-layout`** (off `mobile-ios-android`), pushed to `github.com/fschutt/azul`.
- **remill fork → branch `aarch64-web-lift-decoders`**, pushed to `github.com/fschutt/remill`. The 4 NEON
  decoders (FNEG.2s, FMUL scalar-by-elem, UCVTF scalar, FNMUL scalar) are committed there; the installed
  `remill-lift-17` + `aarch64.bc` already have them built (no remill rebuild needed unless you add more).
- **Font forks PUBLISHED to crates.io** (no more local-path `[patch]`): `allsorts-azul 0.16.5` +
  `rust-fontconfig 4.4.3`; deps bumped in `core/Cargo.toml` + `layout/Cargo.toml`. rust-fontconfig's
  web-lift `StLock` is behind a `single-thread-unsafe-locks` feature that azul-layout's `web_lift`
  auto-enables (native builds keep the thread-safe RwLock). `cargo check -p azul-layout` passes both
  native and `--features web_lift`.

**★ DO THIS FIRST: full rebuild + relift web-text-min — the dylib on disk is STALE.** The current
`target/aarch64-apple-darwin/release/libazul.dylib` was built against the OLD local-path forks. The deps
are now crates.io 0.16.5/4.4.3 which differ in one risky way: **rust-fontconfig 4.4.3 reverted `patterns`
from `Vec` back to the original `BTreeMap`** (the fork used `Vec` because `BTreeMap<FcPattern,_>`'s `Ord`
likely mis-lifted pre-NEON-fixes). So: rebuild (recipe §5) → relift web-text-min → confirm `[g132]
overflow_size.height>0` STILL holds.
- If YES → BTreeMap lifts fine now (NEON fixes resolved the Ord mis-lift); proceed to hello-world.
- If NO (font matching empty / hang / missing_block) → BTreeMap-of-FcPattern still mis-lifts: either
  find+decode the culprit instruction (scan method, §g144), OR re-add the `Vec` for `patterns` *properly
  feature-gated* under rust-fontconfig's `single-thread-unsafe-locks` (publish 4.4.4).

**Where things stand (verified this session):**
- ✅ Multi-session blocker ROOT-CAUSED + FIXED: undecoded NEON instrs silently truncate remill CFG
  recovery → garbage returns (NOT the old "value mis-lift" theory). 4 decoders → web-text-min "Hello"
  POSITIONS (overflow 39×20), `__remill_error` 21→0.
- ✅ hello-world.c: lift COMPLETE (`__remill_error`=0), cascade works (5 nodes), RENDERS the full UI
  server-side (`curl 127.0.0.1:8800` → styled "Increase counter" button + counter "5"), block layout
  partially runs (rects, body 784×21).
- 🔲 hello-world REMAINING (the real next blocker — see g145 below): the WASM-side inline-text layout for
  the NESTED IFC (text inside the button/counter divs) gets an EMPTY tree (`collect_and_measure` sees
  `tree.nodes.len=0` while reconcile/sizing hold the 3-node tree) → text not positioned. A `&tree`
  ref mis-pass for child IFCs (g56 stack-address class; NOT a decode truncation). Then click/dispatch (§6).
- 🔲 Cleanup (once stable): the 6 out-param workarounds + g137/g139 fc.rs loop rewrites are now likely
  UNNECESSARY (the lift is complete — undecoded-instr truncation was the real cause); revert one at a time
  + relift. Strip the `[g129..g145]` diagnostic markers. See §4.

Next-session starting prompt: `scripts/PROMPT_web_helloworld_NEXT.md`.

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
