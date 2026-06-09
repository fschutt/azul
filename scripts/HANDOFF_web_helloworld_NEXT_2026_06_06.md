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

---

## ★★★ DEEP-FIX PLAN (2026-06-08) — fix the remill/transpiler Vec-len mis-lift; delete the source hacks ★★★

USER DIRECTIVE: debug the deep underlying fix so we DON'T need source adjustments (delete force_ifc + the 6
out-param hacks). Cron `fc91adef` (:14/:34/:54) drives this. COMMITTED: azul-mobile 87d796988 (web-lift
breakthrough), rust-fontconfig e7e25fa (4.4.3 reconciled; not pushed).

**THE BUG (exact site):** `fc.rs:7008 let dom_children_len = dom_children.len();` reads **0** in the lifted
loop bound, while the SAME Vec's len reads **1** via the volatile marker at fc.rs:7003 (g139). So a plain
OPTIMIZED `Vec::len()` load mis-lifts to 0; the Vec is genuinely non-empty (len=1). Native pattern =
`ldr x, [reg, #0x10]` (Vec = {ptr@0,cap@8,len@16}; load len from +0x10). `__remill_error=0` (no truncation) —
the loop body IS lifted; it just reads len=0 → iterates 0× → empty content → text never lays out.

**WHY same code, body→1 / div→0:** input-dependent runtime mis-execution of the optimized len load (NOT a
structural IR bug). Classic store-to-load-forwarding-of-wrong-value OR stale-base-register-for-the-load
signature (plain load wrong, volatile load right).

**CRON ROADMAP (analysis-first, cheap before relift):**
1. Map fc.rs:7008 → the EXACT native `ldr x,[reg,#0x10]` instruction. objdump `-l` didn't interleave (no
   dSYM); use `dsymutil libazul.dylib -o /tmp/az.dSYM` then `lldb` `image lookup --address` / `atos`, OR
   `objdump --dwarf=decodedline`. Get the instr's address + the base reg + how the base is computed.
2. Lift collect_and_measure with AZ_REMILL_KEEP_SCRATCH=1 (keeps `__az_dep_<native>.opt.ll`) and read the
   lifted IR around that load. Compare the VOLATILE load (7003, correct) vs the PLAIN load (7008, wrong) in
   the .opt.ll: do they load from the same address? Is the plain load store-forwarded a 0 from an aliasing
   guest store? Is the base register reload mis-modeled (a spill/reload remill gets wrong)?
3. Root cause → fix: if LLVM forwards a wrong value across the single `az_guest` alias scope → tighten the
   transpiler's alias metadata / mark the guest len load `volatile` in a post-pass (transpiler_remill.rs,
   the M10-B1 alias-scope code ~6199). If remill mis-models the load's base (spill/reload) → fix the remill
   semantic. g130 claimed AA is conservative (single scope) — RE-VERIFY that on THIS load; it may be wrong.
4. Verify: AZ_FUEL=ALL relift of hello-world (avoids the hang) → `g116 create_logical_items content.len`
   should become 1, then remove the fuel + the loop caps and confirm text POSITIONS without force_ifc.
5. Once content.len is correct generally: REVERT force_ifc (fc.rs), the 6 out-param hacks, the g137-g139
   loop rewrites, all g147* markers, the loop caps. The deep fix makes them all unnecessary.

**KEY past eliminations (don't redo):** opt-level=1 (regression g140); AZ_LOWOPT_FNS per-fn O0 (no effect);
4 source loop-rewrites g136-g139 (insufficient); enforce_sp __az_indirect_dispatch wrap (real leak fix, KEPT,
not this bug); initializes((672,680)) DSE flag (no effect). The minimal `fn()->Vec` repros lift+execute
CORRECTLY — only the FULL optimized fn mis-executes, so a from-scratch minimal repro may need real register
pressure / the exact spill layout to trigger it.

---
## DEEP-FIX g148 (2026-06-08) — the optimized IR loop GUARD is correct; bug is downstream/runtime

Captured `collect_and_measure`'s optimized IR via AZ_REMILL_KEEP_SCRATCH=1 relift. collect_and_measure is
INLINED into layout_ifc's .opt.ll (`__az_dep_<runtime>.opt.ll`, runtime-named; load_base = runtime(layout_ifc
0x109f2ff08) − static(0x9d7f08) = 0x109558000). Found the dom_children loop (markers 0x606A4=394916 via
[W28]/[W28+16]; W28=0x606A4). The guard chain (verified line-by-line):
```
%v.i8930 = load i64 [SP+2240]          ; dom_children.len()  (Vec len field on the stack post-collect())
%3652    = trunc %v.i8930 to i32        ; written volatile to [W28+16]=0x606B4 (marker peek=1)
%v.i7549 = load i32 [W28+16]            ; read-back (7005)
%cmp     = icmp eq i32 %v.i7549, 0      ; guard: enters loop iff len != 0
           br %cmp, loopexit, loopbody
```
⇒ **The lifted+optimized IR GUARD IS CORRECT** — with len=1 it enters. So the mis-lift is NOT in remill's
decode/semantics NOR the transpiler IR for this guard (re-confirms g129's "IR faithful"). The bug is
DOWNSTREAM: (a) llc IR→wasm miscompile of this pattern, (b) the per-wasm STACK RELOCATION making `[SP+2240]`
(the len load) resolve to the wrong slot at RUNTIME, or (c) the loop ENTERS but the body bails on the
node-type read (the stale g136 "loop iterates 0×" was a PRE-force_ifc build — may not hold now).

NEXT (cron): fresh AZ_FUEL=ALL relift of CURRENT build + a CONSTANT marker on the guard's TAKEN BRANCH
(write 0x60BD0=ENTER right after the `br` lands in loopbody, vs 0x60BD4=SKIP in loopexit) to split
"guard skips (len read 0 at runtime)" from "body bails (node-type)". If SKIP with peek-len=1 ⇒ the
[SP+2240] load reads a different slot than the marker's [W28+16] write at runtime (stack-reloc / llc) ⇒
dump the .wasm (wasm2wat) for this block + check the SP-relative offset lowering. If ENTER ⇒ chase the
node-type read in the body (a separate mis-lift). Tooling: scratch .opt.ll at
$TMPDIR/azul-web-transpiler-<pid>/; find collect_and_measure's file by grepping for the 0x606A4 marker
(value 26723=0x6863 stored to [W28]).

## ★★★ DEEP-FIX g149 (2026-06-08) — REDIRECT: the dom_children loop WORKS; the OUT-PARAM content len doesn't propagate ★★★

Fresh AZ_FUEL relift of the CURRENT build (force_ifc + NEON fixes) + a POST-TRAP read of the g148 markers
(added to the harness catch block, line ~633) gave the DECISIVE result:
```
[g148 dom_children] len(0x606B4)=1 | last-seq=TEXT branch (body ENTERED ✓) | node_type=0xc0de7e70 (Text ✓) | text-branch content.len(0x606BC)=1
```
⇒ The dom_children loop **ENTERS, identifies the Text child, and pushes content.len=1 INSIDE collect_and_measure.**
The g136-g139 "loop iterates 0×" was the OLD pre-NEON truncation — RESOLVED. **But** `g116 create_logical_items
content.len=0` and `g129 layout_ifc inline_content.len(0x60680)=0` ⇒ the CALLER reads content.len=0.

**So the real bug = the out-param `content: &mut Vec<InlineContent>`'s LEN does NOT propagate callee→caller**
(callee pushes → len=1 in collect_and_measure; caller's `inline_content.len()` reads 0). This is the ORIGINAL
cross-call Vec-len mis-lift (g129), now precisely pinned to the out-param len store/read across the
`collect_and_measure_inline_content(... &mut inline_content ...)` call in layout_ifc (fc.rs:2441). g134 already
showed callee content.ptr == caller content.ptr (SAME Vec, &mut ptr is fine) — so it's the LEN field
specifically: the callee's `content.push` updates len=1 (on the caller's stack slot via the &mut), but the
caller reads its `inline_content.len` as 0.

**NEXT (cron):** trace BOTH sides of the len in the .opt.ll (already captured at
$TMPDIR/azul-web-transpiler-45034/__az_dep_109f2ff08.opt.ll, layout_ifc inlines collect_and_measure):
(1) the callee's `content.push` len store (does it write len to [caller_inline_content_slot + 16]?),
(2) the caller's `inline_content.len()` read after the call (load [same slot + 16]) — like the dom_children
guard I traced in g148, this load may be IR-correct but read 0 at runtime ⇒ STACK-SLOT/spill mis-lift. Find
the caller read via the marker 0x60680 (=394368) store in the .opt.ll. If the push-store and the caller-read
target DIFFERENT stack slots in the IR ⇒ remill/transpiler frame-offset bug (fixable generally). If SAME slot
but runtime-divergent ⇒ the stack-relocation/llc layer. THIS is the single mis-lift behind force_ifc + all 6
out-param hacks — fixing it generally lets them all be deleted.

## ★★★ DEEP-FIX g150 (2026-06-08) — drop is at layout_ifc→layout_flow; collect→layout_ifc is FIXED ★★★

g149 (current build, AZ_FUEL + POST-TRAP content-ptr read) overturned the earlier picture:
```
[g149 content-ptr] callee content.ptr(0x60690)=0x283e8 | caller inline_content.ptr(0x606A0)=0x283e8 (SAME, ref ok)
                    | caller inline_content.len(0x60680)=1 ✓✓ | ifc_root_index=0xc0de0002
```
⇒ **collect_and_measure → layout_ifc propagation WORKS now** (inline_content.len=1 in the caller; the
out-param fix is sound; the handoff's old "0x60680=0" was a stale PRE-force_ifc reading). The drop is FURTHER
DOWN: `layout_ifc` calls `text_cache.layout_flow(&inline_content, ...)` (fc.rs:2696) with inline_content.len=1
(ptr 0x283e8), but inside layout_flow `content.to_vec()` (cache.rs:5653) → `create_logical_items` sees
content.len=0 (g116). layout_flow ALREADY takes `&Vec` not `&[]` (workaround #5, cache.rs:5631) — yet the SAME
0x283e8 Vec reads len=1 in layout_ifc and len=0 in layout_flow. So the cross-call Vec-len mis-lift recurs at
the **layout_ifc→layout_flow** boundary (this hop is NOT out-param'd).

**NEXT (cron, ANALYSIS-FIRST then 1 relift):** add a marker at layout_flow ENTRY (cache.rs ~5639) writing
`content as *const _ as usize` (the ref ptr) + `content.len()` to fresh free-band addrs (e.g. 0x60BD0/0x60BD4),
AZ_FUEL relift, read POST-TRAP. Decides:
- ptr != 0x283e8 ⇒ the `&inline_content` REF mis-lifts across the call (g56 stack-address class) → the deep fix
  is remill/transpiler reference-arg passing (a stack-pointer arg whose value doesn't survive the call).
- ptr == 0x283e8 but content.len()==0 ⇒ the len LOAD through the ref mis-lifts (load [0x283e8+16] reads 0) →
  same as the dom_children.len IR I traced in g148 (IR correct, runtime wrong) → it's the llc/wasm/stack-reloc
  layer, NOT remill decode.
This is the SAME systemic mis-lift behind every out-param hack — collect→layout_ifc was patched, layout_flow
is the next unpatched hop. The DEEP fix (remill/transpiler) kills the whole class. Captured .opt.ll for
layout_flow: find its own __az_dep_<runtime>.opt.ll (layout_flow static via `nm | grep
TextShapingCache11layout_flow`; runtime = static + load_base 0x109558000) OR it's inlined into layout_ifc's.

## ★★★ DEEP-FIX g150 RESULT (2026-06-08) — ROOT CAUSE: Vec-len LOAD-THROUGH-POINTER mis-lifts ★★★

`[g150 layout_flow] content.ptr(0x60BD0)=0x283e8` (== layout_ifc inline_content.ptr 0x283e8 → REF IS CORRECT) |
`content.len-via-ref(0x60BD4)=UNSET` (the marker DROPPED). 0x283e8 is a relocated mini-stack addr (~0x28000),
so NOT a relocation bug. ⇒ **ROOT CAUSE: loading the Vec `len` field THROUGH A POINTER (`[content_ptr+0x10]`
= `ldr x,[reg,#0x10]`) mis-lifts at runtime (reads 0) AND destabilizes the lifted control flow (drops the very
next volatile marker write)** — the pointer is correct, the field holds 1. SAME signature as every
field-through-pointer read this session (g147g enum disc, the FC field). It is CONTEXT-dependent: SP-relative
len loads (dom_children.len = `load [SP+2240]`, g148) read CORRECTLY (marker=1); but a len load through a
register-held pointer (`[content_ptr+16]`) mis-lifts. The out-param "fix" works precisely because the caller
reads its OWN Vec SP-relative (relocated OK), not through a passed pointer; g56's Box::new works because heap
addrs aren't involved in this load form.

**THE DEEP FIX TARGET (narrowed to one load form):** `ldr Xt, [Xn, #0x10]` where Xn holds a pointer into the
(relocated) stack, loading a struct field at +16. It is IR-correct (g148 proved the analogous guard IR is
faithful) but mis-EXECUTES + corrupts control flow. NEXT: lift layout_flow standalone (nm | grep
TextShapingCache11layout_flow → static; bytes via python) → find the `ldr x,[reg,#0x10]` for content.len →
inspect the lifted IR + the .wasm (wasm2wat the block) for THIS load: is the address (Xn+16) computed with the
relocated Xn or a raw Xn? Does llc lower it to a load from the wrong wasm offset? The "drops the next marker"
says the load's lifted code does a mis-lifted BRANCH (br_table/computed) — likely remill modeling the
`ldr`+writeback or the pre/post-index form wrong, OR the transpiler's pointer-translation pass mishandling a
register that holds a relocated-stack pointer used as a load base. Compare a WORKING field-through-ptr load
(some struct read that lifts fine) vs this one to isolate the differing instruction encoding. Fix in remill
(the ldr semantic/decoder) or the transpiler (pointer-base translation) → kills force_ifc + all 6 out-param hacks.

## DEEP-FIX g151 (2026-06-08) — relocation RULED OUT; bug is field-thru-ptr load in the MINI wasm

Checked the transpiler relocation: `relocate_stack_if_non_mini` (transpiler_remill.rs:2692) only patches the
SP-INIT value (per-wasm base = STACK_BASE_FIRST 0x30000 + slot*0x20000); it does NOT translate pointers/loads.
content_ptr=0x283e8 is a MINI-stack addr (~0x28000, BELOW the 0x30000 relocation base) ⇒ **the layout runs in
the UN-RELOCATED mini wasm** ⇒ relocation is NOT the cause (ruled out). So: a field load through a VALID
mini-stack pointer (`[0x283e8+16]`) mis-lifts, while the SAME address read SP-relative (layout_ifc's
inline_content.len, g149) reads 1. The "drops the next volatile marker" signature ⇒ NOT a plain wrong-value
load; the load's lifted BLOCK diverges control flow.

ELIMINATED so far: remill decode/truncation (__remill_error=0, static IR clean); the loop (works, g148);
collect→layout_ifc out-param (works, len=1, g149); the ref ptr (correct 0x283e8, g150); stack RELOCATION
(layout is mini, un-relocated, g151). REMAINING candidates: (1) alias-scope store-forwarding — the load is
`!alias.scope !3 !noalias !0`; a `!0`-scoped store that aliases at runtime but is marked noalias could let LLVM
forward/reorder wrongly (g130 dismissed this for a DIFFERENT load — RE-CHECK on THIS field-thru-ptr load);
(2) llc/wasm lowering of `i64.load offset=16` from a register base in this block; (3) the guest-memory MIRROR
mapping for the page holding 0x283f8 differing between the SP-relative access and the pointer access.

NEXT (cron): find the content.len load (`load [content_ptr+16]`) in layout_flow's lifted IR — layout_flow is
likely inlined into layout_ifc's .opt.ll (capture fresh w/ AZ_REMILL_KEEP_SCRATCH; find via the 0x60BD0 marker
store = content ptr, then the adjacent content.len load). Check its !alias.scope/!noalias vs the SP-relative
load that WORKS, and whether a !0 store sits between. If the load is store-forwarded a value from a non-aliasing
store ⇒ tighten the transpiler's alias metadata (M10-B1 ~6199: put guest-stack loads in a scope that aliases
guest-stack stores) OR mark cross-call-returned-Vec field loads volatile in a post-pass. Then AZ_FUEL relift →
content.len=1. Server w/ g150 markers is UP on :8800 (AZ_FUEL build) for re-reads.

## DEEP-FIX g152 (2026-06-08) — the KEY puzzle: the field-thru-ptr READ diverges control flow (not a wrong value)

Refined the g150 symptom: the marker `write_volatile(0x60BD4, content.len() as u32 | 0xC0DE0000)` reads back
'unset' (NOT 0xC0DE0000). If content.len() simply returned 0 (memory overwrite / frame overlap), the OR would
make the store write 0xC0DE0000 and the harness would show len=0 — NOT 'unset'. So **the `content.len()` read
does NOT return a clean value; the READ ITSELF diverges the lifted control flow and the next volatile store is
never executed.** SAME signature as g147g (enum-disc read) + the FC-field reads. ⇒ rules OUT simple
memory-overwrite/frame-overlap (g152). The load instruction's LIFTED CODE mis-executes (a mis-lifted branch
in/after the `ldr x,[reg,#0x10]` block), not just a wrong loaded value.

Both loads are guest-mem scope !3 (alias scope is NOT the differentiator — g152 rules it out too). The
difference remains SP-relative (works) vs register-pointer base (diverges). layout_flow(0x97126c) is INLINED
(no own .opt.ll; the content.len load is in layout_ifc's .opt.ll where layout_flow is inlined, yet content is
still accessed via the ptr 0x283e8, not direct).

STATE: 7 hypotheses eliminated (remill-decode, the loop, out-param hop, ref-ptr, relocation, alias-scope,
memory-overwrite). The bug is pinned to: the lifted code of a `ldr Xt,[Xn,#0x10]` whose base Xn is a
register-held pointer (not SP) DIVERGES control flow at runtime. This needs HANDS-ON wasm tracing (the static
IR is faithful per g148; the divergence is in the wasm/runtime, which V8 executes faithfully ⇒ the wasm itself
must encode a divergent branch). NEXT: wasm2wat the layout_ifc wasm, find the content.len load block (the
i64.load offset=16 whose result feeds an icmp/br_table), and see what branch follows it + why it's taken.
Likely a remill modeling of a load-with-side-effect (pre/post-index, or a flag-setting variant) OR an
__az_indirect_dispatch the transpiler wraps around a non-inlined Vec::len call. RECOMMENDATION TO USER: this
final mechanism is a focused remill/wasm single-stepping task — productive but slow in cron cycles. Consider a
dedicated session, OR let the cron continue the wat-trace. FC-dispatch fix is COMMITTED + working; this deep
fix deletes the remaining workarounds once cracked.

## DEEP-FIX g153 (2026-06-08) — TWO concrete TRANSPILER fix leads (from reading the transpiler load path)

Read the transpiler's IR post-processing (transpiler_remill.rs:903-927). Two leads directly match the bug
(cross-function field-thru-ptr load mis-executes, post-inline):

**LEAD 1 — retarget_to_wasm32 (transpiler_remill.rs:922) incompleteness.** Its OWN comment (910-921):
"Pointer math happens in i64 with i32.wrap_i64 at memory ops — this breaks CROSS-FUNCTION state-ptr
propagation and is the ROOT CAUSE of ... sret writes going to WRONG WASM ADDRESSES." My bug is exactly
cross-function (content ptr 0x283e8 passed layout_ifc→layout_flow; [content_ptr+16] load reads wrong). If
retarget_to_wasm32 retargets fn BODIES but not the i64/i32 handling of POINTER-TYPED FN ARGS passed across
calls (or a pointer that flows through a call), the load's address math stays i64 → i32.wrap_i64 truncates to
a wrong wasm offset → reads garbage AND (if the wrap produces an OOB/odd addr) diverges. ACTION: read
retarget_to_wasm32; check it handles ptr-typed args + cross-fn ptr flow, not just body datalayout. Likely the
fix: ensure content_ptr (a wasm i32) isn't sign/zero-extended to i64 then wrapped when used as a load base
across the call boundary.

**LEAD 2 — inline alias-scope cloning.** tag_state_accesses (line 908) tags guest accesses with ONE shared
scope (!3) so they alias (g130's claim). BUT when LLVM INLINES a lifted fn multiple times (collect_and_measure
+ layout_flow are inlined into layout_ifc.opt.ll), it CLONES `!alias.scope`/`!noalias` per inline-instance →
cross-inline guest accesses to the SAME global memory get marked NON-aliasing → LLVM forwards a STALE value
(the Vec::new len=0) to layout_flow's content.len load instead of the pushed len=1. Classic scoped-noalias +
inline mis-opt. ACTION: make the guest alias scope EXEMPT from inline-cloning — either use `!noalias` domains
that LLVM won't duplicate for guest mem, OR add `noduplicate`-style handling, OR (simplest) mark guest-memory
LOADS `volatile` in tag_state_accesses (the handoff's repeated finding: only VOLATILE reads are correct; stores
are already `store volatile`, loads are not). A volatile load can't be forwarded/eliminated/cloned-wrong.

**RECOMMENDED FIRST ATTEMPT (cheapest, highest-signal): make guest-memory loads volatile.** In
tag_state_accesses (or a post-pass), change guest `load` (the !alias.scope !3 ones) → `load volatile`. Rebuild
azul-dll + AZ_FUEL relift → if content.len becomes 1 (g116/g150) ⇒ CONFIRMED it's an opt-induced load mis-opt
(forwarding/cloning), and volatile is the fix (refine to targeted later for perf). Then REVERT force_ifc + all
6 out-param hacks + the markers and confirm hello-world text POSITIONS. If volatile-loads does NOT fix it ⇒
it's LEAD 1 (the i64/i32 address math) → fix retarget_to_wasm32 for cross-fn ptr args. Either way the deep fix
is now down to 2 specific transpiler changes, both testable in one relift each.

## DEEP-FIX g154 (2026-06-08) — alias-scope mechanism nailed; LEAD 2 (inline scope-cloning) is the likely root

tag_state_accesses (transpiler_remill.rs:6628): tags the lifted body's State/alloca accesses as HOST scope
(AZ_HOST_LIST = !0); GUEST memory accesses (remill inttoptr / __remill_read/write_memory lowerings) are
PRE-tagged GUEST scope (AZ_GUEST_LIST = !3) and skipped here. So in the .opt.ll: `!alias.scope !0`=State,
`!alias.scope !3`=guest memory (the stack is guest mem; BOTH SP-relative and content_ptr loads are !3 → they
alias WITHIN one function, g130 holds). ⇒ **LEAD 2 is the root: LLVM CLONES scoped-noalias metadata on INLINE.
collect_and_measure + layout_flow inline into layout_ifc → the !3 guest scope is duplicated per inline-instance
→ the cross-inline store(len=1, in collect_and_measure's inline) and load(len, in layout_flow's inline) of the
SAME stack memory get DIFFERENT scope clones → marked non-aliasing → LLVM forwards the STALE Vec::new len=0 to
the load.** This matches every symptom (works within a fn / SP-relative; fails cross-inline through a ptr;
"drops the marker" = the forwarded-poison/UB path or a follow-on br on the bad value).

**FIX (next cycle — implement + 1 relift to verify):** make GUEST loads volatile so LLVM can't forward across
the (mis-cloned) scope. Guest loads are pre-tagged `!alias.scope !3`. Add a post-pass AFTER the guest-lowering
(find via the grep above — where inttoptr guest loads / __remill_read_memory are emitted) OR a targeted IR
post-pass: for any `%X = load TYPE, ptr PTR, align N, !alias.scope !{AZ_GUEST_LIST}, !noalias !{AZ_HOST}` →
insert `volatile` after `load`. (Stores are already `store volatile`.) ALTERNATIVELY (cleaner, no perf hit):
strip the alias.scope/noalias from GUEST accesses entirely (keep on HOST/State for SROA) so LLVM is
conservative on guest mem and never forwards across inlines — but this may regress the State-SROA size win;
volatile-guest-loads is the safer first test. Rebuild azul-dll + AZ_FUEL relift → g116/g150 content.len should
become 1. If confirmed → REVERT force_ifc + all 6 out-param hacks + g147* markers + loop caps; hello-world
text POSITIONS; the WHOLE systemic mis-lift class is fixed at the transpiler. THIS is the deep fix.

## ★★★ DEEP-FIX g155 (2026-06-08) — FIX IMPLEMENTED: volatile guest loads (transpiler_remill.rs:6218-6254) ★★★

Applied the lead-2 fix: changed all 6 `__remill_read_memory_{8,16,32,64,f32,f64}` definitions (alwaysinline)
from `%v = load TYPE, ptr %p, ...` → `%v = load volatile TYPE, ptr %p, ...`. The WRITES were ALREADY
`store volatile` — this fixes the exact read/write asymmetry the handoff kept hitting. Volatile guest reads
can't be forwarded/eliminated/clone-mis-opt'd across LLVM inlines → kills the cross-inline alias-scope-clone
that forwarded the stale Vec::new len=0 to layout_flow's content.len read. Rebuilt azul-dll (transpiler) +
AZ_FUEL relift IN FLIGHT (task bxduznbz2). EXPECT: g116/g150 content.len=1 → text lays out → g132
overflow_size>0 → NO hang (the hang was empty-content-starved BreakCursor; with content.len=1 it terminates).
If CONFIRMED: this is THE deep fix — REVERT force_ifc (fc.rs) + all 6 out-param hacks + g147* markers + loop
caps + repr(C,u8); hello-world nested text POSITIONS; the whole systemic Vec-len/iterator/field-thru-ptr
mis-lift class is fixed at the transpiler (correctness; volatile loads cost some wasm perf — can refine to
target only cross-inline-risky loads later). If NOT fixed: fall back to LEAD 1 (retarget_to_wasm32 i64/i32 for
cross-fn ptr args) per g153.

## DEEP-FIX g155 RESULT + g156 (2026-06-08) — volatile-loads did NOT fix it; lead 2 RULED OUT; new hypothesis = frame/save-slot CLOBBER

volatile guest loads (g155) relifted → NO change: g116 content.len=0, g150 content.len-via-ref='unset', still
traps. So it's NOT alias-scope forwarding (lead 2 RULED OUT). The volatile change is KEPT (principled, matches
write-side; revert-at-cleanup; small perf cost) but is not the fix.

★ RE-INTERPRETATION of the g150 'unset': `write_volatile(0x60BD4, content.len() as u32 | 0xC0DE0000)` reads
back 'unset' = (val & 0xffff0000) != 0xC0DE0000. That happens when content.len() returns a value whose
bits 16-31 carry a bit outside 0xC0DE's pattern — i.e. content.len() = POINTER-SHAPED GARBAGE (a stack addr,
high bits set), NOT a clean 0 and NOT a dropped write. This is EXACTLY g129's original symptom ("len reads
pointer-shaped garbage ~0x2A000, a stale stack addr"). ⇒ **[0x283f8] (inline_content's len slot) holds 1 when
layout_ifc reads it (g149) but a STACK ADDRESS when layout_flow reads it (g150) — it's being OVERWRITTEN
across the call.** The "drops the next marker" earlier (g147g/g150) is then the OR'd-garbage misreading as
unset, OR a follow-on branch on the garbage value — NOT a control-flow divergence from the load itself.

⇒ NEW ROOT HYPOTHESIS (g156): the enforce_sp_preservation save/restore (transpiler_remill.rs:5286) OR
layout_flow's lifted stack frame writes a callee-saved register (which holds a stack pointer ~0x2A000) to a
save slot that OVERLAPS layout_ifc's inline_content len slot [0x283f8]. The CS_OFFSETS / frame-size math for
the wrapped call places a save slot inside the CALLER's live frame. This is a TRANSPILER stack-frame bug (the
g56 / "frame overlap" class g129 partially explored, CS_OFFSETS 880/896).

NEXT (cron): (1) CONFIRM garbage-vs-0: change the g150 marker to write content.len() RAW (no |0xC0DE0000) to a
fresh addr → read the actual value; if ~0x2A000-ish stack addr ⇒ clobber confirmed. (2) Then dump
enforce_sp_preservation's save-slot offsets for the layout_ifc→layout_flow call + compare to where
inline_content lives ([SP_layout_ifc + off] = 0x283e8). If a save slot ∈ [0x283e8, 0x28400) ⇒ found it; fix
the CS_OFFSETS / frame-size computation so save slots are BELOW the caller's locals (or in a dedicated region).
This (not volatile, not alias) is the systemic fix. (3) Alt: lead 1 (retarget i64/i32) still open.

## ★★★★ DEEP-FIX g157 (2026-06-08) — TEXT LAYS OUT! (volatile guest loads + g156 markers) ★★★★

`[g132] overflow_size = 98.38 x 16.29 ✓✓✓ TEXT LAYS OUT (h>0)`, STABLE across re-runs, rc=0,
__remill_error=0. content.len=1 end-to-end (g119 cli-entry=1, g133 inline_content.len=1, g136 dom_children=1,
g134 _impl content.len=1). solveLayoutReal COMPLETES (no trap → success path). rect[3]=(8,16,784,29) = a
text-div with real height; 98.38 wide ≈ "Increase counter" on one ~16px line. **hello-world nested text
POSITIONS.** (The harness "5 of 5 rects differ" = its stale FLEXBOX fixture vs hello-world — not a failure.)

⚠ BUT: g155 (volatile-loads ONLY, no g156 markers) FAILED (content.len=0, trap); g156 (volatile + the new RAW
content.len markers at cache.rs:5639, 0x60BD8/0x60BDC) WORKS. So the g156 markers are the LOAD-BEARING
difference = a HEISENBUG (reading content.len()/`*[ptr+16]` at layout_flow entry materializes the value /
perturbs codegen so to_vec's content.len reads 1). The volatile-loads fix is REAL + part of it (it makes the
forced read return the right value) but is NOT sufficient alone. So this is NOT yet the fully-clean
delete-all-workarounds fix — it depends on a marker. DEEP-FIX MECHANISM CONFIRMED THOUGH: the bug is a
content.len read that the optimizer mis-handles unless FORCED (volatile / materialized) — consistent with the
whole session's "only volatile/forced reads are correct."

NEXT (cron): (1) running the decisive test now — REMOVE the g156 raw markers, keep volatile-loads, relift; if
it FAILS ⇒ heisenbug confirmed, need to force the to_vec content.len read in the transpiler (not source). (2)
The clean fix: find the to_vec/calculate_id content.len LLVM load in the .opt.ll that the g156 marker forces;
make THAT specific load volatile/un-SROA'd in the transpiler (it's the SROA'd len the optimizer forwards
wrong — likely NOT a __remill_read_memory load but a post-SROA SSA value, which is why volatile-guest-loads
alone didn't catch it). (3) Once clean: REVERT force_ifc + 6 out-param hacks + ALL markers + caps → confirm
text still POSITIONS → the systemic fix is done. KEEP: volatile guest loads (transpiler_remill.rs:6218-6254).

## DEEP-FIX g158 (2026-06-08) — root: content.len SROA'd to a stale SSA value; clean force-read; transpiler fix is the last mile

Decisive test (volatile-loads, g156 markers REMOVED) → FAILED (content.len=0, trap). So the g156 marker's
DIRECT volatile read of `[content+16]` was the load-bearing piece. ⇒ ROOT (final): `content.len()` in
layout_flow's `content.to_vec()` (cache.rs:5653) is SROA'd by LLVM into a STALE SSA value — the optimizer
forwards the Vec::new len=0 from BEFORE the inlined collect_and_measure→layout_flow call, ignoring the
push(len=1), because the inlined functions' guest-scope alias metadata got cloned (so the store/load look
non-aliasing to the SSA-level mem2reg/GVN). It's a POST-SROA SSA value, NOT a __remill_read_memory load — which
is why making __remill_read_memory volatile (g155) didn't catch it (no load left to mark volatile).

FIX (working, g158): keep the volatile-guest-loads (transpiler_remill.rs:6218-6254) AND add a VOLATILE
force-read of the Vec header `[content+16]` at layout_flow entry (cache.rs:5639, read_volatile + black_box,
web_lift-gated) — this re-materializes the real len and defeats the SROA-forwarding. Rebuild+relift testing
now (b712j6gut). EXPECT: g132 overflow_size>0 (text lays out) WITHOUT the diagnostic markers.

STATE: hello-world NESTED TEXT LAYS OUT (g157, overflow_size 98x16, stable). Current workarounds: force_ifc
(dispatch) + volatile-guest-loads (transpiler, KEEP — principled) + this 1 force-read (cache.rs). FULLY-CLEAN
(delete force_ifc + out-param hacks + the force-read) needs the TRANSPILER to stop the SROA-level
cross-inline forwarding: the real fix is in tag_state_accesses / the inline alias-scope handling — make the
guest alias scope NOT get cloned on inline (e.g. emit `!noalias` with a domain that survives inlining as ONE
scope, OR run the guest-mem GVN/SROA with conservative AA). That kills the forwarding at the SSA level (where
the bug actually is) so no source force-read is needed. THAT is the last mile of the deep fix. Until then the
force-read is a minimal principled stabilizer (1 line, web_lift-gated, vs the 6 out-param hacks it's replacing
the NEED for — though those stay until force_ifc can go too).

================================================================================
## g159 (2026-06-08): REAL ROOT CAUSE = 2 UNDECODED FP INSTRS in layout_ifc
   (NOT SROA/aliasing — that whole g155-g158 theory was WRONG)
================================================================================

The g157 "win" (text lays out) was FRAGILE: it depended on a g156 DIAGNOSTIC MARKER's
side effect (read content.len + write_volatile to a fixed addr). Replacing it with a clean
`read_volatile([content+16]) + black_box` (g158) did NOT work — relift showed
`g116 create_logical_items content.len=0`, trap, SAME as before. So volatile-guest-loads +
clean force-read were NOT sufficient, and the "post-SROA stale-len forwarding" theory was a
RED HERRING.

REAL ROOT CAUSE (found via the proven 2026-06-06 method): `layout_ifc`
(_ZN11azul_layout7solver32fc10layout_ifc17h607747da8645be28E @ file 0x9d7f48) contains
**2 instructions remill's fork did NOT decode** (silent CFG truncation, rc=0, NO __remill_error
at runtime — the missing_block=44 trap is a downstream symptom of the garbage):
  1. `frintm s0, s0`  (0x1e254000) @ file 0x9dc0b4 — FP round toward -Inf (floor)
  2. `fabd  s0, s0, s13` (0x7eadd400) — FP |a-b| (scalar SIMD ASISDSAME)
Both were `return false` decoder STUBS (Decode.cpp). The CFG truncation after frintm/fabd
left layout_ifc's downstream blocks (the inline-content Vec build / content.len) unlifted →
content.len reads garbage(0) → nested text never positions → eventual missing_block trap.

HOW FOUND (reusable):
  • relift w/ AZ_REMILL_KEEP_SCRATCH=1; grep scratch `*.lifted.ll` for `call.* @__remill_error`
    → 27 fns; layout_ifc had 2.
  • Extract fn bytes: __text addr/off from `otool -l`; foff = fileaddr - taddr + toff.
  • Lift standalone: `remill-lift-17 --arch aarch64 --address <rt> --bytes <hex> --ir_pre_out x.ll`
    (--ir_pre_out keeps the pre-opt IR; the error blocks call `HandleInvalidInstruction`).
  • The undecoded instr is the one IMMEDIATELY AFTER the last decoded semantic (here FDIV_Scalar32
    → frintm). Disasm `otool -tV`, find `fdiv s` and read the NEXT line.
  • CONFIRM decode per-instr: lift `<word><ret=c0035fd6>`; count `HandleInvalidInstruction`:
    **1 = decoded (just the trailing fall-through past ret); >=2 = UNDECODED.**
    (Lifting a bare 4-byte word w/o the `ret` terminator ALWAYS shows HandleInvalid for the
    zero fall-through — that artifact made an earlier scan flag everything; the `ret` fixes it.)

THE FIX (remill fork, NO azul source change — exactly the user's "no source hacks" goal):
  • Arch.cpp: real `TryDecodeFRINTM_S_FLOATDP1` + `_D` (mirror FABS_S_FLOATDP1) + 
    `TryDecodeFABD_ASISDSAME_ONLY` (sz bit22 → _S/_D suffix, scalar Fd/Fn/Fm like FADD_S).
  • Decode.cpp: deleted the 3 stubs (2239/2277/40740).
  • Semantics/BINARY.cpp: DEF_SEM FRINTM_S/D (__builtin_floor*), FABD_Scalar32/64 (std::fabs(a-b));
    DEF_ISEL FRINTM_{S,D}_FLOATDP1, FABD_ASISDSAME_ONLY_{S,D}.
  • Rebuild: `ninja remill-lift-17 lib/Arch/AArch64/Runtime/aarch64.bc` in remill-install/build/remill,
    then `cp .../Runtime/aarch64.bc .../install/share/remill/17/semantics/aarch64.bc`.
  VERIFIED: layout_ifc standalone HandleInvalidInstruction 3→1 (only trailing left). frintm→FRINTM_S,
  fabd→FABD_Scalar32 semantics. Relift+test in flight (bn0ypensa).

WHY the g158 force-read "helped" intermittently: perturbing the fn shifts the optimizer/decoder
block layout so the truncated region sometimes lands off the hot path — a heisenbug, never a fix.
IMPLICATION: once this relift confirms text lays out, the force-read (g158) + force_ifc + the 6
out-param hacks are ALL likely revertable (they were all working around CFG truncation from
undecoded NEON, the SAME class as the 2026-06-06 fixes). Revert + relift to confirm each.
NEXT after confirm: scan the other hot fns (collect_inline_content_recursive, create_logical_items,
break_one_line, perform_fragment_layout, layout_bfc, layout_dom_recursive) for more undecoded FP/NEON
via /tmp/scan_fn_undecoded.py; fix any in the same remill rebuild.

================================================================================
## g160 (2026-06-08): frintm/fabd fix CONFIRMED partial; content.len=0 is a
   SEPARATE value mis-lift (host/guest alias-scope stale-forward) — experiment live
================================================================================

RELIFT after the frintm/fabd remill fix (AZ_FUEL=ALL, b2aslmebc):
  • non-fuel relift: missing_block TRAP → **HANG** (exit 124). The frintm/fabd fix REMOVED the
    immediate missing_block trap (CFG no longer truncates in layout_ifc) → code runs further →
    hits the break_one_line cursor loop → hangs. PROGRESS, but not the end.
  • AZ_FUEL relift: traps at __az_fuel (the loop). g116 `content.len = 0` STILL. So frintm/fabd
    were NOT the cause of content.len=0 — that is a SEPARATE, persistent bug.
  • missing_block=44 PC 0x2c00ee8938 → 0xee8938 = `get_ua_property` (jump-table dispatch). Old
    memory flagged its missing_block as "incidental, not the layout" — a known-benign artifact.

content.len=0 is a VALUE MIS-LIFT, NOT a decoder gap:
  • EVERY function touching `content` (collect_inline_span_recursive, layout_ifc[fixed],
    layout_flow, create_logical_items) is decoder-CLEAN (0 undecoded after frintm/fabd).
  • In layout_flow's OPTIMIZED per-dep IR (current volatile-transpiler build): ALL 663 guest
    loads are `load volatile` (0 non-volatile). So the volatile fix IS fully applied.
  • Cron-confirmed: a VOLATILE read of content.len gives 1 (memory HAS len=1); the OPTIMIZED
    loop-bound read gives 0. ⇒ content.len got SROA'd to a NON-volatile HOST-scoped register that
    forwards a STALE 0 (Vec::new) across collect's guest push-store — it never reaches the
    volatile guest load. host⊥guest (tag_state_accesses) proves the guest store NoAlias the host
    len register → the optimizer keeps Vec::new's 0.

LIVE EXPERIMENT (bfyu01con): added `AZ_NO_HOST_SCOPE=1` gate to tag_state_accesses
  (transpiler_remill.rs:6628) — when set, State/local loads/stores are NOT host-tagged →
  conservative AA → the guest push-store can no longer be proven NoAlias the host len register →
  no stale forward. Rebuild dylib + relift hello-world w/ AZ_NO_HOST_SCOPE=1 + AZ_FUEL=ALL.
  EXPECT: g116 content.len → 1 (if the scope-separation hypothesis is right). If so: refine to
  tag ONLY the never-escaping state_buf (keep SROA where safe), NOT stack/escaped accesses; then
  content.len fix is in the TRANSPILER (no azul source) = the user's goal.

ALSO FOUND (downstream decoder gaps, fix in a later remill batch — same FRINTM pattern):
  scalar FP DP1 stubs FSQRT_S (get_line_constraints, 0x1e21c000), FRINTP_S (ShapeDefinition::
  get_size, 0x1e24c000=ceil), + NEON in IntrinsicSizeCalculator. TABLE fns
  (calculate_column_widths_*) have 30+24 NEON gaps but are OFF hello-world's path (no table) —
  skip. These are all AFTER content.len so they don't block the text; fix once content.len=1.
  Scanner: /tmp/scan_fn_undecoded.py. Decode-check: lift `<word>c0035fd6`, HandleInvalid==2→stub.

================================================================================
## g161 (2026-06-08): content.len=0 PRECISELY LOCALIZED to the out-param len-STORE;
   host/guest-scope hypothesis ELIMINATED
================================================================================

EXPERIMENT (bfyu01con): AZ_NO_HOST_SCOPE=1 (tag_state_accesses no-op → conservative AA).
RESULT: content.len STILL 0. ⇒ the host⊥guest stale-forward is NOT the cause. 4th hypothesis
eliminated (after: SROA-volatile-loads, clean force-read, frintm/fabd-only, host/guest-scope).

PRECISE LOCALIZATION (the key progress): content.len=0 is NOT the dom_children.len() read
(fc.rs:7008 — that's ALREADY worked around by the g139 volatile round-trip at 7003-7005, and the
harness PROVES it works: g148 `dom_children len=1`, loop runs, finds the Text node 0xc0de7e70,
`text-branch content.len=1`). So **content IS built len=1 in collect's REGISTER**. The loss is
DOWNSTREAM: g116 create_logical_items reads content.len=0.

THE BUG = the out-param len-STORE to memory:
  • collect_and_measure fills `&mut inline_content`. After the loop, collect's REGISTER view of
    inline_content.len = 1 (g148). But the MEMORY at [&inline_content+16] reads 0 when layout_flow
    later does `content.to_vec()` (g116 downstream = 0).
  • SIGNATURE: [&inline_content+0] (data ptr = 0x283e8) is CONSISTENT callee↔caller↔downstream
    (g148/g149), but [&inline_content+16] (len) is 1-in-collect-register, 0-in-memory-downstream.
  • So the `str x_len, [out_ptr+0x10]` out-param write either (a) stores a STALE 0 value, or
    (b) stores to a WRONG address (+0 lands right, +16 doesn't — same base). collect is
    decoder-CLEAN (0 undecoded), so it's an OPTIMIZATION/value mis-lift, not a decode gap.
  • This is the SAME class the 6 out-param hacks target — but collect ALREADY uses out-params,
    so the out-param pattern does NOT fully fix the len-store. The deep fix must address it.

NEXT (analysis-first):
  1. In layout_ifc's per-dep .opt.ll (collect inlines into it), find the `store ... [out_ptr+16]`
     for inline_content.len vs the `[out_ptr+0]` ptr store. Compare value operand + address
     computation. The "+0 ok, +16 broken, same base" is the crux — likely the len value fed to the
     store is a stale SSA (Vec::new 0) OR the +16 address truncates/mis-computes (wasm32 retarget).
  2. If value-stale: it's cross-call SSA forwarding the optimizer does despite the call writing
     memory — fix = mark sub_ calls as clobbering all memory (they already lack readonly; verify),
     OR a transpiler pass forcing a reload after out-param-call returns.
  3. Reliable memory probe: the markers DROP when they read the field (documented). Need a probe
     that reads [&inline_content+16] via a path the lift can't SROA — e.g., a wasm-side peekU32 of
     the struct address (capture &inline_content to a fixed mini addr FIRST, then peek +16).
  KEEP: frintm/fabd remill fix (real, trap→hang). AZ_NO_HOST_SCOPE gate (off by default, diag).
  Downstream decoder gaps to batch later: FSQRT_S/FRINTP_S (get_line_constraints/ShapeDefinition).

## g162 (2026-06-08): DECISIVE store-vs-read peek — IN FLIGHT (bi96cu8zd)
Harness now peeks the Vec struct DIRECTLY from wasm memory at the captured &inline_content (mini
addr 0x606A0, =0x283e8). peekU32 reads linear memory (reliable, not SROA'd). g162 line reports
mem[base+16]: ==1 ⇒ store OK, bug = to_vec READ (read-side); ==0 ⇒ bug = collect out-param len-STORE.
CAVEAT: if 0x283e8 is a RELOCATED guest stack addr, peekU32(0x283e8) may read the wrong wasm offset
(stack-reloc ~0x30000) → garbage; if so, capture needs an un-relocated addr or a wasm-side helper.
Harness-only change (no dylib rebuild); relift AZ_FUEL=ALL. NEXT: read g162 → fix the indicated side.

================================================================================
## g163 (2026-06-08): KEY CORRECTION — the len read IS a faithful VOLATILE GUEST
   LOAD (not read-side SROA). Bug is store-value OR cross-fn address. NOT read-SROA.
================================================================================

Traced the EXACT to_vec len read in layout_flow's opt.ll (__az_dep_109121224.opt.ll):
  %43 = trunc i64 %38 to i32                          ; content ptr (X0 arg) → i32
  %44 = add i32 %43, 16                               ; content+16 = Vec len addr
  %v.i1387 = load VOLATILE i64, ptr [%44], !scope !3  ; ← the len read: FAITHFUL volatile guest load
  store i64 %v.i1387, ptr %W27 ; %64 = load %W27 ; mul %64,112  ; → to_vec alloc size
⇒ The len read is a VOLATILE GUEST LOAD of [content+16] (reads memory at runtime). So it is NOT
an SROA'd stale-host-value. BOTH the prior "SROA forwarding" theory AND the cron's "SROA'd len
read reads 0" framing are WRONG. A faithful volatile load returning 0 means:
  (a) memory[content+16] genuinely = 0 (collect's STORE wrote 0 or to a different addr), OR
  (b) content ptr (%38) differs from collect's store addr (cross-fn pointer mismatch).
NOT (a per-module reloc) — layout_ifc & layout_flow are both fns in the SAME azul-mini.wasm
(wasm-function[3093]); they share global[0] SP → same stack. So a relocation mismatch between
them is ruled out (relocate_stack_if_non_mini is per-MODULE, not per-fn).

g162 peek was AMBIGUOUS (read [0x283e8+16]=1 but data_ptr=0x4/cap=garbage → 0x283e8 is likely a
relocated/data-ptr addr, peekU32 read the wrong physical wasm offset; the "1" may be coincidental
element data). So store-vs-read is STILL not 100% settled by the peek — but the IR proof (faithful
volatile load) says it is NOT read-side SROA. Most likely STORE-side: collect's out-param len write.

NEXT DIAGNOSTIC (precise): instrument collect RIGHT AT the len-store — capture the store ADDRESS
and VALUE (not via .len() which drops; via raw ptr write of the &mut struct base + a raw volatile
read-back of [base+16] from collect's own context). If collect's own read-back = 1 → memory has 1
in collect's view → the mismatch is cross-fn (compare collect's struct base vs layout_flow's %38
content ptr — they should be identical 0x283e8; if a reloc/trunc diverges, that's the bug). If
read-back = 0 → collect's store value/addr is wrong (store-side value mis-lift). EITHER way the fix
is in remill (faithful store/addr) or the transpiler (reload/clobber), NOT azul source.
ELIMINATED so far (6): SROA-volatile-loads, clean-force-read, frintm/fabd-only, host/guest-scope,
read-side-SROA (IR-disproven), per-module-stack-reloc (same module). KEEP: frintm/fabd fix.
Harness g162 mem-peek block added (layout-flexbox.js ~643) — diagnostic, keep.

================================================================================
## g164 (2026-06-08): REMILL DECODER FIX COMPLETE for the text-layout path (verified)
================================================================================

User: "fix the remill thing in our fork". DONE — comprehensive FP/NEON decoder batch in the fork
(/Users/fschutt/Development/azul/third_party/remill), all verified decoding, all text-path fns clean.

Added (decoder→Arch.cpp, stub deleted from Decode.cpp, semantic+DEF_ISEL→BINARY.cpp/SIMD.cpp):
  • Scalar FP DP1 (S+D): FSQRT, FRINTN, FRINTP, FRINTM, FRINTZ, FRINTA, FRINTX, FRINTI
    — semantics use wasm-native __builtin_* (sqrt/nearbyint/ceil/floor/trunc); FRINTA = ties-away
    via floor(x+copysign(0.5,x)). (FRINTM + FABD landed earlier g159.)
  • Vector FP: FCMGT (.2s/.4s/.2d, per-lane >-mask via UWriteV*), FCVTN (.2s<-.2d narrow f64->f32).
  • FABD scalar (|a-b|, ASISDSAME, _S/_D suffix).
GOTCHA (FCVTN): its TryExtract populates `inst.sz` (bit22), NOT `inst.size` — decoder must read
  `data.sz`. (CVTF/FP_ASIMDSAME use `data.size`.) First FCVTN attempt checked data.size → wrongly
  rejected (HandleInvalid was the FIRST call = extraction OK but decode returned false). Fixed.
VERIFIED: decode-check (`lift <word>c0035fd6`, HandleInvalid==1=ok) PASSES for all 10. Re-scan
  get_line_constraints / ShapeDefinition::get_size / IntrinsicSizeCalculator / layout_ifc = 0 undecoded.
BUILD: `ninja remill-lift-17 lib/Arch/AArch64/Runtime/aarch64.bc`; cp aarch64.bc → install/share/
  remill/17/semantics/. Both artifacts current (binary 22:33, .bc 22:30). remill fork UNCOMMITTED.
DEFERRED (off hello-world's path — no table): calculate_column_widths_* NEON (uaddw/uaddw2.2d,
  trn2.4s, tbl.16b — 30+24 gaps). Add when table layout is exercised; same Arch.cpp/Decode.cpp pattern.

REMINDER: this decoder fix is NECESSARY for the full text layout but does NOT unblock hello-world
e2e by itself — content.len=0 (g163: store-side/address value mis-lift, NOT a decoder gap) is the
remaining blocker, upstream of these decoded fns. Decoder path is now clean for when content.len is fixed.

================================================================================
## g165 (2026-06-08): observation WALL — markers/peeks cannot resolve content.len.
   Both reads are FAITHFUL volatile loads; &inline_content capture is unreliable.
================================================================================

Wide 64-bit memory dump at the captured &inline_content (0x283e8) does NOT show a clean Vec:
  [-0x20]=0x62965b8(heap) [-0x18]=0xea2ac0 [-0x10]=0xea2bb8 [-8]=0x1 [+0]=0x4 [+8]=0x6296ba8(heap)
  [+0x10]=0x1 [+0x18]=0x41b54e0 [+0x20..]=0
The 0xea2xxx words are CODE/return addresses (they recur in the POST-TRAP error/fn-call ring), i.e.
0x283e8 points into a STACK SPILL region (saved LR/regs + scattered ptrs), NOT the inline_content
Vec {ptr,cap,len}. ⇒ the marker's `&inline_content as *const _ as u32` (0x283e8) is itself
unreliable on the lift (the stack-addr capture mis-lifts, OR the value truncates/relocates wrong).
So the g149/g150/g162 "0x283e8" localization has been a RED HERRING — peekU32(0x283e8) reads spill
garbage, which is why [+0]=0x4 etc. never formed a valid Vec.

ALSO confirmed: create_logical_items' len-feeding load (%v.i7613 → mul×112) is ALSO a FAITHFUL
`load volatile i64, !scope !3` guest load — same as layout_flow's. So at the IR level EVERY len read
on the path is a faithful volatile guest load. None is a non-volatile SROA'd host read. So the
prior/cron "SROA'd len read reads 0" model is not visible in the per-dep IR.

HONEST STATE: markers DROP when they read Vec fields (documented), and peeks of stack addresses are
unreliable (capture mis-lifts / spill overlap). So neither tool can observe the true runtime
memory/value at the len site. I've eliminated 8 hypotheses (SROA-volatile, force-read, frintm/fabd,
host/guest-scope, read-side-SROA, per-module-reloc, store-value, pointer-capture) without a
reproducible mechanism. content.len=0 is real (g116, stable) but its mechanism is below the
resolution of the available instrumentation.

NEXT APPROACH (different tool — the marker/peek path is exhausted): add a TRANSPILER-level runtime
TRACE that logs, to a ring buffer in a peekable fixed mini-addr band, the (addr,value) of EVERY
`__remill_read/write_memory_64` whose addr is in the inline_content stack window during the
collect→layout_flow→create_logical_items window — i.e., instrument the LIFT itself (not azul source,
not markers). Then peek the ring post-trap to see the exact store(addr,1) and the load(addr,?) and
whether their addresses match. That directly shows store-vs-read-vs-address without relying on a
mis-liftable `&inline_content` capture. (Gate by a PC/addr window to keep the ring small.)
KEEP: remill decoder fix (g164, done+verified). Harness g165 wide-peek + g162 + AZ_NO_HOST_SCOPE
gate = diagnostics, keep. ALL UNCOMMITTED.

================================================================================
## g166 (2026-06-08): KEY DIFFERENTIAL — web-text-min (flat) WORKS; bug is nesting-
   specific. Minimal repro web-nested-text.c launched. SP-leak ruled out.
================================================================================

CRUCIAL REFRAME: web-text-min.c (body > "Hello", FLAT) POSITIONS — and it uses the SAME
force_ifc → layout_ifc → collect → layout_flow → to_vec → create_logical_items lift code as
hello-world. So the content.len read mechanism is NOT generically broken (it works for the flat
case). hello-world's content.len=0 is SPECIFIC to its structure. So the bug is NOT a universal
"SROA'd Vec::len() read" (that'd break web-text-min too) — it's something the NESTED path
(body BFC > div IFC > text) or the BUTTON triggers.

To isolate nesting-vs-button: created examples/c/web-nested-text.c = web-text-min + the text
wrapped in a DIV (body > div > "Hello", NO button) = hello-world's `label_wrapper` nesting minus
everything else. Compiled (-fno-stack-protector). Relift AZ_FUEL=ALL in flight (b72uvunmw):
  • content.len=0 / hang  → the NESTING (force_ifc IFC-on-a-div) is the trigger; web-nested-text
    is now a MINIMAL repro to iterate the fix on (far cheaper than hello-world).
  • text POSITIONS (overflow>0) → the nesting is FINE; the bug is the BUTTON (cb/styling/multiple
    IFC calls), redirect there.

RULED OUT this cycle (toward the SP/stack-addr theory, then away from it):
  • Both layout_flow AND create_logical_items len reads are FAITHFUL `load volatile` guest loads
    (IR-confirmed) — no non-volatile SROA'd read anywhere on the path.
  • g165 wide dump: captured &inline_content (0x283e8) points into a STACK-SPILL region (saved
    return addrs 0xea2xxx + scattered ptrs), NOT a clean Vec — suggested a stack-addr/SP mis-lift.
  • BUT enforce_sp_preservation (DEFAULT-on, AZ_NO_FIX_SP to disable) already wraps ALL SP-leaking
    calls in layout_ifc: every `sub_` + `__az_indirect_dispatch` is wrapped (save/restore X19-28/
    FP/SP/D8-15); the only UNwrapped calls are leaf intrinsics (memset/memmove/fpu_exception/
    atomic_begin/end) that don't touch SP. So a simple unwrapped-call SP-leak is ruled out.
  ⇒ the marker/peek observation path is exhausted (markers drop on field reads; stack-addr peeks
    hit spill/relocated garbage). The DIFFERENTIAL (web-nested-text) sidesteps observation entirely.
NEXT: read b72uvunmw. If nesting-repro confirmed → bisect web-nested-text (smallest fn-set,
fast relift) for where content diverges from web-text-min's working path. KEEP: remill decoder fix.

## g166-RESULT (2026-06-08): CONFIRMED — web-nested-text REPRODUCES (bug = NESTING, not button)
web-nested-text.bin (body>div>"Hello", 3 nodes, NO button/cb/counter): g116 content.len=0, __az_fuel
trap, missing_block=22. So content.len=0 is triggered by the DIV-as-IFC-root reached via
layout_bfc(body) — NOT the button. web-text-min (body IS the IFC root) works; the ONLY difference is
the extra body-BFC layer + layout_ifc being called for a CHILD (div) vs the root (body). MINIMAL REPRO
established (examples/c/web-nested-text.c) — iterate the fix here (no button/snprintf/cb noise).
Run it: relift web-nested-text.bin (AZ_FUEL=ALL) + `AZ_LENIENT=1 node scripts/m9_e2e/layout-flexbox.js`
(LENIENT bypasses the node_count==5 gate). NEXT: same layout_ifc code, different node arg → DATA/
stack-depth difference. Compare the lift exec of layout_ifc(body)[works] vs layout_ifc(div)[fails];
likely the layout_bfc→layout_ifc(child) recursion/stack-depth perturbs the &inline_content/content.len.

================================================================================
## g167 (2026-06-08): BREAKTHROUGH — content IS built (stack-scan finds len=1 Vecs);
   the &inline_content POINTER is mis-computed. The bug is address-of, not the len.
================================================================================

INDEPENDENT stack scan (harness g167, [0x8000,0x30000], find {heap-ptr@0,small-cap@8,small-len@16})
— does NOT use the mis-liftable &-of capture — found 59 Vec candidates incl several len=1, e.g.
`@0x28028 Vec{ptr=0x628d680 cap=1 len=1}`, `@0x28e68 {ptr=0x628e1c0 cap=1 len=1}`. So the content
Vec IS correctly built with len=1 on the stack. BUT the captured &inline_content=0x283e8 is NOT any
candidate — it points to non-Vec stack data. ⇒ the marker/g149/g150/g165 "0x283e8" is a MIS-LIFTED
address-of; the REAL inline_content is at a different (correct) stack slot with len=1.

ROOT CAUSE (reframed, much more specific): the `&inline_content` ADDRESS-OF (a stack-local addr =
SP+offset) MIS-COMPUTES in the deep/nested call stack → wrong pointer 0x283e8 → passed to layout_flow
+ create_logical_items → they deref the wrong slot → content.len reads garbage (0). The LEN value is
fine; the POINTER is wrong. This is the "g56 stack-addr class" and explains EVERYTHING:
  • why web-text-min (shallow, body-IFC) works but web-nested (deep, div-IFC via layout_bfc) fails:
    the &-of mis-lift is stack-depth-dependent.
  • why faithful volatile loads still read 0: they faithfully load [wrong_ptr+16].
  • why memory peeks at 0x283e8 were garbage: 0x283e8 isn't the real Vec.
  • why every len-store/read looked correct in isolation: they ARE; only the passed ADDRESS is wrong.

NEXT (concrete): find layout_ifc's `&inline_content` computation in its lifted IR — the SP-relative
ADD/inttoptr that yields the arg to the layout_flow `sub_` call (X0) — and why it diverges from the
SP+offset layout_ifc uses INTERNALLY (where collect wrote len=1). Likely a stale-SP or mis-lifted
ADRP/ADD/MOV at deep SP. Fix in remill (the addr-arith semantic) or transpiler (SP base). web-nested-
text.bin is the minimal repro (server may still be up on :8800; `AZ_LENIENT=1 node .../layout-flexbox.js`).
Verify any fix: AZ_FUEL relift web-nested-text → g167 should show the captured &inline_content MATCHING
a len=1 candidate, and g116 content.len=1.

## g168 (2026-06-08): content arg = X0 = load volatile [SP+360] (spilled &inline_content)
In layout_ifc's opt.ll (dep 109187f48, entry sub_ae7f48), the layout_flow call (sub_a81224, 2 sites
@48960/106821) sets its args SP-relative right before the enforce_sp_preservation save-block:
  W0 (content=&inline_content) = `load volatile [SP+360]`   (line 48907-48910) ← the spilled ptr
  W8 (sret/X8 for FlowLayout return) = SP+1456 ; W1 = SP+568 ; W2=8 ; W5=1
So &inline_content is computed earlier, SPILLED to the stack slot [SP+360], reloaded into X0 here.
The load is FAITHFUL (volatile). ⇒ at runtime X0 = whatever is at [SP+360]. The address-of mis-lift
(g167) = [SP+360] holds the WRONG &inline_content at runtime. Most likely mechanism: SP changed
between the SPILL (store to [SP+360]) and this RELOAD — i.e. remill mis-tracks an intra-fn `sub sp`/
frame adjustment so [SP+360] addresses different slots at store vs load (depth-dependent → only the
nested/deep path trips it). enforce_sp_preservation only fixes SP across CALLS, not intra-fn sp moves.
NEXT (needs a tool, not more static reading): runtime trace of [SP+360]'s store value vs reload value
(or X0 at the sub_a81224 call) — instrument enforce_sp_preservation (already wraps this call) to log
arg0 to a ring; peek post-trap. OR trace SP through layout_ifc to find a mis-lifted intra-fn sp adjust.

ASSESSMENT: content.len is now root-caused to a stack-address/SP-spill class (NOT a len-value bug);
content IS built len=1. ~14 cycles invested; the marker/peek/static-IR paths are exhausted. The
remaining definitive step is a transpiler runtime trace (multi-cycle build-out). The remill DECODER
FIX (g164) is the session's landed, verified deliverable.

## g169 (2026-06-09): native==lift CONFIRMED faithful → bug is WASM-CODEGEN/RUNTIME, not the IR
Compared NATIVE layout_ifc (disasm) vs LIFTED opt.ll at the layout_flow call (0x9dcd14 / sub_a81224):
  NATIVE: `add x1, sp, #0x238` (content=&inline_content) ; `add x1.. ` matches LIFT W1=SP+568(=0x238);
          X8=sp+0x5b0=1456 (sret) matches; X0=[sp+0x168]=spilled self/tree matches W0=[SP+360].
  layout_ifc's ONLY sp move: `sub sp,sp,#0xb90` entry / `add sp,sp,#0xb90` exit → SP STABLE within fn.
⇒ the lifted IR is FAITHFUL to native (content = SP+0x238, a stable SP-relative addr). This VALIDATES
the cron's premise ("static IR correct, only mis-EXECUTES"). So the bug is NOT in remill's lift IR nor
the transpiler IR transforms — it's in the WASM CODEGEN (llc: LLVM-IR→wasm, azul_remill.cpp PassBuilder/
CodeGenOpt) OR a runtime memory-model issue. Depth-dependence (web-nested deep vs web-text-min shallow)
+ stack math (only ~32KB/128KB used at content site, no overflow) → likely a wasm-codegen mis-compile
of the deep-SP i32 inttoptr addressing, OR a runtime data corruption that's depth-sensitive.
Stack-overflow RULED OUT (content@~0x28000, SP@~0x281b0 = 32KB used). 512KB stack reverted (g67: overflows
the 128KB per-module STACK_BASE_STRIDE slot → would need stride bump too, but no overflow anyway).
NEXT (off the IR now): (a) disassemble the COMPILED wasm for layout_ifc (wasm2wat azul-mini.wasm, map the
fn) and verify llc compiled SP+0x238 faithfully; OR (b) the runtime mem-op trace. The static-IR/marker/peek
avenues are confirmed exhausted (IR is faithful by construction). remill DECODER FIX (g164) = landed.

## g170 (2026-06-09): RUNTIME SP-TRACE tool built (AZ_SP_TRACE=1) — definitive, off the IR
Static analysis exhausted (IR faithful, g169). Built a runtime trace: enforce_sp_preservation
(transpiler_remill.rs ~5311) now, when AZ_SP_TRACE=1, logs each wrapped call's guest SP (State+1040)
+ X1 (State+560, content-ptr candidate) + call-index to a circular ring @0x78000 (2048x16B, counter
@0x77FF0). Harness g170 (layout-flexbox.js) dumps the last ~44 entries post-trap. AZ_SP_TRACE off by
default (harmless free band 0x78000, above stack/below bump-heap). Rebuild dylib + relift
web-nested-text AZ_FUEL=ALL+AZ_SP_TRACE=1 (bvgy9mu5z). READ: if SP JUMPS between consecutive calls in
the same fn-region (should be stable within a fn), that's the SP mis-track → &inline_content=SP+0x238
resolves wrong. If SP is consistent, the bug is wasm-codegen (llc mis-compiles the i32 inttoptr) or
data corruption — escalate to disasm of the compiled wasm. This is the tool that sees the mis-EXECUTION
the cron describes (which static IR can't, by construction).

================================================================================
## g171 (2026-06-09): RUNTIME SP-TRACE works. content IS a VALID Vec in memory.
   Vec field order is {cap@0, ptr@8, len@16} — earlier "ptr=0x4 garbage" was misread cap.
================================================================================

AZ_SP_TRACE ring (g170) captured 2191 wrapped calls. The content-passing call:
  #2186 SP=0x281b0 X1=0x283e8 (content=&inline_content), [X1+16]=1.
Full 64-bit dump @0x283e8: [+0]=0x4  [+8]=0x6290ed8(HEAP)  [+16]=0x1.
KEY CORRECTION: Rust reorders RawVec fields → Vec<InlineContent> layout is {cap@0, ptr@8, len@16}
(NOT {ptr,cap,len}). So 0x283e8 = a VALID Vec: cap=4, ptr=0x6290ed8(heap), len=1. ALL the prior
"&inline_content points to garbage / ptr=0x4" readings (g162/g165/g167) were misreading the cap
field as the data ptr. CONTENT IS CORRECTLY BUILT IN MEMORY (ptr=heap, len=1, cap=4).
SP is CONSISTENT (~0x281b0, shallow — only ~32KB stack used; deep-stack/overflow ruled out). The
reads are faithful volatile loads (g169). ⇒ content is valid in memory + the read IR is faithful,
yet the optimized fn yields empty. This is squarely the cron's "static IR correct, mis-EXECUTES":
the mis-execution is in the WASM (llc codegen) or in to_vec's OUTPUT (content_vec_flow), NOT in
inline_content's construction (which is valid).
TOOLING BUILT (reusable): AZ_SP_TRACE=1 (enforce_sp_preservation logs SP+X1+callidx to ring @0x78000,
counter @0x77FF0; harness g170/g171 dumps it). Lets us see any wrapped call's SP + arg1 at runtime.
NEXT: trace content_vec_flow (to_vec output) the SAME way — it's passed as X1 to create_logical_items
(another wrapped call); log+peek its {cap,ptr,len}. If content_vec_flow has len=0 while inline_content
has len=1, the bug is to_vec's lifted output (a Vec built in layout_flow); then disasm that wasm fn.

## g172 (2026-06-09): trace enhanced to log call TARGET → DEFINITIVE layout_flow vs create_logical_items
IMPORTANT: g116's "content.len=0" had mark(0x607D8)=0x0 = the marker DROPPED (didn't fire) — so the
"content.len=0" reading is UNRELIABLE (a dropped-marker artifact). The reliable data (g171, corrected
{cap,ptr,len} layout) says inline_content is VALID (len=1). So the content.len=0 premise may be false.
ENHANCED AZ_SP_TRACE: enforce_sp_preservation now logs the call TARGET (sub_<hex> low32) at ring +8
(was callidx). Harness g172 finds layout_flow(0xa81224, X1=&inline_content) + create_logical_items
(0xa829dc, X1=&content_vec_flow), peeks each as Vec{cap@0,ptr@8,len@16}. Rebuild+relift web-nested-text
AZ_FUEL+AZ_SP_TRACE (baztccs07). DEFINITIVE READ:
  • inline_content len=1 AND content_vec_flow len=0 → to_vec's LIFTED OUTPUT is the bug (a Vec built
    in layout_flow's inlined to_vec); disasm that wasm fn next.
  • BOTH len=1 → content is fine end-to-end; the content.len=0 was a red herring, and the HANG is a
    DIFFERENT bug in break_one_line (valid content, cursor not advancing) — pivot the whole investigation.
This resolves whether ~17 cycles of "content.len=0" was chasing a dropped-marker artifact.

================================================================================
## g173 (2026-06-09): MAJOR REDIRECT — the hang is in INTRINSIC SIZING, not content.
   "content.len=0" was a DROPPED-MARKER red herring. inline_content is VALID (len=1).
================================================================================

DEFINITIVE runtime data (enlarged AZ_SP_TRACE ring + target logging, web-nested-text):
  • layout_flow(&inline_content) X1=0x283e8 = Vec{cap=4, ptr=0x6290ed8(heap), len=1} → VALID, len=1.
  • The AZ_FUEL trap (the HANG) is in the INTRINSIC SIZING loop, NOT break_one_line/layout_flow.
    The ring's repeated loop targets resolve to:
      sub_b44e68 = core::hash::sip::Hasher::write (SipHasher)
      sub_a26c94 = BTreeMap::bulk_push (append)
      sub_acc84c = solver3::sizing::compute_dirty_ancestor_closure
      sub_aceddc = solver3::sizing::IntrinsicSizeCalculator::calculate_block_intrinsic_sizes
      sub_ad04dc = solver3::sizing::calculate_used_size_for_node
    — i.e. a MEMOIZATION-CACHE LOOP in the sizing pass (hash key → BTreeMap lookup → recompute →
    never converges/terminates), spinning on an empty Vec (X1=0x2a190 Vec{len=0}).
  • g116's "content.len=0" had mark(0x607D8)=0x0 = the marker DROPPED (never fired) → UNRELIABLE.
    With the corrected Vec layout {cap@0,ptr@8,len@16}, inline_content is a VALID len=1 Vec.

⇒ The whole "SROA'd Vec::len()=0 → empty content" framing (cron + prior sessions) was chasing a
dropped-marker artifact. The REAL blocker is an infinite loop in solver3::sizing
(compute_dirty_ancestor_closure / calculate_used_size_for_node / calculate_block_intrinsic_sizes +
their BTreeMap memoization cache). It's depth/nesting-dependent (web-text-min works, web-nested hangs)
and AZ_FUEL-trappable. Likely a lift mis-execution making the cache key/hash or a loop-bound read
wrong → the memo never hits → infinite recompute (SAME mis-lift CLASS, different site).
NEXT: AZ_FUEL relift web-nested-text; from the fuel trap, identify which sizing fn's loop spins (cap
its loop or trace its loop-bound/cache-key). The memo is a BTreeMap<hash,size> in IntrinsicSizeCalculator
— check whether the dirty-ancestor-closure or the used-size recursion has a non-converging condition
under the lift. Tools: AZ_SP_TRACE ring (built), the g167/g171 stack scan, AZ_FUEL. inline_content/
content.len is FINE — stop chasing it. web-nested-text.bin = minimal repro. remill decoder fix = landed.

## g174 (2026-06-09): CORRECTION — loop is in layout_flow's line builder (not sizing). Call stack:
AZ_FUEL trap call stack (web-nested-text): __az_fuel <- layout_flow(a800ac) <- layout_ifc(div) <-
layout_formatting_context <- calculate_layout_for_subtree <- layout_bfc(body) <-
layout_formatting_context <- calculate_layout_for_subtree <- layout_document <- layout_dom_recursive.
The `ae088c/b4ad00` repeat = the NORMAL body-BFC > div-IFC 2-level nesting (NOT runaway recursion).
The infinite loop is layout_flow's OWN loop (`while !cursor.is_done()` line builder, w/ to_vec +
create_logical_items + perform_fragment_layout + break_one_line ALL INLINED into layout_flow). Last
cycle's "sizing loop" was a MISREAD — the sizing fns (compute_dirty_ancestor_closure etc.) were EARLIER
ring entries (the sizing pass ran first, then layout_flow's loop spun); they're not the current loop.
SO: inline_content is VALID (len=1, g172) but layout_flow's cursor never completes ⇒ the cursor's items
are empty ⇒ to_vec/create_logical_items (inlined) produced EMPTY logical_items from valid input. THIS is
the cron's class (a Vec-len-in-loop-bound reads 0 → empty), but at the DOWNSTREAM Vec (content_vec_flow/
logical_items in layout_flow), NOT inline_content. Everything inlines into layout_flow (dep 109121224,
~15k-line IR, entry sub_a81224/a800ac) → tractable single-fn target. IR is faithful (g169) → wasm-codegen
/runtime mis-exec. NEXT: in layout_flow's IR, find create_logical_items' loop bound (content_vec_flow.len
read) + the output-len store of content_vec_flow (the to_vec result); cap that loop / check if it reads
SROA'd 0. Tools: AZ_SP_TRACE, AZ_FUEL, g167 scan. remill decoder fix = landed.

## g175 (2026-06-09): AZ_LOWOPT_FNS=layout_flow does NOT fix the hang (still hangs at timeout 240).
Tested the per-fn opt-lowering (AZ_LOWOPT_FNS, transpiler_remill.rs:4805 — -O0 for matching fn stems,
intended exactly for "over-aggressive opt fold → infinite loop"). AZ_LOWOPT_FNS=layout_flow → STILL HANGS
at timeout 240 (4/4 runs at t=70 hung; the one t=90 exit-0 was a FLUKE/early-exit, not a layout). So the
infinite loop is GENUINELY in layout_flow's `while !cursor.is_done()` line builder, and -O0 of layout_flow
doesn't fix it ⇒ the mis-exec is NOT layout_flow's own lift-opt. The loop spins because a CALLEE returns
wrong (the cursor never advances). Harness's own note: "TEXT IS CORRECT (AzString 'Hello' len=5); the layout
hang is downstream: box-deref/shaping" → points at the allsorts SHAPING path (shape_text_internal /
font.shape_text / GSUB-GPOS) called inside layout_flow's loop returning 0 glyphs → 0-advance → cursor stuck.
TESTING (b10mixhs9): broad AZ_LOWOPT_FNS=shape_text,allsorts,text35cache,text35default,perform_fragment,
break_one_line,create_logical,BreakCursor,layout_flow + timeout 240. DECISIVE: lays-out → opt artifact in
one of those (narrow next); still-hangs → NOT opt (remill semantic / llc wasm-codegen) → disasm the compiled
wasm fn. content.len/inline_content CONFIRMED FINE (len=1, valid Vec). The bug is the SHAPING returning empty
/looping inside layout_flow's line loop. remill decoder fix (g164) = landed.

## g176 (2026-06-09): ROOT-CAUSE CLASS CONFIRMED = State-alloca SROA breaking state propagation.
AZ_OPT_LEVEL=O1 (global) → STILL HANGS at 240s. O1 still does SROA (only O0 disables it, but O0 →
"local count too large" wasm-validation error for create_logical_items/sub_a81864 = too many locals).
So the bug is the SROA of the per-fn State alloca — EXACTLY the documented suspicion in azul_remill.cpp:746
("Oz's aggressive inlining + SROA promotes the State struct alloca per sub-function, breaking state
propagation between caller and callee"). This IS the cron's "SROA'd value mis-lift" class, CONFIRMED, but
the value isn't content.len (red herring) — it's a State-propagated value in create_logical_items/the
text path that SROA mis-promotes → empty logical_items → layout_flow's `while !cursor.is_done()` loop
spins on empty items (shaping never reached). Eliminations: AZ_LOWOPT=layout_flow (no), O1 (no, still
SROAs), O0 (compile-fails, too big). TESTING (bjev30t0x): AZ_NO_HOST_SCOPE=1 — the host/guest alias
scopes are what ENABLE the State-alloca SROA (prove guest≠host so SROA can promote past the inttoptr guest
accesses); disabling them → conservative AA → State alloca NOT SROA'd → state propagates via memory →
correct (bigger/slower wasm, the accepted trade-off). If web-nested-text LAYS OUT under AZ_NO_HOST_SCOPE,
that's THE FIX (transpiler-level, gate already added at tag_state_accesses). Then refine: tag only the
never-escaping state_buf, or scope-per-fn, to keep SROA where safe. remill decoder fix (g164) = landed.

## g177 (2026-06-09): AZ_NO_HOST_SCOPE=1 HANGS (g176 State-SROA theory ELIMINATED). Definitive ring-histogram in flight.
bjev30t0x (AZ_NO_HOST_SCOPE=1, disables the host/guest alias scopes that ENABLE State-alloca SROA) →
HARNESS-EXIT 124 = STILL HANGS. So the State-alloca-SROA-via-host-scope theory is OUT (10th+ elimination).
Blunt instruments ALL fail now: AZ_LOWOPT_FNS=layout_flow (g175), AZ_OPT_LEVEL=O1 (g176), AZ_NO_HOST_SCOPE
(g177). The text is CONFIRMED CORRECT ("Hello" len=5 via BoxOrStatic indirection); hang is in layout/shaping.

RECONCILING g173 (SP-ring: loop is in SIZING — SipHasher/BTreeMap::bulk_push/compute_dirty_ancestor_closure/
calculate_block_intrinsic_sizes repeating) vs g174 ("correction": loop is layout_flow's `while !cursor.is_done()`):
  • SOURCE FACTS: layout_flow's OUTER `while !cursor.is_done()` loop ALREADY has a hard 4096 cap (cache.rs:7950,
    web_lift-gated) AND breaks on empty line_items (8085). So it CANNOT be the infinite loop — it would break.
  • break_one_line's main loop (cache.rs:8286) always `cursor.consume(N)` (advances) or `break`s. It spins ONLY
    if consume() fails to advance next_item_index AND unit_width≈0 (else width accumulates → overflow → break).
  • compute_dirty_ancestor_closure (sizing.rs:166) `while let Some(idx)=cur { if !closure.insert(idx){break} cur=
    tree.get(idx).parent }` spins ONLY if HashSet::insert dedup fails (SipHasher mis-lift!) AND the parent chain
    cycles (parent mis-reads to non-None/self). SipHasher::write in the ring = HashSet ops = THIS loop's insert.
  ⇒ g175's "capping layout_flow doesn't fix it" + the existing 4096-cap PROVE the spin is NOT layout_flow's own
    outer loop. It's either (a) compute_dirty_ancestor_closure's HashSet-dedup loop [matches g173 SP-ring], or
    (b) break_one_line's consume-non-advance, or (c) the sizing memo calling layout_flow repeatedly.

DEFINITIVE TEST (by12.../byelslo24 in flight): relift web-nested-text AZ_SP_TRACE=1 + AZ_FUEL=ALL; harness g177
adds (1) a HISTOGRAM of the last 512 ring TARGETs + (2) the last 40 ring entries IN CALL ORDER = the innermost
loop's call cycle. The most-repeated target at the tail = the spinning fn; resolve sub_<hex> via server log's
`resolved=NAME@0x..`. This ends the sizing-vs-layout_flow flip-flop with raw runtime call data. If the histogram
is dominated by SipHasher/compute_dirty → fix compute_dirty_ancestor_closure's HashSet/parent-read mis-lift;
if by break_one_line callees → fix consume(). UNCOMMITTED. Harness g177 block = keep (diagnostic).

## g178/g179 (2026-06-09): ★★★★★ DEFINITIVE — infinite loop is in layout_flow (global fuel GID proves it).
Built the DEFINITIVE localizer (no more flip-flop). AZ_FUEL is a GLOBAL tick counter (0x40068) that traps
at AZ_FUEL_LIMIT (default 200M), recording the looping block's GID at 0x40070 (harness g179 reads it).
web-nested-text AZ_FUEL=ALL+AZ_SP_TRACE relift (server_ringdump.log, server STILL UP on 8800 → iterate the
harness with NO relift): tripped=1, tick=0xBEBC201=200,000,001 (EXACTLY the 200M limit ⇒ a REAL infinite
loop), GID=83780.
  • The kept *.fuel.ll only cover GIDs 0..10400 (stems collide/overwrite), BUT the server log prints
    "M12.7: fueled N terminators in STEM" per fn IN TRANSPILATION ORDER → cumsum N → GID 83780 ∈
    __az_dep_1092f80ac = GIDs 83760..84260 (501 terminators). __az_dep_1092f80ac (addr 0x1092f80ac) =
    **_ZN11azul_layout5text35cache16TextShapingCache11layout_flow** = layout_flow (4500 bytes). DEFINITIVE.
  • GID 83780 = the 21st terminator (0-idx 20) of layout_flow's 501.
  • It's a TIGHT loop with NO wrapped sub_ calls: tick hit 200M but the SP-ring counter (0x77FF0) froze at
    2191 wrapped calls ⇒ after the last wrapped call the loop spun 200M× with no function calls (everything
    inlined). shaping PHASE(0x407A0)=0 ⇒ the loop is BEFORE shaping, in layout_flow's logical-items/cursor/
    line-break setup (break_one_line + create_logical_items + cursor methods are ALL inlined into layout_flow).
  • Last wrapped calls before the freeze (ring tail): #2186 layout_flow(X1=0x283e8=&inline_content) → InlineContent::clone
    → SipHasher::write → #2190 String::clone. Then the tight no-call loop.

⇒ RECONCILES EVERYTHING: g174 was RIGHT (loop in layout_flow); g173 "sizing" was the ring's earlier entries
(sizing ran first); g175 "AZ_LOWOPT=layout_flow doesn't fix it" ⇒ NOT an opt artifact = a remill SEMANTIC/
codegen mis-lift of a specific instr in layout_flow's tight inlined loop. The 4096 outer-loop cap (cache.rs:7950)
does NOT catch it because the spinning loop is an INNER inlined loop (break_one_line main loop 8286 / the
leading-ws loop 8250 / create_logical_items), and those have NO iteration cap. The cron's "Vec-len" framing
was wrong; it's a CURSOR-NON-ADVANCE or unit-width=0 tight loop.

SOURCE ANALYSIS (cache.rs): break_one_line's main loop (8286) ALWAYS `cursor.consume(N)` (advances) or breaks,
UNLESS consume() fails to advance next_item_index AND unit_width≈0 (then it loops forever, current_width stuck
at 0 so the width-overflow escape never fires). cursor.consume (10203): `next_item_index += from_main_list`.
The leading-ws loop (8250) only consumes whitespace (breaks for "Hello"). The no_wrap loop (8271) only if
white-space:nowrap/pre. ⇒ prime suspect = break_one_line main loop with a 0-advance cursor.

NEXT (ba6yhjf5w in flight): relift AZ_FUEL=layout_flow ONLY (GIDs restart 0 → trap GID maps directly within
layout_flow) + AZ_REMILL_KEEP_SCRATCH=1 (persists layout_flow.fuel.ll + .opt.ll) + AZ_FUEL_LIMIT=2M (fast trap).
Map the small trap GID → @__az_fuel(i32 GID) in layout_flow.fuel.ll → the exact block → trace to the source
loop + the mis-lifting instruction (the remill fix target). Tools: g178 whole-ring histogram + SP-range,
g179 FUEL-GID reader, cumsum-from-server-log GID→fn mapping. remill decoder fix (g164) = landed. UNCOMMITTED.

## g180 (2026-06-09): ★★★★★★ ROOT CAUSE NAILED — hashbrown SwissTable probe in layout_flow infinite-loops.
Mapped the AZ_FUEL trap GID to the EXACT block + traced to source + ARM64 + remill semantics:
  • GID 25 (AZ_FUEL=layout_flow relift) → layout_flow.fuel.ll block 211⇄219 = a hashbrown SwissTable
    match-bit-iteration: `cmeq.8b`(group cmp vs h2) → `fmov x13,d3` → mask; `rbit+clz`(cttz=lowest bit)
    → check slot → `sub x14,x13,#1; ands x13,x14,x13`(clear lowest bit) → `b.ne` loop; empty-check
    `cmeq.8b v2,v2,v1`(v1=movi.2d 0xFF=EMPTY) + `umaxv.8b` + `tbnz`. ARM64 @ libazul.dylib 0x9702a4..0x970320.
  • The loop comes from `self.logical_items.entry(logical_items_id).or_insert_with(...)` (cache.rs:5678) —
    logical_items/visual_items/shaped_items/per_item_shaped are ALL `HashMap` (cache.rs:5313-5320).
  • The probe NEVER finds the key OR an EMPTY(0xFF) slot ⇒ with bucket_mask keeping it on the group, it
    loops 200M× (native terminates in ≤8). ⇒ the table's CONTROL BYTES read WRONG at runtime.
  • RULED OUT (all faithfully lifted, verified via remill-lift + IR dump): cmeq.8b, fmov x←d, fmov w←s,
    umaxv.8b (correct 8-way umax reduction), movi.2d (correctly stores -1=0xFF to both lanes), dup.8b.
    So it's NOT a NEON decode/semantic gap (unlike g164). It's a hashbrown RawTable control-byte/alloc/
    pointer mis-lift at runtime. memset IS handled (FnClass::LibcMemset, symbol_table.rs:1773, added for
    exactly "hanging HashMap::insert") and hashmap_random_keys IS handled (fixed seed) + EMPTY_GROUP-AUTO
    mirrors static_empty — yet it still hangs ⇒ the ALLOCATED small-table control init (or ctrl ptr/
    bucket_mask read) mis-lifts on the 2nd+ insert. web-text-min (body-IFC, layout_flow's map fresh/cap-0
    → static_empty works) POSITIONS; web-nested-text (div-IFC, 2nd+ layout_flow → allocated table) HANGS
    = depth/state-dependent, consistent with the whole session's theme.

★ THE FIX (g180, applied): layout_flow (5676-5765) was the ONLY text-cache path NOT given the g115/g118
HashMap BYPASS — measure_intrinsic_widths ALREADY bypasses these same 3 HashMaps (cache.rs:5854/5896/5910)
for THIS EXACT hang. Applied the same web_lift-gated bypass to layout_flow's logical_items/visual_items/
shaped_items (build the Arc directly, no HashMap/self round-trip; native keeps the cache via cfg). This is
the established precedent + REVERTABLE once the deep remill hashbrown-RawTable fix lands (then delete ALL of
g115/g118/g180). DEEP REMILL FOLLOW-UP (cron goal): characterize WHY the allocated hashbrown table's control
bytes read wrong (runtime peek of ctrl ptr [x24]/bucket_mask [x24+8]/the 16 control bytes at the hang) →
fix the specific alloc/ctrl-init/pointer mis-lift in remill or the transpiler → then HashMaps lift faithfully
and every bypass (g115/g118/g180 + dom_to_layout→BTreeMap + resolve_char) can be deleted. Method that nailed
it (reuse): global AZ_FUEL tick→GID@0x40070 (harness g179) + cumsum "fueled N in STEM" server-log lines →
GID→fn; AZ_FUEL=<fn> relift → GID maps within fn → .fuel.ll block → ARM64 disasm → remill-lift per-instr
semantic check. UNCOMMITTED.

## g180-RESULT (2026-06-09): ✅✅✅ FIXED — web-nested-text LAYS OUT (the multi-session hang is RESOLVED).
Rebuilt libazul.dylib with the g180 layout_flow HashMap bypass + relifted web-nested-text (bwkmzowqj):
  • HARNESS-EXIT 0 (was 124=HANG) — NO MORE HANG.
  • [g132 lays-out] overflow_size = 39.10 x 20.05 ✓✓✓ TEXT LAYS OUT (h>0) — same as web-text-min.
  • [g133] collect inline_content.len=1, collect=Ok, layout_flow=Ok ✓ ; [g136] dom_children.len=1, child=Text ✓
  • rects: (0,0,800,600)=body, (8,16,800,20)=div-with-"Hello" (h=20, text laid out!), + 1 sentinel (text node).
⇒ CONFIRMS the root cause definitively: the hang WAS the hashbrown SwissTable probe infinite loop in
layout_flow's `self.logical_items/visual_items/shaped_items.entry()` HashMap caches. The bypass (build the
Arc directly, web_lift-gated) resolves it. body>div>"Hello" = hello-world's label_wrapper nesting → that path
now works. UNCOMMITTED (commit when asked).

NEXT: (1) hello-world.c full widget (button + counter) — relift, find next blocker (button cascade/styling,
counter snprintf, click/dispatch). (2) DEEP REMILL FOLLOW-UP (cron goal, to DELETE g115/g118/g180 +
dom_to_layout→BTreeMap + resolve_char bypasses): runtime-peek the allocated hashbrown table at the probe
(ctrl ptr [x24], bucket_mask [x24+8], the 16 control bytes) to pin WHICH alloc/ctrl-init instr writes wrong
control bytes → fix in remill/transpiler → HashMaps lift faithfully → delete every bypass.

## g180-HELLOWORLD (2026-06-09): ✅ hello-world.c LAYS OUT too (no hang) — g180 unblocks the FULL widget.
Relifted examples/c/hello-world.bin on the g180-bypass dylib:
  • HARNESS-EXIT 0 (NO HANG) ; cascade ok node_count=5 (expected 5) ✓
  • [g132] overflow_size = 98.38 x 16.29 ✓✓✓ TEXT LAYS OUT (the button label "Increase counter", h>0)
  • [g133] collect inline_content.len=1, layout_flow=Ok ✓ ; boxed AzString text="Increase counter" len=16 ✓
  • rects (5): (0,0,784,37)=body(has height now!), (0,0,784,0)=?, sentinel, (8,16,784,29)=button+label, sentinel.
⇒ The g180 HashMap-bypass fix unblocks BOTH web-nested-text AND hello-world's layout. The multi-session
layout_flow hang is RESOLVED for the real target.

REMAINING for full hello-world (task #4, NOT layout-blocked anymore): (a) one (0,0,784,0) h=0 node (counter
container/text — check auto-height); (b) [g80b] font Phase2/resolution drops the matched font key (Phase1
matched 1 → 0); (c) positions.len=0 (positioned-rects extraction); (d) counter snprintf text; (e) click/
dispatch. None of these hang. DEEP REMILL FOLLOW-UP (cron, to delete g115/g118/g180): pin the hashbrown
allocated-table ctrl-init mis-lift (runtime-peek ctrl ptr/bucket_mask/control bytes at the probe) → fix in
remill/transpiler. UNCOMMITTED (commit when asked).

## g181 (2026-06-09): DEEP-FIX ANALYSIS — all hashbrown instrs FAITHFUL; bug = runtime VALUE mis-lift. Isolation test in flight.
Cron re-fired the "Vec-len SROA" framing, but g178-g180 DISPROVED that — the hang is the hashbrown probe, NOT a
Vec-len. Pursued the cron's META-goal (remill/transpiler fix to delete azul workarounds) on the REAL target,
ANALYSIS-FIRST (no relift):
  • The hashbrown ctrl-byte init (RawTableInner::fallible_with_capacity @0x6a5eac, and reserve_rehash) is
    `movi.2d v0,#0xffffffffffffffff` (EMPTY=0xFF) + INLINED `str q0,[ctrl,#off]` (NOT a memset call — so
    LibcMemset never applies to these small/medium tables).
  • remill-lift VERIFIED every relevant instr is FAITHFUL: probe (cmeq.8b, fmov x←d/w←s, umaxv.8b [correct
    8-way umax], movi.2d [correct -1=0xFF both lanes], dup.8b, rbit, clz, sub, ands) AND ctrl-init (movi.2d,
    `str q` → two correct __remill_write_memory_64 = 16 bytes). 0 HandleInvalidInstruction; correct semantics.
  ⇒ NOT an instruction decode/semantic gap (unlike g164). The bug is a RUNTIME VALUE mis-lift in the probe's
    DATA INPUTS — the ctrl ptr (ldr x9,[x24]) / bucket_mask (ldr x10,[x24,#8]) / hash / or a MIS-LIFTED `self`
    (X0 of layout_flow) so self.logical_items is read via a wrong base. This IS the cron's "correct IR,
    mis-EXECUTES at runtime" class — just at the HashMap RawTable fields, not a Vec::len. The g115 comment
    already named "a mis-lifted self" as a suspect. Consistent with the depth-dependence (web-text-min's
    layout_flow self is fine = works; web-nested's deeper call mis-lifts = hangs).

DECISIVE ISOLATION TEST (g181, bhw408rhy in flight): a FRESH local `HashMap<u64,u64>` insert5+get5 at
layout_flow ENTRY (web_lift-gated), AZ_FUEL=ALL to catch a hang. Reads found(0x60C00)/bucket_mask/ctrl/
ctrl-bytes. VERDICT: (a) found==5 → hashbrown WORKS in isolation ⇒ the real hang is a MIS-LIFTED self/state in
the nested layout_flow call (NOT hashbrown) → fix = self/SP tracking in the deep call (enforce_sp_preservation
or remill X0 tracking). (b) found<5 / hangs → hashbrown ITSELF mis-executes a fresh map under the lift ⇒ fix =
the hashbrown RawTable runtime value (ctrl/mask) mis-lift. Either way pins the deep fix target. REVERT g181
diag after. UNCOMMITTED.

## g181-RESULT (2026-06-09): isolation test → hashbrown WORKS; deep bug = mis-lifted self/SP in the nested call.
Rebuilt + AZ_FUEL=ALL relift of web-nested-text with the g181 fresh-local-HashMap test at layout_flow entry:
  • RELIABLE signal: HARNESS-EXIT 0 + [g132] overflow_size=39.10×20.05 ⇒ layout_flow COMPLETED ⇒ the g181
    test (a fresh local `HashMap<u64,u64>` insert5+get5, placed BEFORE the g180 bypass) did NOT hang.
    tm.len()=5 (inserts worked via the proper ABI). ⇒ **hashbrown's insert+get ALGORITHM works in isolation
    under the lift.**
  • UNRELIABLE (ignore): found(0x60C00)=UNSET (computed-value marker dropped) + the manual transmute reads
    bucket_mask=2/ctrl=0x5 (the diagnostic's own `&tm+offset` reads mis-lift — the g56 stack-addr class —
    while tm.len()/tm.get() via the real ABI work). This is itself more evidence that COMPUTED-ADDRESS reads
    mis-lift while proper-ABI field access works.
  ⇒ DEEP-FIX VERDICT: the hang is NOT hashbrown's algorithm and NOT a Vec-len. A fresh SP-relative local map
    works; the REAL hang is `self.logical_items` — an X0/`self`-relative access — in the NESTED (deep-stack)
    layout_flow call. So `self` (spilled, SP-relative) reads garbage in the deep call ⇒ self.logical_items's
    RawTable fields (ctrl ptr/bucket_mask) are garbage ⇒ the (faithfully-lifted) probe spins. This is the
    enforce_sp_preservation / SP-&-self tracking domain (transpiler), the SAME depth-dependent class as the
    whole session (web-text-min shallow=works, web-nested deep=hangs). The g115 comment's "mis-lifted self"
    suspicion was RIGHT.

DEEP FIX TARGET (cron goal, to delete g115/g118/g180): fix the `self`/SP mis-tracking in deeply-nested inlined
calls so self.<field> reads resolve correctly (transpiler enforce_sp_preservation or remill SP/X0 tracking).
This is substantial (the recurring SP-tracking problem), NOT a one-instruction decoder fix. The g180 bypass is
the working unblock meanwhile. NEXT concrete step: capture `self as usize` (X0) at layout_flow entry across the
shallow (web-text-min) vs deep (web-nested) calls — if deep-self != a valid TextShapingCache ptr, that's the
direct proof + the value to fix. (Capture via a wrapper/out-param, NOT an inline transmute, to dodge the
computed-address mis-lift.) g181 diag REVERTED (cache.rs + harness). UNCOMMITTED.

## g182/g183 (2026-06-09): DEEP-FIX — self.logical_items reads cap=0/len=0 right before the hang. Capturing self ptr.
Un-bypassed layout_flow's logical_items + captured the map state via the proper ABI (.capacity()/.len(), which
read reliably) BEFORE the (re-introduced) entry() probe; AZ_FUEL=ALL so the hang traps.
  • RESULT: self.logical_items.capacity()=0, .len()=0, id_lo=0x61d62dd9 (a sane hash), entry-reached=NO (hung).
  • The hang reproduced: tick=200M, GID=84347 (still layout_flow's entry() probe).
  • KEY UNIFYING HYPOTHESIS (g183, testing): if `self` (X0) is MIS-LIFTED to ~0/null in this nested call, then
    self.logical_items reads a NULL/ZEROED RawTable (bucket_mask=0, ctrl=null) → entry()'s probe does
    `ldr d2,[null]` = ZEROS (not 0xFF EMPTY) → cmeq vs EMPTY never matches → bucket_mask=0 keeps it on group 0
    → INFINITE LOOP on zeros. This UNIFIES every fact: cap=0/len=0 (self→zeros), the hang on "empty" map (a
    truly-empty VALID map would find static_empty=0xFF immediately and NOT hang), all instrs faithful (it
    correctly probes zeros + correctly never finds 0xFF), the g181 fresh LOCAL map working (SP-relative, no
    self), and the depth-dependence (self only mis-lifts in the deep call).
  • The cap=0/len=0 alone is AMBIGUOUS (could be a genuinely-empty valid map). The DECISIVE test (g183,
    bo6ah5x0t in flight): capture self's POINTER VALUE (X0, not a computed read). VERDICT: self in 0x6xxxxxx
    bump-heap ⇒ self VALID ⇒ the empty-map entry()/allocate+reprobe hangs (hashbrown ctrl-init after reserve);
    self ~0/garbage ⇒ MIS-LIFTED self confirmed ⇒ fix self/X0 tracking in the nested call (transpiler/remill).
NOTE: the hang is on the FIRST insert into an EMPTY map (cap 0 → reserve → allocate → reprobe), not a 2nd
insert — so it's the allocate/reprobe path OR self→null. g182/g183 diags REVERT to the plain g180 bypass after.

## g183-RESULT (2026-06-09): ★ REFRAME — the hang is UNIVERSAL (not depth/self). self=0x2e0e8 (consistent).
Captured self's POINTER at layout_flow entry (X0 value, reliable) under AZ_FUEL=ALL, for BOTH test cases:
  • web-nested-text (deep, div-IFC): self=0x2e0e8, cap=0, len=0, entry()=HUNG, GID≈84400.
  • web-text-min (SHALLOW, body-IFC): self=0x2e0e8 (SAME), cap=0, len=0, entry()=HUNG, GID≈84400.
⇒ web-text-min ALSO HANGS on the un-bypassed entry() — it only "works" because the g180 bypass AVOIDS entry().
  So the hang is NOT depth-dependent and NOT a "mis-lifted self in the nested call" (g181/g183 disprove that —
  self is the SAME consistent 0x2e0e8 in both). The earlier "depth-dependent / mis-lifted self" framing (incl
  this cycle's first pass) is WRONG.
⇒ The bug is: `self.logical_items.entry()` (the hashbrown FIND-probe, GID→block 211⇄219) HANGS UNIVERSALLY
  under the lift, reading the map as empty/null (cap=0/len=0) and never finding EMPTY(0xFF). YET a fresh local
  `HashMap<u64,u64>` insert+get in the SAME function (g181) does NOT hang. The discriminator is the SELF-RELATIVE
  FIELD ACCESS: reading the RawTable's ctrl/bucket_mask through `self.logical_items` (self+offset) mis-lifts to
  garbage/zeros at runtime, while a direct local map's fields read fine. self's POINTER (0x2e0e8) is consistent,
  but the FIELD LOADS through it (or the empty-map static_empty group) read wrong. All probe + ctrl-init instrs
  are faithful (verified). 0x2e0e8 is a stack addr; whether it's the cache's real location vs a stale/wrong
  slot is the open question.

DEEP-FIX STATUS (honest): narrowed to a SELF-RELATIVE-LOAD value mis-lift in `self.logical_items.entry()`'s
find-probe (reads ctrl/bucket_mask/static_empty as zeros), UNIVERSAL (not depth). 5 rebuild+relift cycles this
session; the precise instruction is elusive because the diagnostics THEMSELVES mis-lift (computed-address reads
are exactly what's broken). The g180 bypass works + ships hello-world layout. NEXT ANGLES (cheaper/different):
(1) check the EMPTY_GROUP-AUTO transpiler pass — does the empty HashMap's `Group::static_empty()` const get
mirrored as 0xFF for THIS map, or read 0? (the entry()-on-empty path probes static_empty first). (2) Compare
the g181 local-map vs self.logical_items DISASM for how each reads ctrl/bucket_mask (local SP-relative vs
self+offset) → find the load that differs. (3) A transpiler post-pass to pin self/load fidelity. g182/g183
diags REVERTED (clean g180 bypass restored). UNCOMMITTED.

## g183-FINAL (2026-06-09): the self-relative STORE (resize write-back) is the most likely culprit.
hashbrown's `entry()` on an EMPTY map calls `reserve(1)` FIRST (allocates), THEN probes the ALLOCATED table —
so static_empty / EMPTY_GROUP-AUTO is NOT on this path (ruled out for entry-on-empty). The allocated table's
ctrl-init (`movi.2d 0xFF` + inlined `str q`) is faithful. ⇒ the most likely remaining culprit: after resize,
hashbrown WRITES BACK the new ctrl ptr + bucket_mask into `self.logical_items.table` (a `self+offset` STORE).
If that self-relative STORE mis-lifts, self.logical_items.ctrl stays null/old → the reprobe reads `[null]`=zeros
→ never finds EMPTY(0xFF) → hangs. This matches: g181 fresh LOCAL map (write-back to an SP-relative local)
works; self.logical_items (write-back to self+offset) hangs; universal (not depth); all instrs faithful.
So the DEEP bug = a self-relative load/store value mis-lift in self.logical_items.entry()'s resize+reprobe.

DEEP-FIX STATUS: thoroughly characterized but NOT pinned to the exact instruction — observability is the wall
(every diagnostic that reads/writes via a computed address mis-lifts, which IS the bug, so the probes lie). 5
rebuild+relift cycles this cycle. RECOMMENDED next approach (different angle, since direct probing is exhausted):
(a) a TRANSPILER-LEVEL memory-write TRACER (log every __remill_write_memory addr+val in the resize fn, like
AZ_SP_TRACE but for stores) → see if the ctrl write-back lands at the wrong address; (b) lift the specific
hashbrown `resize`/`reserve_rehash` monomorph for HashMap<CacheId,Arc<Vec>> standalone + diff its self-field
store IR vs the working HashMap<u64,u64>; (c) accept the g180 bypass as the solution and move to hello-world's
remaining (non-hang) blockers. The g180 bypass WORKS and ships both web-nested-text + hello-world layout.
All diags reverted; clean g180 bypass + handoff remain. UNCOMMITTED.

## g184 (2026-06-09): built a GUEST-WRITE TRACER (transpiler) to break the observability wall. Tooling-blocked.
Recommendation #1 from g183: observe where the hashbrown ctrl write-back lands WITHOUT an inline probe (which
itself mis-lifts). Built `instrument_guest_writes` + `parse_write_memory_call` (transpiler_remill.rs) +
AZ_WRITE_TRACE=<stem> gate: logs (addr,val) of every i64 `__remill_write_memory` to a ring @0xD0000 (counter
0xCFFF0); harness g184 block filters for stack-addr writes of heap-ptr values (= the RawTable ctrl/bucket_mask
write-back). Iterations:
  • AZ_WRITE_TRACE=RawTable,reserve_rehash → matched 0 fns ⇒ the hashbrown resize is INLINED into layout_flow
    (no separate RawTable fn in the lift).
  • AZ_WRITE_TRACE=layout_flow on OPT.ll → 0 writes ⇒ opt INLINES `__remill_write_memory` into direct `store`s
    before the pass ran. FIX: moved the tracer to run on PATCHED.ll (pre-opt, calls still present).
  • AZ_WRITE_TRACE=layout_flow on patched.ll → traced 241 i64 guest-writes in layout_flow ✓ (tracer WORKS),
    BUT the instrumented wasm fails to instantiate: `LinkError: Import #0 "env" "memory": memory import must be
    a WebAssembly.Memory object` (the 241-write bloat in the huge layout_flow fn breaks wasm memory import/
    codegen). So the trace data couldn't be read this cycle.
TRACER STATUS: built + functional (traces on patched.ll), OFF BY DEFAULT (AZ_WRITE_TRACE unset = no-op), KEPT as
reusable infra. BLOCKER to debug next: the LinkError when instrumenting a large fn — likely needs (a) tracing
only a NARROW window (e.g. only writes whose addr is computed from an inttoptr in the stack range, via an inline
select-guard to cut volume), or (b) a smaller target fn, or (c) fixing the harness memory-import path for the
bloated wasm. Once readable: see if the ctrl write-back addr != ~0x2e0e8+24 (self-relative store mis-lift) or if
the ctrl-init 0xFF lands at the wrong heap addr.
HONEST STATUS: the deep fix (delete g115/g118/g180 + force_ifc + out-param hacks) has resisted ~11 observation
attempts this session — every angle hits a layer (diagnostics mis-lift / inlining / opt-inlining / wasm
instantiation). The g180 bypass WORKS and ships both web-nested-text + hello-world layout. The deep bug is a
self-relative load/store value mis-lift in hashbrown entry() under the lift, universal (not depth, not Vec-len).
The current BUILT dylib is the broken g184c experiment (un-bypass + tracer) — a clean rebuild from the now-clean
source restores the working g180 state. UNCOMMITTED.

## g185 (2026-06-09): DEEP-FIX STOP POINT — observability wall confirmed; the ONE experiment that cracks it.
The write-tracer LinkError is NOT a missing import (harness cbEnv provides `memory`, line 107) — it's the
241-traces × ~13 SSA-locals ≈ 3100-local BLOAT of the huge layout_flow fn tripping a wasm codegen/size boundary
(same family as the earlier "local count too large"). Volume can't be cut at transpile time without losing the
target write. So the tracer-on-layout_flow approach is wall #12.

★ THE SINGLE EXPERIMENT THAT WOULD CRACK THE DEEP BUG (for the next agent — do THIS, not another inline probe):
Write a STANDALONE EXECUTION-DIFF for hashbrown entry(). In a tiny dedicated #[no_mangle] export
`az_hashbrown_selftest(cache: &mut TextShapingCache) -> u32` (azul, web_lift-gated, called once from the
harness via a new AzStartup_ shim): insert ONE entry into `cache.logical_items` (the REAL self-relative map
that hangs) with a fixed key, then `get` it; return found?1:0. This is SMALL (no layout_flow bloat) so the
tracer (AZ_WRITE_TRACE) fits AND it instantiates. If it hangs/found=0 → the self.<field> map mis-lifts even in
isolation (the deep bug, now traceable in a tiny fn); if found=1 → the bug needs the full layout_flow context
(stack depth / inlining) and only an execution-diff vs native at the wasm level will find it. EITHER outcome
is decisive and the tiny fn dodges every wall hit this session (bloat, inlining, diagnostics-mis-lift).
Alternative cheaper-still: reduce AZ_WRITE_TRACE volume by capping at the LAST 64 writes (a wrap-only ring with
a "started" flag set just before entry()) so layout_flow only gets ~64 trace blocks not 241.

STATUS: g180 bypass WORKS (web-nested-text + hello-world lay out). Deep bug = self-relative load/store value
mis-lift in hashbrown entry(), UNIVERSAL (not depth, not Vec-len — cron framing disproven g183). ~12 observation
walls this session. Tracer infra built (transpiler_remill.rs instrument_guest_writes, off-by-default, traces on
patched.ll — works but bloats large fns). RECOMMENDATION: stop piecemeal probing; either (a) the standalone
selftest above, or (b) accept the bypass + finish hello-world (counter/click). Source clean, dylib = clean g180.
UNCOMMITTED.

## g186 (2026-06-09): ★★★★★★★ BREAKTHROUGH — it's an OPTIMIZER LOAD-FORWARDING HEISENBUG (g157's class).
The g185 selftest gave a TINY reproducer: `az_hb_local_test` (644 bytes, a DIRECT-LOCAL
HashMap<u64, Arc<Vec<LogicalItem>>> insert+get) HANGS, while g181's HashMap<u64,u64> WORKS. Trigger = the
`Arc<Vec<LogicalItem>>` VALUE TYPE (NOT self-relative — it's a direct local; NOT a decoder gap — lifts with 0
HandleInvalidInstruction). The hashbrown probe bytes are IDENTICAL+faithful to layout_flow's, so the
insert/resize writes wrong control bytes.
★ THE DECIDER: fixed the write-tracer (match `store volatile i64` on opt.ll, not __remill calls on patched.ll —
no more LinkError) and traced az_hb_local_test. RESULT: with the tracer's volatile stores added,
az_hb_local_test STOPS HANGING (HARNESS-EXIT 0, lays out!). ⇒ the volatile barriers DEFEAT the bug ⇒ it's an
OPTIMIZER mis-transformation: a guest STORE (control-byte init) and a guest LOAD (probe read) of the SAME memory
get cloned into DIFFERENT alias scopes on inline → the optimizer wrongly treats them as non-aliasing → FORWARDS
a stale/zero value to the probe → probe reads bad control bytes (no 0xFF EMPTY) → spins. EXACTLY the g157 class
("LLVM clones the guest alias-scope on inline → cross-inline same-memory store/load wrongly non-aliasing →
stale forwarded"), which g157 only partially fixed (made __remill_read_memory volatile; the hashbrown
NEON/SROA'd control-byte load isn't covered).
⇒ This is the cron's bug, finally pinned: "OPTIMIZED lifted Rust mis-EXECUTES; volatile reads give the right
value; static IR correct" = the alias-scope-clone load-forwarding. NOT a Vec-len specifically — it's ANY
guest-write-then-guest-read of the same memory across an inline (here: hashbrown ctrl-init then probe-read).
FIX DIRECTION (transpiler, the cron's goal): the guest alias-scope handling (tag_state_accesses). Either (a)
don't give guest accesses a distinct alias scope that survives inline-cloning as false-non-aliasing, or (b)
extend the g157 volatile coverage to the post-SROA/NEON guest loads, or (c) add the barrier the tracer adds.
TESTING (g187, in flight): AZ_NO_HOST_SCOPE=1 (conservative AA) on the clean az_hb_local_test repro — if it
stops hanging, the alias-scope is confirmed as the cause + the transpiler fix is to fix the scoping.
TOOLS: az_hb_local_test = the MINIMAL repro (tiny, in cache.rs, web_lift-gated, REVERT after). Write-tracer now
works on opt.ll (transpiler_remill.rs). UNCOMMITTED.

## g187/g188 (2026-06-09): ★★★★★★★★ THE FIX — AZ_NO_HOST_SCOPE fixes the REAL entry(). Alias-scope load-forwarding.
g187: AZ_NO_HOST_SCOPE=1 (conservative AA) makes az_hb_local_test STOP HANGING ⇒ the alias-scope is the cause.
g188: un-bypassed the REAL self.logical_items.entry() + AZ_NO_HOST_SCOPE=1 → HARNESS-EXIT 0, overflow_size
39.10x20.05 ✓✓✓ LAYS OUT. So AZ_NO_HOST_SCOPE FIXES THE REAL PATH (g176's earlier "doesn't fix" was a stale
dylib state — pre the accumulated g157/g164/etc. fixes). CONFIRMED ROOT CAUSE = the host/guest alias scopes
(tag_state_accesses) let LLVM prove guest≠host for SROA, but on INLINE the guest scope is CLONED → two guest
accesses to the SAME memory (hashbrown ctrl-init STORE then probe-read LOAD) get different cloned scopes →
optimizer treats them as non-aliasing → FORWARDS a stale/zero value → probe reads bad control bytes → spins.
Disabling the scopes → conservative AA → no false-non-aliasing → correct (bigger/slower wasm = the accepted
web-lift trade-off). This IS the cron's bug ("optimized fn mis-EXECUTES; volatile reads right; IR correct") —
NOT a Vec-len specifically, but the same alias-scope class for ANY guest-write-then-read across an inline.
THE FIX (transpiler, cron's goal): make AZ_NO_HOST_SCOPE the DEFAULT (conservative AA) → DELETE the azul
workarounds (g115/g118/g180 HashMap bypasses; likely force_ifc + the out-param hacks too, all the same class).
DOING NOW: default-on the no-host-scope, delete bypasses, rebuild+relift hello-world to confirm the cron goal.

## g189 (2026-06-09): ⚠ CORRECTION — AZ_NO_HOST_SCOPE "fix" was CONFOUNDED by AZ_FUEL. Bug = barrier-defeatable.
HONEST WALK-BACK of g187/g188: I declared AZ_NO_HOST_SCOPE the fix, but EVERY "fixed" run (g186/g187/g188) also
had AZ_FUEL=ALL, whose per-terminator fuel-tick VOLATILE writes act as optimization BARRIERS that ALSO defeat
the heisenbug. g189 = the gate made default-conservative (AZ_NO_HOST_SCOPE behavior) + bypasses DELETED + NO
AZ_FUEL + normal run → HARNESS-EXIT 124 (HANGS). ⇒ conservative AA ALONE does NOT fix it.
Matrix (real un-bypassed entry()): {host-scope + AZ_FUEL}=hang(g180, trap@200M); {no-host-scope + AZ_FUEL}=
WORKS(g188); {no-host-scope + no-fuel}=HANG(g189). So neither AZ_FUEL-barrier alone nor no-host-scope alone
fixes it; the combination did. The bug is REAL + barrier-defeatable (an optimizer load-forward / dead-store-elim
/ reorder of guest mem ops across an inline-cloned alias scope, exactly the cron's "optimized fn mis-EXECUTES")
but the precise principled fix is entangled with AZ_FUEL as a confound and is NOT yet isolated.
REVERTED: gate back to AZ_NO_HOST_SCOPE env (default-off); the g180 logical/visual/shaped bypasses RESTORED;
selftest removed. The g180 bypass remains the WORKING solution (web-nested-text + hello-world lay out).
SOLID GAINS this session: (1) az_hb_local_test = a 644-byte MINIMAL REPRO (HashMap<u64,Arc<Vec>> insert+get
hangs; HashMap<u64,u64> works). (2) The bug is barrier-defeatable (AZ_FUEL ticks / write-tracer volatile stores
both make it complete) ⇒ it's an opt mis-transform, NOT a decoder gap, NOT a value computed-wrong. (3) Write-
tracer works on opt.ll (matches `store volatile i64`). 
★ CLEAN NEXT EXPERIMENT (isolate barrier from AZ_FUEL): re-add az_hb_local_test + run AZ_WRITE_TRACE (barriers)
WITHOUT AZ_FUEL. If it stops hanging → the BARRIER is the fix → add a minimal "volatile fence per guest store"
transpiler pass (write-tracer minus logging) as default → delete bypasses. If it still hangs → AZ_FUEL's
specific instrumentation (not generic barriers) matters → investigate what AZ_FUEL changes. UNCOMMITTED.

## g190 (2026-06-09): ★★★ THE FIX ISOLATED — a VOLATILE BARRIER per guest store fixes it (no AZ_FUEL confound).
Clean isolation on the minimal repro az_hb_local_test, NO AZ_FUEL: AZ_WRITE_TRACE=az_hb (which adds a volatile
ring store after each guest write = an optimization BARRIER) → HARNESS-EXIT 0, LAYS OUT. So the BARRIER ALONE
fixes the hang, independent of AZ_FUEL (g189's confound). ⇒ the real bug = an optimizer mis-transform
(load-forward / dead-store-elim / reorder) of guest memory ops that a volatile fence between guest writes
defeats. THIS is the cron's deep bug + the principled transpiler fix: a "volatile fence per guest store" pass
(the write-tracer minus the logging). Control (no barrier, no fuel) confirming the hang in flight.
THE FIX TO LAND: a default transpiler pass that inserts a minimal optimization barrier after each guest
`store volatile i64` (or a `@llvm.sideeffect` / a single dummy volatile store) so the optimizer cannot
forward/reorder/DSE guest mem ops across the inline-cloned alias scope. Then DELETE g115/g118/g180 + the
out-param hacks (all the same class). Refine to minimize perf cost (only fence stores that feed later same-region
loads, or per-function). NEXT: build that barrier pass (reuse instrument_guest_writes structure, emit just the
fence), make default, rebuild+relift hello-world to confirm + delete the bypasses. UNCOMMITTED.

## g190-CORRECTION (2026-06-09): ⚠⚠ az_hb_local_test was a FALSE REPRO — AZ_FUEL itself perturbs behavior.
Clean control (g190control: NO barrier, NO AZ_FUEL) ALSO → HARNESS-EXIT 0 (no hang). So az_hb_local_test does
NOT hang without AZ_FUEL — it only "hung" in g185 BECAUSE AZ_FUEL was set. ⇒ **AZ_FUEL is NOT a passive observer;
its per-terminator instrumentation CHANGES the optimization/codegen and can INDUCE (or mask) the hang.** The
"minimal reproducer" az_hb_local_test reproduced a FUEL-INDUCED artifact, NOT the real bug. So the entire
g186-g190 chain (barrier fixes it, Arc<Vec> value-type trigger, "the barrier is the fix") was about the
AZ_FUEL artifact and is UNRELIABLE. Likewise g187/g188's AZ_NO_HOST_SCOPE "fix" — all under AZ_FUEL.
THE REAL, FUEL-FREE FACTS (the only trustworthy ones): web-nested-text with the REAL self.logical_items.entry()
+ NO AZ_FUEL HANGS (exit 124; g189 confirmed with bypasses deleted). The g180 HashMap-bypass + NO fuel → LAYS
OUT. That is the entire reliable signal. az_hb_local_test does NOT reproduce it (fuel-free).
⇒ FUNDAMENTAL OBSERVABILITY WALL: the real infinite loop only manifests WITHOUT AZ_FUEL, but AZ_FUEL is the only
tool that traps it for inspection — and using it CHANGES the outcome. Every fuel-based localization this session
(GID mapping, ring dumps, selftest) is therefore suspect for the no-fuel behavior. REVERTED everything: clean
g180 bypass restored, gate default-off, repro/selftest removed. The g180 bypass remains the WORKING solution
(hello-world lays out). HONEST: the deep fix needs a FUEL-FREE observation method (a wasm watchdog that snapshots
state on a wall-clock timeout, or single-stepping) — a real tooling project, not another fuel-based probe. ~17
relift cycles this session; the deep fix is genuinely hard + the primary tool (AZ_FUEL) is unreliable for it.

## g191 (2026-06-09): ANALYSIS-FIRST (no relift/fuel) of the alias-scope machinery — simple theory doesn't hold.
Read tag_state_accesses + the helper IR scope graph (transpiler_remill.rs 6315-6400, 6744-6840):
  • Guest mem ops (via __remill_read/write_memory helpers) are `load/store VOLATILE` + `!alias.scope !guest,
    !noalias !host`. tag_state_accesses tags the BARE (State/local) loads/stores `!alias.scope !host, !noalias
    !guest` (NOT volatile — they're the register file).
  • So guest accesses are ALREADY volatile ⇒ the optimizer can't forward/DSE/reorder them. And ALL guest
    accesses share ONE guest scope ⇒ two guest accesses are NOT in each other's noalias list ⇒ ScopedAA says
    they MAY alias (no false non-aliasing between guest accesses). On inline LLVM clones the scope, but the
    clone (guest') and original (guest) still don't exclude each other ⇒ still MAY alias.
  ⇒ The simple "g157 cloned-guest-scope → false non-aliasing → stale forward" theory does NOT cleanly explain
    the hashbrown hang: the relevant guest mem ops are volatile AND same-scope-aliasing. So the real mechanism
    is SUBTLER than any theory validated this session (alias-scope, barrier, Vec-len — all disproven or unclear).
HONEST DEEP-FIX STATUS (after ~17 cycles + multiple corrected errors): the ONLY reliable facts are (1) web-
nested-text NO-FUEL hangs on self.logical_items.entry()'s hashbrown probe; (2) g180 bypass fixes it (hello-world
lays out); (3) the instructions lift faithfully; (4) AZ_FUEL PERTURBS the bug (unreliable diagnostic). Every
mechanistic claim beyond that came from fuel-perturbed/confounded runs and is suspect. The remaining paths are
both real projects: (A) a FUEL-FREE dynamic observer (wasm watchdog snapshotting state on a wall-clock timeout),
or (B) deep static analysis of layout_flow's 15k-line OPT.LL for the exact optimizer transform that breaks the
probe (needs a non-fuel KEEP_SCRATCH opt.ll + careful diff of the control-init store vs probe load). The
fuel/env-flag probing approach is exhausted + unreliable. g180 bypass = the working solution. Tracer infra
(instrument_guest_writes on opt.ll) + AZ_NO_HOST_SCOPE gate remain (off-by-default). UNCOMMITTED.

## g192 (2026-06-09): ★ FUEL-FREE OPT.LL ANALYSIS — the IR is FAITHFUL ⇒ bug is wasm-CODEGEN, not the IR.
Did the cron's analysis-first PROPERLY (no fuel = no confound): un-bypassed logical_items, NO-FUEL KEEP_SCRATCH
lift → got the real opt.ll files. Found 130 hashbrown-probe (llvm.cttz) functions + 9 with the 0xFF ctrl-init.
Static analysis of the find-probe (sub_a94450, 15k lines) + the ctrl-init resize fns:
  • The probe's control-group is loaded via `load volatile double/i64, !alias.scope !3` (GUEST), RE-LOADED each
    outer iteration (PHI'd between two real loads, NOT a stale-forwarded register). The per-byte empty-check
    (==0xFF) + umaxv reduction + cttz are all CORRECT.
  • The 0xFF ctrl-init is PRESENT as `store volatile i64 -1, ptr ..., !alias.scope !3` (GUEST) in the resize
    fns — NOT dead-store-eliminated.
  ⇒ BOTH the control-byte WRITE (0xFF init) and READ (probe) are FAITHFUL in the optimized IR. So the "optimizer
    forwards/DSEs/reorders the control bytes" theory (and every alias-scope/barrier theory) is DISPROVEN by the
    static IR — the IR is CORRECT.
⇒ CONCLUSION (matches the cron's framing exactly): "the static IR is correct; only the full optimized fn
  mis-EXECUTES." The bug is NOT in remill's decoders, NOT in the optimizer/alias-scopes — it's in the **llc
  AArch64-IR → WASM CODEGEN** (or the wasm runtime) of the CORRECT IR. This also EXPLAINS why AZ_FUEL is an
  unreliable diagnostic: AZ_FUEL injects IR (fuel-tick calls) → changes what llc codegens → masks/moves the
  actual codegen bug (so fuel-on and fuel-off differ; the whole session's fuel-based localization is suspect).
  And it's consistent with the empirically-reliable fact: entry() (no fuel) triggers the hang; the g180 bypass
  (avoids entry()) fixes it.
NEXT (the real path, no more fuel/IR probing): DISASSEMBLE THE WASM. Get the azul-mini.wasm function for the
hashbrown probe (sub_a94450 / __az_dep_109d0c450) and compare its WASM to the (correct) opt.ll — find the
specific IR construct llc mis-lowers to wasm (likely the i128/<16 x i8> NEON ops, the cttz, or the volatile
double load→i64 bitcast). Fix = the transpiler emits an llc-safe IR pattern for it, OR an llc flag, OR lower
the offending op differently. opt.ll files kept at azul-web-transpiler-81046 (this run). g192 un-bypass REVERTED
(g180 bypass restored). The g180 bypass remains the working solution. UNCOMMITTED.

## g193 (2026-06-09): WASM disasm — scalarized, no SIMD; structure intractable for bounded static diff.
Disassembled the probe's wasm object (__az_dep_109d0c450.o = sub_a94450, the hashbrown find-probe) via
wasm-objdump → /tmp/probe.wat (34710 lines, 3 fns). Findings:
  • NO wasm SIMD (i8x16/v128 = 0). llc SCALARIZED all the NEON ops (cmeq.8b → scalar byte compares;
    umaxv → the 7x llvm.umax.i8 chain seen in opt.ll; the control group is loaded as scalar i64, not v128).
    28 i64.ctz, 1382 i64.load, 2600 i64.store, 80 loop, 265 br_if.
  • The inner match loop carries the mask in a local (i64.ctz → i64.and → local.tee = x&(x-1)). The control
    group is held in locals (local.get 27-30 feed the ctz region).
  ⇒ To find a mis-lowering I'd need a full wasm↔opt.ll instruction diff over a 15k-IR / 34k-wat function +
    trace whether the outer-loop control-group locals are re-loaded vs stale — that is NOT tractable as cheap
    static reading, and I won't make another over-confident call without runtime confirmation.
HONEST FINAL STATE OF THE DEEP FIX (after ~18 cycles + several corrected errors this session): the bug is
reliably narrowed to "correct optimized IR that mis-executes at the llc-wasm-codegen/runtime layer" (the cron's
exact framing; g192 proved the IR faithful). It is NOT a decoder gap, NOT an optimizer/alias-scope issue, NOT a
Vec-len-specific decode. AZ_FUEL is unreliable for it (it changes the codegen). The two remaining paths are both
real projects: (A) a FUEL-FREE dynamic observer (wasm watchdog snapshotting state on a wall-clock timeout — the
only way to see the no-fuel behavior without perturbing it), or (B) an instruction-level wasm↔IR diff of the
scalarized probe to find the exact mis-lowered construct (tedious, uncertain). Cheap static analysis is
exhausted. The g180 bypass is the working solution (hello-world lays out). opt.ll/.o artifacts at
azul-web-transpiler-81046. UNCOMMITTED.

## g194 (2026-06-09): +simd128 wasm codegen TESTED (fuel-free) → does NOT fix it. Scalarization ruled out.
Hypothesis from g193: llc scalarizes the NEON ops (no wasm SIMD) → maybe the scalar lowering is buggy. Test:
set the in-process wasm TargetMachine features `""` → `"+simd128"` (azul_remill.cpp:716, the ACTIVE path —
build uses web-transpiler-static → native_remill::compile_to_wasm32_obj, NOT the subprocess llc). Un-bypassed +
NO FUEL (clean). RESULT: HARNESS-EXIT 124 (still HANGS). The wrapper.a WAS rebuilt (07:07) + the lifted fns have
NO target-features attribute override (just `alwaysinline`) so +simd128 applies + it instantiated (hung, not a
SIMD CompileError) ⇒ SIMD codegen took effect and the bug PERSISTS ⇒ the vector SCALARIZATION is NOT the bug.
REVERTED (+simd128 → "", bypass restored). DISPROVEN-HYPOTHESIS LIST (all via reliable, mostly fuel-free tests
this session): remill decoder gap; optimizer load-forward/DSE/reorder; host/guest alias-scope (AZ_NO_HOST_SCOPE,
g188/g189 confounded by AZ_FUEL); the "barrier fix" (g190, az_hb_local_test was a FALSE repro — only hung under
AZ_FUEL); Vec-len-specific decode; wasm SIMD-vs-scalar codegen (g194). The IR is FAITHFUL (g192); the bug is a
correct-IR-mis-executes at the wasm-codegen/runtime layer whose exact construct is NOT findable by cheap static
analysis and whose dynamic observation perturbs it (AZ_FUEL). g180 bypass = working solution. The ONLY untried
reliable path left is a real tooling project: a fuel-free wasm watchdog (snapshot linear memory on wall-clock
timeout) — though even that can't see wasm LOCALS (where SROA'd values live) without instrumentation=perturbation.
This deep fix is at the edge of tractability with current tools. UNCOMMITTED. ~19 relift cycles this session.

## g195 (2026-06-09): ★★ MAJOR REDIRECT — collect_and_measure (the cron's Vec-len target) is DEAD on the web lift.
Did the cron's literal ask (analysis-first): disassembled collect_and_measure_inline_content_impl's direct
dom_children.len() read (g139 workaround reverted) → it's a MEMORY load `ldr x9,[sp,#0x3a0]` (the Vec's len
field), NOT a register-held SROA value. Then TESTED (fuel-free, g180 bypass active) with the direct len():
  • web-nested-text: g195 marker UNSET ⇒ collect_and_measure_inline_content_impl NOT reached. Lays out (39x20).
  • hello-world: "collect_and_measure lifted? 0" ⇒ the fn is NOT EVEN LIFTED (not in the call graph from the
    layout cb). g195 marker UNSET. Lays out (98x16).
⇒ collect_and_measure_inline_content_impl is DEAD CODE on the web lift, for BOTH examples. The text is collected
  via a DIFFERENT live path (measure_intrinsic_widths + layout_flow, with their g115/g118/g180 bypasses). So the
  cron's PRIMARY TARGET — "the SROA'd Vec::len() in collect_and_measure's dom_children loop" — is on a path that
  NEVER EXECUTES on the web lift. THAT is why it was never reproducible across the whole session. The g139
  workaround there is irrelevant/dead (restored anyway: harmless, unverified-dead for other layouts).
⇒ THE CRON'S PREMISE IS MISDIRECTED. The real, LIVE web-lift blockers are the HashMap bypasses (g115/g118 in
  measure_intrinsic_widths + g180 in layout_flow) = the hashbrown-probe class (A), which g192 proved is faithful
  IR mis-executing at the wasm-codegen layer (NOT a Vec-len, NOT collect_and_measure). The 6 out-param hacks +
  force_ifc should likewise be re-audited for whether they're even on a live path (g195 shows at least one cron
  target was dead). NEXT: audit which workarounds are LIVE (reached) vs DEAD (like g139) — delete the dead ones
  (free cron-goal wins); the live ones are all the hashbrown wasm-codegen class. g195 changes REVERTED (clean
  g139 workaround restored). g180 bypass = working. UNCOMMITTED.

## g195-AUDIT (2026-06-09): which workarounds are LIVE vs DEAD on the hello-world web lift (call-graph check).
DEAD (not lifted/reached → workaround is inert → deletable, but keep unless verified dead for ALL layouts):
  • collect_and_measure_inline_content_impl (g139 Vec-len workaround) — the cron's PRIMARY target. DEAD.
  • shape_glyphs (a g127 glyphs-out-param site) — DEAD (shaping goes via shape_text_internal instead).
LIVE (lifted/reached → workaround is load-bearing → needs the deep wasm-codegen fix to delete):
  • measure_intrinsic_widths (g115/g118 HashMap bypass) • layout_flow (g128 &Vec + g180 HashMap bypass)
  • layout_formatting_context (force_ifc, 6 refs) • collect_inline_span_recursive (g129/g130 out-params)
  • shape_text_internal (g127 glyphs out-param) • create_logical_items/reorder_logical_items/
    has_only_inline_children/determine_formatting_context (force_ifc support).
⇒ CONCLUSION: the cron's premise (Vec-len in collect_and_measure) targets a DEAD path. The LIVE workarounds are
  ALL the same class — g192 proved the hashbrown (g180) is faithful IR mis-executing at the llc-wasm-codegen
  layer; the out-param/force_ifc ones are the same "correct IR mis-executes" class (NOT decoder, NOT optimizer,
  NOT Vec-len-decode — comprehensively disproven this session). Deleting the LIVE workarounds requires fixing
  that ONE wasm-codegen bug, which is a deliberate project (LLVM-with-assertions + bisect the wasm backend, or
  a single-step wasm tracer), NOT a cheap analysis-first probe. The session's ~21 cycles have fully MAPPED the
  problem: 1 working fix (g180), the cron's target proven dead, the real bug localized to wasm-codegen, and
  every cheap hypothesis disproven. UNCOMMITTED. The g180 bypass + workarounds = the working solution.

## g196 (2026-06-09): scope-consistency hypothesis CLOSED (negative) + 32-bit-truncation lead + metadata-probe EXP.
New angle g192 never explicitly checked: is the ctrl-byte WRITE (fill_empty) consistently guest-scoped with the
ctrl-byte READ (probe)? Checked statically in surviving g192 scratch (azul-web-transpiler-81046, .opt.ll + the
matching wasm .o — no relift). Findings (all in the hashbrown deps):
  • Probe ctrl READS = `load volatile i8, !alias.scope !3` (guest, volatile) — CORRECT.
  • Table ctrl FILL = `store volatile i8 -1, !alias.scope !3` (×136, guest, volatile) — CORRECT.
  • The other 0xFF writer = `llvm.memset(ptr %D1, i8 -1, 16, i1 false)` (non-volatile, untagged) — but
    `%D1 = getelementptr ptr %state, i32 32` = a STATE REGISTER SLOT ⇒ it's the NEON `movi v1.16b,#0xff`
    EMPTY-splat constant into the register file (benign, correctly host-scoped). NOT a mis-scoped table fill.
  ⇒ SCOPE HYPOTHESIS = NEGATIVE. Fill & probe are both correctly guest-scope + volatile. The last cheap static
    hypothesis g192 hadn't checked is now eliminated. (Also re-confirms: IR is faithful AND correctly mem-tagged.)
NEW SHARPER LEAD (fits the context-dependence az_hb_local_test exposed): every guest access lifts as
`inttoptr i32 %addr to ptr` — guest addresses are TRUNCATED TO 32 BITS for the wasm32 linear memory. Invisible to
az_hb_local_test (clean allocator, low addresses) but could bite layout_flow (deep allocator state): if the
hashbrown ctrl region's guest addr aliases another live region mod 2^32, a write elsewhere overwrites the 0xFF
ctrl bytes → probe reads non-0xFF → spins. This is a RUNTIME-ADDRESS issue (not visible in static IR — consistent
with "IR faithful but mis-executes"), and the fix would live in the transpiler/remill mem-translation.
EXPERIMENT IN FLIGHT (g196): split "corrupt bucket_mask (metadata)" vs "corrupt ctrl bytes (sane metadata)".
Edited layout_flow (cache.rs ~5689, web_lift path, TEMP — revert to g180 bypass after): on the empty map,
`self.logical_items.reserve(1)` allocates+fills the table WITHOUT a probe (nothing to rehash → no hang), markers
capture capacity()/len() of the *allocated* table (tagged 0xCA…/0x1E… to tell "value 0" from "write dropped";
const sentinels 0x96020000 pre-reserve @0x607EC, 0x96010001 post-reserve @0x607E8), then `.entry()` probes it
(hangs) → fuel-trap → harness catch reads 0x607E0/E4/E8/EC (readout added to layout-flexbox.js after the g73
LayoutTree line). PREDICTION: capacity SANE (e.g. 3) + entry still fuel-traps ⇒ ctrl bytes corrupt despite good
metadata ⇒ confirms the 32-bit-alias/ctrl-fill-mis-exec lead (NOT a bucket_mask mis-write). Rebuild + relift
web-nested-text with AZ_FUEL running now. REVERT the cache.rs probe + harness readout after reading the result.

## g196 RESULT (2026-06-09): ★★ capacity=3 SANE + reserve(1)+entry() did NOT hang (under AZ_FUEL). BIG LEAD.
Ran the metadata probe (reserve(1) + 4 volatile markers + entry(), AZ_FUEL=ALL, 1684 fns fueled). Harness EXIT=0:
  • pre-reserve sentinel(0x607EC)=0x96020000 REACHED; reserve-done(0x607E8)=0x96010001 YES (reserve didn't hang).
  • capacity(0x607E0)=0xCA000003 ⇒ **capacity = 3 (SANE bucket_mask=2)**; len(0x607E4)=0 (empty pre-entry). [the
    harness "DROPPED" label is a JS signed-`&` bug on the 0xCA tag-check; the raw 0xCA000003 is a valid tagged 3.]
  • **layout_flow = Ok, inline_content.len = 1, overflow_size 39.10×20.05 — TEXT LAID OUT. NO hang, NO fuel-trap.**
⇒ TWO findings: (1) metadata is SANE (kills the "corrupt bucket_mask" branch — IF a bug remains it's in ctrl
  bytes, per prediction). (2) ★ The original `entry()`-without-reserve HANGS both w/ fuel (GID-via-fuel find) AND
  w/o fuel (g189, 124). My `reserve(1)` + markers + entry() COMPLETED under fuel. So `reserve(1)+markers` AVOIDS
  the hang. This LOCALIZES the hang to entry()'s INTERNAL alloc-during-insert (reserve→resize→reprobe) path — a
  FRESH-ALLOC-then-probe sequence — NOT the steady-state probe. A pre-`reserve(1)` separates the alloc from the
  probe and (apparently) sidesteps it. THIS IS A NARROW TARGET (hashbrown reserve/resize/insert vs the whole map).
  CONFOUND TO RESOLVE: is it `reserve(1)` (real code) or the 4 volatile markers (codegen perturbation, AZ_FUEL-like)?
NEXT (g197, IN FLIGHT): `reserve(1)` ALONE, ZERO markers, NO fuel, timeout-wrapped harness. Lays out ⇒ reserve(1)
  is a genuine fix (better workaround than g180 bypass: keeps the cache; AND pinpoints the bug to the resize path
  → lift just hashbrown resize/insert, find the mis-lifted instr = the cron's real target). Hangs(124) ⇒ the
  markers were masking (perturbation) → reserve(1) is NOT it, reconsider. Rebuilding now (markers removed).

## g197 RESULT (2026-06-09): ★★★ reserve(1) ALONE (no markers, NO fuel) FIXES THE HANG. Bug localized to entry-resize.
Clean isolation run: layout_flow web_lift path = `self.logical_items.reserve(1); …entry()…` (ZERO markers), relift
WITHOUT AZ_FUEL, harness under `timeout 90`. RESULT: HARNESS_EXIT=0, NO hang. `[g132] overflow_size 39.10×20.05
TEXT LAYS OUT`, `[g133] inline_content.len=1, layout_flow=Ok`. ⇒ **reserve(1) is a GENUINE fix** — not the markers
(removed) and not fuel (off). The bare `entry()` hangs w/o fuel (g189=124); `reserve(1)+entry()` lays out w/o fuel.
⇒ THE HANG IS IN `entry()`'s INTERNAL resize-during-insert → re-probe path (reserve_rehash→resize→find_insert_slot),
  NOT the steady-state probe. Pre-`reserve(1)` does the resize in a SEPARATE call → entry()'s probe then runs on an
  already-allocated table → no hang. This is THE narrow target (hashbrown resize/insert, ~a handful of fns) for the
  cron's remill/transpiler fix. reserve(1) itself is still an azul workaround (cron forbids), BUT it's (a) a far
  better interim workaround than the g180 bypass (keeps the cache, 1 line) and (b) a precise LOCALIZATION.
  PUZZLE for the mechanism hunt: for an EMPTY map, both the static_empty probe (all-0xFF) and the post-resize
  reprobe of the freshly-0xFF-filled new table should find EMPTY immediately — so a normal entry() should NOT spin.
  It DOES spin ⇒ in the entry-internal resize, EITHER (a) the new table's ctrl fill (memset 0xFF) mis-executes
  (ctrl has no 0xFF byte → probe never finds EMPTY → spin) OR (b) the reprobe reads a WRONG ctrl pointer (garbage,
  no 0xFF). g196 showed the SEPARATE reserve's fill → capacity=3 SANE, so the standalone fill is fine; the
  entry-INTERNAL resize's fill/reprobe is the suspect. Alias-scope forwarding ruled out (AZ_NO_HOST_SCOPE g189
  didn't fix). NEXT: lift hashbrown resize/find_insert_slot standalone (cron's method) OR static-read the
  resize+reprobe IR in surviving scratch — check the ctrl fill (memset present+correct?) + the reprobe ctrl ptr
  (reloaded from self.table.ctrl after the resize, or stale?). cache.rs still has the g197 reserve(1) probe (TEMP).

## g198 ROOT CAUSE (2026-06-09): ★★★ the hang = a NON-VOLATILE ctrl-fill memset reordered past the VOLATILE probe.
Relifted BARE entry() (hanging) with AZ_FUEL=ALL + KEEP_SCRATCH. Harness fuel-trapped: GID(0x40070)=84421 →
`__az_dep_109f64640` = `TextShapingCache::layout_flow` itself (the hashbrown entry()/find_or_find_insert_slot/
reserve/reprobe ALL INLINED into it, size 3832, 391 fueled terms; 717KB opt.ll — the whole chain in one file).
Traced the spinning loop (block 215/202):
  • The find loop's match_empty NEVER fires ⇒ the ctrl group it reads has no 0xFF byte ⇒ the fresh table's ctrl
    fill did not take effect when the probe runs.
  • Ctrl group read = `%v.i1740 = load volatile double, ptr %p.i1739, !alias.scope !3` (GUEST, VOLATILE — correct),
    base `%v.i1232 + (pos & bucket_mask)`. match_empty is scalarized (no <16 x i8>; per-byte ==0xFF → umax chain).
  • The ctrl FILL = `call void @llvm.memset.p0.i64(ptr %dst_sub_1830fbf80.i, i8 -1, i64 %add.i.i1332, i1 false)`
    (opt.ll:466) — the lifted libc `memset@0x1830fbf80` (write_bytes(0xFF)), emitted NON-VOLATILE by the
    transpiler's LibcMemset helper. dst = trunc(ctrl_ptr), size = bucket_mask+9.
⇒ ROOT CAUSE: when the resize is INLINED into layout_flow, the NON-volatile fill memset and the VOLATILE probe
  loads are in one fn; the fill gets reordered/sunk after the probe reads (or DSE-adjacent) → the probe reads the
  bump allocator's 0x00 pre-fill bytes → never finds EMPTY → spins. A SEPARATE `reserve(1)` call (g197) fixes it
  because the call is an optimization barrier that forces the fill to complete first — NOT a codegen accident.
  This is the cron's "correct-IR-mis-executes" class, pinned to a SPECIFIC transpiler emission (non-volatile bulk
  fill vs volatile guest probe), in the cron-approved location (dll/src/web/transpiler_remill.rs).

## g199 FIX (2026-06-09, IN TEST): LibcMemset → VOLATILE memset (i1 false→true), transpiler_remill.rs:5927.
LLVM LangRef: optimizers "must not change the order of volatile operations relative to other volatile operations."
The probe's ctrl loads are volatile; making the fill memset volatile keeps it ordered BEFORE them even when
inlined. Edited LibcMemset body `@llvm.memset(..., i1 true)` + added the g199 rationale comment. TEST IS CLEAN:
cache.rs has BARE entry() (NO reserve(1), NO bypass) — so if web-nested-text lays out, the TRANSPILER FIX ALONE
killed the hang ⇒ delete the g180 bypass + g115/g118 (measure_intrinsic_widths) + reserve(1) + (re-audit) the 6
out-param/force_ifc workarounds. Rebuilding (transpiler is compiled into the dylib via web-transpiler-static),
then relift web-nested-text NO fuel under timeout 90. If it still hangs ⇒ bug is wrong-address/size not reorder;
also try making LibcMemcpy (rehash) volatile + tag the memset guest-scope (!3). UNCOMMITTED.

## g199 RESULT (2026-06-09): volatile memset did NOT fix it (HARNESS_EXIT=124, still hangs). ⇒ NOT a reorder.
Confirmed the fix APPLIED: server log shows `sub_1830fbf80 = memset, class=LibcMemset, pulled in by hashbrown
reserve_rehash`, and the bare-entry build still hung. cb+cascade run fine (text "Hello" correct, node_count=3),
hang is in layout_flow's entry probe as before. Since the probe's ctrl loads are VOLATILE (re-read each iter) and
the fill is now VOLATILE too, if they hit the SAME address the probe MUST see 0xFF → terminate. It still spins ⇒
**the probe reads a DIFFERENT ctrl pointer than the fill wrote (WRONG-ADDRESS), not a reorder.** g199 volatile
memset is harmless/correct (keep it — bulk guest writes SHOULD be volatile) but insufficient. g198 IR hint: fill
dst = *(table_A+192) [table_A=trunc(%72), single-deref+offset]; find ctrl = *(*(X26)) [double-deref]. Structurally
DIFFERENT access paths to "the ctrl pointer" — one is likely mis-lifted so they diverge. NEXT (g200, IN FLIGHT):
relift g199 build + KEEP_SCRATCH (no fuel; lift itself yields the IR; fn = TextShapingCache::layout_flow), trace
whether fill-dst-ptr and find-ctrl-ptr resolve to the same value + the fill SIZE (%v.i1188+9 — is %v.i1188 read
correctly?). reserve(1) still works (separate call → correct self.table.ctrl); bare entry's inlined resize leaves
find reading a stale/garbage self.table.ctrl. Candidate fixes once mechanism confirmed: (a) exclude hashbrown
reserve_rehash/resize from inject_alwaysinline (keep as a call = the reserve(1) barrier, generally); (b) fix the
self.table.ctrl store/load consistency across the inlined resize. cache.rs = bare entry (g198). UNCOMMITTED.

## g200 ROOT CAUSE — FULLY LOCALIZED (2026-06-09): X19/X26 base divergence in the inlined resize's self.table copy.
Captured the ctrl-fill size+dst at runtime (g200 markers in LibcMemset → fuel-trap): **size=12 (0xc), dst=0x62912f0**
(heap, just below liveBump 0x6291378). 12 = num_ctrl_bytes for a 4-bucket/8-wide table ⇒ **the ctrl fill is
CORRECT** (right size, valid heap dst, now volatile). NOT wrong-size, NOT the "length reads 0" bug. ⇒ WRONG-ADDRESS.
Traced the IR (surviving KEEP_SCRATCH 68254, __az_dep_1093a8b68 = TextShapingCache::layout_flow):
  • FILL fills new_table.ctrl buffer (0x62912f0), size from new_table count — both read at base `trunc(%72)+192/+200`.
  • The resize's `self.table = new_table` struct copy EXISTS: 18 `store volatile i64` to dest base `trunc(*(X19))+64,
    +72, +80 …` (src = new_table fields, all guest !3 volatile). NO memcpy (field-by-field word copy).
  • The FIND reads self.table.ctrl at `trunc(*(X26))+0`, bucket_mask at `+8`.
  ⇒ COPY writes self.table at **X19-base + 64**; FIND reads self.table at **X26-base + 0**. These coincide ONLY if
    `*(X19)+64 == *(X26)` at runtime. The hang ⇒ they DIVERGE: the inlined resize copies new_table into a
    self.table location that is NOT where the find reads it → find reads a stale/zero self.table.ctrl → probes
    ctrl=0 → reads 0x00 low memory → never finds EMPTY (0xFF) → spins. `reserve(1)` works because the SEPARATE call
    keeps X19/X26 consistent (the resize runs in its own frame, not inlined into layout_flow's body).
⇒ THE BUG = a remill REGISTER/BASE-TRACKING mis-lift: X19 (copy-dest base) and X26 (find-src base) hold values that
  should satisfy *(X19)+64==*(X26) but don't, in the inlined resize path. (The g199 volatile-memset is correct +
  KEPT — bulk guest fills should be volatile — but orthogonal to this addr bug; reorder was disproven g199.)
NEXT (focused session, cron's literal method): disassemble native layout_flow's resize region (otool the copy:
  the str pairs to self.table at &cache+offset vs the find's ldr from X26) → find the instruction where remill
  mis-tracks X19/X26 (likely an X19↔X26 mov/add or a spilled base reloaded wrong) → fix the remill semantic OR a
  transpiler post-pass that reloads the find's self.table base. Verify: bare entry() lays out web-nested-text.
  ★ CONVERGENCE/VALIDATION: this root cause MATCHES the team's pre-existing suspicion baked into the AZ_WRITE_TRACE
  tool — its comment (transpiler_remill.rs:5393-5398) says it catches "the hashbrown RawTable ctrl/bucket_mask
  WRITE-BACK after resize... if that store lands at the WRONG address, the self-relative store mis-lift (the deep
  bug) is caught." That IS the X19/X26 wrong-address. TURNKEY NEXT STEP (the team's intended tool): restore bare
  entry() (g201), rebuild, relift with `AZ_WRITE_TRACE=layout_flow AZ_FUEL=ALL` → the harness POST-TRAP dump
  (layout-flexbox.js:708-721, ring @0xD0000/counter 0xCFFF0) lists heap-ptr→stack-addr writes = the ctrl write-back.
  Look for where 0x62912f0 (the new ctrl) is stored: MISSING or a WRONG/garbage addr = the mis-lift, caught
  directly. CAVEAT: AZ_WRITE_TRACE + AZ_FUEL is heavy double-instrumentation (masking risk like AZ_FUEL); if the
  hang masks (no trap → no dump), trace WITHOUT fuel + add a success-path ring dump, or drop to a single fueled site.
  Then native-disasm to the offending instr → remill/transpiler fix → delete g180+g115/g118+reserve workarounds.
CONSOLIDATED STATE (this session): cache.rs RESTORED to the g180 bypass (robust working state, now documents the
  full root cause); g199 volatile LibcMemset KEPT (principled); g200 LibcMemset capture REVERTED. Workarounds still
  needed (g180 + g115/g118 + reserve(1)-class) until the X19/X26 mis-lift is fixed. Needs rebuild. UNCOMMITTED.
  ★ SESSION ARC: scope-hypothesis closed → reserve(1) confirmed fix → localized to entry-internal-resize → volatile
  memset (reorder) DISPROVEN → fill proven CORRECT → WRONG-ADDRESS pinned to X19/X26 base divergence in the copy.

## g201 RESULT (2026-06-09): AZ_WRITE_TRACE RUNTIME CONFIRMATION — write-back is CORRECT, bug is the FIND-side read.
Ran AZ_WRITE_TRACE=layout_flow + AZ_FUEL on bare entry() (the team's purpose-built tool — its comment said it
catches "the resize ctrl write-back landing at the WRONG addr = the self-relative store mis-lift"). It HUNG under
the trace (bug reproduces, NOT masked); harness POST-TRAP [g184] dumped the 0xD0000 ring (41 guest i64 writes
before the read-only find-spin). The resize's self.table write-backs:
  • *0x28178 = 0x62910c8 then *0x27e90 = 0x62910c8 (SAME ctrl → two addrs = new_table→self.table COPY working)
  • *0x27ea0 = 0x6291300 (ctrl) + *0x27ea8 = 0x1 (bucket_mask) = the LAST table write before the find spins.
⇒ The self.table.ctrl WRITE-BACK is PRESENT, to a SANE stack addr, with a VALID heap ctrl ptr — NOT dropped, NOT
  garbage. So the bug is NOT a dropped/mis-addressed STORE. ⇒ REFINED: the bug is the FIND-side READ — it reads
  self.table from a stale/different base than where the resize wrote (multiple self.table slots 0x27e90 vs 0x27ea0,
  16B apart = the X26≠X19+64 divergence, now RUNTIME-EVIDENCED). Two remaining sub-cases for the FINAL datum
  (read-trace, next): (a) find reads self.table.ctrl from a stale slot (→ stale ctrl ptr), or (b) find reads the
  right slot but a ctrl BUFFER that wasn't the one filled. Either is the same resize/find base mis-lift.
NEXT (read-trace, the airtight final confirmation): adapt instrument_guest_writes → an instrument for the find's
  `load volatile double` ctrl-group reads (log the ADDRESS to a 2nd ring e.g. 0xE0000); the find spins on it so the
  ring tail = the find's buffer addrs. Compare to 0x6291300 (the written ctrl): != → stale-ptr; == but 0x00 bytes →
  unfilled-buffer. THEN native-disasm the resize's base computation (X19/X26 setup) → the exact mis-lifted instr →
  remill/transpiler fix → delete g180+g115/g118+reserve. ⚠ DISK: KEEP_SCRATCH dirs (azul-web-transpiler-*) hit 13GB
  this session → ENOSPC stall; `rm -rf .../T/azul-web-transpiler-*` between runs, DON'T leave KEEP_SCRATCH on.
  CONSOLIDATED: cache.rs g180 bypass (documents g201); g199 volatile memset kept; rebuilt working state. UNCOMMITTED.

## g202 RESULT (2026-06-09): ★★★ AIRTIGHT — find reads the STALE PRE-RESIZE self.table. Wrong-base CONFIRMED.
Added AZ_READ_TRACE (transpiler: instrument_guest_double_reads + parse_volatile_double_load, env-gated like
AZ_WRITE_TRACE; logs each guest `load volatile double` ADDR to ring 0xE0000/counter 0xDFFF0; harness g202 dump).
Ran AZ_WRITE_TRACE=layout_flow + AZ_READ_TRACE=layout_flow + AZ_FUEL on bare entry(). DECISIVE:
  • WRITE-trace: resize wrote self.table.ctrl = 0x6291300 (heap) at stack 0x27ea0, bucket_mask=1 at 0x27ea8.
  • READ-trace: the find spins reading ctrl from **0x41bfc08, CONSTANT (88141×, same addr, page 0x41bf000)**.
⇒ 0x41bfc08 ≠ 0x6291300, is BELOW the bump heap (0x6000000), and was NEVER written. The CONSTANT addr ⇒ the
  find's bucket_mask=0 (probe never advances) = the PRE-RESIZE empty/default table. So the find loaded
  self.table = {ctrl=0x41bfc08, mask=0} (STALE pre-resize) while the resize wrote {ctrl=0x6291300, mask=1} to a
  DIFFERENT base. ⇒ ROOT CAUSE AIRTIGHT: the resize's `self.table = new_table` lands at base-A (X19, 0x27ea0) but
  the find reads self.table from base-B (X26) that STILL HOLDS the pre-resize value → never sees the update →
  probes a stale ctrl (0x41bfc08, mask=0, not 0xFF) → spins. NOT unfilled-buffer, NOT dropped-store, NOT Vec-len.
  = a remill mis-lift making the find's &self.table (X26) ≠ the resize's &self.table (X19). reserve(1) works
  because the separate call keeps a single consistent &self.table.
NEXT (find the mis-lift): trace how &self.table is computed in the resize (→0x27ea0) vs the find (→X26-stale) — a
  register holding &self.logical_items.table that's SPILLED+RELOADED across the inlined resize, where the find
  reloads a STALE spill (or the resize updates a COPY, not the original). Cheap: relift bare entry + KEEP_SCRATCH
  (clean after!), in layout_flow opt.ll trace the find's %188=trunc(*X26) base vs the copy's *X19 base back to
  their common &self origin; find the instr where they fork. Then remill semantic OR a transpiler post-pass that
  forces the find to reload &self.table. Tools added (KEEP, env-gated): AZ_READ_TRACE. cache.rs = bare entry (g202
  TEMP, restore g180 bypass). g199 volatile memset + AZ_READ_TRACE are permanent. UNCOMMITTED.

## g203 (2026-06-09): the FORK traced — find base = *(W0) [early], resize base = phi+adds [in-resize]. Two self.tables.
Relifted bare entry + KEEP_SCRATCH (cleaned after), traced layout_flow opt.ll (__az_dep_10b6ffb90):
  • FIND self.table base: `%186 = load[X26]` (line 930); X26 set ONCE at line 122 = `%40 = load[W0]` (the entry/
    early &self-derived value, NOT reloaded after the resize). find ctrl = *(trunc(%186)+0).
  • RESIZE self.table base: `%add.i.i1282 = %119 + %129` → X19 (line 709, computed INSIDE the resize block 127;
    %119 = phi[0, %add.i.i1355], %129 = load[X23]). copy writes new table to trunc(X19)+64 (=0x27ea0 at runtime).
  ⇒ X26 (find, *(W0) early) and X19 (resize, phi+adds) are computed from DIFFERENT sources → at runtime they
    point to DIFFERENT self.table objects (find=empty/stale ctrl 0x41bfc08 mask0; resize=new ctrl 0x6291300 mask1).
    In hashbrown self.table is ONE in-place-replaced object → the lift SPLIT it: the resize updates one location,
    the find reads the stale other. = a &mut-self / self-update-target mis-lift in the INLINED resize.
NEXT (exact instr + fix): IR tracing is exhausted (the IR is internally consistent; the fork reflects native
  faithfully). Need NATIVE-DISASM correlation: rebuild bare entry, `otool -tV`/objdump layout_flow, find the
  native instrs that set X26 (=W0-derived) vs X19 (=phi+adds) for &self.table; in correct native they coincide,
  so the mis-lift is the ONE instr remill mis-models (likely a stack-spill reload of &self.table that the find
  reads stale, or a NEON/addressing decode). Fix = remill semantic/decoder OR a transpiler post-pass forcing the
  find to reload &self.table from the canonical slot. Verify: bare entry() lays out web-nested-text fuel-free.
  ★ ENTIRE CHAIN now RUNTIME-PROVEN: g197 reserve(1) fixes • g199 volatile≠fix (not reorder) • g200 fill correct
  (12B@heap, not Vec-len) • g201 write-back correct • g202 find reads STALE empty self.table (0x41bfc08 vs
  0x6291300) • g203 fork = X26(*(W0)) vs X19(phi+adds). The cron's "Vec-len" premise is fully disproven; the real
  bug is a self.table base mis-lift in the inlined hashbrown resize. CONSOLIDATED: restoring g180 bypass + rebuild.

## g204 (2026-06-09): SP-reloc / enforce_sp / M12-self-base angle RULED OUT. Bug = the resize's X19 &self.table calc.
relocate_stack_if_non_mini patches SP UNIFORMLY (one base for the whole frame → can't cause an X19/X26 split).
enforce_sp_preservation only wraps __remill_function_call (the inlined resize is NOT a call → not wrapped), BUT
X26 is preserved fine across the inlined resize (NO store to X26 between its set @line122 and the find @line930).
⇒ the FIND base X26=*(W0) is CORRECT (= the real &self.table, holding the empty table); the bug is the RESIZE
writing the new table to a WRONG &self.table = X19 = `%119 + %129` (%119=phi[0,%add.i.i1355], %129=load[X23], set
in-resize @line709) ≠ X26. So the in-place `self.table = new_table` lands at X19+64 (a wrong/local addr 0x27ea0)
instead of &self.table (X26) → the field stays the empty table → find spins. NOT M12-class (that was a self(X0)-
base store; here X26 is right, X19 is wrong). NEXT (the ONE remaining path, deliberate): native-disasm — bare
entry rebuild, `otool -tV` the layout_flow sub_<hex>, locate the resize block's `add x19,…` that computes
&self.table (the new-table store dest), compare its inputs (the reg holding &self.logical_items.table + the +64
field offset) to the lifted %119/%129/X23 — find the ONE instr remill mis-models (semantic, NOT a decode gap:
layout_flow lifts __remill_error=0). Fix = remill semantic OR transpiler post-pass. This is a focused remill
session (otool correlation + submodule change + ninja), not cheap autopilot analysis — IR tracing is exhausted.